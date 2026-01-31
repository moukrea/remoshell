//! Error types for the protocol crate.

use thiserror::Error;

/// Protocol error type covering all possible failure modes.
#[derive(Debug, Error)]
pub enum ProtocolError {
    // Serialization errors
    /// Failed to serialize data.
    #[error("serialization failed: {0}")]
    Serialization(String),

    /// Failed to deserialize data.
    #[error("deserialization failed: {0}")]
    Deserialization(String),

    // Cryptographic errors
    /// Encryption operation failed.
    #[error("encryption failed: {0}")]
    Encryption(String),

    /// Decryption operation failed.
    #[error("decryption failed: {0}")]
    Decryption(String),

    /// Signature verification failed.
    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    /// Invalid or malformed public key.
    #[error("invalid public key: {0}")]
    InvalidPublicKey(String),

    // Handshake errors
    /// Noise protocol handshake failed.
    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    /// Attempted to use transport before handshake completion.
    #[error("handshake incomplete: cannot perform operation before handshake is finished")]
    HandshakeIncomplete,

    // Frame errors
    /// Frame exceeds maximum allowed size.
    #[error("frame too large: {size} bytes exceeds maximum of {max} bytes")]
    FrameTooLarge {
        /// Actual frame size.
        size: usize,
        /// Maximum allowed size.
        max: usize,
    },

    /// Frame has invalid magic bytes.
    #[error("invalid frame magic: expected {expected:#06x}, got {got:#06x}")]
    InvalidFrameMagic {
        /// Expected magic value.
        expected: u16,
        /// Actual magic value received.
        got: u16,
    },

    // Connection errors
    /// Connection was closed unexpectedly.
    #[error("connection closed: {0}")]
    ConnectionClosed(String),

    /// Operation timed out.
    #[error("operation timed out: {0}")]
    Timeout(String),

    // Device trust errors
    /// Device is not in the trust store.
    #[error("device not trusted: {device_id}")]
    DeviceNotTrusted {
        /// The untrusted device identifier.
        device_id: String,
    },

    /// Device has been revoked from the trust store.
    #[error("device revoked: {device_id}")]
    DeviceRevoked {
        /// The revoked device identifier.
        device_id: String,
    },

    // Session errors
    /// Session with the given ID was not found.
    #[error("session not found: {session_id}")]
    SessionNotFound {
        /// The missing session identifier.
        session_id: String,
    },

    /// File or data transfer failed.
    #[error("transfer failed: {0}")]
    TransferFailed(String),
}

/// Result type alias for protocol operations.
pub type Result<T> = std::result::Result<T, ProtocolError>;

// Conversions from underlying crate errors

impl From<serde_json::Error> for ProtocolError {
    fn from(err: serde_json::Error) -> Self {
        if err.is_data() || err.is_eof() || err.is_syntax() {
            ProtocolError::Deserialization(err.to_string())
        } else {
            ProtocolError::Serialization(err.to_string())
        }
    }
}

impl From<rmp_serde::encode::Error> for ProtocolError {
    fn from(err: rmp_serde::encode::Error) -> Self {
        ProtocolError::Serialization(err.to_string())
    }
}

impl From<rmp_serde::decode::Error> for ProtocolError {
    fn from(err: rmp_serde::decode::Error) -> Self {
        ProtocolError::Deserialization(err.to_string())
    }
}

impl From<snow::Error> for ProtocolError {
    fn from(err: snow::Error) -> Self {
        let msg = err.to_string();
        // Map snow errors to more specific protocol errors
        if msg.contains("decrypt") {
            ProtocolError::Decryption(msg)
        } else if msg.contains("encrypt") {
            ProtocolError::Encryption(msg)
        } else if msg.contains("handshake") || msg.contains("state") {
            ProtocolError::HandshakeFailed(msg)
        } else {
            ProtocolError::Encryption(msg)
        }
    }
}

impl From<ed25519_dalek::SignatureError> for ProtocolError {
    fn from(err: ed25519_dalek::SignatureError) -> Self {
        ProtocolError::InvalidSignature(err.to_string())
    }
}

impl From<std::io::Error> for ProtocolError {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;
        match err.kind() {
            ErrorKind::TimedOut => ProtocolError::Timeout(err.to_string()),
            ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::UnexpectedEof => ProtocolError::ConnectionClosed(err.to_string()),
            _ => ProtocolError::TransferFailed(err.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization_error_display() {
        let err = ProtocolError::Serialization("invalid utf-8".to_string());
        assert_eq!(err.to_string(), "serialization failed: invalid utf-8");
    }

    #[test]
    fn test_deserialization_error_display() {
        let err = ProtocolError::Deserialization("unexpected end of input".to_string());
        assert_eq!(
            err.to_string(),
            "deserialization failed: unexpected end of input"
        );
    }

    #[test]
    fn test_encryption_error_display() {
        let err = ProtocolError::Encryption("key derivation failed".to_string());
        assert_eq!(err.to_string(), "encryption failed: key derivation failed");
    }

    #[test]
    fn test_decryption_error_display() {
        let err = ProtocolError::Decryption("authentication tag mismatch".to_string());
        assert_eq!(
            err.to_string(),
            "decryption failed: authentication tag mismatch"
        );
    }

    #[test]
    fn test_invalid_signature_error_display() {
        let err = ProtocolError::InvalidSignature("signature verification failed".to_string());
        assert_eq!(
            err.to_string(),
            "invalid signature: signature verification failed"
        );
    }

    #[test]
    fn test_invalid_public_key_error_display() {
        let err = ProtocolError::InvalidPublicKey("wrong key length".to_string());
        assert_eq!(err.to_string(), "invalid public key: wrong key length");
    }

    #[test]
    fn test_handshake_failed_error_display() {
        let err = ProtocolError::HandshakeFailed("pattern mismatch".to_string());
        assert_eq!(err.to_string(), "handshake failed: pattern mismatch");
    }

    #[test]
    fn test_handshake_incomplete_error_display() {
        let err = ProtocolError::HandshakeIncomplete;
        assert_eq!(
            err.to_string(),
            "handshake incomplete: cannot perform operation before handshake is finished"
        );
    }

    #[test]
    fn test_frame_too_large_error_display() {
        let err = ProtocolError::FrameTooLarge {
            size: 100_000,
            max: 65536,
        };
        assert_eq!(
            err.to_string(),
            "frame too large: 100000 bytes exceeds maximum of 65536 bytes"
        );
    }

    #[test]
    fn test_invalid_frame_magic_error_display() {
        let err = ProtocolError::InvalidFrameMagic {
            expected: 0xBEEF,
            got: 0xDEAD,
        };
        assert_eq!(
            err.to_string(),
            "invalid frame magic: expected 0xbeef, got 0xdead"
        );
    }

    #[test]
    fn test_connection_closed_error_display() {
        let err = ProtocolError::ConnectionClosed("peer disconnected".to_string());
        assert_eq!(err.to_string(), "connection closed: peer disconnected");
    }

    #[test]
    fn test_timeout_error_display() {
        let err = ProtocolError::Timeout("operation exceeded 30s limit".to_string());
        assert_eq!(
            err.to_string(),
            "operation timed out: operation exceeded 30s limit"
        );
    }

    #[test]
    fn test_device_not_trusted_error_display() {
        let err = ProtocolError::DeviceNotTrusted {
            device_id: "abc123".to_string(),
        };
        assert_eq!(err.to_string(), "device not trusted: abc123");
    }

    #[test]
    fn test_device_revoked_error_display() {
        let err = ProtocolError::DeviceRevoked {
            device_id: "xyz789".to_string(),
        };
        assert_eq!(err.to_string(), "device revoked: xyz789");
    }

    #[test]
    fn test_session_not_found_error_display() {
        let err = ProtocolError::SessionNotFound {
            session_id: "session-42".to_string(),
        };
        assert_eq!(err.to_string(), "session not found: session-42");
    }

    #[test]
    fn test_transfer_failed_error_display() {
        let err = ProtocolError::TransferFailed("checksum mismatch".to_string());
        assert_eq!(err.to_string(), "transfer failed: checksum mismatch");
    }

    #[test]
    fn test_from_serde_json_error() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let protocol_err: ProtocolError = json_err.into();
        assert!(matches!(protocol_err, ProtocolError::Deserialization(_)));
    }

    #[test]
    fn test_from_rmp_serde_decode_error() {
        // Use invalid msgpack data that will fail to decode as a struct
        #[derive(Debug, serde::Deserialize)]
        #[allow(dead_code)]
        struct TestStruct {
            field: String,
        }
        let msgpack_err = rmp_serde::from_slice::<TestStruct>(&[0x00]).unwrap_err();
        let protocol_err: ProtocolError = msgpack_err.into();
        assert!(matches!(protocol_err, ProtocolError::Deserialization(_)));
    }

    #[test]
    fn test_from_io_error_timeout() {
        let io_err = std::io::Error::new(std::io::ErrorKind::TimedOut, "timed out");
        let protocol_err: ProtocolError = io_err.into();
        assert!(matches!(protocol_err, ProtocolError::Timeout(_)));
    }

    #[test]
    fn test_from_io_error_connection_closed() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        let protocol_err: ProtocolError = io_err.into();
        assert!(matches!(protocol_err, ProtocolError::ConnectionClosed(_)));
    }

    #[test]
    fn test_from_io_error_other() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let protocol_err: ProtocolError = io_err.into();
        assert!(matches!(protocol_err, ProtocolError::TransferFailed(_)));
    }

    #[test]
    fn test_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ProtocolError>();
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_result() -> Result<()> {
            Ok(())
        }
        assert!(returns_result().is_ok());
    }
}
