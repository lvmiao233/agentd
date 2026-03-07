#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
DEFAULT_ERROR_EVIDENCE="$EVIDENCE_DIR/preflight-real-oneapi-error.txt"

usage() {
    cat <<'EOF'
Usage: bash scripts/gates/preflight-real-oneapi.sh [options]

Validate real One-API readiness for gate execution.

Options:
  --base-url <url>         One-API base URL (default: http://127.0.0.1:3000)
  --token <token>          API token (default: ONE_API_TOKEN env)
  --timeout <seconds>      Curl max-time per request (default: 5)
  --error-evidence <path>  Failure evidence output path
  --dry-run                Validate script wiring without network calls
  -h, --help               Show help

Machine-readable markers:
  HEALTH=true|false
  MODELS_CHECKED=true|false
  ENV_READY=true|false
  REASON_CODE=<code>
EOF
}

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "missing command: $cmd" >&2
        exit 1
    fi
}

BASE_URL="${ONE_API_BASE_URL:-http://127.0.0.1:3000}"
TOKEN="${ONE_API_TOKEN:-}"
TIMEOUT_SECS=5
ERROR_EVIDENCE="$DEFAULT_ERROR_EVIDENCE"
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --base-url)
            BASE_URL="${2:-}"
            [[ -n "$BASE_URL" ]] || {
                echo "--base-url requires a value" >&2
                exit 1
            }
            shift 2
            ;;
        --token)
            TOKEN="${2:-}"
            [[ -n "$TOKEN" ]] || {
                echo "--token requires a value" >&2
                exit 1
            }
            shift 2
            ;;
        --timeout)
            TIMEOUT_SECS="${2:-}"
            [[ "$TIMEOUT_SECS" =~ ^[0-9]+$ ]] || {
                echo "--timeout must be an integer" >&2
                exit 1
            }
            shift 2
            ;;
        --error-evidence)
            ERROR_EVIDENCE="${2:-}"
            [[ -n "$ERROR_EVIDENCE" ]] || {
                echo "--error-evidence requires a value" >&2
                exit 1
            }
            shift 2
            ;;
        --dry-run)
            DRY_RUN=true
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

for cmd in curl python3; do
    require_cmd "$cmd"
done

mkdir -p "$EVIDENCE_DIR"

HEALTH=false
MODELS_CHECKED=false
ENV_READY=false
REASON_CODE="UNKNOWN"
HEALTH_ENDPOINT=""
MODELS_VISIBLE_COUNT=0

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

health_body="$TMP_DIR/health.body"
status_body="$TMP_DIR/status.body"
models_body="$TMP_DIR/models.body"

is_ready_json_body() {
    local body_path="$1"
    python3 - "$body_path" <<'PY'
import json
import pathlib
import sys

text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8", errors="replace").strip()
if not text or text.startswith("<"):
    raise SystemExit(1)

payload = json.loads(text)
if isinstance(payload, dict) and "error" in payload:
    raise SystemExit(1)
PY
}

write_failure_evidence() {
    mkdir -p "$(dirname "$ERROR_EVIDENCE")"
    {
        echo "preflight_real_oneapi=failed"
        echo "reason=$REASON_CODE"
        echo "base_url=$BASE_URL"
        echo "health_endpoint=$HEALTH_ENDPOINT"
        echo "health=$HEALTH"
        echo "models_checked=$MODELS_CHECKED"
        echo "env_ready=$ENV_READY"
        echo "models_visible_count=$MODELS_VISIBLE_COUNT"
        if [[ -f "$health_body" ]]; then
            echo "--- health_body ---"
            cat "$health_body" || true
        fi
        if [[ -f "$status_body" ]]; then
            echo "--- status_body ---"
            cat "$status_body" || true
        fi
        if [[ -f "$models_body" ]]; then
            echo "--- models_body ---"
            cat "$models_body" || true
        fi
    } >"$ERROR_EVIDENCE"
}

emit_markers() {
    echo "HEALTH=$HEALTH"
    echo "MODELS_CHECKED=$MODELS_CHECKED"
    echo "ENV_READY=$ENV_READY"
    echo "REASON_CODE=$REASON_CODE"
    if [[ -n "$HEALTH_ENDPOINT" ]]; then
        echo "HEALTH_ENDPOINT=$HEALTH_ENDPOINT"
    fi
    if [[ "$MODELS_CHECKED" == "true" ]]; then
        echo "MODELS_VISIBLE_COUNT=$MODELS_VISIBLE_COUNT"
    fi
}

fail() {
    local reason_code="$1"
    REASON_CODE="$reason_code"
    write_failure_evidence
    emit_markers
    exit 1
}

if [[ "$DRY_RUN" == "true" ]]; then
    REASON_CODE="DRY_RUN"
    emit_markers
    exit 0
fi

trimmed_base_url="${BASE_URL%/}"

status_http_code="$(curl --noproxy '*' -sS --max-time "$TIMEOUT_SECS" -o "$status_body" -w '%{http_code}' "${trimmed_base_url}/api/status" 2>/dev/null || echo 000)"
if [[ "$status_http_code" == "200" ]] && is_ready_json_body "$status_body"; then
    HEALTH=true
    HEALTH_ENDPOINT="/api/status"
else
    health_http_code="$(curl --noproxy '*' -sS --max-time "$TIMEOUT_SECS" -o "$health_body" -w '%{http_code}' "${trimmed_base_url}/health" 2>/dev/null || echo 000)"
    if [[ "$health_http_code" == "200" ]] && is_ready_json_body "$health_body"; then
        HEALTH=true
        HEALTH_ENDPOINT="/health"
    fi
fi

if [[ "$HEALTH" != "true" ]]; then
    fail "ONE_API_HEALTH_UNREACHABLE"
fi

if [[ -z "$TOKEN" ]]; then
    fail "ONE_API_TOKEN_MISSING"
fi

models_http_code="$(curl --noproxy '*' -sS --max-time "$TIMEOUT_SECS" -o "$models_body" -w '%{http_code}' -H "Authorization: Bearer $TOKEN" "${trimmed_base_url}/v1/models" 2>/dev/null || echo 000)"
if [[ "$models_http_code" != "200" ]]; then
    fail "ONE_API_MODELS_REQUEST_FAILED"
fi

if ! MODELS_VISIBLE_COUNT="$(python3 - "$models_body" <<'PY'
import json
import sys

payload = json.load(open(sys.argv[1], 'r', encoding='utf-8'))
data = payload.get('data')
if not isinstance(data, list):
    raise SystemExit(1)
print(len(data))
PY
)"; then
    fail "ONE_API_MODELS_INVALID_JSON"
fi

MODELS_CHECKED=true
if [[ "$MODELS_VISIBLE_COUNT" -le 0 ]]; then
    fail "ONE_API_MODELS_EMPTY"
fi

ENV_READY=true
REASON_CODE="READY"
emit_markers
