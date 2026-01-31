//! Protocol message definitions for RemoShell.
//!
//! This module defines all RPC message types used for communication between
//! the daemon and clients. All messages are serialized using MessagePack.

use serde::{Deserialize, Serialize};

/// Current protocol version.
pub const PROTOCOL_VERSION: u8 = 1;

/// Envelope wrapper for all protocol messages.
///
/// The envelope provides versioning and sequence numbers for message ordering
/// and compatibility checking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// Protocol version for compatibility checking.
    pub version: u8,
    /// Sequence number for message ordering and acknowledgment.
    pub sequence: u64,
    /// The actual message payload.
    pub payload: Message,
}

impl Envelope {
    /// Create a new envelope with the current protocol version.
    pub fn new(sequence: u64, payload: Message) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            sequence,
            payload,
        }
    }
}

/// Top-level message enum containing all message types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Message {
    // Session messages
    /// Request to create a new session.
    SessionCreate(SessionCreate),
    /// Response confirming session creation.
    SessionCreated(SessionCreated),
    /// Request to attach to an existing session.
    SessionAttach(SessionAttach),
    /// Request to detach from a session.
    SessionDetach(SessionDetach),
    /// Request to kill a session.
    SessionKill(SessionKill),
    /// Terminal resize notification.
    SessionResize(SessionResize),
    /// Session data (stdin/stdout/stderr).
    SessionData(SessionData),
    /// Session closed notification.
    SessionClosed(SessionClosed),

    // File messages
    /// Request to list files in a directory.
    FileListRequest(FileListRequest),
    /// Response with directory listing.
    FileListResponse(FileListResponse),
    /// Request to download a file.
    FileDownloadRequest(FileDownloadRequest),
    /// Chunk of downloaded file data.
    FileDownloadChunk(FileDownloadChunk),
    /// Start a file upload.
    FileUploadStart(FileUploadStart),
    /// Chunk of uploaded file data.
    FileUploadChunk(FileUploadChunk),
    /// Complete a file upload.
    FileUploadComplete(FileUploadComplete),

    // Device messages
    /// Device information announcement.
    DeviceInfo(DeviceInfo),
    /// Request approval to connect.
    DeviceApprovalRequest(DeviceApprovalRequest),
    /// Device connection approved.
    DeviceApproved(DeviceApproved),
    /// Device connection rejected.
    DeviceRejected(DeviceRejected),

    // Control messages
    /// Ping for keepalive.
    Ping(Ping),
    /// Pong response to ping.
    Pong(Pong),
    /// Error message.
    Error(ErrorMessage),
    /// Capabilities announcement.
    Capabilities(Capabilities),
}

// ============================================================================
// Session Messages
// ============================================================================

/// Request to create a new shell session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCreate {
    /// Requested terminal columns.
    pub cols: u16,
    /// Requested terminal rows.
    pub rows: u16,
    /// Optional shell command to run (default: user's shell).
    pub shell: Option<String>,
    /// Environment variables to set.
    pub env: Vec<(String, String)>,
    /// Working directory for the session.
    pub cwd: Option<String>,
}

impl Default for SessionCreate {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            shell: None,
            env: Vec::new(),
            cwd: None,
        }
    }
}

/// Response confirming session creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCreated {
    /// Unique session identifier.
    pub session_id: String,
    /// Process ID of the shell.
    pub pid: u32,
}

/// Request to attach to an existing session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAttach {
    /// Session ID to attach to.
    pub session_id: String,
}

/// Request to detach from a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDetach {
    /// Session ID to detach from.
    pub session_id: String,
}

/// Request to kill a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionKill {
    /// Session ID to kill.
    pub session_id: String,
    /// Optional signal to send (default: SIGTERM).
    pub signal: Option<i32>,
}

/// Terminal resize notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResize {
    /// Session ID to resize.
    pub session_id: String,
    /// New terminal columns.
    pub cols: u16,
    /// New terminal rows.
    pub rows: u16,
}

/// Session data (input or output).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionData {
    /// Session ID this data belongs to.
    pub session_id: String,
    /// The data stream type.
    pub stream: DataStream,
    /// The actual data bytes.
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// Data stream type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataStream {
    /// Standard input (client to daemon).
    Stdin,
    /// Standard output (daemon to client).
    Stdout,
    /// Standard error (daemon to client).
    Stderr,
}

/// Session closed notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionClosed {
    /// Session ID that was closed.
    pub session_id: String,
    /// Exit code if the process exited normally.
    pub exit_code: Option<i32>,
    /// Signal number if the process was killed by a signal.
    pub signal: Option<i32>,
    /// Human-readable reason for closure.
    pub reason: Option<String>,
}

// ============================================================================
// File Messages
// ============================================================================

/// Request to list files in a directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileListRequest {
    /// Path to list.
    pub path: String,
    /// Include hidden files.
    pub include_hidden: bool,
}

/// Response with directory listing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileListResponse {
    /// Path that was listed.
    pub path: String,
    /// List of entries in the directory.
    pub entries: Vec<FileEntry>,
}

/// A single file or directory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Entry name (not full path).
    pub name: String,
    /// Entry type.
    pub entry_type: FileEntryType,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Unix permissions mode.
    pub mode: u32,
    /// Last modified timestamp (Unix epoch seconds).
    pub modified: u64,
}

/// Type of file entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileEntryType {
    /// Regular file.
    File,
    /// Directory.
    Directory,
    /// Symbolic link.
    Symlink,
    /// Other (device, socket, etc.).
    Other,
}

/// Request to download a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDownloadRequest {
    /// Path to download.
    pub path: String,
    /// Starting offset (for resuming).
    pub offset: u64,
    /// Maximum chunk size.
    pub chunk_size: u32,
}

/// Chunk of downloaded file data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDownloadChunk {
    /// Path being downloaded.
    pub path: String,
    /// Offset of this chunk.
    pub offset: u64,
    /// Total file size.
    pub total_size: u64,
    /// The chunk data.
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
    /// Whether this is the last chunk.
    pub is_last: bool,
}

/// Start a file upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileUploadStart {
    /// Destination path.
    pub path: String,
    /// Total file size.
    pub size: u64,
    /// Unix permissions mode.
    pub mode: u32,
    /// Whether to overwrite if exists.
    pub overwrite: bool,
}

/// Chunk of uploaded file data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileUploadChunk {
    /// Destination path.
    pub path: String,
    /// Offset of this chunk.
    pub offset: u64,
    /// The chunk data.
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

/// Complete a file upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileUploadComplete {
    /// Destination path.
    pub path: String,
    /// SHA-256 hash of the complete file for verification.
    #[serde(with = "serde_bytes")]
    pub checksum: Vec<u8>,
}

// ============================================================================
// Device Messages
// ============================================================================

/// Device information announcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// Unique device identifier (public key fingerprint).
    pub device_id: String,
    /// Human-readable device name.
    pub name: String,
    /// Operating system.
    pub os: String,
    /// OS version.
    pub os_version: String,
    /// Device architecture.
    pub arch: String,
    /// Protocol version supported.
    pub protocol_version: u8,
}

/// Request approval to connect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceApprovalRequest {
    /// Device ID requesting approval.
    pub device_id: String,
    /// Device name.
    pub name: String,
    /// Public key for verification.
    #[serde(with = "serde_bytes")]
    pub public_key: Vec<u8>,
    /// Human-readable reason for connection.
    pub reason: Option<String>,
}

/// Device connection approved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceApproved {
    /// Device ID that was approved.
    pub device_id: String,
    /// Optional trust expiration (Unix timestamp).
    pub expires_at: Option<u64>,
    /// Allowed capabilities for this device.
    pub allowed_capabilities: Vec<String>,
}

/// Device connection rejected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceRejected {
    /// Device ID that was rejected.
    pub device_id: String,
    /// Reason for rejection.
    pub reason: String,
    /// Whether the device should retry later.
    pub retry_allowed: bool,
}

// ============================================================================
// Control Messages
// ============================================================================

/// Ping for keepalive and latency measurement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ping {
    /// Timestamp when ping was sent (for latency calculation).
    pub timestamp: u64,
    /// Optional payload for echo.
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

/// Pong response to ping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pong {
    /// Original timestamp from ping.
    pub timestamp: u64,
    /// Echo of the original payload.
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
}

/// Error message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorMessage {
    /// Error code for programmatic handling.
    pub code: ErrorCode,
    /// Human-readable error message.
    pub message: String,
    /// Optional context (e.g., session_id, path).
    pub context: Option<String>,
    /// Whether the error is recoverable.
    pub recoverable: bool,
}

/// Error codes for common error conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// Unknown or unspecified error.
    Unknown,
    /// Authentication or authorization failure.
    Unauthorized,
    /// Resource not found.
    NotFound,
    /// Invalid request or parameters.
    InvalidRequest,
    /// Server-side error.
    InternalError,
    /// Request timed out.
    Timeout,
    /// Rate limited.
    RateLimited,
    /// Resource already exists.
    AlreadyExists,
    /// Insufficient permissions.
    PermissionDenied,
    /// Protocol version mismatch.
    VersionMismatch,
}

/// Capabilities announcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Supported protocol versions.
    pub protocol_versions: Vec<u8>,
    /// Supported features.
    pub features: Vec<String>,
    /// Maximum message size supported.
    pub max_message_size: u32,
    /// Maximum concurrent sessions supported.
    pub max_sessions: u32,
    /// Supported compression algorithms.
    pub compression: Vec<String>,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            protocol_versions: vec![PROTOCOL_VERSION],
            features: vec![
                "shell".to_string(),
                "file-transfer".to_string(),
                "device-trust".to_string(),
            ],
            max_message_size: 1024 * 1024, // 1MB
            max_sessions: 16,
            compression: vec!["lz4".to_string()],
        }
    }
}

// ============================================================================
// Serialization helpers
// ============================================================================

impl Envelope {
    /// Serialize the envelope to MessagePack bytes.
    pub fn to_msgpack(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec(self)
    }

    /// Deserialize an envelope from MessagePack bytes.
    pub fn from_msgpack(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        rmp_serde::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to test roundtrip serialization
    fn roundtrip_envelope(msg: Message) {
        let envelope = Envelope::new(42, msg);
        let bytes = envelope.to_msgpack().expect("serialization failed");
        let decoded = Envelope::from_msgpack(&bytes).expect("deserialization failed");
        assert_eq!(envelope, decoded);
    }

    #[test]
    fn test_envelope_version() {
        let envelope = Envelope::new(1, Message::Ping(Ping {
            timestamp: 12345,
            payload: vec![],
        }));
        assert_eq!(envelope.version, PROTOCOL_VERSION);
    }

    #[test]
    fn test_envelope_sequence() {
        let envelope = Envelope::new(999, Message::Ping(Ping {
            timestamp: 0,
            payload: vec![],
        }));
        assert_eq!(envelope.sequence, 999);
    }

    // Session message roundtrip tests

    #[test]
    fn test_session_create_roundtrip() {
        roundtrip_envelope(Message::SessionCreate(SessionCreate {
            cols: 120,
            rows: 40,
            shell: Some("/bin/bash".to_string()),
            env: vec![
                ("TERM".to_string(), "xterm-256color".to_string()),
                ("LANG".to_string(), "en_US.UTF-8".to_string()),
            ],
            cwd: Some("/home/user".to_string()),
        }));
    }

    #[test]
    fn test_session_create_default_roundtrip() {
        roundtrip_envelope(Message::SessionCreate(SessionCreate::default()));
    }

    #[test]
    fn test_session_created_roundtrip() {
        roundtrip_envelope(Message::SessionCreated(SessionCreated {
            session_id: "sess-abc123".to_string(),
            pid: 12345,
        }));
    }

    #[test]
    fn test_session_attach_roundtrip() {
        roundtrip_envelope(Message::SessionAttach(SessionAttach {
            session_id: "sess-xyz789".to_string(),
        }));
    }

    #[test]
    fn test_session_detach_roundtrip() {
        roundtrip_envelope(Message::SessionDetach(SessionDetach {
            session_id: "sess-abc123".to_string(),
        }));
    }

    #[test]
    fn test_session_kill_roundtrip() {
        roundtrip_envelope(Message::SessionKill(SessionKill {
            session_id: "sess-abc123".to_string(),
            signal: Some(9),
        }));
    }

    #[test]
    fn test_session_resize_roundtrip() {
        roundtrip_envelope(Message::SessionResize(SessionResize {
            session_id: "sess-abc123".to_string(),
            cols: 200,
            rows: 50,
        }));
    }

    #[test]
    fn test_session_data_stdin_roundtrip() {
        roundtrip_envelope(Message::SessionData(SessionData {
            session_id: "sess-abc123".to_string(),
            stream: DataStream::Stdin,
            data: b"ls -la\n".to_vec(),
        }));
    }

    #[test]
    fn test_session_data_stdout_roundtrip() {
        roundtrip_envelope(Message::SessionData(SessionData {
            session_id: "sess-abc123".to_string(),
            stream: DataStream::Stdout,
            data: b"total 42\ndrwxr-xr-x  2 user user 4096 Jan  1 12:00 .\n".to_vec(),
        }));
    }

    #[test]
    fn test_session_data_stderr_roundtrip() {
        roundtrip_envelope(Message::SessionData(SessionData {
            session_id: "sess-abc123".to_string(),
            stream: DataStream::Stderr,
            data: b"Error: file not found\n".to_vec(),
        }));
    }

    #[test]
    fn test_session_closed_roundtrip() {
        roundtrip_envelope(Message::SessionClosed(SessionClosed {
            session_id: "sess-abc123".to_string(),
            exit_code: Some(0),
            signal: None,
            reason: Some("Process exited normally".to_string()),
        }));
    }

    #[test]
    fn test_session_closed_signal_roundtrip() {
        roundtrip_envelope(Message::SessionClosed(SessionClosed {
            session_id: "sess-abc123".to_string(),
            exit_code: None,
            signal: Some(9),
            reason: Some("Killed by SIGKILL".to_string()),
        }));
    }

    // File message roundtrip tests

    #[test]
    fn test_file_list_request_roundtrip() {
        roundtrip_envelope(Message::FileListRequest(FileListRequest {
            path: "/home/user/documents".to_string(),
            include_hidden: true,
        }));
    }

    #[test]
    fn test_file_list_response_roundtrip() {
        roundtrip_envelope(Message::FileListResponse(FileListResponse {
            path: "/home/user".to_string(),
            entries: vec![
                FileEntry {
                    name: "file.txt".to_string(),
                    entry_type: FileEntryType::File,
                    size: 1024,
                    mode: 0o644,
                    modified: 1704067200,
                },
                FileEntry {
                    name: "docs".to_string(),
                    entry_type: FileEntryType::Directory,
                    size: 0,
                    mode: 0o755,
                    modified: 1704067200,
                },
                FileEntry {
                    name: "link".to_string(),
                    entry_type: FileEntryType::Symlink,
                    size: 0,
                    mode: 0o777,
                    modified: 1704067200,
                },
            ],
        }));
    }

    #[test]
    fn test_file_download_request_roundtrip() {
        roundtrip_envelope(Message::FileDownloadRequest(FileDownloadRequest {
            path: "/home/user/large-file.bin".to_string(),
            offset: 1024,
            chunk_size: 65536,
        }));
    }

    #[test]
    fn test_file_download_chunk_roundtrip() {
        roundtrip_envelope(Message::FileDownloadChunk(FileDownloadChunk {
            path: "/home/user/file.txt".to_string(),
            offset: 0,
            total_size: 100,
            data: b"Hello, World!".to_vec(),
            is_last: false,
        }));
    }

    #[test]
    fn test_file_upload_start_roundtrip() {
        roundtrip_envelope(Message::FileUploadStart(FileUploadStart {
            path: "/home/user/upload.bin".to_string(),
            size: 1048576,
            mode: 0o644,
            overwrite: true,
        }));
    }

    #[test]
    fn test_file_upload_chunk_roundtrip() {
        roundtrip_envelope(Message::FileUploadChunk(FileUploadChunk {
            path: "/home/user/upload.bin".to_string(),
            offset: 65536,
            data: vec![0u8; 1024],
        }));
    }

    #[test]
    fn test_file_upload_complete_roundtrip() {
        roundtrip_envelope(Message::FileUploadComplete(FileUploadComplete {
            path: "/home/user/upload.bin".to_string(),
            checksum: vec![0xab, 0xcd, 0xef, 0x12, 0x34, 0x56, 0x78, 0x90],
        }));
    }

    // Device message roundtrip tests

    #[test]
    fn test_device_info_roundtrip() {
        roundtrip_envelope(Message::DeviceInfo(DeviceInfo {
            device_id: "abc123def456".to_string(),
            name: "My Laptop".to_string(),
            os: "Linux".to_string(),
            os_version: "6.1.0".to_string(),
            arch: "x86_64".to_string(),
            protocol_version: PROTOCOL_VERSION,
        }));
    }

    #[test]
    fn test_device_approval_request_roundtrip() {
        roundtrip_envelope(Message::DeviceApprovalRequest(DeviceApprovalRequest {
            device_id: "new-device-123".to_string(),
            name: "Work Computer".to_string(),
            public_key: vec![0x04; 32],
            reason: Some("Need to access project files".to_string()),
        }));
    }

    #[test]
    fn test_device_approved_roundtrip() {
        roundtrip_envelope(Message::DeviceApproved(DeviceApproved {
            device_id: "new-device-123".to_string(),
            expires_at: Some(1735689600),
            allowed_capabilities: vec!["shell".to_string(), "file-read".to_string()],
        }));
    }

    #[test]
    fn test_device_rejected_roundtrip() {
        roundtrip_envelope(Message::DeviceRejected(DeviceRejected {
            device_id: "suspicious-device".to_string(),
            reason: "Device not recognized".to_string(),
            retry_allowed: false,
        }));
    }

    // Control message roundtrip tests

    #[test]
    fn test_ping_roundtrip() {
        roundtrip_envelope(Message::Ping(Ping {
            timestamp: 1704067200000,
            payload: b"ping!".to_vec(),
        }));
    }

    #[test]
    fn test_pong_roundtrip() {
        roundtrip_envelope(Message::Pong(Pong {
            timestamp: 1704067200000,
            payload: b"ping!".to_vec(),
        }));
    }

    #[test]
    fn test_error_roundtrip() {
        roundtrip_envelope(Message::Error(ErrorMessage {
            code: ErrorCode::NotFound,
            message: "Session not found".to_string(),
            context: Some("sess-unknown".to_string()),
            recoverable: false,
        }));
    }

    #[test]
    fn test_capabilities_roundtrip() {
        roundtrip_envelope(Message::Capabilities(Capabilities::default()));
    }

    #[test]
    fn test_capabilities_custom_roundtrip() {
        roundtrip_envelope(Message::Capabilities(Capabilities {
            protocol_versions: vec![1, 2],
            features: vec!["shell".to_string(), "files".to_string(), "tunnels".to_string()],
            max_message_size: 2 * 1024 * 1024,
            max_sessions: 32,
            compression: vec!["lz4".to_string(), "zstd".to_string()],
        }));
    }

    // Error code tests

    #[test]
    fn test_all_error_codes_roundtrip() {
        let codes = [
            ErrorCode::Unknown,
            ErrorCode::Unauthorized,
            ErrorCode::NotFound,
            ErrorCode::InvalidRequest,
            ErrorCode::InternalError,
            ErrorCode::Timeout,
            ErrorCode::RateLimited,
            ErrorCode::AlreadyExists,
            ErrorCode::PermissionDenied,
            ErrorCode::VersionMismatch,
        ];

        for code in codes {
            roundtrip_envelope(Message::Error(ErrorMessage {
                code,
                message: format!("Test error: {:?}", code),
                context: None,
                recoverable: true,
            }));
        }
    }

    // Binary size tests

    #[test]
    fn test_typical_message_size() {
        let envelope = Envelope::new(1, Message::SessionData(SessionData {
            session_id: "sess-12345678".to_string(),
            stream: DataStream::Stdout,
            data: b"Hello, World!\n".to_vec(),
        }));

        let bytes = envelope.to_msgpack().unwrap();
        // Typical messages should be well under 1KB
        assert!(bytes.len() < 1024, "Message too large: {} bytes", bytes.len());
    }

    #[test]
    fn test_ping_message_compact() {
        let envelope = Envelope::new(1, Message::Ping(Ping {
            timestamp: u64::MAX,
            payload: vec![],
        }));

        let bytes = envelope.to_msgpack().unwrap();
        // Ping should be very compact, under 100 bytes
        assert!(bytes.len() < 100, "Ping message too large: {} bytes", bytes.len());
    }

    #[test]
    fn test_session_create_size() {
        let envelope = Envelope::new(1, Message::SessionCreate(SessionCreate::default()));
        let bytes = envelope.to_msgpack().unwrap();
        // Default session create should be under 100 bytes
        assert!(bytes.len() < 100, "SessionCreate too large: {} bytes", bytes.len());
    }

    // Edge case tests

    #[test]
    fn test_empty_data() {
        roundtrip_envelope(Message::SessionData(SessionData {
            session_id: "s".to_string(),
            stream: DataStream::Stdin,
            data: vec![],
        }));
    }

    #[test]
    fn test_large_data() {
        // Test with 64KB of data
        roundtrip_envelope(Message::SessionData(SessionData {
            session_id: "sess-large".to_string(),
            stream: DataStream::Stdout,
            data: vec![0xAB; 65536],
        }));
    }

    #[test]
    fn test_unicode_strings() {
        roundtrip_envelope(Message::SessionCreate(SessionCreate {
            cols: 80,
            rows: 24,
            shell: Some("/bin/bash".to_string()),
            env: vec![
                ("LANG".to_string(), "ja_JP.UTF-8".to_string()),
                ("GREETING".to_string(), "Hello!".to_string()),
            ],
            cwd: Some("/home/user/documents".to_string()),
        }));
    }

    #[test]
    fn test_special_characters_in_path() {
        roundtrip_envelope(Message::FileListRequest(FileListRequest {
            path: "/home/user/My Documents/file (1).txt".to_string(),
            include_hidden: false,
        }));
    }

    #[test]
    fn test_binary_data_in_checksum() {
        roundtrip_envelope(Message::FileUploadComplete(FileUploadComplete {
            path: "/tmp/test".to_string(),
            checksum: (0u8..=255).collect(),
        }));
    }
}
