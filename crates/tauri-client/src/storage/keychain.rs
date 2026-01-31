//! Keychain integration for secure storage of device secret keys.
//!
//! This module provides cross-platform keychain access using the `keyring` crate:
//! - macOS: Keychain
//! - Windows: Credential Manager
//! - Linux: Secret Service (via D-Bus)
//! - Android: Android Keystore (when compiled for Android)
//! - iOS: iOS Keychain (when compiled for iOS)

use thiserror::Error;

/// The service name used for keychain entries.
const SERVICE_NAME: &str = "remoshell";

/// The default key name for the device secret.
const DEFAULT_KEY_NAME: &str = "device_secret_key";

/// Errors that can occur during keychain operations.
#[derive(Debug, Error)]
pub enum KeychainError {
    /// The requested key was not found in the keychain.
    #[error("Key not found in keychain: {0}")]
    NotFound(String),

    /// Access to the keychain was denied.
    #[error("Keychain access denied: {0}")]
    AccessDenied(String),

    /// The keychain service is unavailable.
    #[error("Keychain service unavailable: {0}")]
    ServiceUnavailable(String),

    /// An error occurred while encoding/decoding the key.
    #[error("Key encoding error: {0}")]
    EncodingError(String),

    /// A platform-specific keychain error occurred.
    #[error("Keychain error: {0}")]
    PlatformError(String),

    /// Failed to generate a new key.
    #[error("Key generation failed: {0}")]
    GenerationError(String),
}

/// Result type for keychain operations.
pub type KeychainResult<T> = Result<T, KeychainError>;

/// Trait for keychain backend implementations.
///
/// This trait allows for different keychain implementations, including
/// a mock backend for testing purposes.
pub trait KeychainBackend: Send + Sync {
    /// Retrieve a secret from the keychain.
    fn get_secret(&self, service: &str, key: &str) -> KeychainResult<String>;

    /// Store a secret in the keychain.
    fn set_secret(&self, service: &str, key: &str, value: &str) -> KeychainResult<()>;

    /// Delete a secret from the keychain.
    fn delete_secret(&self, service: &str, key: &str) -> KeychainResult<()>;
}

/// Real keychain backend using the system keychain.
#[cfg(not(test))]
pub struct SystemKeychain;

#[cfg(not(test))]
impl KeychainBackend for SystemKeychain {
    fn get_secret(&self, service: &str, key: &str) -> KeychainResult<String> {
        let entry = keyring::Entry::new(service, key)
            .map_err(|e| KeychainError::PlatformError(e.to_string()))?;

        entry.get_password().map_err(|e| match e {
            keyring::Error::NoEntry => KeychainError::NotFound(key.to_string()),
            keyring::Error::Ambiguous(_) => {
                KeychainError::PlatformError("Ambiguous keychain entry".to_string())
            }
            keyring::Error::TooLong(_, _) => {
                KeychainError::EncodingError("Key too long".to_string())
            }
            keyring::Error::Invalid(_, _) => {
                KeychainError::EncodingError("Invalid key format".to_string())
            }
            keyring::Error::NoStorageAccess(_) => {
                KeychainError::AccessDenied("No storage access".to_string())
            }
            keyring::Error::PlatformFailure(_) => {
                KeychainError::ServiceUnavailable("Platform failure".to_string())
            }
            _ => KeychainError::PlatformError(e.to_string()),
        })
    }

    fn set_secret(&self, service: &str, key: &str, value: &str) -> KeychainResult<()> {
        let entry = keyring::Entry::new(service, key)
            .map_err(|e| KeychainError::PlatformError(e.to_string()))?;

        entry.set_password(value).map_err(|e| match e {
            keyring::Error::NoStorageAccess(_) => {
                KeychainError::AccessDenied("No storage access".to_string())
            }
            keyring::Error::PlatformFailure(_) => {
                KeychainError::ServiceUnavailable("Platform failure".to_string())
            }
            keyring::Error::TooLong(_, _) => {
                KeychainError::EncodingError("Value too long".to_string())
            }
            _ => KeychainError::PlatformError(e.to_string()),
        })
    }

    fn delete_secret(&self, service: &str, key: &str) -> KeychainResult<()> {
        let entry = keyring::Entry::new(service, key)
            .map_err(|e| KeychainError::PlatformError(e.to_string()))?;

        entry.delete_credential().map_err(|e| match e {
            keyring::Error::NoEntry => KeychainError::NotFound(key.to_string()),
            keyring::Error::NoStorageAccess(_) => {
                KeychainError::AccessDenied("No storage access".to_string())
            }
            keyring::Error::PlatformFailure(_) => {
                KeychainError::ServiceUnavailable("Platform failure".to_string())
            }
            _ => KeychainError::PlatformError(e.to_string()),
        })
    }
}

/// Mock keychain backend for testing.
#[cfg(test)]
pub struct MockKeychain {
    storage: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

#[cfg(test)]
impl MockKeychain {
    /// Create a new mock keychain.
    pub fn new() -> Self {
        Self {
            storage: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Create a key combining service and key name.
    fn make_key(service: &str, key: &str) -> String {
        format!("{}:{}", service, key)
    }
}

#[cfg(test)]
impl Default for MockKeychain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl KeychainBackend for MockKeychain {
    fn get_secret(&self, service: &str, key: &str) -> KeychainResult<String> {
        let storage = self.storage.lock().unwrap();
        let full_key = Self::make_key(service, key);
        storage
            .get(&full_key)
            .cloned()
            .ok_or_else(|| KeychainError::NotFound(key.to_string()))
    }

    fn set_secret(&self, service: &str, key: &str, value: &str) -> KeychainResult<()> {
        let mut storage = self.storage.lock().unwrap();
        let full_key = Self::make_key(service, key);
        storage.insert(full_key, value.to_string());
        Ok(())
    }

    fn delete_secret(&self, service: &str, key: &str) -> KeychainResult<()> {
        let mut storage = self.storage.lock().unwrap();
        let full_key = Self::make_key(service, key);
        if storage.remove(&full_key).is_some() {
            Ok(())
        } else {
            Err(KeychainError::NotFound(key.to_string()))
        }
    }
}

/// Keychain manager for device secret key operations.
///
/// This struct provides high-level operations for managing the device secret key
/// in the system keychain.
pub struct KeychainManager<B: KeychainBackend> {
    backend: B,
    service: String,
    key_name: String,
}

impl<B: KeychainBackend> KeychainManager<B> {
    /// Create a new KeychainManager with the given backend.
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            service: SERVICE_NAME.to_string(),
            key_name: DEFAULT_KEY_NAME.to_string(),
        }
    }

    /// Create a new KeychainManager with custom service and key names.
    pub fn with_names(backend: B, service: impl Into<String>, key_name: impl Into<String>) -> Self {
        Self {
            backend,
            service: service.into(),
            key_name: key_name.into(),
        }
    }

    /// Get the secret key from the keychain.
    ///
    /// Returns the secret key as a base64-encoded string, or an error if not found.
    pub fn get_secret_key(&self) -> KeychainResult<String> {
        self.backend.get_secret(&self.service, &self.key_name)
    }

    /// Store a secret key in the keychain.
    ///
    /// The key should be provided as a base64-encoded string.
    pub fn store_secret_key(&self, key: &str) -> KeychainResult<()> {
        self.backend.set_secret(&self.service, &self.key_name, key)
    }

    /// Delete the secret key from the keychain.
    pub fn delete_secret_key(&self) -> KeychainResult<()> {
        self.backend.delete_secret(&self.service, &self.key_name)
    }

    /// Get the secret key from the keychain, or generate and store a new one if not found.
    ///
    /// Returns the secret key as a base64-encoded string.
    pub fn get_or_create_secret_key(&self) -> KeychainResult<String> {
        match self.get_secret_key() {
            Ok(key) => Ok(key),
            Err(KeychainError::NotFound(_)) => {
                let new_key = generate_secret_key()?;
                self.store_secret_key(&new_key)?;
                Ok(new_key)
            }
            Err(e) => Err(e),
        }
    }

    /// Check if a secret key exists in the keychain.
    pub fn has_secret_key(&self) -> bool {
        self.get_secret_key().is_ok()
    }
}

/// Generate a new random secret key.
///
/// Returns a 32-byte random key encoded as base64.
fn generate_secret_key() -> KeychainResult<String> {
    use rand::RngCore;

    let mut key = [0u8; 32];
    rand::thread_rng()
        .try_fill_bytes(&mut key)
        .map_err(|e| KeychainError::GenerationError(e.to_string()))?;

    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        key,
    ))
}

/// Decode a base64-encoded secret key to raw bytes.
pub fn decode_secret_key(encoded: &str) -> KeychainResult<Vec<u8>> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|e| KeychainError::EncodingError(e.to_string()))
}

/// Encode raw bytes as a base64 secret key.
pub fn encode_secret_key(bytes: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes)
}

#[cfg(not(test))]
impl KeychainManager<SystemKeychain> {
    /// Create a new KeychainManager with the system keychain backend.
    pub fn system() -> Self {
        Self::new(SystemKeychain)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> KeychainManager<MockKeychain> {
        KeychainManager::new(MockKeychain::new())
    }

    #[test]
    fn test_store_and_get_secret_key() {
        let manager = create_test_manager();
        let test_key = "dGVzdC1zZWNyZXQta2V5LTEyMzQ1Njc4OTAtYWJjZGVm";

        manager
            .store_secret_key(test_key)
            .expect("Failed to store key");

        let retrieved = manager.get_secret_key().expect("Failed to get key");
        assert_eq!(retrieved, test_key);
    }

    #[test]
    fn test_get_nonexistent_key() {
        let manager = create_test_manager();

        let result = manager.get_secret_key();
        assert!(matches!(result, Err(KeychainError::NotFound(_))));
    }

    #[test]
    fn test_delete_secret_key() {
        let manager = create_test_manager();
        let test_key = "dGVzdC1rZXktdG8tZGVsZXRl";

        manager
            .store_secret_key(test_key)
            .expect("Failed to store key");
        manager.delete_secret_key().expect("Failed to delete key");

        let result = manager.get_secret_key();
        assert!(matches!(result, Err(KeychainError::NotFound(_))));
    }

    #[test]
    fn test_delete_nonexistent_key() {
        let manager = create_test_manager();

        let result = manager.delete_secret_key();
        assert!(matches!(result, Err(KeychainError::NotFound(_))));
    }

    #[test]
    fn test_get_or_create_secret_key_creates_new() {
        let manager = create_test_manager();

        // Key doesn't exist, should create a new one
        let key = manager
            .get_or_create_secret_key()
            .expect("Failed to get or create key");

        // Verify it's a valid base64 string
        let decoded = decode_secret_key(&key).expect("Failed to decode key");
        assert_eq!(decoded.len(), 32, "Generated key should be 32 bytes");

        // Verify it was stored
        let retrieved = manager.get_secret_key().expect("Failed to get key");
        assert_eq!(retrieved, key);
    }

    #[test]
    fn test_get_or_create_secret_key_returns_existing() {
        let manager = create_test_manager();
        let test_key = "ZXhpc3Rpbmcta2V5LXRoYXQtc2hvdWxkLWJlLXJldHVybmVk";

        manager
            .store_secret_key(test_key)
            .expect("Failed to store key");

        let key = manager
            .get_or_create_secret_key()
            .expect("Failed to get or create key");

        assert_eq!(key, test_key, "Should return existing key");
    }

    #[test]
    fn test_has_secret_key() {
        let manager = create_test_manager();

        assert!(!manager.has_secret_key(), "Should not have key initially");

        manager
            .store_secret_key("dGVzdC1rZXk=")
            .expect("Failed to store key");

        assert!(manager.has_secret_key(), "Should have key after storing");
    }

    #[test]
    fn test_custom_service_and_key_names() {
        let backend = MockKeychain::new();
        let manager = KeychainManager::with_names(backend, "custom-service", "custom-key");
        let test_key = "Y3VzdG9tLWtleS12YWx1ZQ==";

        manager
            .store_secret_key(test_key)
            .expect("Failed to store key");

        let retrieved = manager.get_secret_key().expect("Failed to get key");
        assert_eq!(retrieved, test_key);
    }

    #[test]
    fn test_generate_secret_key() {
        let key1 = generate_secret_key().expect("Failed to generate key 1");
        let key2 = generate_secret_key().expect("Failed to generate key 2");

        // Keys should be different (extremely unlikely to be the same)
        assert_ne!(key1, key2, "Generated keys should be unique");

        // Verify both are valid base64 and decode to 32 bytes
        let decoded1 = decode_secret_key(&key1).expect("Failed to decode key 1");
        let decoded2 = decode_secret_key(&key2).expect("Failed to decode key 2");

        assert_eq!(decoded1.len(), 32);
        assert_eq!(decoded2.len(), 32);
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let original_bytes = [42u8; 32];
        let encoded = encode_secret_key(&original_bytes);
        let decoded = decode_secret_key(&encoded).expect("Failed to decode");

        assert_eq!(decoded.as_slice(), &original_bytes);
    }

    #[test]
    fn test_decode_invalid_base64() {
        let result = decode_secret_key("not-valid-base64!!!");
        assert!(matches!(result, Err(KeychainError::EncodingError(_))));
    }

    #[test]
    fn test_overwrite_existing_key() {
        let manager = create_test_manager();
        let key1 = "Zmlyc3Qta2V5";
        let key2 = "c2Vjb25kLWtleQ==";

        manager
            .store_secret_key(key1)
            .expect("Failed to store key 1");
        manager
            .store_secret_key(key2)
            .expect("Failed to store key 2");

        let retrieved = manager.get_secret_key().expect("Failed to get key");
        assert_eq!(retrieved, key2, "Should return the overwritten key");
    }
}
