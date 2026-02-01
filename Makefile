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
