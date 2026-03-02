#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
ERROR_EVIDENCE="$EVIDENCE_DIR/task-12-phase-a-error.txt"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $*"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*" >&2
}

BASELINE=""
EXECUTE=false
VERIFY_CMD="cargo check --workspace"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --baseline)
            BASELINE="$2"
            shift 2
            ;;
        --execute)
            EXECUTE=true
            shift
            ;;
        --verify-cmd)
            VERIFY_CMD="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 --baseline <git-ref> [--execute] [--verify-cmd <cmd>]"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

if [[ -z "$BASELINE" ]]; then
    log_error "--baseline is required"
    exit 1
fi

mkdir -p "$EVIDENCE_DIR"

cd "$REPO_ROOT"

if ! git rev-parse --verify "$BASELINE" >/dev/null 2>&1; then
    log_error "Baseline reference does not exist: $BASELINE"
    exit 1
fi

if [[ "$EXECUTE" != "true" ]]; then
    log_warn "Dry run mode. No repository changes will be made."
    {
        echo "phase_a_rollback=dry_run"
        echo "baseline=$BASELINE"
        echo "verify_cmd=$VERIFY_CMD"
        echo "planned_commands=git reset --hard $BASELINE && git clean -fd && $VERIFY_CMD"
    } >"$ERROR_EVIDENCE"
    exit 0
fi

log_info "Executing rollback to baseline: $BASELINE"
git reset --hard "$BASELINE"
git clean -fd

log_info "Running verification command"
bash -lc "$VERIFY_CMD"

{
    echo "phase_a_rollback=executed"
    echo "baseline=$BASELINE"
    echo "verify_cmd=$VERIFY_CMD"
    echo "status=success"
} >"$ERROR_EVIDENCE"

log_info "Rollback completed"
