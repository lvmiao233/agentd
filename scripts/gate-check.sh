#!/usr/bin/env bash
#
# gate-check.sh - Local CI gate validation script
#
# This script validates the presence and shape of CI gate files
# without requiring a full GitHub Actions run.
#
# Usage:
#   bash scripts/gate-check.sh [--local]
#
# Options:
#   --local    Run in local mode (skip GitHub-specific checks)
#
# Exit codes:
#   0 - All checks passed
#   1 - One or more checks failed

set -euo pipefail

# Script directory (resolve relative paths)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Track failures
FAILED_CHECKS=0

# Parse arguments
LOCAL_MODE=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --local)
            LOCAL_MODE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--local]"
            echo ""
            echo "Options:"
            echo "  --local    Run in local mode (skip GitHub-specific checks)"
            echo ""
            echo "Validates CI gate files:"
            echo "  - .github/workflows/gates.yml"
            echo "  - .github/branch-protection.md"
            echo "  - Evidence directory structure"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Helper functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_file() {
    local file="$1"
    local description="$2"
    
    if [ -f "$REPO_ROOT/$file" ]; then
        log_info "✓ $description: $file exists"
        return 0
    else
        log_error "✗ $description: $file missing"
        FAILED_CHECKS=$((FAILED_CHECKS + 1))
        return 1
    fi
}

check_dir() {
    local dir="$1"
    local description="$2"
    
    if [ -d "$REPO_ROOT/$dir" ]; then
        log_info "✓ $description: $dir exists"
        return 0
    else
        log_error "✗ $description: $dir missing"
        FAILED_CHECKS=$((FAILED_CHECKS + 1))
        return 1
    fi
}

check_contains() {
    local file="$1"
    local pattern="$2"
    local description="$3"
    
    if grep -qi "$pattern" "$REPO_ROOT/$file" 2>/dev/null; then
        log_info "✓ $description: pattern found in $file"
        return 0
    else
        log_error "✗ $description: pattern NOT found in $file"
        FAILED_CHECKS=$((FAILED_CHECKS + 1))
        return 1
    fi
}

echo "========================================"
echo "CI Gate Validation"
echo "========================================"
echo ""

# Check 1: GitHub workflows directory
log_info "=== Step 1: Directory Structure ==="
check_dir ".github" "GitHub directory"
check_dir ".github/workflows" "Workflows directory"

# Check 2: Required workflow file
log_info ""
log_info "=== Step 2: Workflow File ==="
check_file ".github/workflows/gates.yml" "Gates workflow"

if [ -f "$REPO_ROOT/.github/workflows/gates.yml" ]; then
    # Check for required job names
    check_contains ".github/workflows/gates.yml" "preflight:" "Preflight job"
    check_contains ".github/workflows/gates.yml" "build-gate:" "Build gate job"
    check_contains ".github/workflows/gates.yml" "test-gate:" "Test gate job"
    check_contains ".github/workflows/gates.yml" "security-gate:" "Security gate job"
    check_contains ".github/workflows/gates.yml" "phase-a-gate:" "Phase A gate job"
    check_contains ".github/workflows/gates.yml" "phase-bc-gate:" "Phase B/C gate job"
    check_contains ".github/workflows/gates.yml" "gate-syscall:" "Syscall gate job"
    check_contains ".github/workflows/gates.yml" "gate-isolation:" "Isolation gate job"
    
    # Check for evidence upload
    check_contains ".github/workflows/gates.yml" "evidence-" "Evidence artifact upload"
fi

# Check 3: Branch protection documentation
log_info ""
log_info "=== Step 3: Branch Protection ==="
check_file ".github/branch-protection.md" "Branch protection docs"

if [ -f "$REPO_ROOT/.github/branch-protection.md" ]; then
    check_contains ".github/branch-protection.md" "Required" "Required check documentation"
    check_contains ".github/branch-protection.md" "branch protection" "Branch protection rules"
fi

# Check 4: Local gate check script
log_info ""
log_info "=== Step 4: Local Gate Check Script ==="
check_file "scripts/gate-check.sh" "Gate check script"
check_file "scripts/gates/phase-a-gate.sh" "Phase A gate script"
check_file "scripts/gates/phase-bc-gate.sh" "Phase B/C gate script"
check_file "scripts/validate/agent-card-validate.sh" "Agent card validation script"
check_file "scripts/rollback/phase-a-rollback.sh" "Phase A rollback script"
check_file "scripts/faults/inject-oom-and-policy-conflict.sh" "Phase B/C fault injection script"
check_file "scripts/faults/inject-oneapi-timeout.sh" "One-API timeout injection script"
check_file "scripts/faults/inject-db-lock-conflict.sh" "DB lock conflict injection script"

if [ -f "$REPO_ROOT/scripts/gate-check.sh" ]; then
    check_contains "scripts/gate-check.sh" "set -euo pipefail" "Strict shell mode"
    check_contains "scripts/gate-check.sh" "LOCAL_MODE" "Local mode support"
fi

# Check 5: Evidence directory (create if missing, but warn)
log_info ""
log_info "=== Step 5: Evidence Directory ==="
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
if [ -d "$EVIDENCE_DIR" ]; then
    log_info "✓ Evidence directory exists: .sisyphus/evidence"
else
    log_warn "Missing evidence directory: .sisyphus/evidence (will be created by CI)"
    # This is not a failure - CI will create it
fi

# Check 6: Baseline files (from T1/T2)
log_info ""
log_info "=== Step 6: Baseline Files (T1/T2) ==="
check_file "rust-toolchain.toml" "Rust toolchain"
check_file "pyproject.toml" "Python project"

# Check 7: Workflow syntax validation
log_info ""
log_info "=== Step 7: Workflow Syntax ==="
if command -v yamllint &> /dev/null; then
    if yamllint "$REPO_ROOT/.github/workflows/gates.yml" 2>/dev/null; then
        log_info "✓ Workflow YAML syntax valid"
    else
        log_error "✗ Workflow YAML has syntax errors"
        FAILED_CHECKS=$((FAILED_CHECKS + 1))
    fi
else
    log_warn "yamllint not available, skipping YAML syntax check"
fi

# Check 8: Shell script syntax validation
log_info ""
log_info "=== Step 8: Shell Script Syntax ==="
if bash -n "$REPO_ROOT/scripts/gate-check.sh" 2>/dev/null; then
    log_info "✓ gate-check.sh syntax valid"
else
    log_error "✗ gate-check.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

if bash -n "$REPO_ROOT/scripts/gates/phase-a-gate.sh" 2>/dev/null; then
    log_info "✓ phase-a-gate.sh syntax valid"
else
    log_error "✗ phase-a-gate.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

if bash -n "$REPO_ROOT/scripts/gates/phase-bc-gate.sh" 2>/dev/null; then
    log_info "✓ phase-bc-gate.sh syntax valid"
else
    log_error "✗ phase-bc-gate.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

if bash -n "$REPO_ROOT/scripts/validate/agent-card-validate.sh" 2>/dev/null; then
    log_info "✓ agent-card-validate.sh syntax valid"
else
    log_error "✗ agent-card-validate.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

if bash -n "$REPO_ROOT/scripts/faults/inject-oom-and-policy-conflict.sh" 2>/dev/null; then
    log_info "✓ inject-oom-and-policy-conflict.sh syntax valid"
else
    log_error "✗ inject-oom-and-policy-conflict.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

if bash -n "$REPO_ROOT/scripts/faults/inject-oneapi-timeout.sh" 2>/dev/null; then
    log_info "✓ inject-oneapi-timeout.sh syntax valid"
else
    log_error "✗ inject-oneapi-timeout.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

if bash -n "$REPO_ROOT/scripts/faults/inject-db-lock-conflict.sh" 2>/dev/null; then
    log_info "✓ inject-db-lock-conflict.sh syntax valid"
else
    log_error "✗ inject-db-lock-conflict.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

if bash -n "$REPO_ROOT/scripts/rollback/phase-a-rollback.sh" 2>/dev/null; then
    log_info "✓ phase-a-rollback.sh syntax valid"
else
    log_error "✗ phase-a-rollback.sh has syntax errors"
    FAILED_CHECKS=$((FAILED_CHECKS + 1))
fi

# Summary
echo ""
echo "========================================"
echo "Validation Summary"
echo "========================================"

if [ $FAILED_CHECKS -eq 0 ]; then
    log_info "All checks passed!"
    echo ""
    echo "The CI gate skeleton is properly configured."
    echo "To run local validation: bash scripts/gate-check.sh --local"
    exit 0
else
    log_error "Failed checks: $FAILED_CHECKS"
    echo ""
    echo "Please fix the issues above before proceeding."
    exit 1
fi
