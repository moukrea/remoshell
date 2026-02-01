# RemoteShell Makefile
# ====================
# Build, test, and development automation for the RemoteShell project.
#
# Usage:
#   make help          Show available targets
#   make setup         Install all dependencies
#   make build         Build all components (debug)
#   make build-release Build all components (release)
#   make test          Run all tests
#   make dev           Start development servers

SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c
MAKEFLAGS += --warn-undefined-variables
MAKEFLAGS += --no-builtin-rules

.DEFAULT_GOAL := help

# Color codes for output
GREEN  := \033[0;32m
YELLOW := \033[0;33m
RED    := \033[0;31m
BLUE   := \033[0;34m
CYAN   := \033[0;36m
NC     := \033[0m
BOLD   := \033[1m

# Version info (from Cargo.toml if available)
VERSION := $(shell grep -m1 '^version' Cargo.toml 2>/dev/null | cut -d'"' -f2 || echo "dev")
GIT_COMMIT := $(shell git rev-parse --short HEAD 2>/dev/null || echo "unknown")
BUILD_DATE := $(shell date +%Y-%m-%d)

# Configuration
RUST_LOG ?= info
PROFILE ?= debug
RELEASE_FLAG := $(if $(filter release,$(PROFILE)),--release,)

# Ports
SIGNALING_PORT ?= 8787
DAEMON_PORT ?= 9000
DEV_SERVER_PORT ?= 5173

# Paths
ROOT_DIR := $(shell pwd)
BUILD_DIR := $(ROOT_DIR)/target
DIST_DIR := $(ROOT_DIR)/client/dist
DAEMON_BIN := $(BUILD_DIR)/$(PROFILE)/remoshell-daemon
DESKTOP_BIN := $(BUILD_DIR)/$(PROFILE)/remoshell-desktop
WEB_DIST := $(ROOT_DIR)/client/dist

# Check if command exists
define check_cmd
	@command -v $(1) >/dev/null 2>&1 || { \
		printf "$(RED)Error: $(1) is not installed$(NC)\n"; \
		exit 1; \
	}
endef

.PHONY: help info
help: ## Show this help message
	@printf "\n"
	@printf "$(CYAN)$(BOLD)RemoteShell$(NC) v$(VERSION) ($(GIT_COMMIT))\n"
	@printf "$(CYAN)==============================$(NC)\n\n"
	@printf "$(BOLD)Usage:$(NC)  make $(GREEN)<target>$(NC) [VARIABLE=value]\n\n"
	@printf "$(BOLD)$(BLUE)Setup:$(NC)\n"
	@printf "  $(GREEN)setup$(NC)               Install all development dependencies\n"
	@printf "  $(GREEN)setup-rust$(NC)          Install Rust toolchain and components\n"
	@printf "  $(GREEN)setup-node$(NC)          Install Node.js dependencies\n"
	@printf "  $(GREEN)setup-system$(NC)        Install system dependencies (Linux)\n"
	@printf "  $(GREEN)setup-android$(NC)       Setup Android SDK for mobile builds\n"
	@printf "  $(GREEN)deps$(NC)                Install all project dependencies\n"
	@printf "  $(GREEN)check-deps$(NC)          Verify all dependencies are installed\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Build:$(NC)\n"
	@printf "  $(GREEN)build$(NC)               Build daemon and web client (debug)\n"
	@printf "  $(GREEN)build-release$(NC)       Build all components in release mode\n"
	@printf "  $(GREEN)build-all$(NC)           Build all components\n"
	@printf "  $(GREEN)build-daemon$(NC)        Build the daemon binary\n"
	@printf "  $(GREEN)build-web$(NC)           Build the web frontend\n"
	@printf "  $(GREEN)build-desktop$(NC)       Build the Tauri desktop app\n"
	@printf "  $(GREEN)build-android$(NC)       Build the Android app\n"
	@printf "  $(GREEN)build-signaling$(NC)     Build the signaling worker\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Run/Development:$(NC)\n"
	@printf "  $(GREEN)dev-daemon$(NC)          Run daemon with TUI interface\n"
	@printf "  $(GREEN)dev-daemon-headless$(NC) Run daemon without TUI (headless mode)\n"
	@printf "  $(GREEN)dev-client$(NC)          Run web client dev server (Vite)\n"
	@printf "  $(GREEN)dev-desktop$(NC)         Run Tauri desktop app in dev mode\n"
	@printf "  $(GREEN)dev-signaling$(NC)       Run signaling worker locally (wrangler)\n"
	@printf "  $(GREEN)dev-stack$(NC)           Run all services in parallel\n"
	@printf "  $(GREEN)dev-tmux$(NC)            Run services in tmux session\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Test:$(NC)\n"
	@printf "  $(GREEN)test$(NC)                Run all tests (Rust + Frontend + Signaling)\n"
	@printf "  $(GREEN)test-rust$(NC)           Run all Rust workspace tests\n"
	@printf "  $(GREEN)test-protocol$(NC)       Run protocol crate tests\n"
	@printf "  $(GREEN)test-daemon$(NC)         Run daemon crate tests\n"
	@printf "  $(GREEN)test-client$(NC)         Run frontend tests\n"
	@printf "  $(GREEN)test-signaling$(NC)      Run signaling worker tests\n"
	@printf "  $(GREEN)bench$(NC)               Run Rust benchmarks\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Lint:$(NC)\n"
	@printf "  $(GREEN)lint$(NC)                Run all linters (Rust + Client + Signaling)\n"
	@printf "  $(GREEN)lint-rust$(NC)           Run Rust linter (clippy)\n"
	@printf "  $(GREEN)lint-client$(NC)         Run client linter (ESLint)\n"
	@printf "  $(GREEN)lint-signaling$(NC)      Run signaling worker linter\n"
	@printf "  $(GREEN)format$(NC)              Format all code (Rust + Client + Signaling)\n"
	@printf "  $(GREEN)fmt-check$(NC)           Check formatting without changes\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Clean:$(NC)\n"
	@printf "  $(GREEN)clean$(NC)               Clean Rust build and artifacts\n"
	@printf "  $(GREEN)clean-all$(NC)           Full clean including node_modules\n"
	@printf "  $(GREEN)clean-rust$(NC)          Clean Rust build outputs (cargo clean)\n"
	@printf "  $(GREEN)clean-node$(NC)          Clean Node.js artifacts\n"
	@printf "  $(GREEN)clean-android$(NC)       Clean Android build artifacts\n"
	@printf "  $(GREEN)clean-tauri$(NC)         Clean Tauri generated outputs\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Release:$(NC)\n"
	@printf "  $(GREEN)release$(NC)             Build release daemon (main release target)\n"
	@printf "  $(GREEN)release-all$(NC)         Build all release artifacts\n"
	@printf "  $(GREEN)release-daemon$(NC)      Build optimized daemon binary\n"
	@printf "  $(GREEN)release-web$(NC)         Build web assets for production\n"
	@printf "  $(GREEN)release-desktop$(NC)     Build desktop apps for all platforms\n"
	@printf "  $(GREEN)release-cross$(NC)       Build daemon for all platforms (cross)\n"
	@printf "  $(GREEN)package-linux$(NC)       Create Linux distribution package\n"
	@printf "  $(GREEN)package-macos$(NC)       Create macOS distribution packages\n"
	@printf "  $(GREEN)package-windows$(NC)     Create Windows distribution package\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Docker:$(NC)\n"
	@printf "  $(GREEN)docker$(NC)              Show docker commands\n"
	@printf "  $(GREEN)docker-build$(NC)        Build all Docker images\n"
	@printf "  $(GREEN)docker-up$(NC)           Start all services in detached mode\n"
	@printf "  $(GREEN)docker-down$(NC)         Stop all services\n"
	@printf "  $(GREEN)docker-logs$(NC)         Tail logs from all services\n"
	@printf "  $(GREEN)docker-ps$(NC)           Show running containers\n"
	@printf "  $(GREEN)docker-clean$(NC)        Remove containers, volumes, and images\n"
	@printf "  $(GREEN)docker-prod$(NC)         Start production deployment\n"
	@printf "  $(GREEN)docker-health$(NC)       Check health of all services\n"
	@printf "\n"
	@printf "$(BOLD)$(YELLOW)Variables:$(NC)\n"
	@printf "  $(GREEN)PROFILE$(NC)=debug|release  Build profile (default: debug)\n"
	@printf "  $(GREEN)RUST_LOG$(NC)=<level>       Logging level (default: info)\n"
	@printf "  $(GREEN)SIGNALING_PORT$(NC)=<port>  Signaling server port (default: 8787)\n"
	@printf "  $(GREEN)DAEMON_PORT$(NC)=<port>     Daemon server port (default: 9000)\n"
	@printf "\n"
	@printf "$(BOLD)$(YELLOW)Examples:$(NC)\n"
	@printf "  make build                    # Debug build\n"
	@printf "  make build PROFILE=release    # Release build\n"
	@printf "  make test-rust RUST_LOG=debug # Verbose test output\n"
	@printf "  make docker-up                # Start Docker services\n"
	@printf "  make dev-stack                # Start all dev servers\n"
	@printf "\n"

## info: Show project configuration and environment
info:
	@printf "\n"
	@printf "$(CYAN)$(BOLD)RemoteShell Project Info$(NC)\n"
	@printf "$(CYAN)========================$(NC)\n\n"
	@printf "$(BOLD)Version:$(NC)      $(VERSION)\n"
	@printf "$(BOLD)Git Commit:$(NC)   $(GIT_COMMIT)\n"
	@printf "$(BOLD)Build Date:$(NC)   $(BUILD_DATE)\n"
	@printf "$(BOLD)Profile:$(NC)      $(PROFILE)\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Paths:$(NC)\n"
	@printf "  Root:       $(ROOT_DIR)\n"
	@printf "  Build:      $(BUILD_DIR)\n"
	@printf "  Artifacts:  $(ARTIFACTS_DIR)\n"
	@printf "  Daemon:     $(DAEMON_BIN)\n"
	@printf "  Web Dist:   $(WEB_DIST)\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Ports:$(NC)\n"
	@printf "  Signaling:  $(SIGNALING_PORT)\n"
	@printf "  Daemon:     $(DAEMON_PORT)\n"
	@printf "  Dev Server: $(DEV_SERVER_PORT)\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Environment:$(NC)\n"
	@printf "  RUST_LOG:   $(RUST_LOG)\n"
	@printf "  Shell:      $(SHELL)\n"
	@printf "\n"
	@printf "$(BOLD)$(BLUE)Tools:$(NC)\n"
	@printf "  Rust:       $$(rustc --version 2>/dev/null || echo 'not installed')\n"
	@printf "  Cargo:      $$(cargo --version 2>/dev/null || echo 'not installed')\n"
	@printf "  Node:       $$(node --version 2>/dev/null || echo 'not installed')\n"
	@printf "  npm:        $$(npm --version 2>/dev/null || echo 'not installed')\n"
	@printf "  Docker:     $$(docker --version 2>/dev/null || echo 'not installed')\n"
	@printf "\n"

# =============================================================================
# Setup Targets
# =============================================================================

.PHONY: setup setup-rust setup-node setup-system setup-android

setup: ## Install all development dependencies
	@printf "$(BOLD)Setting up development environment...$(NC)\n"
	$(MAKE) setup-rust
	$(MAKE) setup-node
	$(MAKE) deps
	@printf "$(GREEN)Setup complete!$(NC)\n"

setup-rust: ## Install Rust toolchain and components
	@printf "$(YELLOW)Setting up Rust...$(NC)\n"
	@if ! command -v rustup >/dev/null 2>&1; then \
		curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y; \
	fi
	rustup update stable
	rustup component add clippy rustfmt
	rustup target add wasm32-unknown-unknown
	@printf "$(GREEN)Rust setup complete$(NC)\n"

setup-node: ## Install Node.js dependencies
	@printf "$(YELLOW)Setting up Node.js...$(NC)\n"
	@if ! command -v node >/dev/null 2>&1; then \
		printf "$(RED)Node.js not found. Install via nvm or package manager$(NC)\n"; \
		exit 1; \
	fi
	@printf "Node.js version: $$(node --version)\n"
	@printf "npm version: $$(npm --version)\n"
	@printf "$(GREEN)Node.js setup complete$(NC)\n"

setup-system: ## Install system dependencies (Linux)
	@printf "$(YELLOW)Installing system dependencies...$(NC)\n"
	@if command -v apt-get >/dev/null 2>&1; then \
		sudo apt-get update && sudo apt-get install -y \
			build-essential pkg-config libssl-dev \
			libgtk-3-dev libwebkit2gtk-4.0-dev libappindicator3-dev; \
	elif command -v pacman >/dev/null 2>&1; then \
		sudo pacman -S --needed base-devel openssl gtk3 webkit2gtk libappindicator-gtk3; \
	fi
	@printf "$(GREEN)System dependencies installed$(NC)\n"

setup-android: ## Setup Android SDK for mobile builds
	@printf "$(YELLOW)Setting up Android SDK...$(NC)\n"
	@if [ -z "$${ANDROID_HOME:-}" ]; then \
		printf "$(RED)ANDROID_HOME not set. Install Android Studio or set manually$(NC)\n"; \
		exit 1; \
	fi
	@printf "ANDROID_HOME: $$ANDROID_HOME\n"
	rustup target add aarch64-linux-android armv7-linux-androideabi
	@printf "$(GREEN)Android setup complete$(NC)\n"

# =============================================================================
# Dependency Installation
# =============================================================================

.PHONY: deps deps-rust deps-node deps-signaling

deps: deps-rust deps-node deps-signaling ## Install all project dependencies
	@printf "$(GREEN)All dependencies installed$(NC)\n"

deps-rust: ## Fetch Rust dependencies
	@printf "$(YELLOW)Fetching Rust dependencies...$(NC)\n"
	cargo fetch
	@printf "$(GREEN)Rust dependencies fetched$(NC)\n"

deps-node: ## Install Node.js dependencies
	@printf "$(YELLOW)Installing Node.js dependencies...$(NC)\n"
	cd $(ROOT_DIR)/client && npm ci
	@printf "$(GREEN)Node.js dependencies installed$(NC)\n"

deps-signaling: ## Setup signaling worker dependencies
	@printf "$(YELLOW)Setting up signaling worker...$(NC)\n"
	cd $(ROOT_DIR)/signaling-worker && npm ci
	@printf "$(GREEN)Signaling worker dependencies installed$(NC)\n"

# =============================================================================
# Dependency Verification
# =============================================================================

.PHONY: check-deps check-rust check-node

check-deps: check-rust check-node ## Verify all dependencies are installed
	@printf "$(GREEN)All dependencies verified$(NC)\n"

check-rust: ## Verify Rust toolchain
	@printf "$(YELLOW)Checking Rust installation...$(NC)\n"
	$(call check_cmd,rustc)
	$(call check_cmd,cargo)
	$(call check_cmd,rustfmt)
	$(call check_cmd,clippy-driver)
	@printf "  rustc: $$(rustc --version)\n"
	@printf "  cargo: $$(cargo --version)\n"
	@printf "$(GREEN)Rust OK$(NC)\n"

check-node: ## Verify Node.js installation
	@printf "$(YELLOW)Checking Node.js installation...$(NC)\n"
	$(call check_cmd,node)
	$(call check_cmd,npm)
	@printf "  node: $$(node --version)\n"
	@printf "  npm: $$(npm --version)\n"
	@printf "$(GREEN)Node.js OK$(NC)\n"

# =============================================================================
# Build Targets
# =============================================================================

.PHONY: build build-daemon build-protocol build-tauri-client build-signaling
.PHONY: build-web build-desktop build-android build-all build-release

build: build-daemon build-web ## Build daemon and web client (debug)
	@printf "$(GREEN)Build complete$(NC)\n"

build-release: ## Build all components in release mode
	$(MAKE) build-all PROFILE=release

build-all: build-daemon build-web build-desktop ## Build all components
	@printf "$(GREEN)All components built$(NC)\n"

build-daemon: ## Build the daemon binary
	@printf "$(YELLOW)Building daemon ($(PROFILE))...$(NC)\n"
	cargo build -p daemon $(RELEASE_FLAG)
	@printf "$(GREEN)Daemon built: $(DAEMON_BIN)$(NC)\n"

build-protocol: ## Build the protocol crate
	@printf "$(YELLOW)Building protocol crate...$(NC)\n"
	cargo build -p protocol $(RELEASE_FLAG)
	@printf "$(GREEN)Protocol crate built$(NC)\n"

build-tauri-client: ## Build the Tauri client library
	@printf "$(YELLOW)Building tauri-client crate...$(NC)\n"
	cargo build -p tauri-client $(RELEASE_FLAG)
	@printf "$(GREEN)Tauri client crate built$(NC)\n"

build-signaling: ## Build the signaling worker
	@printf "$(YELLOW)Building signaling worker...$(NC)\n"
	cd $(ROOT_DIR)/signaling-worker && npm run build
	@printf "$(GREEN)Signaling worker built$(NC)\n"

build-web: ## Build the web frontend
	@printf "$(YELLOW)Building web frontend...$(NC)\n"
	cd $(ROOT_DIR)/client && npm run build
	@printf "$(GREEN)Web frontend built: $(DIST_DIR)$(NC)\n"

build-desktop: build-tauri-client ## Build the Tauri desktop app
	@printf "$(YELLOW)Building desktop app ($(PROFILE))...$(NC)\n"
	cd $(ROOT_DIR)/client && npm run tauri build $(if $(filter release,$(PROFILE)),,-- --debug)
	@printf "$(GREEN)Desktop app built$(NC)\n"

build-android: build-tauri-client ## Build the Android app
	@printf "$(YELLOW)Building Android app...$(NC)\n"
	cd $(ROOT_DIR)/client && npm run tauri android build
	@printf "$(GREEN)Android app built$(NC)\n"

# =============================================================================
# Development / Run Targets
# =============================================================================

.PHONY: dev-daemon dev-daemon-headless dev-client dev-signaling dev-desktop
.PHONY: dev-stack dev-tmux pair-local

dev-daemon: ## Run daemon with TUI interface
	cargo run -p daemon

dev-daemon-headless: ## Run daemon without TUI (headless mode)
	cargo run -p daemon -- --headless

dev-client: ## Run web client dev server (Vite)
	cd $(ROOT_DIR)/client && npm run dev

dev-signaling: ## Run signaling worker locally (wrangler)
	cd $(ROOT_DIR)/signaling && npx wrangler dev

dev-desktop: ## Run Tauri desktop app in dev mode
	cd $(ROOT_DIR)/client && npm run tauri dev

dev-stack: ## Run all services in parallel (background processes)
	@printf "$(YELLOW)Starting development stack...$(NC)\n"
	@$(MAKE) dev-signaling & \
	$(MAKE) dev-client & \
	$(MAKE) dev-daemon
	@printf "$(GREEN)Stack running. Press Ctrl+C to stop.$(NC)\n"

dev-tmux: ## Run services in tmux session
	@tmux new-session -d -s remoshell 'make dev-signaling' \; \
		split-window -h 'make dev-client' \; \
		split-window -v 'make dev-daemon' \; \
		select-pane -t 0 \; \
		attach

pair-local: ## Generate local pairing QR code
	cargo run -p daemon -- pair --local

# =============================================================================
# Test Targets
# =============================================================================

.PHONY: test test-all test-rust test-protocol test-daemon test-tauri-client
.PHONY: test-client test-signaling test-client-watch test-signaling-watch
.PHONY: ci-local bench

test: test-rust test-client test-signaling ## Run all tests (Rust + Frontend + Signaling)
	@printf "$(GREEN)All tests complete$(NC)\n"

test-all: test ## Alias for test

test-rust: ## Run all Rust workspace tests
	@printf "$(YELLOW)Running Rust tests...$(NC)\n"
	cargo test --workspace
	@printf "$(GREEN)Rust tests passed$(NC)\n"

test-protocol: ## Run protocol crate tests
	@printf "$(YELLOW)Running protocol tests...$(NC)\n"
	cargo test -p protocol
	@printf "$(GREEN)Protocol tests passed$(NC)\n"

test-daemon: ## Run daemon crate tests
	@printf "$(YELLOW)Running daemon tests...$(NC)\n"
	cargo test -p daemon
	@printf "$(GREEN)Daemon tests passed$(NC)\n"

test-tauri-client: ## Run tauri-client crate tests
	@printf "$(YELLOW)Running tauri-client tests...$(NC)\n"
	cargo test -p tauri-client
	@printf "$(GREEN)Tauri-client tests passed$(NC)\n"

test-client: ## Run frontend tests
	@printf "$(YELLOW)Running frontend tests...$(NC)\n"
	cd $(ROOT_DIR)/client && npm test
	@printf "$(GREEN)Frontend tests passed$(NC)\n"

test-signaling: ## Run signaling worker tests
	@printf "$(YELLOW)Running signaling tests...$(NC)\n"
	cd $(ROOT_DIR)/signaling && npm test
	@printf "$(GREEN)Signaling tests passed$(NC)\n"

test-client-watch: ## Run frontend tests in watch mode
	cd $(ROOT_DIR)/client && npm test -- --watch

test-signaling-watch: ## Run signaling tests in watch mode
	cd $(ROOT_DIR)/signaling && npm test -- --watch

ci-local: ## Run full CI pipeline locally (format, lint, test)
	@printf "$(BOLD)=== Checking Rust formatting ===$(NC)\n"
	cargo fmt --all -- --check
	@printf "$(BOLD)=== Running Clippy ===$(NC)\n"
	cargo clippy --workspace -- -D warnings
	@printf "$(BOLD)=== Running Rust tests ===$(NC)\n"
	cargo test --workspace
	@printf "$(BOLD)=== Checking TypeScript ===$(NC)\n"
	cd $(ROOT_DIR)/client && npm run typecheck
	@printf "$(BOLD)=== Running frontend tests ===$(NC)\n"
	cd $(ROOT_DIR)/client && npm test
	@printf "$(GREEN)=== CI checks passed ===$(NC)\n"

bench: ## Run Rust benchmarks
	@printf "$(YELLOW)Running benchmarks...$(NC)\n"
	cargo bench --workspace
	@printf "$(GREEN)Benchmarks complete$(NC)\n"

# =============================================================================
# Lint Targets
# =============================================================================

.PHONY: lint lint-rust lint-client lint-signaling

lint: lint-rust lint-client lint-signaling ## Run all linters (Rust + Client + Signaling)
	@printf "$(GREEN)All linting complete$(NC)\n"

lint-rust: ## Run Rust linter (clippy)
	@printf "$(YELLOW)Running Clippy...$(NC)\n"
	cargo clippy --workspace -- -D warnings
	@printf "$(GREEN)Rust lint passed$(NC)\n"

lint-client: ## Run client linter (ESLint)
	@printf "$(YELLOW)Running client linter...$(NC)\n"
	cd $(ROOT_DIR)/client && npm run lint
	@printf "$(GREEN)Client lint passed$(NC)\n"

lint-signaling: ## Run signaling worker linter
	@printf "$(YELLOW)Running signaling linter...$(NC)\n"
	cd $(ROOT_DIR)/signaling && npm run lint
	@printf "$(GREEN)Signaling lint passed$(NC)\n"

# =============================================================================
# Format Targets
# =============================================================================

.PHONY: format fmt-rust fmt-check

format: fmt-rust ## Format all code (Rust + Client + Signaling)
	@printf "$(YELLOW)Formatting client code...$(NC)\n"
	cd $(ROOT_DIR)/client && npm run format
	@printf "$(YELLOW)Formatting signaling code...$(NC)\n"
	cd $(ROOT_DIR)/signaling && npm run format
	@printf "$(GREEN)All formatting complete$(NC)\n"

fmt-rust: ## Format Rust code
	@printf "$(YELLOW)Formatting Rust code...$(NC)\n"
	cargo fmt --all
	@printf "$(GREEN)Rust formatting complete$(NC)\n"

fmt-check: ## Check formatting without changes
	@printf "$(YELLOW)Checking Rust formatting...$(NC)\n"
	cargo fmt --all -- --check
	@printf "$(YELLOW)Checking client formatting...$(NC)\n"
	cd $(ROOT_DIR)/client && npm run format:check
	@printf "$(YELLOW)Checking signaling formatting...$(NC)\n"
	cd $(ROOT_DIR)/signaling && npm run format:check
	@printf "$(GREEN)Format check passed$(NC)\n"

# =============================================================================
# Clean Targets
# =============================================================================

.PHONY: clean clean-all clean-rust clean-node clean-artifacts clean-android clean-tauri

clean: clean-rust clean-artifacts ## Clean Rust build and artifacts
	@printf "$(GREEN)Basic clean complete$(NC)\n"

clean-all: clean-rust clean-node clean-artifacts clean-android clean-tauri ## Full clean including node_modules
	@printf "$(GREEN)Full clean complete$(NC)\n"

clean-rust: ## Clean Rust build outputs (cargo clean)
	@printf "$(YELLOW)Cleaning Rust build...$(NC)\n"
	cargo clean
	@printf "$(GREEN)Rust clean complete$(NC)\n"

clean-node: ## Clean Node.js artifacts (node_modules, dist, .vite)
	@printf "$(YELLOW)Cleaning Node.js artifacts...$(NC)\n"
	rm -rf $(ROOT_DIR)/client/node_modules
	rm -rf $(ROOT_DIR)/client/dist
	rm -rf $(ROOT_DIR)/client/.vite
	rm -rf $(ROOT_DIR)/signaling/node_modules
	rm -rf $(ROOT_DIR)/signaling/dist
	rm -rf $(ROOT_DIR)/signaling/.vite
	@printf "$(GREEN)Node.js clean complete$(NC)\n"

clean-artifacts: ## Clean artifacts directory
	@printf "$(YELLOW)Cleaning artifacts...$(NC)\n"
	rm -rf $(ROOT_DIR)/artifacts
	@printf "$(GREEN)Artifacts clean complete$(NC)\n"

clean-android: ## Clean Android build artifacts (gradle, build dirs)
	@printf "$(YELLOW)Cleaning Android artifacts...$(NC)\n"
	rm -rf $(ROOT_DIR)/client/src-tauri/gen/android/.gradle
	rm -rf $(ROOT_DIR)/client/src-tauri/gen/android/build
	rm -rf $(ROOT_DIR)/client/src-tauri/gen/android/app/build
	@printf "$(GREEN)Android clean complete$(NC)\n"

clean-tauri: ## Clean Tauri generated outputs
	@printf "$(YELLOW)Cleaning Tauri gen outputs...$(NC)\n"
	rm -rf $(ROOT_DIR)/client/src-tauri/gen
	rm -rf $(ROOT_DIR)/client/src-tauri/target
	@printf "$(GREEN)Tauri clean complete$(NC)\n"

# =============================================================================
# Release Targets
# =============================================================================

.PHONY: release release-all release-daemon release-web release-desktop release-android
.PHONY: release-cross release-daemon-macos-x86 release-daemon-macos-arm release-daemon-windows
.PHONY: package-linux package-macos package-windows

## Artifacts directory for release builds
ARTIFACTS_DIR := $(ROOT_DIR)/artifacts

release: release-daemon ## Build release daemon (main release target)
	@printf "$(GREEN)Release build complete$(NC)\n"

release-all: release-daemon release-web release-desktop release-android ## Build all release artifacts
	@printf "$(GREEN)All release builds complete$(NC)\n"

release-daemon: ## Build optimized daemon binary and package
	@printf "$(YELLOW)Building release daemon...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/daemon
	cargo build --release -p daemon
	cp $(BUILD_DIR)/release/remoshell-daemon $(ARTIFACTS_DIR)/daemon/remoshell-linux-x86_64
	@printf "$(GREEN)Daemon release built: $(ARTIFACTS_DIR)/daemon/remoshell-linux-x86_64$(NC)\n"

release-web: ## Build web assets for production
	@printf "$(YELLOW)Building release web assets...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/web
	cd $(ROOT_DIR)/client && npm run build
	cp -r $(ROOT_DIR)/client/dist/* $(ARTIFACTS_DIR)/web/
	@printf "$(GREEN)Web release built: $(ARTIFACTS_DIR)/web/$(NC)\n"

release-desktop: ## Build desktop apps for all platforms
	@printf "$(YELLOW)Building release desktop app...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/desktop
	cd $(ROOT_DIR)/client && npm run tauri build
	@printf "$(GREEN)Desktop release built (check client/src-tauri/target/release/bundle/)$(NC)\n"

release-android: ## Build signed APK for Android
	@printf "$(YELLOW)Building release Android APK...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/android
	cd $(ROOT_DIR)/client && npm run tauri android build -- --apk
	find $(ROOT_DIR)/client/src-tauri/gen/android -name "*.apk" -exec cp {} $(ARTIFACTS_DIR)/android/ \;
	@printf "$(GREEN)Android release built: $(ARTIFACTS_DIR)/android/$(NC)\n"

# Cross-compilation targets (requires cross-rs or appropriate toolchains)
release-cross: release-daemon-macos-x86 release-daemon-macos-arm release-daemon-windows ## Build daemon for all platforms
	@printf "$(GREEN)Cross-compilation complete$(NC)\n"

release-daemon-macos-x86: ## Build daemon for macOS x86_64
	@printf "$(YELLOW)Building daemon for macOS x86_64...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/daemon
	cross build --release -p daemon --target x86_64-apple-darwin
	cp $(BUILD_DIR)/x86_64-apple-darwin/release/remoshell-daemon $(ARTIFACTS_DIR)/daemon/remoshell-macos-x86_64
	@printf "$(GREEN)macOS x86_64 daemon built$(NC)\n"

release-daemon-macos-arm: ## Build daemon for macOS ARM64
	@printf "$(YELLOW)Building daemon for macOS ARM64...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/daemon
	cross build --release -p daemon --target aarch64-apple-darwin
	cp $(BUILD_DIR)/aarch64-apple-darwin/release/remoshell-daemon $(ARTIFACTS_DIR)/daemon/remoshell-macos-aarch64
	@printf "$(GREEN)macOS ARM64 daemon built$(NC)\n"

release-daemon-windows: ## Build daemon for Windows x86_64
	@printf "$(YELLOW)Building daemon for Windows x86_64...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/daemon
	cross build --release -p daemon --target x86_64-pc-windows-gnu
	cp $(BUILD_DIR)/x86_64-pc-windows-gnu/release/remoshell-daemon.exe $(ARTIFACTS_DIR)/daemon/remoshell-windows-x86_64.exe
	@printf "$(GREEN)Windows x86_64 daemon built$(NC)\n"

# =============================================================================
# Package Targets (Platform-specific packaging)
# =============================================================================

package-linux: release-daemon ## Create Linux distribution package
	@printf "$(YELLOW)Creating Linux package...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/packages/linux
	cp $(ARTIFACTS_DIR)/daemon/remoshell-linux-x86_64 $(ARTIFACTS_DIR)/packages/linux/
	cd $(ARTIFACTS_DIR)/packages/linux && tar -czvf remoshell-linux-x86_64.tar.gz remoshell-linux-x86_64
	@printf "$(GREEN)Linux package created: $(ARTIFACTS_DIR)/packages/linux/remoshell-linux-x86_64.tar.gz$(NC)\n"

package-macos: release-daemon-macos-x86 release-daemon-macos-arm ## Create macOS distribution packages
	@printf "$(YELLOW)Creating macOS packages...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/packages/macos
	cp $(ARTIFACTS_DIR)/daemon/remoshell-macos-x86_64 $(ARTIFACTS_DIR)/packages/macos/
	cp $(ARTIFACTS_DIR)/daemon/remoshell-macos-aarch64 $(ARTIFACTS_DIR)/packages/macos/
	cd $(ARTIFACTS_DIR)/packages/macos && tar -czvf remoshell-macos-x86_64.tar.gz remoshell-macos-x86_64
	cd $(ARTIFACTS_DIR)/packages/macos && tar -czvf remoshell-macos-aarch64.tar.gz remoshell-macos-aarch64
	@printf "$(GREEN)macOS packages created: $(ARTIFACTS_DIR)/packages/macos/$(NC)\n"

package-windows: release-daemon-windows ## Create Windows distribution package
	@printf "$(YELLOW)Creating Windows package...$(NC)\n"
	mkdir -p $(ARTIFACTS_DIR)/packages/windows
	cp $(ARTIFACTS_DIR)/daemon/remoshell-windows-x86_64.exe $(ARTIFACTS_DIR)/packages/windows/
	cd $(ARTIFACTS_DIR)/packages/windows && zip remoshell-windows-x86_64.zip remoshell-windows-x86_64.exe
	@printf "$(GREEN)Windows package created: $(ARTIFACTS_DIR)/packages/windows/remoshell-windows-x86_64.zip$(NC)\n"

# =============================================================================
# Docker Targets
# =============================================================================

DOCKER_COMPOSE := docker compose
DOCKER_COMPOSE_PROD := docker compose -f docker-compose.yml -f docker-compose.prod.yml

.PHONY: docker docker-build docker-build-daemon docker-build-signaling docker-up docker-down docker-logs
.PHONY: docker-ps docker-clean docker-push docker-shell-daemon docker-shell-frontend docker-shell-signaling
.PHONY: docker-rebuild-% docker-prod docker-prod-down docker-health

## docker: Show available docker commands
docker:
	@echo "Docker Commands:"
	@echo "  make docker-build          - Build all Docker images"
	@echo "  make docker-build-daemon   - Build daemon image only"
	@echo "  make docker-build-signaling - Build signaling image only"
	@echo "  make docker-up             - Start all services"
	@echo "  make docker-down           - Stop all services"
	@echo "  make docker-logs           - Tail logs from all services"
	@echo "  make docker-ps             - Show running containers"
	@echo "  make docker-clean          - Remove containers, volumes, images"
	@echo "  make docker-push           - Push images to registry"
	@echo "  make docker-shell-daemon   - Shell into daemon container"
	@echo "  make docker-shell-frontend - Shell into frontend container"
	@echo "  make docker-shell-signaling - Shell into signaling container"
	@echo "  make docker-rebuild-<svc>  - Rebuild and restart a service"
	@echo "  make docker-prod           - Start production deployment"
	@echo "  make docker-prod-down      - Stop production deployment"
	@echo "  make docker-health         - Check service health"

## docker-build: Build all Docker images
docker-build:
	$(DOCKER_COMPOSE) build

## docker-build-daemon: Build daemon image only
docker-build-daemon:
	$(DOCKER_COMPOSE) build daemon

## docker-build-signaling: Build signaling image only
docker-build-signaling:
	$(DOCKER_COMPOSE) build signaling

## docker-up: Start all services in detached mode
docker-up:
	$(DOCKER_COMPOSE) up -d

## docker-down: Stop all services
docker-down:
	$(DOCKER_COMPOSE) down

## docker-logs: Tail logs from all services
docker-logs:
	$(DOCKER_COMPOSE) logs -f

## docker-ps: Show running containers
docker-ps:
	$(DOCKER_COMPOSE) ps

## docker-clean: Remove containers, volumes, and images
docker-clean:
	$(DOCKER_COMPOSE) down -v --rmi local

## docker-push: Push images to registry
docker-push:
	$(DOCKER_COMPOSE) push

## docker-shell-daemon: Open shell in daemon container
docker-shell-daemon:
	$(DOCKER_COMPOSE) exec daemon /bin/sh

## docker-shell-frontend: Open shell in frontend container
docker-shell-frontend:
	$(DOCKER_COMPOSE) exec frontend /bin/sh

## docker-shell-signaling: Open shell in signaling container
docker-shell-signaling:
	$(DOCKER_COMPOSE) exec signaling /bin/sh

## docker-rebuild-%: Rebuild and restart a specific service
docker-rebuild-%:
	$(DOCKER_COMPOSE) build $*
	$(DOCKER_COMPOSE) up -d $*

## docker-prod: Start production deployment
docker-prod:
	$(DOCKER_COMPOSE_PROD) up -d

## docker-prod-down: Stop production deployment
docker-prod-down:
	$(DOCKER_COMPOSE_PROD) down

## docker-health: Check health of all services
docker-health:
	@echo "Checking service health..."
	@$(DOCKER_COMPOSE) ps --format "table {{.Name}}\t{{.Status}}\t{{.Health}}"