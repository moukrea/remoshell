# RemoShell Protocol Specification

This document specifies the wire protocol used for communication between RemoShell clients and daemons.

## Overview

The RemoShell protocol is a binary protocol designed for:
- Low latency terminal sessions
- Efficient file transfers
- Secure device authentication

The protocol stack:
```
┌─────────────────────────────────────────┐
│          Application Messages           │  MessagePack-encoded
├─────────────────────────────────────────┤
│           Noise Encryption              │  ChaCha20-Poly1305
├─────────────────────────────────────────┤
│              Framing                    │  Length-prefixed, LZ4
├─────────────────────────────────────────┤
│         Transport (WebRTC/QUIC)         │  Data channels/streams
└─────────────────────────────────────────┘
```

## Frame Format

All messages are wrapped in frames for length-prefixed framing with optional compression.

### Frame Structure

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                         Magic (4 bytes)                        |
|                           "RMSH"                               |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Content Length (4 bytes)                   |
|                      (big-endian u32)                          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|    Flags     |                                                 |
|   (1 byte)   |               Payload (N bytes)                 |
+-+-+-+-+-+-+-+                                                   |
|                         ...                                    |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### Fields

| Field | Size | Description |
|-------|------|-------------|
| Magic | 4 bytes | Magic identifier `RMSH` (0x524D5348) |
| Content Length | 4 bytes | Length of flags + payload in big-endian |
| Flags | 1 byte | Frame flags (see below) |
| Payload | Variable | Frame payload (possibly compressed) |

### Frame Flags

| Bit | Name | Description |
|-----|------|-------------|
| 0 | COMPRESSED | Payload is LZ4-compressed |
| 1-7 | Reserved | Must be zero |

### Constants

```rust
const FRAME_MAGIC: [u8; 4] = *b"RMSH";
const COMPRESSION_THRESHOLD: usize = 1024;  // Compress payloads > 1KB
const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;  // 16 MB maximum
const FRAME_HEADER_SIZE: usize = 9;  // 4 + 4 + 1 bytes
```

### Compression

LZ4 compression is automatically applied to payloads larger than 1KB when compression would reduce size. The compressed payload includes a 4-byte little-endian size prefix (original size) followed by the compressed data.

## Message Envelope

All application messages are wrapped in an envelope for versioning and sequencing.

### Envelope Structure (MessagePack)

```
{
  "version": u8,      // Protocol version (currently 1)
  "sequence": u64,    // Message sequence number
  "payload": Message  // The actual message
}
```

### Message Types

Messages are tagged unions (sum types) encoded in MessagePack:

```
{
  "type": "<message_type>",
  "data": { ... }
}
```

## Session Messages

### SessionCreate

Request to create a new shell session.

```json
{
  "type": "SessionCreate",
  "data": {
    "cols": 80,
    "rows": 24,
    "shell": "/bin/bash",
    "env": [["TERM", "xterm-256color"], ["LANG", "en_US.UTF-8"]],
    "cwd": "/home/user"
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| cols | u16 | Yes | Terminal width in columns |
| rows | u16 | Yes | Terminal height in rows |
| shell | string | No | Shell command (default: user's login shell) |
| env | array | No | Environment variables as key-value pairs |
| cwd | string | No | Working directory |

### SessionCreated

Response confirming session creation.

```json
{
  "type": "SessionCreated",
  "data": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "pid": 12345
  }
}
```

### SessionData

Terminal I/O data.

```json
{
  "type": "SessionData",
  "data": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "stream": "Stdout",
    "data": "<base64-encoded bytes>"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| session_id | string | UUID of the session |
| stream | enum | `Stdin`, `Stdout`, or `Stderr` |
| data | bytes | Raw terminal data |

### SessionResize

Terminal resize notification.

```json
{
  "type": "SessionResize",
  "data": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "cols": 120,
    "rows": 40
  }
}
```

### SessionKill

Request to kill a session.

```json
{
  "type": "SessionKill",
  "data": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "signal": 9
  }
}
```

### SessionClosed

Session closed notification.

```json
{
  "type": "SessionClosed",
  "data": {
    "session_id": "550e8400-e29b-41d4-a716-446655440000",
    "exit_code": 0,
    "signal": null,
    "reason": "Process exited normally"
  }
}
```

## File Messages

### FileListRequest

Request directory listing.

```json
{
  "type": "FileListRequest",
  "data": {
    "path": "/home/user/documents",
    "include_hidden": true
  }
}
```

### FileListResponse

Directory listing response.

```json
{
  "type": "FileListResponse",
  "data": {
    "path": "/home/user/documents",
    "entries": [
      {
        "name": "file.txt",
        "entry_type": "File",
        "size": 1024,
        "mode": 420,
        "modified": 1704067200
      },
      {
        "name": "subdir",
        "entry_type": "Directory",
        "size": 0,
        "mode": 493,
        "modified": 1704067200
      }
    ]
  }
}
```

Entry types: `File`, `Directory`, `Symlink`, `Other`

### FileDownloadRequest

Request to download a file.

```json
{
  "type": "FileDownloadRequest",
  "data": {
    "path": "/home/user/file.txt",
    "offset": 0,
    "chunk_size": 65536
  }
}
```

### FileDownloadChunk

File download chunk.

```json
{
  "type": "FileDownloadChunk",
  "data": {
    "path": "/home/user/file.txt",
    "offset": 0,
    "total_size": 10240,
    "data": "<base64-encoded bytes>",
    "is_last": false
  }
}
```

### FileUploadStart

Start a file upload.

```json
{
  "type": "FileUploadStart",
  "data": {
    "path": "/home/user/upload.txt",
    "size": 10240,
    "mode": 420,
    "overwrite": true
  }
}
```

### FileUploadChunk

File upload chunk.

```json
{
  "type": "FileUploadChunk",
  "data": {
    "path": "/home/user/upload.txt",
    "offset": 0,
    "data": "<base64-encoded bytes>"
  }
}
```

### FileUploadComplete

Complete a file upload with checksum verification.

```json
{
  "type": "FileUploadComplete",
  "data": {
    "path": "/home/user/upload.txt",
    "checksum": "<sha256-bytes>"
  }
}
```

## Device Messages

### DeviceInfo

Device information announcement.

```json
{
  "type": "DeviceInfo",
  "data": {
    "device_id": "a1b2c3d4:e5f67890:12345678:9abcdef0",
    "name": "My Laptop",
    "os": "Linux",
    "os_version": "6.1.0",
    "arch": "x86_64",
    "protocol_version": 1
  }
}
```

### DeviceApprovalRequest

Request connection approval.

```json
{
  "type": "DeviceApprovalRequest",
  "data": {
    "device_id": "a1b2c3d4:e5f67890:12345678:9abcdef0",
    "name": "New Phone",
    "public_key": "<ed25519-public-key-bytes>",
    "reason": "Access requested"
  }
}
```

### DeviceApproved / DeviceRejected

Connection approval/rejection response.

```json
{
  "type": "DeviceApproved",
  "data": {
    "device_id": "a1b2c3d4:e5f67890:12345678:9abcdef0",
    "expires_at": 1735689600,
    "allowed_capabilities": ["shell", "file-read"]
  }
}
```

## Control Messages

### Ping / Pong

Keepalive and latency measurement.

```json
{
  "type": "Ping",
  "data": {
    "timestamp": 1704067200000,
    "payload": "<optional-echo-bytes>"
  }
}
```

### Capabilities

Capabilities announcement.

```json
{
  "type": "Capabilities",
  "data": {
    "protocol_versions": [1],
    "features": ["shell", "file-transfer", "device-trust"],
    "max_message_size": 1048576,
    "max_sessions": 16,
    "compression": ["lz4"]
  }
}
```

### Error

Error message.

```json
{
  "type": "Error",
  "data": {
    "code": "NotFound",
    "message": "Session not found",
    "context": "sess-unknown",
    "recoverable": false
  }
}
```

Error codes:
- `Unknown` - Unspecified error
- `Unauthorized` - Authentication/authorization failure
- `NotFound` - Resource not found
- `InvalidRequest` - Invalid request or parameters
- `InternalError` - Server-side error
- `Timeout` - Request timed out
- `RateLimited` - Rate limited
- `AlreadyExists` - Resource already exists
- `PermissionDenied` - Insufficient permissions
- `VersionMismatch` - Protocol version mismatch

## Noise Protocol Encryption

All messages are encrypted using the Noise Protocol Framework after handshake completion.

### Noise Pattern

```
Noise_XX_25519_ChaChaPoly_BLAKE2s
```

Components:
- **XX**: Mutual authentication pattern with identity hiding
- **25519**: Curve25519 for Diffie-Hellman key exchange
- **ChaChaPoly**: ChaCha20-Poly1305 for AEAD encryption
- **BLAKE2s**: BLAKE2s for hashing

### Handshake Flow

```
    Initiator                         Responder
        |                                  |
        |  1. -> e                         |
        | -------------------------------->|
        |                                  |
        |  2. <- e, ee, s, es              |
        |<-------------------------------- |
        |                                  |
        |  3. -> s, se                     |
        | -------------------------------->|
        |                                  |
        |    [Handshake Complete]          |
        |    [Transport Mode Active]       |
        |                                  |
```

Message meanings:
1. Initiator sends ephemeral public key
2. Responder sends ephemeral key, performs DH, sends encrypted static key
3. Initiator sends encrypted static key, performs final DH

### Key Derivation

Device identities use Ed25519 keys. For Noise (which requires X25519), keys are derived:

```rust
fn ed25519_to_x25519(ed_private: [u8; 32]) -> [u8; 32] {
    let hash = sha256(ed_private);
    let mut x_private = hash[0..32];

    // Clamp for X25519
    x_private[0] &= 248;
    x_private[31] &= 127;
    x_private[31] |= 64;

    x_private
}
```

### Constants

```rust
const MAX_NOISE_MESSAGE_SIZE: usize = 65535;
const NOISE_OVERHEAD: usize = 16;  // Poly1305 tag
```

## Data Channels

WebRTC connections use multiple data channels for different purposes:

| Channel | Label | Ordered | Reliable | Purpose |
|---------|-------|---------|----------|---------|
| Control | `control` | Yes | Yes | Handshake, device messages, errors |
| Shell | `shell` | No | No | Terminal I/O (low latency) |
| File | `file` | Yes | Yes | File transfers |

## Protocol Version

Current protocol version: **1**

Version negotiation:
1. Both sides send `Capabilities` message with supported versions
2. Highest common version is selected
3. `VersionMismatch` error if no common version exists

## MessagePack Encoding

Messages use MessagePack binary format for efficiency. Key considerations:

- Strings are encoded as UTF-8
- Binary data uses MessagePack `bin` format
- Enums are encoded as tagged objects: `{"type": "...", "data": {...}}`
- Optional fields are omitted when null/None
- Arrays preserve order

Example encoding of `SessionCreate`:

```
Hex: 85 A4 74 79 70 65 AD 53 65 73 73 69 6F 6E 43 72 65 61 74 65 ...
     |  | type  | "SessionCreate"                                  ...
```
