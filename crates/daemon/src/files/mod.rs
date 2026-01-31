//! File manager module for directory browsing and file transfer.
//!
//! This module provides secure file operations including:
//! - Directory listing with path validation
//! - Chunked file downloads and uploads
//! - Atomic file writes using temp files
//! - Per-device path permission enforcement
//!
//! # Security
//!
//! All path operations are validated against allowed paths. Path traversal
//! attacks are prevented by canonicalizing all paths and rejecting symlinks
//! that point outside allowed boundaries.

pub mod browser;
pub mod permissions;
pub mod transfer;

pub use browser::{DirectoryBrowser, DirectoryEntry};
pub use permissions::{DevicePermissions, PathPermissions};
pub use transfer::{FileTransfer, TransferError, UploadState};
