#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
HAPPY_EVIDENCE="$EVIDENCE_DIR/task-17-bc-happy.txt"
ERROR_EVIDENCE="$EVIDENCE_DIR/task-17-bc-error.txt"
FAULT_MARKER="/tmp/agentd-phasebc-fault-marker"

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

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        log_error "Required command not found: $cmd"
        exit 1
    fi
}

LOCAL_MODE=false
while [[ $# -gt 0 ]]; do
    case "$1" in
        --local)
            LOCAL_MODE=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [--local]"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

mkdir -p "$EVIDENCE_DIR"

for cmd in cargo bash; do
    require_cmd "$cmd"
done

cd "$REPO_ROOT"

log_info "Running Phase B/C gate checks"

if [[ -f "$FAULT_MARKER" ]]; then
    injected_faults="$(grep '^faults=' "$FAULT_MARKER" | cut -d= -f2- || true)"
    if [[ -z "$injected_faults" ]]; then
        injected_faults="unknown"
    fi
    {
        echo "phase_bc_gate=failed"
        echo "reason=fault marker detected"
        echo "failed_items=$injected_faults"
        echo "marker=$FAULT_MARKER"
        echo "local_mode=$LOCAL_MODE"
    } >"$ERROR_EVIDENCE"

    rm -f "$FAULT_MARKER"
    log_error "Phase B/C gate failed due to injected faults"
    exit 1
fi

if [[ "$LOCAL_MODE" == "true" ]]; then
    log_warn "Running in local mode"
fi

cargo test -p agentd-daemon subscribe_events_returns_next_cursor_after_lifecycle_event >/dev/null
cargo test -p agentd-daemon managed_agent_lifecycle_emits_restart_and_oom_events >/dev/null
cargo test -p agentd-daemon authorize_tool_returns_stable_policy_deny_error_code >/dev/null
cargo test -p agentd-daemon usage_query_and_quota_enforcement_work >/dev/null

{
    echo "phase_bc_gate=passed"
    echo "checks=subscribe_events,restart_and_oom,policy_deny,usage_window_consistency"
    echo "fault_marker_present=false"
    echo "local_mode=$LOCAL_MODE"
} >"$HAPPY_EVIDENCE"

log_info "Phase B/C gate passed"
log_info "Evidence: $HAPPY_EVIDENCE"
