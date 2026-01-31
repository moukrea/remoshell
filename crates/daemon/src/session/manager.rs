//! Session manager for managing multiple PTY sessions.
//!
//! This module provides a thread-safe session manager that can create,
//! retrieve, and manage multiple PTY sessions concurrently.

use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::broadcast;

use super::pty::{Session, SessionError, SessionId, SessionStatus};

/// Trait for session management operations.
///
/// This trait defines the interface for managing PTY sessions.
/// Implementations must be thread-safe and suitable for concurrent access.
#[allow(async_fn_in_trait)]
pub trait SessionManager: Send + Sync {
    /// Creates a new session with the given parameters.
    ///
    /// # Arguments
    /// * `shell` - Optional shell command. If None, uses $SHELL or /bin/sh.
    /// * `cols` - Terminal width in columns.
    /// * `rows` - Terminal height in rows.
    /// * `env` - Additional environment variables to set.
    /// * `cwd` - Working directory for the session.
    ///
    /// # Returns
    /// The session ID and process ID on success.
    async fn create(
        &self,
        shell: Option<String>,
        cols: u16,
        rows: u16,
        env: Vec<(String, String)>,
        cwd: Option<String>,
    ) -> Result<(SessionId, u32), SessionError>;

    /// Attaches to an existing session.
    ///
    /// Returns a receiver for the session's output broadcast.
    async fn attach(
        &self,
        session_id: &SessionId,
    ) -> Result<broadcast::Receiver<Vec<u8>>, SessionError>;

    /// Detaches from a session.
    ///
    /// The session continues running after detachment.
    async fn detach(&self, session_id: &SessionId) -> Result<(), SessionError>;

    /// Writes data to a session's input.
    async fn write(&self, session_id: &SessionId, data: &[u8]) -> Result<(), SessionError>;

    /// Resizes a session's terminal.
    async fn resize(
        &self,
        session_id: &SessionId,
        cols: u16,
        rows: u16,
    ) -> Result<(), SessionError>;

    /// Kills a session.
    ///
    /// # Arguments
    /// * `session_id` - The session to kill.
    /// * `signal` - Optional signal to send. If None, just terminates.
    async fn kill(
        &self,
        session_id: &SessionId,
        signal: Option<i32>,
    ) -> Result<SessionStatus, SessionError>;

    /// Lists all active sessions.
    fn list(&self) -> Vec<SessionInfo>;

    /// Gets information about a specific session.
    async fn get(&self, session_id: &SessionId) -> Option<SessionInfo>;

    /// Checks if a session exists and is running.
    fn exists(&self, session_id: &SessionId) -> bool;

    /// Returns the number of active sessions.
    fn count(&self) -> usize;
}

/// Information about a session.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// Unique session identifier.
    pub id: SessionId,
    /// Process ID of the shell.
    pub pid: Option<u32>,
    /// Current terminal columns.
    pub cols: u16,
    /// Current terminal rows.
    pub rows: u16,
    /// Whether the session is still running.
    pub running: bool,
    /// Number of attached clients.
    pub subscribers: usize,
}

/// Thread-safe session manager implementation using DashMap.
///
/// This implementation provides concurrent access to sessions without
/// requiring external locking at the call site.
pub struct SessionManagerImpl {
    /// Map of session ID to session.
    sessions: DashMap<SessionId, Arc<tokio::sync::Mutex<Session>>>,
}

impl SessionManagerImpl {
    /// Creates a new session manager.
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    /// Cleans up terminated sessions.
    ///
    /// This removes sessions that are no longer running from the manager.
    /// Should be called periodically to free resources.
    pub async fn cleanup(&self) {
        let mut to_remove = Vec::new();

        // Collect IDs of terminated sessions
        for entry in self.sessions.iter() {
            let session = entry.value().lock().await;
            if !session.is_running() {
                to_remove.push(entry.key().clone());
            }
        }

        // Remove terminated sessions
        for id in to_remove {
            if let Some((id, _)) = self.sessions.remove(&id) {
                tracing::info!(session_id = %id, "Cleaned up terminated session");
            }
        }
    }

    /// Starts a background task that periodically cleans up terminated sessions.
    ///
    /// # Arguments
    /// * `interval_secs` - How often to run cleanup, in seconds.
    pub fn start_cleanup_task(self: &Arc<Self>, interval_secs: u64) {
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let interval = std::time::Duration::from_secs(interval_secs);
            loop {
                tokio::time::sleep(interval).await;
                manager.cleanup().await;
            }
        });
    }
}

impl Default for SessionManagerImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager for SessionManagerImpl {
    async fn create(
        &self,
        shell: Option<String>,
        cols: u16,
        rows: u16,
        env: Vec<(String, String)>,
        cwd: Option<String>,
    ) -> Result<(SessionId, u32), SessionError> {
        // Spawn the session
        let (session, _rx) = Session::spawn(shell, cols, rows, env, cwd)?;

        let session_id = session.id().clone();
        let pid = session.pid().unwrap_or(0);

        // Start the read loop
        session.start_read_loop();

        // Store the session
        self.sessions.insert(
            session_id.clone(),
            Arc::new(tokio::sync::Mutex::new(session)),
        );

        tracing::info!(
            session_id = %session_id,
            pid = pid,
            cols = cols,
            rows = rows,
            "Created new session"
        );

        Ok((session_id, pid))
    }

    async fn attach(
        &self,
        session_id: &SessionId,
    ) -> Result<broadcast::Receiver<Vec<u8>>, SessionError> {
        let session_arc = self
            .sessions
            .get(session_id)
            .map(|entry| Arc::clone(entry.value()))
            .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;

        let session = session_arc.lock().await;

        if !session.is_running() {
            return Err(SessionError::AlreadyTerminated(session_id.clone()));
        }

        let rx = session.subscribe();

        tracing::debug!(
            session_id = %session_id,
            subscribers = session.subscriber_count(),
            "Client attached to session"
        );

        Ok(rx)
    }

    async fn detach(&self, session_id: &SessionId) -> Result<(), SessionError> {
        // Just verify the session exists
        if !self.sessions.contains_key(session_id) {
            return Err(SessionError::NotFound(session_id.clone()));
        }

        // Detachment is handled by dropping the receiver on the client side
        tracing::debug!(session_id = %session_id, "Client detached from session");

        Ok(())
    }

    async fn write(&self, session_id: &SessionId, data: &[u8]) -> Result<(), SessionError> {
        let session_arc = self
            .sessions
            .get(session_id)
            .map(|entry| Arc::clone(entry.value()))
            .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;

        let session = session_arc.lock().await;
        session.write(data).await
    }

    async fn resize(
        &self,
        session_id: &SessionId,
        cols: u16,
        rows: u16,
    ) -> Result<(), SessionError> {
        let session_arc = self
            .sessions
            .get(session_id)
            .map(|entry| Arc::clone(entry.value()))
            .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;

        let mut session = session_arc.lock().await;
        session.resize(cols, rows).await
    }

    async fn kill(
        &self,
        session_id: &SessionId,
        signal: Option<i32>,
    ) -> Result<SessionStatus, SessionError> {
        let session_arc = self
            .sessions
            .get(session_id)
            .map(|entry| Arc::clone(entry.value()))
            .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;

        let session = session_arc.lock().await;
        let status = session.kill(signal).await?;

        // Remove from map after killing
        drop(session);
        self.sessions.remove(session_id);

        tracing::info!(
            session_id = %session_id,
            status = ?status,
            "Session killed and removed"
        );

        Ok(status)
    }

    fn list(&self) -> Vec<SessionInfo> {
        // Note: We can't easily get async data here, so we use a sync approach
        // This provides a snapshot of session metadata
        self.sessions
            .iter()
            .map(|entry| {
                let id = entry.key().clone();
                // We can't await here, so we provide minimal info
                // For full info, use get() which is async
                SessionInfo {
                    id,
                    pid: None,
                    cols: 0,
                    rows: 0,
                    running: true, // Assume running if in map
                    subscribers: 0,
                }
            })
            .collect()
    }

    async fn get(&self, session_id: &SessionId) -> Option<SessionInfo> {
        let session_arc = self
            .sessions
            .get(session_id)
            .map(|entry| Arc::clone(entry.value()))?;

        let session = session_arc.lock().await;
        let (cols, rows) = session.size();

        Some(SessionInfo {
            id: session_id.clone(),
            pid: session.pid(),
            cols,
            rows,
            running: session.is_running(),
            subscribers: session.subscriber_count(),
        })
    }

    fn exists(&self, session_id: &SessionId) -> bool {
        self.sessions.contains_key(session_id)
    }

    fn count(&self) -> usize {
        self.sessions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_manager_create_session() {
        let manager = SessionManagerImpl::new();

        let result = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await;

        assert!(
            result.is_ok(),
            "Failed to create session: {:?}",
            result.err()
        );

        let (session_id, pid) = result.unwrap();
        assert!(!session_id.is_empty());
        assert!(manager.exists(&session_id));
        assert_eq!(manager.count(), 1);

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_get_session() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        let info = manager.get(&session_id).await;
        assert!(info.is_some());

        let info = info.unwrap();
        assert_eq!(info.id, session_id);
        assert_eq!(info.cols, 80);
        assert_eq!(info.rows, 24);
        assert!(info.running);

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_get_nonexistent_session() {
        let manager = SessionManagerImpl::new();

        let info = manager.get(&"nonexistent".to_string()).await;
        assert!(info.is_none());
    }

    #[tokio::test]
    async fn test_manager_attach() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        let result = manager.attach(&session_id).await;
        assert!(result.is_ok(), "Failed to attach: {:?}", result.err());

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_attach_nonexistent() {
        let manager = SessionManagerImpl::new();

        let result = manager.attach(&"nonexistent".to_string()).await;
        assert!(matches!(result, Err(SessionError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_manager_detach() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        // Attach first
        let _rx = manager.attach(&session_id).await.unwrap();

        // Detach
        let result = manager.detach(&session_id).await;
        assert!(result.is_ok());

        // Session should still exist
        assert!(manager.exists(&session_id));

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_write() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        let result = manager.write(&session_id, b"echo hello\n").await;
        assert!(result.is_ok(), "Failed to write: {:?}", result.err());

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_write_nonexistent() {
        let manager = SessionManagerImpl::new();

        let result = manager.write(&"nonexistent".to_string(), b"hello").await;
        assert!(matches!(result, Err(SessionError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_manager_resize() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        let result = manager.resize(&session_id, 120, 40).await;
        assert!(result.is_ok(), "Failed to resize: {:?}", result.err());

        let info = manager.get(&session_id).await.unwrap();
        assert_eq!(info.cols, 120);
        assert_eq!(info.rows, 40);

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_kill() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        assert!(manager.exists(&session_id));

        let result = manager.kill(&session_id, Some(9)).await;
        assert!(result.is_ok(), "Failed to kill: {:?}", result.err());

        // Session should be removed
        assert!(!manager.exists(&session_id));
        assert_eq!(manager.count(), 0);
    }

    #[tokio::test]
    async fn test_manager_kill_nonexistent() {
        let manager = SessionManagerImpl::new();

        let result = manager.kill(&"nonexistent".to_string(), Some(9)).await;
        assert!(matches!(result, Err(SessionError::NotFound(_))));
    }

    #[tokio::test]
    async fn test_manager_list() {
        let manager = SessionManagerImpl::new();

        // Create multiple sessions
        let (id1, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();
        let (id2, _) = manager
            .create(Some("/bin/sh".to_string()), 100, 30, vec![], None)
            .await
            .unwrap();

        let sessions = manager.list();
        assert_eq!(sessions.len(), 2);

        let ids: Vec<_> = sessions.iter().map(|s| s.id.clone()).collect();
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));

        // Clean up
        let _ = manager.kill(&id1, Some(9)).await;
        let _ = manager.kill(&id2, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_output_roundtrip() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        // Attach to receive output
        let mut rx = manager.attach(&session_id).await.unwrap();

        // Write a command
        manager
            .write(&session_id, b"echo roundtrip_test_marker\n")
            .await
            .unwrap();

        // Wait for output
        let mut found_output = false;
        for _ in 0..50 {
            match timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Ok(data)) => {
                    let output = String::from_utf8_lossy(&data);
                    if output.contains("roundtrip_test_marker") {
                        found_output = true;
                        break;
                    }
                }
                _ => {}
            }
        }

        assert!(found_output, "Did not receive expected output");

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_multiple_attach() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        // Attach multiple clients
        let _rx1 = manager.attach(&session_id).await.unwrap();
        let _rx2 = manager.attach(&session_id).await.unwrap();
        let _rx3 = manager.attach(&session_id).await.unwrap();

        let info = manager.get(&session_id).await.unwrap();
        assert_eq!(info.subscribers, 3);

        // Clean up
        let _ = manager.kill(&session_id, Some(9)).await;
    }

    #[tokio::test]
    async fn test_manager_cleanup() {
        let manager = SessionManagerImpl::new();

        let (session_id, _) = manager
            .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
            .await
            .unwrap();

        // Kill the session but don't remove it
        // (We need to simulate a session dying without explicit kill)
        // For this test, we'll just verify cleanup doesn't crash
        manager.cleanup().await;

        // Session should still exist (it's still running)
        assert!(manager.exists(&session_id));

        // Kill it properly
        let _ = manager.kill(&session_id, Some(9)).await;

        // Cleanup should now remove nothing (already removed)
        manager.cleanup().await;
        assert_eq!(manager.count(), 0);
    }
}
