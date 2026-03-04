#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ROOTFS_ROOT="$REPO_ROOT/images/agent-rootfs"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"

MANIFEST_PATH="$ROOTFS_ROOT/rootfs-manifest.json"
ROOTFS_DIR=""
SUCCESS_EVIDENCE="$EVIDENCE_DIR/task-19-rootfs-build.txt"
ERROR_EVIDENCE="$EVIDENCE_DIR/task-19-rootfs-missing-runtime.txt"

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

usage() {
    cat <<'EOF'
Usage: bash scripts/firecracker/verify-rootfs.sh [options]

Verify built Firecracker rootfs includes python runtime + agent-lite payload.

Options:
  --manifest <path>          Manifest path (default: images/agent-rootfs/rootfs-manifest.json)
  --rootfs-dir <path>        Rootfs directory (default: read from manifest)
  --success-evidence <path>  Success evidence file path
  --error-evidence <path>    Failure evidence file path
  -h, --help                 Show this help
EOF
}

fail() {
    local reason="$1"
    mkdir -p "$EVIDENCE_DIR"
    {
        echo "task=19"
        echo "status=failed"
        echo "step=verify-rootfs"
        echo "reason=$reason"
        if [[ -n "$ROOTFS_DIR" ]]; then
            echo "rootfs_dir=$ROOTFS_DIR"
        fi
    } >"$ERROR_EVIDENCE"
    log_error "$reason"
    exit 1
}

require_file() {
    local path="$1"
    local label="$2"
    if [[ ! -f "$path" ]]; then
        fail "missing $label: $path"
    fi
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --manifest)
            MANIFEST_PATH="${2:-}"
            [[ -n "$MANIFEST_PATH" ]] || fail "--manifest requires value"
            shift 2
            ;;
        --rootfs-dir)
            ROOTFS_DIR="${2:-}"
            [[ -n "$ROOTFS_DIR" ]] || fail "--rootfs-dir requires value"
            shift 2
            ;;
        --success-evidence)
            SUCCESS_EVIDENCE="${2:-}"
            [[ -n "$SUCCESS_EVIDENCE" ]] || fail "--success-evidence requires value"
            shift 2
            ;;
        --error-evidence)
            ERROR_EVIDENCE="${2:-}"
            [[ -n "$ERROR_EVIDENCE" ]] || fail "--error-evidence requires value"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            fail "unknown option: $1"
            ;;
    esac
done

if [[ ! -f "$MANIFEST_PATH" ]]; then
    fail "manifest not found: $MANIFEST_PATH"
fi

if [[ -z "$ROOTFS_DIR" ]]; then
    ROOTFS_REL="$(python3 - "$MANIFEST_PATH" <<'PY'
import json
import sys

data = json.load(open(sys.argv[1], encoding="utf-8"))
print(data["artifact"]["rootfs_dir"])
PY
)"
    if [[ "$ROOTFS_REL" == /* ]]; then
        ROOTFS_DIR="$ROOTFS_REL"
    else
        ROOTFS_DIR="$REPO_ROOT/$ROOTFS_REL"
    fi
fi

if [[ ! -d "$ROOTFS_DIR" ]]; then
    fail "rootfs directory not found: $ROOTFS_DIR"
fi

PYTHON_ROOTFS="$ROOTFS_DIR/usr/bin/python3"
ENTRYPOINT_ROOTFS="$ROOTFS_DIR/usr/local/bin/agentd-agent-lite"
MODULE_ROOTFS="$ROOTFS_DIR/opt/agent-lite/src/agentd_agent_lite/cli.py"
MANIFEST_ROOTFS="$ROOTFS_DIR/etc/agentd-rootfs-manifest.json"

require_file "$PYTHON_ROOTFS" "python runtime"
require_file "$ENTRYPOINT_ROOTFS" "agent-lite entrypoint"
require_file "$MODULE_ROOTFS" "agent-lite module"
require_file "$MANIFEST_ROOTFS" "rootfs internal manifest"

if [[ ! -x "$PYTHON_ROOTFS" ]]; then
    fail "python runtime is not executable: $PYTHON_ROOTFS"
fi
if [[ ! -x "$ENTRYPOINT_ROOTFS" ]]; then
    fail "agent-lite entrypoint is not executable: $ENTRYPOINT_ROOTFS"
fi

if ! grep -q "agentd_agent_lite.cli" "$ENTRYPOINT_ROOTFS"; then
    fail "agent-lite entrypoint missing cli target"
fi

PYTHON_VERSION="$("$PYTHON_ROOTFS" --version 2>&1 || true)"
if [[ -z "$PYTHON_VERSION" ]]; then
    fail "python runtime failed to report version"
fi

if ! PYTHONPATH="$ROOTFS_DIR/opt/agent-lite/src" "$PYTHON_ROOTFS" -c "import agentd_agent_lite.cli; print('agent-lite-import-ok')" >/tmp/agentd-task19-import.out 2>/tmp/agentd-task19-import.err; then
    IMPORT_ERR="$(tr '\n' ' ' </tmp/agentd-task19-import.err)"
    fail "agent-lite import failed: $IMPORT_ERR"
fi

if ! PYTHONPATH="$ROOTFS_DIR/opt/agent-lite/src" "$PYTHON_ROOTFS" -m agentd_agent_lite.cli --help >/tmp/agentd-task19-help.out 2>/tmp/agentd-task19-help.err; then
    HELP_ERR="$(tr '\n' ' ' </tmp/agentd-task19-help.err)"
    fail "agent-lite help command failed: $HELP_ERR"
fi

mkdir -p "$EVIDENCE_DIR"
if [[ -s "$SUCCESS_EVIDENCE" ]]; then
    echo "---" >>"$SUCCESS_EVIDENCE"
fi
{
    echo "task=19"
    echo "status=passed"
    echo "step=verify-rootfs"
    echo "rootfs_dir=${ROOTFS_DIR#$REPO_ROOT/}"
    echo "python_version=$PYTHON_VERSION"
    echo "python_runtime=available"
    echo "agent_lite=available"
    echo "import_check=passed"
    echo "help_check=passed"
} >>"$SUCCESS_EVIDENCE"

rm -f /tmp/agentd-task19-import.out /tmp/agentd-task19-import.err /tmp/agentd-task19-help.out /tmp/agentd-task19-help.err

log_info "Rootfs verification passed"
log_info "Success evidence: $SUCCESS_EVIDENCE"
