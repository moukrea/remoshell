# RemoShell

![Rust CI](https://github.com/remoshell/remoshell/workflows/Rust%20CI/badge.svg)
![Frontend CI](https://github.com/remoshell/remoshell/workflows/Frontend%20CI/badge.svg)

RemoShell is a secure remote shell access application that enables encrypted terminal sessions between devices over WebRTC and QUIC. It provides a modern alternative to SSH with end-to-end encryption, device trust management, and file transfer capabilities.

## Features

- **Secure Shell Sessions**: Create and manage remote terminal sessions with full PTY support
- **End-to-End Encryption**: All communication encrypted using the Noise Protocol Framework (XX pattern) with ChaCha20-Poly1305
- **Device Trust**: QR code-based pairing with persistent trust management
- **File Transfer**: Browse remote file systems and transfer files securely
- **Multiple Transports**: WebRTC for NAT traversal, QUIC for direct connections
- **Cross-Platform**: Daemon runs on Linux, client available as Tauri desktop app
- **Session Persistence**: Sessions survive reconnections with multiplexed data channels

## Quick Start

### Running the Daemon (Server)

```bash
# Build and run the daemon
cargo build --release -p daemon
./target/release/daemon

# Or run in development mode
cargo run -p daemon
```

The daemon will display a QR code for pairing and listen for incoming connections.

### Running the Client

```bash
cd client

# Install dependencies
npm ci

# Run in development mode (web)
npm run dev

# Run as desktop app
npm run tauri:dev
```

Scan the daemon's QR code with the client to establish a trusted connection.

## Platform Support

| Platform | Daemon | Client |
|----------|--------|--------|
| Linux x64 | Yes | Yes |
| Linux ARM64 | Yes | Yes |
| macOS x64 | Yes (untested) | Yes |
| macOS ARM64 | Yes (untested) | Yes |
| Windows | No | Yes |

## Architecture Overview

RemoShell consists of three main components:

```
+-------------+        WebRTC/QUIC        +-------------+
|   Client    | <--------------------->  |   Daemon    |
| (Tauri App) |                          | (Rust svc)  |
+-------------+        Signaling         +-------------+
       |                  |                    |
       v                  v                    v
+-------------+    +-------------+    +----------------+
| SolidJS UI  |    |  Cloudflare |    | PTY Sessions   |
| + xterm.js  |    |   Worker    |    | File Browser   |
+-------------+    +-------------+    +----------------+
```

- **Client**: SolidJS web app with Tauri for desktop, handles terminal rendering and file management
- **Daemon**: Rust service managing PTY sessions, file access, and connection handling
- **Signaling**: Cloudflare Worker coordinating WebRTC peer connections

For detailed architecture documentation, see [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Project Structure

```
remoshell/
├── crates/
│   ├── protocol/     # Protocol definitions, crypto, framing
│   ├── daemon/       # Background daemon service
│   └── tauri-client/ # Tauri client bindings
├── client/           # SolidJS frontend with Tauri desktop app
├── signaling/        # Cloudflare Worker for WebRTC signaling
└── docs/             # Documentation
```

## Development

### Prerequisites

- Rust 1.85+ (stable)
- Node.js 20+
- npm

### Rust Development

```bash
# Format code
cargo fmt

# Run clippy
cargo clippy

# Run tests
cargo test

# Generate documentation
cargo doc --open
```

### Frontend Development

```bash
cd client

# Install dependencies
npm ci

# Start dev server
npm run dev

# Run tests
npm test

# Build for production
npm run build
```

### Tauri Desktop App

```bash
cd client

# Development
npm run tauri:dev

# Production build
npm run tauri:build
```

### Signaling Worker

```bash
cd signaling

# Install dependencies
npm ci

# Local development
npm run dev

# Deploy to Cloudflare
npm run deploy
```

## Documentation

- [Quick Start Guide](docs/QUICKSTART.md) - **Step-by-step E2E setup guide**
- [Architecture](docs/ARCHITECTURE.md) - System design and component overview
- [Protocol](docs/PROTOCOL.md) - Wire protocol specification
- [Security](docs/SECURITY.md) - Security model and threat analysis
- [Performance](docs/PERFORMANCE.md) - Performance targets and optimization
- [Contributing](CONTRIBUTING.md) - Development setup and contribution guidelines

## Security

RemoShell is designed with security as a primary concern:

- **Mutual Authentication**: Both client and daemon authenticate using Ed25519 keys
- **Perfect Forward Secrecy**: Noise XX handshake ensures past sessions cannot be decrypted
- **Trust-on-First-Use**: QR code pairing establishes initial trust with visual verification
- **No Server Access**: The signaling server only relays encrypted messages, never sees plaintext

For full security documentation, see [docs/SECURITY.md](docs/SECURITY.md).

## License

MIT OR Apache-2.0
