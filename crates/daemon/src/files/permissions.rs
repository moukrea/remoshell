//! Per-device path permission management.
//!
//! This module provides fine-grained path access control for each device.
//! Devices can have different access levels (read, write, full) for different
//! paths, allowing administrators to restrict what files each device can access.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{Context, Result};
use protocol::DeviceId;
use serde::{Deserialize, Serialize};

/// Permission level for a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    /// No access.
    #[default]
    None,
    /// Read-only access (list directory, download files).
    Read,
    /// Read and write access (also upload files).
    ReadWrite,
    /// Full access (also delete files, create directories).
    Full,
}

impl PermissionLevel {
    /// Check if this level allows reading.
    pub fn can_read(&self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite | Self::Full)
    }

    /// Check if this level allows writing.
    pub fn can_write(&self) -> bool {
        matches!(self, Self::ReadWrite | Self::Full)
    }

    /// Check if this level allows deleting.
    pub fn can_delete(&self) -> bool {
        matches!(self, Self::Full)
    }
}

/// Path permission configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathPermission {
    /// The path this permission applies to.
    pub path: PathBuf,
    /// The permission level.
    pub level: PermissionLevel,
    /// Whether this permission applies recursively to subdirectories.
    pub recursive: bool,
}

impl PathPermission {
    /// Create a new path permission.
    pub fn new(path: PathBuf, level: PermissionLevel, recursive: bool) -> Self {
        Self {
            path,
            level,
            recursive,
        }
    }

    /// Create a read-only permission.
    pub fn read_only(path: PathBuf) -> Self {
        Self::new(path, PermissionLevel::Read, true)
    }

    /// Create a read-write permission.
    pub fn read_write(path: PathBuf) -> Self {
        Self::new(path, PermissionLevel::ReadWrite, true)
    }

    /// Create a full access permission.
    pub fn full_access(path: PathBuf) -> Self {
        Self::new(path, PermissionLevel::Full, true)
    }
}

/// Permissions for a specific device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePermissions {
    /// The device ID.
    pub device_id: DeviceId,
    /// Path permissions for this device.
    pub paths: Vec<PathPermission>,
    /// Default permission level for paths not explicitly listed.
    pub default_level: PermissionLevel,
}

impl DevicePermissions {
    /// Create new permissions for a device.
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            paths: Vec::new(),
            default_level: PermissionLevel::None,
        }
    }

    /// Creates permissions that allow ALL filesystem operations.
    ///
    /// # Security Warning
    /// This grants unrestricted access. Only use for:
    /// - Development/testing
    /// - Explicitly configured "full trust" scenarios
    ///
    /// For production, prefer `with_allowed_paths()`.
    pub fn allow_all_dangerous(device_id: DeviceId) -> Self {
        Self {
            device_id,
            paths: Vec::new(),
            default_level: PermissionLevel::Full,
        }
    }

    /// Creates permissions with explicit allowed paths.
    ///
    /// Only the specified paths (and their subdirectories) are accessible.
    /// Everything else is denied by default.
    pub fn with_allowed_paths(device_id: DeviceId, paths: Vec<PathBuf>) -> Self {
        Self {
            device_id,
            paths: paths
                .into_iter()
                .map(|p| PathPermission::new(p, PermissionLevel::ReadWrite, true))
                .collect(),
            default_level: PermissionLevel::None,
        }
    }

    /// Add a path permission.
    pub fn add_path(&mut self, permission: PathPermission) {
        self.paths.push(permission);
    }

    /// Set the default permission level.
    pub fn set_default_level(&mut self, level: PermissionLevel) {
        self.default_level = level;
    }

    /// Get the permission level for a specific path.
    ///
    /// Checks all path permissions and returns the most specific match.
    pub fn get_permission(&self, path: &Path) -> PermissionLevel {
        // Canonicalize the path for comparison
        let canonical = match fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => {
                // For paths that don't exist yet, try to canonicalize parent
                if let Some(parent) = path.parent() {
                    if let Ok(parent_canonical) = fs::canonicalize(parent) {
                        if let Some(file_name) = path.file_name() {
                            parent_canonical.join(file_name)
                        } else {
                            return self.default_level;
                        }
                    } else {
                        return self.default_level;
                    }
                } else {
                    return self.default_level;
                }
            }
        };

        // Find the most specific matching permission
        let mut best_match: Option<&PathPermission> = None;
        let mut best_match_len = 0;

        for perm in &self.paths {
            // Canonicalize the permission path
            let perm_canonical = match fs::canonicalize(&perm.path) {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Check if the path matches
            let matches = if perm.recursive {
                canonical.starts_with(&perm_canonical)
            } else {
                canonical == perm_canonical
            };

            if matches {
                // Use the longest (most specific) match
                let perm_len = perm_canonical.as_os_str().len();
                if perm_len > best_match_len {
                    best_match = Some(perm);
                    best_match_len = perm_len;
                }
            }
        }

        best_match.map(|p| p.level).unwrap_or(self.default_level)
    }

    /// Check if the device can read the given path.
    pub fn can_read(&self, path: &Path) -> bool {
        self.get_permission(path).can_read()
    }

    /// Check if the device can write to the given path.
    pub fn can_write(&self, path: &Path) -> bool {
        self.get_permission(path).can_write()
    }

    /// Check if the device can delete the given path.
    pub fn can_delete(&self, path: &Path) -> bool {
        self.get_permission(path).can_delete()
    }

    /// Get all paths this device can access for reading.
    pub fn readable_paths(&self) -> Vec<&PathBuf> {
        self.paths
            .iter()
            .filter(|p| p.level.can_read())
            .map(|p| &p.path)
            .collect()
    }

    /// Get all paths this device can access for writing.
    pub fn writable_paths(&self) -> Vec<&PathBuf> {
        self.paths
            .iter()
            .filter(|p| p.level.can_write())
            .map(|p| &p.path)
            .collect()
    }
}

/// Permission store wrapper for serialization.
#[derive(Debug, Serialize, Deserialize)]
struct PermissionStoreData {
    /// Version of the store format.
    version: u32,
    /// Device permissions.
    devices: Vec<DevicePermissions>,
}

impl Default for PermissionStoreData {
    fn default() -> Self {
        Self {
            version: 1,
            devices: Vec::new(),
        }
    }
}

/// Thread-safe store for device path permissions.
///
/// Manages per-device path access permissions with persistence.
pub struct PathPermissions {
    /// Path to the permissions file.
    path: PathBuf,
    /// Device permissions, keyed by device ID.
    devices: RwLock<HashMap<DeviceId, DevicePermissions>>,
    /// Global allowed paths (from config). All devices are limited to these.
    global_allowed_paths: Vec<PathBuf>,
}

impl PathPermissions {
    /// Create a new permissions store.
    pub fn new<P: AsRef<Path>>(path: P, global_allowed_paths: Vec<PathBuf>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            devices: RwLock::new(HashMap::new()),
            global_allowed_paths,
        }
    }

    /// Create a permissions store with default path.
    pub fn with_default_path(global_allowed_paths: Vec<PathBuf>) -> Self {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("remoshell")
            .join("permissions.json");
        Self::new(path, global_allowed_paths)
    }

    /// Load permissions from the file.
    pub fn load(&self) -> Result<()> {
        if !self.path.exists() {
            tracing::debug!(
                "Permissions file not found at {:?}, starting empty",
                self.path
            );
            return Ok(());
        }

        let contents = fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read permissions file: {}", self.path.display()))?;

        let data: PermissionStoreData = serde_json::from_str(&contents).with_context(|| {
            format!("Failed to parse permissions file: {}", self.path.display())
        })?;

        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on permissions store"))?;

        devices.clear();
        for device_perms in data.devices {
            devices.insert(device_perms.device_id, device_perms);
        }

        tracing::info!(
            "Loaded permissions for {} devices from {:?}",
            devices.len(),
            self.path
        );
        Ok(())
    }

    /// Save permissions to the file.
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create permissions directory: {}",
                    parent.display()
                )
            })?;
        }

        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on permissions store"))?;

        let data = PermissionStoreData {
            version: 1,
            devices: devices.values().cloned().collect(),
        };

        let contents =
            serde_json::to_string_pretty(&data).context("Failed to serialize permissions")?;

        // Atomic write
        let temp_path = self.path.with_extension("json.tmp");
        fs::write(&temp_path, &contents).with_context(|| {
            format!(
                "Failed to write temp permissions file: {}",
                temp_path.display()
            )
        })?;

        fs::rename(&temp_path, &self.path).with_context(|| {
            format!(
                "Failed to rename temp permissions file {} to {}",
                temp_path.display(),
                self.path.display()
            )
        })?;

        tracing::debug!(
            "Saved permissions for {} devices to {:?}",
            devices.len(),
            self.path
        );
        Ok(())
    }

    /// Set permissions for a device.
    pub fn set_device_permissions(&self, permissions: DevicePermissions) -> Result<()> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on permissions store"))?;

        tracing::info!(
            "Setting permissions for device {} with {} paths",
            permissions.device_id,
            permissions.paths.len()
        );

        devices.insert(permissions.device_id, permissions);
        Ok(())
    }

    /// Get permissions for a device.
    pub fn get_device_permissions(
        &self,
        device_id: &DeviceId,
    ) -> Result<Option<DevicePermissions>> {
        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on permissions store"))?;

        Ok(devices.get(device_id).cloned())
    }

    /// Remove permissions for a device.
    pub fn remove_device_permissions(
        &self,
        device_id: &DeviceId,
    ) -> Result<Option<DevicePermissions>> {
        let mut devices = self
            .devices
            .write()
            .map_err(|_| anyhow::anyhow!("Failed to acquire write lock on permissions store"))?;

        Ok(devices.remove(device_id))
    }

    /// Check if a device can read a path.
    ///
    /// Also checks against global allowed paths.
    pub fn can_device_read(&self, device_id: &DeviceId, path: &Path) -> Result<bool> {
        // First check global allowed paths
        if !self.is_within_global_paths(path) {
            return Ok(false);
        }

        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on permissions store"))?;

        Ok(devices
            .get(device_id)
            .map(|p| p.can_read(path))
            .unwrap_or(false))
    }

    /// Check if a device can write to a path.
    ///
    /// Also checks against global allowed paths.
    pub fn can_device_write(&self, device_id: &DeviceId, path: &Path) -> Result<bool> {
        // First check global allowed paths
        if !self.is_within_global_paths(path) {
            return Ok(false);
        }

        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on permissions store"))?;

        Ok(devices
            .get(device_id)
            .map(|p| p.can_write(path))
            .unwrap_or(false))
    }

    /// Check if a device can delete a path.
    ///
    /// Also checks against global allowed paths.
    pub fn can_device_delete(&self, device_id: &DeviceId, path: &Path) -> Result<bool> {
        // First check global allowed paths
        if !self.is_within_global_paths(path) {
            return Ok(false);
        }

        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on permissions store"))?;

        Ok(devices
            .get(device_id)
            .map(|p| p.can_delete(path))
            .unwrap_or(false))
    }

    /// Check if a path is within global allowed paths.
    fn is_within_global_paths(&self, path: &Path) -> bool {
        // If no global paths are set, allow all
        if self.global_allowed_paths.is_empty() {
            return true;
        }

        // Canonicalize the path
        let canonical = match fs::canonicalize(path) {
            Ok(p) => p,
            Err(_) => {
                // For non-existent paths, try parent
                if let Some(parent) = path.parent() {
                    if let Ok(parent_canonical) = fs::canonicalize(parent) {
                        if let Some(file_name) = path.file_name() {
                            parent_canonical.join(file_name)
                        } else {
                            return false;
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        };

        for allowed in &self.global_allowed_paths {
            if let Ok(allowed_canonical) = fs::canonicalize(allowed) {
                if canonical.starts_with(&allowed_canonical) {
                    return true;
                }
            }
        }

        false
    }

    /// List all devices with permissions.
    pub fn list_devices(&self) -> Result<Vec<DeviceId>> {
        let devices = self
            .devices
            .read()
            .map_err(|_| anyhow::anyhow!("Failed to acquire read lock on permissions store"))?;

        Ok(devices.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_device_id() -> DeviceId {
        let identity = protocol::DeviceIdentity::generate();
        *identity.device_id()
    }

    #[test]
    fn test_permission_level_checks() {
        assert!(!PermissionLevel::None.can_read());
        assert!(!PermissionLevel::None.can_write());
        assert!(!PermissionLevel::None.can_delete());

        assert!(PermissionLevel::Read.can_read());
        assert!(!PermissionLevel::Read.can_write());
        assert!(!PermissionLevel::Read.can_delete());

        assert!(PermissionLevel::ReadWrite.can_read());
        assert!(PermissionLevel::ReadWrite.can_write());
        assert!(!PermissionLevel::ReadWrite.can_delete());

        assert!(PermissionLevel::Full.can_read());
        assert!(PermissionLevel::Full.can_write());
        assert!(PermissionLevel::Full.can_delete());
    }

    #[test]
    fn test_device_permissions_default() {
        let device_id = create_test_device_id();
        let perms = DevicePermissions::new(device_id);

        // Default level is None
        assert_eq!(perms.default_level, PermissionLevel::None);
    }

    #[test]
    fn test_device_permissions_allow_all_dangerous() {
        let device_id = create_test_device_id();
        let perms = DevicePermissions::allow_all_dangerous(device_id);

        assert_eq!(perms.default_level, PermissionLevel::Full);
    }

    #[test]
    fn test_device_permissions_with_allowed_paths() {
        let device_id = create_test_device_id();
        let paths = vec![PathBuf::from("/home/user"), PathBuf::from("/tmp")];
        let perms = DevicePermissions::with_allowed_paths(device_id, paths);

        assert_eq!(perms.default_level, PermissionLevel::None);
        assert_eq!(perms.paths.len(), 2);
        assert_eq!(perms.paths[0].level, PermissionLevel::ReadWrite);
        assert!(perms.paths[0].recursive);
    }

    #[test]
    fn test_device_permissions_specific_path() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();

        let device_id = create_test_device_id();
        let mut perms = DevicePermissions::new(device_id);
        perms.add_path(PathPermission::read_only(temp_dir.path().to_path_buf()));

        assert!(perms.can_read(&file_path));
        assert!(!perms.can_write(&file_path));
        assert!(!perms.can_delete(&file_path));
    }

    #[test]
    fn test_device_permissions_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();
        let file_path = subdir.join("nested.txt");
        fs::write(&file_path, "nested").unwrap();

        let device_id = create_test_device_id();
        let mut perms = DevicePermissions::new(device_id);
        perms.add_path(PathPermission::read_write(temp_dir.path().to_path_buf()));

        // Should apply recursively
        assert!(perms.can_read(&file_path));
        assert!(perms.can_write(&file_path));
    }

    #[test]
    fn test_device_permissions_non_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "test").unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir_all(&subdir).unwrap();

        let device_id = create_test_device_id();
        let mut perms = DevicePermissions::new(device_id);
        perms.add_path(PathPermission::new(
            temp_dir.path().to_path_buf(),
            PermissionLevel::ReadWrite,
            false, // Non-recursive
        ));

        // Should only apply to the exact path
        assert!(perms.can_read(temp_dir.path()));
        // The file inside won't match since it's a different path
        // and we're not recursive
        assert!(!perms.can_read(&subdir));
    }

    #[test]
    fn test_device_permissions_most_specific_wins() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("restricted");
        fs::create_dir_all(&subdir).unwrap();
        let restricted_file = subdir.join("secret.txt");
        fs::write(&restricted_file, "secret").unwrap();
        let normal_file = temp_dir.path().join("normal.txt");
        fs::write(&normal_file, "normal").unwrap();

        let device_id = create_test_device_id();
        let mut perms = DevicePermissions::new(device_id);
        // Allow full access to temp dir
        perms.add_path(PathPermission::full_access(temp_dir.path().to_path_buf()));
        // But read-only for restricted subdir
        perms.add_path(PathPermission::read_only(subdir.clone()));

        // Normal file should have full access
        assert!(perms.can_read(&normal_file));
        assert!(perms.can_write(&normal_file));
        assert!(perms.can_delete(&normal_file));

        // Restricted file should be read-only
        assert!(perms.can_read(&restricted_file));
        assert!(!perms.can_write(&restricted_file));
        assert!(!perms.can_delete(&restricted_file));
    }

    #[test]
    fn test_path_permissions_store_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let perms_path = temp_dir.path().join("permissions.json");

        let device_id = create_test_device_id();
        let mut device_perms = DevicePermissions::new(device_id);
        device_perms.add_path(PathPermission::read_only(PathBuf::from("/home/user")));

        // Create and save
        let store = PathPermissions::new(&perms_path, vec![]);
        store.set_device_permissions(device_perms.clone()).unwrap();
        store.save().unwrap();

        // Create new store and load
        let store2 = PathPermissions::new(&perms_path, vec![]);
        store2.load().unwrap();

        let loaded = store2.get_device_permissions(&device_id).unwrap().unwrap();
        assert_eq!(loaded.device_id, device_id);
        assert_eq!(loaded.paths.len(), 1);
        assert_eq!(loaded.paths[0].path, PathBuf::from("/home/user"));
        assert_eq!(loaded.paths[0].level, PermissionLevel::Read);
    }

    #[test]
    fn test_path_permissions_global_boundary() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_dir = temp_dir.path().join("allowed");
        let forbidden_dir = temp_dir.path().join("forbidden");
        fs::create_dir_all(&allowed_dir).unwrap();
        fs::create_dir_all(&forbidden_dir).unwrap();
        let allowed_file = allowed_dir.join("file.txt");
        let forbidden_file = forbidden_dir.join("file.txt");
        fs::write(&allowed_file, "allowed").unwrap();
        fs::write(&forbidden_file, "forbidden").unwrap();

        let device_id = create_test_device_id();
        let device_perms = DevicePermissions::allow_all_dangerous(device_id);

        let store = PathPermissions::new(
            temp_dir.path().join("perms.json"),
            vec![allowed_dir.clone()], // Only allow "allowed" directory globally
        );
        store.set_device_permissions(device_perms).unwrap();

        // Can read allowed file
        assert!(store.can_device_read(&device_id, &allowed_file).unwrap());

        // Cannot read forbidden file despite device having "allow all"
        assert!(!store.can_device_read(&device_id, &forbidden_file).unwrap());
    }

    #[test]
    fn test_path_permissions_remove_device() {
        let temp_dir = TempDir::new().unwrap();

        let device_id = create_test_device_id();
        let device_perms = DevicePermissions::allow_all_dangerous(device_id);

        let store = PathPermissions::new(temp_dir.path().join("perms.json"), vec![]);
        store.set_device_permissions(device_perms).unwrap();

        // Device should exist
        assert!(store.get_device_permissions(&device_id).unwrap().is_some());

        // Remove device
        let removed = store.remove_device_permissions(&device_id).unwrap();
        assert!(removed.is_some());

        // Device should no longer exist
        assert!(store.get_device_permissions(&device_id).unwrap().is_none());
    }

    #[test]
    fn test_path_permissions_list_devices() {
        let temp_dir = TempDir::new().unwrap();

        let device1 = create_test_device_id();
        let device2 = create_test_device_id();

        let store = PathPermissions::new(temp_dir.path().join("perms.json"), vec![]);
        store
            .set_device_permissions(DevicePermissions::new(device1))
            .unwrap();
        store
            .set_device_permissions(DevicePermissions::new(device2))
            .unwrap();

        let devices = store.list_devices().unwrap();
        assert_eq!(devices.len(), 2);
        assert!(devices.contains(&device1));
        assert!(devices.contains(&device2));
    }

    #[test]
    fn test_readable_writable_paths() {
        let device_id = create_test_device_id();
        let mut perms = DevicePermissions::new(device_id);

        perms.add_path(PathPermission::read_only(PathBuf::from("/home/user/docs")));
        perms.add_path(PathPermission::read_write(PathBuf::from(
            "/home/user/uploads",
        )));
        perms.add_path(PathPermission::new(
            PathBuf::from("/home/user/restricted"),
            PermissionLevel::None,
            true,
        ));

        let readable = perms.readable_paths();
        assert_eq!(readable.len(), 2);

        let writable = perms.writable_paths();
        assert_eq!(writable.len(), 1);
        assert_eq!(writable[0], &PathBuf::from("/home/user/uploads"));
    }

    #[test]
    fn test_permission_level_serialization() {
        let level = PermissionLevel::ReadWrite;
        let json = serde_json::to_string(&level).unwrap();
        assert_eq!(json, "\"readwrite\"");

        let restored: PermissionLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, level);
    }

    #[test]
    fn test_path_permission_serialization() {
        let perm = PathPermission::new(PathBuf::from("/home/user"), PermissionLevel::Read, true);

        let json = serde_json::to_string_pretty(&perm).unwrap();
        let restored: PathPermission = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.path, perm.path);
        assert_eq!(restored.level, perm.level);
        assert_eq!(restored.recursive, perm.recursive);
    }

    #[test]
    fn test_device_permissions_for_nonexistent_path() {
        let temp_dir = TempDir::new().unwrap();
        let device_id = create_test_device_id();
        let mut perms = DevicePermissions::new(device_id);
        perms.add_path(PathPermission::read_write(temp_dir.path().to_path_buf()));

        // Check permission for a file that doesn't exist yet
        let new_file = temp_dir.path().join("new_file.txt");
        assert!(perms.can_write(&new_file));
    }
}
