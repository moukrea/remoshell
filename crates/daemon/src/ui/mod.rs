//! User interface module for the RemoShell daemon.
//!
//! This module provides terminal-based user interfaces for managing
//! the daemon, including a TUI for interactive management, QR code
//! generation for device pairing, and systemd integration.

pub mod qr;
#[cfg(target_os = "linux")]
pub mod systemd;
pub mod tui;

// Re-export main types for convenience
pub use tui::{
    device_id_to_hex, process_approval_result, ApprovalAction, ApprovalInfo, ApprovalResult,
    DaemonStats, DeviceInfo, DisplayTrustLevel, PairingConfig, SessionInfo, Tab, TuiApp, TuiEvent,
};

// Re-export QR types for convenience
pub use qr::{
    generate_pairing_code, generate_png_qr, generate_png_qr_from_data, generate_qr_modules,
    generate_qr_modules_from_data, generate_terminal_qr, generate_terminal_qr_from_data,
    generate_terminal_qr_inverted, pairing_url, register_pairing_code, signaling_url_to_http,
    to_base58, PairingInfo, DEFAULT_EXPIRY_SECONDS,
};

// Re-export systemd types for convenience (Linux only)
#[cfg(target_os = "linux")]
pub use systemd::{
    generate_minimal_unit_file, generate_unit_file, is_systemd, notify_mainpid, notify_ready,
    notify_status, notify_stopping, notify_watchdog, SignalHandler, SystemdContext,
};
