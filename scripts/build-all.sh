#!/bin/bash
set -e

# RemoteShell Build Script
# Builds all artifacts for distribution

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
ARTIFACTS_DIR="$ROOT_DIR/artifacts"

echo "=== RemoteShell Build Script ==="
echo "Root: $ROOT_DIR"
echo ""

# Create artifacts directories
mkdir -p "$ARTIFACTS_DIR"/{web,daemon,desktop,mobile}

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

print_status() {
    echo -e "${GREEN}[✓]${NC} $1"
}

print_error() {
    echo -e "${RED}[✗]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${NC} $1"
}

# =============================================================================
# 1. Web Build (GitHub Pages)
# =============================================================================
echo "=== Building Web Client ==="
cd "$ROOT_DIR/client"

if npm run build; then
    cp -r dist/* "$ARTIFACTS_DIR/web/"
    print_status "Web build complete: artifacts/web/"
else
    print_error "Web build failed"
fi

# =============================================================================
# 2. Daemon Build (Linux x86_64)
# =============================================================================
echo ""
echo "=== Building Daemon (Linux x86_64) ==="
cd "$ROOT_DIR"

if cargo build --release -p daemon; then
    cp target/release/remoshell-daemon "$ARTIFACTS_DIR/daemon/remoshell-daemon-linux-x86_64"
    chmod +x "$ARTIFACTS_DIR/daemon/remoshell-daemon-linux-x86_64"
    print_status "Linux daemon built: artifacts/daemon/remoshell-daemon-linux-x86_64"
else
    print_error "Daemon build failed"
fi

# =============================================================================
# 3. Tauri Desktop App (Linux)
# =============================================================================
echo ""
echo "=== Building Desktop App (Linux) ==="
cd "$ROOT_DIR/client"

# Check for required dependencies
if ! pkg-config --exists glib-2.0 webkit2gtk-4.1 2>/dev/null; then
    print_warning "Missing GTK/WebKit dependencies for Tauri desktop build"
    print_warning "Install: sudo apt install libgtk-3-dev libwebkit2gtk-4.1-dev librsvg2-dev"
else
    if npm run tauri build; then
        # Copy AppImage and deb
        find "$ROOT_DIR/client/src-tauri/target/release/bundle" -name "*.AppImage" -exec cp {} "$ARTIFACTS_DIR/desktop/" \; 2>/dev/null
        find "$ROOT_DIR/client/src-tauri/target/release/bundle" -name "*.deb" -exec cp {} "$ARTIFACTS_DIR/desktop/" \; 2>/dev/null
        print_status "Desktop app built: artifacts/desktop/"
    else
        print_error "Desktop build failed"
    fi
fi

# =============================================================================
# 4. Android Build
# =============================================================================
echo ""
echo "=== Building Android App ==="
cd "$ROOT_DIR/client"

if [ -z "$ANDROID_HOME" ]; then
    print_warning "ANDROID_HOME not set - skipping Android build"
    print_warning "Set up Android SDK and run: npm run tauri android init && npm run tauri android build"
else
    # Initialize if needed
    if [ ! -d "src-tauri/gen/android" ]; then
        npm run tauri android init
    fi

    if npm run tauri android build -- --target aarch64; then
        find "$ROOT_DIR/client/src-tauri/gen/android" -name "*.apk" -exec cp {} "$ARTIFACTS_DIR/mobile/" \; 2>/dev/null
        print_status "Android APK built: artifacts/mobile/"
    else
        print_error "Android build failed"
    fi
fi

# =============================================================================
# 5. Cross-compilation (requires cross or cargo-zigbuild)
# =============================================================================
echo ""
echo "=== Cross-compilation Status ==="

# Check for cross-compilation tools
if command -v cross &> /dev/null; then
    echo "Found 'cross' - can build for other platforms"

    # macOS (requires osxcross)
    # cross build --release -p daemon --target x86_64-apple-darwin
    # cross build --release -p daemon --target aarch64-apple-darwin

    # Windows
    # cross build --release -p daemon --target x86_64-pc-windows-gnu

    print_warning "Cross-compilation available but not automated"
    print_warning "Run manually: cross build --release -p daemon --target <target>"
elif command -v cargo-zigbuild &> /dev/null; then
    echo "Found 'cargo-zigbuild' - can build for other platforms"
    print_warning "Run manually: cargo zigbuild --release -p daemon --target <target>"
else
    print_warning "No cross-compilation tools found"
    print_warning "Install: cargo install cross"
    print_warning "Or: cargo install cargo-zigbuild"
fi

# =============================================================================
# Summary
# =============================================================================
echo ""
echo "=== Build Summary ==="
echo ""

if [ -d "$ARTIFACTS_DIR/web" ] && [ "$(ls -A $ARTIFACTS_DIR/web 2>/dev/null)" ]; then
    WEB_SIZE=$(du -sh "$ARTIFACTS_DIR/web" | cut -f1)
    print_status "Web (GitHub Pages):     $WEB_SIZE"
fi

if [ -f "$ARTIFACTS_DIR/daemon/remoshell-daemon-linux-x86_64" ]; then
    DAEMON_SIZE=$(du -sh "$ARTIFACTS_DIR/daemon/remoshell-daemon-linux-x86_64" | cut -f1)
    print_status "Linux Daemon:           $DAEMON_SIZE"
fi

if [ "$(ls -A $ARTIFACTS_DIR/desktop 2>/dev/null)" ]; then
    DESKTOP_SIZE=$(du -sh "$ARTIFACTS_DIR/desktop" | cut -f1)
    print_status "Desktop Apps:           $DESKTOP_SIZE"
fi

if [ "$(ls -A $ARTIFACTS_DIR/mobile 2>/dev/null)" ]; then
    MOBILE_SIZE=$(du -sh "$ARTIFACTS_DIR/mobile" | cut -f1)
    print_status "Mobile Apps:            $MOBILE_SIZE"
fi

echo ""
echo "Artifacts location: $ARTIFACTS_DIR"
echo ""

# List all artifacts
echo "=== All Artifacts ==="
find "$ARTIFACTS_DIR" -type f -name "*.AppImage" -o -name "*.deb" -o -name "*.apk" -o -name "remoshell-*" 2>/dev/null | sort
