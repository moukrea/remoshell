//! Systemd integration module for the RemoShell daemon.
//!
//! This module provides integration with systemd's service management,
//! including:
//! - Detection of systemd environment
//! - sd_notify protocol for service status notifications
//! - Unit file generation for installation
//! - SIGTERM handling for graceful shutdown
//!
//! ## Usage
//!
//! ```rust,no_run
//! use daemon::ui::systemd::{is_systemd, notify_ready, notify_status, notify_stopping};
//!
//! // Check if running under systemd
//! if is_systemd() {
//!     // Notify systemd that the service is ready
//!     notify_ready();
//!
//!     // Update status during operation
//!     notify_status("Accepting connections");
//!
//!     // Notify before shutdown
//!     notify_stopping();
//! }
//! ```

use std::env;
use std::io;
use std::os::unix::net::UnixDatagram;
use std::path::PathBuf;

use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

/// Environment variable name for the systemd notification socket.
const NOTIFY_SOCKET_ENV: &str = "NOTIFY_SOCKET";

/// Check if the daemon is running under systemd.
///
/// Returns `true` if the `NOTIFY_SOCKET` environment variable is set,
/// indicating that systemd expects notifications from this service.
///
/// # Example
///
/// ```rust,no_run
/// use daemon::ui::systemd::is_systemd;
///
/// if is_systemd() {
///     println!("Running under systemd");
/// }
/// ```
pub fn is_systemd() -> bool {
    env::var(NOTIFY_SOCKET_ENV).is_ok()
}

/// Get the path to the systemd notification socket.
///
/// Returns `None` if not running under systemd or if the socket path is invalid.
fn get_notify_socket_path() -> Option<PathBuf> {
    let socket_path = env::var(NOTIFY_SOCKET_ENV).ok()?;

    // Handle abstract socket namespace (starts with @)
    // or regular Unix socket path
    if socket_path.starts_with('@') || socket_path.starts_with('/') {
        Some(PathBuf::from(socket_path))
    } else {
        warn!("Invalid NOTIFY_SOCKET path: {}", socket_path);
        None
    }
}

/// Send a notification message to systemd.
///
/// This is the low-level function that sends datagrams to the systemd
/// notification socket. Messages should be in the format "KEY=VALUE\n".
///
/// Returns `Ok(())` if the notification was sent successfully, or if
/// not running under systemd (in which case this is a no-op).
///
/// # Errors
///
/// Returns an error if the socket connection fails or if sending the
/// message fails.
fn notify(message: &str) -> io::Result<()> {
    let socket_path = match get_notify_socket_path() {
        Some(path) => path,
        None => {
            debug!("Not running under systemd, skipping notification");
            return Ok(());
        }
    };

    let socket = UnixDatagram::unbound()?;

    // Handle abstract socket namespace (starts with @)
    let actual_path = if socket_path.to_string_lossy().starts_with('@') {
        // For abstract sockets, replace @ with null byte
        let path_str = socket_path.to_string_lossy();
        let abstract_path = format!("\0{}", &path_str[1..]);
        PathBuf::from(abstract_path)
    } else {
        socket_path
    };

    // Send the notification
    socket.send_to(message.as_bytes(), &actual_path)?;

    debug!("Sent systemd notification: {}", message.trim());
    Ok(())
}

/// Notify systemd that the service is ready to accept connections.
///
/// This sends `READY=1` to systemd, indicating that the service has
/// completed its initialization and is ready to handle requests.
///
/// This function is safe to call even when not running under systemd;
/// in that case, it does nothing.
///
/// # Example
///
/// ```rust,no_run
/// use daemon::ui::systemd::notify_ready;
///
/// // After initialization is complete
/// notify_ready();
/// ```
pub fn notify_ready() {
    if let Err(e) = notify("READY=1\n") {
        error!("Failed to notify systemd of ready state: {}", e);
    } else {
        info!("Notified systemd: service ready");
    }
}

/// Send a status update to systemd.
///
/// This sends `STATUS=<message>` to systemd, which can be displayed
/// by `systemctl status`. This is useful for providing human-readable
/// status information about the service's current state.
///
/// # Arguments
///
/// * `message` - A human-readable status message
///
/// # Example
///
/// ```rust,no_run
/// use daemon::ui::systemd::notify_status;
///
/// notify_status("Accepting connections on port 8080");
/// notify_status("Connected clients: 5");
/// ```
pub fn notify_status(message: &str) {
    let notification = format!("STATUS={}\n", message);
    if let Err(e) = notify(&notification) {
        error!("Failed to notify systemd of status: {}", e);
    } else {
        debug!("Notified systemd: STATUS={}", message);
    }
}

/// Notify systemd that the service is stopping.
///
/// This sends `STOPPING=1` to systemd, indicating that the service
/// has begun its shutdown sequence. Systemd will wait for the service
/// to exit (up to the configured timeout) before forcibly terminating it.
///
/// This function is safe to call even when not running under systemd;
/// in that case, it does nothing.
///
/// # Example
///
/// ```rust,no_run
/// use daemon::ui::systemd::notify_stopping;
///
/// // When beginning graceful shutdown
/// notify_stopping();
/// ```
pub fn notify_stopping() {
    if let Err(e) = notify("STOPPING=1\n") {
        error!("Failed to notify systemd of stopping state: {}", e);
    } else {
        info!("Notified systemd: service stopping");
    }
}

/// Notify systemd of the main process PID.
///
/// This sends `MAINPID=<pid>` to systemd. This is typically not needed
/// for services using `Type=notify` unless forking is involved.
///
/// # Arguments
///
/// * `pid` - The process ID of the main daemon process
pub fn notify_mainpid(pid: u32) {
    let notification = format!("MAINPID={}\n", pid);
    if let Err(e) = notify(&notification) {
        error!("Failed to notify systemd of main PID: {}", e);
    } else {
        debug!("Notified systemd: MAINPID={}", pid);
    }
}

/// Send a watchdog keepalive to systemd.
///
/// This sends `WATCHDOG=1` to systemd to reset the watchdog timer.
/// This should be called periodically if the service is configured
/// with `WatchdogSec` in its unit file.
///
/// # Example
///
/// ```rust,no_run
/// use daemon::ui::systemd::notify_watchdog;
///
/// // In a periodic task
/// notify_watchdog();
/// ```
pub fn notify_watchdog() {
    if let Err(e) = notify("WATCHDOG=1\n") {
        error!("Failed to send watchdog keepalive: {}", e);
    } else {
        debug!("Sent watchdog keepalive");
    }
}

/// Generate a systemd unit file for the RemoShell daemon.
///
/// This generates a unit file suitable for installing the daemon
/// as a systemd service. The generated file can be saved to
/// `~/.config/systemd/user/remoshell.service` for user services or
/// `/etc/systemd/system/remoshell.service` for system services.
///
/// # Arguments
///
/// * `exec_path` - Optional path to the executable. If `None`, uses `/usr/bin/remoshell-daemon`.
///
/// # Example
///
/// ```rust
/// use daemon::ui::systemd::generate_unit_file;
///
/// let unit_file = generate_unit_file(None);
/// println!("{}", unit_file);
/// ```
pub fn generate_unit_file(exec_path: Option<&str>) -> String {
    let exec = exec_path.unwrap_or("/usr/bin/remoshell-daemon");

    format!(
        r#"[Unit]
Description=RemoteShell P2P Terminal Daemon
Documentation=https://github.com/remoshell/remoshell
After=network.target

[Service]
Type=notify
ExecStart={} start --systemd
Restart=on-failure
RestartSec=5
TimeoutStartSec=30
TimeoutStopSec=30

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=read-only
PrivateTmp=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes

# Allow network access
RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6

# Allow reading user config
ReadWritePaths=%h/.config/remoshell

[Install]
WantedBy=default.target
"#,
        exec
    )
}

/// Generate a minimal systemd unit file without security hardening.
///
/// This is useful for development or when the hardened unit file
/// causes issues.
///
/// # Arguments
///
/// * `exec_path` - Optional path to the executable. If `None`, uses `/usr/bin/remoshell-daemon`.
pub fn generate_minimal_unit_file(exec_path: Option<&str>) -> String {
    let exec = exec_path.unwrap_or("/usr/bin/remoshell-daemon");

    format!(
        r#"[Unit]
Description=RemoteShell P2P Terminal Daemon
After=network.target

[Service]
Type=notify
ExecStart={} start --systemd
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#,
        exec
    )
}

/// A handle for managing SIGTERM signals.
///
/// This struct provides an async interface for receiving SIGTERM signals,
/// which is the standard way systemd requests a service to shut down.
pub struct SignalHandler {
    shutdown_sender: broadcast::Sender<()>,
}

impl SignalHandler {
    /// Create a new signal handler.
    ///
    /// This registers a handler for SIGTERM signals. When SIGTERM is received,
    /// the handler will notify all subscribers via the shutdown channel.
    ///
    /// # Returns
    ///
    /// Returns the signal handler and a receiver for shutdown notifications.
    ///
    /// # Errors
    ///
    /// Returns an error if signal registration fails.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use daemon::ui::systemd::SignalHandler;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let (handler, mut shutdown_rx) = SignalHandler::new().unwrap();
    ///
    ///     tokio::spawn(async move {
    ///         handler.run().await;
    ///     });
    ///
    ///     // Wait for shutdown signal
    ///     shutdown_rx.recv().await.ok();
    ///     println!("Received shutdown signal");
    /// }
    /// ```
    pub fn new() -> io::Result<(Self, broadcast::Receiver<()>)> {
        let (shutdown_sender, shutdown_receiver) = broadcast::channel(1);

        Ok((Self { shutdown_sender }, shutdown_receiver))
    }

    /// Get a new receiver for shutdown notifications.
    ///
    /// This allows multiple tasks to subscribe to the shutdown signal.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_sender.subscribe()
    }

    /// Run the signal handler.
    ///
    /// This async function listens for SIGTERM and sends a shutdown
    /// notification when received. It should be spawned as a background task.
    pub async fn run(self) {
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to register SIGTERM handler: {}", e);
                return;
            }
        };

        info!("Signal handler started, waiting for SIGTERM");

        // Wait for SIGTERM
        sigterm.recv().await;

        info!("Received SIGTERM, initiating shutdown");

        // Notify systemd that we're stopping
        notify_stopping();

        // Send shutdown notification to all subscribers
        let _ = self.shutdown_sender.send(());
    }
}

/// Context for running the daemon in systemd mode.
///
/// This struct coordinates systemd notifications with the daemon lifecycle.
pub struct SystemdContext {
    signal_handler: Option<SignalHandler>,
    shutdown_receiver: broadcast::Receiver<()>,
}

impl SystemdContext {
    /// Create a new systemd context.
    ///
    /// If running under systemd, this sets up signal handling and
    /// prepares for systemd notifications.
    ///
    /// # Errors
    ///
    /// Returns an error if signal handler setup fails.
    pub fn new() -> io::Result<Self> {
        let (handler, receiver) = SignalHandler::new()?;

        Ok(Self {
            signal_handler: Some(handler),
            shutdown_receiver: receiver,
        })
    }

    /// Notify systemd that the service is ready.
    ///
    /// Call this after the daemon has completed initialization.
    pub fn ready(&self) {
        notify_ready();
    }

    /// Update the service status.
    ///
    /// Call this to update the status shown by `systemctl status`.
    pub fn status(&self, message: &str) {
        notify_status(message);
    }

    /// Take the signal handler for spawning.
    ///
    /// This should be called once to get the handler that should be
    /// spawned as a background task.
    pub fn take_signal_handler(&mut self) -> Option<SignalHandler> {
        self.signal_handler.take()
    }

    /// Get a receiver for shutdown notifications.
    ///
    /// This can be used by tasks that need to react to shutdown signals.
    pub fn shutdown_receiver(&self) -> broadcast::Receiver<()> {
        self.shutdown_receiver.resubscribe()
    }

    /// Wait for a shutdown signal.
    ///
    /// This blocks until SIGTERM is received.
    pub async fn wait_for_shutdown(&mut self) {
        let _ = self.shutdown_receiver.recv().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    // Helper functions to safely manage environment variables in tests
    // SAFETY: These are test-only helpers that manipulate environment variables.
    // Tests that use env vars should be run with --test-threads=1 to avoid races.

    unsafe fn set_notify_socket(value: &str) {
        // SAFETY: This is only called in tests where we control the environment
        unsafe { env::set_var(NOTIFY_SOCKET_ENV, value) };
    }

    unsafe fn remove_notify_socket() {
        // SAFETY: This is only called in tests where we control the environment
        unsafe { env::remove_var(NOTIFY_SOCKET_ENV) };
    }

    #[test]
    fn test_is_systemd_without_env() {
        // Ensure NOTIFY_SOCKET is not set for this test
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        assert!(!is_systemd());
    }

    #[test]
    fn test_is_systemd_with_env() {
        // Set NOTIFY_SOCKET for this test
        // SAFETY: Test-only environment manipulation
        unsafe { set_notify_socket("/run/systemd/notify") };
        assert!(is_systemd());
        unsafe { remove_notify_socket() };
    }

    #[test]
    fn test_get_notify_socket_path_absolute() {
        // SAFETY: Test-only environment manipulation
        unsafe { set_notify_socket("/run/systemd/notify") };
        let path = get_notify_socket_path();
        assert_eq!(path, Some(PathBuf::from("/run/systemd/notify")));
        unsafe { remove_notify_socket() };
    }

    #[test]
    fn test_get_notify_socket_path_abstract() {
        // SAFETY: Test-only environment manipulation
        unsafe { set_notify_socket("@/run/systemd/notify") };
        let path = get_notify_socket_path();
        assert_eq!(path, Some(PathBuf::from("@/run/systemd/notify")));
        unsafe { remove_notify_socket() };
    }

    #[test]
    fn test_get_notify_socket_path_invalid() {
        // SAFETY: Test-only environment manipulation
        unsafe { set_notify_socket("invalid-path") };
        let path = get_notify_socket_path();
        assert!(path.is_none());
        unsafe { remove_notify_socket() };
    }

    #[test]
    fn test_get_notify_socket_path_not_set() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        let path = get_notify_socket_path();
        assert!(path.is_none());
    }

    #[test]
    fn test_notify_without_socket() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        // Should succeed (no-op) when not running under systemd
        let result = notify("READY=1\n");
        assert!(result.is_ok());
    }

    #[test]
    fn test_notify_ready_without_socket() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        // Should not panic when not running under systemd
        notify_ready();
    }

    #[test]
    fn test_notify_status_without_socket() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        // Should not panic when not running under systemd
        notify_status("Test status");
    }

    #[test]
    fn test_notify_stopping_without_socket() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        // Should not panic when not running under systemd
        notify_stopping();
    }

    #[test]
    fn test_notify_watchdog_without_socket() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        // Should not panic when not running under systemd
        notify_watchdog();
    }

    #[test]
    fn test_notify_mainpid_without_socket() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        // Should not panic when not running under systemd
        notify_mainpid(12345);
    }

    #[test]
    fn test_generate_unit_file_default() {
        let unit_file = generate_unit_file(None);

        assert!(unit_file.contains("[Unit]"));
        assert!(unit_file.contains("[Service]"));
        assert!(unit_file.contains("[Install]"));
        assert!(unit_file.contains("Type=notify"));
        assert!(unit_file.contains("ExecStart=/usr/bin/remoshell-daemon start --systemd"));
        assert!(unit_file.contains("Restart=on-failure"));
        assert!(unit_file.contains("RestartSec=5"));
        assert!(unit_file.contains("WantedBy=default.target"));
        assert!(unit_file.contains("After=network.target"));
        assert!(unit_file.contains("Description=RemoteShell P2P Terminal Daemon"));
    }

    #[test]
    fn test_generate_unit_file_custom_path() {
        let unit_file = generate_unit_file(Some("/custom/path/remoshell"));

        assert!(unit_file.contains("ExecStart=/custom/path/remoshell start --systemd"));
    }

    #[test]
    fn test_generate_unit_file_security_hardening() {
        let unit_file = generate_unit_file(None);

        assert!(unit_file.contains("NoNewPrivileges=yes"));
        assert!(unit_file.contains("ProtectSystem=strict"));
        assert!(unit_file.contains("ProtectHome=read-only"));
        assert!(unit_file.contains("PrivateTmp=yes"));
    }

    #[test]
    fn test_generate_minimal_unit_file() {
        let unit_file = generate_minimal_unit_file(None);

        assert!(unit_file.contains("[Unit]"));
        assert!(unit_file.contains("[Service]"));
        assert!(unit_file.contains("[Install]"));
        assert!(unit_file.contains("Type=notify"));
        assert!(unit_file.contains("ExecStart=/usr/bin/remoshell-daemon start --systemd"));
        // Minimal should NOT contain security hardening
        assert!(!unit_file.contains("NoNewPrivileges"));
        assert!(!unit_file.contains("ProtectSystem"));
    }

    #[test]
    fn test_generate_minimal_unit_file_custom_path() {
        let unit_file = generate_minimal_unit_file(Some("/opt/bin/remoshell"));

        assert!(unit_file.contains("ExecStart=/opt/bin/remoshell start --systemd"));
    }

    #[test]
    fn test_unit_file_can_be_written() {
        let dir = tempdir().unwrap();
        let unit_path = dir.path().join("remoshell.service");

        let unit_file = generate_unit_file(None);
        fs::write(&unit_path, &unit_file).unwrap();

        let read_back = fs::read_to_string(&unit_path).unwrap();
        assert_eq!(read_back, unit_file);
    }

    #[tokio::test]
    async fn test_signal_handler_creation() {
        let result = SignalHandler::new();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_signal_handler_subscribe() {
        let (handler, _rx1) = SignalHandler::new().unwrap();
        let _rx2 = handler.subscribe();
        // Both receivers should work
    }

    #[tokio::test]
    async fn test_systemd_context_creation() {
        let result = SystemdContext::new();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_systemd_context_ready() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        let context = SystemdContext::new().unwrap();
        // Should not panic
        context.ready();
    }

    #[tokio::test]
    async fn test_systemd_context_status() {
        // SAFETY: Test-only environment manipulation
        unsafe { remove_notify_socket() };
        let context = SystemdContext::new().unwrap();
        // Should not panic
        context.status("Test status");
    }

    #[tokio::test]
    async fn test_systemd_context_take_handler() {
        let mut context = SystemdContext::new().unwrap();

        // First take should succeed
        let handler = context.take_signal_handler();
        assert!(handler.is_some());

        // Second take should return None
        let handler2 = context.take_signal_handler();
        assert!(handler2.is_none());
    }

    #[tokio::test]
    async fn test_systemd_context_shutdown_receiver() {
        let context = SystemdContext::new().unwrap();
        let _rx = context.shutdown_receiver();
        // Should be able to create multiple receivers
        let _rx2 = context.shutdown_receiver();
    }

    #[test]
    fn test_notify_socket_env_constant() {
        assert_eq!(NOTIFY_SOCKET_ENV, "NOTIFY_SOCKET");
    }
}
