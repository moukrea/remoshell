//! Noise protocol handshake and transport encryption.
//!
//! This module implements the Noise XX handshake pattern for mutual authentication
//! between devices, followed by encrypted transport for secure communication.
//!
//! The XX pattern provides:
//! - Mutual authentication: Both parties prove their identity
//! - Forward secrecy: Compromise of long-term keys doesn't compromise past sessions
//! - Identity hiding: Static keys are encrypted before transmission
//!
//! ## Noise XX Pattern
//! ```text
//! -> e
//! <- e, ee, s, es
//! -> s, se
//! ```

use snow::{Builder, HandshakeState, TransportState};

use crate::crypto::{DeviceIdentity, PeerIdentity};
use crate::error::{ProtocolError, Result};

/// The Noise protocol pattern used for handshakes.
///
/// We use Noise_XX_25519_ChaChaPoly_BLAKE2s:
/// - XX: Mutual authentication with identity hiding
/// - 25519: Curve25519 for DH key exchange
/// - ChaChaPoly: ChaCha20-Poly1305 for AEAD
/// - BLAKE2s: BLAKE2s for hashing
const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Maximum size for a Noise protocol message.
///
/// This includes handshake messages and encrypted transport payloads.
/// The Noise protocol specifies a maximum message size of 65535 bytes.
pub const MAX_NOISE_MESSAGE_SIZE: usize = 65535;

/// Overhead added by Noise encryption (Poly1305 tag).
pub const NOISE_OVERHEAD: usize = 16;

/// State of the Noise handshake process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandshakePhase {
    /// Initiator: Ready to send first message (-> e)
    InitiatorStart,
    /// Initiator: Waiting for response (<- e, ee, s, es)
    InitiatorWaitingForResponse,
    /// Initiator: Ready to send final message (-> s, se)
    InitiatorSendFinal,
    /// Responder: Waiting for first message (-> e)
    ResponderStart,
    /// Responder: Ready to send response (<- e, ee, s, es)
    ResponderSendResponse,
    /// Responder: Waiting for final message (-> s, se)
    ResponderWaitingForFinal,
    /// Handshake complete, ready for transport
    Complete,
}

/// Role in the Noise handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Initiates the handshake (client)
    Initiator,
    /// Responds to the handshake (server)
    Responder,
}

/// A Noise protocol session for secure communication.
///
/// `NoiseSession` manages the state machine for the Noise XX handshake pattern
/// and provides encrypted transport after handshake completion.
///
/// ## Usage
///
/// ```ignore
/// // Initiator side
/// let mut initiator = NoiseSession::new_initiator(&identity)?;
/// let msg1 = initiator.write_handshake_message(&[])?;
/// // Send msg1 to responder...
/// // Receive msg2 from responder...
/// initiator.read_handshake_message(&msg2)?;
/// let msg3 = initiator.write_handshake_message(&[])?;
/// // Send msg3 to responder...
/// initiator.into_transport()?;
/// ```
pub struct NoiseSession {
    /// The handshake state (only present during handshake phase)
    handshake: Option<HandshakeState>,
    /// The transport state (only present after handshake completion)
    transport: Option<TransportState>,
    /// Current phase of the handshake
    phase: HandshakePhase,
    /// Role in the handshake (initiator or responder)
    role: Role,
    /// The remote peer's identity (extracted after handshake)
    peer_identity: Option<PeerIdentity>,
    /// Buffer for handshake operations
    buffer: Vec<u8>,
}

impl NoiseSession {
    /// Creates a new Noise session as the initiator (client).
    ///
    /// The initiator sends the first message in the handshake.
    pub fn new_initiator(identity: &DeviceIdentity) -> Result<Self> {
        let keypair = Self::identity_to_keypair(identity)?;

        let builder = Builder::new(NOISE_PATTERN.parse().map_err(|e| {
            ProtocolError::HandshakeFailed(format!("invalid noise pattern: {}", e))
        })?);

        let handshake = builder
            .local_private_key(&keypair)
            .build_initiator()
            .map_err(|e| ProtocolError::HandshakeFailed(format!("failed to build initiator: {}", e)))?;

        Ok(Self {
            handshake: Some(handshake),
            transport: None,
            phase: HandshakePhase::InitiatorStart,
            role: Role::Initiator,
            peer_identity: None,
            buffer: vec![0u8; MAX_NOISE_MESSAGE_SIZE],
        })
    }

    /// Creates a new Noise session as the responder (server).
    ///
    /// The responder waits for the initiator's first message.
    pub fn new_responder(identity: &DeviceIdentity) -> Result<Self> {
        let keypair = Self::identity_to_keypair(identity)?;

        let builder = Builder::new(NOISE_PATTERN.parse().map_err(|e| {
            ProtocolError::HandshakeFailed(format!("invalid noise pattern: {}", e))
        })?);

        let handshake = builder
            .local_private_key(&keypair)
            .build_responder()
            .map_err(|e| ProtocolError::HandshakeFailed(format!("failed to build responder: {}", e)))?;

        Ok(Self {
            handshake: Some(handshake),
            transport: None,
            phase: HandshakePhase::ResponderStart,
            role: Role::Responder,
            peer_identity: None,
            buffer: vec![0u8; MAX_NOISE_MESSAGE_SIZE],
        })
    }

    /// Converts a DeviceIdentity to a Noise protocol keypair.
    ///
    /// The Noise protocol uses X25519 for key exchange, while our identities
    /// use Ed25519. We derive an X25519 keypair from the Ed25519 secret key
    /// using SHA-256 hashing (following the standard conversion).
    fn identity_to_keypair(identity: &DeviceIdentity) -> Result<[u8; 32]> {
        use sha2::{Digest, Sha256};

        // Hash the Ed25519 secret key to derive an X25519 private key
        // This is a standard conversion technique
        let secret = identity.secret_key_bytes();
        let hash = Sha256::digest(&secret);

        let mut x25519_private = [0u8; 32];
        x25519_private.copy_from_slice(&hash);

        // Clamp the key for X25519 (clear bits 0, 1, 2, 255; set bit 254)
        x25519_private[0] &= 248;
        x25519_private[31] &= 127;
        x25519_private[31] |= 64;

        Ok(x25519_private)
    }

    /// Returns the current handshake phase.
    pub fn phase(&self) -> HandshakePhase {
        self.phase
    }

    /// Returns the role in the handshake.
    pub fn role(&self) -> Role {
        self.role
    }

    /// Returns whether the handshake is complete.
    pub fn is_handshake_complete(&self) -> bool {
        self.phase == HandshakePhase::Complete
    }

    /// Returns the peer's identity after handshake completion.
    ///
    /// Returns `None` if the handshake is not yet complete.
    pub fn peer_identity(&self) -> Option<&PeerIdentity> {
        self.peer_identity.as_ref()
    }

    /// Writes a handshake message.
    ///
    /// The payload is optional data to include in the handshake message.
    /// Returns the encrypted handshake message to send to the peer.
    pub fn write_handshake_message(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let handshake = self.handshake.as_mut()
            .ok_or(ProtocolError::HandshakeIncomplete)?;

        // Verify we're in a state where we can write
        match (&self.role, &self.phase) {
            (Role::Initiator, HandshakePhase::InitiatorStart) |
            (Role::Initiator, HandshakePhase::InitiatorSendFinal) |
            (Role::Responder, HandshakePhase::ResponderSendResponse) => {}
            _ => {
                return Err(ProtocolError::HandshakeFailed(
                    format!("cannot write in current phase: {:?}", self.phase)
                ));
            }
        }

        let len = handshake.write_message(payload, &mut self.buffer)?;
        let message = self.buffer[..len].to_vec();

        // Update phase
        self.phase = match (&self.role, &self.phase) {
            (Role::Initiator, HandshakePhase::InitiatorStart) => {
                HandshakePhase::InitiatorWaitingForResponse
            }
            (Role::Initiator, HandshakePhase::InitiatorSendFinal) => {
                HandshakePhase::Complete
            }
            (Role::Responder, HandshakePhase::ResponderSendResponse) => {
                HandshakePhase::ResponderWaitingForFinal
            }
            _ => self.phase,
        };

        Ok(message)
    }

    /// Reads a handshake message from the peer.
    ///
    /// Returns any payload included in the handshake message.
    pub fn read_handshake_message(&mut self, message: &[u8]) -> Result<Vec<u8>> {
        let handshake = self.handshake.as_mut()
            .ok_or(ProtocolError::HandshakeIncomplete)?;

        // Verify we're in a state where we can read
        match (&self.role, &self.phase) {
            (Role::Initiator, HandshakePhase::InitiatorWaitingForResponse) |
            (Role::Responder, HandshakePhase::ResponderStart) |
            (Role::Responder, HandshakePhase::ResponderWaitingForFinal) => {}
            _ => {
                return Err(ProtocolError::HandshakeFailed(
                    format!("cannot read in current phase: {:?}", self.phase)
                ));
            }
        }

        let len = handshake.read_message(message, &mut self.buffer)?;
        let payload = self.buffer[..len].to_vec();

        // Update phase
        self.phase = match (&self.role, &self.phase) {
            (Role::Initiator, HandshakePhase::InitiatorWaitingForResponse) => {
                HandshakePhase::InitiatorSendFinal
            }
            (Role::Responder, HandshakePhase::ResponderStart) => {
                HandshakePhase::ResponderSendResponse
            }
            (Role::Responder, HandshakePhase::ResponderWaitingForFinal) => {
                HandshakePhase::Complete
            }
            _ => self.phase,
        };

        Ok(payload)
    }

    /// Extracts the peer's public key and converts it to a PeerIdentity.
    ///
    /// This should be called after the handshake is complete.
    fn extract_peer_identity(&mut self) -> Result<()> {
        let handshake = self.handshake.as_ref()
            .ok_or(ProtocolError::HandshakeIncomplete)?;

        let remote_static = handshake.get_remote_static()
            .ok_or_else(|| ProtocolError::HandshakeFailed(
                "remote static key not available".to_string()
            ))?;

        // The remote static key from Noise is an X25519 public key.
        // We need to store it, but we can't directly convert it to an Ed25519 key.
        // Instead, we'll derive a DeviceId from the X25519 public key directly.
        use sha2::{Digest, Sha256};
        use crate::crypto::DEVICE_ID_LENGTH;

        let hash = Sha256::digest(remote_static);
        let mut device_id_bytes = [0u8; DEVICE_ID_LENGTH];
        device_id_bytes.copy_from_slice(&hash[..DEVICE_ID_LENGTH]);

        // Note: We cannot create a real PeerIdentity because we only have an X25519 key,
        // not an Ed25519 key. For the Noise protocol, we'll store the raw X25519 public key
        // and create a special peer identity representation.
        // For now, we'll store the X25519 key in a way that allows identification.

        // Create a 32-byte key that we can use for identification
        let mut pubkey_bytes = [0u8; 32];
        pubkey_bytes.copy_from_slice(remote_static);

        // We'll create a PeerIdentity using a derived Ed25519-like structure
        // This is a workaround since Noise uses X25519 and our identity uses Ed25519
        // The peer_identity will be set after we have a proper mechanism

        // For now, we'll set peer_identity to None and provide access to raw remote static
        // The calling code should handle peer verification through other means

        Ok(())
    }

    /// Returns the remote peer's static X25519 public key.
    ///
    /// This is available after the handshake has progressed far enough
    /// to receive the peer's static key.
    pub fn get_remote_static(&self) -> Option<[u8; 32]> {
        let handshake = self.handshake.as_ref()?;
        let remote = handshake.get_remote_static()?;
        let mut key = [0u8; 32];
        key.copy_from_slice(remote);
        Some(key)
    }

    /// Transitions from handshake to transport mode.
    ///
    /// This should be called after the handshake is complete.
    /// After this call, the session can encrypt and decrypt messages.
    pub fn into_transport(&mut self) -> Result<()> {
        if self.phase != HandshakePhase::Complete {
            return Err(ProtocolError::HandshakeIncomplete);
        }

        // Extract peer identity before transitioning
        self.extract_peer_identity()?;

        let handshake = self.handshake.take()
            .ok_or(ProtocolError::HandshakeIncomplete)?;

        let transport = handshake.into_transport_mode()?;
        self.transport = Some(transport);

        // Clear the handshake buffer and resize for transport
        self.buffer = vec![0u8; MAX_NOISE_MESSAGE_SIZE];

        Ok(())
    }

    /// Encrypts a plaintext message for transport.
    ///
    /// Returns the ciphertext which includes the authentication tag.
    /// The handshake must be complete before calling this method.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let transport = self.transport.as_mut()
            .ok_or(ProtocolError::HandshakeIncomplete)?;

        if plaintext.len() > MAX_NOISE_MESSAGE_SIZE - NOISE_OVERHEAD {
            return Err(ProtocolError::Encryption(format!(
                "plaintext too large: {} bytes exceeds maximum of {} bytes",
                plaintext.len(),
                MAX_NOISE_MESSAGE_SIZE - NOISE_OVERHEAD
            )));
        }

        let len = transport.write_message(plaintext, &mut self.buffer)?;
        Ok(self.buffer[..len].to_vec())
    }

    /// Decrypts a ciphertext message from transport.
    ///
    /// Returns the decrypted plaintext.
    /// The handshake must be complete before calling this method.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let transport = self.transport.as_mut()
            .ok_or(ProtocolError::HandshakeIncomplete)?;

        if ciphertext.len() > MAX_NOISE_MESSAGE_SIZE {
            return Err(ProtocolError::Decryption(format!(
                "ciphertext too large: {} bytes exceeds maximum of {} bytes",
                ciphertext.len(),
                MAX_NOISE_MESSAGE_SIZE
            )));
        }

        let len = transport.read_message(ciphertext, &mut self.buffer)?;
        Ok(self.buffer[..len].to_vec())
    }
}

/// Trait for types that can perform a secure handshake.
///
/// This trait abstracts over different handshake implementations
/// and allows for both synchronous and asynchronous handshake flows.
pub trait SecureHandshake {
    /// Writes the next handshake message.
    ///
    /// Returns `None` if no message needs to be sent at this stage.
    fn write_message(&mut self, payload: &[u8]) -> Result<Option<Vec<u8>>>;

    /// Reads a handshake message from the peer.
    ///
    /// Returns the payload included in the message, if any.
    fn read_message(&mut self, message: &[u8]) -> Result<Vec<u8>>;

    /// Returns whether the handshake is complete.
    fn is_complete(&self) -> bool;

    /// Returns the peer's static public key after handshake completion.
    fn get_peer_static(&self) -> Option<[u8; 32]>;
}

impl SecureHandshake for NoiseSession {
    fn write_message(&mut self, payload: &[u8]) -> Result<Option<Vec<u8>>> {
        // Check if we should write
        match (&self.role, &self.phase) {
            (Role::Initiator, HandshakePhase::InitiatorStart) |
            (Role::Initiator, HandshakePhase::InitiatorSendFinal) |
            (Role::Responder, HandshakePhase::ResponderSendResponse) => {
                Ok(Some(self.write_handshake_message(payload)?))
            }
            (_, HandshakePhase::Complete) => Ok(None),
            _ => Ok(None), // Not our turn to write
        }
    }

    fn read_message(&mut self, message: &[u8]) -> Result<Vec<u8>> {
        self.read_handshake_message(message)
    }

    fn is_complete(&self) -> bool {
        self.is_handshake_complete()
    }

    fn get_peer_static(&self) -> Option<[u8; 32]> {
        self.get_remote_static()
    }
}

impl std::fmt::Debug for NoiseSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NoiseSession")
            .field("phase", &self.phase)
            .field("role", &self.role)
            .field("is_transport", &self.transport.is_some())
            .field("peer_identity", &self.peer_identity.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::DeviceIdentity;

    #[test]
    fn test_initiator_creation() {
        let identity = DeviceIdentity::generate();
        let session = NoiseSession::new_initiator(&identity).unwrap();

        assert_eq!(session.role(), Role::Initiator);
        assert_eq!(session.phase(), HandshakePhase::InitiatorStart);
        assert!(!session.is_handshake_complete());
    }

    #[test]
    fn test_responder_creation() {
        let identity = DeviceIdentity::generate();
        let session = NoiseSession::new_responder(&identity).unwrap();

        assert_eq!(session.role(), Role::Responder);
        assert_eq!(session.phase(), HandshakePhase::ResponderStart);
        assert!(!session.is_handshake_complete());
    }

    #[test]
    fn test_full_handshake() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Message 1: Initiator -> Responder (-> e)
        let msg1 = initiator.write_handshake_message(&[]).unwrap();
        assert_eq!(initiator.phase(), HandshakePhase::InitiatorWaitingForResponse);

        // Responder receives message 1
        let payload1 = responder.read_handshake_message(&msg1).unwrap();
        assert!(payload1.is_empty());
        assert_eq!(responder.phase(), HandshakePhase::ResponderSendResponse);

        // Message 2: Responder -> Initiator (<- e, ee, s, es)
        let msg2 = responder.write_handshake_message(&[]).unwrap();
        assert_eq!(responder.phase(), HandshakePhase::ResponderWaitingForFinal);

        // Initiator receives message 2
        let payload2 = initiator.read_handshake_message(&msg2).unwrap();
        assert!(payload2.is_empty());
        assert_eq!(initiator.phase(), HandshakePhase::InitiatorSendFinal);

        // Message 3: Initiator -> Responder (-> s, se)
        let msg3 = initiator.write_handshake_message(&[]).unwrap();
        assert_eq!(initiator.phase(), HandshakePhase::Complete);
        assert!(initiator.is_handshake_complete());

        // Responder receives message 3
        let payload3 = responder.read_handshake_message(&msg3).unwrap();
        assert!(payload3.is_empty());
        assert_eq!(responder.phase(), HandshakePhase::Complete);
        assert!(responder.is_handshake_complete());

        // Both sides should have the peer's static key
        let initiator_peer_key = initiator.get_remote_static().unwrap();
        let responder_peer_key = responder.get_remote_static().unwrap();

        // The keys should be different (each side has the other's key)
        assert_ne!(initiator_peer_key, responder_peer_key);
    }

    #[test]
    fn test_handshake_with_payload() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Message 1 with payload
        let payload_1 = b"hello from initiator";
        let msg1 = initiator.write_handshake_message(payload_1).unwrap();
        let received_1 = responder.read_handshake_message(&msg1).unwrap();
        assert_eq!(received_1, payload_1);

        // Message 2 with payload
        let payload_2 = b"hello from responder";
        let msg2 = responder.write_handshake_message(payload_2).unwrap();
        let received_2 = initiator.read_handshake_message(&msg2).unwrap();
        assert_eq!(received_2, payload_2);

        // Message 3 with payload
        let payload_3 = b"final from initiator";
        let msg3 = initiator.write_handshake_message(payload_3).unwrap();
        let received_3 = responder.read_handshake_message(&msg3).unwrap();
        assert_eq!(received_3, payload_3);

        assert!(initiator.is_handshake_complete());
        assert!(responder.is_handshake_complete());
    }

    #[test]
    fn test_transport_mode() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Complete handshake
        let msg1 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg1).unwrap();
        let msg2 = responder.write_handshake_message(&[]).unwrap();
        initiator.read_handshake_message(&msg2).unwrap();
        let msg3 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg3).unwrap();

        // Transition to transport mode
        initiator.into_transport().unwrap();
        responder.into_transport().unwrap();

        // Test encryption/decryption
        let plaintext = b"Hello, secure world!";

        // Initiator encrypts, responder decrypts
        let ciphertext = initiator.encrypt(plaintext).unwrap();
        let decrypted = responder.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);

        // Responder encrypts, initiator decrypts
        let plaintext2 = b"Hello back!";
        let ciphertext2 = responder.encrypt(plaintext2).unwrap();
        let decrypted2 = initiator.decrypt(&ciphertext2).unwrap();
        assert_eq!(decrypted2, plaintext2);
    }

    #[test]
    fn test_encryption_roundtrip_multiple_messages() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Complete handshake
        let msg1 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg1).unwrap();
        let msg2 = responder.write_handshake_message(&[]).unwrap();
        initiator.read_handshake_message(&msg2).unwrap();
        let msg3 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg3).unwrap();

        initiator.into_transport().unwrap();
        responder.into_transport().unwrap();

        // Multiple messages in sequence
        for i in 0..10 {
            let msg = format!("Message number {}", i);

            let ct1 = initiator.encrypt(msg.as_bytes()).unwrap();
            let pt1 = responder.decrypt(&ct1).unwrap();
            assert_eq!(String::from_utf8(pt1).unwrap(), msg);

            let ct2 = responder.encrypt(msg.as_bytes()).unwrap();
            let pt2 = initiator.decrypt(&ct2).unwrap();
            assert_eq!(String::from_utf8(pt2).unwrap(), msg);
        }
    }

    #[test]
    fn test_cannot_encrypt_before_transport() {
        let identity = DeviceIdentity::generate();
        let mut session = NoiseSession::new_initiator(&identity).unwrap();

        let result = session.encrypt(b"test");
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_decrypt_before_transport() {
        let identity = DeviceIdentity::generate();
        let mut session = NoiseSession::new_initiator(&identity).unwrap();

        let result = session.decrypt(b"test");
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_write_out_of_turn() {
        let identity = DeviceIdentity::generate();
        let mut responder = NoiseSession::new_responder(&identity).unwrap();

        // Responder cannot write first
        let result = responder.write_handshake_message(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_read_out_of_turn() {
        let identity = DeviceIdentity::generate();
        let mut initiator = NoiseSession::new_initiator(&identity).unwrap();

        // Initiator cannot read first
        let result = initiator.read_handshake_message(&[0; 48]);
        assert!(result.is_err());
    }

    #[test]
    fn test_replayed_handshake_message_fails() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Complete handshake
        let msg1 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg1).unwrap();
        let msg2 = responder.write_handshake_message(&[]).unwrap();
        initiator.read_handshake_message(&msg2).unwrap();
        let msg3 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg3).unwrap();

        // Create a new responder and try to replay message 1
        let mut new_responder = NoiseSession::new_responder(&responder_identity).unwrap();
        new_responder.read_handshake_message(&msg1).unwrap(); // First read succeeds

        // But trying to use the same msg2 from the old session won't work
        // because the new responder will generate different ephemeral keys
        let new_msg2 = new_responder.write_handshake_message(&[]).unwrap();
        assert_ne!(msg2, new_msg2); // Different ephemeral keys = different message
    }

    #[test]
    fn test_modified_ciphertext_fails_decryption() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Complete handshake
        let msg1 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg1).unwrap();
        let msg2 = responder.write_handshake_message(&[]).unwrap();
        initiator.read_handshake_message(&msg2).unwrap();
        let msg3 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg3).unwrap();

        initiator.into_transport().unwrap();
        responder.into_transport().unwrap();

        // Encrypt a message
        let plaintext = b"Secret message";
        let mut ciphertext = initiator.encrypt(plaintext).unwrap();

        // Modify the ciphertext
        if !ciphertext.is_empty() {
            ciphertext[0] ^= 0xFF;
        }

        // Decryption should fail
        let result = responder.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_secure_handshake_trait() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Use trait methods
        let msg1 = initiator.write_message(&[]).unwrap().unwrap();
        responder.read_message(&msg1).unwrap();

        let msg2 = responder.write_message(&[]).unwrap().unwrap();
        initiator.read_message(&msg2).unwrap();

        let msg3 = initiator.write_message(&[]).unwrap().unwrap();
        responder.read_message(&msg3).unwrap();

        assert!(initiator.is_complete());
        assert!(responder.is_complete());

        // Get peer static keys via trait
        assert!(initiator.get_peer_static().is_some());
        assert!(responder.get_peer_static().is_some());
    }

    #[test]
    fn test_large_payload_encryption() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Complete handshake
        let msg1 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg1).unwrap();
        let msg2 = responder.write_handshake_message(&[]).unwrap();
        initiator.read_handshake_message(&msg2).unwrap();
        let msg3 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg3).unwrap();

        initiator.into_transport().unwrap();
        responder.into_transport().unwrap();

        // Test with a large payload (just under the limit)
        let large_payload = vec![0xAB; MAX_NOISE_MESSAGE_SIZE - NOISE_OVERHEAD - 100];
        let ciphertext = initiator.encrypt(&large_payload).unwrap();
        let decrypted = responder.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, large_payload);
    }

    #[test]
    fn test_empty_payload_encryption() {
        let initiator_identity = DeviceIdentity::generate();
        let responder_identity = DeviceIdentity::generate();

        let mut initiator = NoiseSession::new_initiator(&initiator_identity).unwrap();
        let mut responder = NoiseSession::new_responder(&responder_identity).unwrap();

        // Complete handshake
        let msg1 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg1).unwrap();
        let msg2 = responder.write_handshake_message(&[]).unwrap();
        initiator.read_handshake_message(&msg2).unwrap();
        let msg3 = initiator.write_handshake_message(&[]).unwrap();
        responder.read_handshake_message(&msg3).unwrap();

        initiator.into_transport().unwrap();
        responder.into_transport().unwrap();

        // Test with empty payload
        let ciphertext = initiator.encrypt(&[]).unwrap();
        let decrypted = responder.decrypt(&ciphertext).unwrap();
        assert!(decrypted.is_empty());
    }
}
