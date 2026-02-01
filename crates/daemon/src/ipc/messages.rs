//! IPC message types for CLI-daemon communication.
//!
//! This module defines the request and response types used for communication
//! between the CLI and the daemon over Unix Domain Sockets.

use serde::{Deserialize, Serialize};

/// Requests that can be sent from the CLI to the daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IpcRequest {
    /// Check if the daemon is alive.
    Ping,
    /// Get the current status of the daemon.
    Status,
    /// Request the daemon to stop gracefully.
    Stop,
    /// List all active sessions.
    ListSessions,
    /// Kill a specific session by ID.
    KillSession {
        /// The unique identifier of the session to kill.
        session_id: String,
        /// Signal to send to the session (default: SIGTERM/15).
        /// Common values: 1 (SIGHUP), 9 (SIGKILL), 15 (SIGTERM).
        signal: Option<i32>,
    },
}

/// Responses sent from the daemon to the CLI.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IpcResponse {
    /// Response to a Ping request.
    Pong,
    /// Current daemon status.
    Status {
        /// Whether the daemon is running.
        running: bool,
        /// Uptime in seconds.
        uptime_secs: u64,
        /// Number of active sessions.
        session_count: usize,
        /// Number of connected devices.
        device_count: usize,
    },
    /// Acknowledgment that the daemon is stopping.
    Stopping,
    /// List of active sessions.
    Sessions {
        /// Information about each active session.
        sessions: Vec<IpcSessionInfo>,
    },
    /// Confirmation that a session was killed.
    SessionKilled {
        /// The ID of the killed session.
        session_id: String,
    },
    /// An error occurred processing the request.
    Error {
        /// Human-readable error message.
        message: String,
    },
}

/// Information about an active session for IPC communication.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcSessionInfo {
    /// Unique session identifier.
    pub id: String,
    /// Unix timestamp when the session was connected.
    pub connected_at: u64,
    /// Optional peer identifier (device ID or connection ID).
    pub peer_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_ping_serialization() {
        let request = IpcRequest::Ping;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#""Ping""#);

        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_request_status_serialization() {
        let request = IpcRequest::Status;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#""Status""#);

        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_request_stop_serialization() {
        let request = IpcRequest::Stop;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#""Stop""#);

        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_request_list_sessions_serialization() {
        let request = IpcRequest::ListSessions;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#""ListSessions""#);

        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_request_kill_session_serialization() {
        let request = IpcRequest::KillSession {
            session_id: "test-session-123".to_string(),
            signal: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("KillSession"));
        assert!(json.contains("test-session-123"));

        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_request_kill_session_with_signal_serialization() {
        let request = IpcRequest::KillSession {
            session_id: "test-session-456".to_string(),
            signal: Some(9),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("KillSession"));
        assert!(json.contains("test-session-456"));
        assert!(json.contains("9"));

        let deserialized: IpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_response_pong_serialization() {
        let response = IpcResponse::Pong;
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#""Pong""#);

        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_response_status_serialization() {
        let response = IpcResponse::Status {
            running: true,
            uptime_secs: 3600,
            session_count: 2,
            device_count: 3,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Status"));
        assert!(json.contains("3600"));
        assert!(json.contains("true"));

        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_response_stopping_serialization() {
        let response = IpcResponse::Stopping;
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#""Stopping""#);

        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_response_sessions_serialization() {
        let response = IpcResponse::Sessions {
            sessions: vec![
                IpcSessionInfo {
                    id: "session-1".to_string(),
                    connected_at: 1700000000,
                    peer_id: Some("peer-abc".to_string()),
                },
                IpcSessionInfo {
                    id: "session-2".to_string(),
                    connected_at: 1700000100,
                    peer_id: None,
                },
            ],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("session-1"));
        assert!(json.contains("session-2"));
        assert!(json.contains("peer-abc"));

        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_response_session_killed_serialization() {
        let response = IpcResponse::SessionKilled {
            session_id: "killed-session".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("killed-session"));

        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_response_error_serialization() {
        let response = IpcResponse::Error {
            message: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Error"));
        assert!(json.contains("Something went wrong"));

        let deserialized: IpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_ipc_session_info_serialization() {
        let session = IpcSessionInfo {
            id: "test-id".to_string(),
            connected_at: 1234567890,
            peer_id: Some("peer-123".to_string()),
        };
        let json = serde_json::to_string(&session).unwrap();

        let deserialized: IpcSessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, session);
    }

    #[test]
    fn test_ipc_session_info_without_peer_id() {
        let session = IpcSessionInfo {
            id: "test-id".to_string(),
            connected_at: 1234567890,
            peer_id: None,
        };
        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("null"));

        let deserialized: IpcSessionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, session);
    }
}
