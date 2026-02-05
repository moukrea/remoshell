# PRD: Remote Terminal P2P

> **Status:** Draft v1.0  
> **Auteur:** Emeric  
> **Date:** 2025-01-30  
> **Licence:** MIT  

---

## 1. Vision

Permettre à n'importe qui d'accéder à un terminal distant sur sa propre machine depuis n'importe où dans le monde, sans configuration réseau (DNS, ports, firewall), sans serveur intermédiaire pour les données, et avec une sécurité robuste par design.

**En une phrase :** Un tmux accessible de partout via un simple QR code.

---

## 2. Problème

Accéder à un terminal distant aujourd'hui requiert :

| Solution | Contraintes |
|----------|-------------|
| SSH classique | IP publique ou dyndns, port forwarding, firewall config |
| Tailscale/ZeroTier | Compte requis, installation des deux côtés, trust d'un tiers |
| ngrok/cloudflared | Dépendance service tiers, latence, coûts potentiels |
| TeamViewer/AnyDesk | Lourd, pas orienté terminal, propriétaire |

**Aucune solution ne permet :** lancer un binaire → scanner un QR → terminal accessible. C'est ce gap que ce projet comble.

---

## 3. Utilisateurs cibles

### Persona 1 : Homelab enthusiast
- Veut accéder à ses machines depuis n'importe où
- Ne veut pas exposer de ports ni maintenir de VPN
- Valorise le self-hosted et l'open source

### Persona 2 : Développeur nomade  
- Travaille sur des machines de dev distantes
- Connexion depuis laptop, téléphone, tablette
- Besoin ponctuel, pas de setup permanent

### Persona 3 : Ops/SRE en astreinte
- Doit intervenir rapidement sur une machine
- Parfois depuis mobile uniquement
- Sécurité et traçabilité importantes

---

## 4. Principes directeurs

1. **Zero config réseau** — Aucun DNS, port forwarding, ou règle firewall requis
2. **Zero infrastructure** — Pas de serveur à maintenir, données 100% P2P
3. **Security by default** — Chiffrement E2E, approbation explicite, pas de trust implicite
4. **Offline-first** — Fonctionne tant que les deux peers ont internet, peu importe comment
5. **Ephemeral pairing** — Les codes de connexion sont jetables et courts

---

## 5. Architecture système

### 5.1 Vue d'ensemble

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              INFRASTRUCTURE                                 │
│                          (Zero maintenance)                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐        │
│   │  GitHub Pages   │    │    Cloudflare   │    │  STUN publics   │        │
│   │   (Frontend)    │    │     Worker      │    │ (Google, etc.)  │        │
│   │                 │    │   (Signaling)   │    │                 │        │
│   │  Static files   │    │   Stateless     │    │  NAT traversal  │        │
│   │  100% gratuit   │    │   ~50 lignes    │    │  Gratuit        │        │
│   └─────────────────┘    └─────────────────┘    └─────────────────┘        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                               HOST DAEMON                                   │
│                          (Linux / macOS / WSL)                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │
│   │   Network   │  │   Session   │  │    File     │  │   Device    │       │
│   │   Layer     │  │   Manager   │  │   Manager   │  │   Manager   │       │
│   │             │  │             │  │             │  │             │       │
│   │ • WebRTC    │  │ • PTY spawn │  │ • Upload    │  │ • Trust DB  │       │
│   │ • QUIC      │  │ • I/O mux   │  │ • Download  │  │ • Revoke    │       │
│   │ • Signaling │  │ • Lifecycle │  │ • Browse    │  │ • Logs      │       │
│   └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘       │
│                                                                             │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                         PTY Sessions                                │   │
│   │   ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐               │   │
│   │   │ zsh #1  │  │ bash #2 │  │ htop #3 │  │ vim #4  │  ...          │   │
│   │   └─────────┘  └─────────┘  └─────────┘  └─────────┘               │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│   Interfaces:                                                               │
│   • CLI (headless)     ./remote-term serve                                  │
│   • TUI (interactive)  ./remote-term serve --tui                            │
│   • Systemd service    systemctl start remote-term                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

                                    │
                                    │  P2P (WebRTC / QUIC)
                                    │  Chiffré E2E
                                    │
                                    ▼

┌─────────────────────────────────────────────────────────────────────────────┐
│                                CLIENTS                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌─────────────────────────────┐    ┌─────────────────────────────┐       │
│   │       WEB CLIENT            │    │       TAURI APP             │       │
│   │      (Browser)              │    │  (Desktop & Mobile)         │       │
│   │                             │    │                             │       │
│   │  • GitHub Pages hosted      │    │  • Linux                    │       │
│   │  • WebRTC only              │    │  • macOS                    │       │
│   │  • Zero install             │    │  • Windows                  │       │
│   │  • Scan QR or enter code    │    │  • Android                  │       │
│   │                             │    │  • iOS                      │       │
│   │                             │    │                             │       │
│   │                             │    │  • QUIC (faster) + WebRTC   │       │
│   │                             │    │  • Native notifications     │       │
│   │                             │    │  • Persistent device keys   │       │
│   │                             │    │  • QR scanner intégré       │       │
│   └─────────────────────────────┘    └─────────────────────────────┘       │
│                                                                             │
│   UI partagée (même codebase):                                              │
│   • Terminal tabs                                                           │
│   • File browser                                                            │
│   • Session list                                                            │
│   • Device management                                                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Composants détaillés

#### 5.2.1 Signaling Worker (Cloudflare)

**Rôle :** Relai éphémère pour l'échange WebRTC offer/answer. Ne voit jamais les données.

**Caractéristiques :**
- Stateless (mémoire volatile uniquement)
- TTL 60 secondes par room
- Rate limiting par IP (natif Cloudflare)
- ~50 lignes de code
- Zero coût (free tier: 100k req/jour)

**Endpoints :**
```
POST /room/{room_id}/offer    → Dépose l'offer (host)
GET  /room/{room_id}/offer    → Récupère l'offer (client)
POST /room/{room_id}/answer   → Dépose l'answer (client)  
GET  /room/{room_id}/answer   → Récupère l'answer (host, long-poll)
```

**Données transitant :**
- SDP offers/answers (metadata connexion, pas de données user)
- HMAC du PIN (pour validation)
- Aucune donnée terminal, aucun fichier

#### 5.2.2 Host Daemon

**Rôle :** Tourne sur la machine à contrôler. Gère les sessions, fichiers, et connexions.

**Modules :**

| Module | Responsabilité |
|--------|----------------|
| `network` | WebRTC (browsers) + QUIC (Tauri), signaling, hole punching |
| `session` | Spawn/kill PTY, multiplexage I/O, resize |
| `files` | Browse, upload, download, permissions |
| `devices` | Trust store, approbation, révocation, logs |
| `ui` | CLI args, TUI optionnelle, systemd notify |

**Configuration :** `~/.config/remote-term/config.toml`
```toml
[server]
shell = "/bin/zsh"                    # Shell par défaut
allowed_shells = ["/bin/zsh", "/bin/bash"]

[security]
pin_length = 4                        # 4-8 digits
auto_trust = false                    # Toujours demander approbation
session_on_connect = true             # Créer une session à la connexion

[notifications]
notify_on_connect = true
notify_on_disconnect = true

[files]
default_directory = "~"               # Répertoire par défaut file browser
max_upload_chunk = "10MB"             # Taille chunk pour streaming
```

**Stockage local :**
```
~/.config/remote-term/
├── config.toml
├── trusted_devices.json
├── host_keypair.json
└── logs/
    └── connections.log
```

#### 5.2.3 Client Web

**Stack :**
- Framework: SolidJS (léger, réactif)
- Terminal: xterm.js + xterm-addon-fit + xterm-addon-webgl
- WebRTC: simple-peer
- Styling: UnoCSS
- Build: Vite
- Hosting: GitHub Pages

**Limitations browser :**
- WebRTC uniquement (pas de QUIC raw)
- Pas de persistance clés cross-session (sauf localStorage)
- Pas de notifications push (sauf si PWA + service worker)
- Pas de scan QR natif (utilise caméra via getUserMedia)

#### 5.2.4 Client Tauri

**Stack :**
- Frontend: Même SPA que le web client
- Backend: Rust (iroh-net pour QUIC, webrtc-rs pour compat browser)
- Persistance: SQLite local (clés, hosts connus)

**Avantages vs browser :**
- QUIC direct (latence réduite vs WebRTC)
- Notifications natives
- Scan QR via caméra native
- Raccourcis clavier globaux
- Stockage sécurisé des clés (keychain OS)

**Plateformes :**
| Plateforme | Status v1 | Notes |
|------------|-----------|-------|
| Linux | ✅ | x64, arm64 |
| macOS | ✅ | Universal binary |
| Windows | ✅ | x64 |
| Android | ✅ | Via Tauri 2.0 mobile |
| iOS | ✅ | Via Tauri 2.0 mobile |

---

## 6. Flux utilisateur

### 6.1 Premier pairing (nouveau device)

```
HOST                                            CLIENT
────                                            ──────

1. $ remote-term serve
   
   ┌────────────────────────────────────┐
   │  Remote Terminal                   │
   │                                    │
   │  Code: AXBK-7392                   │
   │                                    │
   │  ████████████████                  │
   │  ██            ██                  │
   │  ██  ████████  ██  ← QR contient:  │
   │  ██  ████████  ██    https://      │
   │  ██            ██    remote-term.  │
   │  ████████████████    github.io/    │
   │                      #AXBK-7392    │
   │  Waiting for connection...         │
   │  Press Ctrl+C to cancel            │
   └────────────────────────────────────┘

                                        2. OPTION A: Scan QR → ouvre browser
                                           OPTION B: Va sur site, tape code
                                           OPTION C: App Tauri, scan ou tape

                                        3. Client établit connexion WebRTC/QUIC

4. Connexion reçue
   Vérifie HMAC (PIN valide?)
   ├─ Invalide → ignore silencieux
   └─ Valide → continue

5. Prompt approbation:
   ┌────────────────────────────────────┐
   │  Connection request               │
   │                                    │
   │  📱 iPhone Emeric                  │
   │     ID: x8kj...2f4                 │
   │     IP: 86.242.x.x                 │
   │                                    │
   │  [A]ccept  [R]eject                │
   │  [ ] Always trust this device      │
   └────────────────────────────────────┘

6. Si Accept:
   - Ajoute device au trust store (si coché)
   - Envoie liste sessions existantes
                                        
                                        7. UI s'affiche:
                                           ┌─────────────────────────────┐
                                           │ Sessions  │  Terminal       │
                                           │           │                 │
                                           │ + New     │  $ _            │
                                           │ ───────── │                 │
                                           │ #1 zsh    │                 │
                                           │ #2 htop ● │                 │
                                           │           │                 │
                                           └─────────────────────────────┘
```

### 6.2 Reconnexion (device trusté)

```
HOST                                            CLIENT (Tauri)
────                                            ──────

1. Daemon tourne en background                  2. Ouvre app
   (pas besoin d'afficher le code)                 Voit liste hosts connus:
                                                   ┌─────────────────────┐
                                                   │  Hosts              │
                                                   │                     │
                                                   │  🟢 homelab         │
                                                   │  🔴 workstation     │
                                                   │                     │
                                                   │  + Add new host     │
                                                   └─────────────────────┘

                                                3. Click "homelab"

4. Connexion entrante
   Device ID reconnu + keypair valide
   → Auto-accept (trusted)
   
5. Log: "iPhone Emeric connected"
                                                6. Directement sur UI terminal
```

### 6.3 Gestion des sessions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              CLIENT UI                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────────────────┐  ┌────────────────────────────────────────────┐   │
│  │  SESSIONS            │  │  TERMINAL                                  │   │
│  │                      │  │                                            │   │
│  │  + New session       │  │  emeric@homelab:~$ neofetch               │   │
│  │  ──────────────────  │  │                                            │   │
│  │                      │  │         ▄▄▄▄▄▄▄▄▄▄▄▄▄▄                     │   │
│  │  #1 zsh         ● ←──┼──│         ██████████████                     │   │
│  │     ~/projects       │  │         ██████████████  emeric@homelab     │   │
│  │     idle 5m          │  │         ██████████████  OS: Arch Linux     │   │
│  │                      │  │                         Kernel: 6.7.0      │   │
│  │  #2 htop             │  │                                            │   │
│  │     running          │  │  emeric@homelab:~$ █                       │   │
│  │                      │  │                                            │   │
│  │  #3 nvim        ●    │  │                                            │   │
│  │     ~/.config/nvim   │  │                                            │   │
│  │     modified         │  │                                            │   │
│  │                      │  │                                            │   │
│  │  ──────────────────  │  ├────────────────────────────────────────────┤   │
│  │  × Kill session      │  │  ● = process actif                         │   │
│  │                      │  │  Ctrl+T = new tab  │  Ctrl+W = close       │   │
│  └──────────────────────┘  └────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

Actions disponibles:
• Click session → attach (switch terminal)
• "+ New session" → spawn nouveau PTY
• "× Kill session" → termine le PTY (avec confirmation)
• Sessions persistent même si client déconnecté
```

### 6.4 Transfert de fichiers

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              FILE BROWSER                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  📁 /home/emeric/projects                              ↑ Upload     │   │
│  ├─────────────────────────────────────────────────────────────────────┤   │
│  │                                                                     │   │
│  │  📁 ..                                                              │   │
│  │  📁 remote-term/                              2.3 MB    Jan 30      │   │
│  │  📁 homelab/                                  156 KB    Jan 28      │   │
│  │  📄 notes.md                                  4.2 KB    Jan 29  ↓   │   │
│  │  📄 docker-compose.yml                        1.8 KB    Jan 25  ↓   │   │
│  │  🖼️ screenshot.png                            234 KB    Jan 20  ↓   │   │
│  │                                                                     │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  Actions:                                                                   │
│  • Double-click folder → navigate                                           │
│  • Click ↓ → download file                                                  │
│  • Click "Upload" → file picker → upload to current dir                     │
│  • Drag & drop files → upload to current dir                                │
│  • Right-click → context menu (rename, delete, download)                    │
│                                                                             │
│  Quick paths:                                                               │
│  [Home] [Session CWD] [/tmp] [Custom path...]                               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘

Transfert:
• Streaming chunked (pas de limite de taille)
• Progress bar pour gros fichiers  
• Résumable en cas de déconnexion temporaire (Tauri only)
```

---

## 7. Sécurité

### 7.1 Modèle de menace

| Menace | Mitigation |
|--------|------------|
| Attaquant bruteforce le code | PIN dans le code (10k combinaisons), rate limit worker |
| Attaquant intercepte le QR | QR visible uniquement localement, code éphémère (60s) |
| MITM sur la connexion | Chiffrement E2E (Noise protocol / DTLS), échange clés hors-bande |
| Device volé avec trust | Révocation possible côté host, re-pairing nécessaire |
| Worker compromis | Ne voit que metadata signaling, jamais les données |
| Host daemon compromis | C'est game over de toute façon (c'est la machine elle-même) |

### 7.2 Chiffrement

```
Couche transport:
• WebRTC: DTLS 1.2+ (obligatoire par spec WebRTC)
• QUIC: TLS 1.3 intégré

Couche application:
• Noise Protocol (XX handshake)
• Clés Ed25519 (identité device)
• ChaCha20-Poly1305 (données)

Forward secrecy:
• Nouvelles clés session à chaque connexion
• Clés éphémères X25519
```

### 7.3 Approbation et trust

```
Niveaux de trust:

1. UNKNOWN (nouveau device)
   → Toujours prompt approbation
   → PIN valide requis
   
2. TRUSTED (device approuvé avec "always trust")
   → Auto-accept si keypair match
   → Log de connexion
   
3. REVOKED (device explicitement révoqué)
   → Connexion refusée automatiquement
   → Alerte côté host
```

### 7.4 Logs et audit

```
~/.config/remote-term/logs/connections.log

Format:
[2025-01-30T14:32:15Z] CONNECT device="iPhone Emeric" id=x8kj...2f4 ip=86.242.x.x trusted=true
[2025-01-30T14:32:15Z] SESSION_CREATE device=x8kj...2f4 session=#1 shell=/bin/zsh
[2025-01-30T14:45:00Z] SESSION_ATTACH device=x8kj...2f4 session=#2
[2025-01-30T15:00:00Z] DISCONNECT device=x8kj...2f4 reason=client_closed
[2025-01-30T15:30:00Z] CONNECT_REJECTED device=unknown id=malicious ip=1.2.3.4 reason=invalid_pin
```

---

## 8. Notifications

### 8.1 Événements notifiables

| Événement | Notification | Configurable |
|-----------|--------------|--------------|
| Process terminé (session en background) | "Session #2 (npm install) finished" | ✅ |
| Nouveau client connecté | "iPhone connected to homelab" | ✅ |
| Client déconnecté | "iPhone disconnected" | ✅ |
| Session créée par autre client | "New session #3 created by MacBook" | ✅ |
| Connexion refusée (mauvais PIN) | Silent log only | ❌ |

### 8.2 Implémentation par plateforme

| Plateforme | Méthode |
|------------|---------|
| Tauri Desktop | Native notifications (notify-rust) |
| Tauri Mobile | Push notifications (OS native) |
| Web | Notification API (si permission accordée) |
| Host TUI | Inline alert dans l'UI |

---

## 9. Protocole RPC

### 9.1 Messages Client → Host

```rust
enum ClientMessage {
    // Sessions
    ListSessions,
    CreateSession { shell: Option<String> },
    AttachSession { id: SessionId },
    DetachSession,
    KillSession { id: SessionId },
    
    // Terminal I/O
    Input { data: Bytes },
    Resize { cols: u16, rows: u16 },
    
    // Files
    ListDirectory { path: PathBuf },
    DownloadFile { path: PathBuf },
    UploadFile { path: PathBuf, size: u64 },
    UploadChunk { upload_id: Uuid, offset: u64, data: Bytes },
    DeletePath { path: PathBuf },
    
    // Device
    GetDeviceInfo,
    Ping,
}
```

### 9.2 Messages Host → Client

```rust
enum HostMessage {
    // Sessions
    SessionList { sessions: Vec<SessionInfo> },
    SessionCreated { id: SessionId },
    SessionAttached { id: SessionId, initial_output: Bytes },
    SessionEnded { id: SessionId, exit_code: Option<i32> },
    
    // Terminal I/O
    Output { data: Bytes },
    
    // Files  
    DirectoryListing { path: PathBuf, entries: Vec<FileEntry> },
    FileChunk { download_id: Uuid, offset: u64, data: Bytes, done: bool },
    UploadAccepted { upload_id: Uuid },
    UploadComplete { path: PathBuf },
    
    // Device
    DeviceInfo { hostname: String, os: String, sessions_count: usize },
    
    // Notifications
    Notification { kind: NotificationKind, message: String },
    
    // Errors
    Error { code: ErrorCode, message: String },
    
    Pong,
}

struct SessionInfo {
    id: SessionId,
    shell: String,
    cwd: PathBuf,
    created_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
    has_running_process: bool,
    title: Option<String>,  // Titre dynamique (si le shell le supporte)
}

struct FileEntry {
    name: String,
    kind: FileKind,  // File, Directory, Symlink
    size: u64,
    modified: DateTime<Utc>,
    permissions: u32,
}
```

### 9.3 Sérialisation

- Format: MessagePack (via `rmp-serde`)
- Framing: Length-prefixed (4 bytes big-endian)
- Compression: LZ4 pour chunks fichiers > 1KB

---

## 10. Stack technique

### 10.1 Host Daemon (Rust)

| Composant | Crate | Version | Notes |
|-----------|-------|---------|-------|
| Async runtime | `tokio` | 1.x | |
| WebRTC | `webrtc-rs` | 0.x | Pour browsers |
| QUIC | `iroh-net` | 0.x | Pour Tauri, meilleure perf |
| PTY | `portable-pty` | 0.x | Cross-platform |
| Serialization | `rmp-serde` | 1.x | MessagePack |
| Crypto | `snow` | 0.x | Noise protocol |
| TUI | `ratatui` | 0.x | Mode interactif |
| CLI | `clap` | 4.x | Args parsing |
| Config | `toml` | 0.x | Fichier config |
| Logging | `tracing` | 0.x | Structured logs |
| Systemd | `sd-notify` | 0.x | Intégration systemd |

### 10.2 Client Web

| Composant | Techno | Notes |
|-----------|--------|-------|
| Framework | SolidJS 1.x | Réactif, léger (~7kb) |
| Terminal | xterm.js 5.x | Standard de facto |
| WebRTC | simple-peer | Wrapper propre |
| Styling | UnoCSS | Atomic CSS |
| Icons | Lucide | Consistent, tree-shakeable |
| Build | Vite 5.x | Fast HMR |

### 10.3 Client Tauri

| Composant | Techno | Notes |
|-----------|--------|-------|
| Framework | Tauri 2.0 | Mobile support |
| Frontend | Même SPA | Code sharing |
| QUIC | iroh-net | Via Rust backend |
| Storage | SQLite | rusqlite |
| Keychain | keyring-rs | Secure storage |
| Camera | tauri-plugin-barcode-scanner | QR scanning |
| Notifications | tauri-plugin-notification | Native |

### 10.4 Signaling Worker

| Composant | Techno | Notes |
|-----------|--------|-------|
| Runtime | Cloudflare Workers | Edge, gratuit |
| Language | JavaScript | Simple, pas de build |
| Storage | In-memory Map | Stateless, volatile |

---

## 11. Structure du projet

```
remote-term/
├── README.md
├── LICENSE                          # MIT
├── Cargo.toml                       # Workspace
│
├── crates/
│   ├── daemon/                      # Host daemon
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── network/
│   │       │   ├── mod.rs
│   │       │   ├── webrtc.rs
│   │       │   ├── quic.rs
│   │       │   └── signaling.rs
│   │       ├── session/
│   │       │   ├── mod.rs
│   │       │   ├── pty.rs
│   │       │   └── manager.rs
│   │       ├── files/
│   │       │   ├── mod.rs
│   │       │   ├── browser.rs
│   │       │   └── transfer.rs
│   │       ├── devices/
│   │       │   ├── mod.rs
│   │       │   ├── trust.rs
│   │       │   └── logs.rs
│   │       └── ui/
│   │           ├── mod.rs
│   │           ├── cli.rs
│   │           └── tui.rs
│   │
│   ├── protocol/                    # Shared protocol definitions
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── messages.rs
│   │       └── crypto.rs
│   │
│   └── tauri-client/                # Tauri Rust backend
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── connection.rs
│           ├── storage.rs
│           └── commands.rs          # Tauri IPC commands
│
├── client/                          # Shared frontend (SolidJS)
│   ├── package.json
│   ├── vite.config.ts
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── components/
│       │   ├── Terminal.tsx
│       │   ├── SessionList.tsx
│       │   ├── FileBrowser.tsx
│       │   ├── PairingInput.tsx
│       │   └── DeviceManager.tsx
│       ├── lib/
│       │   ├── connection.ts        # WebRTC + Tauri bridge
│       │   ├── protocol.ts
│       │   └── storage.ts
│       └── stores/
│           ├── sessions.ts
│           └── connection.ts
│
├── tauri/                           # Tauri app wrapper
│   ├── tauri.conf.json
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
│
├── worker/                          # Cloudflare Worker
│   ├── wrangler.toml
│   └── src/
│       └── index.js                 # ~50 lignes
│
├── docs/
│   ├── ARCHITECTURE.md
│   ├── PROTOCOL.md
│   └── SECURITY.md
│
└── .github/
    └── workflows/
        ├── ci.yml
        ├── release-daemon.yml       # Linux/macOS binaries
        ├── release-tauri.yml        # All platforms
        └── deploy-web.yml           # GitHub Pages
```

---

## 12. Scope v1 vs Futur

### 12.1 v1.0 (MVP)

| Feature | Status |
|---------|--------|
| Pairing QR code / code simple | ✅ |
| Approbation manuelle connexion | ✅ |
| Multi-sessions PTY | ✅ |
| Terminal UI (tabs) | ✅ |
| File browser + upload/download | ✅ |
| Trusted devices | ✅ |
| Révocation device | ✅ |
| Logs connexion | ✅ |
| Notifications (process fini) | ✅ |
| Web client | ✅ |
| Tauri Desktop (Linux/Mac/Win) | ✅ |
| Tauri Mobile (Android/iOS) | ✅ |
| Host daemon (Linux/Mac) | ✅ |
| TUI mode host | ✅ |
| Systemd service | ✅ |

### 12.2 Futur (post-v1)

| Feature | Priorité | Notes |
|---------|----------|-------|
| Split panes UI | Medium | Alternative à tmux intégré |
| Reconnexion auto | Medium | Surtout mobile |
| Multi-host depuis un client | Medium | Liste de machines |
| Shared sessions (multi-user same session) | Low | Collaboration |
| Recording/playback sessions | Low | Audit, training |
| Port forwarding | Low | Tunnel TCP arbitraire |
| Clipboard sync | Low | Copy/paste cross-device |
| Themes terminal | Low | Cosmétique |

---

## 13. Métriques de succès

### 13.1 Techniques

| Métrique | Cible |
|----------|-------|
| Latence pairing (scan → connected) | < 3s |
| Latence input → output (P2P établi) | < 50ms |
| Taux de succès hole punching | > 95% |
| Taille binaire daemon | < 20MB |
| Taille app Tauri | < 30MB |
| Consommation RAM daemon (idle) | < 50MB |

### 13.2 UX

| Métrique | Cible |
|----------|-------|
| Steps pour première connexion | ≤ 3 (lancer daemon, scan QR, approuver) |
| Steps pour reconnexion (trusted) | ≤ 1 (ouvrir app, click host) |
| Temps pour comprendre l'UI | < 1 min |

---

## 14. Risques et mitigations

| Risque | Impact | Probabilité | Mitigation |
|--------|--------|-------------|------------|
| WebRTC instable sur certains browsers | Medium | Medium | Fallback TURN relay, test matrix CI |
| NAT symétrique bloque hole punch | Low | Low (~5%) | TURN relay automatique |
| Cloudflare change pricing Workers | Low | Low | Code portable, self-host possible |
| iOS restrictions background | Medium | High | Clear UX que sessions persistent côté host |
| Adoption faible | N/A | N/A | Projet perso, pas un objectif business |

---

## 15. Questions ouvertes

1. **Nom du projet** — TBD, impacte domaine, branding, repos

2. **TURN relay** — Self-host un petit ou utiliser services gratuits existants ? (Metered.ca a un free tier)

3. **Authentification multi-factor** — PIN + device key suffisant ? Ou option TOTP pour les paranos ?

4. **Session sharing entre users** — Même host, multiple users connectés, voient-ils les mêmes sessions ? (Actuellement: oui, comme SSH)

5. **Limite sessions** — Cap arbitraire (ex: 20 sessions max) ou illimité ?

---

## Appendix A: Références

- [libp2p](https://libp2p.io/) — Stack P2P
- [iroh](https://iroh.computer/) — QUIC moderne en Rust  
- [simple-peer](https://github.com/feross/simple-peer) — WebRTC simplifié
- [xterm.js](https://xtermjs.org/) — Terminal web
- [Tauri](https://tauri.app/) — Framework app native
- [Noise Protocol](http://noiseprotocol.org/) — Crypto framework
- [WebRTC](https://webrtc.org/) — P2P browser

---

## Appendix B: Inspirations

- **Tailscale** — UX de "ça marche tout seul"
- **tmux** — Persistence sessions
- **magic-wormhole** — Pairing simple
- **Eternal Terminal** — Résilience connexion
- **ttyd** — Terminal web (mais serveur exposé)