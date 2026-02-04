//! Unix Domain Socket IPC module for CLI-daemon communication.
//!
//! This module provides a secure, efficient local communication channel between
//! the CLI and the daemon using Unix Domain Sockets.
//!
//! ## Overview
//!
//! The IPC system uses a JSON newline-delimited protocol for simplicity and
//! debugging convenience. Each message is a single JSON object followed by a newline.
//!
//! ## Socket Path
//!
//! The socket path follows the XDG Base Directory Specification:
//! - Primary: `$XDG_RUNTIME_DIR/remoshell/daemon.sock`
//! - Fallback: `/tmp/remoshell-$UID/daemon.sock`
//!
//! ## Example
//!
//! ### Server (Daemon) Side
//!
//! ```rust,no_run
//! use daemon::ipc::{IpcServer, IpcResponse, get_socket_path};
//!
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     let socket_path = get_socket_path();
//!     let server = IpcServer::bind(&socket_path).await?;
//!
//!     loop {
//!         let mut conn = server.accept().await?;
//!         tokio::spawn(async move {
//!             while let Ok(Some(request)) = conn.read_request().await {
//!                 // Handle request...
//!                 conn.send_response(&IpcResponse::Pong).await.ok();
//!             }
//!         });
//!     }
//! }
//! ```
//!
//! ### Client (CLI) Side
//!
//! ```rust,no_run
//! use daemon::ipc::{IpcClient, get_socket_path};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let socket_path = get_socket_path();
//!     let mut client = IpcClient::connect(&socket_path).await?;
//!
//!     if client.ping().await? {
//!         println!("Daemon is running");
//!     }
//!
//!     Ok(())
//! }
//! ```

mod client;
mod messages;
pub mod pidfile;
mod server;

pub use client::IpcClient;
pub use messages::{IpcRequest, IpcResponse, IpcSessionInfo};
pub use pidfile::{get_daemon_pid, get_pid_file_path, is_daemon_running};
pub use server::{IpcConnection, IpcError, IpcServer};

use std::path::PathBuf;

/// Get the socket path for IPC communication.
///
/// This function returns the path where the daemon's Unix socket should be located,
/// following the XDG Base Directory Specification.
///
/// ## Path Resolution
///
/// 1. If `$XDG_RUNTIME_DIR` is set: `$XDG_RUNTIME_DIR/remoshell/daemon.sock`
/// 2. Otherwise: `/tmp/remoshell-$UID/daemon.sock`
///
/// The XDG_RUNTIME_DIR is preferred because:
/// - It's typically on a tmpfs (fast, volatile)
/// - It has proper permissions (0700)
/// - It's automatically cleaned up on logout
///
/// ## Example
///
/// ```rust
/// use daemon::ipc::get_socket_path;
///
/// let path = get_socket_path();
/// println!("Socket will be at: {:?}", path);
/// ```
#[cfg(unix)]
pub fn get_socket_path() -> PathBuf {
    use std::os::unix::fs::MetadataExt;

    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir)
            .join("remoshell")
            .join("daemon.sock")
    } else {
        // Get UID by checking metadata of a file we own
        let uid = std::fs::metadata("/proc/self")
            .map(|m| m.uid())
            .unwrap_or(0);

        PathBuf::from(format!("/tmp/remoshell-{}", uid)).join("daemon.sock")
    }
}

/// Non-Unix platforms are not supported for Unix Domain Sockets.
#[cfg(not(unix))]
pub fn get_socket_path() -> PathBuf {
    // This will fail at runtime on non-Unix platforms
    PathBuf::from("/tmp/remoshell-unsupported/daemon.sock")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_socket_path_with_xdg_runtime_dir() {
        // Save current value
        let original = std::env::var("XDG_RUNTIME_DIR").ok();

        // Test with XDG_RUNTIME_DIR set
        // SAFETY: This is a test, running in isolation
        unsafe {
            std::env::set_var("XDG_RUNTIME_DIR", "/run/user/1000");
        }
        let path = get_socket_path();
        assert_eq!(path, PathBuf::from("/run/user/1000/remoshell/daemon.sock"));

        // Restore original value
        // SAFETY: This is a test, running in isolation
        unsafe {
            if let Some(val) = original {
                std::env::set_var("XDG_RUNTIME_DIR", val);
            } else {
                std::env::remove_var("XDG_RUNTIME_DIR");
            }
        }
    }

    #[test]
    fn test_get_socket_path_without_xdg_runtime_dir() {
        // Save current value
        let original = std::env::var("XDG_RUNTIME_DIR").ok();

        // Test without XDG_RUNTIME_DIR
        // SAFETY: This is a test, running in isolation
        unsafe {
            std::env::remove_var("XDG_RUNTIME_DIR");
        }
        let path = get_socket_path();
        // Should contain /tmp/remoshell- prefix
        assert!(path.to_str().unwrap().starts_with("/tmp/remoshell-"));
        assert!(path.to_str().unwrap().ends_with("/daemon.sock"));

        // Restore original value
        // SAFETY: This is a test, running in isolation
        unsafe {
            if let Some(val) = original {
                std::env::set_var("XDG_RUNTIME_DIR", val);
            }
        }
    }

    #[test]
    fn test_socket_path_is_absolute() {
        let path = get_socket_path();
        assert!(path.is_absolute());
    }

    #[test]
    fn test_socket_path_ends_with_sock() {
        let path = get_socket_path();
        assert!(path.extension().map(|e| e == "sock").unwrap_or(false));
    }
}
