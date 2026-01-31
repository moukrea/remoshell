# RemoShell Security Model

This document describes the security architecture of RemoShell, including threat modeling, cryptographic design, and attack surface analysis.

## Security Goals

RemoShell is designed to provide:

1. **Confidentiality**: All communication is encrypted end-to-end
2. **Integrity**: Messages cannot be modified without detection
3. **Authentication**: Both parties prove their identity
4. **Forward Secrecy**: Past sessions cannot be decrypted if keys are compromised
5. **Identity Hiding**: Static keys are encrypted during handshake

## Threat Model

### Adversary Capabilities

We consider adversaries who can:

- **Network Observer**: Passively observe all network traffic
- **Network MITM**: Actively modify or inject network traffic
- **Compromised Signaling Server**: Control the signaling server
- **Stolen Device**: Possess a previously-trusted device

### Trust Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│                      TRUSTED                                     │
│  ┌───────────────────────┐      ┌───────────────────────────┐   │
│  │      Client App       │      │         Daemon            │   │
│  │                       │      │                           │   │
│  │  - Private keys       │      │  - Private keys           │   │
│  │  - Trust store        │      │  - Trust store            │   │
│  │  - Session data       │      │  - PTY sessions           │   │
│  └───────────────────────┘      └───────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                              │
                    ══════════╪══════════  Trust Boundary
                              │
┌─────────────────────────────────────────────────────────────────┐
│                      UNTRUSTED                                   │
│                                                                  │
│  ┌───────────────────────┐      ┌───────────────────────────┐   │
│  │   Signaling Server    │      │     Network / Internet    │   │
│  │                       │      │                           │   │
│  │  - No access to       │      │  - Assumes hostile        │   │
│  │    plaintext          │      │  - MITM possible          │   │
│  │  - Relays only        │      │                           │   │
│  └───────────────────────┘      └───────────────────────────┘   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Non-Goals

The following are explicitly not protected against:

- Compromised operating system on client or server
- Side-channel attacks (timing, power analysis)
- Physical access to running devices
- Social engineering of users

## Cryptographic Primitives

### Algorithm Choices

| Purpose | Algorithm | Parameters |
|---------|-----------|------------|
| Identity Keys | Ed25519 | 256-bit keys |
| Key Agreement | X25519 | Curve25519 |
| AEAD Encryption | ChaCha20-Poly1305 | 256-bit key, 96-bit nonce |
| Hashing | BLAKE2s | 256-bit output |
| Compression | LZ4 | Block mode |

### Why These Choices?

**Ed25519** for identities:
- Fast signature generation and verification
- Small signatures (64 bytes)
- Resistant to timing attacks
- Widely audited implementation (ed25519-dalek)

**X25519 + ChaCha20-Poly1305** (via Noise):
- ChaCha20 resistant to timing attacks (no table lookups)
- Better performance than AES on devices without AES-NI
- Strong security margin
- Poly1305 provides authentication

**BLAKE2s** (via Noise):
- Faster than SHA-256
- Security margin equivalent to SHA-3
- Optimized for 32-bit platforms

## Noise Protocol Details

### Pattern Selection

We use `Noise_XX_25519_ChaChaPoly_BLAKE2s`:

```
XX Pattern:
  -> e
  <- e, ee, s, es
  -> s, se
```

The XX pattern was chosen because:

1. **Mutual Authentication**: Both parties prove identity
2. **Identity Hiding**: Static public keys encrypted before transmission
3. **No Pre-Shared Keys**: Works without prior key exchange
4. **Forward Secrecy**: Ephemeral keys ensure past sessions are protected

### Handshake Security Properties

| Property | Protection Level |
|----------|------------------|
| Confidentiality of initiator identity | Protected from passive observers after msg 2 |
| Confidentiality of responder identity | Protected from passive observers after msg 2 |
| Initiator authentication | Confirmed by responder after msg 3 |
| Responder authentication | Confirmed by initiator after msg 2 |
| Forward secrecy | Full (ephemeral keys deleted after handshake) |

### Key Derivation

Ed25519 keys must be converted to X25519 for Noise:

```rust
fn derive_x25519_key(ed25519_secret: &[u8; 32]) -> [u8; 32] {
    // SHA-256 hash of Ed25519 secret key
    let hash = sha256(ed25519_secret);

    let mut x25519_key = [0u8; 32];
    x25519_key.copy_from_slice(&hash[0..32]);

    // Clamp for X25519 (RFC 7748)
    x25519_key[0] &= 248;    // Clear bits 0, 1, 2
    x25519_key[31] &= 127;   // Clear bit 255
    x25519_key[31] |= 64;    // Set bit 254

    x25519_key
}
```

## Device Identity

### Device ID Generation

Device IDs are derived from public keys to provide:
- Unique identification
- Verifiable binding to public key
- Human-readable format

```rust
const DEVICE_ID_LENGTH: usize = 16;

fn derive_device_id(public_key: &[u8; 32]) -> [u8; 16] {
    let hash = sha256(public_key);
    hash[0..16].try_into().unwrap()
}

// Display format: "a1b2c3d4:e5f67890:12345678:9abcdef0"
fn fingerprint(device_id: &[u8; 16]) -> String {
    device_id
        .chunks(2)
        .map(|c| format!("{:02x}{:02x}", c[0], c[1]))
        .collect::<Vec<_>>()
        .join(":")
}
```

### Key Storage

**Daemon** (Linux):
- Private key stored in `~/.config/remoshell/identity.key`
- File permissions: `0600` (owner read/write only)
- Raw 32-byte Ed25519 secret key

**Client** (Tauri):
- Private key stored in OS keychain (keyring crate)
- Service name: `remoshell`
- Key name: `device_identity`

**Trust Store**:
- JSON file at `~/.config/remoshell/trusted_devices.json`
- Contains device IDs, public keys, names, trust levels
- Atomic writes prevent corruption

## Trust Establishment

### QR Code Pairing

Initial trust is established via QR code scanning:

```
┌──────────────────────────────────────────────────────────────┐
│                    QR Code Contents                           │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  URL: remoshell://pair?                                       │
│       device_id=<fingerprint>&                                │
│       room_id=<uuid>&                                         │
│       sig=<signature>                                         │
│                                                               │
│  Components:                                                  │
│  - device_id: Daemon's fingerprint for verification          │
│  - room_id: Unique room for WebRTC negotiation               │
│  - sig: Ed25519 signature over device_id||room_id            │
│                                                               │
└──────────────────────────────────────────────────────────────┘
```

### Pairing Flow

```
     User           Client App          Daemon            Signaling
       │                │                 │                   │
       │  1. View QR    │                 │                   │
       │ ◄──────────────┼─────────────────│                   │
       │                │                 │                   │
       │  2. Scan QR    │                 │                   │
       │ ──────────────►│                 │                   │
       │                │                 │                   │
       │                │  3. Parse URL   │                   │
       │                │  Verify sig     │                   │
       │                │                 │                   │
       │                │  4. Join room   │                   │
       │                │ ────────────────┼──────────────────►│
       │                │                 │                   │
       │                │                 │  5. Connect       │
       │                │ ◄───────────────┼───────────────────│
       │                │                 │                   │
       │                │  6. Noise XX handshake              │
       │                │ ◄──────────────────────────────────►│
       │                │                 │                   │
       │                │  7. Verify fingerprint matches QR   │
       │                │                 │                   │
       │  8. Confirm?   │                 │                   │
       │ ◄──────────────│                 │                   │
       │                │                 │                   │
       │  9. Approve    │                 │                   │
       │ ──────────────►│  10. Save trust ├──────────────────►│
       │                │                 │   Save trust       │
       │                │                 │                   │
```

### Trust Levels

```rust
pub enum TrustLevel {
    Unknown,   // Not yet evaluated
    Trusted,   // Approved for connection
    Revoked,   // Explicitly denied
}
```

Trust decisions are persisted and survive restarts.

## Attack Surface Analysis

### Signaling Server Attacks

**Attack**: Compromised signaling server attempts MITM

**Mitigation**: The signaling server only relays encrypted Noise handshake messages. It cannot:
- Read plaintext (messages are encrypted)
- Forge messages (no access to private keys)
- Perform MITM (Noise handshake verifies identities)

**Residual Risk**: Traffic analysis (who talks to whom, timing)

### Replay Attacks

**Attack**: Adversary replays captured handshake messages

**Mitigation**:
- Ephemeral keys ensure each handshake is unique
- Noise protocol rejects replayed messages
- Nonces increment and cannot be reused

### Device Compromise

**Attack**: Adversary steals trusted device

**Mitigation**:
- Users can revoke device trust from any device
- Device keys are stored in OS keychain (with optional password)
- Sessions can be manually terminated

**Residual Risk**: Active sessions during compromise

### Denial of Service

**Attack**: Flood daemon with connection requests

**Mitigation**:
- Connection rate limiting (not yet implemented)
- Resource limits on concurrent sessions
- Signaling server can implement rate limiting

### File System Access

**Attack**: Unauthorized file access via file transfer

**Mitigation**:
- Configurable allowed paths (`file.allowed_paths`)
- Path traversal prevention
- Permission checking

## Security Recommendations

### For Users

1. **Verify QR Codes**: Always scan QR codes directly from the daemon's display
2. **Review Trust Requests**: Carefully review device approval requests
3. **Revoke Unused Devices**: Remove trust for devices no longer in use
4. **Keep Software Updated**: Security patches are released regularly

### For Deployment

1. **Restrict Allowed Paths**: Configure `file.allowed_paths` to minimum necessary
2. **Use Firewall**: Limit network exposure where possible
3. **Monitor Logs**: Watch for suspicious connection patterns
4. **Rotate Keys**: Periodically regenerate device identity (requires re-pairing)

### For Development

1. **Update Dependencies**: Regularly update cryptographic libraries
2. **Security Audit**: Periodic third-party security review
3. **Fuzzing**: Fuzz protocol parsing code
4. **Memory Safety**: Use Rust's memory safety guarantees

## Known Limitations

1. **No Perfect Forward Secrecy for Stored Data**: Trust stores contain public keys that could be used to identify historical connections if compromised

2. **No Hardware Key Support**: Currently software-only key storage

3. **No Multi-Factor Authentication**: Single factor (device possession)

4. **Limited Audit Logging**: Connection logs exist but may not capture all security events

## Security Contacts

Report security vulnerabilities privately to the maintainers. Include:
- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

Do not open public issues for security vulnerabilities.
