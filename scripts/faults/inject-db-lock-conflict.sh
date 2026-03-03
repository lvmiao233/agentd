#!/usr/bin/env bash

set -euo pipefail

EVIDENCE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/.sisyphus/evidence"
FAULT_MARKER="/tmp/agentd-phasebc-fault-marker"

mkdir -p "$EVIDENCE_DIR"

existing_faults=""
if [[ -f "$FAULT_MARKER" ]]; then
    existing_faults="$(grep '^faults=' "$FAULT_MARKER" | cut -d= -f2- || true)"
fi

combined="${existing_faults},db_lock_conflict"
combined="${combined#,}"
combined="$(printf '%s' "$combined" | tr ',' '\n' | awk 'NF' | sort -u | paste -sd, -)"

cat >"$FAULT_MARKER" <<EOF
faults=$combined
injected=true
EOF

cat >"$EVIDENCE_DIR/task-17-bc-error.txt" <<EOF
phase_bc_fault_injection=applied
marker=$FAULT_MARKER
faults=$combined
EOF

echo "Injected DB-lock conflict marker: $FAULT_MARKER"
