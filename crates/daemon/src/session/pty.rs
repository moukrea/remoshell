//! PTY session management.
//!
//! This module provides the core PTY spawning and I/O functionality.
//! A session represents a single terminal session with a shell process.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use uuid::Uuid;

/// Unique identifier for a session.
pub type SessionId = String;

/// Errors that can occur during session operations.
#[derive(Error, Debug)]
pub enum SessionError {
    /// The session was not found.
    #[error("session not found: {0}")]
    NotFound(SessionId),

    /// The session has already been terminated.
    #[error("session already terminated: {0}")]
    AlreadyTerminated(SessionId),

    /// Failed to spawn the PTY.
    #[error("failed to spawn PTY: {0}")]
    SpawnFailed(String),

    /// Failed to write to the PTY.
    #[error("failed to write to PTY: {0}")]
    WriteFailed(String),

    /// Failed to read from the PTY.
    #[error("failed to read from PTY: {0}")]
    ReadFailed(String),

    /// Failed to resize the PTY.
    #[error("failed to resize PTY: {0}")]
    ResizeFailed(String),

    /// Failed to kill the session.
    #[error("failed to kill session: {0}")]
    KillFailed(String),

    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    /// Session is running.
    Running,
    /// Session has exited with a code.
    Exited(i32),
    /// Session was killed by a signal.
    Killed(i32),
    /// Session terminated for unknown reason.
    Terminated,
}

/// Buffer size for reading from PTY.
const READ_BUFFER_SIZE: usize = 4096;

/// Channel capacity for broadcast output.
const BROADCAST_CAPACITY: usize = 256;

/// A PTY session with a shell process.
///
/// The session manages a pseudo-terminal with a shell process. It provides
/// methods to write input, read output (via broadcast channel), resize the
/// terminal, and kill the process.
pub struct Session {
    /// Unique session identifier.
    id: SessionId,

    /// The PTY master handle.
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,

    /// The writer for the PTY.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,

    /// The child process.
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,

    /// Broadcast sender for output data.
    output_tx: broadcast::Sender<Vec<u8>>,

    /// Flag indicating if the session is still running.
    running: Arc<AtomicBool>,

    /// Current terminal size.
    cols: u16,
    rows: u16,

    /// Process ID.
    pid: Option<u32>,
}

impl Session {
    /// Spawns a new PTY session with the given shell and terminal size.
    ///
    /// # Arguments
    /// * `shell` - Optional shell command. If None, uses $SHELL or /bin/sh.
    /// * `cols` - Terminal width in columns.
    /// * `rows` - Terminal height in rows.
    /// * `env` - Additional environment variables to set.
    /// * `cwd` - Working directory for the session.
    ///
    /// # Returns
    /// A new session and a receiver for output data.
    pub fn spawn(
        shell: Option<String>,
        cols: u16,
        rows: u16,
        env: Vec<(String, String)>,
        cwd: Option<String>,
    ) -> Result<(Self, broadcast::Receiver<Vec<u8>>), SessionError> {
        let id = Uuid::new_v4().to_string();

        // Detect shell
        let shell_cmd = detect_shell(shell);

        // Create PTY system
        let pty_system = native_pty_system();

        // Create PTY pair with specified size
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SessionError::SpawnFailed(e.to_string()))?;

        // Build command
        let mut cmd = CommandBuilder::new(&shell_cmd);

        // Set working directory
        if let Some(ref dir) = cwd {
            cmd.cwd(dir);
        }

        // Set environment variables
        for (key, value) in env {
            cmd.env(key, value);
        }

        // Spawn the child process
        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| SessionError::SpawnFailed(e.to_string()))?;

        let pid = child.process_id();

        // Get the writer for input
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| SessionError::SpawnFailed(e.to_string()))?;

        // Create broadcast channel for output
        let (output_tx, output_rx) = broadcast::channel(BROADCAST_CAPACITY);

        let session = Session {
            id,
            master: Arc::new(Mutex::new(pair.master)),
            writer: Arc::new(Mutex::new(writer)),
            child: Arc::new(Mutex::new(child)),
            output_tx,
            running: Arc::new(AtomicBool::new(true)),
            cols,
            rows,
            pid,
        };

        Ok((session, output_rx))
    }

    /// Returns the session ID.
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    /// Returns the process ID of the shell, if available.
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Returns the current terminal size.
    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Returns whether the session is still running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Writes data to the PTY (stdin).
    ///
    /// This sends input to the shell process.
    pub async fn write(&self, data: &[u8]) -> Result<(), SessionError> {
        if !self.is_running() {
            return Err(SessionError::AlreadyTerminated(self.id.clone()));
        }

        let mut writer = self.writer.lock().await;
        writer
            .write_all(data)
            .map_err(|e| SessionError::WriteFailed(e.to_string()))?;
        writer
            .flush()
            .map_err(|e| SessionError::WriteFailed(e.to_string()))?;

        Ok(())
    }

    /// Starts the read loop for capturing output.
    ///
    /// This spawns a blocking task that reads from the PTY and broadcasts
    /// the output to all subscribers. The loop continues until the session
    /// is terminated or an error occurs.
    pub fn start_read_loop(&self) {
        let master = Arc::clone(&self.master);
        let output_tx = self.output_tx.clone();
        let running = Arc::clone(&self.running);
        let session_id = self.id.clone();

        tokio::spawn(async move {
            // Get the reader from the master
            let reader = {
                let master = master.lock().await;
                match master.try_clone_reader() {
                    Ok(reader) => reader,
                    Err(e) => {
                        tracing::error!(
                            session_id = %session_id,
                            error = %e,
                            "Failed to get PTY reader"
                        );
                        running.store(false, Ordering::SeqCst);
                        return;
                    }
                }
            };

            // Wrap reader in Arc<Mutex> for the blocking task
            let reader = Arc::new(std::sync::Mutex::new(reader));

            loop {
                if !running.load(Ordering::SeqCst) {
                    tracing::debug!(session_id = %session_id, "Read loop stopping: session not running");
                    break;
                }

                let reader_clone = Arc::clone(&reader);

                // Use spawn_blocking to read from the PTY
                let result = tokio::task::spawn_blocking(move || {
                    let mut buffer = vec![0u8; READ_BUFFER_SIZE];
                    let mut reader = reader_clone.lock().unwrap();
                    match reader.read(&mut buffer) {
                        Ok(0) => Ok(None), // EOF
                        Ok(n) => {
                            buffer.truncate(n);
                            Ok(Some(buffer))
                        }
                        Err(e) => Err(e),
                    }
                })
                .await;

                match result {
                    Ok(Ok(Some(data))) => {
                        // Broadcast the output
                        if output_tx.send(data).is_err() {
                            // No receivers, but that's okay - session might be detached
                            tracing::trace!(
                                session_id = %session_id,
                                "No receivers for output"
                            );
                        }
                    }
                    Ok(Ok(None)) => {
                        // EOF - process exited
                        tracing::info!(session_id = %session_id, "PTY EOF - process exited");
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                    Ok(Err(e)) => {
                        // I/O error
                        if running.load(Ordering::SeqCst) {
                            tracing::error!(
                                session_id = %session_id,
                                error = %e,
                                "Error reading from PTY"
                            );
                        }
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                    Err(e) => {
                        // Task join error
                        tracing::error!(
                            session_id = %session_id,
                            error = %e,
                            "Read task panicked"
                        );
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            }
        });
    }

    /// Resizes the PTY to the given dimensions.
    pub async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), SessionError> {
        if !self.is_running() {
            return Err(SessionError::AlreadyTerminated(self.id.clone()));
        }

        let master = self.master.lock().await;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| SessionError::ResizeFailed(e.to_string()))?;

        self.cols = cols;
        self.rows = rows;

        tracing::debug!(
            session_id = %self.id,
            cols = cols,
            rows = rows,
            "Resized PTY"
        );

        Ok(())
    }

    /// Kills the session and cleans up resources.
    ///
    /// If a signal is provided, sends that signal to the process.
    /// Otherwise, waits for the process to exit naturally.
    pub async fn kill(&self, signal: Option<i32>) -> Result<SessionStatus, SessionError> {
        if !self.is_running() {
            return Err(SessionError::AlreadyTerminated(self.id.clone()));
        }

        // Mark as not running first
        self.running.store(false, Ordering::SeqCst);

        let mut child = self.child.lock().await;

        // Kill the child process
        if signal.is_some() {
            // Note: portable-pty's kill() doesn't take a signal parameter
            // It just terminates the process
            child
                .kill()
                .map_err(|e| SessionError::KillFailed(e.to_string()))?;
        }

        // Wait for the process to exit
        let status = child
            .wait()
            .map_err(|e| SessionError::KillFailed(e.to_string()))?;

        let code = status.exit_code();
        let session_status = SessionStatus::Exited(code as i32);

        tracing::info!(
            session_id = %self.id,
            status = ?session_status,
            "Session killed"
        );

        Ok(session_status)
    }

    /// Checks if the child process has exited and returns its status.
    ///
    /// This does not wait for the process to exit; it only checks the current status.
    pub async fn try_wait(&self) -> Result<Option<SessionStatus>, SessionError> {
        let mut child = self.child.lock().await;

        match child.try_wait() {
            Ok(Some(status)) => {
                self.running.store(false, Ordering::SeqCst);
                let code = status.exit_code();
                let session_status = SessionStatus::Exited(code as i32);
                Ok(Some(session_status))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(SessionError::Io(e)),
        }
    }

    /// Subscribes to the output broadcast channel.
    ///
    /// Returns a receiver that will receive all output data from the PTY.
    /// Multiple clients can subscribe to the same session.
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.output_tx.receiver_count()
    }
}

/// Detects the shell to use.
///
/// Returns the shell in this order of preference:
/// 1. The provided shell if Some
/// 2. The $SHELL environment variable
/// 3. /bin/sh as fallback
fn detect_shell(shell: Option<String>) -> String {
    if let Some(s) = shell {
        return s;
    }

    std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[test]
    fn test_detect_shell_with_provided() {
        let shell = detect_shell(Some("/bin/bash".to_string()));
        assert_eq!(shell, "/bin/bash");
    }

    #[test]
    fn test_detect_shell_from_env() {
        // This test depends on the environment
        let shell = detect_shell(None);
        // Should either be from $SHELL or /bin/sh
        assert!(!shell.is_empty());
    }

    #[test]
    fn test_session_id_generation() {
        // Just verify UUID format
        let id = Uuid::new_v4().to_string();
        assert_eq!(id.len(), 36); // UUID v4 string length
    }

    #[tokio::test]
    async fn test_session_spawn() {
        let result = Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None);

        // The session should spawn successfully
        assert!(result.is_ok(), "Failed to spawn session: {:?}", result.err());

        let (session, _rx) = result.unwrap();
        assert!(session.is_running());
        assert_eq!(session.size(), (80, 24));

        // Clean up
        let _ = session.kill(Some(9)).await;
    }

    #[tokio::test]
    async fn test_session_write() {
        let (session, _rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        // Start read loop to prevent blocking
        session.start_read_loop();

        // Write some data
        let result = session.write(b"echo hello\n").await;
        assert!(result.is_ok(), "Failed to write: {:?}", result.err());

        // Give it a moment to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Clean up
        let _ = session.kill(Some(9)).await;
    }

    #[tokio::test]
    async fn test_session_resize() {
        let (mut session, _rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        assert_eq!(session.size(), (80, 24));

        let result = session.resize(120, 40).await;
        assert!(result.is_ok(), "Failed to resize: {:?}", result.err());
        assert_eq!(session.size(), (120, 40));

        // Clean up
        let _ = session.kill(Some(9)).await;
    }

    #[tokio::test]
    async fn test_session_kill() {
        let (session, _rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        assert!(session.is_running());

        let result = session.kill(Some(9)).await;
        assert!(result.is_ok(), "Failed to kill: {:?}", result.err());
        assert!(!session.is_running());
    }

    #[tokio::test]
    async fn test_session_output_broadcast() {
        let (session, mut rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        // Start the read loop
        session.start_read_loop();

        // Write a command
        session.write(b"echo test_output_marker\n").await.unwrap();

        // Try to receive output with timeout
        let mut found_output = false;
        for _ in 0..50 {
            match timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Ok(data)) => {
                    let output = String::from_utf8_lossy(&data);
                    if output.contains("test_output_marker") {
                        found_output = true;
                        break;
                    }
                }
                Ok(Err(e)) => {
                    // Lagged behind, try again
                    tracing::warn!("Receiver lagged: {}", e);
                }
                Err(_) => {
                    // Timeout, continue
                }
            }
        }

        assert!(found_output, "Did not receive expected output");

        // Clean up
        let _ = session.kill(Some(9)).await;
    }

    #[tokio::test]
    async fn test_session_multiple_subscribers() {
        let (session, _rx1) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        // Subscribe additional receivers
        let _rx2 = session.subscribe();
        let _rx3 = session.subscribe();

        // Should have 3 subscribers
        assert_eq!(session.subscriber_count(), 3);

        // Clean up
        let _ = session.kill(Some(9)).await;
    }

    #[tokio::test]
    async fn test_session_env_vars() {
        let env = vec![("TEST_VAR".to_string(), "test_value".to_string())];
        let (session, mut rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, env, None).unwrap();

        // Start the read loop
        session.start_read_loop();

        // Echo the environment variable
        session.write(b"echo $TEST_VAR\n").await.unwrap();

        // Try to receive output with timeout
        let mut found_value = false;
        for _ in 0..50 {
            match timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Ok(data)) => {
                    let output = String::from_utf8_lossy(&data);
                    if output.contains("test_value") {
                        found_value = true;
                        break;
                    }
                }
                _ => {}
            }
        }

        assert!(found_value, "Did not receive expected environment variable value");

        // Clean up
        let _ = session.kill(Some(9)).await;
    }

    #[tokio::test]
    async fn test_session_write_after_kill() {
        let (session, _rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        // Kill the session
        let _ = session.kill(Some(9)).await;

        // Try to write - should fail
        let result = session.write(b"hello\n").await;
        assert!(matches!(result, Err(SessionError::AlreadyTerminated(_))));
    }

    #[tokio::test]
    async fn test_session_resize_after_kill() {
        let (mut session, _rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        // Kill the session
        let _ = session.kill(Some(9)).await;

        // Try to resize - should fail
        let result = session.resize(100, 50).await;
        assert!(matches!(result, Err(SessionError::AlreadyTerminated(_))));
    }

    #[tokio::test]
    async fn test_session_try_wait() {
        let (session, _rx) =
            Session::spawn(Some("/bin/sh".to_string()), 80, 24, vec![], None).unwrap();

        // Session should still be running
        let result = session.try_wait().await.unwrap();
        assert!(result.is_none());

        // Make the shell exit
        session.start_read_loop();
        session.write(b"exit 42\n").await.unwrap();

        // Wait a bit for the process to exit
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Now it should have exited
        let result = session.try_wait().await.unwrap();
        assert!(result.is_some());
        if let Some(SessionStatus::Exited(code)) = result {
            assert_eq!(code, 42);
        }
    }
}
