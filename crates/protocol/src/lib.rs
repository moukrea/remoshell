//! # RemoShell Protocol Library
//!
//! This crate provides protocol definitions and cryptographic primitives
//! for the RemoShell remote shell system.
//!
//! ## Overview
//!
//! The protocol crate is the foundation of RemoShell's communication layer,
//! providing:
//!
//! - **Message Definitions**: All RPC message types for session, file, and device operations
//! - **Cryptographic Identity**: Ed25519 key generation, signing, and verification
//! - **Noise Protocol**: Secure handshake and transport encryption using Noise XX
//! - **Frame Codec**: Length-prefixed framing with optional LZ4 compression
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │          Application Messages           │  MessagePack-encoded
//! ├─────────────────────────────────────────┤
//! │           Noise Encryption              │  ChaCha20-Poly1305
//! ├─────────────────────────────────────────┤
//! │              Framing                    │  Length-prefixed, LZ4
//! ├─────────────────────────────────────────┤
//! │         Transport (WebRTC/QUIC)         │
//! └─────────────────────────────────────────┘
//! ```
//!
//! ## Example Usage
//!
//! ```rust
//! use protocol::{DeviceIdentity, Envelope, Message, FrameCodec, Frame};
//! use protocol::messages::SessionCreate;
//!
//! // Generate a device identity
//! let identity = DeviceIdentity::generate();
//! println!("Device ID: {}", identity.fingerprint());
//!
//! // Create a session request message
//! let message = Message::SessionCreate(SessionCreate::default());
//! let envelope = Envelope::new(1, message);
//!
//! // Serialize to MessagePack
//! let bytes = envelope.to_msgpack().unwrap();
//!
//! // Wrap in a frame for transport
//! let codec = FrameCodec::new();
//! let frame_bytes = codec.encode(&Frame::new(bytes)).unwrap();
//! ```
//!
//! ## Modules
//!
//! - [`crypto`]: Device identity, key management, and signatures
//! - [`messages`]: Protocol message definitions
//! - [`framing`]: Frame codec with compression
//! - [`noise`]: Noise XX handshake and encryption
//! - [`error`]: Error types

pub mod crypto;
pub mod error;
pub mod framing;
pub mod messages;
pub mod noise;

pub use crypto::{DeviceId, DeviceIdentity, PeerIdentity, Signature, DEVICE_ID_LENGTH};
pub use error::{ProtocolError, Result};
pub use framing::{
    Frame, FrameCodec, FrameFlags, COMPRESSION_THRESHOLD, FRAME_HEADER_SIZE, FRAME_MAGIC,
    MAX_FRAME_SIZE,
};
pub use messages::{Envelope, Message, PROTOCOL_VERSION};
pub use noise::{
    HandshakePhase, NoiseSession, Role, SecureHandshake, MAX_NOISE_MESSAGE_SIZE, NOISE_OVERHEAD,
};
