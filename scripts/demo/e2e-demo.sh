#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
HAPPY_EVIDENCE_DEFAULT="$EVIDENCE_DIR/task-19-demo-happy.txt"
ERROR_EVIDENCE_DEFAULT="$EVIDENCE_DIR/task-19-demo-error.txt"

usage() {
    cat <<'EOF'
Usage: bash scripts/demo/e2e-demo.sh [--dry-run] [--prompt <text>] [--tool <name>] [--model <model>]

Run a full Phase-D demo flow:
  create agent -> run builtin lite task -> validate agent card -> collect events/audit/usage summary.

Options:
  --dry-run                 Validate prerequisites and planned steps without starting daemon
  --prompt <text>           Prompt for builtin lite run
  --tool <name>             Builtin tool name (default: builtin.lite.upper)
  --model <model>           Model name (default: claude-4-sonnet)
  --tamper-card-field <f>   Remove one top-level field from generated card before validation
  --happy-evidence <path>   Output path for happy-path evidence
  --error-evidence <path>   Output path for failure evidence
  -h, --help                Show help
EOF
}

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "missing command: $cmd" >&2
        exit 1
    fi
}

DRY_RUN=false
PROMPT="分析当前目录结构"
TOOL="builtin.lite.upper"
MODEL="claude-4-sonnet"
TAMPER_CARD_FIELD=""
HAPPY_EVIDENCE="$HAPPY_EVIDENCE_DEFAULT"
ERROR_EVIDENCE="$ERROR_EVIDENCE_DEFAULT"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --prompt)
            PROMPT="${2:-}"
            if [[ -z "$PROMPT" ]]; then
                echo "--prompt requires a value" >&2
                exit 1
            fi
            shift 2
            ;;
        --tool)
            TOOL="${2:-}"
            if [[ -z "$TOOL" ]]; then
                echo "--tool requires a value" >&2
                exit 1
            fi
            shift 2
            ;;
        --model)
            MODEL="${2:-}"
            if [[ -z "$MODEL" ]]; then
                echo "--model requires a value" >&2
                exit 1
            fi
            shift 2
            ;;
        --tamper-card-field)
            TAMPER_CARD_FIELD="${2:-}"
            if [[ -z "$TAMPER_CARD_FIELD" ]]; then
                echo "--tamper-card-field requires a value" >&2
                exit 1
            fi
            shift 2
            ;;
        --happy-evidence)
            HAPPY_EVIDENCE="${2:-}"
            if [[ -z "$HAPPY_EVIDENCE" ]]; then
                echo "--happy-evidence requires a value" >&2
                exit 1
            fi
            shift 2
            ;;
        --error-evidence)
            ERROR_EVIDENCE="${2:-}"
            if [[ -z "$ERROR_EVIDENCE" ]]; then
                echo "--error-evidence requires a value" >&2
                exit 1
            fi
            shift 2
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

for cmd in cargo uv python3; do
    require_cmd "$cmd"
done

if [[ ! -x "$REPO_ROOT/scripts/validate/agent-card-validate.sh" ]]; then
    echo "validator script is missing or not executable: scripts/validate/agent-card-validate.sh" >&2
    exit 1
fi

if [[ "$DRY_RUN" == "true" ]]; then
    echo "phase_d_demo=dry_run_ok"
    echo "would_build=agentd-daemon,agentctl"
    echo "would_run=CreateAgent+StartManagedAgent+builtin-lite"
    echo "would_validate_card=scripts/validate/agent-card-validate.sh"
    if [[ -n "$TAMPER_CARD_FIELD" ]]; then
        echo "would_tamper_card_field=$TAMPER_CARD_FIELD"
    fi
    echo "would_collect=audit,events,usage"
    exit 0
fi

TMP_DIR="$(mktemp -d)"
SOCKET_PATH="$TMP_DIR/agentd.sock"
DB_PATH="$TMP_DIR/agentd.sqlite"
CGROUP_ROOT="$TMP_DIR/cgroup"
AGENT_CARD_ROOT="$TMP_DIR/agent-cards"
HEALTH_PORT="$((17000 + (RANDOM % 1000)))"
CONFIG_PATH="$TMP_DIR/agentd.toml"
DAEMON_LOG="$TMP_DIR/daemon.log"
RUN_JSON="$TMP_DIR/run.json"
CARD_VALIDATE_JSON="$TMP_DIR/card-validate.json"
CARD_VALIDATE_ERR="$TMP_DIR/card-validate.err"
AUDIT_JSON="$TMP_DIR/audit.json"
EVENTS_JSON="$TMP_DIR/events.json"
USAGE_JSON="$TMP_DIR/usage.json"

AGENTD_BIN="$REPO_ROOT/target/debug/agentd"
AGENTCTL_BIN="$REPO_ROOT/target/debug/agentctl"
DAEMON_PID=""

cleanup() {
    if [[ -n "$DAEMON_PID" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
        kill "$DAEMON_PID" >/dev/null 2>&1 || true
        wait "$DAEMON_PID" >/dev/null 2>&1 || true
    fi
    rm -rf "$TMP_DIR"
}

fail() {
    local message="$1"
    {
        echo "task_19_demo=failed"
        echo "reason=$message"
        echo "daemon_log=$DAEMON_LOG"
        if [[ -f "$DAEMON_LOG" ]]; then
            echo "--- daemon_log_tail ---"
            tail -n 60 "$DAEMON_LOG" || true
        fi
        if [[ -f "$CARD_VALIDATE_ERR" ]]; then
            echo "--- card_validate_error ---"
            cat "$CARD_VALIDATE_ERR" || true
        fi
        if [[ -f "$CARD_VALIDATE_JSON" ]]; then
            echo "--- card_validate_output ---"
            cat "$CARD_VALIDATE_JSON" || true
        fi
    } >"$ERROR_EVIDENCE"
    echo "$message" >&2
    exit 1
}

trap cleanup EXIT

cat >"$CONFIG_PATH" <<EOF
[daemon]
health_host = "127.0.0.1"
health_port = $HEALTH_PORT
shutdown_timeout_secs = 5
socket_path = "$SOCKET_PATH"
db_path = "$DB_PATH"
cgroup_root = "$CGROUP_ROOT"
cgroup_parent = "agentd"
agent_card_root = "$AGENT_CARD_ROOT"

[one_api]
enabled = false
management_enabled = false
EOF

cd "$REPO_ROOT"
cargo build -p agentd-daemon -p agentctl >/dev/null

"$AGENTD_BIN" --config "$CONFIG_PATH" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

for _ in $(seq 1 80); do
    if "$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" health --json >/dev/null 2>&1; then
        break
    fi
    sleep 0.1
done

if ! "$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" health --json >/dev/null 2>&1; then
    fail "daemon health endpoint did not become ready"
fi

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent run --builtin lite --name demo-phase-d --model "$MODEL" --tool "$TOOL" --restart-max-attempts 0 --json "$PROMPT" >"$RUN_JSON" || fail "builtin lite run failed"

AGENT_ID="$(python3 - "$RUN_JSON" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as f:
    payload = json.load(f)
print(payload['agent']['id'])
PY
)" || fail "failed to parse agent id"

CARD_PATH="$AGENT_CARD_ROOT/$AGENT_ID/agent.json"
if [[ ! -f "$CARD_PATH" ]]; then
    fail "agent card not found at expected path: $CARD_PATH"
fi

if [[ -n "$TAMPER_CARD_FIELD" ]]; then
    python3 - "$CARD_PATH" "$TAMPER_CARD_FIELD" <<'PY'
import json
import sys

card_path, field_name = sys.argv[1:3]
with open(card_path, 'r', encoding='utf-8') as f:
    payload = json.load(f)
payload.pop(field_name, None)
with open(card_path, 'w', encoding='utf-8') as f:
    json.dump(payload, f)
PY
fi

"$REPO_ROOT/scripts/validate/agent-card-validate.sh" --card-path "$CARD_PATH" --json >"$CARD_VALIDATE_JSON" 2>"$CARD_VALIDATE_ERR" || fail "agent card validation failed"

sleep 1

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent audit --agent-id "$AGENT_ID" --json >"$AUDIT_JSON" || fail "audit query failed"
"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent events --limit 30 --json >"$EVENTS_JSON" || fail "events query failed"
"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" usage "$AGENT_ID" --json >"$USAGE_JSON" || fail "usage query failed"

python3 - "$RUN_JSON" "$CARD_VALIDATE_JSON" "$AUDIT_JSON" "$EVENTS_JSON" "$USAGE_JSON" "$CARD_PATH" "$HAPPY_EVIDENCE" <<'PY'
import json
import sys

run_path, card_validate_path, audit_path, events_path, usage_path, card_path, out_path = sys.argv[1:8]

with open(run_path, 'r', encoding='utf-8') as f:
    run_payload = json.load(f)
with open(card_validate_path, 'r', encoding='utf-8') as f:
    card_validate = json.load(f)
with open(audit_path, 'r', encoding='utf-8') as f:
    audit_payload = json.load(f)
with open(events_path, 'r', encoding='utf-8') as f:
    events_payload = json.load(f)
with open(usage_path, 'r', encoding='utf-8') as f:
    usage_payload = json.load(f)

if card_validate.get('status') != 'valid':
    raise SystemExit('card validation did not return valid status')

audit_events = audit_payload.get('events', [])
if not any(event.get('event_type') in {'ToolInvoked', 'ToolApproved'} for event in audit_events):
    raise SystemExit('missing ToolInvoked/ToolApproved in audit events')

events = events_payload.get('events', [])
if len(events) == 0:
    raise SystemExit('lifecycle events should not be empty')

total_tokens = int(usage_payload.get('total_tokens', 0))
if total_tokens <= 0:
    raise SystemExit('usage total_tokens should be > 0')

summary = {
    'task_19_demo': 'passed',
    'agent_id': run_payload['agent']['id'],
    'agent_card_path': card_path,
    'model': run_payload['agent']['model']['model_name'],
    'audit_events': len(audit_events),
    'lifecycle_events': len(events),
    'usage_total_tokens': total_tokens,
}

with open(out_path, 'w', encoding='utf-8') as f:
    for key, value in summary.items():
        f.write(f"{key}={value}\n")

print(json.dumps(summary, ensure_ascii=False))
PY

echo "task-19 e2e demo passed"
