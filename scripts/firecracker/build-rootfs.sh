#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ROOTFS_ROOT="$REPO_ROOT/images/agent-rootfs"
EVIDENCE_DIR="$REPO_ROOT/.sisyphus/evidence"
RUNTIME_ROOT_DEFAULT="$REPO_ROOT/data/firecracker"

VERSION_FILE="$ROOTFS_ROOT/VERSION"
DEFAULT_VERSION="0.1.0"
BASE_VERSION="$DEFAULT_VERSION"
if [[ -f "$VERSION_FILE" ]]; then
    BASE_VERSION="$(tr -d '[:space:]' <"$VERSION_FILE")"
fi

TAG_DEFAULT="$BASE_VERSION"
OUTPUT_ROOT_DEFAULT="$ROOTFS_ROOT/out"
MANIFEST_PATH_DEFAULT="$ROOTFS_ROOT/rootfs-manifest.json"

TAG="$TAG_DEFAULT"
OUTPUT_ROOT="$OUTPUT_ROOT_DEFAULT"
MANIFEST_PATH="$MANIFEST_PATH_DEFAULT"
RUNTIME_ROOT="$RUNTIME_ROOT_DEFAULT"
SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-}"
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
Usage: bash scripts/firecracker/build-rootfs.sh [options]

Build minimal Firecracker rootfs with Python runtime and agent-lite payload.

Options:
  --tag <value>              Rootfs image tag (default: images/agent-rootfs/VERSION)
  --output-root <path>       Output directory root (default: images/agent-rootfs/out)
  --manifest <path>          Manifest output path (default: images/agent-rootfs/rootfs-manifest.json)
  --runtime-root <path>      Runtime artifact directory (default: data/firecracker)
  --source-date-epoch <sec>  Deterministic build timestamp
  --success-evidence <path>  Success evidence file path
  --error-evidence <path>    Failure evidence file path
  -h, --help                 Show this help

Determinism notes:
  - If SOURCE_DATE_EPOCH is not set, script uses latest git commit timestamp.
  - rootfs tarball uses sorted paths + fixed mtime + numeric owner/group.
EOF
}

require_cmd() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        log_error "Required command not found: $cmd"
        exit 1
    fi
}

fail() {
    local reason="$1"
    mkdir -p "$EVIDENCE_DIR"
    {
        echo "task=19"
        echo "status=failed"
        echo "step=build-rootfs"
        echo "reason=$reason"
        echo "tag=$TAG"
    } >"$ERROR_EVIDENCE"
    log_error "$reason"
    exit 1
}

copy_file_with_parents() {
    local src="$1"
    local dst_root="$2"

    if [[ ! -f "$src" ]]; then
        fail "dependency file missing: $src"
    fi

    local rel="${src#/}"
    local dst="$dst_root/$rel"
    mkdir -p "$(dirname "$dst")"
    cp -L --preserve=mode,timestamps "$src" "$dst"
}

copy_python_runtime_deps() {
    local python_bin="$1"
    local dst_root="$2"

    local loader=""
    while IFS= read -r line; do
        if [[ "$line" == *"=>"* ]]; then
            local dep="${line#*=> }"
            dep="${dep%% (*}"
            if [[ -f "$dep" ]]; then
                copy_file_with_parents "$dep" "$dst_root"
            fi
        elif [[ "$line" == /*" ("* ]]; then
            local dep2="${line%% (*}"
            if [[ -f "$dep2" ]]; then
                copy_file_with_parents "$dep2" "$dst_root"
            fi
        fi

        if [[ "$line" == *"ld-linux"* || "$line" == *"ld-musl"* ]]; then
            loader="${line##*=> }"
            loader="${loader%% (*}"
            if [[ ! -f "$loader" ]]; then
                loader="${line%% (*}"
            fi
        fi
    done < <(ldd "$python_bin")

    if [[ -n "$loader" && -f "$loader" ]]; then
        copy_file_with_parents "$loader" "$dst_root"
    fi
}

calculate_rootfs_image_size_bytes() {
    local rootfs_dir="$1"
    local rootfs_bytes
    rootfs_bytes="$(du -sb "$rootfs_dir" | cut -f1)"
    local minimum_bytes=$((128 * 1024 * 1024))
    local padding_bytes=$((64 * 1024 * 1024))
    local total_bytes=$((rootfs_bytes + padding_bytes))
    if (( total_bytes < minimum_bytes )); then
        total_bytes=$minimum_bytes
    fi
    printf '%s\n' "$total_bytes"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --tag)
            TAG="${2:-}"
            [[ -n "$TAG" ]] || fail "--tag requires value"
            shift 2
            ;;
        --output-root)
            OUTPUT_ROOT="${2:-}"
            [[ -n "$OUTPUT_ROOT" ]] || fail "--output-root requires value"
            shift 2
            ;;
        --manifest)
            MANIFEST_PATH="${2:-}"
            [[ -n "$MANIFEST_PATH" ]] || fail "--manifest requires value"
            shift 2
            ;;
        --runtime-root)
            RUNTIME_ROOT="${2:-}"
            [[ -n "$RUNTIME_ROOT" ]] || fail "--runtime-root requires value"
            shift 2
            ;;
        --source-date-epoch)
            SOURCE_DATE_EPOCH="${2:-}"
            [[ -n "$SOURCE_DATE_EPOCH" ]] || fail "--source-date-epoch requires value"
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

for cmd in python3 ldd tar gzip sha256sum mkfs.ext4 truncate; do
    require_cmd "$cmd"
done

if [[ -z "$SOURCE_DATE_EPOCH" ]]; then
    if command -v git >/dev/null 2>&1; then
        SOURCE_DATE_EPOCH="$(git -C "$REPO_ROOT" log -1 --format=%ct 2>/dev/null || true)"
    fi
fi
if [[ -z "$SOURCE_DATE_EPOCH" ]]; then
    SOURCE_DATE_EPOCH="$(date +%s)"
fi

if [[ ! "$SOURCE_DATE_EPOCH" =~ ^[0-9]+$ ]]; then
    fail "SOURCE_DATE_EPOCH must be unix seconds"
fi

PYTHON_BIN="$(command -v python3)"
if [[ -z "$PYTHON_BIN" ]]; then
    fail "python3 not found"
fi

PYTHON_REAL_BIN="$(readlink -f "$PYTHON_BIN")"
if [[ ! -f "$PYTHON_REAL_BIN" ]]; then
    fail "python3 resolved path missing: $PYTHON_REAL_BIN"
fi

AGENT_LITE_SRC="$REPO_ROOT/python/agentd-agent-lite/src/agentd_agent_lite"
if [[ ! -d "$AGENT_LITE_SRC" ]]; then
    fail "agent-lite source not found: $AGENT_LITE_SRC"
fi

AGENT_LITE_VERSION="$(python3 - "$REPO_ROOT/python/agentd-agent-lite/pyproject.toml" <<'PY'
from pathlib import Path
import sys

pyproject = Path(sys.argv[1])
version = "unknown"
for line in pyproject.read_text(encoding="utf-8").splitlines():
    if line.strip().startswith("version"):
        version = line.split("=", 1)[1].strip().strip('"')
        break
print(version)
PY
)"

ARTIFACT_DIR="$OUTPUT_ROOT/$TAG"
ROOTFS_DIR="$ARTIFACT_DIR/rootfs"
TARBALL="$ARTIFACT_DIR/rootfs.tar"
TARBALL_GZ="$ARTIFACT_DIR/rootfs.tar.gz"
CHECKSUM_FILE="$ARTIFACT_DIR/rootfs.tar.gz.sha256"
ROOTFS_EXT4="$ARTIFACT_DIR/rootfs.ext4"
ROOTFS_EXT4_CHECKSUM_FILE="$ARTIFACT_DIR/rootfs.ext4.sha256"
RUNTIME_ROOTFS="$RUNTIME_ROOT/rootfs.ext4"
RUNTIME_MANIFEST="$RUNTIME_ROOT/rootfs-manifest.json"
TMP_DIR="$(mktemp -d)"

cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

mkdir -p "$ROOTFS_ROOT" "$EVIDENCE_DIR" "$ARTIFACT_DIR" "$RUNTIME_ROOT"
rm -rf "$ROOTFS_DIR"

log_info "Building rootfs tag=$TAG"

mkdir -p \
    "$ROOTFS_DIR/bin" \
    "$ROOTFS_DIR/dev" \
    "$ROOTFS_DIR/etc" \
    "$ROOTFS_DIR/lib" \
    "$ROOTFS_DIR/lib64" \
    "$ROOTFS_DIR/proc" \
    "$ROOTFS_DIR/root" \
    "$ROOTFS_DIR/run" \
    "$ROOTFS_DIR/sbin" \
    "$ROOTFS_DIR/sys" \
    "$ROOTFS_DIR/tmp" \
    "$ROOTFS_DIR/usr/bin" \
    "$ROOTFS_DIR/usr/local/bin" \
    "$ROOTFS_DIR/usr/lib" \
    "$ROOTFS_DIR/var/log" \
    "$ROOTFS_DIR/opt/agent-lite/src"

cp -L --preserve=mode,timestamps "$PYTHON_REAL_BIN" "$ROOTFS_DIR/usr/bin/python3"
ln -sf python3 "$ROOTFS_DIR/usr/bin/python"
copy_python_runtime_deps "$PYTHON_REAL_BIN" "$ROOTFS_DIR"

cp -a "$AGENT_LITE_SRC" "$ROOTFS_DIR/opt/agent-lite/src/"
cp -a "$REPO_ROOT/python/agentd-agent-lite/pyproject.toml" "$ROOTFS_DIR/opt/agent-lite/pyproject.toml"

cat >"$ROOTFS_DIR/usr/local/bin/agentd-agent-lite" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
export PYTHONPATH="/opt/agent-lite/src${PYTHONPATH:+:${PYTHONPATH}}"
exec /usr/bin/python3 -m agentd_agent_lite.cli "$@"
EOF
chmod +x "$ROOTFS_DIR/usr/local/bin/agentd-agent-lite"

cat >"$ROOTFS_DIR/etc/agentd-rootfs-release" <<EOF
AGENTD_ROOTFS_TAG=$TAG
AGENTD_ROOTFS_VERSION=$BASE_VERSION
AGENTD_AGENT_LITE_VERSION=$AGENT_LITE_VERSION
SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH
EOF

cat >"$ROOTFS_DIR/etc/agentd-rootfs-manifest.json" <<EOF
{
  "schema_version": 1,
  "tag": "$TAG",
  "base_version": "$BASE_VERSION",
  "source_date_epoch": $SOURCE_DATE_EPOCH,
  "python": {
    "binary": "/usr/bin/python3",
    "host_source": "$PYTHON_REAL_BIN"
  },
  "agent_lite": {
    "version": "$AGENT_LITE_VERSION",
    "entrypoint": "/usr/local/bin/agentd-agent-lite",
    "module_path": "/opt/agent-lite/src/agentd_agent_lite"
  }
}
EOF

tar --sort=name --mtime="@$SOURCE_DATE_EPOCH" --owner=0 --group=0 --numeric-owner -C "$ROOTFS_DIR" -cf "$TARBALL" .
gzip -n -f "$TARBALL"
sha256sum "$TARBALL_GZ" >"$CHECKSUM_FILE"

ROOTFS_IMAGE_SIZE_BYTES="$(calculate_rootfs_image_size_bytes "$ROOTFS_DIR")"
rm -f "$ROOTFS_EXT4"
truncate -s "$ROOTFS_IMAGE_SIZE_BYTES" "$ROOTFS_EXT4"
mkfs.ext4 -q -F -d "$ROOTFS_DIR" "$ROOTFS_EXT4"
sha256sum "$ROOTFS_EXT4" >"$ROOTFS_EXT4_CHECKSUM_FILE"

cp -f "$ROOTFS_EXT4" "$RUNTIME_ROOTFS"

ROOTFS_SHA256="$(awk '{print $1}' "$CHECKSUM_FILE")"
ROOTFS_EXT4_SHA256="$(awk '{print $1}' "$ROOTFS_EXT4_CHECKSUM_FILE")"

cat >"$MANIFEST_PATH" <<EOF
{
  "schema_version": 1,
  "image": {
    "tag": "$TAG",
    "base_version": "$BASE_VERSION",
    "source_date_epoch": $SOURCE_DATE_EPOCH
  },
  "artifact": {
    "rootfs_dir": "${ROOTFS_DIR#$REPO_ROOT/}",
    "archive": "${TARBALL_GZ#$REPO_ROOT/}",
    "sha256": "$ROOTFS_SHA256",
    "checksum_file": "${CHECKSUM_FILE#$REPO_ROOT/}",
    "ext4_image": "${ROOTFS_EXT4#$REPO_ROOT/}",
    "ext4_sha256": "$ROOTFS_EXT4_SHA256",
    "ext4_checksum_file": "${ROOTFS_EXT4_CHECKSUM_FILE#$REPO_ROOT/}"
  },
  "content": {
    "python_binary": "/usr/bin/python3",
    "agent_lite_entrypoint": "/usr/local/bin/agentd-agent-lite",
    "agent_lite_module": "/opt/agent-lite/src/agentd_agent_lite/cli.py"
  },
  "runtime": {
    "root": "${RUNTIME_ROOT#$REPO_ROOT/}",
    "rootfs_image": "${RUNTIME_ROOTFS#$REPO_ROOT/}",
    "manifest": "${RUNTIME_MANIFEST#$REPO_ROOT/}"
  }
}
EOF

cp -f "$MANIFEST_PATH" "$RUNTIME_MANIFEST"

{
    echo "task=19"
    echo "status=passed"
    echo "step=build-rootfs"
    echo "tag=$TAG"
    echo "rootfs_dir=${ROOTFS_DIR#$REPO_ROOT/}"
    echo "archive=${TARBALL_GZ#$REPO_ROOT/}"
    echo "sha256=$ROOTFS_SHA256"
    echo "ext4_image=${ROOTFS_EXT4#$REPO_ROOT/}"
    echo "ext4_sha256=$ROOTFS_EXT4_SHA256"
    echo "runtime_rootfs=${RUNTIME_ROOTFS#$REPO_ROOT/}"
    echo "manifest=${MANIFEST_PATH#$REPO_ROOT/}"
} >"$SUCCESS_EVIDENCE"

log_info "Rootfs build complete"
log_info "Manifest: $MANIFEST_PATH"
log_info "Archive: $TARBALL_GZ"
log_info "Ext4 image: $ROOTFS_EXT4"
log_info "Runtime rootfs: $RUNTIME_ROOTFS"
log_info "Success evidence: $SUCCESS_EVIDENCE"
