#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
HAPPY_EVIDENCE="$EVIDENCE_DIR/task-12-phase-a-happy.json"
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

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        log_error "Required command not found: $cmd"
        exit 1
    fi
}

mkdir -p "$EVIDENCE_DIR"

for cmd in cargo curl python3; do
    require_cmd "$cmd"
done

RUN_ID="${RANDOM}${RANDOM}"
HEALTH_PORT="$((20000 + (RANDOM % 10000)))"
SOCKET_PATH="/tmp/agentd-phasea-${RUN_ID}.sock"
DB_PATH="/tmp/agentd-phasea-${RUN_ID}.db"
CONFIG_PATH="/tmp/agentd-phasea-${RUN_ID}.toml"
DAEMON_LOG="/tmp/agentd-phasea-${RUN_ID}.log"

RPC_HEALTH_JSON="/tmp/agentd-phasea-${RUN_ID}-rpc-health.json"
CREATE_JSON="/tmp/agentd-phasea-${RUN_ID}-create.json"
LIST_JSON="/tmp/agentd-phasea-${RUN_ID}-list.json"
USAGE_INITIAL_JSON="/tmp/agentd-phasea-${RUN_ID}-usage-initial.json"
RECORD_OK_JSON="/tmp/agentd-phasea-${RUN_ID}-record-ok.json"
RECORD_OVER_JSON="/tmp/agentd-phasea-${RUN_ID}-record-over.json"
USAGE_FINAL_JSON="/tmp/agentd-phasea-${RUN_ID}-usage-final.json"

DAEMON_PID=""

cleanup() {
    if [[ -n "${DAEMON_PID}" ]] && kill -0 "$DAEMON_PID" >/dev/null 2>&1; then
        kill -TERM "$DAEMON_PID" >/dev/null 2>&1 || true
        wait "$DAEMON_PID" >/dev/null 2>&1 || true
    fi

    rm -f "$SOCKET_PATH" "$CONFIG_PATH" "$DB_PATH" "${DB_PATH}-wal" "${DB_PATH}-shm"
}

fail() {
    local message="$1"
    {
        echo "phase_a_gate=failed"
        echo "reason=${message}"
        echo "daemon_log=${DAEMON_LOG}"
        if [[ -f "$DAEMON_LOG" ]]; then
            echo "--- daemon_log_tail ---"
            tail -n 50 "$DAEMON_LOG" || true
        fi
    } >"$ERROR_EVIDENCE"

    log_error "$message"
    exit 1
}

normalize_json_file() {
    local file_path="$1"
    python3 - "$file_path" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8", errors="ignore")
start = text.find("{")
if start == -1:
    raise SystemExit(1)
path.write_text(text[start:], encoding="utf-8")
PY
}

trap cleanup EXIT

cat >"$CONFIG_PATH" <<EOF
[daemon]
health_host = "127.0.0.1"
health_port = ${HEALTH_PORT}
shutdown_timeout_secs = 5
socket_path = "${SOCKET_PATH}"
db_path = "${DB_PATH}"

[one_api]
enabled = false
command = "one-api"
args = []
health_url = "http://127.0.0.1:3000/health"
startup_timeout_secs = 30
restart_max_attempts = 3
restart_backoff_secs = 2
management_enabled = false
management_base_url = "http://127.0.0.1:3000"
management_timeout_secs = 5
management_retries = 3
management_retry_backoff_secs = 1
create_token_path = "/api/token/"
create_channel_path = "/api/channel/"
provision_channel = false
EOF

log_info "Starting Phase A gate run"
log_info "Using health port: ${HEALTH_PORT}, socket: ${SOCKET_PATH}"

cd "$REPO_ROOT"

cargo build -p agentd-daemon -p agentctl >/dev/null
AGENTD_BIN="$REPO_ROOT/target/debug/agentd"
AGENTCTL_BIN="$REPO_ROOT/target/debug/agentctl"

START_MS=$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)

"$AGENTD_BIN" --config "$CONFIG_PATH" >"$DAEMON_LOG" 2>&1 &
DAEMON_PID=$!

health_ready=false
for _ in $(seq 1 80); do
    if curl --noproxy '*' -fsS "http://127.0.0.1:${HEALTH_PORT}/health" >/dev/null 2>&1; then
        health_ready=true
        break
    fi
    sleep 0.25
done

if [[ "$health_ready" != "true" ]]; then
    fail "daemon health endpoint did not become ready"
fi

READY_MS=$(python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
)
STARTUP_LATENCY_MS=$((READY_MS - START_MS))

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" health --json >"$RPC_HEALTH_JSON" \
    || fail "agentctl health failed"
normalize_json_file "$RPC_HEALTH_JSON" || fail "failed to normalize health json"

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent create \
    --name phasea-agent --model claude-4-sonnet --token-budget 100 --json >"$CREATE_JSON" \
    || fail "agentctl agent create failed"
normalize_json_file "$CREATE_JSON" || fail "failed to normalize create json"

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" agent list --json >"$LIST_JSON" \
    || fail "agentctl agent list failed"
normalize_json_file "$LIST_JSON" || fail "failed to normalize list json"

AGENT_ID=$(python3 - "$CREATE_JSON" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))
print(data["agent"]["id"])
PY
) || fail "failed to parse agent id from create response"

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" usage "$AGENT_ID" --json >"$USAGE_INITIAL_JSON" \
    || fail "initial usage query failed"
normalize_json_file "$USAGE_INITIAL_JSON" || fail "failed to normalize initial usage json"

python3 - "$SOCKET_PATH" "$AGENT_ID" "$RECORD_OK_JSON" "$RECORD_OVER_JSON" <<'PY'
import json
import socket
import sys

sock_path, agent_id, ok_path, over_path = sys.argv[1:5]

def rpc(method, params):
    c = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    c.connect(sock_path)
    req = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    c.sendall(json.dumps(req).encode())
    c.shutdown(socket.SHUT_WR)
    payload = c.recv(1 << 20)
    c.close()
    return payload

ok_payload = rpc("RecordUsage", {
    "agent_id": agent_id,
    "model_name": "claude-4-sonnet",
    "input_tokens": 60,
    "output_tokens": 30,
    "cost_usd": 0.15,
})
open(ok_path, "wb").write(ok_payload)

over_payload = rpc("RecordUsage", {
    "agent_id": agent_id,
    "model_name": "claude-4-sonnet",
    "input_tokens": 20,
    "output_tokens": 5,
    "cost_usd": 0.05,
})
open(over_path, "wb").write(over_payload)
PY

"$AGENTCTL_BIN" --socket-path "$SOCKET_PATH" usage "$AGENT_ID" --json >"$USAGE_FINAL_JSON" \
    || fail "final usage query failed"
normalize_json_file "$USAGE_FINAL_JSON" || fail "failed to normalize final usage json"

python3 - "$RPC_HEALTH_JSON" "$CREATE_JSON" "$LIST_JSON" "$USAGE_INITIAL_JSON" "$RECORD_OK_JSON" "$RECORD_OVER_JSON" "$USAGE_FINAL_JSON" "$HAPPY_EVIDENCE" "$STARTUP_LATENCY_MS" <<'PY'
import json
import sys

health_p, create_p, list_p, initial_p, ok_p, over_p, final_p, out_p, startup_latency = sys.argv[1:10]

health = json.load(open(health_p))
created = json.load(open(create_p))
listed = json.load(open(list_p))
initial = json.load(open(initial_p))
record_ok = json.load(open(ok_p))
record_over = json.load(open(over_p))
final = json.load(open(final_p))

assert health["status"] in ("ok", "degraded"), "unexpected health status"
assert created["agent"]["status"] == "ready", "created agent is not ready"
assert isinstance(listed.get("agents"), list), "list response missing agents"
assert any(a.get("id") == created["agent"]["id"] for a in listed["agents"]), "created agent not in list"
assert initial["total_tokens"] == 0, "initial total tokens must be 0"
assert "error" not in record_ok or record_ok["error"] is None, "first RecordUsage should succeed"
assert record_over.get("error", {}).get("code") == -32015, "expected llm.quota_exceeded error code -32015"
assert final["input_tokens"] == 60, "final input tokens mismatch"
assert final["output_tokens"] == 30, "final output tokens mismatch"
assert final["total_tokens"] == 90, "final total tokens mismatch"

result = {
    "phase": "A",
    "status": "pass",
    "startup_latency_ms": int(startup_latency),
    "create_success_rate": 1.0,
    "request_success_rate": 1.0,
    "usage_accuracy": {
        "expected_total_tokens": 90,
        "actual_total_tokens": final["total_tokens"],
    },
    "quota_gate": {
        "blocked": True,
        "error_code": record_over["error"]["code"],
        "error_message": record_over["error"]["message"],
    },
    "agent_id": created["agent"]["id"],
}

with open(out_p, "w", encoding="utf-8") as f:
    json.dump(result, f, ensure_ascii=False, indent=2)
PY

log_info "Phase A gate passed"
log_info "Evidence: $HAPPY_EVIDENCE"
