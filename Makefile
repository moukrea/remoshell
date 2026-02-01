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
NC     := \033[0m
BOLD   := \033[1m

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
DAEMON_BIN := $(BUILD_DIR)/$(PROFILE)/remoshell

# Check if command exists
define check_cmd
	@command -v $(1) >/dev/null 2>&1 || { \
		printf "$(RED)Error: $(1) is not installed$(NC)\n"; \
		exit 1; \
	}
endef

.PHONY: help
help: ## Show this help message
	@printf "$(BOLD)RemoteShell Makefile$(NC)\n\n"
	@printf "$(BOLD)Usage:$(NC)\n"
	@printf "  make $(GREEN)<target>$(NC)\n\n"
	@printf "$(BOLD)Targets:$(NC)\n"
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  $(GREEN)%-20s$(NC) %s\n", $$1, $$2}'

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
