#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
HAPPY_EVIDENCE="$EVIDENCE_DIR/task-20-rc-happy.txt"
ERROR_EVIDENCE="$EVIDENCE_DIR/task-20-rc-error.txt"

usage() {
    cat <<'EOF'
Usage: bash scripts/release/rc-gate.sh [--local] [--pr-number <number>]

Run release-candidate hardening checks.

Options:
  --local               Run without GitHub PR checks (recommended for pre-commit)
  --pr-number <number>  PR number for required check validation via gh pr checks
  -h, --help            Show help
EOF
}

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "missing command: $cmd" >&2
        exit 1
    fi
}

LOCAL_MODE=false
PR_NUMBER=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --local)
            LOCAL_MODE=true
            shift
            ;;
        --pr-number)
            PR_NUMBER="${2:-}"
            if [[ -z "$PR_NUMBER" ]]; then
                echo "--pr-number requires a value" >&2
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

for cmd in bash cargo git python3; do
    require_cmd "$cmd"
done

if [[ "$LOCAL_MODE" != "true" ]]; then
    require_cmd gh
fi

SECRET_REPORT_TMP="$(mktemp)"

cleanup() {
    rm -f "$SECRET_REPORT_TMP"
}

fail() {
    local reason="$1"
    {
        echo "task_20_rc=failed"
        echo "reason=$reason"
        echo "local_mode=$LOCAL_MODE"
        if [[ -s "$SECRET_REPORT_TMP" ]]; then
            echo "--- secret_scan_report ---"
            cat "$SECRET_REPORT_TMP" || true
        fi
    } >"$ERROR_EVIDENCE"
    echo "$reason" >&2
    exit 1
}

trap cleanup EXIT

cd "$REPO_ROOT"

bash scripts/gate-check.sh --local || fail "gate-check failed"
bash scripts/gates/phase-a-gate.sh || fail "phase-a gate failed"
bash scripts/gates/phase-bc-gate.sh --local || fail "phase-bc gate failed"
bash scripts/demo/e2e-demo.sh --dry-run || fail "phase-d demo dry-run failed"

cargo check --workspace >/dev/null || fail "cargo check failed"
cargo test -p agentd-daemon create_agent_returns_a2a_card_path_and_persists_card_file >/dev/null || fail "daemon card test failed"

python3 - "$REPO_ROOT" "$SECRET_REPORT_TMP" <<'PY' || fail "secret scanning detected potential leaks"
import pathlib
import re
import subprocess
import sys

repo_root = pathlib.Path(sys.argv[1])
report_path = pathlib.Path(sys.argv[2])

patterns = {
    "github_pat": re.compile(r"github_pat_[A-Za-z0-9_]{20,}"),
    "ghp_token": re.compile(r"\bghp_[A-Za-z0-9]{36,}\b"),
    "openai_key": re.compile(r"\bsk-[A-Za-z0-9]{20,}\b"),
    "aws_access_key": re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    "private_key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
}

excluded_prefixes = (
    ".sisyphus/evidence/",
    "target/",
    "research/",
)

tracked = subprocess.check_output(["git", "ls-files"], cwd=repo_root, text=True).splitlines()
findings = []

for rel in tracked:
    if rel.startswith(excluded_prefixes):
        continue
    path = repo_root / rel
    try:
        text = path.read_text(encoding="utf-8")
    except Exception:
        continue
    for name, pattern in patterns.items():
        if pattern.search(text):
            findings.append({"file": rel, "pattern": name})

if findings:
    report_path.write_text("\n".join(f"{i['file']}:{i['pattern']}" for i in findings), encoding="utf-8")
    raise SystemExit(1)

report_path.write_text("no_findings", encoding="utf-8")
PY

dependency_scan="skipped"
if cargo audit --version >/dev/null 2>&1; then
    cargo audit -q || fail "cargo audit reported vulnerabilities"
    dependency_scan="cargo_audit_pass"
elif [[ "$LOCAL_MODE" != "true" ]]; then
    fail "cargo audit is required in non-local mode"
fi

if [[ "$LOCAL_MODE" != "true" ]]; then
    if [[ -z "$PR_NUMBER" ]]; then
        fail "--pr-number is required in non-local mode"
    fi
    gh pr checks "$PR_NUMBER" --required >/dev/null || fail "required PR checks failed"
fi

{
    echo "task_20_rc=passed"
    echo "local_mode=$LOCAL_MODE"
    echo "gate_check=passed"
    echo "phase_a_gate=passed"
    echo "phase_bc_gate=passed"
    echo "phase_d_demo_smoke=passed"
    echo "cargo_check=passed"
    echo "daemon_card_test=passed"
    echo "secret_scan=passed"
    echo "dependency_scan=$dependency_scan"
    if [[ "$LOCAL_MODE" != "true" ]]; then
        echo "required_pr_checks=passed"
        echo "pr_number=$PR_NUMBER"
    fi
} >"$HAPPY_EVIDENCE"

echo "task-20 rc gate passed"
