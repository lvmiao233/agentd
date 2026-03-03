#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
ERROR_EVIDENCE="$EVIDENCE_DIR/task-20-rc-error.txt"

usage() {
    cat <<'EOF'
Usage: bash scripts/rollback/final-rollback.sh [--baseline <git-ref>] [--execute]

Rollback helper for final release hardening rehearsal.

Options:
  --baseline <git-ref>  Target baseline to reset to (default: HEAD)
  --execute             Perform destructive rollback (without this flag, dry-run only)
  -h, --help            Show help
EOF
}

BASELINE="HEAD"
EXECUTE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --baseline)
            BASELINE="${2:-}"
            if [[ -z "$BASELINE" ]]; then
                echo "--baseline requires a value" >&2
                exit 1
            fi
            shift 2
            ;;
        --execute)
            EXECUTE=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

mkdir -p "$EVIDENCE_DIR"

cd "$REPO_ROOT"

if ! git rev-parse --verify "$BASELINE" >/dev/null 2>&1; then
    echo "baseline does not exist: $BASELINE" >&2
    exit 1
fi

if [[ "$EXECUTE" != "true" ]]; then
    {
        echo "task_20_final_rollback=dry_run"
        echo "baseline=$BASELINE"
        echo "planned_reset=git reset --hard $BASELINE"
        echo "planned_clean=git clean -fd"
        echo "planned_verify=bash scripts/release/rc-gate.sh --local"
    } >>"$ERROR_EVIDENCE"
    echo "final rollback dry-run recorded"
    exit 0
fi

git reset --hard "$BASELINE"
git clean -fd

bash scripts/release/rc-gate.sh --local

{
    echo "task_20_final_rollback=executed"
    echo "baseline=$BASELINE"
    echo "verify_rc_gate=passed"
} >>"$ERROR_EVIDENCE"

echo "final rollback executed and verified"
