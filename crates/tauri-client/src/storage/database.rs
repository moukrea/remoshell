//! SQLite database implementation for RemoShell Tauri client.
//!
//! This module provides the core database functionality including:
//! - Schema management and migrations
//! - Paired device persistence
//! - Connection history logging
//! - Settings storage

use rusqlite::{Connection, OptionalExtension, Result as SqliteResult, params};
use std::path::Path;
use thiserror::Error;

/// Errors that can occur during database operations.
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// SQLite error.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    /// Migration error.
    #[error("Migration error: {0}")]
    Migration(String),

    /// Path error.
    #[error("Invalid database path: {0}")]
    InvalidPath(String),
}

/// Result type for database operations.
pub type StorageResult<T> = Result<T, DatabaseError>;

/// A paired device stored in the database.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PairedDevice {
    /// Unique identifier for the device.
    pub id: String,
    /// Human-readable name of the device.
    pub name: String,
    /// Public key of the device for authentication.
    pub public_key: String,
    /// Unix timestamp when the device was first paired.
    pub created_at: i64,
    /// Unix timestamp when the device was last seen (connected).
    pub last_seen: Option<i64>,
}

/// A connection history entry.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConnectionHistoryEntry {
    /// Unique identifier for the history entry.
    pub id: i64,
    /// Device ID this connection was with.
    pub device_id: String,
    /// Unix timestamp when the connection was established.
    pub connected_at: i64,
    /// Unix timestamp when the connection was closed.
    pub disconnected_at: Option<i64>,
    /// Whether the connection was successful.
    pub successful: bool,
}

/// A setting stored in the database.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Setting {
    /// The setting key.
    pub key: String,
    /// The setting value.
    pub value: String,
}

/// Current schema version.
#[cfg(test)]
const CURRENT_SCHEMA_VERSION: i32 = 1;

/// Database wrapper providing all storage operations.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create a database at the given path.
    ///
    /// If the database doesn't exist, it will be created and migrations will be run.
    /// If it exists, any pending migrations will be applied.
    pub fn open<P: AsRef<Path>>(path: P) -> StorageResult<Self> {
        let path = path.as_ref();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    DatabaseError::InvalidPath(format!(
                        "Failed to create directory {}: {}",
                        parent.display(),
                        e
                    ))
                })?;
            }
        }

        let conn = Connection::open(path)?;

        // Enable foreign keys
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let mut db = Self { conn };
        db.run_migrations()?;

        Ok(db)
    }

    /// Open an in-memory database (useful for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;

        let mut db = Self { conn };
        db.run_migrations()?;

        Ok(db)
    }

    /// Get the current schema version.
    pub fn get_schema_version(&self) -> StorageResult<i32> {
        let version: i32 = self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        Ok(version)
    }

    /// Set the schema version.
    #[allow(dead_code)]
    fn set_schema_version(&self, version: i32) -> StorageResult<()> {
        self.conn
            .execute(&format!("PRAGMA user_version = {}", version), [])?;
        Ok(())
    }

    /// Run all pending migrations.
    fn run_migrations(&mut self) -> StorageResult<()> {
        let current_version = self.get_schema_version()?;

        if current_version < 1 {
            self.migrate_v1()?;
        }

        // Future migrations would be added here:
        // if current_version < 2 {
        //     self.migrate_v2()?;
        // }

        Ok(())
    }

    /// Migration to version 1: Initial schema.
    fn migrate_v1(&mut self) -> StorageResult<()> {
        let tx = self.conn.transaction()?;

        // Create paired_devices table
        tx.execute(
            r#"
            CREATE TABLE IF NOT EXISTS paired_devices (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                public_key TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_seen INTEGER
            )
            "#,
            [],
        )?;

        // Create connection_history table
        tx.execute(
            r#"
            CREATE TABLE IF NOT EXISTS connection_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                device_id TEXT NOT NULL,
                connected_at INTEGER NOT NULL,
                disconnected_at INTEGER,
                successful INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY (device_id) REFERENCES paired_devices(id) ON DELETE CASCADE
            )
            "#,
            [],
        )?;

        // Create index on connection_history for faster queries
        tx.execute(
            r#"
            CREATE INDEX IF NOT EXISTS idx_connection_history_device_id
            ON connection_history(device_id)
            "#,
            [],
        )?;

        tx.execute(
            r#"
            CREATE INDEX IF NOT EXISTS idx_connection_history_connected_at
            ON connection_history(connected_at DESC)
            "#,
            [],
        )?;

        // Create settings table
        tx.execute(
            r#"
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )
            "#,
            [],
        )?;

        // Update schema version
        tx.execute(&format!("PRAGMA user_version = {}", 1), [])?;

        tx.commit()?;
        Ok(())
    }

    // =========================================================================
    // Paired Devices
    // =========================================================================

    /// Add a new paired device.
    pub fn add_paired_device(&self, device: &PairedDevice) -> StorageResult<()> {
        self.conn.execute(
            r#"
            INSERT INTO paired_devices (id, name, public_key, created_at, last_seen)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                device.id,
                device.name,
                device.public_key,
                device.created_at,
                device.last_seen
            ],
        )?;
        Ok(())
    }

    /// Get a paired device by ID.
    pub fn get_paired_device(&self, id: &str) -> StorageResult<Option<PairedDevice>> {
        let result = self
            .conn
            .query_row(
                r#"
                SELECT id, name, public_key, created_at, last_seen
                FROM paired_devices
                WHERE id = ?1
                "#,
                params![id],
                |row| {
                    Ok(PairedDevice {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        public_key: row.get(2)?,
                        created_at: row.get(3)?,
                        last_seen: row.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(result)
    }

    /// List all paired devices.
    pub fn list_paired_devices(&self) -> StorageResult<Vec<PairedDevice>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, name, public_key, created_at, last_seen
            FROM paired_devices
            ORDER BY created_at DESC
            "#,
        )?;

        let devices = stmt
            .query_map([], |row| {
                Ok(PairedDevice {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    public_key: row.get(2)?,
                    created_at: row.get(3)?,
                    last_seen: row.get(4)?,
                })
            })?
            .collect::<SqliteResult<Vec<_>>>()?;

        Ok(devices)
    }

    /// Remove a paired device by ID.
    ///
    /// This also removes all connection history for the device due to ON DELETE CASCADE.
    pub fn remove_paired_device(&self, id: &str) -> StorageResult<bool> {
        let rows_affected = self
            .conn
            .execute("DELETE FROM paired_devices WHERE id = ?1", params![id])?;
        Ok(rows_affected > 0)
    }

    /// Update the last_seen timestamp for a device.
    pub fn update_device_last_seen(&self, id: &str, last_seen: i64) -> StorageResult<bool> {
        let rows_affected = self.conn.execute(
            "UPDATE paired_devices SET last_seen = ?1 WHERE id = ?2",
            params![last_seen, id],
        )?;
        Ok(rows_affected > 0)
    }

    /// Update a paired device's name.
    pub fn update_device_name(&self, id: &str, name: &str) -> StorageResult<bool> {
        let rows_affected = self.conn.execute(
            "UPDATE paired_devices SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(rows_affected > 0)
    }

    // =========================================================================
    // Connection History
    // =========================================================================

    /// Log a connection event.
    pub fn log_connection(
        &self,
        device_id: &str,
        connected_at: i64,
        disconnected_at: Option<i64>,
        successful: bool,
    ) -> StorageResult<i64> {
        self.conn.execute(
            r#"
            INSERT INTO connection_history (device_id, connected_at, disconnected_at, successful)
            VALUES (?1, ?2, ?3, ?4)
            "#,
            params![device_id, connected_at, disconnected_at, successful as i32],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Update a connection's disconnected_at timestamp.
    pub fn update_connection_disconnected(
        &self,
        connection_id: i64,
        disconnected_at: i64,
    ) -> StorageResult<bool> {
        let rows_affected = self.conn.execute(
            "UPDATE connection_history SET disconnected_at = ?1 WHERE id = ?2",
            params![disconnected_at, connection_id],
        )?;
        Ok(rows_affected > 0)
    }

    /// Get connection history for a device.
    ///
    /// Returns entries ordered by connected_at descending (most recent first).
    /// If `limit` is provided, only that many entries are returned.
    pub fn get_connection_history(
        &self,
        device_id: &str,
        limit: Option<u32>,
    ) -> StorageResult<Vec<ConnectionHistoryEntry>> {
        let query = match limit {
            Some(l) => format!(
                r#"
                SELECT id, device_id, connected_at, disconnected_at, successful
                FROM connection_history
                WHERE device_id = ?1
                ORDER BY connected_at DESC
                LIMIT {}
                "#,
                l
            ),
            None => r#"
                SELECT id, device_id, connected_at, disconnected_at, successful
                FROM connection_history
                WHERE device_id = ?1
                ORDER BY connected_at DESC
                "#
            .to_string(),
        };

        let mut stmt = self.conn.prepare(&query)?;
        let entries = stmt
            .query_map(params![device_id], |row| {
                Ok(ConnectionHistoryEntry {
                    id: row.get(0)?,
                    device_id: row.get(1)?,
                    connected_at: row.get(2)?,
                    disconnected_at: row.get(3)?,
                    successful: row.get::<_, i32>(4)? != 0,
                })
            })?
            .collect::<SqliteResult<Vec<_>>>()?;

        Ok(entries)
    }

    /// Get all connection history entries.
    ///
    /// Returns entries ordered by connected_at descending (most recent first).
    /// If `limit` is provided, only that many entries are returned.
    pub fn get_all_connection_history(
        &self,
        limit: Option<u32>,
    ) -> StorageResult<Vec<ConnectionHistoryEntry>> {
        let query = match limit {
            Some(l) => format!(
                r#"
                SELECT id, device_id, connected_at, disconnected_at, successful
                FROM connection_history
                ORDER BY connected_at DESC
                LIMIT {}
                "#,
                l
            ),
            None => r#"
                SELECT id, device_id, connected_at, disconnected_at, successful
                FROM connection_history
                ORDER BY connected_at DESC
                "#
            .to_string(),
        };

        let mut stmt = self.conn.prepare(&query)?;
        let entries = stmt
            .query_map([], |row| {
                Ok(ConnectionHistoryEntry {
                    id: row.get(0)?,
                    device_id: row.get(1)?,
                    connected_at: row.get(2)?,
                    disconnected_at: row.get(3)?,
                    successful: row.get::<_, i32>(4)? != 0,
                })
            })?
            .collect::<SqliteResult<Vec<_>>>()?;

        Ok(entries)
    }

    // =========================================================================
    // Settings
    // =========================================================================

    /// Get a setting value by key.
    pub fn get_setting(&self, key: &str) -> StorageResult<Option<String>> {
        let result = self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()?;
        Ok(result)
    }

    /// Set a setting value (insert or update).
    pub fn set_setting(&self, key: &str, value: &str) -> StorageResult<()> {
        self.conn.execute(
            r#"
            INSERT INTO settings (key, value) VALUES (?1, ?2)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![key, value],
        )?;
        Ok(())
    }

    /// Delete a setting by key.
    pub fn delete_setting(&self, key: &str) -> StorageResult<bool> {
        let rows_affected = self
            .conn
            .execute("DELETE FROM settings WHERE key = ?1", params![key])?;
        Ok(rows_affected > 0)
    }

    /// List all settings.
    pub fn list_settings(&self) -> StorageResult<Vec<Setting>> {
        let mut stmt = self
            .conn
            .prepare("SELECT key, value FROM settings ORDER BY key")?;

        let settings = stmt
            .query_map([], |row| {
                Ok(Setting {
                    key: row.get(0)?,
                    value: row.get(1)?,
                })
            })?
            .collect::<SqliteResult<Vec<_>>>()?;

        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_database() {
        let db = Database::open_in_memory().expect("Failed to create in-memory database");
        let version = db
            .get_schema_version()
            .expect("Failed to get schema version");
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn test_cascade_delete() {
        let db = Database::open_in_memory().expect("Failed to create database");

        // Add a device
        let device = PairedDevice {
            id: "cascade-test".to_string(),
            name: "Cascade Test".to_string(),
            public_key: "cascade-key".to_string(),
            created_at: 1000,
            last_seen: None,
        };
        db.add_paired_device(&device).expect("Failed to add device");

        // Add some connection history
        db.log_connection("cascade-test", 1000, Some(2000), true)
            .expect("Failed to log connection");
        db.log_connection("cascade-test", 3000, Some(4000), true)
            .expect("Failed to log connection");

        // Verify history exists
        let history = db
            .get_connection_history("cascade-test", None)
            .expect("Failed to get history");
        assert_eq!(history.len(), 2);

        // Delete the device - should cascade delete history
        db.remove_paired_device("cascade-test")
            .expect("Failed to remove device");

        // Verify history is also deleted
        let history = db
            .get_connection_history("cascade-test", None)
            .expect("Failed to get history");
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_update_device_name() {
        let db = Database::open_in_memory().expect("Failed to create database");

        let device = PairedDevice {
            id: "rename-test".to_string(),
            name: "Original Name".to_string(),
            public_key: "rename-key".to_string(),
            created_at: 1000,
            last_seen: None,
        };
        db.add_paired_device(&device).expect("Failed to add device");

        db.update_device_name("rename-test", "New Name")
            .expect("Failed to update name");

        let updated = db
            .get_paired_device("rename-test")
            .expect("Failed to get device")
            .expect("Device should exist");
        assert_eq!(updated.name, "New Name");
    }
}
