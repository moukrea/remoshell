# RemoteShell Quick Start Guide

This guide walks you through setting up RemoteShell for real-world use:
- Deploy signaling server to Cloudflare (free)
- Run daemon on your Linux server/PC
- Build and install Android app on your phone

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Deploy Signaling Server to Cloudflare](#deploy-signaling-server-to-cloudflare)
3. [Set Up the Daemon (Host Machine)](#set-up-the-daemon-host-machine)
4. [Build Android App](#build-android-app)
5. [Connect and Use](#connect-and-use)
6. [Local Development Setup](#local-development-setup)
7. [Troubleshooting](#troubleshooting)

---

## Prerequisites

### For Signaling Server (Cloudflare)
- [Cloudflare account](https://dash.cloudflare.com/sign-up) (free tier works)
- Node.js 20+ and npm

### For Daemon (Linux Host)
- Rust 1.85+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Linux (Ubuntu/Debian/Fedora/Arch)

### For Android App
- Node.js 20+ and npm
- Rust 1.85+
- Android Studio (for SDK and NDK)
- Java 17+

---

## Deploy Signaling Server to Cloudflare

The signaling server coordinates WebRTC connections. Cloudflare Workers free tier is plenty (100k requests/day).

### Step 1: Install Wrangler CLI

```bash
npm install -g wrangler
```

### Step 2: Login to Cloudflare

```bash
wrangler login
```

This opens a browser to authenticate.

### Step 3: Deploy

```bash
cd signaling
npm ci
npm run deploy
```

Output will show your worker URL:
```
Published remoshell-signaling
  https://remoshell-signaling.moukrea.workers.dev
```

**Save this URL** - you'll need it for the daemon and client.

### Step 4: Verify Deployment

```bash
curl https://remoshell-signaling.moukrea.workers.dev/health
# Should return: {"status":"ok","rooms":0}
```

### Custom Domain (Optional)

In Cloudflare Dashboard:
1. Go to Workers & Pages → your worker
2. Settings → Triggers → Custom Domains
3. Add your domain (e.g., `signal.yourdomain.com`)

---

## Set Up the Daemon (Host Machine)

The daemon runs on the machine you want to access remotely.

### Step 1: Build

```bash
cd /path/to/remoshell
cargo build --release -p daemon
```

### Step 2: Configure (Optional)

Create `~/.config/remoshell/config.toml`:

```toml
[network]
relay_url = "wss://remoshell-signaling.moukrea.workers.dev"

[session]
default_shell = "/bin/bash"
scrollback_lines = 3000

[security]
require_approval = true
```

### Step 3: Run

```bash
# Interactive TUI mode
./target/release/remoshell-daemon start

# Headless mode (for servers)
./target/release/remoshell-daemon start --headless

# With your signaling server
./target/release/remoshell-daemon start
```

### Step 4: Install as Service (Recommended)

```bash
# Create systemd user service
mkdir -p ~/.config/systemd/user

cat > ~/.config/systemd/user/remoshell.service << 'EOF'
[Unit]
Description=RemoteShell Daemon
After=network.target

[Service]
Type=notify
ExecStart=%h/.local/bin/remoshell-daemon start --systemd
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

# Install binary
mkdir -p ~/.local/bin
cp target/release/remoshell-daemon ~/.local/bin/

# Enable and start
systemctl --user daemon-reload
systemctl --user enable remoshell
systemctl --user start remoshell

# Check status
systemctl --user status remoshell
```

---

## Build Android App

### Step 1: Install Android Prerequisites

**Android Studio:**
1. Download from https://developer.android.com/studio
2. Install and open Android Studio
3. Go to Settings → Languages & Frameworks → Android SDK
4. Install:
   - SDK Platform: Android 14 (API 34)
   - SDK Tools: NDK (Side by side), CMake, Android SDK Command-line Tools

**Set Environment Variables** (add to `~/.bashrc` or `~/.zshrc`):

```bash
export ANDROID_HOME="$HOME/Android/Sdk"
export NDK_HOME="$ANDROID_HOME/ndk/$(ls -1 $ANDROID_HOME/ndk | tail -1)"
export PATH="$PATH:$ANDROID_HOME/platform-tools"
export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin"
```

Reload: `source ~/.bashrc`

**Install Rust Android Targets:**

```bash
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
```

### Step 2: Initialize Tauri Android

```bash
cd client

# Install dependencies
npm ci

# Initialize Android project
npm run tauri android init
```

This creates `src-tauri/gen/android/` with the Android project.

### Step 3: Configure Signaling URL

Edit `client/src/lib/signaling/SignalingClient.ts` and update the default URL:

```typescript
const DEFAULT_SIGNALING_URL = 'wss://remoshell-signaling.moukrea.workers.dev';
```

Or the app will prompt for it on first use.

### Step 4: Build APK

```bash
cd client

# Debug build (faster, for testing)
npm run tauri android build -- --target aarch64 --debug

# Release build (optimized, smaller)
npm run tauri android build -- --target aarch64 --release
```

APK location:
- Debug: `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`
- Release: `src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk`

### Step 5: Install on Phone

**Option A: ADB Install**
```bash
# Enable USB debugging on your phone
# Connect via USB

adb install src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

**Option B: Direct Transfer**
1. Copy APK to phone
2. Open with file manager
3. Allow "Install from unknown sources" when prompted

### Building for Multiple Architectures

```bash
# ARM64 (most modern phones)
npm run tauri android build -- --target aarch64

# ARM32 (older phones)
npm run tauri android build -- --target armv7

# All architectures (universal APK, larger file)
npm run tauri android build
```

---

## Connect and Use

### Step 1: Generate Pairing Code on Daemon

```bash
# Using your deployed signaling server
./target/release/remoshell-daemon pair --relay-url wss://remoshell-signaling.moukrea.workers.dev

# Or if configured in config.toml, just:
./target/release/remoshell-daemon pair
```

This displays a QR code in your terminal.

### Step 2: Scan with Android App

1. Open RemoShell app on your phone
2. Tap **"Add Device"** or the **+** button
3. Point camera at the QR code on your computer
4. Or tap "Enter manually" and type the code

### Step 3: Approve on Daemon

If running with TUI:
1. Press `4` or `Tab` to go to Approvals tab
2. Select the pending device
3. Press `A` to approve (or `T` then `A` to always trust)

If running headless:
```bash
./target/release/remoshell-daemon devices list
./target/release/remoshell-daemon devices trust <device-id>
```

### Step 4: Use Terminal

1. In the app, tap the connected device
2. Tap **"New Terminal"**
3. You now have shell access from your phone!

### Step 5: File Browser

1. Tap **"Files"** tab
2. Browse remote filesystem
3. Tap to download, long-press for options

---

## Local Development Setup

For testing on the same network without deploying to Cloudflare:

### Terminal 1: Local Signaling Server

```bash
cd signaling
npm ci
npm run dev
# Running at http://localhost:8787
```

### Terminal 2: Daemon

```bash
./target/release/remoshell-daemon start
```

### Terminal 3: Web Client (instead of Android)

```bash
cd client
npm ci
npm run dev
# Open http://localhost:5173
```

### Generate pairing with local signaling:

```bash
./target/release/remoshell-daemon pair --relay-url ws://localhost:8787
```

---

## Troubleshooting

### Cloudflare Deployment

**"Error: No account id found"**
```bash
wrangler login  # Re-authenticate
```

**"Durable Objects are not available"**
- Durable Objects require a paid plan OR subdomain setup
- Go to Workers & Pages → your worker → Settings → Subdomain
- Enable workers.dev subdomain

### Android Build

**"SDK location not found"**
```bash
export ANDROID_HOME="$HOME/Android/Sdk"
# Or on macOS: export ANDROID_HOME="$HOME/Library/Android/sdk"
```

**"NDK not found"**
- Open Android Studio → Settings → Android SDK → SDK Tools
- Check "NDK (Side by side)" and apply

**"Build failed: CMake not found"**
- Same location, check "CMake" under SDK Tools

**Gradle errors**
```bash
cd client/src-tauri/gen/android
./gradlew clean
cd ../../..
npm run tauri android build
```

### Connection Issues

**"Signaling connection failed"**
1. Check signaling server is deployed: `curl https://your-worker.workers.dev/health`
2. Check daemon is using correct `--relay-url`
3. Check firewall allows outbound WebSocket connections

**"Peer connection timeout"**
- Both devices need internet access
- Corporate firewalls may block WebRTC - try on mobile data

**"Device not approved"**
- Check daemon's Approvals tab
- If headless: `./target/release/remoshell-daemon devices list`

### App Crashes on Android

Check logs:
```bash
adb logcat | grep -E "(RemoShell|tauri|rust)"
```

---

## Architecture Summary

```
┌─────────────────┐                    ┌─────────────────┐
│  Android App    │                    │    Daemon       │
│  (Your Phone)   │                    │  (Linux PC)     │
│                 │                    │                 │
│  ┌───────────┐  │    Encrypted       │  ┌───────────┐  │
│  │ Terminal  │  │◄──────────────────►│  │ PTY       │  │
│  │ (WebView) │  │     WebRTC         │  │ Sessions  │  │
│  └───────────┘  │                    │  └───────────┘  │
│                 │                    │                 │
│  ┌───────────┐  │                    │  ┌───────────┐  │
│  │ Files     │  │◄──────────────────►│  │ File      │  │
│  │ Browser   │  │    Data Channel    │  │ Manager   │  │
│  └───────────┘  │                    │  └───────────┘  │
└────────┬────────┘                    └────────┬────────┘
         │                                      │
         │    ┌────────────────────────┐        │
         └───►│   Cloudflare Worker    │◄───────┘
              │   (Signaling Only)     │
              │   - No data access     │
              │   - Just coordinates   │
              │     WebRTC handshake   │
              └────────────────────────┘
```

**Security**: All terminal/file data is end-to-end encrypted with Noise Protocol (ChaCha20-Poly1305). The signaling server only sees encrypted WebRTC offer/answer messages.

---

## Quick Reference

| Task | Command |
|------|---------|
| Deploy signaling | `cd signaling && npm run deploy` |
| Start daemon | `./target/release/remoshell-daemon start` |
| Generate QR | `./target/release/remoshell-daemon pair` |
| Build Android | `cd client && npm run tauri android build` |
| Install APK | `adb install <path-to-apk>` |
| Check daemon status | `systemctl --user status remoshell` |
| View daemon logs | `journalctl --user -u remoshell -f` |
