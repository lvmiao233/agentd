# T1 Issues: GitHub Repository Bootstrap Script

## Date
2026-03-02

## Blockers Encountered
None - T1 completed without blockers.

## Potential Issues
1. gh CLI must be installed - script checks and exits gracefully if missing
2. gh must be authenticated - script verifies auth status before any operations
3. Repository name collisions - script handles by falling back to next name in list
4. Non-public existing repos - script warns if repo exists but is not public

## Notes
- No .github files created as bootstrap script handles all required functionality
- No README updates needed as script is self-documenting via --help

## T1 QA Fixes (2026-03-02)

### Original Issues (Fixed)
1. **Dry-run short-circuit**: Was exiting after first fallback attempt due to `set -e` - Fixed by removing `set -e` and adding explicit dry-run mode handling
2. **Check mode premature exit**: Was exiting on first non-match - Fixed by removing `set -e` so functions can return non-zero without aborting
3. **Wrong visibility format**: Was using `--json visibility` instead of `--json visibility,name` - Fixed to match plan exactly
4. **Visibility casing mismatch**: Could fail on "PUBLIC" from GH API - Fixed with case-insensitive comparison using `${visibility,,}`

## T1 Fix: Non-Git Create Mode (2026-03-02)

### Issue
Create mode failed: "current directory is not a git repository. Run git init to initialize it"

### Fix Applied
- Added `is_git_repo()` function to detect git repo presence
- Conditionally use `--source=.` only when in git repo
- Dry-run now shows appropriate command for current environment

---

## T2 Issues: Rust + uv Toolchain Baseline

## Date
2026-03-02

## Blockers Encountered
None - T2 completed without blockers.

## Potential Issues
1. **Rust workspace not yet created**: cargo check --workspace fails because no Cargo.toml exists - this is expected (T4 creates the workspace)
2. **Python package = false**: Required because no Python source code exists; may need adjustment when Python code is added
3. **Deprecated uv warning**: `tool.uv.dev-dependencies` is deprecated - recommend using `dependency-groups.dev` in future

## Notes
- Tools (rustup, uv) installed during T2 execution for verification
- rust-toolchain.toml automatically installs correct Rust version when cargo is invoked
- uv.lock generated successfully with no dependencies
- No-Go guard script allows .go files in research/ directory only

---

## T2 QA Fixes (2026-03-02)

### Original Issues (Fixed)
1. **Cargo command not found**: rustup installed to ~/.cargo/bin but not in default PATH - Fixed by creating `.cargo-env` for PATH setup
2. **No-Go guard path matching**: Regex `^research/` didn't match `./research/` from find output - Fixed by changing to `^(\./)?research/`

---

## T3 Issues: CI 硬门禁骨架与证据工件规范

## Date
2026-03-02

## Blockers Encountered
None - T3 completed without blockers.

## Potential Issues
1. **yamllint not available**: Workflow YAML syntax not validated locally (advisory only)
2. **Evidence directory not created**: `.sisyphus/evidence/` will be created by CI runs (not a failure)
3. **Placeholder gates**: Build, test, security, syscall gates are placeholders (expected - T4 implementation)
4. **Self-hosted runner availability**: Ubuntu 25.10 runner must be self-hosted; GitHub-hosted runners don't support it yet

## Notes
- All workflow jobs have `needs` dependencies forming a proper pipeline
- Evidence artifacts use `actions/upload-artifact@v4`
- Local gate-check.sh uses `set -euo pipefail` for strict error handling
- Branch protection documentation includes gate-to-check mapping table

## T3 QA Fixes (2026-03-02)

### Original Issues (Fixed)
1. **Case-insensitive grep**: Pattern "required-check" didn't match "Required Checks" - Fixed by adding `-i` flag to grep in check_contains function
2. **Pattern mismatch**: Changed "required-check" to "Required" for accurate matching

---

## T4 Issues: Rust Workspace 与核心 Crates 骨架

## Date
2026-03-02

## Blockers Encountered
None - T4 completed without blockers.

## Potential Issues
1. **LSP diagnostics not available**: rust-analyzer not installed; cargo check used for verification instead
2. **No business logic**: All crates are skeletons only; T5+ will implement actual functionality
3. **No tests yet**: Crates compile but have no test coverage (T4 is skeleton-only)

## Notes
- All crates use Rust edition 2021
- Workspace resolver set to "2" for workspace dependencies
- agentd-core exposes minimal public API: AgentProfile, AuditEvent, AgentError
- Two binaries created: agentd (daemon) and agentctl (CLI)

## T4 QA Fixes (2026-03-02)

### Original Issues (Fixed)
1. **Missing uuid dependency**: agentd-protocol and agentd-store used uuid::Uuid but uuid wasn't declared - Fixed by adding uuid.workspace = true to both manifests
2. **Unused imports**: agentd-protocol/src/lib.rs had unused imports - Fixed by running cargo fix

---

## T5 Issues: daemon 主进程骨架 + systemd notify + health

## Date
2026-03-02

## Blockers Encountered
1. **rust-analyzer unavailable to LSP tool**: `lsp_diagnostics` initially failed because `rust-analyzer` was not discoverable in default tool PATH.

## Fix Applied
1. Installed component: `source .cargo-env && rustup component add rust-analyzer`
2. Added symlink for tool PATH discovery: `~/.local/bin/rust-analyzer -> ~/.cargo/bin/rust-analyzer`
3. Re-ran `lsp_diagnostics` successfully with clean result.

## Potential Issues
1. Minimal health endpoint intentionally supports only basic `GET /health` checks and returns 404 for other routes.
2. `systemd/agentd.service` uses `/usr/local/bin/agentd`; deployment scripts must ensure binary is installed at that path.

## Notes
- No scope creep into policy/cgroup/protocol logic.
- Shutdown path enforces 5s timeout and logs timeout or completion outcome.

---

## T5 Fix Issues: health Content-Length mismatch

## Date
2026-03-02

## Failure Observed
1. `curl --noproxy '*' -fsS http://127.0.0.1:7000/health` failed with exit code 18.
2. stderr: `end of response with 1 bytes missing`.

## Root Cause
1. Daemon returned `Content-Length: 16` for body `{"status":"ok"}` which is actually 15 bytes.

## Fix Applied
1. Updated `/health` response header to `Content-Length: 15`.
2. Re-ran automated and hands-on checks; curl health probes now pass.
