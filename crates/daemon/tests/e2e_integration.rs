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
use daemon::files::{DirectoryBrowser, FileTransfer, PathPermissions};
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
    let path_permissions = Arc::new(PathPermissions::new(
        temp_dir.path().join("permissions.json"),
        vec![temp_dir.path().to_path_buf()],
    ));

    // Register a trusted device for tests that require session operations
    let device_id = test_device_id();
    let device = daemon::devices::TrustedDevice::new(device_id, "Test Device".to_string(), [0u8; 32]);
    trust_store.add_device(device).unwrap();

    // Set up permissions for the test device (allow all within temp_dir)
    // Using allow_all_dangerous() is acceptable here since this is a test environment
    let device_perms = daemon::files::DevicePermissions::allow_all_dangerous(device_id);
    path_permissions.set_device_permissions(device_perms).unwrap();

    MessageRouter::new(
        session_manager,
        file_transfer,
        directory_browser,
        trust_store,
        path_permissions,
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

    let result = router.route(msg, &test_device_id(), None).await;
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

    let result = router.route(msg, &test_device_id(), None).await;
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

    let result = router.route(msg, &test_device_id(), None).await;
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

// =============================================================================
// Device Auth Integration Tests
// =============================================================================

/// Test that when `require_approval` is enabled, new devices are added to the
/// pending queue instead of being immediately registered.
#[tokio::test]
async fn test_pending_approval_flow() {
    use protocol::messages::{DeviceApprovalRequest, Message};

    let temp_dir = TempDir::new().unwrap();
    let session_manager = Arc::new(MockSessionManager::new());
    let browser_for_transfer = daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
    let file_transfer = Arc::new(
        daemon::files::FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp")),
    );
    let directory_browser = Arc::new(daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));

    // Create trust store with require_approval enabled
    let mut trust_store = TrustStore::new(temp_dir.path().join("trust.json"));
    trust_store.set_require_approval(true);
    let trust_store = Arc::new(trust_store);

    let path_permissions = Arc::new(daemon::files::PathPermissions::new(
        temp_dir.path().join("permissions.json"),
        vec![temp_dir.path().to_path_buf()],
    ));

    let router = MessageRouter::new(
        session_manager,
        file_transfer,
        directory_browser,
        trust_store.clone(),
        path_permissions,
    );

    // Generate a new device identity
    let identity = protocol::DeviceIdentity::generate();
    let device_id_str = identity.device_id().to_string();
    let public_key_bytes = identity.public_key_bytes();

    // Send approval request with matching authenticated key
    let msg = Message::DeviceApprovalRequest(DeviceApprovalRequest {
        device_id: device_id_str.clone(),
        name: "New Device".to_string(),
        public_key: public_key_bytes.to_vec(),
        reason: Some("Testing pending approval".to_string()),
    });

    let result = router.route(msg, &test_device_id(), Some(&public_key_bytes)).await;
    assert!(result.is_ok());

    // Should be rejected with "pending approval" message
    match result.unwrap() {
        Some(Message::DeviceRejected(rejected)) => {
            assert_eq!(rejected.device_id, device_id_str);
            assert!(rejected.reason.contains("pending"));
            assert!(rejected.retry_allowed);
        }
        _ => panic!("Expected DeviceRejected for new device with require_approval enabled"),
    }

    // Verify device is in pending queue
    let pending = trust_store.list_pending().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].device_name, "New Device");

    // Verify device is NOT in trusted devices yet
    assert!(!trust_store.is_trusted(identity.device_id()).unwrap());
}

/// Test that approving a pending device transitions it to trusted status.
#[tokio::test]
async fn test_approval_to_trusted_transition() {
    use protocol::messages::{DeviceApprovalRequest, Message, SessionCreate};

    let temp_dir = TempDir::new().unwrap();
    let session_manager = Arc::new(MockSessionManager::new());
    let browser_for_transfer = daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
    let file_transfer = Arc::new(
        daemon::files::FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp")),
    );
    let directory_browser = Arc::new(daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));

    // Create trust store with require_approval enabled
    let mut trust_store = TrustStore::new(temp_dir.path().join("trust.json"));
    trust_store.set_require_approval(true);
    let trust_store = Arc::new(trust_store);

    let path_permissions = Arc::new(daemon::files::PathPermissions::new(
        temp_dir.path().join("permissions.json"),
        vec![temp_dir.path().to_path_buf()],
    ));

    // Set up permissions for the device we'll approve
    let identity = protocol::DeviceIdentity::generate();
    let device_id = *identity.device_id();
    let device_perms = daemon::files::DevicePermissions::allow_all_dangerous(device_id);
    path_permissions.set_device_permissions(device_perms).unwrap();

    let router = MessageRouter::new(
        session_manager,
        file_transfer,
        directory_browser,
        trust_store.clone(),
        path_permissions,
    );

    // Step 1: Send approval request (goes to pending)
    let device_id_str = identity.device_id().to_string();
    let public_key_bytes = identity.public_key_bytes();

    let msg = Message::DeviceApprovalRequest(DeviceApprovalRequest {
        device_id: device_id_str.clone(),
        name: "Pending Device".to_string(),
        public_key: public_key_bytes.to_vec(),
        reason: Some("Testing".to_string()),
    });

    let result = router.route(msg, &device_id, Some(&public_key_bytes)).await;
    assert!(result.is_ok());

    // Verify device is pending
    assert!(trust_store.is_pending(&device_id).unwrap());
    assert!(!trust_store.is_trusted(&device_id).unwrap());

    // Step 2: Try to create session - should fail (pending)
    let msg = Message::SessionCreate(SessionCreate {
        cols: 80,
        rows: 24,
        shell: None,
        env: vec![],
        cwd: None,
    });

    let result = router.route(msg, &device_id, None).await;
    assert!(result.is_err());

    // Step 3: Approve the device
    trust_store.approve_pending(&device_id).unwrap();

    // Verify device is now trusted
    assert!(!trust_store.is_pending(&device_id).unwrap());
    assert!(trust_store.is_trusted(&device_id).unwrap());

    // Step 4: Try to create session again - should succeed now
    let msg = Message::SessionCreate(SessionCreate {
        cols: 80,
        rows: 24,
        shell: None,
        env: vec![],
        cwd: None,
    });

    let result = router.route(msg, &device_id, None).await;
    assert!(result.is_ok());

    match result.unwrap() {
        Some(Message::SessionCreated(created)) => {
            assert!(!created.session_id.is_empty());
        }
        _ => panic!("Expected SessionCreated after approval"),
    }
}

/// Test that session access requires a trusted device.
#[tokio::test]
async fn test_session_access_requires_trust() {
    use daemon::devices::TrustLevel;
    use protocol::messages::{Message, SessionCreate};

    let temp_dir = TempDir::new().unwrap();
    let session_manager = Arc::new(MockSessionManager::new());
    let browser_for_transfer = daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
    let file_transfer = Arc::new(
        daemon::files::FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp")),
    );
    let directory_browser = Arc::new(daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
    let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
    let path_permissions = Arc::new(daemon::files::PathPermissions::new(
        temp_dir.path().join("permissions.json"),
        vec![temp_dir.path().to_path_buf()],
    ));

    let router = MessageRouter::new(
        session_manager,
        file_transfer,
        directory_browser,
        trust_store.clone(),
        path_permissions.clone(),
    );

    // Generate a device identity
    let identity = protocol::DeviceIdentity::generate();
    let device_id = *identity.device_id();

    // Register device with Unknown trust level (not trusted yet)
    let device = daemon::devices::TrustedDevice::new_unknown(
        device_id,
        "Untrusted Device".to_string(),
        identity.public_key_bytes(),
    );
    trust_store.add_device(device).unwrap();

    // Set up permissions
    let device_perms = daemon::files::DevicePermissions::allow_all_dangerous(device_id);
    path_permissions.set_device_permissions(device_perms).unwrap();

    // Attempt session create - should fail (Unknown != Trusted)
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
        Err(daemon::router::RouterError::Device(msg)) => {
            assert!(msg.contains("pending"), "Expected 'pending' error, got: {}", msg);
        }
        _ => panic!("Expected RouterError::Device"),
    }

    // Trust the device
    trust_store.set_trust_level(&device_id, TrustLevel::Trusted).unwrap();

    // Now session create should work
    let msg = Message::SessionCreate(SessionCreate {
        cols: 80,
        rows: 24,
        shell: None,
        env: vec![],
        cwd: None,
    });

    let result = router.route(msg, &device_id, None).await;
    assert!(result.is_ok());
}

/// Test that file permissions are enforced correctly.
#[tokio::test]
async fn test_file_permission_enforcement() {
    use protocol::messages::{FileListRequest, Message};

    let temp_dir = TempDir::new().unwrap();

    // Create test directories
    let allowed_dir = temp_dir.path().join("allowed");
    let forbidden_dir = temp_dir.path().join("forbidden");
    std::fs::create_dir_all(&allowed_dir).unwrap();
    std::fs::create_dir_all(&forbidden_dir).unwrap();
    std::fs::write(allowed_dir.join("test.txt"), "allowed content").unwrap();
    std::fs::write(forbidden_dir.join("secret.txt"), "forbidden content").unwrap();

    let session_manager = Arc::new(MockSessionManager::new());
    let browser_for_transfer = daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
    let file_transfer = Arc::new(
        daemon::files::FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp")),
    );
    let directory_browser = Arc::new(daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
    let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
    let path_permissions = Arc::new(daemon::files::PathPermissions::new(
        temp_dir.path().join("permissions.json"),
        vec![temp_dir.path().to_path_buf()], // Global allowed path
    ));

    // Create a trusted device with limited permissions
    let identity = protocol::DeviceIdentity::generate();
    let device_id = *identity.device_id();
    let device = daemon::devices::TrustedDevice::new(
        device_id,
        "Limited Device".to_string(),
        identity.public_key_bytes(),
    );
    trust_store.add_device(device).unwrap();

    // Only allow access to "allowed" directory
    let mut device_perms = daemon::files::DevicePermissions::new(device_id);
    device_perms.add_path(daemon::files::permissions::PathPermission::read_write(allowed_dir.clone()));
    path_permissions.set_device_permissions(device_perms).unwrap();

    let router = MessageRouter::new(
        session_manager,
        file_transfer,
        directory_browser,
        trust_store,
        path_permissions,
    );

    // Access within allowed path - should work
    let msg = Message::FileListRequest(FileListRequest {
        path: allowed_dir.to_string_lossy().to_string(),
        include_hidden: false,
    });

    let result = router.route(msg, &device_id, None).await;
    assert!(result.is_ok());
    match result.unwrap() {
        Some(Message::FileListResponse(response)) => {
            assert_eq!(response.entries.len(), 1); // test.txt
        }
        _ => panic!("Expected FileListResponse"),
    }

    // Access outside allowed path - should fail
    let msg = Message::FileListRequest(FileListRequest {
        path: forbidden_dir.to_string_lossy().to_string(),
        include_hidden: false,
    });

    let result = router.route(msg, &device_id, None).await;
    assert!(result.is_err());
    assert!(matches!(result, Err(daemon::router::RouterError::Permission(_))));
}

/// Test that pending approval requests expire after timeout.
#[tokio::test]
async fn test_approval_timeout() {
    use std::time::Duration;

    let temp_dir = TempDir::new().unwrap();
    let mut trust_store = TrustStore::new(temp_dir.path().join("trust.json"));
    trust_store.set_require_approval(true);

    // Add a pending approval
    let identity = protocol::DeviceIdentity::generate();
    let device_id = *identity.device_id();
    let approval = daemon::devices::PendingApproval::new(
        device_id,
        "Timeout Test Device".to_string(),
        identity.public_key_bytes(),
        None,
    );
    trust_store.add_pending(approval).unwrap();

    // Verify pending
    assert_eq!(trust_store.pending_count().unwrap(), 1);
    assert!(trust_store.is_pending(&device_id).unwrap());

    // With a long timeout, nothing should expire
    let expired = trust_store.cleanup_expired_approvals(3600).unwrap();
    assert!(expired.is_empty());
    assert_eq!(trust_store.pending_count().unwrap(), 1);

    // Wait for approval to age (just over 1 second)
    tokio::time::sleep(Duration::from_millis(1100)).await;

    // With 1 second timeout, it should now expire
    let expired = trust_store.cleanup_expired_approvals(1).unwrap();
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0], device_id);

    // Should be removed from pending
    assert_eq!(trust_store.pending_count().unwrap(), 0);
    assert!(!trust_store.is_pending(&device_id).unwrap());
}

/// Test that public key mismatch is rejected during device approval.
#[tokio::test]
async fn test_public_key_mismatch_rejection() {
    use protocol::messages::{DeviceApprovalRequest, Message};

    let temp_dir = TempDir::new().unwrap();
    let session_manager = Arc::new(MockSessionManager::new());
    let browser_for_transfer = daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]);
    let file_transfer = Arc::new(
        daemon::files::FileTransfer::new(browser_for_transfer, 100 * 1024 * 1024)
            .with_temp_dir(temp_dir.path().join("tmp")),
    );
    let directory_browser = Arc::new(daemon::files::DirectoryBrowser::new(vec![temp_dir.path().to_path_buf()]));
    let trust_store = Arc::new(TrustStore::new(temp_dir.path().join("trust.json")));
    let path_permissions = Arc::new(daemon::files::PathPermissions::new(
        temp_dir.path().join("permissions.json"),
        vec![temp_dir.path().to_path_buf()],
    ));

    let router = MessageRouter::new(
        session_manager,
        file_transfer,
        directory_browser,
        trust_store,
        path_permissions,
    );

    // Generate a device identity
    let identity = protocol::DeviceIdentity::generate();
    let device_id_str = identity.device_id().to_string();

    // Create a request with the device's public key
    let claimed_public_key = identity.public_key_bytes();
    let msg = Message::DeviceApprovalRequest(DeviceApprovalRequest {
        device_id: device_id_str.clone(),
        name: "Spoofed Device".to_string(),
        public_key: claimed_public_key.to_vec(),
        reason: Some("Testing spoofing".to_string()),
    });

    // Provide a DIFFERENT authenticated public key (simulating spoofing attempt)
    let different_key: [u8; 32] = [0xDE; 32];

    let result = router.route(msg, &test_device_id(), Some(&different_key)).await;
    assert!(result.is_err());

    match result {
        Err(daemon::router::RouterError::Auth(msg)) => {
            assert!(msg.contains("mismatch"), "Expected 'mismatch' error, got: {}", msg);
        }
        _ => panic!("Expected RouterError::Auth for public key mismatch"),
    }
}
