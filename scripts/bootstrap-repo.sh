#!/bin/bash
#
# bootstrap-repo.sh - Bootstrap a public GitHub repository with fallback naming
#
# Usage:
#   ./scripts/bootstrap-repo.sh [--dry-run]
#   ./scripts/bootstrap-repo.sh --check        # Verify existing repo visibility
#
# This script attempts to create a PUBLIC repository with fallback names:
#   1. agentd
#   2. agentd-runtime
#   3. agentd-core
#
# Always creates PUBLIC repositories (never private) for security.
#

# Use pipefail but handle set -e carefully in functions that may return non-zero
set -uo pipefail

# Configuration
REPO_NAMES=("agentd" "agentd-runtime" "agentd-core")
DEFAULT_ORG=""  # Empty = use current user account

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

# Check if gh is installed and authenticated
check_gh_auth() {
    if ! command -v gh &> /dev/null; then
        log_error "gh CLI is not installed."
        log_error "Install from: https://cli.github.com/"
        exit 1
    fi

    if ! gh auth status &> /dev/null; then
        log_error "gh is not authenticated."
        log_error "Run 'gh auth login' to authenticate."
        exit 1
    fi

    log_info "gh authentication verified."
}

# Get the authenticated username or org
get_gh_owner() {
    gh api user --jq '.login' 2>/dev/null || echo ""
}

# Check if a repository exists
repo_exists() {
    local repo="$1"
    gh repo view "$repo" &>/dev/null
}

# Get repository visibility using exact format from plan: --json visibility,name
get_repo_visibility() {
    local repo="$1"
    gh repo view "$repo" --json visibility,name 2>/dev/null | grep -o '"visibility":"[^"]*"' | cut -d'"' -f4 || echo "not_found"
}

# Check if visibility is public (case-insensitive for GH API)
is_public() {
    local visibility="$1"
    [[ "${visibility,,}" == "public" ]]
}

# Check if current directory is a git repository
is_git_repo() {
    git rev-parse --git-dir &>/dev/null
}

# Attempt to create a public repository
create_repo() {
    local repo_name="$1"
    local dry_run="${2:-false}"
    
    local full_repo="$repo_name"
    if [[ -n "$DEFAULT_ORG" ]]; then
        full_repo="$DEFAULT_ORG/$repo_name"
    fi

    # Build gh command based on whether we're in a git repo
    local source_flag=""
    if is_git_repo; then
        source_flag="--source=."
    fi

    if [[ "$dry_run" == "true" ]]; then
        if [[ -n "$source_flag" ]]; then
            log_info "[DRY-RUN] Would execute: gh repo create $repo_name --public --source=. --description='agentd - System-level AI Agent runtime'"
        else
            log_info "[DRY-RUN] Would execute: gh repo create $repo_name --public --description='agentd - System-level AI Agent runtime'"
        fi
        return 1
    fi

    log_info "Attempting to create repository: $repo_name"

    if gh repo create "$repo_name" --public $source_flag --description="agentd - System-level AI Agent runtime" 2>/dev/null; then
        log_success "Repository '$repo_name' created successfully!"
        
        local actual_visibility
        actual_visibility=$(get_repo_visibility "$repo_name")
        
        if is_public "$actual_visibility"; then
            log_success "Verified: Repository '$repo_name' is PUBLIC"
            echo ""
            echo "=========================================="
            echo "REPOSITORY CREATED SUCCESSFULLY"
            echo "=========================================="
            echo "Name: $repo_name"
            echo "Visibility: $actual_visibility"
            echo "URL: https://github.com/$(get_gh_owner)/$repo_name"
            echo "=========================================="
            return 0
        else
            log_warn "Repository created but visibility is: $actual_visibility"
            return 1
        fi
    else
        log_warn "Failed to create repository '$repo_name' (may already exist)"
        return 1
    fi
}

# Check existing repository visibility
check_existing_repo() {
    local repo_name="$1"
    
    if repo_exists "$repo_name"; then
        local visibility
        visibility=$(get_repo_visibility "$repo_name")
        
        echo ""
        echo "=========================================="
        echo "REPOSITORY STATUS CHECK"
        echo "=========================================="
        echo "Name: $repo_name"
        echo "Visibility: $visibility"
        
        if is_public "$visibility"; then
            log_success "Repository '$repo_name' is PUBLIC"
            echo "URL: https://github.com/$(get_gh_owner)/$repo_name"
            echo "=========================================="
            return 0
        else
            log_error "Repository '$repo_name' exists but is NOT PUBLIC (visibility: $visibility)"
            echo "=========================================="
            return 1
        fi
    else
        log_info "Repository '$repo_name' does not exist"
        return 2
    fi
}

# Main function
main() {
    local mode="create"
    local check_name=""
    
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dry-run)
                mode="dry-run"
                shift
                ;;
            --check)
                mode="check"
                shift
                ;;
            --check=*)
                mode="check"
                check_name="${1#*=}"
                shift
                ;;
            -h|--help)
                echo "Usage: $0 [OPTIONS]"
                echo ""
                echo "Options:"
                echo "  --dry-run              Show commands that would be executed without running them"
                echo "  --check[=REPO_NAME]    Check visibility of existing repository"
                echo "  -h, --help             Show this help message"
                echo ""
                echo "Examples:"
                echo "  $0                      # Create repository with fallback naming"
                echo "  $0 --dry-run            # Show what would be created"
                echo "  $0 --check              # Check if 'agentd' exists and is public"
                echo "  $0 --check=agentd-core # Check specific repo"
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                exit 1
                ;;
        esac
    done

    echo ""
    echo "=========================================="
    echo "GitHub Repository Bootstrap Script"
    echo "=========================================="
    echo ""

    # Check gh authentication first
    check_gh_auth

    if [[ "$mode" == "check" ]]; then
        local check_specific=""
        if [[ -n "$check_name" ]]; then
            check_specific=" (checking: $check_name)"
        fi
        log_info "Checking repository visibility$check_specific:"
        echo "  Order: ${REPO_NAMES[*]}"
        echo ""
        
        for repo_name in "${REPO_NAMES[@]}"; do
            if [[ -n "$check_name" && "$repo_name" != "$check_name" ]]; then
                continue
            fi
            
            local result
            check_existing_repo "$repo_name"
            result=$?
            
            if [[ $result -eq 0 ]]; then
                exit 0
            elif [[ $result -eq 1 ]]; then
                exit 1
            fi
        done
        
        if [[ -n "$check_name" ]]; then
            log_warn "Repository '$check_name' not found or not public"
        else
            log_warn "No existing repository found from: ${REPO_NAMES[*]}"
        fi
        exit 2
    fi

    # Create or dry-run mode
    local owner
    owner=$(get_gh_owner)
    log_info "Authenticated as: $owner"
    echo ""
    log_info "Attempting to create PUBLIC repository with fallback names:"
    echo "  Order: ${REPO_NAMES[*]}"
    echo ""

    for repo_name in "${REPO_NAMES[@]}"; do
        echo "--- Trying: $repo_name ---"
        
        local result
        create_repo "$repo_name" "$([[ "$mode" == "dry-run" ]] && echo "true" || echo "false")"
        result=$?
        
        # In dry-run mode, always continue to show all fallback attempts
        if [[ "$mode" == "dry-run" ]]; then
            echo ""
            continue
        fi
        
        # In create mode, exit on success
        if [[ $result -eq 0 ]]; then
            exit 0
        fi
        
        # Check if repo already exists with correct visibility
        if repo_exists "$repo_name"; then
            local visibility
            visibility=$(get_repo_visibility "$repo_name")
            
            if is_public "$visibility"; then
                log_success "Repository '$repo_name' already exists and is PUBLIC!"
                echo ""
                echo "=========================================="
                echo "REPOSITORY ALREADY EXISTS (PUBLIC)"
                echo "=========================================="
                echo "Name: $repo_name"
                echo "Visibility: $visibility"
                echo "URL: https://github.com/$owner/$repo_name"
                echo "=========================================="
                exit 0
            else
                log_warn "Repository '$repo_name' exists but is $visibility (not public)"
            fi
        fi
        
        echo ""
    done

    if [[ "$mode" == "dry-run" ]]; then
        echo ""
        echo "=========================================="
        echo "DRY-RUN COMPLETE"
        echo "=========================================="
        echo "All fallback attempts logged above."
        echo "Would have tried: ${REPO_NAMES[*]}"
        echo "=========================================="
        exit 0
    fi

    log_error "All repository name options exhausted."
    log_error "Could not create or find a public repository."
    log_error "Tried: ${REPO_NAMES[*]}"
    exit 1
}

# Run main function
main "$@"
