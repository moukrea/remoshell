//! End-to-end integration tests for RemoShell.
//!
//! These tests verify complete flows work correctly:
//! - Daemon startup and shutdown
//! - Session management
//! - File operations
//! - Message routing

use std::path::PathBuf;
use std::sync::Arc;

use daemon::config::Config;
use daemon::devices::TrustStore;
use daemon::files::{DirectoryBrowser, FileTransfer};
use daemon::orchestrator::{DaemonOrchestrator, OrchestratorState};
use daemon::router::MessageRouter;
use daemon::session::{SessionManager, SessionManagerImpl};
use protocol::messages::{FileListRequest, Message, Ping, SessionCreate};
use protocol::DeviceId;
use tempfile::TempDir;

/// Create a test configuration with a temporary directory.
fn create_test_config() -> (Config, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let mut config = Config::default();
    config.daemon.data_dir = temp_dir.path().to_path_buf();
    config.file.allowed_paths = vec![temp_dir.path().to_path_buf()];
    config.security.require_approval = false;
    (config, temp_dir)
}

// =============================================================================
// Orchestrator Lifecycle Tests
// =============================================================================

#[tokio::test]
async fn test_orchestrator_creates_identity() {
    let (config, _temp_dir) = create_test_config();

    let orchestrator = DaemonOrchestrator::new(config).unwrap();

    // Verify identity was created
    let fingerprint = orchestrator.device_id_fingerprint();
    assert!(!fingerprint.is_empty());
    assert!(fingerprint.contains(':'));
}

#[tokio::test]
async fn test_orchestrator_state_starts_stopped() {
    let (config, _temp_dir) = create_test_config();

    let orchestrator = DaemonOrchestrator::new(config).unwrap();

    assert_eq!(orchestrator.state().await, OrchestratorState::Stopped);
}

#[tokio::test]
async fn test_orchestrator_stop_when_already_stopped() {
    let (config, _temp_dir) = create_test_config();

    let mut orchestrator = DaemonOrchestrator::new(config).unwrap();

    // Stop should succeed even when already stopped
    let result = orchestrator.stop().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_orchestrator_event_subscription() {
    let (config, _temp_dir) = create_test_config();

    let orchestrator = DaemonOrchestrator::new(config).unwrap();
    let _receiver = orchestrator.subscribe();

    // Should be able to get multiple receivers
    let _receiver2 = orchestrator.subscribe();
}

// =============================================================================
// Session Manager Tests
// =============================================================================

#[tokio::test]
async fn test_session_create_and_list() {
    let manager = SessionManagerImpl::new();

    // Create a session
    let result = manager
        .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
        .await;

    assert!(result.is_ok());
    let (session_id, pid) = result.unwrap();
    assert!(!session_id.is_empty());
    assert!(pid > 0);

    // List sessions
    let sessions = manager.list();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session_id);

    // Cleanup
    let _ = manager.kill(&session_id, Some(9)).await;
}

#[tokio::test]
async fn test_session_resize() {
    let manager = SessionManagerImpl::new();

    let (session_id, _) = manager
        .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
        .await
        .unwrap();

    // Resize the session
    let result = manager.resize(&session_id, 120, 40).await;
    assert!(result.is_ok());

    // Verify new dimensions
    let info = manager.get(&session_id).await.unwrap();
    assert_eq!(info.cols, 120);
    assert_eq!(info.rows, 40);

    // Cleanup
    let _ = manager.kill(&session_id, Some(9)).await;
}

#[tokio::test]
async fn test_session_write_and_attach() {
    let manager = SessionManagerImpl::new();

    let (session_id, _) = manager
        .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
        .await
        .unwrap();

    // Attach to session
    let rx = manager.attach(&session_id).await;
    assert!(rx.is_ok());

    // Write to session
    let result = manager.write(&session_id, b"echo test\n").await;
    assert!(result.is_ok());

    // Cleanup
    let _ = manager.kill(&session_id, Some(9)).await;
}

#[tokio::test]
async fn test_session_kill() {
    let manager = SessionManagerImpl::new();

    let (session_id, _) = manager
        .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
        .await
        .unwrap();

    assert!(manager.exists(&session_id));

    // Kill the session
    let result = manager.kill(&session_id, Some(9)).await;
    assert!(result.is_ok());

    // Session should be gone
    assert!(!manager.exists(&session_id));
}

// =============================================================================
// Message Router Tests
// =============================================================================

fn create_test_router(temp_dir: &TempDir) -> MessageRouter<MockSessionManager> {
    let session_manager = Arc::new(MockSessionManager::new());
    let browser_for_transfer = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
    let file_transfer = Arc::new(
        FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp")),
    );
    let directory_browser = Arc::new(DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
    let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));

    // Register a trusted device for tests that require session operations
    let device_id = test_device_id();
    let device = daemon::devices::TrustedDevice::new(device_id, "Test Device".to_string(), [0u8; 32]);
    trust_store.add_device(device).unwrap();

    MessageRouter::new(
        session_manager,
        file_transfer,
        directory_browser,
        trust_store,
    )
}

/// Creates a test device ID for use in integration tests.
fn test_device_id() -> DeviceId {
    DeviceId::from_bytes([0u8; 16])
}

/// Mock session manager for router tests
struct MockSessionManager;

impl MockSessionManager {
    fn new() -> Self {
        Self
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
    ) -> Result<(String, u32), daemon::session::SessionError> {
        Ok(("mock-session-123".to_string(), 12345))
    }

    async fn attach(
        &self,
        _session_id: &String,
    ) -> Result<tokio::sync::broadcast::Receiver<Vec<u8>>, daemon::session::SessionError> {
        let (tx, rx) = tokio::sync::broadcast::channel(16);
        drop(tx);
        Ok(rx)
    }

    async fn detach(&self, _session_id: &String) -> Result<(), daemon::session::SessionError> {
        Ok(())
    }

    async fn write(
        &self,
        _session_id: &String,
        _data: &[u8],
    ) -> Result<(), daemon::session::SessionError> {
        Ok(())
    }

    async fn resize(
        &self,
        _session_id: &String,
        _cols: u16,
        _rows: u16,
    ) -> Result<(), daemon::session::SessionError> {
        Ok(())
    }

    async fn kill(
        &self,
        _session_id: &String,
        _signal: Option<i32>,
    ) -> Result<daemon::session::SessionStatus, daemon::session::SessionError> {
        Ok(daemon::session::SessionStatus::Exited(0))
    }

    fn list(&self) -> Vec<daemon::session::manager::SessionInfo> {
        vec![]
    }

    async fn get(&self, _session_id: &String) -> Option<daemon::session::manager::SessionInfo> {
        None
    }

    fn exists(&self, _session_id: &String) -> bool {
        true
    }

    fn count(&self) -> usize {
        0
    }
}

#[tokio::test]
async fn test_router_session_create() {
    let temp_dir = TempDir::new().unwrap();
    let router = create_test_router(&temp_dir);

    let msg = Message::SessionCreate(SessionCreate {
        cols: 80,
        rows: 24,
        shell: Some("/bin/bash".to_string()),
        env: vec![],
        cwd: None,
    });

    let result = router.route(msg, &test_device_id()).await;
    assert!(result.is_ok());

    match result.unwrap() {
        Some(Message::SessionCreated(created)) => {
            assert_eq!(created.session_id, "mock-session-123");
            assert_eq!(created.pid, 12345);
        }
        _ => panic!("Expected SessionCreated response"),
    }
}

#[tokio::test]
async fn test_router_ping_pong() {
    let temp_dir = TempDir::new().unwrap();
    let router = create_test_router(&temp_dir);

    let msg = Message::Ping(Ping {
        timestamp: 1234567890,
        payload: b"test".to_vec(),
    });

    let result = router.route(msg, &test_device_id()).await;
    assert!(result.is_ok());

    match result.unwrap() {
        Some(Message::Pong(pong)) => {
            assert_eq!(pong.timestamp, 1234567890);
            assert_eq!(pong.payload, b"test");
        }
        _ => panic!("Expected Pong response"),
    }
}

#[tokio::test]
async fn test_router_file_list() {
    let temp_dir = TempDir::new().unwrap();

    // Create some test files
    std::fs::write(temp_dir.path().join("test1.txt"), "content1").unwrap();
    std::fs::write(temp_dir.path().join("test2.txt"), "content2").unwrap();
    std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

    let router = create_test_router(&temp_dir);

    let msg = Message::FileListRequest(FileListRequest {
        path: temp_dir.path().to_string_lossy().to_string(),
        include_hidden: false,
    });

    let result = router.route(msg, &test_device_id()).await;
    assert!(result.is_ok());

    match result.unwrap() {
        Some(Message::FileListResponse(response)) => {
            assert_eq!(response.entries.len(), 3); // 2 files + 1 directory
        }
        _ => panic!("Expected FileListResponse"),
    }
}

// =============================================================================
// Trust Store Tests
// =============================================================================

#[tokio::test]
async fn test_trust_store_add_and_get() {
    let temp_dir = TempDir::new().unwrap();
    let store = TrustStore::new(temp_dir.path().join("trust.json"));

    let identity = protocol::DeviceIdentity::generate();
    let device = daemon::devices::TrustedDevice::new(
        *identity.device_id(),
        "Test Device".to_string(),
        identity.public_key_bytes(),
    );
    let device_id = device.device_id;

    store.add_device(device).unwrap();
    store.save().unwrap();

    // Verify device was added
    assert!(store.is_trusted(&device_id).unwrap());

    let retrieved = store.get_device(&device_id).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "Test Device");
}

#[tokio::test]
async fn test_trust_store_revoke() {
    let temp_dir = TempDir::new().unwrap();
    let store = TrustStore::new(temp_dir.path().join("trust.json"));

    let identity = protocol::DeviceIdentity::generate();
    let device = daemon::devices::TrustedDevice::new(
        *identity.device_id(),
        "Test Device".to_string(),
        identity.public_key_bytes(),
    );
    let device_id = device.device_id;

    store.add_device(device).unwrap();
    assert!(store.is_trusted(&device_id).unwrap());

    // Revoke the device
    store
        .set_trust_level(&device_id, daemon::devices::TrustLevel::Revoked)
        .unwrap();
    assert!(!store.is_trusted(&device_id).unwrap());
}

#[tokio::test]
async fn test_trust_store_persistence() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join("trust.json");

    let identity = protocol::DeviceIdentity::generate();
    let device_id = *identity.device_id();

    // Create and save
    {
        let store = TrustStore::new(&path);
        let device = daemon::devices::TrustedDevice::new(
            device_id,
            "Persistent Device".to_string(),
            identity.public_key_bytes(),
        );
        store.add_device(device).unwrap();
        store.save().unwrap();
    }

    // Load and verify
    {
        let store = TrustStore::new(&path);
        store.load().unwrap();
        assert!(store.is_trusted(&device_id).unwrap());
    }
}

// =============================================================================
// File Transfer Tests
// =============================================================================

#[tokio::test]
async fn test_file_download_chunk() {
    let temp_dir = TempDir::new().unwrap();
    let content = b"Hello, World! This is test content for download.";
    std::fs::write(temp_dir.path().join("download_test.txt"), content).unwrap();

    let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
    let transfer =
        FileTransfer::new(browser, 100 * 1024 * 1024).with_temp_dir(temp_dir.path().join("tmp"));

    let path = temp_dir.path().join("download_test.txt");
    let (data, total_size, is_last) = transfer.download_chunk(&path, 0, 1024).unwrap();

    assert_eq!(data, content);
    assert_eq!(total_size, content.len() as u64);
    assert!(is_last);
}

#[tokio::test]
async fn test_file_upload_flow() {
    let temp_dir = TempDir::new().unwrap();
    let upload_dir = temp_dir.path().join("uploads");
    std::fs::create_dir_all(&upload_dir).unwrap();

    let browser = DirectoryBrowser::new(vec![upload_dir.clone()]);
    let transfer =
        FileTransfer::new(browser, 100 * 1024 * 1024).with_temp_dir(temp_dir.path().join("tmp"));

    let dest_path = upload_dir.join("uploaded.txt");
    let content = b"Uploaded content!";

    // Start upload
    transfer
        .start_upload(&dest_path, content.len() as u64, 0o644, false)
        .unwrap();

    // Write chunk
    transfer.write_chunk(&dest_path, 0, content).unwrap();

    // Calculate checksum
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content);
    let checksum = hasher.finalize().to_vec();

    // Complete upload
    transfer.complete_upload(&dest_path, &checksum).unwrap();

    // Verify file was created
    assert!(dest_path.exists());
    assert_eq!(std::fs::read(&dest_path).unwrap(), content);
}

// =============================================================================
// Directory Browser Tests
// =============================================================================

#[tokio::test]
async fn test_directory_browser_list() {
    let temp_dir = TempDir::new().unwrap();

    // Create test structure
    std::fs::write(temp_dir.path().join("file1.txt"), "content").unwrap();
    std::fs::write(temp_dir.path().join(".hidden"), "hidden").unwrap();
    std::fs::create_dir(temp_dir.path().join("subdir")).unwrap();

    let browser = DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);

    // List without hidden files
    let entries = browser.list_directory(temp_dir.path(), false).unwrap();
    assert_eq!(entries.len(), 2); // file1.txt + subdir

    // List with hidden files
    let entries_with_hidden = browser.list_directory(temp_dir.path(), true).unwrap();
    assert_eq!(entries_with_hidden.len(), 3); // file1.txt + .hidden + subdir
}

#[tokio::test]
async fn test_directory_browser_permission_denied() {
    let browser = DirectoryBrowser::new(vec![PathBuf::from("/tmp")]);

    // Try to list a directory outside allowed paths
    let result = browser.list_directory(std::path::Path::new("/etc"), false);
    assert!(result.is_err());
}

// =============================================================================
// Identity Tests
// =============================================================================

#[test]
fn test_identity_generation_deterministic() {
    let identity1 = protocol::DeviceIdentity::generate();
    let identity2 = protocol::DeviceIdentity::generate();

    // Two generated identities should be different
    assert_ne!(identity1.device_id(), identity2.device_id());
}

#[test]
fn test_identity_from_bytes_reproducible() {
    let identity1 = protocol::DeviceIdentity::generate();
    let bytes = identity1.secret_key_bytes();

    let identity2 = protocol::DeviceIdentity::from_secret_key_bytes(&bytes);

    // Same bytes should produce same device ID
    assert_eq!(identity1.device_id(), identity2.device_id());
}

// =============================================================================
// Graceful Shutdown Test
// =============================================================================

#[tokio::test]
async fn test_session_manager_cleanup() {
    let manager = Arc::new(SessionManagerImpl::new());

    // Create multiple sessions
    let (session1, _) = manager
        .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
        .await
        .unwrap();
    let (session2, _) = manager
        .create(Some("/bin/sh".to_string()), 80, 24, vec![], None)
        .await
        .unwrap();

    assert_eq!(manager.count(), 2);

    // Kill all sessions
    manager.kill(&session1, Some(9)).await.unwrap();
    manager.kill(&session2, Some(9)).await.unwrap();

    assert_eq!(manager.count(), 0);
}
