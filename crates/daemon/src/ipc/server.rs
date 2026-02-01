//! IPC Server for handling CLI connections via Unix Domain Sockets.
//!
//! The server listens on a Unix socket and accepts connections from CLI clients,
//! allowing them to send commands and receive responses.

use std::io;
use std::path::Path;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use super::messages::{IpcRequest, IpcResponse};

/// A server that listens for IPC connections on a Unix Domain Socket.
pub struct IpcServer {
    listener: UnixListener,
}

impl IpcServer {
    /// Bind the server to the specified socket path.
    ///
    /// This will create the socket file and any necessary parent directories.
    /// If a socket file already exists at the path, it will be removed first.
    ///
    /// # Arguments
    ///
    /// * `path` - The path where the Unix socket should be created.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The parent directories cannot be created
    /// - The existing socket cannot be removed
    /// - The socket cannot be bound
    pub async fn bind(path: &Path) -> Result<Self, io::Error> {
        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }

        // Remove existing socket file if present
        if path.exists() {
            std::fs::remove_file(path)?;
        }

        let listener = UnixListener::bind(path)?;

        Ok(Self { listener })
    }

    /// Accept a new incoming connection.
    ///
    /// This method blocks until a client connects.
    ///
    /// # Returns
    ///
    /// An `IpcConnection` representing the connected client.
    pub async fn accept(&self) -> Result<IpcConnection, io::Error> {
        let (stream, _addr) = self.listener.accept().await?;
        Ok(IpcConnection::new(stream))
    }
}

/// A connection to an IPC client.
///
/// This struct wraps a Unix stream and provides methods for reading
/// requests and sending responses using JSON newline-delimited protocol.
pub struct IpcConnection {
    reader: BufReader<tokio::io::ReadHalf<UnixStream>>,
    writer: tokio::io::WriteHalf<UnixStream>,
}

impl IpcConnection {
    /// Create a new IPC connection from a Unix stream.
    fn new(stream: UnixStream) -> Self {
        let (read_half, write_half) = tokio::io::split(stream);
        Self {
            reader: BufReader::new(read_half),
            writer: write_half,
        }
    }

    /// Read the next request from the client.
    ///
    /// Returns `None` if the client has disconnected.
    ///
    /// # Errors
    ///
    /// Returns an error if the message cannot be read or parsed.
    pub async fn read_request(&mut self) -> Result<Option<IpcRequest>, IpcError> {
        let mut line = String::new();
        let bytes_read = self
            .reader
            .read_line(&mut line)
            .await
            .map_err(IpcError::Io)?;

        if bytes_read == 0 {
            return Ok(None);
        }

        let request = serde_json::from_str(line.trim()).map_err(IpcError::Json)?;
        Ok(Some(request))
    }

    /// Send a response to the client.
    ///
    /// # Errors
    ///
    /// Returns an error if the response cannot be serialized or sent.
    pub async fn send_response(&mut self, response: &IpcResponse) -> Result<(), IpcError> {
        let mut json = serde_json::to_string(response).map_err(IpcError::Json)?;
        json.push('\n');

        self.writer
            .write_all(json.as_bytes())
            .await
            .map_err(IpcError::Io)?;
        self.writer.flush().await.map_err(IpcError::Io)?;

        Ok(())
    }
}

/// Errors that can occur during IPC communication.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// A JSON serialization/deserialization error occurred.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_server_bind_creates_parent_dirs() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("nested").join("dir").join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();
        assert!(socket_path.exists());
        drop(server);
    }

    #[tokio::test]
    async fn test_server_bind_removes_existing_socket() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        // Create first server
        let _server1 = IpcServer::bind(&socket_path).await.unwrap();
        drop(_server1);

        // Create second server at same path - should succeed
        let _server2 = IpcServer::bind(&socket_path).await.unwrap();
        assert!(socket_path.exists());
    }

    #[tokio::test]
    async fn test_server_accept_and_communicate() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        // Spawn a client task
        let socket_path_clone = socket_path.clone();
        let client_handle = tokio::spawn(async move {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            use tokio::net::UnixStream;

            let stream = UnixStream::connect(&socket_path_clone).await.unwrap();
            let (read_half, mut write_half) = tokio::io::split(stream);
            let mut reader = BufReader::new(read_half);

            // Send a ping request
            let request = serde_json::to_string(&IpcRequest::Ping).unwrap();
            write_half
                .write_all(format!("{}\n", request).as_bytes())
                .await
                .unwrap();
            write_half.flush().await.unwrap();

            // Read response
            let mut response_line = String::new();
            reader.read_line(&mut response_line).await.unwrap();
            let response: IpcResponse = serde_json::from_str(response_line.trim()).unwrap();

            response
        });

        // Accept connection and handle request
        let mut conn = server.accept().await.unwrap();
        let request = conn.read_request().await.unwrap().unwrap();
        assert_eq!(request, IpcRequest::Ping);

        conn.send_response(&IpcResponse::Pong).await.unwrap();

        // Verify client received correct response
        let response = client_handle.await.unwrap();
        assert_eq!(response, IpcResponse::Pong);
    }

    #[tokio::test]
    async fn test_connection_read_returns_none_on_disconnect() {
        let temp_dir = tempdir().unwrap();
        let socket_path = temp_dir.path().join("test.sock");

        let server = IpcServer::bind(&socket_path).await.unwrap();

        // Spawn a client that connects and immediately disconnects
        let socket_path_clone = socket_path.clone();
        tokio::spawn(async move {
            let _stream = UnixStream::connect(&socket_path_clone).await.unwrap();
            // Stream drops immediately
        });

        let mut conn = server.accept().await.unwrap();

        // Give the client time to disconnect
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let request = conn.read_request().await.unwrap();
        assert!(request.is_none());
    }
}
