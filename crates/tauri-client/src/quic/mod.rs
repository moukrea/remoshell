//! QUIC networking module for Tauri client.
//!
//! This module provides QUIC connectivity to the RemoShell daemon using the iroh crate.
//! It handles:
//! - Connection management with automatic reconnection
//! - Bi-directional streams for different channel types
//! - Event emission for Tauri frontend integration
//! - TLS 1.3 encryption (native to QUIC)

pub mod manager;

pub use manager::{
    ChannelType, ConnectionEvent, ConnectionState, QuicConfig, QuicManager,
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_STREAM_TIMEOUT, REMOSHELL_ALPN,
};
