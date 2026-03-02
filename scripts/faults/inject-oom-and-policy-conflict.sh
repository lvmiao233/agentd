#!/usr/bin/env bash

set -euo pipefail

EVIDENCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/.sisyphus/evidence"
FAULT_MARKER="/tmp/agentd-phasebc-fault-marker"

mkdir -p "$EVIDENCE_DIR"

cat >"$FAULT_MARKER" <<'EOF'
faults=oom,policy_conflict
injected=true
EOF

cat >"$EVIDENCE_DIR/task-17-bc-error.txt" <<EOF
phase_bc_fault_injection=applied
marker=$FAULT_MARKER
faults=oom,policy_conflict
EOF

echo "Injected Phase B/C fault marker: $FAULT_MARKER"
