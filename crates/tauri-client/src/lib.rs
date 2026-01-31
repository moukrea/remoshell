//! # RemoShell Tauri Client Library
//!
//! This crate provides the Tauri desktop client backend for RemoShell,
//! including QUIC connectivity to daemon instances and persistent storage.
//!
//! ## Overview
//!
//! The tauri-client crate bridges the SolidJS frontend with native Rust
//! capabilities, providing:
//!
//! - **QUIC Connections**: Direct connections to daemons via iroh
//! - **SQLite Storage**: Persistent storage for paired devices and settings
//! - **OS Keychain**: Secure storage for device identity keys
//! - **IPC Commands**: Tauri command handlers for frontend communication
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    SolidJS Frontend                             │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                   Tauri IPC Bridge                              │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
//! │  │ QUIC Manager │  │   Storage    │  │      Keychain        │  │
//! │  │   (iroh)     │  │   (SQLite)   │  │  (OS keyring)        │  │
//! │  └──────────────┘  └──────────────┘  └──────────────────────┘  │
//! │                                                                  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage with Tauri
//!
//! Register the command handlers in your Tauri application:
//!
//! ```rust,ignore
//! use tauri_client::generate_handler;
//!
//! fn main() {
//!     tauri::Builder::default()
//!         .invoke_handler(generate_handler!())
//!         .run(tauri::generate_context!())
//!         .expect("error while running tauri application");
//! }
//! ```
//!
//! ## Available Commands
//!
//! The following IPC commands are exposed to the frontend:
//!
//! - `initialize_app`: Initialize app state and load device keys
//! - `connect_quic`: Establish QUIC connection to a daemon
//! - `disconnect_quic`: Close QUIC connection
//! - `send_quic_data`: Send data over QUIC channel
//! - `get_connection_status`: Get current connection state
//! - `get_device_keys`: Retrieve device identity
//! - `get_paired_devices`: List all paired devices
//! - `store_paired_device`: Save a new paired device
//! - `remove_paired_device`: Remove a paired device
//! - `show_native_notification`: Display OS notification
//!
//! ## Modules
//!
//! - [`commands`]: Tauri IPC command handlers
//! - [`quic`]: QUIC connection management
//! - [`storage`]: SQLite database and keychain access

pub mod commands;
pub mod quic;
pub mod storage;

// Re-export protocol for convenience
pub use protocol;

// Re-export key types from quic module for convenience
pub use quic::{
    ChannelType, ConnectionEvent, ConnectionState, QuicConfig, QuicManager, REMOSHELL_ALPN,
};

// Re-export command types and app state
pub use commands::{AppState, CommandError, CommandResult};

/// Client configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClientConfig {
    /// Server address to connect to.
    pub server_address: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_address: "localhost:9000".to_string(),
        }
    }
}

/// Generate the Tauri command handler with all registered commands.
///
/// This macro generates a `tauri::generate_handler![]` with all the
/// IPC commands exposed by this crate.
///
/// # Example
///
/// ```rust,ignore
/// use tauri_client::generate_handler;
///
/// fn main() {
///     tauri::Builder::default()
///         .invoke_handler(generate_handler!())
///         .run(tauri::generate_context!())
///         .expect("error while running tauri application");
/// }
/// ```
#[macro_export]
macro_rules! generate_handler {
    () => {
        tauri::generate_handler![
            $crate::commands::initialize_app,
            $crate::commands::connect_quic,
            $crate::commands::disconnect_quic,
            $crate::commands::send_quic_data,
            $crate::commands::get_connection_status,
            $crate::commands::get_device_keys,
            $crate::commands::has_device_keys,
            $crate::commands::get_paired_devices,
            $crate::commands::get_paired_device,
            $crate::commands::store_paired_device,
            $crate::commands::remove_paired_device,
            $crate::commands::update_device_last_seen,
            $crate::commands::show_native_notification,
        ]
    };
}

/// List of all Tauri command functions for use with `tauri::generate_handler![]`.
///
/// If you need more control over the command handler, you can use these
/// functions directly:
///
/// ```rust,ignore
/// tauri::generate_handler![
///     tauri_client::commands::initialize_app,
///     tauri_client::commands::connect_quic,
///     tauri_client::commands::disconnect_quic,
///     // ... etc
/// ]
/// ```
pub mod command_list {
    pub use crate::commands::{
        connect_quic, disconnect_quic, get_connection_status, get_device_keys, get_paired_device,
        get_paired_devices, has_device_keys, initialize_app, remove_paired_device, send_quic_data,
        show_native_notification, store_paired_device, update_device_last_seen,
    };
}
