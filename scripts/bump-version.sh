#!/usr/bin/env bash
# Version Bump Script for RemoShell
# Updates version numbers across all project components
#
# Usage: ./scripts/bump-version.sh <version> [--dry-run]
# Example: ./scripts/bump-version.sh 1.2.3
# Example: ./scripts/bump-version.sh 1.2.3 --dry-run

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# Script configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Parse arguments
VERSION=""
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 <version> [--dry-run]"
            echo ""
            echo "Arguments:"
            echo "  version    Semantic version in X.Y.Z format (e.g., 1.2.3)"
            echo ""
            echo "Options:"
            echo "  --dry-run  Preview changes without modifying files"
            echo "  -h, --help Show this help message"
            echo ""
            echo "Files updated:"
            echo "  - Cargo.toml (workspace root)"
            echo "  - client/package.json"
            echo "  - signaling/package.json"
            echo "  - client/src-tauri/Cargo.toml"
            echo "  - client/src-tauri/tauri.conf.json"
            exit 0
            ;;
        -*)
            echo -e "${RED}Error: Unknown option: $1${NC}" >&2
            echo "Use --help for usage information" >&2
            exit 1
            ;;
        *)
            if [[ -z "$VERSION" ]]; then
                VERSION="$1"
            else
                echo -e "${RED}Error: Unexpected argument: $1${NC}" >&2
                exit 1
            fi
            shift
            ;;
    esac
done

# Check if version is provided
if [[ -z "$VERSION" ]]; then
    echo -e "${RED}Error: Version argument is required${NC}" >&2
    echo "Usage: $0 <version> [--dry-run]" >&2
    exit 1
fi

# Validate semver format (X.Y.Z)
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo -e "${RED}Error: Invalid version format. Expected semver (X.Y.Z), got: $VERSION${NC}" >&2
    exit 1
fi

# Files to update with their update patterns
declare -A FILES
FILES=(
    ["Cargo.toml"]="workspace"
    ["client/package.json"]="json"
    ["signaling/package.json"]="json"
    ["client/src-tauri/Cargo.toml"]="cargo"
    ["client/src-tauri/tauri.conf.json"]="json"
)

echo "========================================="
echo "RemoShell Version Bump Script"
echo "========================================="
echo "New version: $VERSION"
if [[ "$DRY_RUN" == true ]]; then
    echo -e "${YELLOW}Mode: DRY RUN (no files will be modified)${NC}"
else
    echo "Mode: LIVE"
fi
echo ""

# Track success/failure
FAILED_FILES=()
UPDATED_FILES=()

# Function to update Cargo.toml (workspace root)
update_workspace_cargo() {
    local file="$1"
    local filepath="$PROJECT_ROOT/$file"

    if [[ ! -f "$filepath" ]]; then
        echo -e "${RED}  ERROR: File not found: $file${NC}" >&2
        return 1
    fi

    # Match version in [workspace.package] section
    if grep -q '^\[workspace\.package\]' "$filepath"; then
        if [[ "$DRY_RUN" == true ]]; then
            echo "  Would update: version = \"X.Y.Z\" -> version = \"$VERSION\""
            # Show current version
            current=$(grep -A 5 '^\[workspace\.package\]' "$filepath" | grep '^version = ' | head -1 | sed 's/version = "\(.*\)"/\1/')
            echo "  Current: $current"
        else
            sed -i '/^\[workspace\.package\]/,/^\[/ s/^version = ".*"/version = "'"$VERSION"'"/' "$filepath"
        fi
        return 0
    else
        echo -e "${RED}  ERROR: [workspace.package] section not found in $file${NC}" >&2
        return 1
    fi
}

# Function to update regular Cargo.toml (package)
update_cargo() {
    local file="$1"
    local filepath="$PROJECT_ROOT/$file"

    if [[ ! -f "$filepath" ]]; then
        echo -e "${RED}  ERROR: File not found: $file${NC}" >&2
        return 1
    fi

    # Match version in [package] section
    if grep -q '^\[package\]' "$filepath"; then
        if [[ "$DRY_RUN" == true ]]; then
            echo "  Would update: version = \"X.Y.Z\" -> version = \"$VERSION\""
            # Show current version
            current=$(grep -A 5 '^\[package\]' "$filepath" | grep '^version = ' | head -1 | sed 's/version = "\(.*\)"/\1/')
            echo "  Current: $current"
        else
            sed -i '/^\[package\]/,/^\[/ s/^version = ".*"/version = "'"$VERSION"'"/' "$filepath"
        fi
        return 0
    else
        echo -e "${RED}  ERROR: [package] section not found in $file${NC}" >&2
        return 1
    fi
}

# Function to update package.json files
update_json() {
    local file="$1"
    local filepath="$PROJECT_ROOT/$file"

    if [[ ! -f "$filepath" ]]; then
        echo -e "${RED}  ERROR: File not found: $file${NC}" >&2
        return 1
    fi

    if [[ "$DRY_RUN" == true ]]; then
        echo "  Would update: \"version\": \"X.Y.Z\" -> \"version\": \"$VERSION\""
        # Show current version
        current=$(grep '"version":' "$filepath" | head -1 | sed 's/.*"version": *"\([^"]*\)".*/\1/')
        echo "  Current: $current"
    else
        # Use sed to update the version field (first occurrence only)
        sed -i '0,/"version": *"[^"]*"/ s/"version": *"[^"]*"/"version": "'"$VERSION"'"/' "$filepath"
    fi
    return 0
}

# Process each file
for file in "${!FILES[@]}"; do
    type="${FILES[$file]}"
    echo "Processing: $file"

    case "$type" in
        workspace)
            if update_workspace_cargo "$file"; then
                UPDATED_FILES+=("$file")
                echo -e "${GREEN}  OK${NC}"
            else
                FAILED_FILES+=("$file")
            fi
            ;;
        cargo)
            if update_cargo "$file"; then
                UPDATED_FILES+=("$file")
                echo -e "${GREEN}  OK${NC}"
            else
                FAILED_FILES+=("$file")
            fi
            ;;
        json)
            if update_json "$file"; then
                UPDATED_FILES+=("$file")
                echo -e "${GREEN}  OK${NC}"
            else
                FAILED_FILES+=("$file")
            fi
            ;;
    esac
done

echo ""
echo "========================================="
echo "Summary"
echo "========================================="

if [[ "$DRY_RUN" == true ]]; then
    echo -e "${YELLOW}DRY RUN - No files were modified${NC}"
fi

echo "Files processed: ${#UPDATED_FILES[@]}"
for f in "${UPDATED_FILES[@]}"; do
    echo -e "  ${GREEN}✓${NC} $f"
done

if [[ ${#FAILED_FILES[@]} -gt 0 ]]; then
    echo ""
    echo -e "${RED}Failed files: ${#FAILED_FILES[@]}${NC}"
    for f in "${FAILED_FILES[@]}"; do
        echo -e "  ${RED}✗${NC} $f"
    done
    exit 1
fi

if [[ "$DRY_RUN" == false ]]; then
    echo ""
    echo -e "${GREEN}Version updated to $VERSION in all files${NC}"
fi

exit 0
