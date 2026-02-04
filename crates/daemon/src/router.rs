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
use tracing::{debug, error, info, warn};

use crate::devices::{PendingApproval, TrustLevel, TrustStore};
use crate::files::{DirectoryBrowser, FileTransfer, PathPermissions};
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

    /// Permission denied error.
    #[error("permission denied: {0}")]
    Permission(String),

    /// Authentication error.
    #[error("authentication error: {0}")]
    Auth(String),
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
            RouterError::Permission(_) => (ErrorCode::Unauthorized, false),
            RouterError::InvalidRequest(_) => (ErrorCode::InvalidRequest, false),
            RouterError::Internal(_) => (ErrorCode::InternalError, true),
            RouterError::Auth(_) => (ErrorCode::Unauthorized, false),
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
/// File operation types for permission checking.
#[derive(Debug, Clone, Copy)]
pub enum FileOperation {
    /// Read file or list directory.
    Read,
    /// List directory contents.
    List,
    /// Write or create file.
    Write,
    /// Delete file or directory.
    Delete,
}

pub struct MessageRouter<S: SessionManager> {
    /// Session manager for PTY operations.
    session_manager: Arc<S>,
    /// File transfer handler for file operations.
    file_transfer: Arc<FileTransfer>,
    /// Directory browser for listing directories.
    directory_browser: Arc<DirectoryBrowser>,
    /// Trust store for device management.
    trust_store: Arc<TrustStore>,
    /// Path permissions for device file access control.
    path_permissions: Arc<PathPermissions>,
}

impl<S: SessionManager> MessageRouter<S> {
    /// Create a new message router with the given dependencies.
    pub fn new(
        session_manager: Arc<S>,
        file_transfer: Arc<FileTransfer>,
        directory_browser: Arc<DirectoryBrowser>,
        trust_store: Arc<TrustStore>,
        path_permissions: Arc<PathPermissions>,
    ) -> Self {
        Self {
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        }
    }

    /// Checks if the device has permission for a file operation on the given path.
    ///
    /// Returns `Ok(())` if the operation is allowed, otherwise returns
    /// `Err(RouterError::Permission)` with an appropriate error message.
    fn check_file_permission(
        &self,
        device_id: &DeviceId,
        path: &Path,
        operation: FileOperation,
    ) -> Result<(), RouterError> {
        let allowed = match operation {
            FileOperation::Read | FileOperation::List => {
                self.path_permissions.can_device_read(device_id, path)
            }
            FileOperation::Write => self.path_permissions.can_device_write(device_id, path),
            FileOperation::Delete => self.path_permissions.can_device_delete(device_id, path),
        };

        match allowed {
            Ok(true) => Ok(()),
            Ok(false) => {
                warn!(
                    device_id = ?device_id,
                    path = %path.display(),
                    operation = ?operation,
                    "Permission denied for file operation"
                );
                Err(RouterError::Permission(format!(
                    "Device {} not permitted to {:?} path: {}",
                    device_id,
                    operation,
                    path.display()
                )))
            }
            Err(e) => {
                error!(
                    device_id = ?device_id,
                    path = %path.display(),
                    operation = ?operation,
                    error = %e,
                    "Failed to check file permission"
                );
                Err(RouterError::Internal(format!(
                    "Failed to check file permission: {}",
                    e
                )))
            }
        }
    }

    /// Verifies that the device is trusted.
    ///
    /// Returns `Ok(())` if the device has `TrustLevel::Trusted`, otherwise
    /// returns `Err(RouterError::Device)` with an appropriate error message.
    fn require_trusted(&self, device_id: &DeviceId) -> Result<(), RouterError> {
        match self.trust_store.get_device(device_id) {
            Ok(Some(device)) => match device.trust_level {
                TrustLevel::Trusted => Ok(()),
                TrustLevel::Unknown => {
                    Err(RouterError::Device("Device pending approval".to_string()))
                }
                TrustLevel::Revoked => {
                    Err(RouterError::Device("Device has been revoked".to_string()))
                }
            },
            Ok(None) => Err(RouterError::Device("Device not registered".to_string())),
            Err(e) => Err(RouterError::Device(format!(
                "Failed to check device trust: {}",
                e
            ))),
        }
    }

    /// Route a message to the appropriate handler.
    ///
    /// Returns `Ok(Some(response))` if a response should be sent back,
    /// `Ok(None)` if no response is needed, or `Err(error)` if routing failed.
    ///
    /// The `device_id` parameter identifies which device sent the message,
    /// enabling device-aware routing decisions and authorization checks.
    ///
    /// The `authenticated_public_key` parameter is the Noise-authenticated public key
    /// from the handshake. If provided, it is used to verify claimed public keys
    /// in device approval requests to prevent spoofing attacks.
    pub async fn route(
        &self,
        message: Message,
        device_id: &DeviceId,
        authenticated_public_key: Option<&[u8; 32]>,
    ) -> RouterResult {
        debug!(?message, ?device_id, "Routing message");

        match message {
            // Session messages (require trusted device)
            Message::SessionCreate(req) => self.handle_session_create(req, device_id).await,
            Message::SessionAttach(req) => self.handle_session_attach(req, device_id).await,
            Message::SessionDetach(req) => self.handle_session_detach(req).await,
            Message::SessionKill(req) => self.handle_session_kill(req, device_id).await,
            Message::SessionResize(req) => self.handle_session_resize(req).await,
            Message::SessionData(data) => self.handle_session_data(data, device_id).await,
            Message::SessionCreated(_) | Message::SessionClosed(_) => {
                // These are response messages, not requests - ignore them
                debug!("Ignoring response message received as request");
                Ok(None)
            }

            // File messages (require permission checks)
            Message::FileListRequest(req) => self.handle_file_list(req, device_id).await,
            Message::FileDownloadRequest(req) => self.handle_file_download(req, device_id).await,
            Message::FileUploadStart(req) => self.handle_file_upload_start(req, device_id).await,
            Message::FileUploadChunk(req) => self.handle_file_upload_chunk(req, device_id).await,
            Message::FileUploadComplete(req) => {
                self.handle_file_upload_complete(req, device_id).await
            }
            Message::FileListResponse(_) | Message::FileDownloadChunk(_) => {
                // These are response messages, not requests - ignore them
                debug!("Ignoring response message received as request");
                Ok(None)
            }

            // Device messages
            Message::DeviceInfo(info) => self.handle_device_info(info).await,
            Message::DeviceApprovalRequest(req) => {
                self.handle_device_approval_request(req, authenticated_public_key)
                    .await
            }
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

    async fn handle_session_create(
        &self,
        req: SessionCreate,
        device_id: &DeviceId,
    ) -> RouterResult {
        // Verify device is trusted before creating session
        self.require_trusted(device_id)?;

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

    async fn handle_session_attach(
        &self,
        req: SessionAttach,
        device_id: &DeviceId,
    ) -> RouterResult {
        // Verify device is trusted before attaching to session
        self.require_trusted(device_id)?;

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

    async fn handle_session_kill(&self, req: SessionKill, device_id: &DeviceId) -> RouterResult {
        // Verify device is trusted before killing session
        self.require_trusted(device_id)?;

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

    async fn handle_session_data(&self, data: SessionData, device_id: &DeviceId) -> RouterResult {
        // Verify device is trusted before sending session data
        self.require_trusted(device_id)?;

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

    async fn handle_file_list(&self, req: FileListRequest, device_id: &DeviceId) -> RouterResult {
        debug!(path = %req.path, include_hidden = req.include_hidden, "Listing directory");

        let path = Path::new(&req.path);

        // Check permission before listing directory
        self.check_file_permission(device_id, path, FileOperation::List)?;

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

    async fn handle_file_download(
        &self,
        req: FileDownloadRequest,
        device_id: &DeviceId,
    ) -> RouterResult {
        debug!(
            path = %req.path,
            offset = req.offset,
            chunk_size = req.chunk_size,
            "Downloading file chunk"
        );

        let path = Path::new(&req.path);

        // Check permission before downloading file
        self.check_file_permission(device_id, path, FileOperation::Read)?;

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

    async fn handle_file_upload_start(
        &self,
        req: FileUploadStart,
        device_id: &DeviceId,
    ) -> RouterResult {
        debug!(
            path = %req.path,
            size = req.size,
            mode = format!("{:o}", req.mode),
            overwrite = req.overwrite,
            "Starting file upload"
        );

        let path = Path::new(&req.path);

        // Check permission before starting upload
        self.check_file_permission(device_id, path, FileOperation::Write)?;

        self.file_transfer
            .start_upload(path, req.size, req.mode, req.overwrite)
            .map_err(|e| RouterError::File(e.to_string()))?;

        // No response needed - client should start sending chunks
        Ok(None)
    }

    async fn handle_file_upload_chunk(
        &self,
        req: FileUploadChunk,
        device_id: &DeviceId,
    ) -> RouterResult {
        debug!(
            path = %req.path,
            offset = req.offset,
            size = req.data.len(),
            "Writing upload chunk"
        );

        let path = Path::new(&req.path);

        // Check permission before writing chunk
        self.check_file_permission(device_id, path, FileOperation::Write)?;

        self.file_transfer
            .write_chunk(path, req.offset, &req.data)
            .map_err(|e| RouterError::File(e.to_string()))?;

        // No response needed - client should continue sending chunks
        Ok(None)
    }

    async fn handle_file_upload_complete(
        &self,
        req: FileUploadComplete,
        device_id: &DeviceId,
    ) -> RouterResult {
        debug!(path = %req.path, "Completing file upload");

        let path = Path::new(&req.path);

        // Check permission before completing upload
        self.check_file_permission(device_id, path, FileOperation::Write)?;

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
                if let Err(e) = self.trust_store.update_last_seen(&device_id) {
                    error!(error = %e, device_id = ?device_id, "Failed to update device last seen");
                }
            }
        }

        Ok(None)
    }

    async fn handle_device_approval_request(
        &self,
        req: DeviceApprovalRequest,
        authenticated_public_key: Option<&[u8; 32]>,
    ) -> RouterResult {
        info!(
            device_id = %req.device_id,
            name = %req.name,
            reason = ?req.reason,
            "Received device approval request"
        );

        // Parse the device ID from hex fingerprint format
        let device_id = parse_device_id_from_fingerprint(&req.device_id)
            .ok_or_else(|| RouterError::InvalidRequest("Invalid device ID format".to_string()))?;

        // Verify the claimed public key matches the Noise-authenticated key
        // This prevents key spoofing attacks where a peer claims a different identity
        if let Some(authenticated_key) = authenticated_public_key {
            let claimed_key: [u8; 32] = req.public_key.clone().try_into().map_err(|_| {
                RouterError::InvalidRequest("Invalid public key length".to_string())
            })?;

            // Compare keys (note: ideally this should use constant-time comparison
            // via the subtle crate to prevent timing attacks, but the security impact
            // is minimal since the attacker already knows both keys)
            if &claimed_key != authenticated_key {
                warn!(
                    claimed = ?hex::encode(claimed_key),
                    authenticated = ?hex::encode(authenticated_key),
                    "Public key mismatch - possible spoofing attempt"
                );
                return Err(RouterError::Auth("Public key mismatch".to_string()));
            }
        }

        // First, check if the device is already in the trusted devices store
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
                // Check if device is already in the pending queue
                if self.trust_store.is_pending(&device_id).unwrap_or(false) {
                    info!(device_id = %req.device_id, "Device already in pending queue");
                    return Ok(Some(Message::DeviceRejected(DeviceRejected {
                        device_id: req.device_id,
                        reason: "Device pending approval".to_string(),
                        retry_allowed: true,
                    })));
                }

                // New device - handle based on require_approval setting
                let public_key: [u8; 32] = req.public_key.try_into().map_err(|_| {
                    RouterError::InvalidRequest("Invalid public key length".to_string())
                })?;

                if self.trust_store.require_approval() {
                    // Add to pending approvals queue
                    let pending = PendingApproval::new(
                        device_id,
                        req.name.clone(),
                        public_key,
                        None, // remote_addr not available here, could be passed from connection handler
                    );

                    self.trust_store
                        .add_pending(pending)
                        .map_err(|e| RouterError::Device(e.to_string()))?;

                    info!(
                        device_id = %req.device_id,
                        name = %req.name,
                        "New device added to pending approvals queue"
                    );

                    Ok(Some(Message::DeviceRejected(DeviceRejected {
                        device_id: req.device_id,
                        reason: "Device pending approval".to_string(),
                        retry_allowed: true,
                    })))
                } else {
                    // require_approval is false - add as unknown (legacy behavior)
                    use crate::devices::TrustedDevice;
                    let new_device =
                        TrustedDevice::new_unknown(device_id, req.name.clone(), public_key);

                    self.trust_store
                        .add_device(new_device)
                        .map_err(|e| RouterError::Device(e.to_string()))?;

                    // Save the trust store
                    if let Err(e) = self.trust_store.save() {
                        tracing::error!(error = %e, "Failed to save trust store");
                    }

                    info!(
                        device_id = %req.device_id,
                        name = %req.name,
                        "New device registered, pending approval"
                    );

                    Ok(Some(Message::DeviceRejected(DeviceRejected {
                        device_id: req.device_id,
                        reason: "New device requires approval".to_string(),
                        retry_allowed: true,
                    })))
                }
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
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));

        // Set up default permissions for test device (allow all within temp_dir)
        let device_id = test_device_id();
        let device_perms = crate::files::DevicePermissions::allow_all_dangerous(device_id);
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        )
    }

    /// Creates a test device ID for use in unit tests.
    fn test_device_id() -> DeviceId {
        DeviceId::from_bytes([0u8; 16])
    }

    /// Creates a trusted device in the trust store and returns its device ID.
    fn create_trusted_device(trust_store: &TrustStore) -> DeviceId {
        let device_id = test_device_id();
        let device =
            crate::devices::TrustedDevice::new(device_id, "Test Device".to_string(), [0u8; 32]);
        trust_store.add_device(device).unwrap();
        device_id
    }

    /// Creates a router with a trusted device already registered.
    fn create_test_router_with_trusted_device(
        temp_dir: &TempDir,
    ) -> (MessageRouter<MockSessionManager>, DeviceId) {
        let session_manager = Arc::new(MockSessionManager::new());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));

        // Register a trusted device
        let device_id = create_trusted_device(&trust_store);

        // Set up default permissions for trusted device (allow all within temp_dir)
        let device_perms = crate::files::DevicePermissions::allow_all_dangerous(device_id);
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        let router = MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        );
        (router, device_id)
    }

    // =========================================================================
    // Session Message Tests
    // =========================================================================

    #[tokio::test]
    async fn test_route_session_create() {
        let temp_dir = TempDir::new().unwrap();
        let (router, device_id) = create_test_router_with_trusted_device(&temp_dir);

        let msg = Message::SessionCreate(SessionCreate {
            cols: 80,
            rows: 24,
            shell: Some("/bin/bash".to_string()),
            env: vec![],
            cwd: None,
        });

        let result = router.route(msg, &device_id, None).await;
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
        // Create a router with a failing session manager but trusted device
        let session_manager = Arc::new(MockSessionManager::failing());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));
        let device_id = create_trusted_device(&trust_store);

        // Set up permissions for the device
        let device_perms = crate::files::DevicePermissions::allow_all_dangerous(device_id);
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        let router = MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        );

        let msg = Message::SessionCreate(SessionCreate::default());

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_route_session_create_untrusted_device_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        // Use an unregistered device ID
        let untrusted_device = DeviceId::from_bytes([1u8; 16]);

        let msg = Message::SessionCreate(SessionCreate {
            cols: 80,
            rows: 24,
            shell: Some("/bin/bash".to_string()),
            env: vec![],
            cwd: None,
        });

        let result = router.route(msg, &untrusted_device, None).await;
        assert!(result.is_err());

        match result {
            Err(RouterError::Device(msg)) => {
                assert!(
                    msg.contains("not registered"),
                    "Expected 'not registered' error, got: {}",
                    msg
                );
            }
            _ => panic!("Expected RouterError::Device"),
        }
    }

    #[tokio::test]
    async fn test_route_session_create_pending_device_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(MockSessionManager::new());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));

        // Register a device with Unknown trust level (pending)
        let device_id = DeviceId::from_bytes([2u8; 16]);
        let device = crate::devices::TrustedDevice::new_unknown(
            device_id,
            "Pending Device".to_string(),
            [0u8; 32],
        );
        trust_store.add_device(device).unwrap();

        // Set up permissions for the device
        let device_perms = crate::files::DevicePermissions::allow_all_dangerous(device_id);
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        let router = MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        );

        let msg = Message::SessionCreate(SessionCreate {
            cols: 80,
            rows: 24,
            shell: None,
            env: vec![],
            cwd: None,
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_err());

        match result {
            Err(RouterError::Device(msg)) => {
                assert!(
                    msg.contains("pending"),
                    "Expected 'pending' error, got: {}",
                    msg
                );
            }
            _ => panic!("Expected RouterError::Device"),
        }
    }

    #[tokio::test]
    async fn test_route_session_attach() {
        let temp_dir = TempDir::new().unwrap();
        let (router, device_id) = create_test_router_with_trusted_device(&temp_dir);

        let msg = Message::SessionAttach(SessionAttach {
            session_id: "test-session".to_string(),
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none()); // No direct response
    }

    #[tokio::test]
    async fn test_route_session_detach() {
        let temp_dir = TempDir::new().unwrap();
        let (router, device_id) = create_test_router_with_trusted_device(&temp_dir);

        let msg = Message::SessionDetach(SessionDetach {
            session_id: "test-session".to_string(),
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_session_kill() {
        let temp_dir = TempDir::new().unwrap();
        let (router, device_id) = create_test_router_with_trusted_device(&temp_dir);

        let msg = Message::SessionKill(SessionKill {
            session_id: "test-session".to_string(),
            signal: Some(9),
        });

        let result = router.route(msg, &device_id, None).await;
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
        let (router, device_id) = create_test_router_with_trusted_device(&temp_dir);

        let msg = Message::SessionResize(SessionResize {
            session_id: "test-session".to_string(),
            cols: 120,
            rows: 40,
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_session_data_stdin() {
        let temp_dir = TempDir::new().unwrap();
        let (router, device_id) = create_test_router_with_trusted_device(&temp_dir);

        let msg = Message::SessionData(SessionData {
            session_id: "test-session".to_string(),
            stream: DataStream::Stdin,
            data: b"hello".to_vec(),
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_route_session_data_stdout_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let (router, device_id) = create_test_router_with_trusted_device(&temp_dir);

        let msg = Message::SessionData(SessionData {
            session_id: "test-session".to_string(),
            stream: DataStream::Stdout,
            data: b"hello".to_vec(),
        });

        let result = router.route(msg, &device_id, None).await;
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

        let result = router.route(msg, &test_device_id(), None).await;
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

        let result = router.route(msg, &test_device_id(), None).await;
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
        let result = router.route(msg, &test_device_id(), None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());

        // Send chunk
        let msg = Message::FileUploadChunk(FileUploadChunk {
            path: dest_path.to_string_lossy().to_string(),
            offset: 0,
            data: b"Hello World!".to_vec(),
        });
        let result = router.route(msg, &test_device_id(), None).await;
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
        let result = router.route(msg, &test_device_id(), None).await;
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

        let result = router.route(msg, &test_device_id(), None).await;
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

        let result = router.route(msg, &test_device_id(), None).await;
        assert!(result.is_ok());

        match result.unwrap() {
            Some(Message::DeviceRejected(rejected)) => {
                assert_eq!(rejected.device_id, device_id);
                assert!(rejected.retry_allowed);
            }
            _ => panic!("Expected DeviceRejected for new device"),
        }
    }

    #[tokio::test]
    async fn test_route_device_approval_request_public_key_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        // Generate a device identity
        let identity = protocol::DeviceIdentity::generate();
        let device_id = identity.device_id().to_string();

        // Create a request with the device's public key
        let msg = Message::DeviceApprovalRequest(DeviceApprovalRequest {
            device_id: device_id.clone(),
            name: "Test Device".to_string(),
            public_key: identity.public_key_bytes().to_vec(),
            reason: Some("Testing".to_string()),
        });

        // Provide a DIFFERENT authenticated public key (simulating spoofing)
        let different_key: [u8; 32] = [0xAB; 32]; // Different from device's key

        let result = router
            .route(msg, &test_device_id(), Some(&different_key))
            .await;
        assert!(result.is_err());

        match result {
            Err(RouterError::Auth(msg)) => {
                assert!(
                    msg.contains("mismatch"),
                    "Expected 'mismatch' error, got: {}",
                    msg
                );
            }
            _ => panic!("Expected RouterError::Auth for public key mismatch"),
        }
    }

    #[tokio::test]
    async fn test_route_device_approval_request_public_key_match() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        // Generate a device identity
        let identity = protocol::DeviceIdentity::generate();
        let device_id = identity.device_id().to_string();
        let public_key_bytes = identity.public_key_bytes();

        // Create a request with the device's public key
        let msg = Message::DeviceApprovalRequest(DeviceApprovalRequest {
            device_id: device_id.clone(),
            name: "Test Device".to_string(),
            public_key: public_key_bytes.to_vec(),
            reason: Some("Testing".to_string()),
        });

        // Provide the SAME authenticated public key
        let result = router
            .route(msg, &test_device_id(), Some(&public_key_bytes))
            .await;

        // Should succeed (returns DeviceRejected since device is new, but no Auth error)
        assert!(result.is_ok());
        match result.unwrap() {
            Some(Message::DeviceRejected(rejected)) => {
                assert_eq!(rejected.device_id, device_id);
                assert!(rejected.retry_allowed);
            }
            _ => panic!("Expected DeviceRejected for new device with matching key"),
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

        let result = router.route(msg, &test_device_id(), None).await;
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

        let result = router.route(msg, &test_device_id(), None).await;
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

        let result = router.route(msg, &test_device_id(), None).await;
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
        assert!(router
            .route(msg, &test_device_id(), None)
            .await
            .unwrap()
            .is_none());

        // SessionClosed
        let msg = Message::SessionClosed(SessionClosed {
            session_id: "test".to_string(),
            exit_code: Some(0),
            signal: None,
            reason: None,
        });
        assert!(router
            .route(msg, &test_device_id(), None)
            .await
            .unwrap()
            .is_none());

        // FileListResponse
        let msg = Message::FileListResponse(FileListResponse {
            path: "/tmp".to_string(),
            entries: vec![],
        });
        assert!(router
            .route(msg, &test_device_id(), None)
            .await
            .unwrap()
            .is_none());

        // FileDownloadChunk
        let msg = Message::FileDownloadChunk(FileDownloadChunk {
            path: "/tmp/test".to_string(),
            offset: 0,
            total_size: 100,
            data: vec![],
            is_last: true,
        });
        assert!(router
            .route(msg, &test_device_id(), None)
            .await
            .unwrap()
            .is_none());

        // DeviceApproved
        let msg = Message::DeviceApproved(DeviceApproved {
            device_id: "test".to_string(),
            expires_at: None,
            allowed_capabilities: vec![],
        });
        assert!(router
            .route(msg, &test_device_id(), None)
            .await
            .unwrap()
            .is_none());

        // DeviceRejected
        let msg = Message::DeviceRejected(DeviceRejected {
            device_id: "test".to_string(),
            reason: "test".to_string(),
            retry_allowed: false,
        });
        assert!(router
            .route(msg, &test_device_id(), None)
            .await
            .unwrap()
            .is_none());
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
    async fn test_file_list_path_outside_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        // Path outside allowed paths returns Permission error
        let msg = Message::FileListRequest(FileListRequest {
            path: "/nonexistent/path/that/does/not/exist".to_string(),
            include_hidden: false,
        });

        let result = router.route(msg, &test_device_id(), None).await;
        assert!(result.is_err());
        // Permission check happens first, before filesystem access
        assert!(matches!(result, Err(RouterError::Permission(_))));
    }

    #[tokio::test]
    async fn test_file_list_nonexistent_path_within_allowed() {
        let temp_dir = TempDir::new().unwrap();
        let router = create_test_router(&temp_dir);

        // Path inside allowed directory but doesn't exist
        let nonexistent = temp_dir.path().join("nonexistent_subdir");
        let msg = Message::FileListRequest(FileListRequest {
            path: nonexistent.to_string_lossy().to_string(),
            include_hidden: false,
        });

        let result = router.route(msg, &test_device_id(), None).await;
        assert!(result.is_err());
        // File error because the path is allowed but doesn't exist
        assert!(matches!(result, Err(RouterError::File(_))));
    }

    // =========================================================================
    // Permission Tests
    // =========================================================================

    #[tokio::test]
    async fn test_file_list_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(MockSessionManager::new());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));

        // Create a device with NO permissions (default is None)
        let device_id = DeviceId::from_bytes([3u8; 16]);
        let device_perms = crate::files::DevicePermissions::new(device_id);
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        let router = MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        );

        // Create a test directory
        std::fs::create_dir(temp_dir.path().join("test_dir")).unwrap();

        let msg = Message::FileListRequest(FileListRequest {
            path: temp_dir
                .path()
                .join("test_dir")
                .to_string_lossy()
                .to_string(),
            include_hidden: false,
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(RouterError::Permission(_))));
    }

    #[tokio::test]
    async fn test_file_download_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(MockSessionManager::new());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));

        // Create a device with NO permissions
        let device_id = DeviceId::from_bytes([4u8; 16]);
        let device_perms = crate::files::DevicePermissions::new(device_id);
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        let router = MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        );

        // Create a test file
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test content").unwrap();

        let msg = Message::FileDownloadRequest(FileDownloadRequest {
            path: test_file.to_string_lossy().to_string(),
            offset: 0,
            chunk_size: 1024,
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(RouterError::Permission(_))));
    }

    #[tokio::test]
    async fn test_file_upload_permission_denied() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(MockSessionManager::new());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));

        // Create a device with read-only permissions
        let device_id = DeviceId::from_bytes([5u8; 16]);
        let mut device_perms = crate::files::DevicePermissions::new(device_id);
        device_perms.add_path(crate::files::permissions::PathPermission::read_only(
            temp_dir.path().to_path_buf(),
        ));
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        let router = MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        );

        let dest_path = temp_dir.path().join("upload.txt");

        let msg = Message::FileUploadStart(FileUploadStart {
            path: dest_path.to_string_lossy().to_string(),
            size: 12,
            mode: 0o644,
            overwrite: false,
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(RouterError::Permission(_))));
    }

    #[tokio::test]
    async fn test_file_operations_with_proper_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let session_manager = Arc::new(MockSessionManager::new());
        let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
        let file_transfer = Arc::new(
            FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
                .with_temp_dir(temp_dir.path().join("tmp")),
        );
        let directory_browser =
            Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
        let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
        let path_permissions = Arc::new(PathPermissions::new(
            temp_dir.path().join("permissions.json"),
            vec![temp_dir.path().to_path_buf()],
        ));

        // Create a device with read-write permissions
        let device_id = DeviceId::from_bytes([6u8; 16]);
        let mut device_perms = crate::files::DevicePermissions::new(device_id);
        device_perms.add_path(crate::files::permissions::PathPermission::read_write(
            temp_dir.path().to_path_buf(),
        ));
        path_permissions
            .set_device_permissions(device_perms)
            .unwrap();

        let router = MessageRouter::new(
            session_manager,
            file_transfer,
            directory_browser,
            trust_store,
            path_permissions,
        );

        // Create a test file
        let test_file = temp_dir.path().join("readable.txt");
        std::fs::write(&test_file, "test content").unwrap();

        // Test read operation succeeds
        let msg = Message::FileDownloadRequest(FileDownloadRequest {
            path: test_file.to_string_lossy().to_string(),
            offset: 0,
            chunk_size: 1024,
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());

        // Test list operation succeeds
        let msg = Message::FileListRequest(FileListRequest {
            path: temp_dir.path().to_string_lossy().to_string(),
            include_hidden: false,
        });

        let result = router.route(msg, &device_id, None).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
