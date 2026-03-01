# T1 Learnings: GitHub Repository Bootstrap Script

## Date
2026-03-02

## Task
Created `/home/vipa/agentd/scripts/bootstrap-repo.sh` for public GitHub repository bootstrap with fallback naming.

## Key Findings

### Script Features
1. Fallback naming mechanism tries `agentd`, `agentd-runtime`, `agentd-core` in order
2. Always creates PUBLIC repositories (never private) - enforced via `--public` flag
3. Verifies visibility using `gh repo view --json visibility,name`
4. Handles missing `gh` auth with clear error messages and non-zero exit
5. Includes dry-run mode (`--dry-run`) that prints commands without executing
6. Includes check mode (`--check[=REPO_NAME]`) to verify existing repo visibility

### Shell Script Patterns
- Used `set -euo pipefail` for strict error handling
- Used array for repository name fallback list
- Color output using ANSI escape codes for user-friendly CLI
- Proper argument parsing with case statement

### Verification
- Bash syntax validated with `bash -n`
- Help output tested with `--help`
- Dry-run mode tested with `--dry-run`
- Check mode tested with `--check`
- LSP diagnostics clean

### gh Authentication
- Script checks `gh auth status` before any operations
- Requires authentication to proceed
- Uses `gh api user --jq '.login'` to get authenticated user

## T1 Fixes Applied (2026-03-02)

### Issues Fixed
1. **Dry-run now shows ALL fallback attempts**: Previously only showed first attempt, now logs all three (agentd, agentd-runtime, agentd-core)
2. **Fixed set -e early exit**: Changed to `set -uo pipefail` (removed `set -e`) to allow functions to return non-zero without aborting
3. **Exact visibility format**: Now uses `gh repo view --json visibility,name` exactly as specified in plan
4. **Case-insensitive visibility**: Added `is_public()` function using `${visibility,,}` for case-insensitive comparison (handles PUBLIC vs public)
5. **Check mode specific repo**: Fixed message when checking specific repo to show correct repo name

### Key Code Changes
- `set -uo pipefail` instead of `set -euo pipefail`
- `get_repo_visibility()` now parses `--json visibility,name` output
- `is_public()` function for case-insensitive comparison
- Dry-run loop continues through all fallbacks with summary at end

## T1 Fix: Non-Git Directory Create Mode (2026-03-02)

### Issue
Create mode failed with: "current directory is not a git repository. Run git init to initialize it"

### Root Cause
`gh repo create --source=.` requires current directory to be a git repo.

### Fix Applied
- Added `is_git_repo()` helper function using `git rev-parse --git-dir`
- Modified `create_repo()` to conditionally include `--source=.` only when in a git repo
- Dry-run message now shows appropriate command based on git status

### Verification
- Current directory detected as NOT git repo
- Dry-run shows command without `--source=.`: `gh repo create agentd --public --description=...`
- Create mode should now succeed from non-git directories

---

## T2 Learnings: Rust + uv Toolchain Baseline

## Date
2026-03-02

## Task
Created Rust and Python (uv) toolchain baseline with no-Go guard script.

## Key Findings

### Files Created
1. `rust-toolchain.toml` - Pins Rust 1.85.0 with rustfmt, clippy components
2. `pyproject.toml` - Python uv workspace configuration (package = false for baseline)
3. `uv.lock` - Lockfile for reproducible Python environment
4. `scripts/no-go-guard.sh` - Guard script to detect Go toolchain/build markers

### Toolchain Installation
- Rust toolchain automatically installed via rust-toolchain.toml when cargo is invoked
- uv (Python package manager) installed via official installer script
- Both tools successfully installed and verified working

### uv Configuration Notes
- Set `package = false` in `[tool.uv]` because no Python source code exists yet
- Without this, hatchling build fails as it expects a package directory matching project name
- Lockfile created via `uv lock` command

### No-Go Guard Features
- Detects Go toolchain files (go.mod, go.sum, go.work)
- Scans for .go source files in forbidden directories (crates, src, cmd, internal, pkg)
- Checks for Go build markers in Cargo.toml
- Checks GitHub Actions workflows for go-version
- Allows .go files in research/ directory (for studying prior art)
- Returns exit code 1 on violations, 0 on pass

### Baseline Commands
```bash
# Python (uv)
uv sync --frozen          # Reproducible sync with locked dependencies
uv lock                   # Regenerate lockfile

# Rust (cargo)
cargo check --workspace   # Check all workspace crates (requires Cargo.toml - T4)
# Note: Will auto-install Rust 1.85.0 via rust-toolchain.toml

# No-Go guard
bash scripts/no-go-guard.sh
```

### Verification
- uv sync --frozen: PASS (exit 0)
- No-Go guard: PASS (exit 0, no Go files detected)
- cargo check: Expected to fail (no workspace exists yet - T4)
- rust-toolchain.toml: Correctly auto-installs Rust 1.85.0

---

## T2 Fixes Applied (2026-03-02)

### Issue 1: Cargo Command Not Found
**Problem**: `cargo: command not found` in verification
**Fix**: Created `.cargo-env` file to document PATH setup; rustup installed to `~/.cargo/bin/`
**Verification**: `cargo --version` now returns `cargo 1.85.0`

### Issue 2: No-Go Guard Path Matching Bug
**Problem**: Regex `^research/` didn't match `./research/` paths from find output
**Fix**: Changed regex to `^(\./)?research/` to handle both formats
**Verification**: Tested with .go files in research/ (pass) and src/ (fail correctly)

---

## T3 Learnings: CI 硬门禁骨架与证据工件规范

## Date
2026-03-02

## Task
Created CI hard gate skeleton and evidence artifact specification.

## Key Findings

### Files Created
1. `.github/workflows/gates.yml` - GitHub Actions workflow with 8 gate jobs
2. `scripts/gate-check.sh` - Local validation script for gate configuration
3. `.github/branch-protection.md` - Branch protection documentation

### CI Gate Structure
The workflow implements a 7-gate pipeline:
1. **preflight**: Baseline file validation (T1/T2)
2. **no-go-guard**: Go toolchain absence verification
3. **build-gate**: Cargo workspace build (T4 placeholder)
4. **test-gate**: Unit/integration tests (T4 placeholder)
5. **security-gate**: Security scanning (T4 placeholder)
6. **gate-syscall**: System call validation (T4 placeholder)
7. **gate-isolation**: cgroup/systemd isolation (Ubuntu 25.10)

### Ubuntu 25.10 Self-Hosted Runner
- Isolation gate uses `runs-on: self-hosted-ubuntu-25.10` label
- This is the only job requiring Ubuntu 25.10 specifically
- Checks cgroup v2 and systemd availability

### Evidence Artifacts
- Each job uploads evidence to `.sisyphus/evidence/{job-name}/`
- All evidence retained for 30 days
- Supports debugging and audit trails

### Local Validation
- `bash scripts/gate-check.sh --local` validates gate configuration
- Checks file presence, job names, runner labels, shell syntax
- Uses case-insensitive grep for robust pattern matching

## T3 Fixes Applied (2026-03-02)

### Issue 1: Case-Insensitive Pattern Matching
**Problem**: grep pattern "required-check" didn't match "Required Checks" in branch-protection.md
**Fix**: Added `-i` flag to grep in check_contains function for case-insensitive matching
**Verification**: gate-check.sh now passes all checks

### Issue 2: Initial Script Error
**Problem**: First run of gate-check.sh showed false failure on Required Check
**Fix**: Updated pattern from "required-check" to "Required"
**Verification**: All 8 grep-based checks pass

---

## T4 Learnings: Rust Workspace 与核心 Crates 骨架

## Date
2026-03-02

## Task
Created Rust workspace with 5 crates: agentd-core, agentd-daemon, agentd-protocol, agentd-store, agentctl

## Key Findings

### Files Created
1. `Cargo.toml` - Workspace root with resolver = "2" and 5 workspace members
2. `crates/agentd-core/` - Core types library (lib)
   - `Cargo.toml`
   - `src/lib.rs` - Module exports
   - `src/error.rs` - AgentError enum
   - `src/profile.rs` - AgentProfile, ModelConfig, PermissionConfig, BudgetConfig
   - `src/audit.rs` - AuditEvent, EventType, EventPayload, EventResult
3. `crates/agentd-daemon/` - Daemon binary
   - `Cargo.toml`
   - `src/main.rs` - Minimal main with clap + tokio
4. `crates/agentd-protocol/` - Protocol types library
   - `Cargo.toml`
   - `src/lib.rs` - Module exports
   - `src/v1.rs` - Request/Response types
5. `crates/agentd-store/` - Storage abstractions library
   - `Cargo.toml`
   - `src/lib.rs` - AgentStore and AuditStore traits
6. `crates/agentctl/` - CLI binary
   - `Cargo.toml`
   - `src/main.rs` - Minimal CLI with clap

### Workspace Structure
- Used workspace dependencies for DRY (Don't Repeat Yourself)
- Each crate uses `.workspace = true` for package metadata
- Shared dependencies defined once in root Cargo.toml

### Dependencies
- agentd-core: thiserror, serde, chrono, uuid, serde_json
- agentd-daemon: agentd-core, tokio, clap, tracing
- agentd-protocol: agentd-core, serde, uuid
- agentd-store: agentd-core, thiserror, async-trait, uuid
- agentctl: agentd-core, agentd-protocol, clap, reqwest, tokio, tracing

### Verification Commands
```bash
source .cargo-env && cargo metadata --no-deps   # Lists all 5 workspace members
source .cargo-env && cargo check --workspace    # Compiles all crates
```

### Build Results
- cargo metadata: PASS - All 5 crates listed
- cargo check: PASS - All crates compile successfully
- cargo fix: Applied 2 fixes for unused imports in agentd-protocol

---

## T4 Fixes Applied (2026-03-02)

### Issue 1: Missing uuid dependency
**Problem**: agentd-protocol and agentd-store used uuid::Uuid but uuid wasn't in their dependencies
**Fix**: Added `uuid.workspace = true` to both crate manifests
**Verification**: cargo check passes cleanly

### Issue 2: Unused import warnings
**Problem**: agentd-protocol/src/lib.rs had unused imports (AgentProfile, AgentError, Deserialize, Serialize)
**Fix**: Ran `cargo fix --lib -p agentd-protocol --allow-dirty` to auto-fix
**Verification**: cargo check passes with no warnings

---

## T5 Learnings: daemon 主进程骨架 + systemd notify + health

## Date
2026-03-02

## Task
Implemented minimal daemon lifecycle with health endpoint, systemd Type=notify readiness, and graceful shutdown.

## Key Findings

### Lifecycle Skeleton
1. Health server can be implemented without adding a heavy web framework by using `tokio::net::TcpListener` and minimal HTTP response handling.
2. `SIGINT` + `SIGTERM` handling with `tokio::select!` gives a single shutdown path and consistent logs.
3. A `watch` channel cleanly coordinates shutdown from signal handler to health server task.
4. `tokio::time::timeout(Duration::from_secs(5), task_join)` enforces bounded graceful shutdown behavior.

### systemd notify Integration
1. `NOTIFY_SOCKET` can be handled directly via `std::os::unix::net::UnixDatagram` without extra runtime coupling.
2. Abstract namespace sockets (`@...`) require `\0` prefix conversion before `connect`.
3. `READY=1` after listener startup aligns Type=notify startup semantics.
4. Sending `STOPPING=1` on shutdown improves observability in systemd environments.

### Config and Runtime Defaults
1. Added `configs/agentd.toml` minimal defaults:
   - `health_host = "127.0.0.1"`
   - `health_port = 7000`
   - `shutdown_timeout_secs = 5`
2. Daemon supports `--config`, `--health-host`, and `--health-port` for minimal runtime override.
3. Missing config file falls back to in-code defaults, keeping startup robust.

### Verification
- `source .cargo-env && cargo test -p agentd-daemon`: PASS
- `source .cargo-env && cargo check --workspace`: PASS
- `lsp_diagnostics` on `crates/agentd-daemon/src/main.rs`: clean
- `TODO/FIXME` scan on changed files: no matches

---

## T5 Fix Learnings: health Content-Length correction

## Date
2026-03-02

## Task
Fixed `/health` response framing bug where `Content-Length` header was 1 byte too large.

## Key Findings
1. `{"status":"ok"}` is 15 bytes, so declaring `Content-Length: 16` causes curl error 18 (`end of response with 1 bytes missing`).
2. Even a minimal raw-HTTP handler must keep header/body byte counts exact, or strict clients fail hard.
3. After correcting to `Content-Length: 15`, repeated health probes succeed reliably.

## Verification
- `source .cargo-env && cargo test -p agentd-daemon`: PASS
- Hands-on QA: daemon start + 10x curl health checks: PASS
- Hands-on QA: SIGTERM graceful shutdown: PASS (`elapsed_ms=7`, within 5s budget)
