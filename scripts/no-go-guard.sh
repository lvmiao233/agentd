#!/usr/bin/env bash
#
# no-go-guard.sh - Detect Go toolchain/build markers in forbidden paths
#
# This guard script ensures Go is not introduced into the agentd project.
# It checks for Go toolchain files, build markers, and Go-related configurations.
#
# Exit codes:
#   0 - No Go markers found (PASS)
#   1 - Go markers detected (FAIL)
#
# Usage:
#   bash scripts/no-go-guard.sh
#   bash scripts/no-go-guard.sh --verbose

set -uo pipefail

# Configuration
FORBIDDEN_PATTERNS=(
    # Go toolchain files
    "go.mod"
    "go.sum"
    "go.work"
    "go.work.sum"
    
    # Go build markers in source files
    "^package main"
    "^package "
    
    # Go-specific directories
    "/cmd/"
    "/internal/"
    "/pkg/"
    
    # Go build artifacts (common)
    "*.go"
)

# Paths to check (relative to repo root)
CHECK_PATHS=(
    "."
    "crates"
    "src"
    "cmd"
    "internal"
    "pkg"
    "."
)

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Verbose mode
VERBOSE=${VERBOSE:-0}

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Detect Go toolchain/build markers in forbidden paths.

OPTIONS:
    -h, --help      Show this help message
    -v, --verbose   Enable verbose output

EXIT CODES:
    0 - No Go markers found (PASS)
    1 - Go markers detected (FAIL)

EXAMPLES:
    # Basic check
    bash scripts/no-go-guard.sh

    # Verbose output
    bash scripts/no-go-guard.sh --verbose
EOF
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -v|--verbose)
            VERBOSE=1
            shift
            ;;
        *)
            echo "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Find repo root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$REPO_ROOT" || exit 1

log_info "Running no-Go guard check in: $REPO_ROOT"

# Track violations
VIOLATIONS=0

# Check 1: Go toolchain files in root
for file in go.mod go.sum go.work go.work.sum; do
    if [[ -f "$file" ]]; then
        log_error "Forbidden Go toolchain file found: $file"
        VIOLATIONS=$((VIOLATIONS + 1))
    fi
done

# Check 2: Go source files (*.go) in forbidden locations
# We allow go files only in research/ directory (for studying prior art)
forbidden_go_dirs=(
    "crates"
    "src"
    "cmd"
    "internal"
    "pkg"
    "agentd"
    "."
)

for dir in "${forbidden_go_dirs[@]}"; do
    if [[ -d "$dir" ]]; then
        # Find .go files but exclude test files in research directory
        while IFS= read -r -d '' gofile; do
            # Skip if it's in research/ (allowed for study) - handles both "research/" and "./research/"
            if [[ "$gofile" =~ ^(\./)?research/ ]]; then
                continue
            fi
            log_error "Forbidden Go source file found: $gofile"
            VIOLATIONS=$((VIOLATIONS + 1))
        done < <(find "$dir" -maxdepth 3 -name "*.go" -print0 2>/dev/null || true)
    fi
done

# Check 3: Go-specific directories in root (except research)
forbidden_dirs=("cmd" "internal" "pkg")
for dir in "${forbidden_dirs[@]}"; do
    if [[ -d "$dir" ]] && [[ "$dir" != "research" ]]; then
        # Check if directory contains Go files
        if find "$dir" -maxdepth 2 -name "*.go" 2>/dev/null | grep -q .; then
            log_error "Forbidden Go-specific directory with Go files: $dir/"
            VIOLATIONS=$((VIOLATIONS + 1))
        fi
    fi
done

# Check 4: Look for Go build configuration in Cargo.toml (shouldn't have go dependencies)
if grep -r "^go " . --include="*.toml" 2>/dev/null | grep -v "^./research" | grep -q .; then
    log_error "Go build markers found in Cargo.toml files"
    grep -r "^go " . --include="*.toml" 2>/dev/null | grep -v "^./research" || true
    VIOLATIONS=$((VIOLATIONS + 1))
fi

# Check 5: GitHub Actions workflows with Go
if grep -r "go-version" .github/workflows/ 2>/dev/null | grep -q .; then
    log_error "Go version specified in GitHub Actions workflows"
    grep -r "go-version" .github/workflows/ 2>/dev/null || true
    VIOLATIONS=$((VIOLATIONS + 1))
fi

# Summary
if [[ $VIOLATIONS -gt 0 ]]; then
    log_error "No-Go guard FAILED: $VIOLATIONS violation(s) detected"
    log_error "This project does not permit Go toolchain or Go implementation files"
    exit 1
else
    log_info "No-Go guard PASSED: No Go toolchain or build markers detected"
    exit 0
fi
