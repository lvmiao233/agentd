#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
HAPPY_EVIDENCE="$EVIDENCE_DIR/task-18-lite-happy.txt"
ERROR_EVIDENCE="$EVIDENCE_DIR/task-18-lite-error.txt"

mkdir -p "$EVIDENCE_DIR"

for cmd in cargo uv python3; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "missing command: $cmd" >&2
        exit 1
    fi
done

cd "$REPO_ROOT"

cargo build -p agentd-daemon -p agentctl >/dev/null

TMP_DIR="$(mktemp -d)"
SOCKET_PATH="$TMP_DIR/agentd.sock"
DB_PATH="$TMP_DIR/agentd.sqlite"
CGROUP_ROOT="$TMP_DIR/cgroup"
CONFIG_PATH="$TMP_DIR/agentd.toml"
DAEMON_LOG="$TMP_DIR/daemon.log"

cat >"$CONFIG_PATH" <<EOF
[daemon]
socket_path = "$SOCKET_PATH"
health_host = "127.0.0.1"
health_port = 17017
shutdown_timeout_secs = 5
db_path = "$DB_PATH"
cgroup_root = "$CGROUP_ROOT"
cgroup_parent = "agentd"

[one_api]
enabled = false
management_enabled = false
EOF

AGENTD_BIN="$REPO_ROOT/target/debug/agentd"
AGENTCTL_BIN="$REPO_ROOT/target/debug/agentctl"

"$AGENTD_BIN" --config "$CONFIG_PATH" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

cleanup() {
    if kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
        kill "$DAEMON_PID" >/dev/null 2>&1 || true
        wait "$DAEMON_PID" >/dev/null 2>&1 || true
    fi
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

for _ in $(seq 1 80); do
    if "$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" health --json >/dev/null 2>&1; then
        break
    fi
    sleep 0.1
done

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent run --builtin lite --name demo-lite --model claude-4-sonnet --tool builtin.lite.upper --restart-max-attempts 0 --json "分析当前目录结构" >"$TMP_DIR/happy-run.json"

HAPPY_AGENT_ID="$(python3 - "$TMP_DIR/happy-run.json" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as f:
    payload = json.load(f)
print(payload['agent']['id'])
PY
)"

sleep 1

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" usage "$HAPPY_AGENT_ID" --json >"$TMP_DIR/happy-usage.json"
"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent audit --agent-id "$HAPPY_AGENT_ID" --json >"$TMP_DIR/happy-audit.json"

python3 - "$TMP_DIR/happy-usage.json" "$TMP_DIR/happy-audit.json" "$HAPPY_EVIDENCE" <<'PY'
import json
import sys

usage_path, audit_path, out_path = sys.argv[1:4]

with open(usage_path, 'r', encoding='utf-8') as f:
    usage = json.load(f)
with open(audit_path, 'r', encoding='utf-8') as f:
    audit = json.load(f)

if int(usage.get('total_tokens', 0)) <= 0:
    raise SystemExit('happy path expected total_tokens > 0')

events = audit.get('events', [])
if not any(event.get('event_type') in {'ToolInvoked', 'ToolApproved'} for event in events):
    raise SystemExit('happy path expected ToolInvoked/ToolApproved audit event')

with open(out_path, 'w', encoding='utf-8') as out:
    out.write('task_18_happy=passed\n')
    out.write(f"total_tokens={usage.get('total_tokens')}\n")
    out.write(f"events={len(events)}\n")
PY

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent create --name demo-lite-deny --model claude-4-sonnet --permission-policy ask --deny-tool builtin.lite.echo --json >"$TMP_DIR/deny-create.json"

DENY_AGENT_ID="$(python3 - "$TMP_DIR/deny-create.json" <<'PY'
import json
import sys

with open(sys.argv[1], 'r', encoding='utf-8') as f:
    payload = json.load(f)
print(payload['agent']['id'])
PY
)"

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent run --builtin lite --name demo-lite-deny --model claude-4-sonnet --tool builtin.lite.echo --restart-max-attempts 0 --json "触发 deny 测试" >"$TMP_DIR/deny-run.json"

sleep 1

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" usage "$DENY_AGENT_ID" --json >"$TMP_DIR/deny-usage.json"
"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent audit --agent-id "$DENY_AGENT_ID" --json >"$TMP_DIR/deny-audit.json"

python3 - "$TMP_DIR/deny-usage.json" "$TMP_DIR/deny-audit.json" "$ERROR_EVIDENCE" <<'PY'
import json
import sys

usage_path, audit_path, out_path = sys.argv[1:4]

with open(usage_path, 'r', encoding='utf-8') as f:
    usage = json.load(f)
with open(audit_path, 'r', encoding='utf-8') as f:
    audit = json.load(f)

events = audit.get('events', [])
if not any(event.get('event_type') == 'ToolDenied' for event in events):
    raise SystemExit('error path expected ToolDenied audit event')

if int(usage.get('total_tokens', -1)) != 0:
    raise SystemExit('error path expected total_tokens to remain 0')

with open(out_path, 'w', encoding='utf-8') as out:
    out.write('task_18_error=passed\n')
    out.write(f"total_tokens={usage.get('total_tokens')}\n")
    out.write(f"events={len(events)}\n")
PY

echo "task-18-lite gate passed"
