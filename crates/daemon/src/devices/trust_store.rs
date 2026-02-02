//! Persistent trusted device storage.
//!
//! This module provides a thread-safe store for managing trusted devices.
//! Devices can be added, removed, and queried. The store persists to JSON
//! at `~/.config/remoshell/trusted_devices.json`.

use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};
use protocol::DeviceId;
use serde::{Deserialize, Serialize};

/// Trust level for a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrustLevel {
    /// Unknown device - not yet trusted or revoked.
    #[default]
    Unknown,
    /// Trusted device - allowed to connect.
    Trusted,
    /// Revoked device - explicitly denied connection.
    Revoked,
}

/// A trusted device entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedDevice {
    /// The unique device identifier.
    pub device_id: DeviceId,
    /// Human-readable name for the device.
    pub name: String,
    /// The device's public key (32 bytes, Ed25519).
    #[serde(with = "public_key_serde")]
    pub public_key: [u8; 32],
    /// The trust level of this device.
    pub trust_level: TrustLevel,
    /// Timestamp when the device was first seen.
    pub first_seen: SystemTime,
    /// Timestamp when the device was last seen.
    pub last_seen: SystemTime,
}

impl TrustedDevice {
    /// Creates a new trusted device entry.
    pub fn new(device_id: DeviceId, name: String, public_key: [u8; 32]) -> Self {
        let now = SystemTime::now();
        Self {
            device_id,
            name,
            public_key,
            trust_level: TrustLevel::Trusted,
            first_seen: now,
            last_seen: now,
        }
    }

    /// Creates a new device entry with Unknown trust level.
    pub fn new_unknown(device_id: DeviceId, name: String, public_key: [u8; 32]) -> Self {
        let now = SystemTime::now();
        Self {
            device_id,
            name,
            public_key,
            trust_level: TrustLevel::Unknown,
            first_seen: now,
            last_seen: now,
        }
    }
}

/// A device pending manual approval.
///
/// When `require_approval` is enabled, unknown devices are added to the pending
/// queue instead of being immediately rejected. An administrator can then
/// approve or reject these devices.
#[derive(Debug, Clone)]
pub struct PendingApproval {
    /// The unique device identifier.
    pub device_id: DeviceId,
    /// Human-readable name for the device.
    pub device_name: String,
    /// The device's public key (32 bytes, Ed25519).
    pub public_key: [u8; 32],
    /// When the approval request was created.
    pub requested_at: Instant,
    /// The remote address of the device (if available).
    pub remote_addr: Option<SocketAddr>,
}

impl PendingApproval {
    /// Creates a new pending approval request.
    pub fn new(
        device_id: DeviceId,
        device_name: String,
        public_key: [u8; 32],
        remote_addr: Option<SocketAddr>,
    ) -> Self {
        Self {
            device_id,
            device_name,
            public_key,
            requested_at: Instant::now(),
            remote_addr,
        }
    }

    /// Returns how many seconds have elapsed since the approval was requested.
    pub fn age_secs(&self) -> u64 {
        self.requested_at.elapsed().as_secs()
    }
}

/// Serde support for public key (serializes as base64).
mod public_key_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(key: &[u8; 32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(key);
        encoded.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
    where
        D: Deserializer<'de>,
    {
        use base64::Engine;
        let encoded: String = Deserialize::deserialize(deserializer)?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .map_err(serde::de::Error::custom)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom(format!(
                "invalid public key length: expected 32, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(arr)
    }
}

/// Wrapper for serializing the device store.
#[derive(Debug, Serialize, Deserialize)]
struct TrustStoreData {
    /// Version of the store format (for future migrations).
    version: u32,
    /// The devices in the store.
    devices: Vec<TrustedDevice>,
}

impl Default for TrustStoreData {
    fn default() -> Self {
        Self {
            version: 1,
            devices: Vec::new(),
        }
    }
}

/// Thread-safe store for trusted devices.
///
/// The store uses a `RwLock<HashMap>` for concurrent access and persists
/// to JSON for durability across restarts.
pub struct TrustStore {
    /// The path to the JSON file.
    path: PathBuf,
    /// The devices, keyed by device ID.
    devices: RwLock<HashMap<DeviceId, TrustedDevice>>,
    /// Devices pending manual approval.
    pending_approvals: RwLock<HashMap<DeviceId, PendingApproval>>,
    /// Whether manual approval is required for new devices.
    require_approval: bool,
}

impl TrustStore {
    /// Creates a new trust store that will persist to the given path.
    ///
    /// This does not load the file; call `load()` to read existing data.
    /// By default, `require_approval` is set to `false`.
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            devices: RwLock::new(HashMap::new()),
            pending_approvals: RwLock::new(HashMap::new()),
            require_approval: false,
        }
    }

    /// Creates a trust store using the default path.
    ///
    /// The default path is `~/.config/remoshell/trusted_devices.json`.
    pub fn with_default_path() -> Self {
        Self::new(default_trust_store_path())
    }

    /// Sets whether manual approval is required for new devices.
    pub fn set_require_approval(&mut self, require: bool) {
        self.require_approval = require;
    }

    /// Returns whether manual approval is required for new devices.
    pub fn require_approval(&self) -> bool {
        self.require_approval
    }

    /// Returns the path to the trust store file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the trust store from the JSON file.
    ///
    /// If the file does not exist, the store will be empty.
    /// If the file exists but is invalid, returns an error.
    pub fn load(&self) -> Result<()> {
        if !self.path.exists() {
            tracing::debug!(
                "Trust store file not found at {:?}, starting empty",
                self.path
            );
            return Ok(());
        }

        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read trust store: {}", self.path.display()))?;

        let data: TrustStoreData = serde_json::from_str(&contents)
            .with_context(|| format!("Failed to parse trust store: {}", self.path.display()))?;

        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on trust store"))?;

        devices.clear();
        for device in data.devices {
            devices.insert(device.device_id, device);
        }

        tracing::info!(
            "Loaded {} trusted devices from {:?}",
            devices.len(),
            self.path
        );
        Ok(())
    }

    /// Saves the trust store to the JSON file.
    ///
    /// Uses atomic write (write to temp file, then rename) to prevent corruption.
    /// Creates parent directories if they don't exist.
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create trust store directory: {}",
                    parent.display()
                )
            })?;
        }

        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on trust store"))?;

        let data = TrustStoreData {
            version: 1,
            devices: devices.values().cloned().collect(),
        };

        let contents =
            serde_json::to_string_pretty(&data).context("Failed to serialize trust store")?;

        // Atomic write: write to temp file, then rename
        let temp_path = self.path.with_extension("json.tmp");
        fs::write(&temp_path, &contents).with_context(|| {
            format!("Failed to write temp trust store: {}", temp_path.display())
        })?;

        fs::rename(&temp_path, &self.path).with_context(|| {
            format!(
                "Failed to rename temp trust store {} to {}",
                temp_path.display(),
                self.path.display()
            )
        })?;

        tracing::debug!("Saved {} trusted devices to {:?}", devices.len(), self.path);
        Ok(())
    }

    /// Adds a device to the trust store.
    ///
    /// If the device already exists, it will be updated.
    /// Does not automatically save; call `save()` after making changes.
    pub fn add_device(&self, device: TrustedDevice) -> Result<()> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on trust store"))?;

        tracing::info!(
            "Adding device {} ({}) with trust level {:?}",
            device.device_id,
            device.name,
            device.trust_level
        );

        devices.insert(device.device_id, device);
        Ok(())
    }

    /// Removes a device from the trust store.
    ///
    /// Returns the removed device if it existed.
    /// Does not automatically save; call `save()` after making changes.
    pub fn remove_device(&self, device_id: &DeviceId) -> Result<Option<TrustedDevice>> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on trust store"))?;

        let removed = devices.remove(device_id);
        if let Some(ref device) = removed {
            tracing::info!("Removed device {} ({})", device.device_id, device.name);
        }
        Ok(removed)
    }

    /// Checks if a device is trusted.
    ///
    /// Returns `true` only if the device exists and has `TrustLevel::Trusted`.
    /// Revoked and unknown devices return `false`.
    pub fn is_trusted(&self, device_id: &DeviceId) -> Result<bool> {
        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on trust store"))?;

        Ok(devices
            .get(device_id)
            .map(|d| d.trust_level == TrustLevel::Trusted)
            .unwrap_or(false))
    }

    /// Gets a device by its ID.
    ///
    /// Returns `None` if the device is not in the store.
    pub fn get_device(&self, device_id: &DeviceId) -> Result<Option<TrustedDevice>> {
        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on trust store"))?;

        Ok(devices.get(device_id).cloned())
    }

    /// Updates the trust level of a device.
    ///
    /// Returns an error if the device doesn't exist.
    pub fn set_trust_level(&self, device_id: &DeviceId, level: TrustLevel) -> Result<()> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on trust store"))?;

        let device = devices
            .get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device {} not found in trust store", device_id))?;

        tracing::info!(
            "Changing trust level for device {} ({}) from {:?} to {:?}",
            device.device_id,
            device.name,
            device.trust_level,
            level
        );

        device.trust_level = level;
        Ok(())
    }

    /// Updates the last seen timestamp for a device.
    ///
    /// Returns an error if the device doesn't exist.
    pub fn update_last_seen(&self, device_id: &DeviceId) -> Result<()> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on trust store"))?;

        let device = devices
            .get_mut(device_id)
            .ok_or_else(|| anyhow::anyhow!("Device {} not found in trust store", device_id))?;

        device.last_seen = SystemTime::now();
        Ok(())
    }

    /// Lists all devices in the store.
    pub fn list_devices(&self) -> Result<Vec<TrustedDevice>> {
        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on trust store"))?;

        Ok(devices.values().cloned().collect())
    }

    /// Returns the number of devices in the store.
    pub fn len(&self) -> Result<usize> {
        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on trust store"))?;
        Ok(devices.len())
    }

    /// Returns true if the store is empty.
    pub fn is_empty(&self) -> Result<bool> {
        Ok(self.len()? == 0)
    }

    // =========================================================================
    // Pending Approval Methods
    // =========================================================================

    /// Adds a device to the pending approvals queue.
    ///
    /// If the device is already pending, it will be updated with the new request.
    pub fn add_pending(&self, approval: PendingApproval) -> Result<()> {
        let mut pending = self
            .pending_approvals
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on pending approvals"))?;

        tracing::info!(
            "Adding device {} ({}) to pending approvals queue",
            approval.device_id,
            approval.device_name
        );

        pending.insert(approval.device_id, approval);
        Ok(())
    }

    /// Gets a pending approval by device ID.
    ///
    /// Returns `None` if the device is not in the pending queue.
    pub fn get_pending(&self, device_id: &DeviceId) -> Result<Option<PendingApproval>> {
        let pending = self
            .pending_approvals
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on pending approvals"))?;

        Ok(pending.get(device_id).cloned())
    }

    /// Lists all pending approvals.
    pub fn list_pending(&self) -> Result<Vec<PendingApproval>> {
        let pending = self
            .pending_approvals
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on pending approvals"))?;

        Ok(pending.values().cloned().collect())
    }

    /// Approves a pending device, moving it to the trusted devices list.
    ///
    /// Returns an error if the device is not in the pending queue.
    pub fn approve_pending(&self, device_id: &DeviceId) -> Result<()> {
        // First, remove from pending
        let pending_device = {
            let mut pending = self
                .pending_approvals
                .write()
                .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on pending approvals"))?;

            pending
                .remove(device_id)
                .ok_or_else(|| anyhow::anyhow!("Device {} not found in pending approvals", device_id))?
        };

        // Create a trusted device from the pending approval
        let trusted_device = TrustedDevice::new(
            pending_device.device_id,
            pending_device.device_name.clone(),
            pending_device.public_key,
        );

        // Add to trusted devices
        self.add_device(trusted_device)?;

        tracing::info!(
            "Approved device {} ({})",
            device_id,
            pending_device.device_name
        );

        Ok(())
    }

    /// Rejects a pending device, removing it from the pending queue.
    ///
    /// Returns `true` if the device was in the pending queue and removed,
    /// `false` if it was not found.
    pub fn reject_pending(&self, device_id: &DeviceId) -> Result<bool> {
        let mut pending = self
            .pending_approvals
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on pending approvals"))?;

        let removed = pending.remove(device_id);

        if let Some(ref device) = removed {
            tracing::info!(
                "Rejected device {} ({})",
                device_id,
                device.device_name
            );
        }

        Ok(removed.is_some())
    }

    /// Checks if a device is in the pending approvals queue.
    pub fn is_pending(&self, device_id: &DeviceId) -> Result<bool> {
        let pending = self
            .pending_approvals
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on pending approvals"))?;

        Ok(pending.contains_key(device_id))
    }

    /// Returns the number of pending approvals.
    pub fn pending_count(&self) -> Result<usize> {
        let pending = self
            .pending_approvals
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on pending approvals"))?;

        Ok(pending.len())
    }

    /// Removes expired pending approvals (older than the given timeout in seconds).
    ///
    /// Returns a list of device IDs that were expired and removed.
    pub fn cleanup_expired_approvals(&self, timeout_secs: u64) -> Result<Vec<DeviceId>> {
        let mut pending = self
            .pending_approvals
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on pending approvals"))?;

        let mut expired = Vec::new();

        pending.retain(|device_id, approval| {
            if approval.age_secs() >= timeout_secs {
                expired.push(*device_id);
                false // Remove from map
            } else {
                true // Keep in map
            }
        });

        if !expired.is_empty() {
            tracing::info!("Removed {} expired pending approvals", expired.len());
        }

        Ok(expired)
    }
}

/// Returns the default trust store path.
///
/// The default path is `~/.config/remoshell/trusted_devices.json`.
pub fn default_trust_store_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("remoshell")
        .join("trusted_devices.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_device(name: &str) -> TrustedDevice {
        let identity = protocol::DeviceIdentity::generate();
        TrustedDevice::new(
            *identity.device_id(),
            name.to_string(),
            identity.public_key_bytes(),
        )
    }

    fn create_test_store(temp_dir: &TempDir) -> TrustStore {
        let path = temp_dir.path().join("trusted_devices.json");
        TrustStore::new(path)
    }

    #[test]
    fn test_trust_level_default() {
        let level = TrustLevel::default();
        assert_eq!(level, TrustLevel::Unknown);
    }

    #[test]
    fn test_trust_level_serialization() {
        let trusted = TrustLevel::Trusted;
        let json = serde_json::to_string(&trusted).unwrap();
        assert_eq!(json, "\"trusted\"");

        let revoked = TrustLevel::Revoked;
        let json = serde_json::to_string(&revoked).unwrap();
        assert_eq!(json, "\"revoked\"");

        let unknown = TrustLevel::Unknown;
        let json = serde_json::to_string(&unknown).unwrap();
        assert_eq!(json, "\"unknown\"");
    }

    #[test]
    fn test_trust_level_deserialization() {
        let trusted: TrustLevel = serde_json::from_str("\"trusted\"").unwrap();
        assert_eq!(trusted, TrustLevel::Trusted);

        let revoked: TrustLevel = serde_json::from_str("\"revoked\"").unwrap();
        assert_eq!(revoked, TrustLevel::Revoked);

        let unknown: TrustLevel = serde_json::from_str("\"unknown\"").unwrap();
        assert_eq!(unknown, TrustLevel::Unknown);
    }

    #[test]
    fn test_trusted_device_new() {
        let device = create_test_device("Test Device");
        assert_eq!(device.name, "Test Device");
        assert_eq!(device.trust_level, TrustLevel::Trusted);
        assert!(device.first_seen <= SystemTime::now());
        assert!(device.last_seen <= SystemTime::now());
    }

    #[test]
    fn test_trusted_device_new_unknown() {
        let identity = protocol::DeviceIdentity::generate();
        let device = TrustedDevice::new_unknown(
            *identity.device_id(),
            "Unknown Device".to_string(),
            identity.public_key_bytes(),
        );
        assert_eq!(device.name, "Unknown Device");
        assert_eq!(device.trust_level, TrustLevel::Unknown);
    }

    #[test]
    fn test_trusted_device_serialization() {
        let device = create_test_device("Serialization Test");
        let json = serde_json::to_string_pretty(&device).unwrap();
        let restored: TrustedDevice = serde_json::from_str(&json).unwrap();

        assert_eq!(device.device_id, restored.device_id);
        assert_eq!(device.name, restored.name);
        assert_eq!(device.public_key, restored.public_key);
        assert_eq!(device.trust_level, restored.trust_level);
    }

    #[test]
    fn test_trust_store_new() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn test_trust_store_add_device() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let device = create_test_device("Add Test");
        let device_id = device.device_id;

        store.add_device(device).unwrap();
        assert_eq!(store.len().unwrap(), 1);
        assert!(store.is_trusted(&device_id).unwrap());
    }

    #[test]
    fn test_trust_store_get_device() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let device = create_test_device("Get Test");
        let device_id = device.device_id;

        store.add_device(device.clone()).unwrap();
        let retrieved = store.get_device(&device_id).unwrap().unwrap();

        assert_eq!(retrieved.device_id, device.device_id);
        assert_eq!(retrieved.name, device.name);
    }

    #[test]
    fn test_trust_store_get_nonexistent_device() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let identity = protocol::DeviceIdentity::generate();

        let retrieved = store.get_device(identity.device_id()).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_trust_store_remove_device() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let device = create_test_device("Remove Test");
        let device_id = device.device_id;

        store.add_device(device).unwrap();
        assert_eq!(store.len().unwrap(), 1);

        let removed = store.remove_device(&device_id).unwrap();
        assert!(removed.is_some());
        assert_eq!(store.len().unwrap(), 0);
        assert!(!store.is_trusted(&device_id).unwrap());
    }

    #[test]
    fn test_trust_store_remove_nonexistent_device() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let identity = protocol::DeviceIdentity::generate();

        let removed = store.remove_device(identity.device_id()).unwrap();
        assert!(removed.is_none());
    }

    #[test]
    fn test_trust_store_is_trusted() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let device = create_test_device("Trust Test");
        let device_id = device.device_id;

        // Not in store -> not trusted
        assert!(!store.is_trusted(&device_id).unwrap());

        // Add with Trusted level
        store.add_device(device).unwrap();
        assert!(store.is_trusted(&device_id).unwrap());

        // Change to Revoked
        store
            .set_trust_level(&device_id, TrustLevel::Revoked)
            .unwrap();
        assert!(!store.is_trusted(&device_id).unwrap());

        // Change to Unknown
        store
            .set_trust_level(&device_id, TrustLevel::Unknown)
            .unwrap();
        assert!(!store.is_trusted(&device_id).unwrap());

        // Change back to Trusted
        store
            .set_trust_level(&device_id, TrustLevel::Trusted)
            .unwrap();
        assert!(store.is_trusted(&device_id).unwrap());
    }

    #[test]
    fn test_trust_store_revoked_device_rejected() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let device = create_test_device("Revoked Test");
        let device_id = device.device_id;

        store.add_device(device).unwrap();
        store
            .set_trust_level(&device_id, TrustLevel::Revoked)
            .unwrap();

        // Revoked device should not be trusted
        assert!(!store.is_trusted(&device_id).unwrap());

        // But it should still exist in the store
        let retrieved = store.get_device(&device_id).unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().trust_level, TrustLevel::Revoked);
    }

    #[test]
    fn test_trust_store_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("trusted_devices.json");

        // Create store and add devices
        let store1 = TrustStore::new(&path);
        let device1 = create_test_device("Device 1");
        let device2 = create_test_device("Device 2");
        let device1_id = device1.device_id;
        let device2_id = device2.device_id;

        store1.add_device(device1).unwrap();
        store1.add_device(device2).unwrap();
        store1.save().unwrap();

        // Create new store and load
        let store2 = TrustStore::new(&path);
        store2.load().unwrap();

        assert_eq!(store2.len().unwrap(), 2);
        assert!(store2.is_trusted(&device1_id).unwrap());
        assert!(store2.is_trusted(&device2_id).unwrap());
    }

    #[test]
    fn test_trust_store_persistence_across_restarts() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("trusted_devices.json");

        // First "session"
        {
            let store = TrustStore::new(&path);
            let device = create_test_device("Persistent Device");
            store.add_device(device).unwrap();
            store.save().unwrap();
        }

        // Verify file exists
        assert!(path.exists());

        // Second "session" (simulating restart)
        {
            let store = TrustStore::new(&path);
            store.load().unwrap();
            assert_eq!(store.len().unwrap(), 1);

            let devices = store.list_devices().unwrap();
            assert_eq!(devices[0].name, "Persistent Device");
        }
    }

    #[test]
    fn test_trust_store_load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);

        // Should not error, just start empty
        store.load().unwrap();
        assert!(store.is_empty().unwrap());
    }

    #[test]
    fn test_trust_store_atomic_write() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("trusted_devices.json");
        let temp_path = path.with_extension("json.tmp");

        let store = TrustStore::new(&path);
        let device = create_test_device("Atomic Test");
        store.add_device(device).unwrap();
        store.save().unwrap();

        // After save, the temp file should not exist (renamed to final)
        assert!(!temp_path.exists());
        assert!(path.exists());
    }

    #[test]
    fn test_trust_store_creates_parent_directories() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir
            .path()
            .join("nested")
            .join("dirs")
            .join("trusted_devices.json");

        let store = TrustStore::new(&path);
        let device = create_test_device("Nested Test");
        store.add_device(device).unwrap();
        store.save().unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_trust_store_update_last_seen() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);
        let device = create_test_device("Last Seen Test");
        let device_id = device.device_id;
        let first_seen = device.first_seen;

        store.add_device(device).unwrap();

        // Wait a tiny bit
        std::thread::sleep(std::time::Duration::from_millis(10));

        store.update_last_seen(&device_id).unwrap();

        let updated = store.get_device(&device_id).unwrap().unwrap();
        assert_eq!(updated.first_seen, first_seen);
        assert!(updated.last_seen > first_seen);
    }

    #[test]
    fn test_trust_store_list_devices() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);

        let device1 = create_test_device("Device A");
        let device2 = create_test_device("Device B");
        let device3 = create_test_device("Device C");

        store.add_device(device1).unwrap();
        store.add_device(device2).unwrap();
        store.add_device(device3).unwrap();

        let devices = store.list_devices().unwrap();
        assert_eq!(devices.len(), 3);

        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"Device A"));
        assert!(names.contains(&"Device B"));
        assert!(names.contains(&"Device C"));
    }

    #[test]
    fn test_trust_store_update_existing_device() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);

        let mut device = create_test_device("Original Name");
        let device_id = device.device_id;

        store.add_device(device.clone()).unwrap();

        // Update the device with new name
        device.name = "Updated Name".to_string();
        store.add_device(device).unwrap();

        // Should still only have one device
        assert_eq!(store.len().unwrap(), 1);

        let retrieved = store.get_device(&device_id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
    }

    #[test]
    fn test_trust_store_concurrent_read_access() {
        use std::sync::Arc;
        use std::thread;

        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("trusted_devices.json");
        let store = Arc::new(TrustStore::new(path));

        // Add some devices
        for i in 0..10 {
            let device = create_test_device(&format!("Device {}", i));
            store.add_device(device).unwrap();
        }

        // Spawn multiple reader threads
        let handles: Vec<_> = (0..5)
            .map(|_| {
                let store = Arc::clone(&store);
                thread::spawn(move || {
                    for _ in 0..100 {
                        let count = store.len().unwrap();
                        assert_eq!(count, 10);
                        let _ = store.list_devices().unwrap();
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_default_trust_store_path() {
        let path = default_trust_store_path();
        assert!(path.to_string_lossy().contains("remoshell"));
        assert!(path.to_string_lossy().contains("trusted_devices.json"));
    }

    #[test]
    fn test_public_key_serialization() {
        let identity = protocol::DeviceIdentity::generate();
        let device = TrustedDevice::new(
            *identity.device_id(),
            "Key Test".to_string(),
            identity.public_key_bytes(),
        );

        let json = serde_json::to_string(&device).unwrap();
        let restored: TrustedDevice = serde_json::from_str(&json).unwrap();

        assert_eq!(device.public_key, restored.public_key);
    }

    #[test]
    fn test_store_data_version() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("trusted_devices.json");

        let store = TrustStore::new(&path);
        let device = create_test_device("Version Test");
        store.add_device(device).unwrap();
        store.save().unwrap();

        // Read the raw JSON and verify version
        let contents = fs::read_to_string(&path).unwrap();
        let data: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(data["version"], 1);
    }

    #[test]
    fn test_cleanup_expired_approvals() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);

        // Add a pending approval
        let identity = protocol::DeviceIdentity::generate();
        let approval = PendingApproval::new(
            *identity.device_id(),
            "Test Device".to_string(),
            identity.public_key_bytes(),
            None,
        );
        let device_id = approval.device_id;

        store.add_pending(approval).unwrap();
        assert_eq!(store.pending_count().unwrap(), 1);

        // With a long timeout, nothing should be expired
        let expired = store.cleanup_expired_approvals(3600).unwrap();
        assert!(expired.is_empty());
        assert_eq!(store.pending_count().unwrap(), 1);

        // Wait just over 1 second for the approval to age
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // With a 1 second timeout, it should now be expired
        let expired = store.cleanup_expired_approvals(1).unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], device_id);
        assert_eq!(store.pending_count().unwrap(), 0);
    }

    #[test]
    fn test_cleanup_expired_approvals_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);

        // Add multiple pending approvals
        for i in 0..5 {
            let identity = protocol::DeviceIdentity::generate();
            let approval = PendingApproval::new(
                *identity.device_id(),
                format!("Device {}", i),
                identity.public_key_bytes(),
                None,
            );
            store.add_pending(approval).unwrap();
        }
        assert_eq!(store.pending_count().unwrap(), 5);

        // Wait just over 1 second for all approvals to age
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // With 1 second timeout, all should expire
        let expired = store.cleanup_expired_approvals(1).unwrap();
        assert_eq!(expired.len(), 5);
        assert_eq!(store.pending_count().unwrap(), 0);
    }

    #[test]
    fn test_cleanup_expired_approvals_returns_device_ids() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);

        let identity = protocol::DeviceIdentity::generate();
        let expected_device_id = *identity.device_id();
        let approval = PendingApproval::new(
            expected_device_id,
            "Test Device".to_string(),
            identity.public_key_bytes(),
            None,
        );

        store.add_pending(approval).unwrap();

        // Wait just over 1 second
        std::thread::sleep(std::time::Duration::from_millis(1100));

        // Expire with 1 second timeout
        let expired = store.cleanup_expired_approvals(1).unwrap();

        // Verify the returned device ID matches
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0], expected_device_id);
    }

    #[test]
    fn test_cleanup_expired_approvals_respects_timeout() {
        let temp_dir = TempDir::new().unwrap();
        let store = create_test_store(&temp_dir);

        // Add a pending approval
        let identity = protocol::DeviceIdentity::generate();
        let approval = PendingApproval::new(
            *identity.device_id(),
            "Test Device".to_string(),
            identity.public_key_bytes(),
            None,
        );

        store.add_pending(approval).unwrap();
        assert_eq!(store.pending_count().unwrap(), 1);

        // With a very long timeout, nothing should be expired even after waiting
        std::thread::sleep(std::time::Duration::from_millis(100));
        let expired = store.cleanup_expired_approvals(3600).unwrap();
        assert!(expired.is_empty());
        assert_eq!(store.pending_count().unwrap(), 1);
    }
}
