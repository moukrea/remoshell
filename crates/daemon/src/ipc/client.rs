//! IPC Client for communicating with the daemon via Unix Domain Sockets.
//!
//! The client connects to the daemon's Unix socket and sends commands,
//! receiving responses in a request-response pattern.

use std::io;
use std::path::Path;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use super::messages::{IpcRequest, IpcResponse};
use super::server::IpcError;

/// Default timeout for client operations in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 5;

/// A client for communicating with the daemon via IPC.
pub struct IpcClient {
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    writer: tokio::io::WriteHalf<UnixStream>,
    timeout: Duration,
}

impl IpcClient {
    /// Connect to the daemon at the specified socket path.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the Unix socket.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established.
    /// This typically indicates that the daemon is not running.
    pub async fn connect(path: &Path) -> Result<Self, IpcError> {
        let stream = UnixStream::connect(path).await.map_err(IpcError::Io)?;
        let (read_half, write_half) = tokio::io::split(stream);

        Ok(Self {
            reader: BufReader::new(read_half),
            writer: write_half,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        })
    }

    /// Connect to the daemon with a custom timeout.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the Unix socket.
    /// * `timeout` - The timeout duration for operations.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection cannot be established.
    pub async fn connect_with_timeout(path: &Path, timeout: Duration) -> Result<Self, IpcError> {
        let connect_future = UnixStream::connect(path);
        let stream = tokio::time::timeout(timeout, connect_future)
            .await
            .map_err(|_| {
                IpcError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "connection timed out",
                ))
            })?
            .map_err(IpcError::Io)?;

        let (read_half, write_half) = tokio::io::split(stream);

        Ok(Self {
            reader: BufReader::new(read_half),
            writer: write_half,
            timeout,
        })
    }

    /// Set the timeout for operations.
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Send a request to the daemon and wait for a response.
    ///
    /// # Arguments
    ///
    /// * `request` - The request to send.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The request cannot be serialized
    /// - The request cannot be sent
    /// - The response cannot be read
    /// - The response cannot be parsed
    /// - The operation times out
    pub async fn send(&mut self, request: IpcRequest) -> Result<IpcResponse, IpcError> {
        tokio::time::timeout(self.timeout, self.send_internal(request))
            .await
            .map_err(|_| {
                IpcError::Io(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "operation timed out",
                ))
            })?
    }

    /// Internal send implementation without timeout.
    async fn send_internal(&mut self, request: IpcRequest) -> Result<IpcResponse, IpcError> {
        // Serialize and send request
        let mut json = serde_json::to_string(&request).map_err(IpcError::Json)?;
        json.push('\n');

        self.writer
            .write_all(json.as_bytes())
            .await
            .map_err(IpcError::Io)?;
        self.writer.flush().await.map_err(IpcError::Io)?;

        // Read response
        let mut line = String::new();
        let bytes_read = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(IpcError::Io)?;

        if bytes_read == 0 {
            return Err(IpcError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "daemon closed connection",
            )));
        }

        let response = serde_json::from_str(line.trim()).map_err(IpcError::Json)?;
        Ok(response)
    }

    /// Send a ping request to check if the daemon is responsive.
    ///
    /// # Returns
    ///
    /// Returns `true` if the daemon responds with a Pong, `false` otherwise.
    pub async fn ping(&mut self) -> Result<bool, IpcError> {
        let response = self.send(IpcRequest::Ping).await?;
        Ok(matches!(response, IpcResponse::Pong))
    }

    /// Get the current status of the daemon.
    pub async fn status(&mut self) -> Result<IpcResponse, IpcError> {
        self.send(IpcRequest::Status).await
    }

    /// Request the daemon to stop.
    pub async fn stop(&mut self) -> Result<IpcResponse, IpcError> {
        self.send(IpcRequest::Stop).await
    }

    /// List all active sessions.
    pub async fn list_sessions(&mut self) -> Result<IpcResponse, IpcError> {
        self.send(IpcRequest::ListSessions).await
    }

    /// Kill a specific session by ID.
    pub async fn kill_session(&mut self, session_id: String) -> Result<IpcResponse, IpcError> {
        self.send(IpcRequest::KillSession { session_id }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::server::IpcServer;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_client_connect_fails_when_daemon_not_running() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("nonexistent.sock");

        let result = IpcClient::connect(&socket_path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_client_ping() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        // Spawn server handler
        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let request = conn.read_request().await.unwrap().unwrap();
            assert_eq!(request, IpcRequest::Ping);
            conn.send_response(&IpcResponse::Pong).await.unwrap();
        });

        // Give server time to start listening
        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();
        let result = client.ping().await.unwrap();
        assert!(result);

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_status() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let request = conn.read_request().await.unwrap().unwrap();
            assert_eq!(request, IpcRequest::Status);
            conn.send_response(&IpcResponse::Status {
                running: true,
                uptime_secs: 100,
                session_count: 2,
            })
            .await
            .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();
        let response = client.status().await.unwrap();

        match response {
            IpcResponse::Status {
                running,
                uptime_secs,
                session_count,
            } => {
                assert!(running);
                assert_eq!(uptime_secs, 100);
                assert_eq!(session_count, 2);
            }
            _ => panic!("Expected Status response"),
        }

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_stop() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let request = conn.read_request().await.unwrap().unwrap();
            assert_eq!(request, IpcRequest::Stop);
            conn.send_response(&IpcResponse::Stopping).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();
        let response = client.stop().await.unwrap();
        assert_eq!(response, IpcResponse::Stopping);

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_list_sessions() {
        use crate::ipc::messages::IpcSessionInfo;

        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let request = conn.read_request().await.unwrap().unwrap();
            assert_eq!(request, IpcRequest::ListSessions);
            conn.send_response(&IpcResponse::Sessions {
                sessions: vec![IpcSessionInfo {
                    id: "sess-1".to_string(),
                    connected_at: 1000,
                    peer_id: Some("peer-1".to_string()),
                }],
            })
            .await
            .unwrap();
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();
        let response = client.list_sessions().await.unwrap();

        match response {
            IpcResponse::Sessions { sessions } => {
                assert_eq!(sessions.len(), 1);
                assert_eq!(sessions[0].id, "sess-1");
            }
            _ => panic!("Expected Sessions response"),
        }

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_kill_session() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        let server_handle = tokio::spawn(async move {
            let mut conn = server.accept().await.unwrap();
            let request = conn.read_request().await.unwrap().unwrap();
            match request {
                IpcRequest::KillSession { session_id } => {
                    assert_eq!(session_id, "test-session");
                    conn.send_response(&IpcResponse::SessionKilled { session_id })
                        .await
                        .unwrap();
                }
                _ => panic!("Expected KillSession request"),
            }
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut client = IpcClient::connect(&socket_path).await.unwrap();
        let response = client.kill_session("test-session".to_string()).await.unwrap();

        match response {
            IpcResponse::SessionKilled { session_id } => {
                assert_eq!(session_id, "test-session");
            }
            _ => panic!("Expected SessionKilled response"),
        }

        server_handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_client_timeout() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        // Server that never responds
        let _server_handle = tokio::spawn(async move {
            let _conn = server.accept().await.unwrap();
            // Don't read or respond, just sleep
            tokio::time::sleep(Duration::from_secs(10)).await;
        });

        tokio::time::sleep(Duration::from_millis(10)).await;

        let mut client =
            IpcClient::connect_with_timeout(&socket_path, Duration::from_millis(100))
                .await
                .unwrap();

        let result = client.ping().await;
        assert!(result.is_err());
    }
}
