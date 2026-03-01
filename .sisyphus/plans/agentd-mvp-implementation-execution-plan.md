# agentd MVP 全量实施执行计划（Phase A~D）

## TL;DR

> **Quick Summary**：在现有 `design/mvp-implementation-roadmap-v1.md` 基础上，交付一个可执行、可并行、可量化门禁的全量 MVP 计划（Phase A~D），并把“彻底验收”落地为硬性 CI/证据标准。  
> **关键原则**：零妥协验收、零人工判定通过、零范围漂移。

**Deliverables**
- 完整分波次任务图（高并行）
- 每任务可执行验收 + 失败/回滚条件
- Python `uv` / Rust 工具链基线与 GitHub public 仓库初始化策略
- One-API 同机托管、策略引擎、cgroup、审计链路、agent-lite 与 E2E 演示的全链路交付计划

**Estimated Effort**：XL（7–10 周，取决于 One-API 稳定性与故障注入轮次）  
**Parallel Execution**：YES（4 个实现波次 + Final 验证波次）  
**Critical Path**：T2 → T5 → T8 → T10 → T13 → T14 → T18 → T20

---

## Context

### Original Request
- 用户已提供初稿：`design/mvp-implementation-roadmap-v1.md`
- 目标：把路线图完善成“可切实实现、无妥协、验收彻底”的执行计划
- 约束：Python 用 `uv`；Go 本轮不配置；One-API 同机托管；`gh` 创建 public 仓库（优先名 `agentd`，允许备选）
- 验收：全量量化门禁，接入 GitHub Actions 作为 PR 硬门禁
- OS 基线：Ubuntu 25.10

### Interview Summary
**Key Decisions**
- 计划范围覆盖 Phase A~D 全量（单计划，不拆子计划）
- 测试策略：搭建测试基础设施并采用 TDD
- CI 策略：PR 必过硬门禁（测试/指标/安全/证据）
- 命名冲突策略：`agentd` 冲突时自动回退备选名

**Research Findings（摘要）**
- 现有文档结构完整，但量化阈值、回滚触发和故障注入覆盖不足
- 测试基础设施未明确，需要前置搭建
- One-API/策略/cgroup/审计存在强依赖，需严格 DAG 化

### Metis Review（已吸收）
- 强制加入：量化门禁 + 证据产物 + 回滚触发矩阵
- 强制 guardrails：不引入 Go、本轮不做多机、不得以人工主观判断替代验收
- 风险前置：Ubuntu 25.10 与 CI runner 一致性方案必须明确

---

## Work Objectives

### Core Objective
将 agentd MVP 从“路线图描述”落地为“可执行工程计划”：每个任务都有明确输入输出、依赖、验收门槛、失败路径、证据文件和回滚策略。

### Concrete Deliverables
- [ ] `.github/workflows/` 下 PR 硬门禁流水线设计（计划级）
- [ ] Rust + uv 工具链与仓库初始化实施路径
- [ ] Phase A/B/C/D 每阶段任务与量化验收矩阵
- [ ] 全链路 E2E 演示与最终并行审查波次

### Definition of Done
- [ ] 每个任务均具备可执行 Acceptance Criteria（命令 + 阈值 + 证据路径）
- [ ] 每个任务均具备 happy-path 与 failure-path QA 场景
- [ ] 每阶段均具备“回滚触发条件 + 回滚动作 + 回滚验收”
- [ ] 最终 4 个审查代理（F1-F4）全部 APPROVE

### Must Have
- 全量覆盖 Phase A~D，且任务可并行执行
- One-API 同机托管模式
- cgroup v2 + policy + audit 的可验证闭环
- GitHub public repo（`gh`）与 CI 硬门禁策略

### Must NOT Have (Guardrails)
- 不引入 Go 工具链与 Go 实现任务（本轮禁止）
- 不扩展到多机发现/迁移/服务网格
- 不引入 OPA/Rego 替换、Firecracker、Web UI 控制台等 MVP 外扩
- 不使用“人工点击/人工观察”作为验收通过条件

---

## Verification Strategy (MANDATORY)

> **ZERO HUMAN INTERVENTION**：验收必须由执行代理直接运行命令或自动化脚本完成。

### Test Decision
- **Infrastructure exists**：NO（未发现明确现有测试基线）
- **Automated tests**：YES（TDD）
- **Framework**：Rust `cargo test` + Python `uv run pytest` + Shell gate scripts
- **CI Gate**：GitHub Actions required checks（PR 硬门禁）
- **OS Baseline**：Ubuntu 25.10（默认采用 `self-hosted ubuntu-25.10` runner 执行 systemd/cgroup 门禁；静态检查可用 GitHub-hosted）

### Hard Gate Baselines（默认值，可在执行时微调但不得降低）
- 启动与健康：`agentd` cold start ≤ 3s，`/health` 成功率 100%（10/10）
- 控制面稳定性：JSON-RPC 请求成功率 ≥ 99.5%（200 次）
- One-API Token 供应：创建成功率 ≥ 99%（100 次并发注册）
- 策略判定准确率：规则测试通过率 100%（含 deny 覆盖）
- 资源隔离：内存超限触发行为符合预期（OOM/拒绝）100% 可观测
- 审计完整性：关键事件字段完整率 100%，trace 可关联率 100%

### QA Policy
每个任务必须给出：
- 至少 1 个 happy-path 场景
- 至少 1 个 error/edge 场景
- 明确工具（Playwright / interactive_bash / Bash）
- 证据保存到 `.sisyphus/evidence/task-{N}-{slug}.{ext}`

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1（Foundation，立即并行）
├── T1: gh public repo 初始化策略与命名回退机制
├── T2: Rust+uv 工具链基线（显式 no-Go guard）
├── T3: CI 硬门禁骨架与证据工件标准
├── T4: Rust workspace + crates 骨架
├── T5: daemon 主进程骨架 + systemd notify + health
└── T6: UDS JSON-RPC + agentctl 基础命令骨架

Wave 2（Phase A 核心能力）
├── T7: SQLite store + 迁移（agent/quota）
├── T8: One-API 同机托管监督器
├── T9: One-API 管理客户端 + token/channel 映射
├── T10: agent create/list 端到端打通（含幂等）
├── T11: 用量采集与预算基线控制
└── T12: Phase A 量化门禁 + 回滚演练

Wave 3（Phase B/C 治理与观测）
├── T13: 策略引擎（allow/ask/deny + wildcard + merge）
├── T14: cgroup v2 + lifecycle 管理（fork/exec/restart）
├── T15: 审计事件模型与持久化
├── T16: 事件订阅流 + agentctl events
└── T17: usage/cost 查询 + B/C 故障注入门禁

Wave 4（Phase D + E2E）
├── T18: Python agent-lite（uv）+ agentctl run 集成
├── T19: A2A Agent Card 生成 + 端到端演示脚本
└── T20: 最终硬化（安全门禁 + 发布候选 + 回滚总演练）

Wave FINAL（并行独立审查）
├── F1: Plan Compliance Audit（oracle）
├── F2: Code Quality Review（unspecified-high）
├── F3: Real QA Replay（unspecified-high）
└── F4: Scope Fidelity Check（deep）
```

### Dependency Matrix（FULL）

- **T1**: Blocked By — None | Blocks — T3, T20
- **T2**: Blocked By — None | Blocks — T3, T4, T18
- **T3**: Blocked By — T1, T2 | Blocks — T12, T17, T20
- **T4**: Blocked By — T2 | Blocks — T5, T6, T7, T9, T13, T14, T15
- **T5**: Blocked By — T4 | Blocks — T8, T10, T14
- **T6**: Blocked By — T4 | Blocks — T10, T16, T18
- **T7**: Blocked By — T4 | Blocks — T10, T11, T15, T17
- **T8**: Blocked By — T5 | Blocks — T9, T10, T11, T12
- **T9**: Blocked By — T4, T8 | Blocks — T10, T11
- **T10**: Blocked By — T5, T6, T7, T8, T9 | Blocks — T12, T18, T19
- **T11**: Blocked By — T7, T8, T9 | Blocks — T12, T17
- **T12**: Blocked By — T3, T8, T10, T11 | Blocks — T20
- **T13**: Blocked By — T4 | Blocks — T14, T16, T18, T19
- **T14**: Blocked By — T4, T5, T13 | Blocks — T17, T18, T20
- **T15**: Blocked By — T4, T7 | Blocks — T16, T17, T19, T20
- **T16**: Blocked By — T6, T13, T15 | Blocks — T19, T20
- **T17**: Blocked By — T3, T7, T11, T14, T15 | Blocks — T20
- **T18**: Blocked By — T2, T6, T10, T13, T14 | Blocks — T19, T20
- **T19**: Blocked By — T10, T13, T15, T16, T18 | Blocks — T20
- **T20**: Blocked By — T1, T3, T12, T14, T15, T16, T17, T18, T19 | Blocks — F1, F2, F3, F4

### Agent Dispatch Summary

- **Wave 1（6 tasks）**
  - T1/T2/T3/T4/T6 → `quick`
  - T5 → `unspecified-high`
- **Wave 2（6 tasks）**
  - T7/T9/T10 → `unspecified-high`
  - T8 → `deep`
  - T11/T12 → `deep`
- **Wave 3（5 tasks）**
  - T13/T14/T17 → `deep`
  - T15/T16 → `unspecified-high`
- **Wave 4（3 tasks）**
  - T18 → `deep`
  - T19 → `unspecified-high`
  - T20 → `deep`
- **Final（4 tasks）**
  - F1 → `oracle`, F2/F3 → `unspecified-high`, F4 → `deep`

---

## TODOs

---

- [x] 1. GitHub public 仓库初始化与命名回退机制

  **What to do**:
  - 设计并实现 `gh` 初始化流程：优先创建 `agentd`，冲突时回退 `agentd-runtime` → `agentd-core`。
  - 固化 public 仓库默认策略：默认分支、PR 模板、required checks、secret scanning/code scanning 占位。
  - 编写自动化校验脚本，验证仓库可见性为 public。

  **Must NOT do**:
  - 不创建 private 仓库。
  - 不引入组织级策略（用户已指定个人账号）。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 仓库初始化与命令脚本属于短链路配置任务。
  - **Skills**: [`git-master`, `conventional-commits`]
    - `git-master`: 约束仓库初始化与分支流程。
    - `conventional-commits`: 统一初始化提交规范。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非浏览器交互场景。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1（与 T2/T3/T4/T5/T6 并行）
  - **Blocks**: T3, T20
  - **Blocked By**: None

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - MVP 实施入口与阶段边界，确保仓库初始化不超范围。
  - `README.md` - 项目愿景与目录规范，初始化模板需对齐。
  - Official docs: `https://cli.github.com/manual/gh_repo_create` - `gh repo create` 参数与行为。
  - Official docs: `https://docs.github.com/en/repositories/creating-and-managing-repositories/creating-a-new-repository` - public 可见性与默认策略。
  - Official docs: `https://docs.github.com/en/code-security/secret-scanning/about-secret-scanning` - public 仓库 secrets 风险防护。

  **WHY Each Reference Matters**:
  - 路线图与 README 约束“只做 MVP 必需初始化”。
  - gh/GitHub 官方文档保证命令行为可预期、可复现。

  **Acceptance Criteria**:
  - [ ] `gh repo create agentd --public --confirm` 成功；若冲突自动尝试备选名并成功。
  - [ ] `gh repo view --json visibility,name` 输出 `visibility=PUBLIC`。
  - [ ] `secret scanning` 与关键安全检查在 public 场景下启用并可见。
  - [ ] 回退策略日志包含“尝试序列 + 成功名称”。

  **QA Scenarios**:
  ```
  Scenario: Happy path — agentd 名称可用并成功创建 public 仓库
    Tool: Bash
    Preconditions: gh 已登录个人账号；本地无同名 remote
    Steps:
      1. 运行: gh repo create agentd --public --confirm
      2. 运行: gh repo view agentd --json visibility,name
      3. 断言: visibility 字段为 "PUBLIC"，name 为 "agentd"
    Expected Result: 公有仓库创建成功，名称为 agentd
    Failure Indicators: 命令非 0；visibility 非 PUBLIC
    Evidence: .sisyphus/evidence/task-1-gh-create.txt

  Scenario: Error path — agentd 已占用时自动回退
    Tool: Bash
    Preconditions: 账号下已存在 agentd（或模拟创建冲突）
    Steps:
      1. 执行初始化脚本（含回退序列）
      2. 读取脚本输出中的候选尝试记录
      3. 断言: 最终创建的仓库为备选名且 visibility=PUBLIC
    Expected Result: 不中断，成功回退并创建 public 仓库
    Evidence: .sisyphus/evidence/task-1-gh-fallback.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-1-gh-create.txt`
  - [ ] `.sisyphus/evidence/task-1-gh-fallback.txt`

  **Commit**: YES
  - Message: `chore(repo): bootstrap public repository creation with fallback`
  - Files: `.github/`, `scripts/bootstrap-repo.sh`, `README.md`
  - Pre-commit: `bash scripts/bootstrap-repo.sh --dry-run`

- [x] 2. Rust + uv 工具链基线（含 no-Go guard）

  **What to do**:
  - 建立 Rust toolchain 固定版本与 workspace 约束。
  - 建立 Python `uv` workspace 与 lockfile 流程（`uv sync --frozen` 可重放）。
  - 增加 no-Go guard：CI 若检测到 Go 相关新增构建路径则失败。

  **Must NOT do**:
  - 不安装/配置 Go 工具链。
  - 不允许未锁定版本进入主分支。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 基础环境约束是高价值、低复杂度前置。
  - **Skills**: [`git-master`]
    - `git-master`: 确保基线配置原子提交、便于回滚。
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: 非 UI 任务。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T3, T4, T18
  - **Blocked By**: None

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - 技术栈明确 Rust + Python（Phase D lite agent）。
  - `analysis/decisions/ADR-003-mvp-composition-architecture.md` - MVP 组件边界与语言职责。
  - Official docs: `https://docs.astral.sh/uv/` - uv workspace/lock 规范。
  - Official docs: `https://doc.rust-lang.org/cargo/reference/workspaces.html` - Cargo workspace 标准。

  **WHY Each Reference Matters**:
  - ADR 与路线图防止环境配置偏离 MVP 语言边界。
  - 官方文档确保锁版本和可复现构建做法正确。

  **Acceptance Criteria**:
  - [ ] `uv sync --frozen` 退出码 0。
  - [ ] `cargo check --workspace` 退出码 0。
  - [ ] `grep -R "^go " .`（或等效脚本）在允许路径外无 Go 构建信号。

  **QA Scenarios**:
  ```
  Scenario: Happy path — Rust/uv 基线可复现
    Tool: Bash
    Preconditions: Ubuntu 25.10；网络可访问依赖源
    Steps:
      1. 运行: uv sync --frozen
      2. 运行: cargo check --workspace
      3. 断言: 两个命令均 exit 0
    Expected Result: Python/Rust 环境可重放，无漂移
    Failure Indicators: lockfile 不一致、编译失败
    Evidence: .sisyphus/evidence/task-2-toolchain-happy.txt

  Scenario: Error path — 引入未锁定依赖时被拦截
    Tool: Bash
    Preconditions: 人为修改依赖版本但未更新锁文件
    Steps:
      1. 运行: uv sync --frozen
      2. 运行: cargo check --workspace
      3. 断言: 至少一个命令失败并输出锁文件不一致信息
    Expected Result: 非法依赖漂移被拒绝
    Evidence: .sisyphus/evidence/task-2-toolchain-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-2-toolchain-happy.txt`
  - [ ] `.sisyphus/evidence/task-2-toolchain-error.txt`

  **Commit**: YES
  - Message: `chore(env): pin rust and uv toolchain with no-go guard`
  - Files: `rust-toolchain.toml`, `pyproject.toml`, `uv.lock`, `.github/workflows/ci.yml`
  - Pre-commit: `uv sync --frozen && cargo check --workspace`

- [x] 3. CI 硬门禁骨架与证据工件规范

  **What to do**:
  - 建立 GitHub Actions workflow：测试、lint、安全扫描、门禁阈值检查。
  - 将 required checks 写入分支保护策略（计划级定义 + 执行脚本）。
  - 固化 Ubuntu 25.10 运行策略：systemd/cgroup 门禁 job 必须跑在 `self-hosted ubuntu-25.10` label。
  - 统一证据产物目录和命名（按 task/scenario 固定）。

  **Must NOT do**:
  - 不允许“仅警告不失败”的关键门禁。
  - 不依赖人工在 PR 评论里手工确认。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: CI 架构搭建属于标准化流水线配置。
  - **Skills**: [`git-master`]
    - `git-master`: 分支保护与工作流变更需要严谨版本控制。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 本任务主要是 CI 编排，不是 UI 自动化。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T12, T17, T20
  - **Blocked By**: T1, T2

  **References**:
  - `design/phase3-delivery-plan-and-gates.md` - 阶段门禁思想来源。
  - `design/mvp-implementation-roadmap-v1.md` - Phase A~D 验收目标来源。
  - Official docs: `https://docs.github.com/en/actions` - Actions 编排。
  - Official docs: `https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches` - required checks。

  **WHY Each Reference Matters**:
  - 先把“文档验收”转成“CI 可执行门禁”。
  - required checks 是“彻底验收”在工程流程上的强约束。

  **Acceptance Criteria**:
  - [ ] PR 打开后自动触发 gate workflow。
  - [ ] 关键 job（build/test/security/gate）任一失败，PR 不能 merge。
  - [ ] Ubuntu 25.10 基线在 CI 配置中可验证（systemd/cgroup job 使用 `self-hosted ubuntu-25.10`）。
  - [ ] `.sisyphus/evidence/` 工件可在 CI artifact 中下载。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 全部门禁通过
    Tool: Bash
    Preconditions: 分支保护已配置 required checks；提交为有效变更
    Steps:
      1. 推送 PR 分支并触发 CI
      2. 运行: gh pr checks <PR_NUMBER> --required
      3. 断言: 所有 required checks 为 PASS
    Expected Result: PR 可进入可合并状态
    Failure Indicators: 任一 required check 非 PASS
    Evidence: .sisyphus/evidence/task-3-ci-happy.txt

  Scenario: Error path — 人为制造门禁失败
    Tool: Bash
    Preconditions: 在 PR 中引入故意失败测试
    Steps:
      1. 推送包含失败测试的提交
      2. 运行: gh pr checks <PR_NUMBER> --required
      3. 断言: 至少一个 required check 失败且 PR 不可 merge
    Expected Result: CI 正确阻断不合格变更
    Evidence: .sisyphus/evidence/task-3-ci-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-3-ci-happy.txt`
  - [ ] `.sisyphus/evidence/task-3-ci-error.txt`

  **Commit**: YES
  - Message: `ci(gates): enforce required checks and evidence artifacts`
  - Files: `.github/workflows/gates.yml`, `.github/branch-protection.md`, `scripts/gate-check.sh`
  - Pre-commit: `bash scripts/gate-check.sh --local`

---

- [x] 4. Rust workspace 与核心 crates 骨架

  **What to do**:
  - 建立 `Cargo.toml` workspace 根与 `crates/` 目录骨架。
  - 初始化核心 crate：`agentd-core`、`agentd-daemon`、`agentd-protocol`、`agentd-store`、`agentctl`。
  - 在 `agentd-core` 先定义最小类型：`AgentProfile`、`AuditEvent`、`AgentError`。

  **Must NOT do**:
  - 不一次性填满业务逻辑（先骨架，后分任务填充）。
  - 不引入未在 MVP 路线图出现的 crate。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 工程骨架初始化可标准化快速完成。
  - **Skills**: [`git-master`]
    - `git-master`: 结构性变更需要清晰原子提交。
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: 非浏览器流程。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T5, T6, T7, T9, T13, T14, T15
  - **Blocked By**: T2

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - 已给出目标目录结构与 crate 划分。
  - `analysis/decisions/ADR-003-mvp-composition-architecture.md` - MVP 组件职责边界。
  - Official docs: `https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html` - workspace 实践。

  **WHY Each Reference Matters**:
  - 路线图中的 crate 切分是后续并行开发的基础。
  - 官方 workspace 规范降低后续依赖冲突概率。

  **Acceptance Criteria**:
  - [ ] `cargo metadata --no-deps` 能列出预期 workspace members。
  - [ ] `cargo check --workspace` 在骨架状态下通过。
  - [ ] `agentd-core` 暴露最小公共类型并可被 `agentd-daemon` 引用。

  **QA Scenarios**:
  ```
  Scenario: Happy path — workspace 架构可编译
    Tool: Bash
    Preconditions: rust toolchain 已固定
    Steps:
      1. 运行: cargo metadata --no-deps
      2. 运行: cargo check --workspace
      3. 断言: 输出包含 agentd-core/agentd-daemon/agentctl 等成员且 check 通过
    Expected Result: 工程骨架可编译、依赖关系正确
    Failure Indicators: member 缺失或编译失败
    Evidence: .sisyphus/evidence/task-4-workspace-happy.txt

  Scenario: Error path — 非法 crate 注入被识别
    Tool: Bash
    Preconditions: 人为添加未授权 crate 到 workspace
    Steps:
      1. 运行: cargo metadata --no-deps
      2. 运行结构校验脚本
      3. 断言: 脚本报出“超出 MVP crate 范围”并失败
    Expected Result: MVP 边界被守住
    Evidence: .sisyphus/evidence/task-4-workspace-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-4-workspace-happy.txt`
  - [ ] `.sisyphus/evidence/task-4-workspace-error.txt`

  **Commit**: YES
  - Message: `chore(rust): scaffold workspace and core crates`
  - Files: `Cargo.toml`, `crates/*/Cargo.toml`, `crates/agentd-core/src/*`
  - Pre-commit: `cargo check --workspace`

- [ ] 5. daemon 主进程骨架 + systemd notify + health

  **What to do**:
  - 在 `agentd-daemon` 实现最小启动流程、信号处理和优雅退出。
  - 接入 `sd_notify`（Type=notify）并提供健康检查接口。
  - 产出 `systemd/agentd.service` 最小可运行模板。

  **Must NOT do**:
  - 不在该任务引入完整策略/隔离逻辑（后续任务处理）。
  - 不将健康检查与业务逻辑强耦合。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 涉及进程生命周期与 systemd 交互，错误代价高。
  - **Skills**: [`git-master`]
    - `git-master`: 生命周期代码需可追溯提交与可回滚。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T8, T10, T14
  - **Blocked By**: T4

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase A 明确 systemd notify 与 daemon 入口。
  - `design/architecture/agentd-reference-architecture-v1.md` - 生命周期与控制面边界。
  - Official docs: `https://www.freedesktop.org/software/systemd/man/sd_notify.html` - notify 协议。

  **WHY Each Reference Matters**:
  - 保证 daemon 启停语义和路线图一致。
  - 避免错误实现导致 `systemctl` 假成功。

  **Acceptance Criteria**:
  - [ ] `systemctl start agentd` 后 `systemctl is-active agentd` 返回 `active`。
  - [ ] 健康端点连续 10 次检查成功率 100%。
  - [ ] `SIGTERM` 后在 5s 内优雅退出并写入退出日志。

  **QA Scenarios**:
  ```
  Scenario: Happy path — daemon 启动并上报 ready
    Tool: Bash
    Preconditions: Ubuntu 25.10，systemd 可用，service 文件已安装
    Steps:
      1. 运行: systemctl start agentd
      2. 运行: systemctl is-active agentd
      3. 运行: curl -sf http://127.0.0.1:7000/health | jq -r '.status'
    Expected Result: 状态为 active，health 返回 ok
    Failure Indicators: is-active 非 active；health 超时或非 ok
    Evidence: .sisyphus/evidence/task-5-daemon-happy.txt

  Scenario: Error path — 配置错误时启动失败可诊断
    Tool: Bash
    Preconditions: 注入无效配置（如缺失必须字段）
    Steps:
      1. 运行: systemctl start agentd
      2. 运行: journalctl -u agentd -n 50 --no-pager
      3. 断言: 服务失败并输出可诊断错误码/字段
    Expected Result: 失败可观测且不进入僵死状态
    Evidence: .sisyphus/evidence/task-5-daemon-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-5-daemon-happy.txt`
  - [ ] `.sisyphus/evidence/task-5-daemon-error.txt`

  **Commit**: YES
  - Message: `feat(daemon): add systemd notify lifecycle and health endpoint`
  - Files: `crates/agentd-daemon/src/main.rs`, `systemd/agentd.service`, `configs/agentd.toml`
  - Pre-commit: `cargo test -p agentd-daemon`

- [ ] 6. UDS JSON-RPC 协议骨架 + agentctl 基础命令

  **What to do**:
  - 在 `agentd-protocol` 完成 UDS 监听、JSON-RPC 编解码与基础路由。
  - 在 `agentctl` 实现 `health`、`agent list` 基础命令。
  - 定义基础错误码和超时策略。

  **Must NOT do**:
  - 不在此任务扩展远程网络协议（仅本地 UDS）。
  - 不混入业务权限判断。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 协议骨架 + CLI 基础命令是可拆分标准任务。
  - **Skills**: [`git-master`]
    - `git-master`: 协议契约变更需可追踪。
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1
  - **Blocks**: T10, T16, T18
  - **Blocked By**: T4

  **References**:
  - `design/protocols/agentd-protocol-profile-v1.md` - 协议层与消息语义。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 控制/观测接口行为。
  - `design/mvp-implementation-roadmap-v1.md` - Phase A 中 agentctl 与 protocol 要求。

  **WHY Each Reference Matters**:
  - 避免命令语义与协议文档脱节。
  - 为后续 create/list/events 命令复用统一通道。

  **Acceptance Criteria**:
  - [ ] `agentctl health` 在 daemon 存活时返回 `ok`。
  - [ ] daemon 停止时 `agentctl health` 返回可识别错误码。
  - [ ] `agentctl agent list` 在空数据库返回空列表而非异常。

  **QA Scenarios**:
  ```
  Scenario: Happy path — UDS 控制通道可用
    Tool: Bash
    Preconditions: daemon 已启动，UDS 文件存在
    Steps:
      1. 运行: agentctl health
      2. 运行: agentctl agent list --json
      3. 断言: health.status=ok；list 返回 [] 或合法数组
    Expected Result: 控制通道稳定，命令输出可解析
    Failure Indicators: 超时、JSON 非法、退出码非 0
    Evidence: .sisyphus/evidence/task-6-protocol-happy.txt

  Scenario: Error path — daemon 不可用时超时与错误码正确
    Tool: Bash
    Preconditions: daemon 已停止
    Steps:
      1. 运行: agentctl health --timeout 2s
      2. 捕获 stderr 与退出码
      3. 断言: 错误码为预定义连接失败码
    Expected Result: 快速失败且可诊断
    Evidence: .sisyphus/evidence/task-6-protocol-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-6-protocol-happy.txt`
  - [ ] `.sisyphus/evidence/task-6-protocol-error.txt`

  **Commit**: YES
  - Message: `feat(protocol): add uds json-rpc skeleton and agentctl base commands`
  - Files: `crates/agentd-protocol/src/*`, `crates/agentctl/src/main.rs`
  - Pre-commit: `cargo test -p agentd-protocol -p agentctl`

---

- [ ] 7. SQLite store 初始化与迁移（agent/quota）

  **What to do**:
  - 在 `agentd-store` 实现数据库连接、迁移执行与健康检测。
  - 建立 `agents`、`quota_usage` 基础表结构与索引。
  - 提供最小 CRUD：创建 Agent 元数据、按条件查询列表。

  **Must NOT do**:
  - 不引入非 SQLite 存储后端。
  - 不在此任务实现完整审计表（T15 负责）。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 存储 schema 一旦错误，后续返工成本高。
  - **Skills**: [`git-master`]
    - `git-master`: schema 变更需要精细版本演进。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2
  - **Blocks**: T10, T11, T15, T17
  - **Blocked By**: T4

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - MVP 存储明确使用 SQLite。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 查询接口输出约束。
  - Official docs: `https://www.sqlite.org/lang.html` - SQL 语义与约束行为。

  **WHY Each Reference Matters**:
  - 先保证控制面最小数据闭环，避免后续协议层“无状态漂移”。

  **Acceptance Criteria**:
  - [ ] 初始化命令能创建数据库并完成迁移。
  - [ ] 重复运行迁移幂等（第二次运行不报错、不重复建表）。
  - [ ] `agentctl agent list` 可读取空/非空状态。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 首次迁移成功
    Tool: Bash
    Preconditions: 数据库文件不存在
    Steps:
      1. 启动 daemon 触发迁移
      2. 运行: sqlite3 data/agentd.db '.tables'
      3. 断言: 包含 agents 与 quota_usage
    Expected Result: 表结构一次性创建成功
    Failure Indicators: 缺表、迁移异常
    Evidence: .sisyphus/evidence/task-7-store-happy.txt

  Scenario: Error path — 迁移冲突可检测
    Tool: Bash
    Preconditions: 人为篡改 schema 版本号
    Steps:
      1. 再次启动 daemon
      2. 捕获迁移日志与退出码
      3. 断言: 明确报出版本冲突并拒绝继续
    Expected Result: 不一致 schema 被阻断
    Evidence: .sisyphus/evidence/task-7-store-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-7-store-happy.txt`
  - [ ] `.sisyphus/evidence/task-7-store-error.txt`

  **Commit**: YES
  - Message: `feat(store): add sqlite bootstrap migrations and base agent/quota schema`
  - Files: `crates/agentd-store/src/db.rs`, `crates/agentd-store/src/agent.rs`, `migrations/*`
  - Pre-commit: `cargo test -p agentd-store`

- [ ] 8. One-API 同机托管监督器

  **What to do**:
  - 在 daemon 生命周期中加入 One-API 子进程拉起、探活、退出重试策略。
  - 确保 `systemctl start agentd` 时能同步准备 One-API 可用状态。
  - 暴露 One-API 状态到健康报告。

  **Must NOT do**:
  - 不改造成外部依赖模式（本轮固定同机托管）。
  - 不引入多实例编排。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 进程监督 + 依赖服务时序复杂，需系统性推演。
  - **Skills**: [`git-master`]
    - `git-master`: 监督策略需明确版本演进与回滚点。
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: 非浏览器。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2
  - **Blocks**: T9, T10, T11, T12
  - **Blocked By**: T5

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase A 要求启动 agentd 同时拉起 One-API。
  - `analysis/decisions/ADR-003-mvp-composition-architecture.md` - One-API 作为 MVP 网关核心。
  - Official docs: `https://www.freedesktop.org/software/systemd/man/systemd.service.html` - 服务依赖与重启策略。

  **WHY Each Reference Matters**:
  - One-API 可用性直接决定 Agent 注册和调用链是否可达。

  **Acceptance Criteria**:
  - [ ] `systemctl start agentd` 后 One-API 健康检查在 30s 内转为 ready。
  - [ ] One-API 进程异常退出时可按策略拉起，3 次内恢复。
  - [ ] 健康报告包含 `one_api.status=ready|degraded`。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 联动启动成功
    Tool: Bash
    Preconditions: One-API 二进制/容器运行条件满足
    Steps:
      1. 运行: systemctl restart agentd
      2. 轮询: curl -sf http://127.0.0.1:3000/health
      3. 断言: 30s 内返回健康状态
    Expected Result: agentd 与 One-API 联动就绪
    Failure Indicators: 超时未 ready
    Evidence: .sisyphus/evidence/task-8-oneapi-happy.txt

  Scenario: Error path — One-API 崩溃后自动恢复
    Tool: Bash
    Preconditions: 服务已正常运行
    Steps:
      1. 杀掉 One-API 进程
      2. 观察 agentd 日志中的重试/恢复记录
      3. 断言: 3 次重试窗口内恢复健康
    Expected Result: 无人工介入恢复
    Evidence: .sisyphus/evidence/task-8-oneapi-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-8-oneapi-happy.txt`
  - [ ] `.sisyphus/evidence/task-8-oneapi-error.txt`

  **Commit**: YES
  - Message: `feat(gateway): add managed one-api supervisor lifecycle`
  - Files: `crates/agentd-lifecycle/src/systemd.rs`, `crates/agentd-daemon/src/main.rs`, `configs/agentd.toml`
  - Pre-commit: `cargo test -p agentd-daemon`

- [ ] 9. One-API 管理客户端与 token/channel 映射

  **What to do**:
  - 在 `agentd-gateway` 实现 One-API 管理 API 客户端。
  - 新建 Agent 时自动创建 token/channel，并建立本地映射关系。
  - 处理重试、超时、重复创建（幂等键）。

  **Must NOT do**:
  - 不在本任务实现完整成本聚合报表（T11/T17 负责）。
  - 不绕过本地持久层直写临时状态。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 外部 API 集成易出现边界条件错误。
  - **Skills**: [`git-master`]
    - `git-master`: 集成层改动需可回滚。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2
  - **Blocks**: T10, T11
  - **Blocked By**: T4, T8

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase A 要求创建 Agent 即生成 One-API token。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 管控接口输入输出约束。
  - Official docs: `https://docs.oneapi.com` - 管理 API 行为（按实际 One-API 文档版本对齐）。

  **WHY Each Reference Matters**:
  - token/channel 映射是“注册即可用”的关键路径。

  **Acceptance Criteria**:
  - [ ] 调用 `agent create` 后 One-API 中可查询到对应 token。
  - [ ] 并发重复创建同名 agent 时不产生重复 token。
  - [ ] 超时重试策略在 3 次内有确定结论（成功或失败回滚）。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 创建 agent 自动配发 token
    Tool: Bash
    Preconditions: One-API ready；daemon ready
    Steps:
      1. 运行: agentctl agent create --name my-agent --model claude-4-sonnet --token-budget 100000
      2. 运行: agentctl agent list --json
      3. 调用 One-API 管理接口查询 token
    Expected Result: agent 与 token 映射存在且一致
    Failure Indicators: agent 创建成功但 token 缺失
    Evidence: .sisyphus/evidence/task-9-gateway-happy.txt

  Scenario: Error path — One-API 超时触发重试与回滚
    Tool: Bash
    Preconditions: 人为注入 One-API 慢响应/超时
    Steps:
      1. 触发 agent create 请求
      2. 观察重试日志与最终状态
      3. 断言: 超过重试上限后本地状态回滚为 failed
    Expected Result: 无半完成脏状态
    Evidence: .sisyphus/evidence/task-9-gateway-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-9-gateway-happy.txt`
  - [ ] `.sisyphus/evidence/task-9-gateway-error.txt`

  **Commit**: YES
  - Message: `feat(gateway): provision one-api tokens with idempotent mapping`
  - Files: `crates/agentd-gateway/src/oneapi.rs`, `crates/agentd-gateway/src/quota.rs`, `crates/agentd-store/src/agent.rs`
  - Pre-commit: `cargo test -p agentd-gateway`

---

- [ ] 10. `agent create/list` 端到端打通（含幂等与状态机）

  **What to do**:
  - 打通 `agentctl -> UDS JSON-RPC -> daemon -> store -> gateway` 全链路。
  - 实现 `agent create` 幂等行为（重复请求不重复创建 token/记录）。
  - 明确状态机：`creating -> ready | failed`。

  **Must NOT do**:
  - 不允许“创建成功但状态未知”的中间态泄漏。
  - 不把失败请求误记为 ready。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 跨模块集成任务，状态一致性要求高。
  - **Skills**: [`git-master`]
    - `git-master`: 需要清晰追踪跨模块变更范围。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2（与 T11/T12 并行，但依赖 T5/T6/T7/T8/T9）
  - **Blocks**: T12, T18, T19
  - **Blocked By**: T5, T6, T7, T8, T9

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase A 验收核心链路（create/list + token 分配）。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 管控接口契约。
  - `design/protocols/agentd-protocol-profile-v1.md` - 请求/响应语义。

  **WHY Each Reference Matters**:
  - 保证“注册即可用”不是局部成功，而是端到端成功。

  **Acceptance Criteria**:
  - [ ] `agentctl agent create` 成功后 `agentctl agent list` 可见且状态为 ready。
  - [ ] 对同一 `name+model` 重复提交 10 次，仅生成 1 个实体。
  - [ ] 失败路径能返回稳定错误码并写入失败原因。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 创建后立刻可见且可用
    Tool: Bash
    Preconditions: daemon、One-API、store 均 ready
    Steps:
      1. 运行: agentctl agent create --name e2e-agent --model claude-4-sonnet --token-budget 100000
      2. 运行: agentctl agent list --json
      3. 断言: e2e-agent 存在且 status=ready
    Expected Result: create/list 全链路打通
    Failure Indicators: list 无该 agent 或状态非 ready
    Evidence: .sisyphus/evidence/task-10-create-list-happy.txt

  Scenario: Error path — 重复创建触发幂等
    Tool: Bash
    Preconditions: e2e-agent 已存在
    Steps:
      1. 并发发起 10 次相同 create 请求
      2. 运行: agentctl agent list --json | jq
      3. 断言: 仅 1 条 e2e-agent 记录，且无重复 token
    Expected Result: 幂等生效，无重复实体
    Evidence: .sisyphus/evidence/task-10-create-list-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-10-create-list-happy.txt`
  - [ ] `.sisyphus/evidence/task-10-create-list-error.txt`

  **Commit**: YES
  - Message: `feat(phase-a): wire agent create/list end-to-end with idempotency`
  - Files: `crates/agentd-daemon/src/*`, `crates/agentd-protocol/src/*`, `crates/agentctl/src/*`
  - Pre-commit: `cargo test --workspace`

- [ ] 11. 用量采集与预算基线控制

  **What to do**:
  - 周期性拉取 One-API 用量并聚合到 agent 维度。
  - 实现日预算门控：超过预算触发 `llm.quota_exceeded` 并阻断请求。
  - 提供 `agentctl usage <agent-id>` 最小查询能力。

  **Must NOT do**:
  - 不做跨日账单系统（仅 MVP 预算门控与查询）。
  - 不忽略采集失败（失败必须可观测）。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 涉及计量一致性、门控和事件联动。
  - **Skills**: [`git-master`]
    - `git-master`: 计量逻辑必须版本可追溯。
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2
  - **Blocks**: T12, T17
  - **Blocked By**: T7, T8, T9

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - token 用量与成本查询目标。
  - `design/runtime-and-governance-v1.md` - 预算/治理语义。
  - `design/security-trust-and-audit-v1.md` - 超额事件审计要求。

  **WHY Each Reference Matters**:
  - 预算门控是“资源管控”价值主张的核心，不可后补。

  **Acceptance Criteria**:
  - [ ] 定时采集任务执行成功率 ≥ 99%（100 次周期）。
  - [ ] 超预算后新请求被阻断并写入 `llm.quota_exceeded` 事件。
  - [ ] `agentctl usage` 显示 input/output/total 与模型成本分布。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 用量可聚合并可查询
    Tool: Bash
    Preconditions: 已有 agent 发起过若干 LLM 请求
    Steps:
      1. 触发采集任务（或等待周期）
      2. 运行: agentctl usage e2e-agent --json
      3. 断言: 返回 total_tokens>0 且包含 model_cost_breakdown
    Expected Result: 用量与成本可观测
    Failure Indicators: usage 空白或字段缺失
    Evidence: .sisyphus/evidence/task-11-usage-happy.txt

  Scenario: Error path — 预算超限触发阻断
    Tool: Bash
    Preconditions: 将 e2e-agent 日预算设置为极低值
    Steps:
      1. 连续发送请求直到超过预算
      2. 再发起 1 次请求
      3. 断言: 返回 quota_exceeded 错误且事件流可见对应事件
    Expected Result: 超限后稳定阻断
    Evidence: .sisyphus/evidence/task-11-usage-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-11-usage-happy.txt`
  - [ ] `.sisyphus/evidence/task-11-usage-error.txt`

  **Commit**: YES
  - Message: `feat(quota): add usage aggregation and budget enforcement`
  - Files: `crates/agentd-gateway/src/quota.rs`, `crates/agentd-store/src/*`, `crates/agentctl/src/*`
  - Pre-commit: `cargo test -p agentd-gateway -p agentctl`

- [ ] 12. Phase A 量化门禁与回滚演练

  **What to do**:
  - 将 Phase A 验收项脚本化：启动、注册、token 分配、请求成功、计量记录。
  - 定义失败阈值触发器：成功率、超时、错误分布。
  - 编写回滚手册与回滚脚本（恢复到 Wave 1 稳定点）。

  **Must NOT do**:
  - 不仅给“通过/失败”结论，必须输出量化指标。
  - 不做不可重放的手工回滚流程。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 门禁/回滚属于发布质量核心保障。
  - **Skills**: [`git-master`]
    - `git-master`: 需要精确标注回滚基线提交。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 此任务主为系统级脚本门禁。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2（后半段）
  - **Blocks**: T20
  - **Blocked By**: T3, T8, T10, T11

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase A 验收原始目标。
  - `design/phase3-delivery-plan-and-gates.md` - 门禁化思路。
  - `README.md` - MVP 价值主张（资源与审计可控）。

  **WHY Each Reference Matters**:
  - 把叙述性验收转为“硬门槛 + 回滚触发矩阵”。

  **Acceptance Criteria**:
  - [ ] Phase A gate script 在 CI required checks 中执行。
  - [ ] 指标报告含：启动时延、创建成功率、请求成功率、计量准确率。
  - [ ] 回滚脚本可在 10 分钟内恢复到前一稳定提交并验证通过。

  **QA Scenarios**:
  ```
  Scenario: Happy path — Phase A 全项通过
    Tool: Bash
    Preconditions: T3/T8/T10/T11 均完成
    Steps:
      1. 运行: bash scripts/gates/phase-a-gate.sh
      2. 读取输出指标 JSON
      3. 断言: 所有阈值满足（如成功率 >= 99%）
    Expected Result: Phase A 准入通过
    Failure Indicators: 任一阈值未达标
    Evidence: .sisyphus/evidence/task-12-phase-a-happy.json

  Scenario: Error path — 强制失败后执行回滚
    Tool: Bash
    Preconditions: 人为注入 One-API 不可达
    Steps:
      1. 运行: bash scripts/gates/phase-a-gate.sh
      2. 断言: 门禁失败
      3. 运行: bash scripts/rollback/phase-a-rollback.sh
      4. 断言: 回滚后基线检查通过
    Expected Result: 失败可自动回退并恢复稳定状态
    Evidence: .sisyphus/evidence/task-12-phase-a-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-12-phase-a-happy.json`
  - [ ] `.sisyphus/evidence/task-12-phase-a-error.txt`

  **Commit**: YES
  - Message: `test(gates): enforce phase-a quantitative gates and rollback drill`
  - Files: `scripts/gates/phase-a-gate.sh`, `scripts/rollback/phase-a-rollback.sh`, `.github/workflows/gates.yml`
  - Pre-commit: `bash scripts/gates/phase-a-gate.sh --local`

---

- [ ] 13. 策略引擎：allow/ask/deny + wildcard + 多层合并

  **What to do**:
  - 实现规则匹配（含通配符）与决策优先级（deny 优先）。
  - 实现三层合并：global → agent profile → session override。
  - 输出策略解释信息（命中规则、来源层级、最终决策）。

  **Must NOT do**:
  - 不引入 OPA/Rego（MVP 外）。
  - 不接受“无法解释”的黑盒策略结果。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 规则冲突与优先级推导复杂。
  - **Skills**: [`git-master`]
    - `git-master`: 策略语义变化需要强追溯。
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3（与 T14/T15/T16/T17 并行）
  - **Blocks**: T14, T16, T18, T19
  - **Blocked By**: T4

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase B 策略引擎目标。
  - `design/security-trust-and-audit-v1.md` - deny/ask 审计与安全语义。
  - `analysis/decisions/ADR-003-mvp-composition-architecture.md` - MVP 策略范围边界。

  **WHY Each Reference Matters**:
  - 策略引擎是“权限治理”核心，必须先把语义钉死。

  **Acceptance Criteria**:
  - [ ] 规则测试集通过率 100%（含冲突规则与 deny 优先）。
  - [ ] 合并策略可输出解释字段：`matched_rule`, `decision`, `source_layer`。
  - [ ] 被 deny 的调用稳定返回策略拒绝错误码。

  **QA Scenarios**:
  ```
  Scenario: Happy path — allow/ask 正常命中
    Tool: Bash
    Preconditions: 已配置 profile 策略（bash=ask, edit=allow）
    Steps:
      1. 发起 edit 工具请求
      2. 发起 bash 工具请求
      3. 断言: edit=allow，bash=ask，且解释字段完整
    Expected Result: 决策符合策略配置
    Failure Indicators: 决策与配置不一致或解释字段缺失
    Evidence: .sisyphus/evidence/task-13-policy-happy.txt

  Scenario: Error path — deny 优先覆盖 allow
    Tool: Bash
    Preconditions: 同时配置 `read:* = allow` 与 `read:*.env = deny`
    Steps:
      1. 发起 read:.env 请求
      2. 捕获返回结果与策略解释
      3. 断言: 最终 decision=deny，命中规则为 read:*.env
    Expected Result: deny 优先语义正确
    Evidence: .sisyphus/evidence/task-13-policy-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-13-policy-happy.txt`
  - [ ] `.sisyphus/evidence/task-13-policy-error.txt`

  **Commit**: YES
  - Message: `feat(policy): implement layered allow-ask-deny engine with wildcard precedence`
  - Files: `crates/agentd-policy/src/engine.rs`, `crates/agentd-policy/src/merge.rs`, `crates/agentd-core/src/policy.rs`
  - Pre-commit: `cargo test -p agentd-policy`

- [ ] 14. cgroup v2 + lifecycle 管理（fork/exec/restart）

  **What to do**:
  - 为每个 agent 创建独立 cgroup 并设置 `cpu.weight`、`memory.max`、`memory.high`。
  - 将 agent 进程放入对应 cgroup，接入健康检查与自动重启策略。
  - 记录资源异常事件（如 OOM）到统一事件流。

  **Must NOT do**:
  - 不降级为“仅记录，不限制”。
  - 不忽略 Ubuntu 25.10 上 cgroup v2 实际行为差异。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 内核资源控制 + 生命周期控制有较高系统复杂度。
  - **Skills**: [`git-master`]
    - `git-master`: 系统级改动需严格可回滚。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3
  - **Blocks**: T17, T18, T20
  - **Blocked By**: T4, T5, T13

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase B 明确 cgroup v2 目标与验收。
  - `design/runtime-and-governance-v1.md` - 资源治理语义。
  - Official docs: `https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html` - cgroup v2 行为。

  **WHY Each Reference Matters**:
  - 资源限制是“系统级运行时”价值核心之一，必须可验证。

  **Acceptance Criteria**:
  - [ ] 每个 agent 创建独立 cgroup 路径且参数写入成功。
  - [ ] 内存压测触发 `memory.max` 行为且事件流可见 `cgroup.oom`。
  - [ ] 进程异常退出后按策略自动重启并记录事件。

  **QA Scenarios**:
  ```
  Scenario: Happy path — cgroup 限制生效
    Tool: Bash
    Preconditions: 已启动受管 agent，配置 memory.max=256M
    Steps:
      1. 运行: cat /sys/fs/cgroup/agentd/<agent-id>/memory.max
      2. 运行 agent 轻负载任务
      3. 断言: memory.max 值正确，任务正常执行
    Expected Result: cgroup 参数正确并不影响正常任务
    Failure Indicators: 参数未写入或路径不存在
    Evidence: .sisyphus/evidence/task-14-cgroup-happy.txt

  Scenario: Error path — 内存超限触发 OOM 事件
    Tool: Bash
    Preconditions: 受管 agent 存在，注入超限内存任务
    Steps:
      1. 执行高内存占用任务
      2. 查询事件流/日志
      3. 断言: 出现 cgroup.oom 事件，进程按策略处理
    Expected Result: 超限行为可控且可观测
    Evidence: .sisyphus/evidence/task-14-cgroup-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-14-cgroup-happy.txt`
  - [ ] `.sisyphus/evidence/task-14-cgroup-error.txt`

  **Commit**: YES
  - Message: `feat(lifecycle): enforce cgroup-v2 isolation and managed restart`
  - Files: `crates/agentd-lifecycle/src/cgroup.rs`, `crates/agentd-lifecycle/src/manager.rs`, `crates/agentd-core/src/event.rs`
  - Pre-commit: `cargo test -p agentd-lifecycle`

- [ ] 15. 审计事件模型与持久化（事件完整性 100%）

  **What to do**:
  - 实现统一审计事件模型与写入管道。
  - 覆盖 MVP 关键事件：`agent.created`、`agent.started`、`llm.request`、`policy.deny`、`cgroup.oom` 等。
  - 建立 trace 关联：同一请求链路可按 `trace_id` 串联。

  **Must NOT do**:
  - 不允许关键字段缺失（`event_id/timestamp/agent_id/event_type/severity/payload/trace_id`）。
  - 不允许“最佳努力写入”导致静默丢失。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 审计链路是合规与问题追踪基础。
  - **Skills**: [`git-master`]
    - `git-master`: 事件 schema 演进需严谨。
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: 非浏览器。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3
  - **Blocks**: T16, T17, T19, T20
  - **Blocked By**: T4, T7

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase C 事件模型定义。
  - `design/security-trust-and-audit-v1.md` - 安全审计字段与信任链。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 观测接口契约。

  **WHY Each Reference Matters**:
  - 审计是“彻底验收”的关键证据来源。

  **Acceptance Criteria**:
  - [ ] 关键事件字段完整率 100%（抽样 500 条）。
  - [ ] `trace_id` 关联查询成功率 100%。
  - [ ] 审计写入失败时有告警/错误事件且主流程策略明确（阻断或降级）。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 关键事件全链路可追踪
    Tool: Bash
    Preconditions: 执行一次完整 create->request->tool 调用流程
    Steps:
      1. 触发流程
      2. 查询审计表按 trace_id 聚合
      3. 断言: 关键事件序列完整且字段齐全
    Expected Result: 单条链路可完整追踪
    Failure Indicators: 缺事件/缺字段/trace 断裂
    Evidence: .sisyphus/evidence/task-15-audit-happy.txt

  Scenario: Error path — 审计写入异常可观测
    Tool: Bash
    Preconditions: 人为制造 sqlite 写入受阻（锁冲突或只读）
    Steps:
      1. 触发会产生日志事件的请求
      2. 捕获错误事件与处理策略输出
      3. 断言: 出现明确错误，不发生静默丢失
    Expected Result: 写入故障可检测可追踪
    Evidence: .sisyphus/evidence/task-15-audit-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-15-audit-happy.txt`
  - [ ] `.sisyphus/evidence/task-15-audit-error.txt`

  **Commit**: YES
  - Message: `feat(audit): persist structured lifecycle events with trace correlation`
  - Files: `crates/agentd-store/src/audit.rs`, `crates/agentd-core/src/event.rs`, `migrations/*`
  - Pre-commit: `cargo test -p agentd-store`

---

- [ ] 16. 事件订阅流（UDS streaming）+ `agentctl events`

  **What to do**:
  - 在协议层实现 `SubscribeEvents` streaming（UDS 长连接）。
  - 在 CLI 提供 `agentctl events --follow` 实时消费。
  - 增加断线重连与游标恢复（最小可用）。

  **Must NOT do**:
  - 不降级为轮询文件日志替代事件流。
  - 不遗漏 backpressure/慢消费者保护。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 流式协议与稳定性控制需要额外谨慎。
  - **Skills**: [`git-master`]
    - `git-master`: 协议变化需清晰历史追踪。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3
  - **Blocks**: T19, T20
  - **Blocked By**: T6, T13, T15

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase C 要求 SubscribeEvents 与 events 命令。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 观测接口约束。
  - `design/protocols/agentd-protocol-profile-v1.md` - 协议栈和消息结构。

  **WHY Each Reference Matters**:
  - 事件流是审计“实时可见”的用户价值体现。

  **Acceptance Criteria**:
  - [ ] `agentctl events --follow` 可持续输出事件（5 分钟无崩溃）。
  - [ ] 断连后 5s 内自动重连并继续消费。
  - [ ] 慢消费者场景不导致 daemon 崩溃。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 实时事件流连续输出
    Tool: Bash
    Preconditions: daemon + audit 持久化可用
    Steps:
      1. 运行: agentctl events --follow > /tmp/events.log
      2. 触发 agent create 与 llm.request
      3. 断言: /tmp/events.log 出现对应事件且顺序合理
    Expected Result: 事件实时、连续、可读
    Failure Indicators: 无事件/顺序错乱/进程退出
    Evidence: .sisyphus/evidence/task-16-events-happy.txt

  Scenario: Error path — 人为断开连接后自动恢复
    Tool: Bash
    Preconditions: follow 进程已运行
    Steps:
      1. 模拟 UDS 连接中断
      2. 观察客户端重连日志
      3. 断言: 5s 内恢复并继续收到新事件
    Expected Result: 自动重连生效
    Evidence: .sisyphus/evidence/task-16-events-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-16-events-happy.txt`
  - [ ] `.sisyphus/evidence/task-16-events-error.txt`

  **Commit**: YES
  - Message: `feat(observe): add uds event subscription and follow mode`
  - Files: `crates/agentd-protocol/src/*`, `crates/agentctl/src/*`
  - Pre-commit: `cargo test -p agentd-protocol -p agentctl`

- [ ] 17. `agentctl usage` 成本查询 + Phase B/C 故障注入门禁

  **What to do**:
  - 完成 `agentctl usage <agent-id>` 成本分布与时间窗查询。
  - 建立 Phase B/C 门禁脚本：策略拒绝、资源超限、审计链路完整性。
  - 故障注入：One-API 超时、策略冲突、cgroup OOM、DB 锁冲突。

  **Must NOT do**:
  - 不将故障注入结果仅记录日志而不门禁。
  - 不忽略成本字段精度一致性。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 横跨策略/资源/审计多模块验证。
  - **Skills**: [`git-master`]
    - `git-master`: 需要明确“门禁失败对应回滚点”。
  - **Skills Evaluated but Omitted**:
    - `frontend-ui-ux`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3（后半段）
  - **Blocks**: T20
  - **Blocked By**: T3, T7, T11, T14, T15

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase C usage/cost 与事件验收目标。
  - `design/security-trust-and-audit-v1.md` - 拒绝/异常事件审计要求。
  - `design/runtime-and-governance-v1.md` - 资源超限治理语义。

  **WHY Each Reference Matters**:
  - B/C 不是“功能存在”，而是“故障时仍可控可证”。

  **Acceptance Criteria**:
  - [ ] `agentctl usage` 对 3 个时间窗查询结果一致且字段完整。
  - [ ] B/C gate 脚本可复跑，失败即阻断。
  - [ ] 故障注入 4 类场景均有可验证证据。

  **QA Scenarios**:
  ```
  Scenario: Happy path — usage 查询与 B/C 门禁通过
    Tool: Bash
    Preconditions: T11/T14/T15 完成
    Steps:
      1. 运行: agentctl usage e2e-agent --window 24h --json
      2. 运行: bash scripts/gates/phase-bc-gate.sh
      3. 断言: usage 字段完整，gate 全部 PASS
    Expected Result: B/C 阶段达到可发布门槛
    Failure Indicators: 字段缺失或 gate 失败
    Evidence: .sisyphus/evidence/task-17-bc-happy.txt

  Scenario: Error path — 注入 OOM 与策略冲突触发门禁失败
    Tool: Bash
    Preconditions: 可执行故障注入脚本
    Steps:
      1. 运行: bash scripts/faults/inject-oom-and-policy-conflict.sh
      2. 运行: bash scripts/gates/phase-bc-gate.sh
      3. 断言: gate 失败并输出失败条目（oom/policy）
    Expected Result: 门禁正确阻断问题版本
    Evidence: .sisyphus/evidence/task-17-bc-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-17-bc-happy.txt`
  - [ ] `.sisyphus/evidence/task-17-bc-error.txt`

  **Commit**: YES
  - Message: `test(phase-bc): add usage queries and fault-injection gates`
  - Files: `crates/agentctl/src/*`, `scripts/gates/phase-bc-gate.sh`, `scripts/faults/*`
  - Pre-commit: `bash scripts/gates/phase-bc-gate.sh --local`

- [ ] 18. Python `agentd-agent-lite`（uv）+ `agentctl agent run`

  **What to do**:
  - 基于 `uv` 建立 `agent-lite` 包与运行入口。
  - 实现最小循环：接收指令 → 调 LLM → 工具调用（受策略）→ 返回结果。
  - 与 `agentctl agent run --builtin lite` 集成，接入 cgroup 与审计事件。

  **Must NOT do**:
  - 不引入复杂第三方 Agent 框架替代（保持 lite）。
  - 不绕开策略引擎直接执行工具。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 需要跨 Python runtime 与 Rust 控制面联调。
  - **Skills**: [`git-master`]
    - `git-master`: 跨语言联调改动面大，需清晰提交边界。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 本任务非浏览器。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4
  - **Blocks**: T19, T20
  - **Blocked By**: T2, T6, T10, T13, T14

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase D 对内置 lite agent 的目标定义。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 运行与观测接口约束。
  - Official docs: `https://docs.astral.sh/uv/` - Python 环境管理与执行命令规范。

  **WHY Each Reference Matters**:
  - lite agent 是 MVP 端到端演示闭环关键，不可缺。

  **Acceptance Criteria**:
  - [ ] `agentctl agent run --builtin lite --name demo "..."` 可成功启动。
  - [ ] lite agent 的 LLM 请求被计量，工具调用受策略约束。
  - [ ] lite agent 进程可被 cgroup 限制并产生日志/审计事件。

  **QA Scenarios**:
  ```
  Scenario: Happy path — lite agent 完成一次任务循环
    Tool: Bash
    Preconditions: T10/T13/T14 已完成，One-API 可用
    Steps:
      1. 运行: agentctl agent run --builtin lite --name demo "分析当前目录结构"
      2. 查询: agentctl events --follow（短时）
      3. 断言: 出现 llm.request + tool 调用 + 完成事件
    Expected Result: lite agent 可在受管环境完成任务
    Failure Indicators: 任务卡死/无事件/策略绕过
    Evidence: .sisyphus/evidence/task-18-lite-happy.txt

  Scenario: Error path — 被 deny 的工具调用被拦截
    Tool: Bash
    Preconditions: 对 demo agent 配置 deny 规则（如 web_fetch=deny）
    Steps:
      1. 给出会触发 deny 工具的指令
      2. 观察返回与事件流
      3. 断言: 请求被拒绝，记录 policy.deny 事件
    Expected Result: 策略生效且可观测
    Evidence: .sisyphus/evidence/task-18-lite-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-18-lite-happy.txt`
  - [ ] `.sisyphus/evidence/task-18-lite-error.txt`

  **Commit**: YES
  - Message: `feat(agent-lite): add uv-managed builtin lite agent runtime integration`
  - Files: `python/agentd-agent-lite/*`, `crates/agentctl/src/*`, `configs/agents/example.toml`
  - Pre-commit: `uv run pytest -q && cargo test -p agentctl`

---

- [ ] 19. A2A Agent Card 生成 + 端到端演示脚本

  **What to do**:
  - 在 Agent 注册成功后自动生成 `agent.json`（A2A card 兼容最小字段）。
  - 编写端到端演示脚本：创建 agent → 启动 lite → 触发任务 → 查看事件/用量。
  - 将演示脚本纳入 CI 可执行 smoke 流程。

  **Must NOT do**:
  - 不扩展完整 A2A 全协议（MVP 外）。
  - 不用手工步骤替代可执行脚本。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 需要跨控制面、观测面和文档契约联动。
  - **Skills**: [`git-master`]
    - `git-master`: 端到端脚本应保持原子演进。
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: 非浏览器。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 4
  - **Blocks**: T20
  - **Blocked By**: T10, T13, T15, T16, T18

  **References**:
  - `design/mvp-implementation-roadmap-v1.md` - Phase D 要求 A2A card 与演示。
  - `design/protocols/agentd-protocol-profile-v1.md` - 协议兼容边界。
  - `design/interfaces/agentd-control-and-observation-interfaces-v1.md` - 端到端观测接口。

  **WHY Each Reference Matters**:
  - card 与演示脚本构成“可对外复现”的 MVP 证明材料。

  **Acceptance Criteria**:
  - [ ] 注册后可在预期路径找到 `agent.json`，字段完整。
  - [ ] 演示脚本可在 Ubuntu 25.10 一键执行并成功。
  - [ ] 演示脚本结果包含事件与用量输出摘要。

  **QA Scenarios**:
  ```
  Scenario: Happy path — 一键演示成功
    Tool: Bash
    Preconditions: T10/T16/T18 已完成
    Steps:
      1. 运行: bash scripts/demo/e2e-demo.sh
      2. 检查输出中的 agent.json 路径与摘要
      3. 断言: create/run/events/usage 全部成功
    Expected Result: 端到端演示完整通过
    Failure Indicators: 任一步骤失败或输出缺失
    Evidence: .sisyphus/evidence/task-19-demo-happy.txt

  Scenario: Error path — card 缺字段被门禁拦截
    Tool: Bash
    Preconditions: 人为移除 agent.json 必填字段
    Steps:
      1. 运行: bash scripts/validate/agent-card-validate.sh
      2. 断言: 校验失败并输出缺失字段
      3. 运行 demo 脚本确认被阻断
    Expected Result: 非法 card 无法进入演示流程
    Evidence: .sisyphus/evidence/task-19-demo-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-19-demo-happy.txt`
  - [ ] `.sisyphus/evidence/task-19-demo-error.txt`

  **Commit**: YES
  - Message: `feat(phase-d): generate a2a agent card and executable e2e demo`
  - Files: `scripts/demo/e2e-demo.sh`, `scripts/validate/agent-card-validate.sh`, `configs/agents/*`
  - Pre-commit: `bash scripts/demo/e2e-demo.sh --dry-run`

- [ ] 20. 最终硬化（public 仓库安全门禁 + 发布候选 + 总回滚演练）

  **What to do**:
  - 完成 public 仓库场景的安全硬化：secret scanning、依赖漏洞扫描、最小权限 token。
  - 汇总 Phase A~D 所有门禁为 release-candidate pipeline。
  - 执行“全链路失败注入 → 回滚 → 复验”总演练。

  **Must NOT do**:
  - 不允许存在高危漏洞未处理即标记 RC 通过。
  - 不允许回滚流程仅文档化不实操。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 这是最终发布质量关口，涉及全局联动。
  - **Skills**: [`git-master`, `conventional-commits`]
    - `git-master`: 硬化与回滚依赖严格变更控制。
    - `conventional-commits`: 发布候选提交语义清晰。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非主要路径。

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Sequential（Wave 4 收口任务）
  - **Blocks**: F1, F2, F3, F4
  - **Blocked By**: T1, T3, T12, T14, T15, T16, T17, T18, T19

  **References**:
  - `design/security-trust-and-audit-v1.md` - 安全与审计硬化基准。
  - `design/phase3-delivery-plan-and-gates.md` - Gate 汇总与发布门禁思想。
  - Official docs: `https://docs.github.com/en/code-security` - GitHub code security 能力。

  **WHY Each Reference Matters**:
  - public 仓库暴露面更大，必须在发布前强化默认防护。

  **Acceptance Criteria**:
  - [ ] 发布候选流水线一次性跑通，required checks 全绿。
  - [ ] secret scanning + 漏洞扫描结果达到门槛（Critical/High = 0）。
  - [ ] 总回滚演练在 15 分钟内完成并恢复至稳定基线。

  **QA Scenarios**:
  ```
  Scenario: Happy path — RC 流水线全绿
    Tool: Bash
    Preconditions: T1~T19 完成
    Steps:
      1. 运行: bash scripts/release/rc-gate.sh
      2. 运行: gh pr checks <PR_NUMBER> --required
      3. 断言: 所有 required checks PASS 且安全检查无高危
    Expected Result: RC 可进入最终验证波次
    Failure Indicators: 任一 required check 失败或高危漏洞>0
    Evidence: .sisyphus/evidence/task-20-rc-happy.txt

  Scenario: Error path — 注入 secret 泄漏后阻断并回滚
    Tool: Bash
    Preconditions: 可控测试分支用于故障注入
    Steps:
      1. 注入模拟 secret 到测试文件并触发扫描
      2. 断言: 安全门禁失败，PR 被阻断
      3. 执行: bash scripts/rollback/final-rollback.sh
      4. 断言: 回滚后门禁恢复通过
    Expected Result: public 场景泄漏风险可被及时阻断和恢复
    Evidence: .sisyphus/evidence/task-20-rc-error.txt
  ```

  **Evidence to Capture**:
  - [ ] `.sisyphus/evidence/task-20-rc-happy.txt`
  - [ ] `.sisyphus/evidence/task-20-rc-error.txt`

  **Commit**: YES
  - Message: `chore(release): harden public repo gates and verify rollback readiness`
  - Files: `.github/workflows/*`, `scripts/release/*`, `scripts/rollback/*`
  - Pre-commit: `bash scripts/release/rc-gate.sh --local`

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

- [ ] F1. **Plan Compliance Audit** — `oracle`
  - 对照本计划逐项核验 Must Have / Must NOT Have
  - 验证每个任务证据文件存在且可追溯
  - 输出：`Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  - 运行 `cargo check --workspace`、`cargo clippy --workspace -- -D warnings`、`uv run pytest`
  - 检查不允许项（`as any`、空 catch、未使用代码、临时代码）
  - 输出：`Build/Lint/Test 状态 + 文件级问题列表 + VERDICT`

- [ ] F3. **Real QA Replay** — `unspecified-high`
  - 复跑所有任务的 QA Scenarios，验证证据闭环
  - 覆盖 happy-path + error-path + 跨任务集成
  - 输出：`Scenarios [N/N] | Integration [N/N] | Edge Cases [N] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  - 对照任务说明与实际变更，确认零范围漂移
  - 检查是否误引入 Go、多机、MVP 外扩功能
  - 输出：`Tasks compliant [N/N] | Scope creep [0/N] | VERDICT`

---

## Commit Strategy

- **Commit 1 (Wave 1)**: `chore(bootstrap): establish repo/toolchain/ci skeleton`
- **Commit 2 (Wave 2)**: `feat(phase-a): deliver managed one-api and agent registration flow`
- **Commit 3 (Wave 3)**: `feat(phase-bc): add policy isolation and audit observability`
- **Commit 4 (Wave 4)**: `feat(phase-d): ship agent-lite and end-to-end demo`
- **Commit 5 (Hardening)**: `chore(release): enforce security gates and rollback readiness`

---

## Success Criteria

### Verification Commands（示例）
```bash
uv sync --frozen                              # exit 0
cargo check --workspace                       # exit 0
cargo test --workspace                        # all pass
uv run pytest -q                              # all pass
gh pr checks <PR_NUMBER> --required           # all required checks pass
```

### Final Checklist
- [ ] 所有 Must Have 均有证据
- [ ] 所有 Must NOT Have 均未出现
- [ ] 阶段门禁全部通过
- [ ] 回滚演练通过且可重复
- [ ] Final Wave（F1-F4）全部 APPROVE
