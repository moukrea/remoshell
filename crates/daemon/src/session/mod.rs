//! Session management module.
//!
//! This module provides PTY spawning and session lifecycle management.
//! Sessions can be created, attached to, detached from, resized, and killed.

pub mod manager;
pub mod multiplexer;
pub mod pty;

pub use manager::{SessionManager, SessionManagerImpl};
pub use multiplexer::{ClientHandle, ClientId, ClientStats, SessionOutputBroadcaster};
pub use pty::{Session, SessionError, SessionId, SessionStatus};
