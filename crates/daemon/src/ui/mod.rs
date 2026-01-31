//! User interface module for the RemoShell daemon.
//!
//! This module provides terminal-based user interfaces for managing
//! the daemon, including a TUI for interactive management, QR code
//! generation for device pairing, and systemd integration.

pub mod qr;
pub mod systemd;
pub mod tui;

// Re-export main types for convenience
pub use tui::{
    device_id_to_hex, process_approval_result, ApprovalAction, ApprovalInfo, ApprovalResult,
    DaemonStats, DeviceInfo, SessionInfo, Tab, TuiApp, TuiEvent,
};

// Re-export QR types for convenience
pub use qr::{generate_png_qr, generate_terminal_qr, PairingInfo, DEFAULT_EXPIRY_SECONDS};

// Re-export systemd types for convenience
pub use systemd::{
    generate_minimal_unit_file, generate_unit_file, is_systemd, notify_mainpid, notify_ready,
    notify_status, notify_stopping, notify_watchdog, SignalHandler, SystemdContext,
};
