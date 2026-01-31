//! Cryptographic identity and key management for RemoShell devices.
//!
//! This module provides Ed25519 key generation, device identity management,
//! message signing, and signature verification.

use ed25519_dalek::{
    Signature as Ed25519Signature, Signer, SigningKey, Verifier, VerifyingKey, PUBLIC_KEY_LENGTH,
    SECRET_KEY_LENGTH, SIGNATURE_LENGTH,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::error::{ProtocolError, Result};

/// Length of a device ID in bytes (SHA-256 output truncated to 16 bytes).
pub const DEVICE_ID_LENGTH: usize = 16;

/// A device identifier derived from the public key.
///
/// This is a 16-byte identifier derived by hashing the public key with SHA-256
/// and taking the first 16 bytes. This provides sufficient uniqueness while
/// keeping the identifier compact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId(#[serde(with = "serde_bytes")] pub [u8; DEVICE_ID_LENGTH]);

impl DeviceId {
    /// Creates a new DeviceId from raw bytes.
    pub fn from_bytes(bytes: [u8; DEVICE_ID_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes of this device ID.
    pub fn as_bytes(&self) -> &[u8; DEVICE_ID_LENGTH] {
        &self.0
    }

    /// Generates a human-readable fingerprint of this device ID.
    ///
    /// The fingerprint is formatted as groups of 4 hex characters separated by colons,
    /// for example: `a1b2:c3d4:e5f6:7890:1234:5678:9abc:def0`
    pub fn fingerprint(&self) -> String {
        self.0
            .chunks(2)
            .map(|chunk| format!("{:02x}{:02x}", chunk[0], chunk[1]))
            .collect::<Vec<_>>()
            .join(":")
    }

    /// Derives a DeviceId from a public key by hashing it with SHA-256.
    fn from_public_key(public_key: &VerifyingKey) -> Self {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(public_key.as_bytes());
        let mut id = [0u8; DEVICE_ID_LENGTH];
        id.copy_from_slice(&hash[..DEVICE_ID_LENGTH]);
        Self(id)
    }
}

impl std::fmt::Display for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.fingerprint())
    }
}

/// A 64-byte Ed25519 signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature(#[serde(with = "serde_bytes")] pub [u8; SIGNATURE_LENGTH]);

impl Signature {
    /// Creates a new Signature from raw bytes.
    pub fn from_bytes(bytes: [u8; SIGNATURE_LENGTH]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes of this signature.
    pub fn as_bytes(&self) -> &[u8; SIGNATURE_LENGTH] {
        &self.0
    }

    /// Converts from ed25519_dalek Signature.
    fn from_ed25519(sig: Ed25519Signature) -> Self {
        Self(sig.to_bytes())
    }

    /// Converts to ed25519_dalek Signature.
    fn as_ed25519(&self) -> Ed25519Signature {
        Ed25519Signature::from_bytes(&self.0)
    }
}

/// The identity of the local device, including the secret key.
///
/// This struct contains the full keypair (secret and public keys) and should
/// be kept secure. It is used for signing messages and proving identity.
#[derive(Clone)]
pub struct DeviceIdentity {
    /// The Ed25519 signing key (secret key).
    signing_key: SigningKey,
    /// The Ed25519 verifying key (public key), derived from signing_key.
    verifying_key: VerifyingKey,
    /// The device identifier, derived from the public key.
    device_id: DeviceId,
}

impl DeviceIdentity {
    /// Generates a new random device identity.
    ///
    /// This creates a new Ed25519 keypair using the operating system's
    /// cryptographically secure random number generator.
    pub fn generate() -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let device_id = DeviceId::from_public_key(&verifying_key);

        Self {
            signing_key,
            verifying_key,
            device_id,
        }
    }

    /// Creates a DeviceIdentity from raw secret key bytes.
    ///
    /// The public key and device ID are derived from the secret key.
    pub fn from_secret_key_bytes(bytes: &[u8; SECRET_KEY_LENGTH]) -> Self {
        let signing_key = SigningKey::from_bytes(bytes);
        let verifying_key = signing_key.verifying_key();
        let device_id = DeviceId::from_public_key(&verifying_key);

        Self {
            signing_key,
            verifying_key,
            device_id,
        }
    }

    /// Returns the secret key bytes.
    ///
    /// **Security Warning**: The secret key should be kept confidential.
    /// Only use this method for secure storage or serialization.
    pub fn secret_key_bytes(&self) -> [u8; SECRET_KEY_LENGTH] {
        self.signing_key.to_bytes()
    }

    /// Returns the public key bytes.
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.verifying_key.to_bytes()
    }

    /// Returns the device ID.
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// Returns the public key.
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Creates a `PeerIdentity` representing this device as a remote peer.
    ///
    /// This is useful when you need to share your identity with others
    /// without exposing the secret key.
    pub fn to_peer_identity(&self) -> PeerIdentity {
        PeerIdentity {
            verifying_key: self.verifying_key,
            device_id: self.device_id,
        }
    }

    /// Signs a message with this device's secret key.
    ///
    /// Returns a 64-byte Ed25519 signature that can be verified using
    /// the corresponding public key.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.signing_key.sign(message);
        Signature::from_ed25519(sig)
    }

    /// Verifies a signature against a message using this device's public key.
    ///
    /// Returns `Ok(())` if the signature is valid, or an error if verification fails.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        let sig = signature.as_ed25519();
        self.verifying_key
            .verify(message, &sig)
            .map_err(ProtocolError::from)
    }

    /// Generates a human-readable fingerprint for this device.
    pub fn fingerprint(&self) -> String {
        self.device_id.fingerprint()
    }
}

impl std::fmt::Debug for DeviceIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceIdentity")
            .field("device_id", &self.device_id)
            .field("public_key", &"[REDACTED]")
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}

/// The identity of a remote peer (public information only).
///
/// This struct contains only the public key and derived device ID,
/// suitable for storing information about trusted peers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerIdentity {
    /// The Ed25519 verifying key (public key).
    #[serde(with = "verifying_key_serde")]
    verifying_key: VerifyingKey,
    /// The device identifier, derived from the public key.
    device_id: DeviceId,
}

impl PeerIdentity {
    /// Creates a PeerIdentity from public key bytes.
    ///
    /// The device ID is derived from the public key.
    pub fn from_public_key_bytes(bytes: &[u8; PUBLIC_KEY_LENGTH]) -> Result<Self> {
        let verifying_key = VerifyingKey::from_bytes(bytes)
            .map_err(|e| ProtocolError::InvalidPublicKey(e.to_string()))?;
        let device_id = DeviceId::from_public_key(&verifying_key);

        Ok(Self {
            verifying_key,
            device_id,
        })
    }

    /// Returns the public key bytes.
    pub fn public_key_bytes(&self) -> [u8; PUBLIC_KEY_LENGTH] {
        self.verifying_key.to_bytes()
    }

    /// Returns the device ID.
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }

    /// Returns the verifying key (public key).
    pub fn verifying_key(&self) -> &VerifyingKey {
        &self.verifying_key
    }

    /// Verifies a signature against a message using this peer's public key.
    ///
    /// Returns `Ok(())` if the signature is valid, or an error if verification fails.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        let sig = signature.as_ed25519();
        self.verifying_key
            .verify(message, &sig)
            .map_err(ProtocolError::from)
    }

    /// Generates a human-readable fingerprint for this peer.
    pub fn fingerprint(&self) -> String {
        self.device_id.fingerprint()
    }
}

/// Serde support for VerifyingKey (serializes as raw bytes).
mod verifying_key_serde {
    use ed25519_dalek::{VerifyingKey, PUBLIC_KEY_LENGTH};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(key: &VerifyingKey, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_bytes::Bytes::new(key.as_bytes()).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<VerifyingKey, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: serde_bytes::ByteBuf = Deserialize::deserialize(deserializer)?;
        if bytes.len() != PUBLIC_KEY_LENGTH {
            return Err(serde::de::Error::custom(format!(
                "invalid public key length: expected {}, got {}",
                PUBLIC_KEY_LENGTH,
                bytes.len()
            )));
        }
        let mut arr = [0u8; PUBLIC_KEY_LENGTH];
        arr.copy_from_slice(&bytes);
        VerifyingKey::from_bytes(&arr).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let identity = DeviceIdentity::generate();

        // Keys should have the correct lengths
        assert_eq!(identity.secret_key_bytes().len(), SECRET_KEY_LENGTH);
        assert_eq!(identity.public_key_bytes().len(), PUBLIC_KEY_LENGTH);
        assert_eq!(identity.device_id().as_bytes().len(), DEVICE_ID_LENGTH);
    }

    #[test]
    fn test_key_generation_produces_unique_keys() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = DeviceIdentity::generate();

        // Two generated identities should be different
        assert_ne!(identity1.secret_key_bytes(), identity2.secret_key_bytes());
        assert_ne!(identity1.public_key_bytes(), identity2.public_key_bytes());
        assert_ne!(identity1.device_id(), identity2.device_id());
    }

    #[test]
    fn test_key_roundtrip_from_bytes() {
        let original = DeviceIdentity::generate();
        let secret_bytes = original.secret_key_bytes();

        let restored = DeviceIdentity::from_secret_key_bytes(&secret_bytes);

        assert_eq!(original.secret_key_bytes(), restored.secret_key_bytes());
        assert_eq!(original.public_key_bytes(), restored.public_key_bytes());
        assert_eq!(original.device_id(), restored.device_id());
    }

    #[test]
    fn test_device_id_stable_for_same_keypair() {
        let identity = DeviceIdentity::generate();
        let secret_bytes = identity.secret_key_bytes();

        // Create the same identity multiple times
        let restored1 = DeviceIdentity::from_secret_key_bytes(&secret_bytes);
        let restored2 = DeviceIdentity::from_secret_key_bytes(&secret_bytes);

        // Device ID should be the same each time
        assert_eq!(restored1.device_id(), restored2.device_id());
        assert_eq!(identity.device_id(), restored1.device_id());
    }

    #[test]
    fn test_signature_roundtrip() {
        let identity = DeviceIdentity::generate();
        let message = b"Hello, RemoShell!";

        // Sign the message
        let signature = identity.sign(message);

        // Verify with the same identity
        assert!(identity.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_signature_verification_with_peer_identity() {
        let identity = DeviceIdentity::generate();
        let peer = identity.to_peer_identity();
        let message = b"Hello from peer!";

        // Sign with device identity
        let signature = identity.sign(message);

        // Verify with peer identity
        assert!(peer.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_signature_fails_with_wrong_key() {
        let identity1 = DeviceIdentity::generate();
        let identity2 = DeviceIdentity::generate();
        let message = b"Secret message";

        // Sign with identity1
        let signature = identity1.sign(message);

        // Verification with identity2 should fail
        assert!(identity2.verify(message, &signature).is_err());
    }

    #[test]
    fn test_signature_fails_with_modified_message() {
        let identity = DeviceIdentity::generate();
        let message = b"Original message";
        let modified = b"Modified message";

        // Sign original message
        let signature = identity.sign(message);

        // Verification with modified message should fail
        assert!(identity.verify(modified, &signature).is_err());
    }

    #[test]
    fn test_fingerprint_format() {
        let identity = DeviceIdentity::generate();
        let fingerprint = identity.fingerprint();

        // Fingerprint should have 8 groups of 4 hex chars separated by colons
        // Total: 8*4 + 7 = 39 characters
        assert_eq!(fingerprint.len(), 39);

        // Should have 7 colons
        assert_eq!(fingerprint.matches(':').count(), 7);

        // Each group should be 4 hex characters
        for (i, group) in fingerprint.split(':').enumerate() {
            assert_eq!(group.len(), 4, "Group {} should have 4 characters", i);
            assert!(
                group.chars().all(|c| c.is_ascii_hexdigit()),
                "Group {} should be hex",
                i
            );
        }
    }

    #[test]
    fn test_fingerprint_human_readable() {
        let identity = DeviceIdentity::generate();
        let fingerprint = identity.fingerprint();

        // Should only contain lowercase hex and colons
        assert!(fingerprint
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == ':'));
    }

    #[test]
    fn test_device_id_display() {
        let identity = DeviceIdentity::generate();
        let display = format!("{}", identity.device_id());
        let fingerprint = identity.fingerprint();

        // Display should equal fingerprint
        assert_eq!(display, fingerprint);
    }

    #[test]
    fn test_peer_identity_from_public_key_bytes() {
        let identity = DeviceIdentity::generate();
        let public_bytes = identity.public_key_bytes();

        let peer = PeerIdentity::from_public_key_bytes(&public_bytes).unwrap();

        assert_eq!(peer.public_key_bytes(), public_bytes);
        assert_eq!(peer.device_id(), identity.device_id());
    }

    #[test]
    fn test_peer_identity_serialization() {
        let identity = DeviceIdentity::generate();
        let peer = identity.to_peer_identity();

        // Serialize to JSON
        let json = serde_json::to_string(&peer).unwrap();

        // Deserialize back
        let restored: PeerIdentity = serde_json::from_str(&json).unwrap();

        assert_eq!(peer.public_key_bytes(), restored.public_key_bytes());
        assert_eq!(peer.device_id(), restored.device_id());
    }

    #[test]
    fn test_peer_identity_msgpack_serialization() {
        let identity = DeviceIdentity::generate();
        let peer = identity.to_peer_identity();

        // Serialize to MessagePack
        let msgpack = rmp_serde::to_vec(&peer).unwrap();

        // Deserialize back
        let restored: PeerIdentity = rmp_serde::from_slice(&msgpack).unwrap();

        assert_eq!(peer.public_key_bytes(), restored.public_key_bytes());
        assert_eq!(peer.device_id(), restored.device_id());
    }

    #[test]
    fn test_device_id_serialization() {
        let identity = DeviceIdentity::generate();
        let device_id = *identity.device_id();

        // Serialize to JSON
        let json = serde_json::to_string(&device_id).unwrap();

        // Deserialize back
        let restored: DeviceId = serde_json::from_str(&json).unwrap();

        assert_eq!(device_id, restored);
    }

    #[test]
    fn test_signature_serialization() {
        let identity = DeviceIdentity::generate();
        let signature = identity.sign(b"test message");

        // Serialize to JSON
        let json = serde_json::to_string(&signature).unwrap();

        // Deserialize back
        let restored: Signature = serde_json::from_str(&json).unwrap();

        assert_eq!(signature.as_bytes(), restored.as_bytes());
    }

    #[test]
    fn test_device_identity_debug_redacts_secrets() {
        let identity = DeviceIdentity::generate();
        let debug = format!("{:?}", identity);

        // Debug output should not contain actual key bytes
        assert!(debug.contains("REDACTED"));
        // But should still show device_id
        assert!(debug.contains("device_id"));
    }

    #[test]
    fn test_invalid_public_key_bytes_fails_verification() {
        // Generate a valid identity and get a signature
        let identity = DeviceIdentity::generate();
        let message = b"test message";
        let signature = identity.sign(message);

        // Create a different identity (simulating a "wrong" key)
        let other_identity = DeviceIdentity::generate();
        let other_peer = other_identity.to_peer_identity();

        // Verification with wrong key should fail
        assert!(other_peer.verify(message, &signature).is_err());
    }

    #[test]
    fn test_signature_with_corrupted_data() {
        let identity = DeviceIdentity::generate();
        let message = b"test message";
        let signature = identity.sign(message);

        // Corrupt the signature
        let mut corrupted_sig_bytes = *signature.as_bytes();
        corrupted_sig_bytes[0] ^= 0xFF; // Flip bits in first byte
        let corrupted_sig = Signature::from_bytes(corrupted_sig_bytes);

        // Verification should fail with corrupted signature
        assert!(identity.verify(message, &corrupted_sig).is_err());
    }
}
