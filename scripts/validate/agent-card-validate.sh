#!/usr/bin/env bash

set -euo pipefail

usage() {
    cat <<'EOF'
Usage: bash scripts/validate/agent-card-validate.sh [--card-path <path>] [--json]

Validate minimal required fields for agentd A2A-compatible agent card.

Options:
  --card-path <path>   Path to agent.json. If omitted, auto-discover from data/agents/*/agent.json
  --json               Print machine-readable JSON summary
  -h, --help           Show help
EOF
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

CARD_PATH=""
AS_JSON=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --card-path)
            CARD_PATH="${2:-}"
            if [[ -z "$CARD_PATH" ]]; then
                echo "--card-path requires a value" >&2
                exit 1
            fi
            shift 2
            ;;
        --json)
            AS_JSON=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ -z "$CARD_PATH" ]]; then
    discovered="$(compgen -G "$REPO_ROOT/data/agents/*/agent.json" | head -n 1 || true)"
    if [[ -z "$discovered" ]]; then
        echo "No agent card found in data/agents/*/agent.json, please pass --card-path" >&2
        exit 1
    fi
    CARD_PATH="$discovered"
fi

if [[ ! -f "$CARD_PATH" ]]; then
    echo "agent card file not found: $CARD_PATH" >&2
    exit 1
fi

python3 - "$CARD_PATH" "$AS_JSON" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
as_json = sys.argv[2].lower() == "true"

payload = json.loads(path.read_text(encoding="utf-8"))

def dotted_get(obj, dotted):
    cur = obj
    for part in dotted.split("."):
        if not isinstance(cur, dict) or part not in cur:
            return None
        cur = cur[part]
    return cur

def is_non_empty_string(value):
    return isinstance(value, str) and value.strip() != ""

checks = {
    "agent_id": is_non_empty_string,
    "name": is_non_empty_string,
    "version": is_non_empty_string,
    "model": is_non_empty_string,
    "provider": is_non_empty_string,
    "capabilities.protocol": is_non_empty_string,
    "capabilities.tools.default_policy": is_non_empty_string,
    "capabilities.tools.allowed": lambda value: isinstance(value, list),
    "capabilities.tools.denied": lambda value: isinstance(value, list),
}

missing_or_invalid = []
for dotted, validator in checks.items():
    value = dotted_get(payload, dotted)
    if not validator(value):
        missing_or_invalid.append(dotted)

if missing_or_invalid:
    if as_json:
        print(
            json.dumps(
                {
                    "status": "invalid",
                    "card_path": str(path),
                    "missing_or_invalid_fields": missing_or_invalid,
                },
                ensure_ascii=False,
            )
        )
    else:
        print(f"agent card invalid: {path}", file=sys.stderr)
        print("missing_or_invalid_fields:", file=sys.stderr)
        for field in missing_or_invalid:
            print(f"- {field}", file=sys.stderr)
    raise SystemExit(1)

summary = {
    "status": "valid",
    "card_path": str(path),
    "agent_id": payload["agent_id"],
    "name": payload["name"],
    "model": payload["model"],
    "provider": payload["provider"],
}

if as_json:
    print(json.dumps(summary, ensure_ascii=False))
else:
    print(f"agent card valid: {path}")
    print(f"agent_id={summary['agent_id']}")
    print(f"name={summary['name']}")
    print(f"model={summary['model']}")
    print(f"provider={summary['provider']}")
PY
