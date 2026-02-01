//! Tauri IPC commands for RemoShell.
//!
//! This module exposes Rust functionality to the JavaScript frontend via Tauri's
//! IPC mechanism using `#[tauri::command]` attributes.
//!
//! Commands are organized into categories:
//! - QUIC connection operations (connect, disconnect, send data)
//! - Device key management (get device keys from keychain)
//! - Paired device storage (get/store/remove via SQLite)
//! - Native notifications

use crate::quic::{ChannelType, ConnectionState, QuicConfig, QuicManager};
use crate::storage::{Database, DatabaseError, KeychainError, KeychainManager, PairedDevice};
use iroh::NodeAddr;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

// ============================================================================
// Error Types
// ============================================================================

/// Unified error type for Tauri commands.
///
/// This error type is serializable so it can be returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandError {
    /// Error code for programmatic handling.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<protocol::error::ProtocolError> for CommandError {
    fn from(e: protocol::error::ProtocolError) -> Self {
        Self {
            code: "PROTOCOL_ERROR".to_string(),
            message: e.to_string(),
        }
    }
}

impl From<DatabaseError> for CommandError {
    fn from(e: DatabaseError) -> Self {
        Self {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        }
    }
}

impl From<KeychainError> for CommandError {
    fn from(e: KeychainError) -> Self {
        Self {
            code: "KEYCHAIN_ERROR".to_string(),
            message: e.to_string(),
        }
    }
}

/// Result type for Tauri commands.
pub type CommandResult<T> = Result<T, CommandError>;

// ============================================================================
// Application State
// ============================================================================

/// Application state managed by Tauri.
///
/// This struct holds shared resources that are accessible across all commands.
pub struct AppState {
    /// The QUIC connection manager.
    pub quic_manager: Arc<RwLock<Option<QuicManager>>>,
    /// The SQLite database for paired devices.
    pub database: Arc<RwLock<Option<Database>>>,
}

impl AppState {
    /// Create a new AppState with no initialized resources.
    pub fn new() -> Self {
        Self {
            quic_manager: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
        }
    }

    /// Initialize the QUIC manager with the given configuration.
    pub async fn init_quic(&self, config: QuicConfig) -> CommandResult<()> {
        let manager = QuicManager::new(config).await?;
        let mut guard = self.quic_manager.write().await;
        *guard = Some(manager);
        Ok(())
    }

    /// Initialize the database at the given path.
    pub fn init_database(&self, path: &str) -> CommandResult<()> {
        let db = Database::open(path)?;
        // Use try_write to avoid deadlock in sync context
        if let Ok(mut guard) = self.database.try_write() {
            *guard = Some(db);
            Ok(())
        } else {
            Err(CommandError {
                code: "DATABASE_LOCK_ERROR".to_string(),
                message: "Failed to acquire database lock".to_string(),
            })
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// QUIC Connection Commands
// ============================================================================

/// Request payload for connecting to a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectRequest {
    /// The node ID (public key) of the peer to connect to.
    pub node_id: String,
    /// Optional relay URL for NAT traversal.
    pub relay_url: Option<String>,
    /// Optional direct addresses for the peer.
    pub direct_addresses: Option<Vec<String>>,
}

/// Response from a successful connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectResponse {
    /// Whether the connection was successful.
    pub connected: bool,
    /// The connected peer's node ID.
    pub peer_node_id: String,
}

/// Connect to a peer via QUIC.
///
/// This command establishes a QUIC connection to a remote peer using their
/// node address. It supports both direct connections and relay-assisted
/// connections for NAT traversal.
#[tauri::command]
pub async fn connect_quic(
    state: tauri::State<'_, AppState>,
    request: ConnectRequest,
) -> CommandResult<ConnectResponse> {
    let guard = state.inner().quic_manager.read().await;
    let manager = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "QUIC manager not initialized".to_string(),
    })?;

    // Parse the node ID
    let node_id: iroh::NodeId = request.node_id.parse().map_err(|e| CommandError {
        code: "INVALID_NODE_ID".to_string(),
        message: format!("Invalid node ID: {}", e),
    })?;

    // Parse the relay URL if provided
    let relay_url = if let Some(url) = &request.relay_url {
        Some(url.parse().map_err(|e| CommandError {
            code: "INVALID_RELAY_URL".to_string(),
            message: format!("Invalid relay URL: {}", e),
        })?)
    } else {
        None
    };

    // Parse direct addresses if provided
    let direct_addrs: Vec<std::net::SocketAddr> = request
        .direct_addresses
        .unwrap_or_default()
        .iter()
        .filter_map(|addr| addr.parse().ok())
        .collect();

    // Create the node address
    let node_addr = NodeAddr::from_parts(node_id, relay_url, direct_addrs);

    // Connect to the peer
    manager.connect(node_addr).await?;

    Ok(ConnectResponse {
        connected: true,
        peer_node_id: node_id.to_string(),
    })
}

/// Response from disconnection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectResponse {
    /// Whether the disconnection was successful.
    pub disconnected: bool,
}

/// Disconnect from the current peer.
///
/// This command gracefully closes the QUIC connection to the remote peer,
/// closing all streams and releasing resources.
#[tauri::command]
pub async fn disconnect_quic(
    state: tauri::State<'_, AppState>,
) -> CommandResult<DisconnectResponse> {
    let guard = state.inner().quic_manager.read().await;
    let manager = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "QUIC manager not initialized".to_string(),
    })?;

    manager.disconnect().await?;

    Ok(DisconnectResponse { disconnected: true })
}

/// Request payload for sending data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendDataRequest {
    /// The channel type to send on (control, terminal, or files).
    pub channel: ChannelType,
    /// The data to send (base64-encoded).
    pub data: String,
}

/// Response from sending data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendDataResponse {
    /// Whether the data was sent successfully.
    pub sent: bool,
    /// Number of bytes sent.
    pub bytes_sent: usize,
}

/// Send data over the QUIC connection.
///
/// This command sends data over a specific channel type (control, terminal,
/// or files). The data should be base64-encoded.
#[tauri::command]
pub async fn send_quic_data(
    state: tauri::State<'_, AppState>,
    request: SendDataRequest,
) -> CommandResult<SendDataResponse> {
    let guard = state.inner().quic_manager.read().await;
    let manager = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "QUIC manager not initialized".to_string(),
    })?;

    // Decode the base64 data
    let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &request.data)
        .map_err(|e| CommandError {
            code: "INVALID_DATA".to_string(),
            message: format!("Invalid base64 data: {}", e),
        })?;

    let bytes_sent = data.len();
    manager.send(request.channel, &data).await?;

    Ok(SendDataResponse {
        sent: true,
        bytes_sent,
    })
}

/// Response for connection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionStatusResponse {
    /// Current connection state.
    pub state: ConnectionState,
    /// Connected peer's node ID, if connected.
    pub peer_node_id: Option<String>,
    /// This client's node ID.
    pub local_node_id: Option<String>,
}

/// Get the current QUIC connection status.
///
/// This command returns the current connection state and peer information.
#[tauri::command]
pub async fn get_connection_status(
    state: tauri::State<'_, AppState>,
) -> CommandResult<ConnectionStatusResponse> {
    let guard = state.inner().quic_manager.read().await;
    let manager = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "QUIC manager not initialized".to_string(),
    })?;

    let conn_state = manager.state().await;
    let peer_node_id = manager.peer_node_id().map(|id| id.to_string());
    let local_node_id = Some(manager.node_id().to_string());

    Ok(ConnectionStatusResponse {
        state: conn_state,
        peer_node_id,
        local_node_id,
    })
}

// ============================================================================
// Device Key Commands
// ============================================================================

/// Response for device keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeysResponse {
    /// The device's secret key (base64-encoded).
    pub secret_key: String,
    /// Whether the key was newly generated.
    pub newly_generated: bool,
}

/// Get or create the device's secret key from the keychain.
///
/// This command retrieves the device's secret key from the system keychain.
/// If no key exists, a new one is generated and stored.
#[tauri::command]
pub fn get_device_keys() -> CommandResult<DeviceKeysResponse> {
    #[cfg(not(test))]
    {
        use crate::storage::keychain::SystemKeychain;
        let manager = KeychainManager::new(SystemKeychain);

        // Check if key exists first
        let existed = manager.has_secret_key();

        let secret_key = manager.get_or_create_secret_key()?;

        Ok(DeviceKeysResponse {
            secret_key,
            newly_generated: !existed,
        })
    }

    #[cfg(test)]
    {
        // In tests, return a mock response
        Ok(DeviceKeysResponse {
            secret_key: "dGVzdC1zZWNyZXQta2V5".to_string(),
            newly_generated: false,
        })
    }
}

/// Check if device keys exist in the keychain.
#[tauri::command]
pub fn has_device_keys() -> CommandResult<bool> {
    #[cfg(not(test))]
    {
        use crate::storage::keychain::SystemKeychain;
        let manager = KeychainManager::new(SystemKeychain);
        Ok(manager.has_secret_key())
    }

    #[cfg(test)]
    {
        Ok(false)
    }
}

// ============================================================================
// Paired Device Commands
// ============================================================================

/// Get all paired devices from the database.
///
/// This command retrieves all devices that have been paired with this client.
#[tauri::command]
pub async fn get_paired_devices(
    state: tauri::State<'_, AppState>,
) -> CommandResult<Vec<PairedDevice>> {
    let guard = state.inner().database.read().await;
    let db = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "Database not initialized".to_string(),
    })?;

    let devices = db.list_paired_devices()?;
    Ok(devices)
}

/// Request payload for storing a paired device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorePairedDeviceRequest {
    /// Unique identifier for the device.
    pub id: String,
    /// Human-readable name for the device.
    pub name: String,
    /// Public key of the device.
    pub public_key: String,
}

/// Store a new paired device in the database.
///
/// This command adds a new device to the paired devices list.
#[tauri::command]
pub async fn store_paired_device(
    state: tauri::State<'_, AppState>,
    request: StorePairedDeviceRequest,
) -> CommandResult<PairedDevice> {
    let guard = state.inner().database.read().await;
    let db = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "Database not initialized".to_string(),
    })?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let device = PairedDevice {
        id: request.id,
        name: request.name,
        public_key: request.public_key,
        created_at: now,
        last_seen: None,
    };

    db.add_paired_device(&device)?;

    Ok(device)
}

/// Response from removing a paired device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveDeviceResponse {
    /// Whether the device was successfully removed.
    pub removed: bool,
}

/// Remove a paired device from the database.
///
/// This command removes a device from the paired devices list. This also
/// removes any connection history for the device due to cascade deletion.
#[tauri::command]
pub async fn remove_paired_device(
    state: tauri::State<'_, AppState>,
    device_id: String,
) -> CommandResult<RemoveDeviceResponse> {
    let guard = state.inner().database.read().await;
    let db = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "Database not initialized".to_string(),
    })?;

    let removed = db.remove_paired_device(&device_id)?;

    Ok(RemoveDeviceResponse { removed })
}

/// Get a specific paired device by ID.
#[tauri::command]
pub async fn get_paired_device(
    state: tauri::State<'_, AppState>,
    device_id: String,
) -> CommandResult<Option<PairedDevice>> {
    let guard = state.inner().database.read().await;
    let db = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "Database not initialized".to_string(),
    })?;

    let device = db.get_paired_device(&device_id)?;
    Ok(device)
}

/// Update the last_seen timestamp for a device.
#[tauri::command]
pub async fn update_device_last_seen(
    state: tauri::State<'_, AppState>,
    device_id: String,
) -> CommandResult<bool> {
    let guard = state.inner().database.read().await;
    let db = guard.as_ref().ok_or_else(|| CommandError {
        code: "NOT_INITIALIZED".to_string(),
        message: "Database not initialized".to_string(),
    })?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let updated = db.update_device_last_seen(&device_id, now)?;
    Ok(updated)
}

// ============================================================================
// Notification Commands
// ============================================================================

/// Request payload for showing a notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationRequest {
    /// The notification title.
    pub title: String,
    /// The notification body text.
    pub body: String,
    /// Optional icon name or path.
    pub icon: Option<String>,
}

/// Response from showing a notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationResponse {
    /// Whether the notification was shown successfully.
    pub shown: bool,
}

/// Show a native notification.
///
/// This command displays a native system notification. On platforms that
/// support it, notifications may include actions and rich content.
#[tauri::command]
pub async fn show_native_notification(
    app: tauri::AppHandle,
    request: NotificationRequest,
) -> CommandResult<NotificationResponse> {
    // Use Tauri's notification API
    use tauri::Manager;

    // Get the notification plugin
    #[cfg(feature = "notification")]
    {
        use tauri_plugin_notification::NotificationExt;

        let mut notification = app.notification().builder();
        notification = notification.title(&request.title).body(&request.body);

        if let Some(icon) = &request.icon {
            notification = notification.icon(icon);
        }

        notification.show().map_err(|e| CommandError {
            code: "NOTIFICATION_ERROR".to_string(),
            message: format!("Failed to show notification: {}", e),
        })?;

        Ok(NotificationResponse { shown: true })
    }

    // Fallback when notification feature is not enabled
    #[cfg(not(feature = "notification"))]
    {
        // Log that we would show a notification
        let _ = app; // Suppress unused warning
        tracing::info!(
            "Notification requested (feature disabled): {} - {}",
            request.title,
            request.body
        );

        Ok(NotificationResponse { shown: false })
    }
}

// ============================================================================
// Initialization Command
// ============================================================================

/// Request payload for initializing the application.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitRequest {
    /// Path to the database file.
    pub database_path: String,
    /// Optional relay URL for QUIC connections.
    pub relay_url: Option<String>,
}

/// Response from initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResponse {
    /// Whether initialization was successful.
    pub initialized: bool,
    /// The local node ID for QUIC connections.
    pub node_id: String,
}

/// Initialize the application state.
///
/// This command initializes the database and QUIC manager. It should be
/// called once when the application starts.
#[tauri::command]
pub async fn initialize_app(
    state: tauri::State<'_, AppState>,
    request: InitRequest,
) -> CommandResult<InitResponse> {
    // Initialize the database
    state.inner().init_database(&request.database_path)?;

    // Create QUIC configuration
    let config = if let Some(url) = &request.relay_url {
        let relay_url = url.parse().map_err(|e| CommandError {
            code: "INVALID_RELAY_URL".to_string(),
            message: format!("Invalid relay URL: {}", e),
        })?;
        QuicConfig::with_relay(relay_url)
    } else {
        QuicConfig::default()
    };

    // Initialize the QUIC manager
    state.inner().init_quic(config).await?;

    // Get the local node ID
    let guard = state.inner().quic_manager.read().await;
    let manager = guard.as_ref().ok_or_else(|| CommandError {
        code: "INIT_FAILED".to_string(),
        message: "QUIC manager initialization failed".to_string(),
    })?;
    let node_id = manager.node_id().to_string();

    Ok(InitResponse {
        initialized: true,
        node_id,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_error_display() {
        let error = CommandError {
            code: "TEST_ERROR".to_string(),
            message: "Test error message".to_string(),
        };
        assert_eq!(format!("{}", error), "TEST_ERROR: Test error message");
    }

    #[test]
    fn test_connect_request_serialization() {
        let request = ConnectRequest {
            node_id: "abc123".to_string(),
            relay_url: Some("https://relay.example.com".to_string()),
            direct_addresses: Some(vec!["127.0.0.1:9000".to_string()]),
        };
        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(json.contains("abc123"));
        assert!(json.contains("relay.example.com"));
    }

    #[test]
    fn test_send_data_request_serialization() {
        let request = SendDataRequest {
            channel: ChannelType::Control,
            data: "SGVsbG8gV29ybGQ=".to_string(),
        };
        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(json.contains("Control"));
        assert!(json.contains("SGVsbG8gV29ybGQ="));
    }

    #[test]
    fn test_notification_request_serialization() {
        let request = NotificationRequest {
            title: "Test Title".to_string(),
            body: "Test Body".to_string(),
            icon: Some("icon.png".to_string()),
        };
        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(json.contains("Test Title"));
        assert!(json.contains("Test Body"));
        assert!(json.contains("icon.png"));
    }

    #[test]
    fn test_app_state_creation() {
        let state = AppState::new();
        // Verify state is created with None values
        assert!(state.inner().quic_manager.try_read().is_ok());
        assert!(state.inner().database.try_read().is_ok());
    }

    #[test]
    fn test_store_paired_device_request_serialization() {
        let request = StorePairedDeviceRequest {
            id: "device-1".to_string(),
            name: "Test Device".to_string(),
            public_key: "public-key-123".to_string(),
        };
        let json = serde_json::to_string(&request).expect("Failed to serialize");
        assert!(json.contains("device-1"));
        assert!(json.contains("Test Device"));
        assert!(json.contains("public-key-123"));
    }

    #[test]
    fn test_device_keys_response_serialization() {
        let response = DeviceKeysResponse {
            secret_key: "dGVzdC1rZXk=".to_string(),
            newly_generated: true,
        };
        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("dGVzdC1rZXk="));
        assert!(json.contains("true"));
    }

    #[test]
    fn test_connection_status_response_serialization() {
        let response = ConnectionStatusResponse {
            state: ConnectionState::Connected,
            peer_node_id: Some("peer-123".to_string()),
            local_node_id: Some("local-456".to_string()),
        };
        let json = serde_json::to_string(&response).expect("Failed to serialize");
        assert!(json.contains("Connected"));
        assert!(json.contains("peer-123"));
        assert!(json.contains("local-456"));
    }
}
