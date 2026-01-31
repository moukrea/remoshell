//! Network module for peer-to-peer connections.
//!
//! This module provides connection handling for both:
//! - WebRTC connections for browser clients (ICE, signaling, data channels)
//! - QUIC connections for native Tauri clients (iroh, hole punching, TLS 1.3)

pub mod quic;
pub mod signaling;
pub mod webrtc;

use std::future::Future;
use std::pin::Pin;

use protocol::error::Result;

/// A trait representing a secure network connection.
///
/// This trait abstracts over different connection types (WebRTC, TCP, etc.)
/// and provides a common interface for sending and receiving encrypted messages.
pub trait Connection: Send + Sync {
    /// Sends an encrypted message over the connection.
    ///
    /// The data is encrypted using the established Noise session before transmission.
    fn send<'a>(
        &'a mut self,
        channel: ChannelType,
        data: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Receives and decrypts a message from the connection.
    ///
    /// Returns the decrypted plaintext data.
    fn recv<'a>(
        &'a mut self,
        channel: ChannelType,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>>> + Send + 'a>>;

    /// Closes the connection gracefully.
    fn close<'a>(&'a mut self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Returns whether the connection is currently open.
    fn is_connected(&self) -> bool;

    /// Returns the remote peer's X25519 public key (from Noise handshake).
    fn peer_public_key(&self) -> Option<[u8; 32]>;
}

/// The type of data channel for message routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelType {
    /// Control channel: ordered, reliable - session management
    Control,
    /// Terminal channel: unordered - low-latency I/O
    Terminal,
    /// Files channel: ordered, reliable - file transfers
    Files,
}

impl ChannelType {
    /// Returns the channel name for WebRTC data channel creation.
    pub fn channel_name(&self) -> &'static str {
        match self {
            ChannelType::Control => "control",
            ChannelType::Terminal => "terminal",
            ChannelType::Files => "files",
        }
    }
}

// Re-export key types
pub use quic::{QuicConfig, QuicConnectionHandler, REMOSHELL_ALPN};
pub use signaling::{
    ConnectionState as SignalingConnectionState, SignalingClient, SignalingConfig, SignalingEvent,
    SignalingMessage, WebSocketSignalingClient,
};
pub use webrtc::{IceServer, WebRtcConfig, WebRtcConnectionHandler};
