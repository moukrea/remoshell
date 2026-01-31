# RemoShell Architecture

This document describes the system architecture of RemoShell, including component diagrams, data flows, and design decisions.

## System Overview

RemoShell is a distributed system with three main components:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           USER DEVICES                                   │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────┐                     ┌──────────────────────────┐  │
│  │   Client App     │                     │      Daemon              │  │
│  │   (Tauri/Web)    │                     │   (Linux Service)        │  │
│  │                  │                     │                          │  │
│  │ ┌──────────────┐ │    WebRTC/QUIC     │ ┌──────────────────────┐ │  │
│  │ │  Terminal UI │ │◄──────────────────►│ │   Session Manager    │ │  │
│  │ │  (xterm.js)  │ │    Noise XX        │ │      (PTY)           │ │  │
│  │ └──────────────┘ │    Encrypted       │ └──────────────────────┘ │  │
│  │                  │                     │                          │  │
│  │ ┌──────────────┐ │                     │ ┌──────────────────────┐ │  │
│  │ │ File Browser │ │                     │ │   File Transfer      │ │  │
│  │ └──────────────┘ │                     │ └──────────────────────┘ │  │
│  │                  │                     │                          │  │
│  │ ┌──────────────┐ │                     │ ┌──────────────────────┐ │  │
│  │ │Device Manager│ │                     │ │    Trust Store       │ │  │
│  │ └──────────────┘ │                     │ └──────────────────────┘ │  │
│  └────────┬─────────┘                     └────────────┬─────────────┘  │
│           │                                            │                 │
└───────────┼────────────────────────────────────────────┼─────────────────┘
            │                                            │
            │         ┌────────────────────┐             │
            └────────►│  Signaling Server  │◄────────────┘
                      │  (Cloudflare Worker)│
                      │                    │
                      │  - Room management │
                      │  - SDP relay       │
                      │  - ICE candidates  │
                      └────────────────────┘
```

## Daemon Architecture

The daemon is the core of RemoShell, running on the target machine to provide remote access.

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          DAEMON ORCHESTRATOR                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                      Message Router                              │    │
│  │   Routes incoming protocol messages to appropriate handlers      │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                    │                                     │
│         ┌──────────────────────────┼──────────────────────────┐         │
│         │                          │                          │         │
│         ▼                          ▼                          ▼         │
│  ┌──────────────┐          ┌──────────────┐          ┌──────────────┐  │
│  │   Session    │          │     File     │          │    Device    │  │
│  │   Manager    │          │   Handler    │          │    Trust     │  │
│  │              │          │              │          │    Store     │  │
│  │ - Create PTY │          │ - Browse     │          │              │  │
│  │ - Attach     │          │ - Download   │          │ - Add device │  │
│  │ - Resize     │          │ - Upload     │          │ - Revoke     │  │
│  │ - Kill       │          │ - Permissions│          │ - Verify     │  │
│  └──────────────┘          └──────────────┘          └──────────────┘  │
│         │                                                    │          │
│         ▼                                                    ▼          │
│  ┌──────────────┐                                    ┌──────────────┐  │
│  │ Multiplexer  │                                    │  JSON Store  │  │
│  │              │                                    │              │  │
│  │ - Channel    │                                    │ trusted_     │  │
│  │   mapping    │                                    │ devices.json │  │
│  │ - Flow ctrl  │                                    │              │  │
│  └──────────────┘                                    └──────────────┘  │
│                                                                          │
├─────────────────────────────────────────────────────────────────────────┤
│                         NETWORK LAYER                                    │
│                                                                          │
│  ┌─────────────────────────┐       ┌─────────────────────────────────┐  │
│  │   WebRTC Handler        │       │      QUIC Handler               │  │
│  │                         │       │                                 │  │
│  │ - ICE negotiation       │       │ - iroh-based connections        │  │
│  │ - Data channels:        │       │ - Bi-directional streams        │  │
│  │   * control (reliable)  │       │ - Certificate authentication    │  │
│  │   * shell (unreliable)  │       │                                 │  │
│  │   * file (reliable)     │       │                                 │  │
│  └─────────────────────────┘       └─────────────────────────────────┘  │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    Signaling Client                              │    │
│  │   WebSocket connection to signaling server for WebRTC setup     │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Session Management

Sessions are persistent PTY instances that survive client reconnections:

```
┌───────────────────────────────────────────────────────────────┐
│                    Session Manager                             │
├───────────────────────────────────────────────────────────────┤
│                                                                │
│  Sessions: HashMap<SessionId, Session>                        │
│                                                                │
│  ┌─────────────────────────────────────────────────────────┐  │
│  │  Session                                                 │  │
│  │  ├── id: UUID                                           │  │
│  │  ├── status: Running | Stopped | Exited                 │  │
│  │  ├── pty: PtyMaster                                     │  │
│  │  ├── attached_clients: Vec<DeviceId>                    │  │
│  │  └── created_at: Timestamp                              │  │
│  └─────────────────────────────────────────────────────────┘  │
│                                                                │
│  Operations:                                                   │
│  - create(cols, rows, shell, env) -> SessionId                │
│  - attach(session_id, device_id)                              │
│  - detach(session_id, device_id)                              │
│  - write_stdin(session_id, data)                              │
│  - read_stdout(session_id) -> Stream<bytes>                   │
│  - resize(session_id, cols, rows)                             │
│  - kill(session_id, signal)                                   │
│                                                                │
└───────────────────────────────────────────────────────────────┘
```

### UI Components

The daemon provides several user interface options:

```
┌────────────────────────────────────────────────────────────────┐
│                       UI Layer                                  │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐  ┌─────────────────┐  ┌────────────────┐  │
│  │    TUI App      │  │   QR Generator  │  │  systemd       │  │
│  │   (ratatui)     │  │                 │  │  Integration   │  │
│  │                 │  │ - Terminal QR   │  │                │  │
│  │ - Status view   │  │ - PNG QR        │  │ - sd_notify    │  │
│  │ - Session list  │  │ - Pairing URL   │  │ - Unit file    │  │
│  │ - Device list   │  │                 │  │   generation   │  │
│  │ - Approval UI   │  │                 │  │                │  │
│  └─────────────────┘  └─────────────────┘  └────────────────┘  │
│                                                                 │
└────────────────────────────────────────────────────────────────┘
```

## Client Architecture

The client is a SolidJS application with Tauri for desktop support.

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│                          CLIENT APPLICATION                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                      SolidJS UI Layer                            │    │
│  │                                                                  │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐   │    │
│  │  │   Terminal   │  │ File Browser │  │   Device Manager     │   │    │
│  │  │  Component   │  │  Component   │  │     Component        │   │    │
│  │  │              │  │              │  │                      │   │    │
│  │  │ - xterm.js   │  │ - Tree view  │  │ - Pairing UI         │   │    │
│  │  │ - Fit addon  │  │ - Upload/DL  │  │ - Trust management   │   │    │
│  │  │ - Web links  │  │ - Progress   │  │ - QR scanner         │   │    │
│  │  └──────────────┘  └──────────────┘  └──────────────────────┘   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                    │                                     │
│                                    ▼                                     │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                      State Stores (SolidJS)                      │    │
│  │                                                                  │    │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐   │    │
│  │  │ Connection   │  │   Sessions   │  │      Devices         │   │    │
│  │  │    Store     │  │    Store     │  │       Store          │   │    │
│  │  │              │  │              │  │                      │   │    │
│  │  │ - State      │  │ - List       │  │ - Paired devices     │   │    │
│  │  │ - Peer info  │  │ - Active     │  │ - Trust levels       │   │    │
│  │  │ - Reconnect  │  │ - History    │  │ - Last seen          │   │    │
│  │  └──────────────┘  └──────────────┘  └──────────────────────┘   │    │
│  │                                                                  │    │
│  │  ┌──────────────┐  ┌──────────────────────────────────────────┐ │    │
│  │  │    Files     │  │              Notifications               │ │    │
│  │  │    Store     │  │                 Store                    │ │    │
│  │  └──────────────┘  └──────────────────────────────────────────┘ │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                    │                                     │
│                                    ▼                                     │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                      Library Layer                               │    │
│  │                                                                  │    │
│  │  ┌──────────────────┐  ┌─────────────────┐  ┌────────────────┐  │    │
│  │  │  WebRTC Manager  │  │Signaling Client │  │   Protocol     │  │    │
│  │  │                  │  │                 │  │   Messages     │  │    │
│  │  │ - Peer conn      │  │ - WebSocket     │  │                │  │    │
│  │  │ - Data channels  │  │ - Reconnection  │  │ - MessagePack  │  │    │
│  │  │ - ICE handling   │  │ - Event system  │  │ - Framing      │  │    │
│  │  └──────────────────┘  └─────────────────┘  └────────────────┘  │    │
│  │                                                                  │    │
│  │  ┌──────────────────┐  ┌─────────────────────────────────────┐  │    │
│  │  │  Barcode Scanner │  │         Tauri IPC Bridge            │  │    │
│  │  │                  │  │                                     │  │    │
│  │  │ - Camera access  │  │ - Native notifications              │  │    │
│  │  │ - QR detection   │  │ - Keychain access                   │  │    │
│  │  │ - URL parsing    │  │ - SQLite storage                    │  │    │
│  │  └──────────────────┘  └─────────────────────────────────────┘  │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Tauri Integration

When running as a desktop app, Tauri provides native capabilities:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                       TAURI RUST BACKEND                                 │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    Command Handlers                              │    │
│  │                                                                  │    │
│  │  initialize_app()      connect_quic()        get_device_keys()  │    │
│  │  disconnect_quic()     send_quic_data()      get_connection_status()│ │
│  │  get_paired_devices()  store_paired_device() show_native_notification()│
│  └─────────────────────────────────────────────────────────────────┘    │
│                                    │                                     │
│         ┌──────────────────────────┼──────────────────────────┐         │
│         │                          │                          │         │
│         ▼                          ▼                          ▼         │
│  ┌──────────────┐          ┌──────────────┐          ┌──────────────┐  │
│  │ QUIC Manager │          │   Storage    │          │   Keychain   │  │
│  │              │          │              │          │              │  │
│  │ - iroh-based │          │ - SQLite DB  │          │ - OS keyring │  │
│  │ - Connection │          │ - Devices    │          │ - Keys       │  │
│  │   pooling    │          │ - Settings   │          │ - Secrets    │  │
│  └──────────────┘          └──────────────┘          └──────────────┘  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Signaling Flow

WebRTC connections are established through the signaling server:

```
     Client                    Signaling Server                    Daemon
        │                            │                               │
        │   1. WebSocket Connect     │                               │
        │   GET /room/{device_id}    │                               │
        │ ─────────────────────────► │                               │
        │                            │                               │
        │   2. Join confirmation     │                               │
        │   {type: "join", peerId}   │                               │
        │ ◄───────────────────────── │                               │
        │                            │                               │
        │                            │     3. WebSocket Connect      │
        │                            │     GET /room/{device_id}     │
        │                            │ ◄─────────────────────────────│
        │                            │                               │
        │   4. Peer joined           │     5. Join confirmation      │
        │   {type: "peer-joined"}    │     {type: "join", peerId}    │
        │ ◄───────────────────────── │ ─────────────────────────────►│
        │                            │                               │
        │   6. SDP Offer             │                               │
        │   {type: "offer", sdp}     │     7. Relay offer            │
        │ ─────────────────────────► │ ─────────────────────────────►│
        │                            │                               │
        │                            │     8. SDP Answer             │
        │   9. Relay answer          │     {type: "answer", sdp}     │
        │ ◄───────────────────────── │ ◄─────────────────────────────│
        │                            │                               │
        │  10. ICE Candidates        │    11. Relay candidates       │
        │ ─────────────────────────► │ ─────────────────────────────►│
        │ ◄───────────────────────── │ ◄─────────────────────────────│
        │                            │                               │
        │                            │                               │
        │◄══════════════════════════════════════════════════════════►│
        │              WebRTC Data Channel (P2P)                      │
        │                                                             │
```

## Data Flow

### Terminal Session Flow

```
┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐
│  User   │     │ xterm.js│     │ WebRTC  │     │ Daemon  │     │  PTY    │
│Keyboard │     │         │     │ Channel │     │ Router  │     │(shell)  │
└────┬────┘     └────┬────┘     └────┬────┘     └────┬────┘     └────┬────┘
     │               │               │               │               │
     │  1. Keypress  │               │               │               │
     │ ─────────────►│               │               │               │
     │               │               │               │               │
     │               │ 2. SessionData│               │               │
     │               │    (stdin)    │               │               │
     │               │ ─────────────►│               │               │
     │               │               │               │               │
     │               │               │ 3. Noise      │               │
     │               │               │    encrypt    │               │
     │               │               │ ─────────────►│               │
     │               │               │               │               │
     │               │               │               │ 4. Write to   │
     │               │               │               │    PTY master │
     │               │               │               │ ─────────────►│
     │               │               │               │               │
     │               │               │               │ 5. PTY output │
     │               │               │               │ ◄─────────────│
     │               │               │               │               │
     │               │               │ 6. SessionData│               │
     │               │               │    (stdout)   │               │
     │               │ ◄─────────────│ ◄─────────────│               │
     │               │               │               │               │
     │  7. Render    │               │               │               │
     │ ◄─────────────│               │               │               │
     │               │               │               │               │
```

### File Transfer Flow

```
┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐
│  User   │     │  File   │     │ WebRTC  │     │  File   │     │  File   │
│  (UI)   │     │ Browser │     │ Channel │     │Transfer │     │ System  │
└────┬────┘     └────┬────┘     └────┬────┘     └────┬────┘     └────┬────┘
     │               │               │               │               │
     │ 1. Request    │               │               │               │
     │    download   │               │               │               │
     │ ─────────────►│               │               │               │
     │               │               │               │               │
     │               │ 2. FileDownloadRequest        │               │
     │               │ ─────────────►│ ─────────────►│               │
     │               │               │               │               │
     │               │               │               │ 3. Read file  │
     │               │               │               │ ─────────────►│
     │               │               │               │               │
     │               │               │               │ 4. File data  │
     │               │               │               │ ◄─────────────│
     │               │               │               │               │
     │               │ 5. FileDownloadChunk (loop)   │               │
     │               │ ◄─────────────│ ◄─────────────│               │
     │               │               │               │               │
     │ 6. Save/Show  │               │               │               │
     │    progress   │               │               │               │
     │ ◄─────────────│               │               │               │
     │               │               │               │               │
```

## Security Architecture

See [SECURITY.md](SECURITY.md) for detailed security documentation.

### Trust Model

```
┌───────────────────────────────────────────────────────────────────────┐
│                         TRUST ESTABLISHMENT                            │
├───────────────────────────────────────────────────────────────────────┤
│                                                                        │
│   ┌──────────────┐                              ┌──────────────┐      │
│   │    Client    │                              │    Daemon    │      │
│   │              │                              │              │      │
│   │  Ed25519     │      QR Code Scan            │  Ed25519     │      │
│   │  Keypair     │ ◄───────────────────────────►│  Keypair     │      │
│   │              │  (Device ID + Room ID)       │              │      │
│   └──────────────┘                              └──────────────┘      │
│         │                                              │              │
│         │              Noise XX Handshake              │              │
│         │◄────────────────────────────────────────────►│              │
│         │                                              │              │
│   ┌──────────────┐                              ┌──────────────┐      │
│   │  Trust Store │                              │  Trust Store │      │
│   │              │       Mutual Trust           │              │      │
│   │  - Device ID │ ◄───────────────────────────►│  - Device ID │      │
│   │  - Public key│                              │  - Public key│      │
│   │  - Trust lvl │                              │  - Trust lvl │      │
│   └──────────────┘                              └──────────────┘      │
│                                                                        │
└───────────────────────────────────────────────────────────────────────┘
```

## Deployment Architecture

### Self-Hosted Deployment

```
┌────────────────────────────────────────────────────────────────────┐
│                      LOCAL NETWORK                                  │
├────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────────┐         ┌─────────────────────────────────┐   │
│  │   User Device   │         │       Server Machine            │   │
│  │                 │         │                                 │   │
│  │  ┌───────────┐  │  QUIC   │  ┌─────────────────────────┐    │   │
│  │  │  Client   │──┼────────►│  │    remoshell daemon     │    │   │
│  │  │  App      │  │         │  │                         │    │   │
│  │  └───────────┘  │         │  │  systemd service        │    │   │
│  │                 │         │  │  ~/.config/remoshell/   │    │   │
│  └─────────────────┘         │  └─────────────────────────┘    │   │
│                              │                                 │   │
└──────────────────────────────┴─────────────────────────────────┴───┘
                                        │
                                        │ WebRTC (optional)
                                        ▼
                              ┌─────────────────────┐
                              │  Signaling Server   │
                              │  (Cloudflare Worker)│
                              │                     │
                              │  For NAT traversal  │
                              │  and remote access  │
                              └─────────────────────┘
```

### Running as systemd Service

The daemon can be installed as a systemd user service:

```bash
# Generate unit file
remoshell daemon --generate-systemd

# Install and enable
systemctl --user enable remoshell
systemctl --user start remoshell
```

Unit file location: `~/.config/systemd/user/remoshell.service`
