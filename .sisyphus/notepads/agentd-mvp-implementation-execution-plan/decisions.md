# T1 Decisions: GitHub Repository Bootstrap Script

## Date
2026-03-02

## Architectural Choices

### 1. Shell Script vs Other Approaches
**Decision**: Use bash script rather than Python/Node.js
**Rationale**: 
- No additional dependencies required (gh CLI is already the dependency)
- Portable across Unix-like systems
- Self-contained, no virtual environment needed

### 2. Fallback Naming Order
**Decision**: Try `agentd` → `agentd-runtime` → `agentd-core`
**Rationale**:
- Primary name `agentd` is the preferred project name
- `agentd-runtime` suggests runtime component
- `agentd-core` is final fallback for core component

### 3. Public Repository Only
**Decision**: Always use `--public` flag, never private
**Rationale**:
- Project is open source (based on README)
- Security requirement explicitly stated in task
- Script rejects any private visibility with warning

### 4. Visibility Verification
**Decision**: Verify with `gh repo view --json visibility,name`
**Rationale**:
- Direct gh command (no jq dependency)
- Returns structured JSON for reliable parsing
- Accurate visibility check after repo creation

### 5. Dry-Run Implementation
**Decision**: Dry-run logs planned commands without execution
**Rationale**:
- Allows users to verify commands before actual execution
- Continues through fallback names in dry-run mode
- Provides transparency in operations

### 6. Fix: Remove set -e
**Decision**: Changed from `set -euo pipefail` to `set -uo pipefail`
**Rationale**:
- `set -e` causes script to exit on any non-zero return
- This breaks fallback loops when functions intentionally return non-zero
- Removed while keeping `pipefail` for pipeline error handling

### 7. Fix: Case-Insensitive Visibility
**Decision**: Added `is_public()` function with `${visibility,,}` comparison
**Rationale**:
- GitHub API may return "PUBLIC" (uppercase) or "public" (lowercase)
- Plan requires robust handling of actual GH value casing
- Uses bash lowercase expansion for reliable comparison

### 8. Fix: Non-Git Directory Support
**Decision**: Conditionally use `--source=.` only when in git repo
**Rationale**:
- `gh repo create --source=.` requires current directory to be a git repo
- When not in git repo, create bare repo without pushing source
- Use `git rev-parse --git-dir` to detect git repo presence

---

## T2 Decisions: Rust + uv Toolchain Baseline

## Date
2026-03-02

## Architectural Choices

### 1. Rust Toolchain Version
**Decision**: Pin Rust 1.85.0
**Rationale**:
- Recent stable version (Feb 2025) with rustfmt and clippy
- Compatible with Ubuntu 25.10 baseline
- Auto-installed via rust-toolchain.toml when cargo runs

### 2. Python UV Workspace Setup
**Decision**: Use uv with package = false for baseline
**Rationale**:
- No Python source code exists in baseline
- hatchling build fails without package structure
- Setting package = false allows uv sync to succeed without a package

### 3. No-Go Guard Scope
**Decision**: Check crates/, src/, cmd/, internal/, pkg/ but allow research/
**Rationale**:
- Main codebase must remain Go-free per MVP requirements
- research/ directory may contain Go prior art for study
- Guards against Go toolchain files at repo root
- Guards against Go in GitHub Actions workflows

### 4. Baseline Verification Strategy
**Decision**: Test uv sync --frozen; defer cargo check to T4
**Rationale**:
- uv can work with empty dependencies
- Rust workspace doesn't exist yet (Cargo.toml created in T4)
- T2 focus is toolchain configuration, not crate implementation

### 5. Fix: Cargo PATH Availability
**Decision**: Create .cargo-env file for PATH setup
**Rationale**:
- rustup installs to ~/.cargo/bin which isn't in default PATH
- Provides deterministic way to load cargo: `source .cargo-env`
- Documented in baseline for reproducibility

### 6. Fix: No-Go Guard Research Path Matching
**Decision**: Use regex `^(\./)?research/` instead of `^research/`
**Rationale**:
- find command may output paths as `./research/...` or `research/...`
- Original regex failed to match `./research/` prefix
- Both formats now correctly matched and allowed

---

## T3 Decisions: CI 硬门禁骨架与证据工件规范

## Date
2026-03-02

## Architectural Choices

### 1. Gate Pipeline Structure
**Decision**: Sequential gate jobs with explicit `needs` dependencies
**Rationale**:
- Each gate must pass before next executes
- Clear failure isolation (know which gate failed)
- Evidence artifacts uploaded after each gate
- Form: preflight → no-go-guard → build → test → security → syscall → isolation

### 2. Placeholder Gate Strategy
**Decision**: Include placeholder gates for T4 implementation with TODO comments
**Rationale**:
- T3 scope is skeleton only, not full implementation
- Provides clear contract for T4 to implement
- Placeholder jobs have descriptive comments
- Allows CI to pass with skeleton while T4 fills in logic

### 3. Ubuntu 25.10 Runner for Isolation
**Decision**: Use `self-hosted-ubuntu-25.10` label for isolation gate only
**Rationale**:
- Ubuntu 25.10 required for systemd + cgroup v2 validation
- GitHub-hosted runners don't offer Ubuntu 25.10 yet
- Only isolation gate needs this specific OS version
- Other gates use standard `ubuntu-latest`

### 4. Evidence Artifact Strategy
**Decision**: Each job uploads to `.sisyphus/evidence/{job-name}/`
**Rationale**:
- Structured evidence for debugging failed gates
- 30-day retention supports post-mortem analysis
- `if-no-files-found: ignore` prevents workflow failure
- Consistent naming: `evidence-{job-name}`

### 5. Local Validation Script
**Decision**: Separate `gate-check.sh` for local validation
**Rationale**:
- Allows developers to verify configuration before push
- Uses `set -euo pipefail` for strict error handling
- Supports `--local` flag for GitHub-specific skip
- Grep-based checks are portable (no external dependencies)

### 6. Case-Insensitive Pattern Matching
**Decision**: Use `grep -qi` for pattern validation
**Rationale**:
- Branch-protection.md uses "Required Checks" (Title Case)
- Check patterns need to match regardless of case
- Prevents false failures on documentation variations

### 7. Branch Protection Mapping
**Decision**: Explicit gate-to-required-check mapping in documentation
**Rationale**:
- GitHub UI requires status check names for protection rules
- Table format provides clear reference for setup
- Includes job name → status check name mapping

---

## T4 Decisions: Rust Workspace 与核心 Crates 骨架

## Date
2026-03-02

## Architectural Choices

### 1. Workspace Dependency Management
**Decision**: Use workspace dependencies for shared crates
**Rationale**:
- DRY principle: define dependencies once in root Cargo.toml
- Each crate references with `.workspace = true` syntax
- Consistent version across all crates

### 2. Crate Type Distribution
**Decision**: 3 libraries + 2 binaries
**Rationale**:
- agentd-core: lib - foundational types (AgentProfile, AuditEvent, AgentError)
- agentd-protocol: lib - protocol request/response types
- agentd-store: lib - storage trait abstractions
- agentd-daemon: bin - main daemon binary
- agentctl: bin - CLI binary

### 3. Public API Design (agentd-core)
**Decision**: Minimal public types with clear module organization
**Rationale**:
- error.rs: AgentError enum with domain-specific variants
- profile.rs: AgentProfile, ModelConfig, PermissionConfig, BudgetConfig
- audit.rs: AuditEvent, EventType, EventPayload, EventResult
- Re-exports in lib.rs for clean public API

### 4. Binary Structure
**Decision**: Minimal main functions with clap + tokio
**Rationale**:
- Skeleton-only for T4 (no business logic)
- Demonstrates async runtime setup (tokio::main)
- CLI argument parsing structure (clap)
- Logging setup (tracing)

### 5. Storage Abstraction
**Decision**: Trait-based storage layer
**Rationale**:
- AgentStore: CRUD for agent profiles
- AuditStore: Append-only audit log
- async-trait for async method definitions
- Allows implementation swapping (T9: DB logic)

### 6. Fix: uuid Dependency Placement
**Decision**: Add uuid to agentd-protocol and agentd-store manifests
**Rationale**:
- uuid::Uuid used in v1.rs and lib.rs but wasn't declared
- Workspace dependency ensures consistent version
- Fix required for compilation

### 7. Fix: Unused Import Cleanup
**Decision**: Auto-fix with cargo fix
**Rationale**:
- cargo fix handles mechanical cleanup safely
- Removes unused imports without manual editing
- Ensures clean build without warnings

---

## T5 Decisions: daemon 主进程骨架 + systemd notify + health

## Date
2026-03-02

## Architectural Choices

### 1. Health Endpoint Implementation Strategy
**Decision**: Implement a minimal HTTP health endpoint directly on `tokio::net::TcpListener`.
**Rationale**:
- Keeps T5 scope minimal without introducing a full web framework.
- Satisfies `/health` readiness checks required by plan acceptance.
- Reduces dependency surface for early daemon bootstrap.

### 2. systemd Notification Strategy
**Decision**: Implement `sd_notify` behavior via `NOTIFY_SOCKET` + `UnixDatagram` (`READY=1`, `STOPPING=1`).
**Rationale**:
- Works with `Type=notify` without additional system bindings.
- Keeps lifecycle semantics explicit and easy to reason about.
- Matches MVP requirement: readiness signal after daemon is actually serving health endpoint.

### 3. Graceful Shutdown Contract
**Decision**: Use unified signal handling (`SIGINT`/`SIGTERM`) and enforce 5-second bounded shutdown via timeout.
**Rationale**:
- Meets plan hard requirement: graceful stop within 5s.
- Avoids hanging process termination when a task does not stop promptly.
- Produces clear shutdown outcome logs (success/error/timeout).

### 4. Config Handling Strategy
**Decision**: Load optional TOML config from `--config` path with default fallback and CLI overrides for health host/port.
**Rationale**:
- Enables systemd/runtime configuration with minimal complexity.
- Keeps behavior deterministic when config file is missing.
- Aligns sample `configs/agentd.toml` with daemon defaults.

### 5. Service Unit Scope
**Decision**: Add a minimal `systemd/agentd.service` template with `Type=notify`, `TimeoutStopSec=5`, and restart-on-failure.
**Rationale**:
- Provides the smallest viable unit for MVP bootstrap and local deployment.
- Encodes shutdown timing contract directly in service manager settings.

---

## T5 Fix Decisions: health response correctness

## Date
2026-03-02

### 1. Response Header Correction Scope
**Decision**: Apply a minimal one-line fix to `/health` `Content-Length` only.
**Rationale**:
- Directly addresses QA failure without expanding scope.
- Keeps the fix atomic and easy to review/revert.

### 2. Validation Approach
**Decision**: Validate using both package test command and live process QA loop (`10x curl + SIGTERM timing`).
**Rationale**:
- Unit/test command preserves routine verification baseline.
- Live probes confirm wire-level HTTP framing correctness and shutdown behavior.
