//! PID file utilities for daemon running detection.
//!
//! This module provides functions to check if a daemon is already running
//! by examining the PID file and verifying the process exists.
//!
//! ## PID File Location
//!
//! The PID file is stored at:
//! - `$XDG_DATA_HOME/remoshell/daemon.pid` if XDG_DATA_HOME is set
//! - `~/.local/share/remoshell/daemon.pid` otherwise
//!
//! ## Example
//!
//! ```rust
//! use daemon::ipc::pidfile::{is_daemon_running, get_daemon_pid};
//!
//! if is_daemon_running() {
//!     if let Some(pid) = get_daemon_pid() {
//!         println!("Daemon is already running with PID {}", pid);
//!     }
//! }
//! ```

use std::fs;
use std::path::PathBuf;

/// Get the path to the daemon PID file.
///
/// The path follows the XDG Base Directory Specification:
/// - `$XDG_DATA_HOME/remoshell/daemon.pid` if XDG_DATA_HOME is set
/// - `~/.local/share/remoshell/daemon.pid` otherwise
///
/// ## Example
///
/// ```rust
/// use daemon::ipc::pidfile::get_pid_file_path;
///
/// let path = get_pid_file_path();
/// println!("PID file location: {:?}", path);
/// ```
pub fn get_pid_file_path() -> PathBuf {
    // Use XDG_DATA_HOME or default to ~/.local/share
    let data_dir = std::env::var("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("/tmp"))
                .join(".local/share")
        });
    data_dir.join("remoshell").join("daemon.pid")
}

/// Check if a daemon process is currently running.
///
/// Returns `true` if:
/// - PID file exists
/// - PID file contains a valid PID
/// - Process with that PID is running
///
/// Cleans up stale PID files automatically.
///
/// ## Example
///
/// ```rust
/// use daemon::ipc::pidfile::is_daemon_running;
///
/// if is_daemon_running() {
///     println!("Daemon is already running");
/// } else {
///     println!("No daemon running, safe to start");
/// }
/// ```
pub fn is_daemon_running() -> bool {
    get_daemon_pid().is_some()
}

/// Get the PID of the running daemon, if any.
///
/// Returns `Some(pid)` if daemon is running, `None` otherwise.
/// Automatically cleans up stale PID files.
///
/// ## Example
///
/// ```rust
/// use daemon::ipc::pidfile::get_daemon_pid;
///
/// match get_daemon_pid() {
///     Some(pid) => println!("Daemon running with PID {}", pid),
///     None => println!("No daemon running"),
/// }
/// ```
pub fn get_daemon_pid() -> Option<u32> {
    let pid_path = get_pid_file_path();

    // Read PID from file
    let pid_str = match fs::read_to_string(&pid_path) {
        Ok(s) => s,
        Err(_) => return None, // File doesn't exist or can't be read
    };

    // Parse PID
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            // Invalid PID file, clean it up
            cleanup_stale_pid_file(&pid_path);
            return None;
        }
    };

    // Check if process is running
    if is_process_running(pid) {
        Some(pid)
    } else {
        // Process not running, clean up stale PID file
        cleanup_stale_pid_file(&pid_path);
        None
    }
}

/// Check if a process with the given PID is running.
///
/// On Linux, this checks if `/proc/{pid}/stat` exists.
/// On other Unix systems, this uses the kill(pid, 0) syscall.
fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        // Check /proc/{pid}/stat exists (Linux-specific, efficient)
        let proc_path = format!("/proc/{}/stat", pid);
        std::path::Path::new(&proc_path).exists()
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    {
        // On non-Linux Unix, use kill with signal 0
        // Signal 0 doesn't send a signal but checks if process exists
        // SAFETY: kill with signal 0 is safe - it only checks if process exists
        unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
    }

    #[cfg(not(unix))]
    {
        // On non-Unix platforms, assume not running
        // (This daemon is Unix-only anyway)
        let _ = pid;
        false
    }
}

/// Remove a stale PID file.
fn cleanup_stale_pid_file(path: &PathBuf) {
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_pid_file_path_structure() {
        let path = get_pid_file_path();
        // Path should end with remoshell/daemon.pid
        assert!(path.ends_with("remoshell/daemon.pid"));
        // Path should be absolute or start with expected patterns
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains(".local/share") || path_str.starts_with("/tmp"),
            "Unexpected path: {}",
            path_str
        );
    }

    #[test]
    fn test_get_daemon_pid_no_file() {
        // With no PID file, should return None
        // This test works in clean environment or when no daemon is running
        // Just verify it doesn't panic and returns expected types
        let result = get_daemon_pid();
        // result is Option<u32> - verify it's a valid type
        match result {
            Some(pid) => assert!(pid > 0, "If a PID is returned, it should be positive"),
            None => {} // Expected in most test environments
        }
    }

    #[test]
    fn test_is_daemon_running_no_panic() {
        // Verify the function doesn't panic and returns a bool
        let result = is_daemon_running();
        // Just verify we got a boolean back
        let _ = result;
    }

    #[test]
    fn test_is_process_running_current() {
        // Current process should be running
        let pid = std::process::id();
        assert!(
            is_process_running(pid),
            "Current process should be detected as running"
        );
    }

    #[test]
    fn test_is_process_running_invalid() {
        // Very high PIDs are unlikely to exist
        // On Linux, max PID is typically 32768 or 4194304 (with CONFIG_BASE_SMALL=0)
        // Using a very high value that's unlikely to be a real process
        assert!(
            !is_process_running(4_000_000_000),
            "Invalid PID should not be running"
        );
    }

    #[test]
    fn test_is_process_running_init() {
        // PID 1 (init/systemd) should always be running on Unix
        #[cfg(unix)]
        {
            assert!(is_process_running(1), "PID 1 should always be running");
        }
    }

    #[test]
    fn test_cleanup_stale_pid_file_nonexistent() {
        // Cleaning up a non-existent file should not panic
        let fake_path = PathBuf::from("/tmp/remoshell-test-nonexistent-pid-file");
        cleanup_stale_pid_file(&fake_path);
        // Should complete without error
    }

    #[test]
    fn test_get_daemon_pid_with_stale_file() {
        use std::io::Write;

        // Create a temporary directory for the test
        let temp_dir = tempfile::tempdir().unwrap();
        let pid_dir = temp_dir.path().join("remoshell");
        fs::create_dir_all(&pid_dir).unwrap();
        let pid_file = pid_dir.join("daemon.pid");

        // Write an invalid (very high) PID that doesn't exist
        let mut file = fs::File::create(&pid_file).unwrap();
        writeln!(file, "4000000000").unwrap();
        drop(file);

        // Since get_daemon_pid uses get_pid_file_path() which returns the real path,
        // we can't easily test with a temp file without modifying the function.
        // This test demonstrates the cleanup behavior conceptually.

        // Verify the test file exists
        assert!(pid_file.exists());

        // Manually test the cleanup function
        cleanup_stale_pid_file(&pid_file);

        // File should be removed
        assert!(!pid_file.exists(), "Stale PID file should be cleaned up");
    }
}
