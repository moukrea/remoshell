//! Storage module for RemoShell Tauri client.
//!
//! This module provides SQLite-based persistence for:
//! - Paired devices
//! - Connection history
//! - Application settings
//!
//! And secure keychain storage for:
//! - Device secret keys

mod database;
pub mod keychain;

pub use database::{
    ConnectionHistoryEntry, Database, DatabaseError, PairedDevice, Setting, StorageResult,
};

pub use keychain::{
    KeychainBackend, KeychainError, KeychainManager, KeychainResult, decode_secret_key,
    encode_secret_key,
};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_db() -> (Database, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test.db");
        let db = Database::open(&db_path).expect("Failed to open database");
        (db, temp_dir)
    }

    #[test]
    fn test_database_creation() {
        let (db, _temp_dir) = create_test_db();
        // Database should be created and migrations run
        let version = db
            .get_schema_version()
            .expect("Failed to get schema version");
        assert!(version > 0, "Schema version should be greater than 0");
    }

    #[test]
    fn test_add_and_get_paired_device() {
        let (db, _temp_dir) = create_test_db();

        let device = PairedDevice {
            id: "device-123".to_string(),
            name: "Test Device".to_string(),
            public_key: "test-public-key".to_string(),
            created_at: 1234567890,
            last_seen: None,
        };

        db.add_paired_device(&device)
            .expect("Failed to add paired device");

        let retrieved = db
            .get_paired_device("device-123")
            .expect("Failed to get paired device")
            .expect("Device should exist");

        assert_eq!(retrieved.id, device.id);
        assert_eq!(retrieved.name, device.name);
        assert_eq!(retrieved.public_key, device.public_key);
        assert_eq!(retrieved.created_at, device.created_at);
        assert_eq!(retrieved.last_seen, device.last_seen);
    }

    #[test]
    fn test_list_paired_devices() {
        let (db, _temp_dir) = create_test_db();

        let device1 = PairedDevice {
            id: "device-1".to_string(),
            name: "Device 1".to_string(),
            public_key: "key-1".to_string(),
            created_at: 1000,
            last_seen: None,
        };

        let device2 = PairedDevice {
            id: "device-2".to_string(),
            name: "Device 2".to_string(),
            public_key: "key-2".to_string(),
            created_at: 2000,
            last_seen: Some(3000),
        };

        db.add_paired_device(&device1)
            .expect("Failed to add device 1");
        db.add_paired_device(&device2)
            .expect("Failed to add device 2");

        let devices = db.list_paired_devices().expect("Failed to list devices");
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn test_remove_paired_device() {
        let (db, _temp_dir) = create_test_db();

        let device = PairedDevice {
            id: "device-to-remove".to_string(),
            name: "Removable Device".to_string(),
            public_key: "remove-key".to_string(),
            created_at: 1234567890,
            last_seen: None,
        };

        db.add_paired_device(&device)
            .expect("Failed to add paired device");
        db.remove_paired_device("device-to-remove")
            .expect("Failed to remove device");

        let retrieved = db
            .get_paired_device("device-to-remove")
            .expect("Failed to query for removed device");
        assert!(retrieved.is_none(), "Device should have been removed");
    }

    #[test]
    fn test_update_device_last_seen() {
        let (db, _temp_dir) = create_test_db();

        let device = PairedDevice {
            id: "device-seen".to_string(),
            name: "Seen Device".to_string(),
            public_key: "seen-key".to_string(),
            created_at: 1000,
            last_seen: None,
        };

        db.add_paired_device(&device).expect("Failed to add device");
        db.update_device_last_seen("device-seen", 2000)
            .expect("Failed to update last_seen");

        let retrieved = db
            .get_paired_device("device-seen")
            .expect("Failed to get device")
            .expect("Device should exist");

        assert_eq!(retrieved.last_seen, Some(2000));
    }

    #[test]
    fn test_connection_history() {
        let (db, _temp_dir) = create_test_db();

        // First add the device (required due to foreign key constraint)
        let device = PairedDevice {
            id: "device-1".to_string(),
            name: "Test Device".to_string(),
            public_key: "test-key".to_string(),
            created_at: 500,
            last_seen: None,
        };
        db.add_paired_device(&device).expect("Failed to add device");

        db.log_connection("device-1", 1000, Some(2000), true)
            .expect("Failed to log connection");
        db.log_connection("device-1", 3000, None, false)
            .expect("Failed to log connection");

        let history = db
            .get_connection_history("device-1", Some(10))
            .expect("Failed to get connection history");

        assert_eq!(history.len(), 2);
        // Most recent first
        assert_eq!(history[0].connected_at, 3000);
        assert_eq!(history[0].disconnected_at, None);
        assert!(!history[0].successful);
        assert_eq!(history[1].connected_at, 1000);
        assert_eq!(history[1].disconnected_at, Some(2000));
        assert!(history[1].successful);
    }

    #[test]
    fn test_settings() {
        let (db, _temp_dir) = create_test_db();

        db.set_setting("theme", "dark")
            .expect("Failed to set setting");
        db.set_setting("auto_connect", "true")
            .expect("Failed to set setting");

        let theme = db
            .get_setting("theme")
            .expect("Failed to get setting")
            .expect("Setting should exist");
        assert_eq!(theme, "dark");

        let auto_connect = db
            .get_setting("auto_connect")
            .expect("Failed to get setting")
            .expect("Setting should exist");
        assert_eq!(auto_connect, "true");

        // Update existing setting
        db.set_setting("theme", "light")
            .expect("Failed to update setting");
        let theme = db
            .get_setting("theme")
            .expect("Failed to get setting")
            .expect("Setting should exist");
        assert_eq!(theme, "light");
    }

    #[test]
    fn test_list_settings() {
        let (db, _temp_dir) = create_test_db();

        db.set_setting("key1", "value1")
            .expect("Failed to set setting");
        db.set_setting("key2", "value2")
            .expect("Failed to set setting");
        db.set_setting("key3", "value3")
            .expect("Failed to set setting");

        let settings = db.list_settings().expect("Failed to list settings");
        assert_eq!(settings.len(), 3);
    }

    #[test]
    fn test_delete_setting() {
        let (db, _temp_dir) = create_test_db();

        db.set_setting("to_delete", "value")
            .expect("Failed to set setting");
        db.delete_setting("to_delete")
            .expect("Failed to delete setting");

        let result = db
            .get_setting("to_delete")
            .expect("Failed to query deleted setting");
        assert!(result.is_none(), "Setting should have been deleted");
    }

    #[test]
    fn test_nonexistent_setting() {
        let (db, _temp_dir) = create_test_db();

        let result = db
            .get_setting("nonexistent")
            .expect("Failed to query nonexistent setting");
        assert!(result.is_none());
    }

    #[test]
    fn test_database_persists_across_opens() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("persist_test.db");

        // First connection: add data
        {
            let db = Database::open(&db_path).expect("Failed to open database");
            let device = PairedDevice {
                id: "persistent-device".to_string(),
                name: "Persistent".to_string(),
                public_key: "persist-key".to_string(),
                created_at: 1234567890,
                last_seen: None,
            };
            db.add_paired_device(&device).expect("Failed to add device");
            db.set_setting("persistent_setting", "persistent_value")
                .expect("Failed to set setting");
        }

        // Second connection: verify data persists
        {
            let db = Database::open(&db_path).expect("Failed to reopen database");
            let device = db
                .get_paired_device("persistent-device")
                .expect("Failed to get device")
                .expect("Device should persist");
            assert_eq!(device.name, "Persistent");

            let setting = db
                .get_setting("persistent_setting")
                .expect("Failed to get setting")
                .expect("Setting should persist");
            assert_eq!(setting, "persistent_value");
        }
    }
}
