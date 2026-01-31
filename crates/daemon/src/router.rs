//! Message router for dispatching incoming messages to appropriate handlers.
//!
//! This module provides the `MessageRouter` struct that receives protocol messages
//! and routes them to the appropriate subsystem (session manager, file manager,
//! device manager) based on message type.

use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use protocol::messages::{
    DataStream, DeviceApprovalRequest, DeviceApproved, DeviceInfo, DeviceRejected, ErrorCode,
    ErrorMessage, FileDownloadChunk, FileDownloadRequest, FileListRequest, FileListResponse,
    FileUploadChunk, FileUploadComplete, FileUploadStart, Message, Ping, Pong, SessionAttach,
    SessionClosed, SessionCreate, SessionCreated, SessionData, SessionDetach, SessionKill,
    SessionResize,
};
use protocol::DeviceId;
use tracing::{debug, info, warn};

use crate::devices::{TrustLevel, TrustStore};
use crate::files::{DirectoryBrowser, FileTransfer};
use crate::session::{SessionError, SessionId, SessionManager, SessionStatus};

/// Result type for router operations.
pub type RouterResult = Result<Option<Message>, RouterError>;

/// Errors that can occur during message routing.
#[derive(Debug, thiserror::Error)]
pub enum RouterError {
    /// Session-related error.
    #[error("session error: {0}")]
    Session(#[from] SessionError),

    /// File operation error.
    #[error("file error: {0}")]
    File(String),

    /// Device/trust error.
    #[error("device error: {0}")]
    Device(String),

    /// Invalid request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// Internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

impl RouterError {
    /// Convert the error to a protocol ErrorMessage.
    pub fn to_error_message(&self, context: Option<String>) -> ErrorMessage {
        let (code, recoverable) = match self {
            RouterError::Session(e) => match e {
                SessionError::NotFound(_) => (ErrorCode::NotFound, false),
                SessionError::AlreadyTerminated(_) => (ErrorCode::InvalidRequest, false),
                SessionError::SpawnFailed(_) => (ErrorCode::InternalError, true),
                SessionError::WriteFailed(_) => (ErrorCode::InternalError, true),
                SessionError::ReadFailed(_) => (ErrorCode::InternalError, true),
                SessionError::ResizeFailed(_) => (ErrorCode::InternalError, true),
                SessionError::KillFailed(_) => (ErrorCode::InternalError, true),
                SessionError::Io(_) => (ErrorCode::InternalError, true),
            },
            RouterError::File(_) => (ErrorCode::InternalError, true),
            RouterError::Device(_) => (ErrorCode::Unauthorized, false),
            RouterError::InvalidRequest(_) => (ErrorCode::InvalidRequest, false),
            RouterError::Internal(_) => (ErrorCode::InternalError, true),
        };

        ErrorMessage {
            code,
            message: self.to_string(),
            context,
            recoverable,
        }
    }
}

/// Message router that dispatches messages to appropriate handlers.
///
/// The router holds references to the session manager, file transfer handler,
/// and device trust store. It receives incoming messages and routes them to
/// the appropriate handler based on message type.
pub struct MessageRouter<S: SessionManager> {
    /// Session manager for PTY operations.
    session_manager: Arc<S>,
    /// File transfer handler for file operations.
    file_transfer: Arc<FileTransfer>,
    /// Directory browser for listing directories.
    directory_browser: Arc<DirectoryBrowser>,
    /// Trust store for device management.
    trust_store: Arc<TrustStore>,
}

impl<S: SessionManager> MessageRouter<S> {
    /// Create a new message router with the given dependencies.
    pub fn new(
        session_manager: Arc<S>,
        file_transfer: Arc<FileTransfer>,
        directory_browser: Arc<DirectoryBrowser>,
        trust_store: Arc<TrustStore>,
    ) -> Self {
        Self {
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
        }
    }

    /// Route a message to the appropriate handler.
    ///
    /// Returns `Ok(Some(response))` if a response should be sent back,
    /// `Ok(None)` if no response is needed, or `Err(error)` if routing failed.
    pub async fn route(&self, message: Message) -> RouterResult {
        debug!(?message, "Routing message");

        match message {
            // Session messages
            Message::SessionCreate(req) => self.handle_session_create(req).await,
            Message::SessionAttach(req) => self.handle_session_attach(req).await,
            Message::SessionDetach(req) => self.handle_session_detach(req).await,
            Message::SessionKill(req) => self.handle_session_kill(req).await,
            Message::SessionResize(req) => self.handle_session_resize(req).await,
            Message::SessionData(data) => self.handle_session_data(data).await,
            Message::SessionCreated(_) | Message::SessionClosed(_) => {
                // These are response messages, not requests - ignore them
                debug!("Ignoring response message received as request");
                Ok(None)
            }

            // File messages
            Message::FileListRequest(req) => self.handle_file_list(req).await,
            Message::FileDownloadRequest(req) => self.handle_file_download(req).await,
            Message::FileUploadStart(req) => self.handle_file_upload_start(req).await,
            Message::FileUploadChunk(req) => self.handle_file_upload_chunk(req).await,
            Message::FileUploadComplete(req) => self.handle_file_upload_complete(req).await,
            Message::FileListResponse(_) | Message::FileDownloadChunk(_) => {
                // These are response messages, not requests - ignore them
                debug!("Ignoring response message received as request");
                Ok(None)
            }

            // Device messages
            Message::DeviceInfo(info) => self.handle_device_info(info).await,
            Message::DeviceApprovalRequest(req) => self.handle_device_approval_request(req).await,
            Message::DeviceApproved(_) | Message::DeviceRejected(_) => {
                // These are response messages, not requests - ignore them
                debug!("Ignoring response message received as request");
                Ok(None)
            }

            // Control messages
            Message::Ping(ping) => self.handle_ping(ping).await,
            Message::Pong(_) => {
                // Pong is a response to our ping, just log it
                debug!("Received pong");
                Ok(None)
            }
            Message::Error(err) => {
                // Log errors received from peer
                warn!(?err, "Received error from peer");
                Ok(None)
            }
            Message::Capabilities(_) => {
                // Capabilities are handled during connection setup
                debug!("Received capabilities message");
                Ok(None)
            }
        }
    }

    // =========================================================================
    // Session Handlers
    // =========================================================================

    async fn handle_session_create(&self, req: SessionCreate) -> RouterResult {
        info!(
            cols = req.cols,
            rows = req.rows,
            shell = ?req.shell,
            "Creating new session"
        );

        let (session_id, pid) = self
            .session_manager
            .create(req.shell, req.cols, req.rows, req.env, req.cwd)
            .await?;

        info!(session_id = %session_id, pid = pid, "Session created");

        Ok(Some(Message::SessionCreated(SessionCreated {
            session_id: session_id.to_string(),
            pid,
        })))
    }

    async fn handle_session_attach(&self, req: SessionAttach) -> RouterResult {
        info!(session_id = %req.session_id, "Attaching to session");

        let session_id: SessionId = req.session_id.clone();
        let _rx = self.session_manager.attach(&session_id).await?;

        // Return None - actual data will be streamed via the receiver
        // The caller is responsible for handling the broadcast receiver
        Ok(None)
    }

    async fn handle_session_detach(&self, req: SessionDetach) -> RouterResult {
        info!(session_id = %req.session_id, "Detaching from session");

        let session_id: SessionId = req.session_id.clone();
        self.session_manager.detach(&session_id).await?;

        Ok(None)
    }

    async fn handle_session_kill(&self, req: SessionKill) -> RouterResult {
        info!(
            session_id = %req.session_id,
            signal = ?req.signal,
            "Killing session"
        );

        let session_id: SessionId = req.session_id.clone();
        let status = self.session_manager.kill(&session_id, req.signal).await?;

        // Convert SessionStatus enum to exit_code/signal
        let (exit_code, signal) = match status {
            SessionStatus::Running => (None, None),
            SessionStatus::Exited(code) => (Some(code), None),
            SessionStatus::Killed(sig) => (None, Some(sig)),
            SessionStatus::Terminated => (None, None),
        };

        Ok(Some(Message::SessionClosed(SessionClosed {
            session_id: req.session_id,
            exit_code,
            signal,
            reason: Some("Session killed by request".to_string()),
        })))
    }

    async fn handle_session_resize(&self, req: SessionResize) -> RouterResult {
        debug!(
            session_id = %req.session_id,
            cols = req.cols,
            rows = req.rows,
            "Resizing session"
        );

        let session_id: SessionId = req.session_id.clone();
        self.session_manager
            .resize(&session_id, req.cols, req.rows)
            .await?;

        Ok(None)
    }

    async fn handle_session_data(&self, data: SessionData) -> RouterResult {
        // Only handle stdin data (client -> daemon)
        if data.stream != DataStream::Stdin {
            return Err(RouterError::InvalidRequest(
                "Only stdin data is accepted from clients".to_string(),
            ));
        }

        let session_id: SessionId = data.session_id.clone();
        self.session_manager.write(&session_id, &data.data).await?;

        Ok(None)
    }

    // =========================================================================
    // File Handlers
    // =========================================================================

    async fn handle_file_list(&self, req: FileListRequest) -> RouterResult {
        debug!(path = %req.path, include_hidden = req.include_hidden, "Listing directory");

        let path = Path::new(&req.path);
        let entries = self
            .directory_browser
            .list_directory(path, req.include_hidden)
            .map_err(|e| RouterError::File(e.to_string()))?;

        let protocol_entries: Vec<_> = entries.iter().map(|e| e.to_protocol()).collect();

        Ok(Some(Message::FileListResponse(FileListResponse {
            path: req.path,
            entries: protocol_entries,
        })))
    }

    async fn handle_file_download(&self, req: FileDownloadRequest) -> RouterResult {
        debug!(
            path = %req.path,
            offset = req.offset,
            chunk_size = req.chunk_size,
            "Downloading file chunk"
        );

        let path = Path::new(&req.path);
        let (data, total_size, is_last) = self
            .file_transfer
            .download_chunk(path, req.offset, req.chunk_size)
            .map_err(|e| RouterError::File(e.to_string()))?;

        Ok(Some(Message::FileDownloadChunk(FileDownloadChunk {
            path: req.path,
            offset: req.offset,
            total_size,
            data,
            is_last,
        })))
    }

    async fn handle_file_upload_start(&self, req: FileUploadStart) -> RouterResult {
        debug!(
            path = %req.path,
            size = req.size,
            mode = format!("{:o}", req.mode),
            overwrite = req.overwrite,
            "Starting file upload"
        );

        let path = Path::new(&req.path);
        self.file_transfer
            .start_upload(path, req.size, req.mode, req.overwrite)
            .map_err(|e| RouterError::File(e.to_string()))?;

        // No response needed - client should start sending chunks
        Ok(None)
    }

    async fn handle_file_upload_chunk(&self, req: FileUploadChunk) -> RouterResult {
        debug!(
            path = %req.path,
            offset = req.offset,
            size = req.data.len(),
            "Writing upload chunk"
        );

        let path = Path::new(&req.path);
        self.file_transfer
            .write_chunk(path, req.offset, &req.data)
            .map_err(|e| RouterError::File(e.to_string()))?;

        // No response needed - client should continue sending chunks
        Ok(None)
    }

    async fn handle_file_upload_complete(&self, req: FileUploadComplete) -> RouterResult {
        debug!(path = %req.path, "Completing file upload");

        let path = Path::new(&req.path);
        self.file_transfer
            .complete_upload(path, &req.checksum)
            .map_err(|e| RouterError::File(e.to_string()))?;

        info!(path = %req.path, "File upload completed successfully");

        // No response needed - success is implied by lack of error
        Ok(None)
    }

    // =========================================================================
    // Device Handlers
    // =========================================================================

    async fn handle_device_info(&self, info: DeviceInfo) -> RouterResult {
        info!(
            device_id = %info.device_id,
            name = %info.name,
            os = %info.os,
            "Received device info"
        );

        // Try to parse the device ID from hex fingerprint format
        if let Some(device_id) = parse_device_id_from_fingerprint(&info.device_id) {
            // Update last seen timestamp if device exists
            if let Ok(Some(_)) = self.trust_store.get_device(&device_id) {
                let _ = self.trust_store.update_last_seen(&device_id);
            }
        }

        Ok(None)
    }

    async fn handle_device_approval_request(&self, req: DeviceApprovalRequest) -> RouterResult {
        info!(
            device_id = %req.device_id,
            name = %req.name,
            reason = ?req.reason,
            "Received device approval request"
        );

        // Parse the device ID from hex fingerprint format
        let device_id = parse_device_id_from_fingerprint(&req.device_id)
            .ok_or_else(|| RouterError::InvalidRequest("Invalid device ID format".to_string()))?;

        match self.trust_store.get_device(&device_id) {
            Ok(Some(device)) => {
                match device.trust_level {
                    TrustLevel::Trusted => {
                        info!(device_id = %req.device_id, "Device already trusted");
                        Ok(Some(Message::DeviceApproved(DeviceApproved {
                            device_id: req.device_id,
                            expires_at: None, // No expiration for already trusted devices
                            allowed_capabilities: vec![
                                "shell".to_string(),
                                "file-transfer".to_string(),
                            ],
                        })))
                    }
                    TrustLevel::Revoked => {
                        warn!(device_id = %req.device_id, "Device is revoked");
                        Ok(Some(Message::DeviceRejected(DeviceRejected {
                            device_id: req.device_id,
                            reason: "Device has been revoked".to_string(),
                            retry_allowed: false,
                        })))
                    }
                    TrustLevel::Unknown => {
                        // Device exists but not yet approved - this would typically
                        // trigger a user prompt in a real implementation
                        info!(device_id = %req.device_id, "Device pending approval");
                        Ok(Some(Message::DeviceRejected(DeviceRejected {
                            device_id: req.device_id,
                            reason: "Device pending approval".to_string(),
                            retry_allowed: true,
                        })))
                    }
                }
            }
            Ok(None) => {
                // New device - add as unknown and require approval
                let public_key: [u8; 32] = req.public_key.try_into().map_err(|_| {
                    RouterError::InvalidRequest("Invalid public key length".to_string())
                })?;

                use crate::devices::TrustedDevice;
                let new_device =
                    TrustedDevice::new_unknown(device_id, req.name.clone(), public_key);

                self.trust_store
                    .add_device(new_device)
                    .map_err(|e| RouterError::Device(e.to_string()))?;

                // Save the trust store
                let _ = self.trust_store.save();

                info!(device_id = %req.device_id, name = %req.name, "New device registered, pending approval");

                Ok(Some(Message::DeviceRejected(DeviceRejected {
                    device_id: req.device_id,
                    reason: "New device requires approval".to_string(),
                    retry_allowed: true,
                })))
            }
            Err(e) => Err(RouterError::Device(e.to_string())),
        }
    }

    // =========================================================================
    // Control Handlers
    // =========================================================================

    async fn handle_ping(&self, ping: Ping) -> RouterResult {
        debug!(timestamp = ping.timestamp, "Received ping");

        Ok(Some(Message::Pong(Pong {
            timestamp: ping.timestamp,
            payload: ping.payload,
        })))
    }
}

/// Helper function to get current timestamp in milliseconds.
#[allow(dead_code)]
fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Parse a device ID from its fingerprint format (e.g., "a1b2:c3d4:e5f6:7890:1234:5678:9abc:def0").
///
/// Returns None if the string is not in the expected format.
fn parse_device_id_from_fingerprint(fingerprint: &str) -> Option<DeviceId> {
    // Remove colons and decode hex
    let hex_str: String = fingerprint.chars().filter(|c| *c != ':').collect();

    if hex_str.len() != 32 {
        // DeviceId is 16 bytes = 32 hex chars
        return None;
    }

    let bytes = hex::decode(&hex_str).ok()?;
    if bytes.len() != 16 {
        return None;
    }

    let mut arr = [0u8; 16];
    arr.copy_from_slice(&bytes);
    Some(DeviceId::from_bytes(arr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::sync::broadcast;

    /// Mock session manager for testing.
    struct MockSessionManager {
        should_fail: bool,
    }

    impl MockSessionManager {
        fn new() -> Self {
            Self { should_fail: false }
        }

        fn failing() -> Self {
            Self { should_fail: true }
        }
    }

    impl SessionManager for MockSessionManager {
        async fn create(
            &self,
            _shell: Option<String>,
            _cols: u16,
            _rows: u16,
            _env: Vec<(String, String)>,
            _cwd: Option<String>,
        ) -> Result<(SessionId, u32), SessionError> {
            if self.should_fail {
                Err(SessionError::SpawnFailed("Mock failure".to_string()))
            } else {
                Ok(("test-session-123".to_string(), 12345))
            }
        }

        async fn attach(
            &self,
            session_id: &SessionId,
        ) -> Result<broadcast::Receiver<Vec<u8>>, SessionError> {
            if self.should_fail {
                Err(SessionError::NotFound(session_id.clone()))
            } else {
                let (tx, rx) = broadcast::channel(16);
                drop(tx);
                Ok(rx)
            }
        }

        async fn detach(&self, session_id: &SessionId) -> Result<(), SessionError> {
            if self.should_fail {
                Err(SessionError::NotFound(session_id.clone()))
            } else {
                Ok(())
            }
        }

        async fn write(&self, session_id: &SessionId, _data: &[u8]) -> Result<(), SessionError> {
            if self.should_fail {
                Err(SessionError::NotFound(session_id.clone()))
            } else {
                Ok(())
            }
        }

        async fn resize(
            &self,
            session_id: &SessionId,
            _cols: u16,
            _rows: u16,
        ) -> Result<(), SessionError> {
            if self.should_fail {
                Err(SessionError::NotFound(session_id.clone()))
            } else {
                Ok(())
            }
        }

        async fn kill(
            &self,
            session_id: &SessionId,
            _signal: Option<i32>,
        ) -> Result<SessionStatus, SessionError> {
            if self.should_fail {
                Err(SessionError::NotFound(session_id.clone()))
            } else {
                Ok(SessionStatus::Exited(0))
            }
        }

        fn list(&self) -> Vec<crate::session::manager::SessionInfo> {
            vec![]
        }

        async fn get(
            &self,
            _session_id: &SessionId,
        ) -> Option<crate::session::manager::SessionInfo> {
            None
        }

        fn exists(&self, _session_id: &SessionId) -> bool {
            !self.should_fail
        }

        fn count(&self) -> usize {
            0
        }
    }

    fn create_test_router(temp_dir: &TempDir) -> MessageRouter<MockSessionManager> {
        let session_manager = Arc::new(MockSessionManager::new());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));

        MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
        )
    }

    fn create_failing_router(temp_dir: &TempDir) -> MessageRouter<MockSessionManager> {
        let session_manager = Arc::new(MockSessionManager::failing());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));

        MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
        )
    }

    // =========================================================================
    // Session Message Tests
    // =========================================================================

    #[tokio::test]
    async fn test_route_session_create() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::SessionCreate(SessionCreate {
            cols: 80,
            rows: 24,
            shell: Some("/bin/bash".to_string()),
            env: vec![],
            cwd: None,
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.is_some());

        match response.unwrap() {
            Message::SessionCreated(created) => {
                assert_eq!(created.session_id, "test-session-123");
                assert_eq!(created.pid, 12345);
            }
            _ => panic!("Expected SessionCreated response"),
        }
    }

    #[tokio::test]
    async fn test_route_session_create_failure() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_failing_router(&temp_dir);

        let msg = Message::SessionCreate(SessionCreate::default());

        let result = router.route(msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_route_session_attach() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::SessionAttach(SessionAttach {
            session_id: "test-session".to_string(),
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // No direct response
    }

    #[tokio::test]
    async fn test_route_session_detach() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::SessionDetach(SessionDetach {
            session_id: "test-session".to_string(),
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_session_kill() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::SessionKill(SessionKill {
            session_id: "test-session".to_string(),
            signal: Some(9),
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());

        match result.unwrap() {
            Some(Message::SessionClosed(closed)) => {
                assert_eq!(closed.session_id, "test-session");
                assert_eq!(closed.exit_code, Some(0));
            }
            _ => panic!("Expected SessionClosed response"),
        }
    }

    #[tokio::test]
    async fn test_route_session_resize() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::SessionResize(SessionResize {
            session_id: "test-session".to_string(),
            cols: 120,
            rows: 40,
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_session_data_stdin() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::SessionData(SessionData {
            session_id: "test-session".to_string(),
            stream: DataStream::Stdin,
            data: b"hello".to_vec(),
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_session_data_stdout_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::SessionData(SessionData {
            session_id: "test-session".to_string(),
            stream: DataStream::Stdout,
            data: b"hello".to_vec(),
        });

        let result = router.route(msg).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(RouterError::InvalidRequest(_))));
    }

    // =========================================================================
    // File Message Tests
    // =========================================================================

    #[tokio::test]
    async fn test_route_file_list() {
        let temp_dir = TempDir::new().unwrap();

        // Create some test files
        std::fs::write(temp_dir.path().join("file1.txt"), "hello").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "world").unwrap();
        std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

        let router = create_test_router(&temp_dir);

        let msg = Message::FileListRequest(FileListRequest {
            path: temp_dir.path().to_string_lossy().to_string(),
            include_hidden: false,
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());

        match result.unwrap() {
            Some(Message::FileListResponse(response)) => {
                assert_eq!(response.entries.len(), 3);
            }
            _ => panic!("Expected FileListResponse"),
        }
    }

    #[tokio::test]
    async fn test_route_file_download() {
        let temp_dir = TempDir::new().unwrap();
        let content = b"Hello, World!";
        std::fs::write(temp_dir.path().join("download.txt"), content).unwrap();

        let router = create_test_router(&temp_dir);

        let msg = Message::FileDownloadRequest(FileDownloadRequest {
            path: temp_dir
                .path()
                .join("download.txt")
                .to_string_lossy()
                .to_string(),
            offset: 0,
            chunk_size: 1024,
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());

        match result.unwrap() {
            Some(Message::FileDownloadChunk(chunk)) => {
                assert_eq!(chunk.data, content);
                assert_eq!(chunk.total_size, content.len() as u64);
                assert!(chunk.is_last);
            }
            _ => panic!("Expected FileDownloadChunk"),
        }
    }

    #[tokio::test]
    async fn test_route_file_upload_flow() {
        let temp_dir = TempDir::new().unwrap();
        let upload_dir = temp_dir.path().join("uploads");
        std::fs::create_dir_all(&upload_dir).unwrap();

        let router = create_test_router(&temp_dir);
        let dest_path = upload_dir.join("uploaded.txt");

        // Start upload
        let msg = Message::FileUploadStart(FileUploadStart {
            path: dest_path.to_string_lossy().to_string(),
            size: 12,
            mode: 0o644,
            overwrite: false,
        });
        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Send chunk
        let msg = Message::FileUploadChunk(FileUploadChunk {
            path: dest_path.to_string_lossy().to_string(),
            offset: 0,
            data: b"Hello World!".to_vec(),
        });
        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Complete upload with correct checksum
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"Hello World!");
        let checksum = hasher.finalize().to_vec();

        let msg = Message::FileUploadComplete(FileUploadComplete {
            path: dest_path.to_string_lossy().to_string(),
            checksum,
        });
        let result = router.route(msg).await;
        assert!(result.is_ok());

        // Verify file was created
        assert!(dest_path.exists());
        assert_eq!(std::fs::read(&dest_path).unwrap(), b"Hello World!");
    }

    // =========================================================================
    // Device Message Tests
    // =========================================================================

    #[tokio::test]
    async fn test_route_device_info() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::DeviceInfo(DeviceInfo {
            device_id: "test-device-123".to_string(),
            name: "Test Device".to_string(),
            os: "Linux".to_string(),
            os_version: "6.1.0".to_string(),
            arch: "x86_64".to_string(),
            protocol_version: 1,
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_device_approval_request_new_device() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        // Generate a valid device ID
        let identity = protocol::DeviceIdentity::generate();
        let device_id = identity.device_id().to_string();

        let msg = Message::DeviceApprovalRequest(DeviceApprovalRequest {
            device_id: device_id.clone(),
            name: "New Device".to_string(),
            public_key: identity.public_key_bytes().to_vec(),
            reason: Some("Testing".to_string()),
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());

        match result.unwrap() {
            Some(Message::DeviceRejected(rejected)) => {
                assert_eq!(rejected.device_id, device_id);
                assert!(rejected.retry_allowed);
            }
            _ => panic!("Expected DeviceRejected for new device"),
        }
    }

    // =========================================================================
    // Control Message Tests
    // =========================================================================

    #[tokio::test]
    async fn test_route_ping() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::Ping(Ping {
            timestamp: 1234567890,
            payload: b"ping!".to_vec(),
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());

        match result.unwrap() {
            Some(Message::Pong(pong)) => {
                assert_eq!(pong.timestamp, 1234567890);
                assert_eq!(pong.payload, b"ping!");
            }
            _ => panic!("Expected Pong response"),
        }
    }

    #[tokio::test]
    async fn test_route_pong_ignored() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::Pong(Pong {
            timestamp: 1234567890,
            payload: vec![],
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_error_message_logged() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::Error(ErrorMessage {
            code: ErrorCode::InternalError,
            message: "Something went wrong".to_string(),
            context: None,
            recoverable: false,
        });

        let result = router.route(msg).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // =========================================================================
    // Response Messages (should be ignored)
    // =========================================================================

    #[tokio::test]
    async fn test_route_response_messages_ignored() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        // SessionCreated
        let msg = Message::SessionCreated(SessionCreated {
            session_id: "test".to_string(),
            pid: 123,
        });
        assert!(router.route(msg).await.unwrap().is_none());

        // SessionClosed
        let msg = Message::SessionClosed(SessionClosed {
            session_id: "test".to_string(),
            exit_code: Some(0),
            signal: None,
            reason: None,
        });
        assert!(router.route(msg).await.unwrap().is_none());

        // FileListResponse
        let msg = Message::FileListResponse(FileListResponse {
            path: "/tmp".to_string(),
            entries: vec![],
        });
        assert!(router.route(msg).await.unwrap().is_none());

        // FileDownloadChunk
        let msg = Message::FileDownloadChunk(FileDownloadChunk {
            path: "/tmp/test".to_string(),
            offset: 0,
            total_size: 100,
            data: vec![],
            is_last: true,
        });
        assert!(router.route(msg).await.unwrap().is_none());

        // DeviceApproved
        let msg = Message::DeviceApproved(DeviceApproved {
            device_id: "test".to_string(),
            expires_at: None,
            allowed_capabilities: vec![],
        });
        assert!(router.route(msg).await.unwrap().is_none());

        // DeviceRejected
        let msg = Message::DeviceRejected(DeviceRejected {
            device_id: "test".to_string(),
            reason: "test".to_string(),
            retry_allowed: false,
        });
        assert!(router.route(msg).await.unwrap().is_none());
    }

    // =========================================================================
    // Error Handling Tests
    // =========================================================================

    #[tokio::test]
    async fn test_router_error_to_message() {
        let err = RouterError::Session(SessionError::NotFound("test-session".to_string()));
        let msg = err.to_error_message(Some("test-session".to_string()));

        assert_eq!(msg.code, ErrorCode::NotFound);
        assert!(!msg.recoverable);
        assert_eq!(msg.context, Some("test-session".to_string()));
    }

    #[tokio::test]
    async fn test_file_list_invalid_path() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        let msg = Message::FileListRequest(FileListRequest {
            path: "/nonexistent/path/that/does/not/exist".to_string(),
            include_hidden: false,
        });

        let result = router.route(msg).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(RouterError::File(_))));
    }
}
