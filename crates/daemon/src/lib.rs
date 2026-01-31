//! # RemoShell Daemon Library
//!
//! This crate provides the daemon (server) functionality for RemoShell,
//! enabling secure remote shell access to the host machine.
//!
//! ## Overview
//!
//! The daemon is the core service that runs on machines you want to access remotely.
//! It provides:
//!
//! - **PTY Session Management**: Create and manage pseudo-terminal sessions
//! - **File Operations**: Browse directories and transfer files securely
//! - **Device Trust**: Manage trusted client devices with QR code pairing
//! - **Network Handlers**: WebRTC and QUIC connection handling
//! - **User Interface**: Terminal UI with QR code display and systemd integration
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Daemon Orchestrator                         │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
//! │  │   Session    │  │     File     │  │      Device          │  │
//! │  │   Manager    │  │   Transfer   │  │    Trust Store       │  │
//! │  └──────────────┘  └──────────────┘  └──────────────────────┘  │
//! │                                                                  │
//! │  ┌────────────────────────────────────────────────────────────┐ │
//! │  │                   Message Router                           │ │
//! │  └────────────────────────────────────────────────────────────┘ │
//! │                                                                  │
//! │  ┌───────────────────┐  ┌───────────────────────────────────┐  │
//! │  │  WebRTC Handler   │  │        QUIC Handler               │  │
//! │  └───────────────────┘  └───────────────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use daemon::{Config, DaemonOrchestrator};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Load or create configuration
//!     let config = Config::load_or_default()?;
//!
//!     // Create and start the orchestrator
//!     let mut orchestrator = DaemonOrchestrator::new(config)?;
//!     orchestrator.start().await?;
//!
//!     // The daemon is now running and accepting connections
//!     // Wait for shutdown signal...
//!
//!     orchestrator.stop().await?;
//!     Ok(())
//! }
//! ```
//!
//! ## Modules
//!
//! - [`config`]: Configuration loading and defaults
//! - [`session`]: PTY session creation and management
//! - [`devices`]: Device trust store
//! - [`files`]: File browsing and transfer
//! - [`network`]: WebRTC and QUIC connection handlers
//! - [`router`]: Message routing to handlers
//! - [`ui`]: TUI, QR code generation, systemd integration
//! - [`orchestrator`]: Main daemon coordinator

pub mod config;
pub mod devices;
pub mod files;
pub mod network;
pub mod orchestrator;
pub mod router;
pub mod session;
pub mod ui;

// Re-export protocol for convenience
pub use protocol;

// Re-export config types for convenience
pub use config::Config;

// Re-export device types for convenience
pub use devices::{TrustLevel, TrustStore, TrustedDevice};

// Re-export session types for convenience
pub use session::{Session, SessionError, SessionId, SessionManager, SessionManagerImpl, SessionStatus};

// Re-export network types for convenience
pub use network::{
    ChannelType, Connection, IceServer, QuicConfig, QuicConnectionHandler, WebRtcConfig,
    WebRtcConnectionHandler, REMOSHELL_ALPN,
};

// Re-export files types for convenience
pub use files::{DevicePermissions, DirectoryBrowser, DirectoryEntry, FileTransfer, PathPermissions};

// Re-export router types for convenience
pub use router::{MessageRouter, RouterError, RouterResult};

// Re-export UI types for convenience
pub use ui::{
    device_id_to_hex, generate_minimal_unit_file, generate_png_qr, generate_terminal_qr,
    generate_unit_file, is_systemd, notify_mainpid, notify_ready, notify_status, notify_stopping,
    notify_watchdog, process_approval_result, ApprovalAction, ApprovalInfo, ApprovalResult,
    DaemonStats, DeviceInfo, PairingInfo, SessionInfo, SignalHandler, SystemdContext, Tab, TuiApp,
    TuiEvent, DEFAULT_EXPIRY_SECONDS,
};

// Re-export orchestrator types for convenience
pub use orchestrator::{DaemonOrchestrator, OrchestratorEvent, OrchestratorState};
