#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUTPUT_DIR="$REPO_ROOT/data/firecracker"
RELEASE_TAG="v1.14.2"
ARCH="$(uname -m)"
SUCCESS_EVIDENCE="$REPO_ROOT/.sisyphus/evidence/task-20-real-firecracker.txt"
ERROR_EVIDENCE="$REPO_ROOT/.sisyphus/evidence/task-20-vm-timeout.txt"

usage() {
    cat <<'EOF'
Usage: bash scripts/firecracker/fetch-assets.sh [options]

Download and stage Firecracker binary + compatible kernel into data/firecracker.

Options:
  --release <tag>            Firecracker release tag (default: v1.14.2)
  --arch <arch>              Target arch for binary/kernel (default: uname -m)
  --output-dir <path>        Output directory (default: data/firecracker)
  --success-evidence <path>  Success evidence path
  --error-evidence <path>    Failure evidence path
  -h, --help                 Show help
EOF
}

fail() {
    local reason="$1"
    mkdir -p "$(dirname "$ERROR_EVIDENCE")"
    {
        echo "task=20"
        echo "status=failed"
        echo "step=fetch-firecracker-assets"
        echo "reason=$reason"
        echo "release=$RELEASE_TAG"
        echo "arch=$ARCH"
    } >"$ERROR_EVIDENCE"
    printf '[ERROR] %s\n' "$reason" >&2
    exit 1
}

require_cmd() {
    local cmd="$1"
    command -v "$cmd" >/dev/null 2>&1 || fail "required command not found: $cmd"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release)
            RELEASE_TAG="${2:-}"
            [[ -n "$RELEASE_TAG" ]] || fail "--release requires value"
            shift 2
            ;;
        --arch)
            ARCH="${2:-}"
            [[ -n "$ARCH" ]] || fail "--arch requires value"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="${2:-}"
            [[ -n "$OUTPUT_DIR" ]] || fail "--output-dir requires value"
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

for cmd in gh python3 tar sha256sum; do
    require_cmd "$cmd"
done

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

mkdir -p "$OUTPUT_DIR" "$(dirname "$SUCCESS_EVIDENCE")"

ASSET_NAME="firecracker-${RELEASE_TAG}-${ARCH}.tgz"
gh release download "$RELEASE_TAG" -R firecracker-microvm/firecracker -p "$ASSET_NAME" -D "$TMP_DIR"
tar -xzf "$TMP_DIR/$ASSET_NAME" -C "$TMP_DIR"

RELEASE_DIR="$TMP_DIR/release-${RELEASE_TAG}-${ARCH}"
FIRECRACKER_BIN_SRC="$RELEASE_DIR/firecracker-${RELEASE_TAG}-${ARCH}"
[[ -f "$FIRECRACKER_BIN_SRC" ]] || fail "downloaded firecracker binary missing: $FIRECRACKER_BIN_SRC"

KERNEL_KEY="$(python3 - "$RELEASE_TAG" "$ARCH" <<'PY'
import re
import sys
import urllib.request

release_tag = sys.argv[1]
arch = sys.argv[2]
ci_version = '.'.join(release_tag.lstrip('v').split('.')[:2])
url = f'https://s3.amazonaws.com/spec.ccfc.min/?prefix=firecracker-ci/v{ci_version}/{arch}/vmlinux-&list-type=2'
text = urllib.request.urlopen(url, timeout=30).read().decode('utf-8', 'replace')
keys = re.findall(r'<Key>(firecracker-ci/.+?/vmlinux-[^<]+)</Key>', text)
kernel_keys = [key for key in keys if not key.endswith('.config') and '-no-acpi' not in key]
if not kernel_keys:
    raise SystemExit('')
print(sorted(kernel_keys)[-1])
PY
)"
[[ -n "$KERNEL_KEY" ]] || fail "unable to resolve Firecracker kernel asset"

KERNEL_URL="https://s3.amazonaws.com/spec.ccfc.min/$KERNEL_KEY"
KERNEL_DST="$OUTPUT_DIR/vmlinux.bin"
python3 - "$KERNEL_URL" "$KERNEL_DST" <<'PY'
import sys
import urllib.request

urllib.request.urlretrieve(sys.argv[1], sys.argv[2])
PY

FIRECRACKER_DST="$OUTPUT_DIR/firecracker"
cp -f "$FIRECRACKER_BIN_SRC" "$FIRECRACKER_DST"
chmod +x "$FIRECRACKER_DST"

FIRECRACKER_SHA="$(sha256sum "$FIRECRACKER_DST" | awk '{print $1}')"
KERNEL_SHA="$(sha256sum "$KERNEL_DST" | awk '{print $1}')"

{
    echo "task=20"
    echo "status=passed"
    echo "step=fetch-firecracker-assets"
    echo "release=$RELEASE_TAG"
    echo "arch=$ARCH"
    echo "binary=${FIRECRACKER_DST#$REPO_ROOT/}"
    echo "binary_sha256=$FIRECRACKER_SHA"
    echo "kernel=${KERNEL_DST#$REPO_ROOT/}"
    echo "kernel_sha256=$KERNEL_SHA"
    echo "kernel_asset=$KERNEL_KEY"
} >"$SUCCESS_EVIDENCE"

printf '[INFO] Firecracker binary staged at %s\n' "$FIRECRACKER_DST"
printf '[INFO] Kernel staged at %s\n' "$KERNEL_DST"
printf '[INFO] Success evidence: %s\n' "$SUCCESS_EVIDENCE"
