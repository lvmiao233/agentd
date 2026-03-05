# Post-MVP 路线图执行细化计划（并行无干扰版）

## TL;DR

> **Quick Summary**: 在 MVP 基础上，于约 20 周内完成 Post-MVP 双轨能力交付：Track A 打通 MCP→OPA/Rego→Firecracker→A2A→跨设备发现/迁移，Track B 完成 agent-lite 工具与会话增强、交互式 TUI 与 Web UI，形成“受管、可审计、可隔离、可协作”的生产级 Agent 平台。  
>
> **Deliverables**:
> - **P1**：MCP Host/Gateway/Registry + 内置 MCP Server + agent-lite 动态工具发现 + OPA/Rego 基线
> - **P2**：Firecracker microVM 隔离 + Runtime Selector + 完整 A2A + 交互式 TUI
> - **P3**：跨设备发现/注册 + L1/L2 上下文迁移
> - **P4**：WebSocket Bridge + Agent Shell Web UI（Chat/Dashboard/Events/Usage/Settings）
>
> **Estimated Effort**: XL（20 周路线图的工程化细化）  
> **Parallel Execution**: YES（5 waves + final verification）  
> **Critical Path**: 1 → 7 → 9 → 12 → 13 → 15 → 16 → 22 → 24 → 26 → 27 → Final

---

## Context

### Product Baseline（来自初始路线图）
MVP 已交付 daemon/CLI/agent-lite/策略审计闭环，Post-MVP 目标是在此基础上进入双轨并行演进：
- **Track A（基础设施）**：MCP → OPA/Rego → Firecracker → A2A → 跨设备发现 → 上下文迁移
- **Track B（Agent 体验）**：工具生态 → 交互式 CLI/TUI → MCP 工具体验深化 → Web UI

### Product Direction
- **MCP-first**：MCP 是 Track A/B 的交汇点与首要落地点
- **可增量交付**：每个 Sprint 必须形成可独立验收闭环
- **向后兼容**：新能力不破坏 MVP 已有链路
- **分级隔离不妥协**：高风险 Agent 必须落在 microVM 隔离

### Planning Constraints（执行约束）
- 任务拆分到工程任务级，支持 4-6 Agent 并发
- 依赖优先跨 Sprint，前提是依赖/契约/QA 准入全部满足
- 每个任务应声明影响范围与边界（默认遵守）；必要时可偏离，但需在提交说明或 merge note 记录原因与影响

---

## Work Objectives

### Core Objective
在保持向后兼容与安全治理前提下，完整交付 Post-MVP 双轨能力：
1) 建立标准化、可策略化、可隔离的 Agent 运行基础设施；
2) 提供可用于真实开发与协作的 Agent 交互体验（CLI/TUI/Web）。

### Concrete Deliverables
- **P1**：MCP Host/Gateway/Registry + 内置 MCP Server + agent-lite 动态工具发现 + OPA/Rego 基线
- **P2**：Firecracker 隔离（含 runtime selector/jailer/network）+ 完整 A2A + 交互式 TUI
- **P3**：跨设备发现注册 + L1/L2 上下文迁移
- **P4**：WebSocket Bridge + Agent Shell Web UI（Chat/Dashboard/Events/Usage/Settings）

### Definition of Done
- [ ] Track A 与 Track B 的目标能力均有可验证交付（非占位）
- [ ] 各阶段门禁（策略、隔离、协议、体验）均可通过自动化命令验证
- [ ] 每个任务有明确依赖/影响范围/边界声明（soft guardrail）/QA 证据
- [ ] Final verification wave 全部 APPROVE

### Must Have
- 路线图主键映射（`T-Ax/T-Bx`）到执行任务
- MCP-first 实施顺序与双轨交汇点优先级
- 向后兼容（MVP 链路）与策略审计完整性
- 高风险运行时必须具备 microVM 隔离落地路径

### Must NOT Have (Guardrails)
- 默认避免“顺手扩展”超出任务定义；若发生扩展，建议在提交说明中写明目的与影响
- 默认应提供 `Allowed/Forbidden/Shared` 边界清单；若暂缺，可先推进开发并在合并前补齐
- 验收优先机器可执行；必要时可附人工观察结论，但不替代关键自动化验证
- 优先在依赖/契约稳定后跨 Sprint 拉起任务；若提前启动，应限定为预备开发并在合并前校验关键项

### Scope-Delta Control（范围变更控制）

`scope-delta` 为可选记录机制。建议在跨任务触达或偏离原任务边界时记录：
- `why`: 为什么必须扩展
- `impact`: 影响哪些任务/契约/测试
- `risk`: 新增风险与回滚策略
- `approver`: 谁批准（负责人）

未记录 scope-delta 不视为违规，也不自动阻塞开发；Merge Agent 可在合并备注补记。

---

## Verification Strategy (MANDATORY)

> **ZERO HUMAN INTERVENTION**：所有验收必须由 Agent 通过命令执行并产出证据。

### Test Decision
- **Infrastructure exists**: YES（Rust + Python + CI gates）
- **Automated tests**: TDD
- **Framework**: `cargo test` + `pytest` + gate scripts
- **TDD policy**: 每任务都要有 RED→GREEN→REFACTOR 轨迹

### QA Policy
每个任务都必须提供：
1. Happy path 场景（应成功）
2. Failure/edge 场景（应被正确拒绝或降级处理）
3. 证据文件输出到 `.sisyphus/evidence/task-{id}-*.{txt|json|png}`

### Acceptance Criteria Hardening（新增强制条款，防止“只看测试通过”）

> 从本版本开始，**任何任务都不得仅以单元测试通过判定完成**。  
> 每个任务的验收必须同时满足：

1. **Contract/Test Gate**：原有单元/集成测试通过（RED→GREEN 保留）
2. **Functional Replay Gate**：执行真实功能链路（至少 1 条 happy + 1 条 failure）
3. **Evidence Gate**：功能链路证据写入 `.sisyphus/evidence/task-{id}-*`

若缺失 Functional Replay Gate，即使测试全部通过，也判定为 **NOT DONE**。

#### Task-level Functional Replay Matrix（Task 1-28）

| Task | Functional Replay（必须执行） |
|---|---|
| 1 | `cargo test -p agentd-daemon mcp_config_loads_all_servers -- --exact` + `cargo test -p agentd-daemon mcp_config_rejects_invalid_transport -- --exact` |
| 2 | `cargo test -p agentd-daemon mcp_registry_roundtrip_entry -- --exact` + `cargo test -p agentd-daemon mcp::tests::mcp_registry_rejects_unknown_trust -- --exact` |
| 3 | `cargo test -p agentd-daemon tests::authorize_mcp_tool_allow_forwards -- --exact` + `cargo test -p agentd-daemon tests::authorize_mcp_tool_deny_blocks_forward -- --exact` |
| 4 | `uv run pytest python/agentd-agent-lite/tests/test_session_tree.py` + orphan/invalid parent case replay |
| 5 | `cargo test -p agentd-daemon rego_policy_loaded_and_evaluated -- --exact` + invalid context/input case replay |
| 6 | `cargo test -p agentctl shell_command_routes_to_tui -- --exact` + invalid flag/quit flow replay |
| 7 | `cargo test -p agentd-daemon mcp_host_starts_declared_servers -- --exact` + `cargo test -p agentd-daemon mcp_host_rolls_back_on_init_failure -- --exact` |
| 8 | `cargo test -p agentd-daemon mcp_registry_syncs_capabilities_from_initialize -- --exact` + `cargo test -p agentd-daemon unhealthy_server_removed_from_available_tools -- --exact` |
| 9 | `cargo test -p agentd-daemon list_available_tools_filters_by_policy -- --exact` + `cargo test -p agentd-daemon invoke_skill_denied_writes_audit -- --exact` |
| 10 | `uv run pytest python/agentd-mcp-fs/tests/test_tools.py python/agentd-mcp-shell/tests/test_tools.py` + policy deny replay |
| 11 | `uv run pytest python/agentd-mcp-search/tests/test_tools.py python/agentd-mcp-git/tests/test_tools.py` + git patch error replay |
| 12 | `uv run pytest python/agentd-agent-lite/tests/test_tool_discovery.py python/agentd-agent-lite/tests/test_third_party_mcp.py` + policy filtered replay |
| 13 | `uv run pytest python/agentd-agent-lite/tests/test_multi_turn.py python/agentd-agent-lite/tests/test_tool_loop.py` + context budget edge replay |
| 14 | `uv run pytest python/agentd-agent-lite/tests/test_compact.py python/agentd-agent-lite/tests/test_session_persistence.py` + corrupted session replay |
| 15 | `cargo test -p agentd-daemon rego_policy_loaded_and_evaluated -- --exact` + `cargo test -p agentd-daemon rego_invalid_policy_compile_error -- --exact` |
| 16 | `cargo test -p agentd-daemon toml_to_rego_equivalence_suite -- --exact` + `cargo test -p agentd-daemon toml_to_rego_rejects_unsupported_construct -- --exact` |
| 17 | `cargo test -p agentd-daemon rego_hot_reload_without_restart -- --exact` + `cargo test -p agentd-daemon rego_reload_bad_policy_keeps_previous_engine -- --exact` |
| 18 | `cargo test -p agentctl approval_queue_roundtrip -- --exact` + `cargo test -p agentctl slash_commands_core_set_available -- --exact` |
| 19 | `bash scripts/firecracker/build-rootfs.sh` + `bash scripts/firecracker/verify-rootfs.sh` |
| 20 | `cargo test -p agentd-daemon firecracker_executor_launches_vm -- --exact` + `cargo test -p agentd-daemon firecracker_launch_timeout_returns_stable_error -- --exact` |
| 21 | `cargo test -p agentd-daemon untrusted_agent_uses_firecracker_runtime -- --exact` + `cargo test -p agentd-daemon jailer_policy_blocks_forbidden_network -- --exact` |
| 22 | `cargo test -p agentd-daemon a2a_server_task_crud_and_stream -- --exact` + `cargo test -p agentd-daemon a2a_state_machine_rejects_completed_to_working -- --exact` |
| 23 | `cargo test -p agentd-daemon a2a_client_discovers_remote_card -- --exact` + `cargo test -p agentctl a2a_discover_handles_unreachable_remote -- --exact` |
| 24 | `cargo test -p agentd-daemon orchestrator_splits_and_aggregates_tasks -- --exact` + `cargo test -p agentd-daemon orchestrator_retries_failed_child_once -- --exact` |
| 25 | `cargo test -p agentd-daemon mdns_peer_discovery_finds_remote_agent -- --exact` + registry down replay |
| 26 | `cargo test -p agentd-daemon semantic_migration_l1_continues_workflow -- --exact` + `cargo test -p agentd-daemon migration_failure_rolls_back_source_session -- --exact` |
| 27 | `cargo test -p agentd-daemon ws_bridge_forwards_rpc_and_stream -- --exact` + `node web/agent-shell/tests/run-tests.mjs chat-page-streaming.spec.ts` |
| 28 | `corepack pnpm --filter agent-shell build` + `node web/agent-shell/tests/run-tests.mjs dashboard-events.spec.ts` + Settings/Tools onboarding/manual RPC replay |


---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (foundation contracts): Tasks 1-6
Wave 2 (MCP runtime + builtin tools): Tasks 7-12
Wave 3 (agent-lite + policy depth): Tasks 13-18
Wave 4 (sandbox + A2A core): Tasks 19-23
Wave 5 (orchestration + discovery + migration + web): Tasks 24-28
Wave FINAL (independent verification): F1-F4
```

### Worktree 并行假设（merge-stage 冲突模型）

- 开发阶段在独立 worktree 中推进，默认互不阻塞
- 冲突主要发生在 merge 阶段（而非开发阶段）
- 默认由 Merge Agent 自治解决并记录（merge note / conflict note）
- 无需等待人工接管；仅在安全或契约重大风险下再升级决策

### Pull-forward Eligibility Gate（跨 Sprint 提前启动准入）

以下 4 条是“优先满足”的准入条件（建议）：
- 依赖任务全部完成（含 contract 依赖）
- 关联契约文件处于稳定窗口（无待合并 breaking change）
- 指定 owner 已就绪（避免无 owner 抢占）
- 对应 QA harness 已可执行（至少能跑 RED）

若未全部满足，可先开展预备开发；进入合并前应校验关键项（依赖闭环 + 契约关键校验通过）。

### Shared-File Ownership Preference（共享高冲突文件，软约束）

以下文件/目录为高冲突区域，建议由首选责任任务优先修改：
- `crates/agentd-protocol/src/rpc.rs`
- `crates/agentd-protocol/src/v1.rs`
- `web/agent-shell/lib/*schema*`
- `configs/mcp-servers/*.toml`

并行修改允许发生；若出现冲突，由 Merge Agent 在合并阶段统一仲裁并记录。

### Dependency Matrix (FULL)

| Task | Depends On | Dependency Type | Blocks |
|---|---|---|---|
| 1 | — | — | 7,10,11 |
| 2 | — | — | 7,8 |
| 3 | — | — | 8,9 |
| 4 | — | — | 13,14 |
| 5 | — | — | 15 |
| 6 | — | — | 17 |
| 7 | 1,2 | code/contract | 8,9,12 |
| 8 | 2,3,7 | code/contract | 9,12,18 |
| 9 | 3,7,8 | code/contract | 12,13,18 |
| 10 | 1 | code | 12 |
| 11 | 1 | code | 12 |
| 12 | 7,8,9,10,11 | code/contract | 13,17,27,28 |
| 13 | 4,12 | code/contract | 14,18,27 |
| 14 | 4,13 | code | 26 |
| 15 | 5 | infra/contract | 16,21 |
| 16 | 15 | contract/test-data | 18,21 |
| 17 | 6,12 | code | 18,24 |
| 18 | 8,9,13,17 | code/decision | 24,28 |
| 19 | — | infra | 20 |
| 20 | 19 | infra/code | 21 |
| 21 | 15,16,20 | code/contract/infra | 22,23 |
| 22 | 21 | code/contract | 23,24 |
| 23 | 21,22 | code/contract | 24,25,26 |
| 24 | 22,23,17,18 | code/contract | 26,27,28 |
| 25 | 23 | infra/contract | 26 |
| 26 | 14,24,25 | code/contract | 28 |
| 27 | 12,24 | code/contract | 28 |
| 28 | 12,18,26,27 | code/contract/test-data | F1-F4 |

### Agent Dispatch Summary

- **Wave 1（6）**: `quick/unspecified-high/deep` 混合，先打合同与边界
- **Wave 2（6）**: MCP Host + Gateway + 内置工具并发构建
- **Wave 3（6）**: agent-lite 会话与策略深水区并行
- **Wave 4（5）**: sandbox 与 A2A 核心推进
- **Wave 5（5）**: 编排/发现/迁移/Web 收口
- **Final（4）**: 独立审查并行阻塞门

---

## TODOs

> 说明：每个任务都带有 Source Mapping（映射回路线图任务）、Impact Scope、Boundary Control、QA。

### Boundary 语义（Soft Guardrail）

- 任务里的 `Forbidden Paths` 表示“高风险触达清单”，默认不改
- 如确有必要可最小化改动，并在提交说明或 merge note 记录原因与影响
- 发生越界不自动阻塞，但会在 Final Scope Fidelity 中重点审查

---

- [ ] 1. **T-A1.1 MCP 配置契约与加载入口**

  **Source Mapping**: `T-A1`  
  **What to do**:
  - 定义 MCP server 配置结构与校验（name/command/args/transport/trust_level）
  - 实现 `configs/mcp-servers/*.toml` 扫描解析入口（先加载，不启动）
  - TDD 覆盖：合法配置、缺字段、非法 transport

  **Impact Scope**: `crates/agentd-daemon/src/mcp.rs(新增)`, `crates/agentd-daemon/src/main.rs`, `configs/mcp-servers/`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/mcp.rs`, `crates/agentd-daemon/src/main.rs`, `configs/mcp-servers/`
  - **Forbidden Paths**: `crates/agentctl/`, `python/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（只读）

  **Recommended Agent Profile**: `quick`  
  **Parallelization**: Wave 1, Blocks 7/10/11, Blocked By None

  **References**:
  - `design/post-mvp-roadmap-v1.md:136-140`（MCP 启动扫描流程）
  - `crates/agentd-daemon/src/main.rs`（daemon 启动挂载点）

  **Acceptance Criteria**:
  - [ ] RED→GREEN: `cargo test -p agentd-daemon mcp_config_rejects_invalid_transport -- --exact`
  - [ ] `cargo test -p agentd-daemon mcp_config_loads_all_servers -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: valid config load
    Tool: Bash
    Steps: run cargo test -p agentd-daemon mcp_config_loads_all_servers -- --exact
    Expected: test result ok
    Evidence: .sisyphus/evidence/task-1-valid-load.txt

  Scenario: invalid transport rejected
    Tool: Bash
    Steps: run cargo test -p agentd-daemon mcp_config_rejects_invalid_transport -- --exact
    Expected: contains "invalid transport"
    Evidence: .sisyphus/evidence/task-1-invalid-transport.txt
  ```

  **Commit**: YES — `feat(daemon): add mcp config loader`

- [ ] 2. **T-A3.1 MCP Registry 数据模型与信任元数据**

  **Source Mapping**: `T-A3`  
  **What to do**:
  - 定义 Registry entry（server_id/capabilities/trust_level/health）
  - 提供内存态 CRUD（先不接进程生命周期）
  - 增加 trust level 合法性测试

  **Impact Scope**: `crates/agentd-daemon/src/mcp.rs`, `crates/agentd-core/src/profile.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/mcp.rs`, `crates/agentd-core/src/profile.rs`
  - **Forbidden Paths**: `python/`, `web/`, `crates/agentctl/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（只读）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 1, Blocks 7/8, Blocked By None

  **References**:
  - `design/post-mvp-roadmap-v1.md:110-113`（Registry 职责）
  - `crates/agentd-core/src/profile.rs`（trust/policy 风格）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon mcp::tests::mcp_registry_roundtrip_entry -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon mcp::tests::mcp_registry_rejects_unknown_trust -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: register capability entry
    Tool: Bash
    Steps: run cargo test -p agentd-daemon mcp::tests::mcp_registry_roundtrip_entry -- --exact
    Expected: entry count and capability digest match
    Evidence: .sisyphus/evidence/task-2-registry-roundtrip.txt

  Scenario: reject invalid trust level
    Tool: Bash
    Steps: run cargo test -p agentd-daemon mcp::tests::mcp_registry_rejects_unknown_trust -- --exact
    Expected: validation error returned
    Evidence: .sisyphus/evidence/task-2-invalid-trust.txt
  ```

  **Commit**: YES — `feat(daemon): add mcp registry model`

- [ ] 3. **T-A2.1 Policy Gateway 拦截点与决策接口**

  **Source Mapping**: `T-A2`  
  **What to do**:
  - 在 MCP 调用链路加入前置授权拦截（authorize-before-forward）
  - 统一决策结构（allow/ask/deny + reason + trace_id）
  - deny 场景落审计并可回放

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-core/src/policy.rs`, `crates/agentd-core/src/audit.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-core/src/policy.rs`, `crates/agentd-core/src/audit.rs`
  - **Forbidden Paths**: `python/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（只读）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 1, Blocks 8/9, Blocked By None

  **References**:
  - `design/post-mvp-roadmap-v1.md:111-112`（Gateway 语义）
  - `crates/agentd-core/src/policy.rs`（当前判定逻辑）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon tests::authorize_mcp_tool_deny_blocks_forward -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon tests::authorize_mcp_tool_writes_audit_event -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: allow forwards to downstream
    Tool: Bash
    Steps: run cargo test -p agentd-daemon tests::authorize_mcp_tool_allow_forwards -- --exact
    Expected: downstream invoke count = 1
    Evidence: .sisyphus/evidence/task-3-allow-forward.txt

  Scenario: deny blocks and audits
    Tool: Bash
    Steps: run cargo test -p agentd-daemon tests::authorize_mcp_tool_deny_blocks_forward -- --exact
    Expected: downstream invoke count = 0 and audit reason present
    Evidence: .sisyphus/evidence/task-3-deny-audit.txt
  ```

  **Commit**: YES — `feat(policy): add mcp gateway interception`

- [ ] 4. **T-B6.1 AgentSession 树形消息模型（id/parent_id）**

  **Source Mapping**: `T-B6`  
  **What to do**:
  - 引入 `AgentSession.messages/head_id` 与 parent 链追踪
  - 实现 `_append_message`、`_get_active_branch`
  - 增加分支回溯与顺序恢复测试

  **Impact Scope**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/`  
  **Boundary Control**:
  - **Allowed Paths**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/`
  - **Forbidden Paths**: `crates/`, `web/`
  - **Shared Contract Files**: `python/agentd-agent-lite/src/agentd_agent_lite/config.py`（只读）

  **Recommended Agent Profile**: `quick`  
  **Parallelization**: Wave 1, Blocks 13/14, Blocked By None

  **References**:
  - `design/post-mvp-roadmap-v1.md:314-350`（会话结构目标）
  - `python/agentd-agent-lite/tests/test_tool_loop.py`（测试风格）

  **Acceptance Criteria**:
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_session_tree.py::test_append_message_sets_parent -q` PASS
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_session_tree.py::test_get_active_branch_returns_ordered_chain -q` PASS

  **QA Scenarios**:
  ```
  Scenario: branch chain reconstruction
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_session_tree.py::test_get_active_branch_returns_ordered_chain -q
    Expected: ordered chain root->...->head
    Evidence: .sisyphus/evidence/task-4-branch-order.txt

  Scenario: orphan parent handled
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_session_tree.py::test_get_active_branch_handles_missing_parent -q
    Expected: stable error or graceful fallback
    Evidence: .sisyphus/evidence/task-4-orphan-parent.txt
  ```

  **Commit**: YES — `feat(agent-lite): add session tree model`

- [ ] 5. **T-A4.1 Rego 引擎抽象层与输入上下文模型**

  **Source Mapping**: `T-A4`  
  **What to do**:
  - 抽象 `PolicyEngine` trait（evaluate/load/reload/explain）
  - 定义输入上下文结构（agent/tool/resource/time/request_meta）
  - 为后续 regorus 接入预留统一接口测试

  **Impact Scope**: `crates/agentd-core/src/policy.rs`, `crates/agentd-daemon/src/main.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-core/src/policy.rs`, `crates/agentd-daemon/src/main.rs`
  - **Forbidden Paths**: `python/`, `web/`, `crates/agentctl/`
  - **Shared Contract Files**: `policies/`（仅读取结构，不写策略内容）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 1, Blocks 15, Blocked By None

  **References**:
  - `design/post-mvp-roadmap-v1.md:215-223`（Policy Gateway + regorus 关系）
  - `crates/agentd-core/src/policy.rs`（现有策略接口）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-core policy_engine_trait_contract -- --exact` PASS
  - [ ] `cargo test -p agentd-core policy_input_context_roundtrip -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: evaluate contract path
    Tool: Bash
    Steps: run cargo test -p agentd-core policy::tests::policy_engine_trait_contract -- --exact
    Expected: trait mock passes all required methods
    Evidence: .sisyphus/evidence/task-5-policy-trait.txt

  Scenario: missing context field rejected
    Tool: Bash
    Steps: run cargo test -p agentd-core policy::tests::policy_input_context_missing_tool_rejected -- --exact
    Expected: deterministic validation error
    Evidence: .sisyphus/evidence/task-5-invalid-context.txt
  ```

  **Commit**: YES — `refactor(policy): introduce engine abstraction`

- [ ] 6. **T-B10.1 agentctl shell 子命令与 TUI 框架脚手架**

  **Source Mapping**: `T-B10`  
  **What to do**:
  - 增加 `agentctl agent shell` 子命令路由
  - 建立 ratatui app skeleton（input panel/message panel/status bar）
  - 空实现下完成键盘输入与退出循环

  **Impact Scope**: `crates/agentctl/src/main.rs`, `crates/agentctl/src/tui.rs(新增)`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentctl/src/main.rs`, `crates/agentctl/src/tui.rs`
  - **Forbidden Paths**: `crates/agentd-daemon/`, `python/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（只读）

  **Recommended Agent Profile**: `quick`  
  **Parallelization**: Wave 1, Blocks 17, Blocked By None

  **References**:
  - `design/post-mvp-roadmap-v1.md:396-408`（TUI 结构建议）
  - `crates/agentctl/src/main.rs`（CLI 命令分发入口）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentctl shell_command_routes_to_tui -- --exact` PASS
  - [ ] `cargo test -p agentctl tui_app_handles_quit_key -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: shell command enters TUI loop
    Tool: interactive_bash (tmux)
    Steps: run `agentctl agent shell`, send key `q`
    Expected: process exits cleanly with code 0
    Evidence: .sisyphus/evidence/task-6-shell-quit.txt

  Scenario: invalid flag rejected
    Tool: Bash
    Steps: run `agentctl agent shell --bad-flag`
    Expected: non-zero exit + usage output
    Evidence: .sisyphus/evidence/task-6-invalid-flag.txt
  ```

  **Commit**: YES — `feat(agentctl): scaffold interactive shell tui`

- [ ] 7. **T-A1.2 MCP Host 生命周期管理（启动/停止/健康）**

  **Source Mapping**: `T-A1`  
  **What to do**:
  - 基于任务 1/2 实现 server process 启停与健康探测
  - 完成 initialize 握手调用并缓存 server handle
  - 启动失败可回滚并记录审计

  **Impact Scope**: `crates/agentd-daemon/src/mcp.rs`, `crates/agentd-daemon/src/main.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/mcp.rs`, `crates/agentd-daemon/src/main.rs`
  - **Forbidden Paths**: `crates/agentctl/`, `python/`, `web/`
  - **Shared Contract Files**: `configs/mcp-servers/`（可读写配置示例，不改全局默认）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 2, Blocks 8/9/12, Blocked By 1/2

  **References**:
  - `design/post-mvp-roadmap-v1.md:136-139`（MCP host 关键流程）
  - `crates/agentd-daemon/src/lifecycle.rs`（现有生命周期处理模式）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon mcp_host_starts_declared_servers -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon mcp_host_rolls_back_on_init_failure -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: startup initializes servers
    Tool: Bash
    Steps: run cargo test -p agentd-daemon mcp_host_starts_declared_servers -- --exact
    Expected: registered server count equals config count
    Evidence: .sisyphus/evidence/task-7-host-start.txt

  Scenario: handshake failure rollback
    Tool: Bash
    Steps: run cargo test -p agentd-daemon mcp_host_rolls_back_on_init_failure -- --exact
    Expected: process cleaned up + audit failure event
    Evidence: .sisyphus/evidence/task-7-host-rollback.txt
  ```

  **Commit**: YES — `feat(daemon): implement mcp host lifecycle`

- [ ] 8. **T-A3.2 Registry 能力同步与健康状态刷新**

  **Source Mapping**: `T-A3`  
  **What to do**:
  - 将 MCP initialize 返回的 capabilities 同步入 registry
  - 增加健康状态刷新（healthy/degraded/unreachable）
  - 失效 server 从可用工具集中剔除

  **Impact Scope**: `crates/agentd-daemon/src/mcp.rs`, `crates/agentd-daemon/src/main.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/mcp.rs`, `crates/agentd-daemon/src/main.rs`
  - **Forbidden Paths**: `python/`, `web/`, `crates/agentctl/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（只读）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 2, Blocks 9/12/18, Blocked By 2/3/7

  **References**:
  - `design/post-mvp-roadmap-v1.md:138-139`（能力注册）
  - `design/post-mvp-roadmap-v1.md:110-113`（registry 责任边界）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon mcp_registry_syncs_capabilities_from_initialize -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon unhealthy_server_removed_from_available_tools -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: capability sync after initialize
    Tool: Bash
    Steps: run cargo test -p agentd-daemon mcp_registry_syncs_capabilities_from_initialize -- --exact
    Expected: tool list contains initialize advertised tools
    Evidence: .sisyphus/evidence/task-8-cap-sync.txt

  Scenario: unhealthy server pruned
    Tool: Bash
    Steps: run cargo test -p agentd-daemon unhealthy_server_removed_from_available_tools -- --exact
    Expected: unavailable tool absent from ListAvailableTools
    Evidence: .sisyphus/evidence/task-8-prune-unhealthy.txt
  ```

  **Commit**: YES — `feat(daemon): sync mcp capabilities into registry`

- [ ] 9. **T-A2.2 ListAvailableTools / InvokeSkill RPC 与策略审计闭环**

  **Source Mapping**: `T-A2`, `T-B5`  
  **What to do**:
  - 在 daemon 暴露 `ListAvailableTools(agent_id)` 与 `InvokeSkill(server,tool,args)`
  - `InvokeSkill` 强制走 policy gateway + audit
  - 补齐 deny/ask/allow 三类行为测试

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-protocol/src/rpc.rs`, `crates/agentd-core/src/audit.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-protocol/src/rpc.rs`, `crates/agentd-core/src/audit.rs`
  - **Forbidden Paths**: `python/`, `web/`, `crates/agentctl/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（首选由本任务负责；并行修改可由 Merge Agent 合并仲裁）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 2, Blocks 12/13/18, Blocked By 3/7/8

  **References**:
  - `design/post-mvp-roadmap-v1.md:149-156`（新增 RPC 与 discover_tools）
  - `crates/agentd-protocol/src/rpc.rs`（RPC 类型定义）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon list_available_tools_filters_by_policy -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon invoke_skill_denied_writes_audit -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: tool list filtered by policy
    Tool: Bash
    Steps: run cargo test -p agentd-daemon list_available_tools_filters_by_policy -- --exact
    Expected: denied tools absent in response
    Evidence: .sisyphus/evidence/task-9-list-filtered.txt

  Scenario: denied invoke blocked
    Tool: Bash
    Steps: run cargo test -p agentd-daemon invoke_skill_denied_writes_audit -- --exact
    Expected: invoke not forwarded and audit contains deny
    Evidence: .sisyphus/evidence/task-9-invoke-deny.txt
  ```

  **Commit**: YES — `feat(protocol): add mcp tool rpc endpoints`

- [ ] 10. **T-B1/B2.1 内置 mcp-fs + mcp-shell Server 基线实现**

  **Source Mapping**: `T-B1`, `T-B2`, `MCP-3`  
  **What to do**:
  - 实现 `mcp-fs`：`read_file/list_directory/search_files/patch_file/tree`
  - 实现 `mcp-shell`：`execute_with_timeout/get_output`
  - 保证工具声明可被 daemon initialize 发现

  **Impact Scope**: `python/agentd-mcp-fs/`, `python/agentd-mcp-shell/`, `configs/mcp-servers/`  
  **Boundary Control**:
  - **Allowed Paths**: `python/agentd-mcp-fs/`, `python/agentd-mcp-shell/`, `configs/mcp-servers/`
  - **Forbidden Paths**: `crates/agentd-daemon/src/main.rs`, `web/`
  - **Shared Contract Files**: `configs/mcp-servers/*.toml`（可写）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 2, Blocks 12, Blocked By 1

  **References**:
  - `design/post-mvp-roadmap-v1.md:163-167`（内置 server 工具列表）
  - `python/agentd-agent-lite/tests/test_tool_loop.py`（tool 调用交互风格）

  **Acceptance Criteria**:
  - [ ] `uv run pytest python/agentd-mcp-fs/tests/test_tools.py -q` PASS
  - [ ] `uv run pytest python/agentd-mcp-shell/tests/test_tools.py -q` PASS

  **QA Scenarios**:
  ```
  Scenario: fs tools callable
    Tool: Bash
    Steps: run pytest python/agentd-mcp-fs/tests/test_tools.py::test_read_and_tree -q
    Expected: read + tree return structured payload
    Evidence: .sisyphus/evidence/task-10-fs-tools.txt

  Scenario: shell timeout enforced
    Tool: Bash
    Steps: run pytest python/agentd-mcp-shell/tests/test_tools.py::test_execute_timeout -q
    Expected: timeout error code returned, process terminated
    Evidence: .sisyphus/evidence/task-10-shell-timeout.txt
  ```

  **Commit**: YES — `feat(mcp): add builtin fs and shell servers`

- [ ] 11. **T-B3/B4.1 内置 mcp-search + mcp-git Server 基线实现**

  **Source Mapping**: `T-B3`, `T-B4`, `MCP-3`  
  **What to do**:
  - 实现 `mcp-search`：`ripgrep/find_definition/semantic_search(占位+可扩展)`
  - 实现 `mcp-git`：`git_status/git_diff/git_log/git_apply_patch`
  - 工具错误返回统一结构（code/message/details）

  **Impact Scope**: `python/agentd-mcp-search/`, `python/agentd-mcp-git/`, `configs/mcp-servers/`  
  **Boundary Control**:
  - **Allowed Paths**: `python/agentd-mcp-search/`, `python/agentd-mcp-git/`, `configs/mcp-servers/`
  - **Forbidden Paths**: `crates/agentd-daemon/src/main.rs`, `web/`
  - **Shared Contract Files**: MCP tool schema JSON（本任务可写）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 2, Blocks 12, Blocked By 1

  **References**:
  - `design/post-mvp-roadmap-v1.md:167-169`（search/git server 定义）
  - `design/post-mvp-roadmap-v1.md:301-309`（补充工具集）

  **Acceptance Criteria**:
  - [ ] `uv run pytest python/agentd-mcp-search/tests/test_tools.py -q` PASS
  - [ ] `uv run pytest python/agentd-mcp-git/tests/test_tools.py -q` PASS

  **QA Scenarios**:
  ```
  Scenario: ripgrep and find_definition work
    Tool: Bash
    Steps: run pytest python/agentd-mcp-search/tests/test_tools.py::test_ripgrep_and_find_definition -q
    Expected: returns matches with file and line
    Evidence: .sisyphus/evidence/task-11-search-tools.txt

  Scenario: git apply patch validation
    Tool: Bash
    Steps: run pytest python/agentd-mcp-git/tests/test_tools.py::test_git_apply_patch_rejects_invalid_patch -q
    Expected: invalid patch rejected with stable error
    Evidence: .sisyphus/evidence/task-11-git-patch-error.txt
  ```

  **Commit**: YES — `feat(mcp): add builtin search and git servers`

- [ ] 12. **T-B5.1 agent-lite 动态工具发现与 schema 转换**

  **Source Mapping**: `T-B5`, `MCP-2`  
  **What to do**:
  - 将 `_build_tool_schema()` 改为 RPC 动态发现
  - 将 `ListAvailableTools` 结果转换为 OpenAI tool schema
  - 缓存 discovered tools，并在 server 变化时刷新

  **Impact Scope**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/`  
  **Boundary Control**:
  - **Allowed Paths**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/`
  - **Forbidden Paths**: `crates/agentctl/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（只读，契约由任务 9 维护）

  **Recommended Agent Profile**: `quick`  
  **Parallelization**: Wave 2, Blocks 13/17/27/28, Blocked By 7/8/9/10/11

  **References**:
  - `design/post-mvp-roadmap-v1.md:153-157`（discover_tools 设计）
  - `python/agentd-agent-lite/tests/test_tool_loop.py`（工具循环测试入口）

  **Acceptance Criteria**:
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_tool_discovery.py::test_dynamic_discover_tools -q` PASS
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_tool_discovery.py::test_policy_filtered_tools_not_exposed -q` PASS

  **QA Scenarios**:
  ```
  Scenario: dynamic tools visible to LLM
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_tool_discovery.py::test_dynamic_discover_tools -q
    Expected: discovered tool names appear in prompt schema
    Evidence: .sisyphus/evidence/task-12-dynamic-discovery.txt

  Scenario: denied tools hidden
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_tool_discovery.py::test_policy_filtered_tools_not_exposed -q
    Expected: denied tool absent from schema
    Evidence: .sisyphus/evidence/task-12-policy-filtered.txt
  ```

  **Commit**: YES — `feat(agent-lite): enable dynamic mcp tool discovery`

- [ ] 13. **T-B6.2 多轮对话运行循环与上下文预算管理**

  **Source Mapping**: `T-B6`  
  **What to do**:
  - 将单次 `run_once` 扩展为会话内多轮 `chat`
  - 引入 token budget 计数与阈值触发逻辑
  - 确保工具调用循环在多轮中可重入且状态一致

  **Impact Scope**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/test_tool_loop.py`  
  **Boundary Control**:
  - **Allowed Paths**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/`
  - **Forbidden Paths**: `crates/`, `web/`
  - **Shared Contract Files**: `python/agentd-agent-lite/src/agentd_agent_lite/config.py`（只读）

  **Recommended Agent Profile**: `quick`  
  **Parallelization**: Wave 3, Blocks 14/18/27, Blocked By 4/12

  **References**:
  - `design/post-mvp-roadmap-v1.md:312-350`（多轮 chat 设计）
  - `python/agentd-agent-lite/tests/test_tool_loop.py`（tool loop 回归）

  **Acceptance Criteria**:
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_multi_turn.py::test_chat_keeps_context_across_turns -q` PASS
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_multi_turn.py::test_tool_loop_reentrant_in_multi_turn -q` PASS

  **QA Scenarios**:
  ```
  Scenario: context preserved over 3 turns
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_multi_turn.py::test_chat_keeps_context_across_turns -q
    Expected: third response references prior turn facts
    Evidence: .sisyphus/evidence/task-13-multi-turn-context.txt

  Scenario: token budget overflow path
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_multi_turn.py::test_chat_triggers_compact_on_budget_threshold -q
    Expected: compact hook called once, loop continues
    Evidence: .sisyphus/evidence/task-13-budget-threshold.txt
  ```

  **Commit**: YES — `feat(agent-lite): add multi-turn chat loop`

- [ ] 14. **T-B7/B9.1 auto-compact 与会话持久化（save/load）**

  **Source Mapping**: `T-B7`, `T-B9`  
  **What to do**:
  - 实现上下文压缩触发策略（80% 阈值）与摘要回填
  - 会话 JSONL tree 持久化（含 id/parent_id）
  - 提供 session save/load 命令级接口（供 TUI 与 CLI 复用）

  **Impact Scope**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/`  
  **Boundary Control**:
  - **Allowed Paths**: `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`, `python/agentd-agent-lite/tests/`
  - **Forbidden Paths**: `crates/agentd-daemon/`, `web/`
  - **Shared Contract Files**: 会话格式文档（新增 `design/...` 可读写）

  **Recommended Agent Profile**: `quick`  
  **Parallelization**: Wave 3, Blocks 26, Blocked By 4/13

  **References**:
  - `design/post-mvp-roadmap-v1.md:347-350`（compact 触发）
  - `design/post-mvp-roadmap-v1.md:428`（session save/load 需求）

  **Acceptance Criteria**:
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_compact.py::test_auto_compact_preserves_key_facts -q` PASS
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_session_persistence.py::test_save_load_roundtrip -q` PASS

  **QA Scenarios**:
  ```
  Scenario: compact keeps key facts
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_compact.py::test_auto_compact_preserves_key_facts -q
    Expected: key entities retained after compact
    Evidence: .sisyphus/evidence/task-14-compact-facts.txt

  Scenario: broken session file rejected
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_session_persistence.py::test_load_rejects_corrupted_session_file -q
    Expected: stable parse error, no crash
    Evidence: .sisyphus/evidence/task-14-corrupted-session.txt
  ```

  **Commit**: YES — `feat(agent-lite): add compact and session persistence`

- [ ] 15. **T-A4.2 regorus 集成与 `.rego` 加载执行**

  **Source Mapping**: `T-A4`  
  **What to do**:
  - 将任务 5 的抽象绑定到 regorus 实现
  - 支持 `policies/*.rego` 加载、编译、执行
  - 提供最小 conformance 测试（allow/deny/explain）

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-core/src/policy.rs`, `policies/`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-core/src/policy.rs`, `crates/agentd-daemon/src/main.rs`, `policies/`
  - **Forbidden Paths**: `python/`, `web/`, `crates/agentctl/`
  - **Shared Contract Files**: `configs/agents/*.toml`（只读，转译由任务16负责）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 3, Blocks 16/17/21, Blocked By 5

  **References**:
  - `design/post-mvp-roadmap-v1.md:209-230`（regorus 架构）
  - `design/post-mvp-roadmap-v1.md:273-276`（验收标准）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon rego_policy_loaded_and_evaluated -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon rego_deny_returns_explanation_path -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: rego policy allow path
    Tool: Bash
    Steps: run cargo test -p agentd-daemon rego_policy_loaded_and_evaluated -- --exact
    Expected: allow decision true for allowed tool
    Evidence: .sisyphus/evidence/task-15-rego-allow.txt

  Scenario: invalid rego compile error
    Tool: Bash
    Steps: run cargo test -p agentd-daemon rego_invalid_policy_compile_error -- --exact
    Expected: compile error surfaced with file path
    Evidence: .sisyphus/evidence/task-15-rego-invalid.txt
  ```

  **Commit**: YES — `feat(policy): integrate regorus engine`

- [ ] 16. **T-A5.1 TOML→Rego 转译与行为一致性回归**

  **Source Mapping**: `T-A5`  
  **What to do**:
  - 将 `[policy]` TOML 自动转译为等价 Rego 规则
  - 搭建回归对比测试：旧引擎结果 vs 新引擎结果一致
  - 输出差异报告（用于升级安全阈值）

  **Impact Scope**: `crates/agentd-core/src/policy.rs`, `crates/agentd-daemon/src/main.rs`, `configs/agents/`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-core/src/policy.rs`, `crates/agentd-daemon/src/main.rs`, `configs/agents/`
  - **Forbidden Paths**: `python/`, `web/`
  - **Shared Contract Files**: `policies/`（可写生成物）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 3, Blocks 17/21, Blocked By 15

  **References**:
  - `design/post-mvp-roadmap-v1.md:240-267`（转译样例）
  - `design/post-mvp-roadmap-v1.md:274-275`（向后兼容验收）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon toml_to_rego_equivalence_suite -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon toml_policy_legacy_behavior_unchanged -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: translated policies match legacy decisions
    Tool: Bash
    Steps: run cargo test -p agentd-daemon toml_to_rego_equivalence_suite -- --exact
    Expected: decision parity = 100%
    Evidence: .sisyphus/evidence/task-16-parity.txt

  Scenario: unsupported toml construct rejected
    Tool: Bash
    Steps: run cargo test -p agentd-daemon toml_to_rego_rejects_unsupported_construct -- --exact
    Expected: clear error with offending key
    Evidence: .sisyphus/evidence/task-16-unsupported.txt
  ```

  **Commit**: YES — `feat(policy): add toml to rego transpiler`

- [ ] 17. **T-A6.1 策略热更新与 explain 输出**

  **Source Mapping**: `T-A6`  
  **What to do**:
  - 监听 `policies/` 文件变化并触发无重启 reload
  - 对 deny/ask 输出命中规则路径与 input 快照摘要
  - 增加 reload 失败回退机制（保持最后可用策略）

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-core/src/policy.rs`, `policies/`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-core/src/policy.rs`, `policies/`
  - **Forbidden Paths**: `python/`, `web/`, `crates/agentctl/`
  - **Shared Contract Files**: audit event schema（只读）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 3, Blocks 18/24, Blocked By 15/16

  **References**:
  - `design/post-mvp-roadmap-v1.md:275-276`（热更新与解释输出）
  - `.github/workflows/gates.yml`（审计与 gate 兼容）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon rego_hot_reload_without_restart -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon deny_explain_contains_rule_path -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: policy reload after file change
    Tool: Bash
    Steps: run cargo test -p agentd-daemon rego_hot_reload_without_restart -- --exact
    Expected: second decision reflects updated policy
    Evidence: .sisyphus/evidence/task-17-hot-reload.txt

  Scenario: bad policy does not replace current engine
    Tool: Bash
    Steps: run cargo test -p agentd-daemon rego_reload_bad_policy_keeps_previous_engine -- --exact
    Expected: previous engine still active
    Evidence: .sisyphus/evidence/task-17-reload-fallback.txt
  ```

  **Commit**: YES — `feat(policy): add hot reload and explain`

- [ ] 18. **T-B11/B12/B13.1 TUI 流式输出、Slash 命令与审批队列**

  **Source Mapping**: `T-B11`, `T-B12`, `T-B13`  
  **What to do**:
  - 实现消息流式渲染与工具调用折叠展示
  - 实现 `/usage /events /tools /compact /model /approve /deny /session save/load`
  - ask 工具审批队列：可查看、approve、deny 并回写 daemon

  **Impact Scope**: `crates/agentctl/src/tui.rs`, `crates/agentctl/src/main.rs`, `crates/agentd-daemon/src/main.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentctl/src/tui.rs`, `crates/agentctl/src/main.rs`, `crates/agentd-daemon/src/main.rs`
  - **Forbidden Paths**: `python/agentd-agent-lite/src/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/rpc.rs`（只读）

  **Recommended Agent Profile**: `visual-engineering`  
  **Parallelization**: Wave 3, Blocks 24/28, Blocked By 8/9/13/6

  **References**:
  - `design/post-mvp-roadmap-v1.md:411-429`（交互流与 slash 列表）
  - `design/post-mvp-roadmap-v1.md:435-438`（验收要求）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentctl slash_commands_core_set_available -- --exact` PASS
  - [ ] `cargo test -p agentctl approval_queue_roundtrip -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: slash command flow
    Tool: interactive_bash (tmux)
    Steps: enter /tools, /usage, /events in TUI
    Expected: each command renders panel response without crash
    Evidence: .sisyphus/evidence/task-18-slash-flow.txt

  Scenario: deny approval item
    Tool: interactive_bash (tmux)
    Steps: trigger ask tool -> run /deny <id>
    Expected: request marked denied and daemon receives decision
    Evidence: .sisyphus/evidence/task-18-approval-deny.txt
  ```

  **Commit**: YES — `feat(agentctl): add streaming slash and approvals`

- [ ] 19. **T-A7.1 Firecracker rootfs 构建工具链**

  **Source Mapping**: `T-A7`  
  **What to do**:
  - 构建最小 rootfs（含 Python runtime + agent-lite）
  - 提供可重复构建脚本与镜像版本标签
  - 增加 rootfs 内容校验（关键二进制/依赖存在）

  **Impact Scope**: `images/agent-rootfs/`, `scripts/`, `python/agentd-agent-lite/`  
  **Boundary Control**:
  - **Allowed Paths**: `images/agent-rootfs/`, `scripts/`, `python/agentd-agent-lite/`
  - **Forbidden Paths**: `crates/agentctl/`, `web/`
  - **Shared Contract Files**: rootfs manifest（首选由本任务负责；并行修改可由 Merge Agent 合并仲裁）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 4, Blocks 20, Blocked By None

  **References**:
  - `design/post-mvp-roadmap-v1.md:514-516`（rootfs + vsock 关键点）
  - `design/post-mvp-roadmap-v1.md:523-524`（rootfs 验收）

  **Acceptance Criteria**:
  - [ ] `bash scripts/firecracker/build-rootfs.sh` 退出码 0
  - [ ] `bash scripts/firecracker/verify-rootfs.sh` 报告 python + agent-lite 可用

  **QA Scenarios**:
  ```
  Scenario: rootfs builds successfully
    Tool: Bash
    Steps: run build-rootfs.sh then verify-rootfs.sh
    Expected: manifest complete and checksum generated
    Evidence: .sisyphus/evidence/task-19-rootfs-build.txt

  Scenario: missing runtime detected
    Tool: Bash
    Steps: run verify-rootfs.sh on intentionally broken image
    Expected: non-zero exit with missing python/runtime detail
    Evidence: .sisyphus/evidence/task-19-rootfs-missing-runtime.txt
  ```

  **Commit**: YES — `feat(firecracker): add rootfs build pipeline`

- [ ] 20. **T-A8.1 FirecrackerExecutor 与 vsock 通信链路**

  **Source Mapping**: `T-A8`  
  **What to do**:
  - 实现 VM 启动构建器（kernel/rootfs/vcpu/mem/network）
  - 打通 VM 内 agent-lite 到宿主 daemon 的 vsock 通信
  - 增加启动失败清理与超时处理

  **Impact Scope**: `crates/agentd-daemon/src/firecracker.rs(新增)`, `crates/agentd-daemon/src/main.rs`, `scripts/firecracker/`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/firecracker.rs`, `crates/agentd-daemon/src/main.rs`, `scripts/firecracker/`
  - **Forbidden Paths**: `web/`, `crates/agentctl/`
  - **Shared Contract Files**: runtime config schema（只读）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 4, Blocks 21, Blocked By 19

  **References**:
  - `design/post-mvp-roadmap-v1.md:482-509`（Executor 伪代码）
  - `design/post-mvp-roadmap-v1.md:524-525`（microVM 验收）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon firecracker_executor_launches_vm -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon firecracker_vsock_roundtrip -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: vm launch and connect via vsock
    Tool: Bash
    Steps: run cargo test -p agentd-daemon firecracker_vsock_roundtrip -- --exact
    Expected: request-response success across vsock
    Evidence: .sisyphus/evidence/task-20-vsock-roundtrip.txt

  Scenario: vm launch timeout handling
    Tool: Bash
    Steps: run cargo test -p agentd-daemon firecracker_launch_timeout_returns_stable_error -- --exact
    Expected: stable timeout error, no orphan vm process
    Evidence: .sisyphus/evidence/task-20-vm-timeout.txt
  ```

  **Commit**: YES — `feat(firecracker): implement vm executor and vsock`

- [ ] 21. **T-A9/A10.1 Runtime Selector + jailer/网络隔离策略**

  **Source Mapping**: `T-A9`, `T-A10`  
  **What to do**:
  - 基于 `trust_level` 在 cgroup 与 Firecracker 执行器间路由
  - 配置 jailer 约束与最小权限
  - 加入网络隔离规则（tap + nftables）并策略化控制

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-daemon/src/lifecycle.rs`, `crates/agentd-daemon/src/firecracker.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-daemon/src/lifecycle.rs`, `crates/agentd-daemon/src/firecracker.rs`
  - **Forbidden Paths**: `python/`, `web/`, `crates/agentctl/`
  - **Shared Contract Files**: `configs/agents/*.toml`（只读，避免破坏现有 profile）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 4, Blocks 22/23, Blocked By 15/16/20

  **References**:
  - `design/post-mvp-roadmap-v1.md:445-450`（分级隔离模型）
  - `design/post-mvp-roadmap-v1.md:525-527`（Selector 与网络策略验收）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon untrusted_agent_uses_firecracker_runtime -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon jailer_policy_blocks_forbidden_network -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: trust level routes to runtime
    Tool: Bash
    Steps: run cargo test -p agentd-daemon untrusted_agent_uses_firecracker_runtime -- --exact
    Expected: runtime selector chooses firecracker
    Evidence: .sisyphus/evidence/task-21-runtime-selector.txt

  Scenario: network denied by policy
    Tool: Bash
    Steps: run cargo test -p agentd-daemon jailer_policy_blocks_forbidden_network -- --exact
    Expected: outbound blocked with policy error
    Evidence: .sisyphus/evidence/task-21-network-deny.txt
  ```

  **Commit**: YES — `feat(runtime): add trust-level runtime selector`

- [ ] 22. **T-A11.1 A2A Server 端点、状态机与 SSE 流**

  **Source Mapping**: `T-A11`  
  **What to do**:
  - 实现 `/a2a/tasks` 创建/查询与 `/a2a/stream` SSE
  - 实现任务状态机：submitted→working→input-required→completed/failed/canceled
  - 将状态机映射到现有 AgentLifecycleState

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-protocol/src/v1.rs`, `crates/agentd-protocol/src/rpc.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-protocol/src/v1.rs`, `crates/agentd-protocol/src/rpc.rs`
  - **Forbidden Paths**: `python/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/v1.rs`（首选由本任务负责；并行修改可由 Merge Agent 合并仲裁）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 4, Blocks 23/24, Blocked By 21

  **References**:
  - `design/post-mvp-roadmap-v1.md:551-554`（A2A server 接口）
  - `design/post-mvp-roadmap-v1.md:565-577`（状态机映射）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon a2a_server_task_crud_and_stream -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon a2a_state_machine_valid_transitions -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: create task and stream updates
    Tool: Bash (curl)
    Steps:
      1. POST /a2a/tasks with input payload
      2. Subscribe /a2a/stream
      3. Assert states progress submitted->working->completed
    Evidence: .sisyphus/evidence/task-22-a2a-stream.json

  Scenario: invalid state transition rejected
    Tool: Bash
    Steps: run cargo test -p agentd-daemon a2a_state_machine_rejects_completed_to_working -- --exact
    Expected: transition denied with stable code
    Evidence: .sisyphus/evidence/task-22-invalid-transition.txt
  ```

  **Commit**: YES — `feat(a2a): add server endpoints and state machine`

- [ ] 23. **T-A12/A14.1 A2A Client SDK 与 `agentctl a2a` 命令集**

  **Source Mapping**: `T-A12`, `T-A14`  
  **What to do**:
  - 实现 A2A client：discover/create_task/stream_task
  - 在 agentctl 增加 `a2a discover/send/status`
  - 增加远端 agent card 发现与错误处理回归

  **Impact Scope**: `crates/agentctl/src/main.rs`, `crates/agentd-protocol/src/v1.rs`, `crates/agentd-daemon/src/main.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentctl/src/main.rs`, `crates/agentd-protocol/src/v1.rs`, `crates/agentd-daemon/src/main.rs`
  - **Forbidden Paths**: `python/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/v1.rs`（只读，契约由任务22管理）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 4, Blocks 24/25/26, Blocked By 21/22

  **References**:
  - `design/post-mvp-roadmap-v1.md:582-586`（client 与 CLI 验收）
  - `crates/agentctl/src/main.rs`（子命令扩展入口）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentctl a2a_cli_discover_send_status_flow -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon a2a_client_discovers_remote_card -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: cli sends task to remote agent
    Tool: Bash
    Steps: run agentctl a2a send --target <url> --input "ping"
    Expected: task id returned and status retrievable
    Evidence: .sisyphus/evidence/task-23-a2a-cli-send.txt

  Scenario: unreachable remote handled
    Tool: Bash
    Steps: run agentctl a2a discover --url http://127.0.0.1:9
    Expected: stable connection error code
    Evidence: .sisyphus/evidence/task-23-a2a-unreachable.txt
  ```

  **Commit**: YES — `feat(agentctl): add a2a client commands`

- [ ] 24. **T-A13/B14/B15.1 Task Orchestrator 与多 Agent 委托体验**

  **Source Mapping**: `T-A13`, `T-B14`, `T-B15`  
  **What to do**:
  - 实现任务分解→分配→聚合的 orchestrator
  - 在 TUI 增加多 Agent 视图与委托状态展示
  - 支持失败子任务重试与结果归并策略

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentctl/src/tui.rs`, `crates/agentd-store/src/agent.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentctl/src/tui.rs`, `crates/agentd-store/src/agent.rs`
  - **Forbidden Paths**: `python/agentd-agent-lite/src/`, `web/`
  - **Shared Contract Files**: `crates/agentd-protocol/src/v1.rs`（只读）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 5, Blocks 26/27/28, Blocked By 22/23/17/18

  **References**:
  - `design/post-mvp-roadmap-v1.md:557-559`（Task Orchestrator 角色）
  - `design/post-mvp-roadmap-v1.md:812-814`（多 Agent 体验目标）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon orchestrator_splits_and_aggregates_tasks -- --exact` PASS
  - [ ] `cargo test -p agentctl tui_multi_agent_panel_updates_on_events -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: orchestrator executes multi-agent fanout
    Tool: Bash
    Steps: run cargo test -p agentd-daemon orchestrator_splits_and_aggregates_tasks -- --exact
    Expected: child task results aggregated in deterministic order
    Evidence: .sisyphus/evidence/task-24-orchestrator-fanout.txt

  Scenario: one child task fails and retries
    Tool: Bash
    Steps: run cargo test -p agentd-daemon orchestrator_retries_failed_child_once -- --exact
    Expected: retry path executed and final status reflects outcome
    Evidence: .sisyphus/evidence/task-24-orchestrator-retry.txt
  ```

  **Commit**: YES — `feat(a2a): add orchestrator and multi-agent tui view`

- [ ] 25. **T-A15/A16.1 mDNS 发现与中心注册打通**

  **Source Mapping**: `T-A15`, `T-A16`  
  **What to do**:
  - 使用 `mdns-sd` 广播/发现 `_agentd._tcp.local.`
  - 实现中心注册 API（Agent Card 注册、查询、健康状态）
  - agentctl discover 同时支持 LAN + Registry 两种来源

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentctl/src/main.rs`, `crates/agentd-store/src/agent.rs`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentctl/src/main.rs`, `crates/agentd-store/src/agent.rs`
  - **Forbidden Paths**: `python/`, `web/`
  - **Shared Contract Files**: Agent Card schema（只读）

  **Recommended Agent Profile**: `unspecified-high`  
  **Parallelization**: Wave 5, Blocks 26, Blocked By 23

  **References**:
  - `design/post-mvp-roadmap-v1.md:605-607`（发现拓扑）
  - `design/post-mvp-roadmap-v1.md:612-615`（发现验收）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon mdns_peer_discovery_finds_remote_agent -- --exact` PASS
  - [ ] `cargo test -p agentctl discover_lists_lan_and_registry_sources -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: lan discovery across two nodes
    Tool: Bash
    Steps: run integration test with two daemon fixtures
    Expected: each node sees the other via mdns
    Evidence: .sisyphus/evidence/task-25-mdns-two-node.txt

  Scenario: registry unavailable fallback
    Tool: Bash
    Steps: run discover with registry endpoint down
    Expected: LAN results still returned + registry error noted
    Evidence: .sisyphus/evidence/task-25-registry-down.txt
  ```

  **Commit**: YES — `feat(discovery): add mdns and central registry`

- [ ] 26. **T-A17/A18.1 上下文迁移 L1/L2（语义迁移 + 状态快照）**

  **Source Mapping**: `T-A17`, `T-A18`  
  **What to do**:
  - 实现 L1：迁移摘要生成、A2A 携带 migration_context、目标端恢复
  - 实现 L2：会话消息/工具缓存/工作目录快照序列化
  - 增加失败回滚：源会话保持可继续

  **Impact Scope**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-store/src/agent.rs`, `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/main.rs`, `crates/agentd-store/src/agent.rs`, `python/agentd-agent-lite/src/agentd_agent_lite/cli.py`
  - **Forbidden Paths**: `web/`, `crates/agentctl/src/tui.rs`
  - **Shared Contract Files**: A2A payload schema（只读）

  **Recommended Agent Profile**: `deep`  
  **Parallelization**: Wave 5, Blocks 28, Blocked By 14/24/25

  **References**:
  - `design/post-mvp-roadmap-v1.md:623-629`（迁移分级与范围）
  - `design/post-mvp-roadmap-v1.md:633-647`（L1 流程）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon semantic_migration_l1_continues_workflow -- --exact` PASS
  - [ ] `cargo test -p agentd-daemon snapshot_migration_l2_roundtrip -- --exact` PASS

  **QA Scenarios**:
  ```
  Scenario: semantic migration succeeds
    Tool: Bash
    Steps: run cargo test -p agentd-daemon semantic_migration_l1_continues_workflow -- --exact
    Expected: target resumes with migration summary context
    Evidence: .sisyphus/evidence/task-26-l1-success.txt

  Scenario: migration failure rollback to source
    Tool: Bash
    Steps: run cargo test -p agentd-daemon migration_failure_rolls_back_source_session -- --exact
    Expected: source session remains runnable
    Evidence: .sisyphus/evidence/task-26-migration-rollback.txt
  ```

  **Commit**: YES — `feat(migration): implement l1 and l2 context migration`

- [ ] 27. **T-B16/B17.1 daemon WebSocket Bridge 与 Web Agent Chat**

  **Source Mapping**: `T-B16`, `T-B17`  
  **What to do**:
  - daemon 暴露 WS JSON-RPC bridge（映射既有 UDS RPC）
  - Web Agent Chat 页面实现：多轮消息、流式输出、工具调用可视化
  - 与审批/策略事件联动显示

  **Impact Scope**: `crates/agentd-daemon/src/ws_bridge.rs(新增)`, `web/agent-shell/app/`, `web/agent-shell/lib/`  
  **Boundary Control**:
  - **Allowed Paths**: `crates/agentd-daemon/src/ws_bridge.rs`, `web/agent-shell/app/`, `web/agent-shell/lib/`
  - **Forbidden Paths**: `python/agentd-agent-lite/src/`, `crates/agentctl/src/tui.rs`
  - **Shared Contract Files**: WS message schema（首选由本任务负责；并行修改可由 Merge Agent 合并仲裁）

  **Recommended Agent Profile**: `visual-engineering`  
  **Parallelization**: Wave 5, Blocks 28, Blocked By 12/18/24

  **References**:
  - `design/post-mvp-roadmap-v1.md:705-714`（WS Bridge 定义）
  - `design/post-mvp-roadmap-v1.md:720-722`（Chat 页面验收）

  **Acceptance Criteria**:
  - [ ] `cargo test -p agentd-daemon ws_bridge_forwards_rpc_and_stream -- --exact` PASS
  - [ ] `pnpm test --filter agent-shell chat-page-streaming.spec.ts` PASS

  **QA Scenarios**:
  ```
  Scenario: web chat streaming response
    Tool: Playwright
    Steps:
      1. open /chat
      2. fill textarea with "分析 main.rs"
      3. click `.send-button`
      4. assert `.stream-token` appears within 10s
    Evidence: .sisyphus/evidence/task-27-web-chat-stream.png

  Scenario: websocket disconnect recovery
    Tool: Playwright
    Steps:
      1. simulate ws disconnect
      2. assert `.reconnect-banner` visible
      3. restore ws and assert banner disappears
    Evidence: .sisyphus/evidence/task-27-ws-reconnect.png
  ```

  **Commit**: YES — `feat(web): add ws bridge and agent chat`

- [ ] 28. **T-B18/B19/B8.1 Dashboard/Settings + 第三方 MCP 接入硬化**

  **Source Mapping**: `T-B18`, `T-B19`, `T-B8`  
  **What to do**:
  - 实现 Dashboard/Events/Usage/Settings 页面最小闭环
  - 加入第三方 MCP Server onboarding 流程与兼容性测试
  - 输出跨语言契约兼容矩阵（daemon↔agent-lite↔web）

  **Impact Scope**: `web/agent-shell/app/`, `web/agent-shell/components/`, `crates/agentd-daemon/src/main.rs`, `python/agentd-agent-lite/tests/`  
  **Boundary Control**:
  - **Allowed Paths**: `web/agent-shell/app/`, `web/agent-shell/components/`, `python/agentd-agent-lite/tests/`, `crates/agentd-daemon/src/main.rs`
  - **Forbidden Paths**: `crates/agentctl/src/tui.rs`, `images/`
  - **Shared Contract Files**: API/WS schema（默认只读；必要变更可进行并在 merge note 记录影响）

  **Recommended Agent Profile**: `visual-engineering`  
  **Parallelization**: Wave 5, Blocks F1-F4, Blocked By 12/18/26/27

  **References**:
  - `design/post-mvp-roadmap-v1.md:697-704`（核心页面清单）
  - `design/post-mvp-roadmap-v1.md:777-783`（第三方 MCP 接入验收）

  **Acceptance Criteria**:
  - [ ] `pnpm test --filter agent-shell dashboard-events.spec.ts` PASS
  - [ ] `uv run pytest python/agentd-agent-lite/tests/test_third_party_mcp.py -q` PASS
  - [ ] 兼容矩阵文档落盘：`.sisyphus/evidence/task-28-contract-matrix.json`

  **QA Scenarios**:
  ```
  Scenario: dashboard and usage render live data
    Tool: Playwright
    Steps:
      1. open /dashboard
      2. assert `.agent-count-card` has numeric value
      3. open /usage and assert `.token-chart` visible
    Evidence: .sisyphus/evidence/task-28-dashboard-usage.png

  Scenario: third-party mcp handshake failure isolated
    Tool: Bash
    Steps: run pytest python/agentd-agent-lite/tests/test_third_party_mcp.py::test_third_party_mcp_handshake_failure_isolated -q
    Expected: failing third-party server does not break builtin tool listing
    Evidence: .sisyphus/evidence/task-28-third-party-isolation.txt
  ```

  **Commit**: YES — `feat(web): add dashboard settings and mcp hardening`

## Final Verification Wave (MANDATORY)

- [ ] F1. **Plan Compliance Audit** — `oracle`
  - 校验每个任务的 Must Have / Must NOT Have 是否在实现中兑现
  - 校验任务证据文件是否完整
  - 输出：`VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  - 运行：`cargo fmt --check`、`cargo clippy -- -D warnings`、`cargo test --workspace`、`pytest`
  - 检查 AI slop、无意义抽象、未使用代码、静默失败
  - 输出：`VERDICT: APPROVE/REJECT`

- [ ] F3. **Real QA Replay** — `unspecified-high` (+ `playwright` if Web/UI)
  - 重放所有任务 QA 场景（含失败路径）
  - 核验证据落盘路径与内容一致性
  - 输出：`VERDICT: APPROVE/REJECT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  - 逐任务对照计划与实际变更，检查 scope creep 与跨任务污染
  - 输出：`VERDICT: APPROVE/REJECT`

---

## Commit Strategy

- 原子提交：单任务单意图，默认单任务单 PR
- 提交格式：`type(scope): summary`
- 契约文件（protocol/types/schema）变更必须单独提交，避免混入实现细节

---

## Success Criteria

### Verification Commands
```bash
cargo test --workspace
uv run pytest
bash scripts/gates/phase-a-gate.sh
bash scripts/gates/phase-bc-gate.sh --local
```

### Final Checklist
- [ ] 全部任务依赖闭环（无未声明依赖）
- [ ] 全部任务边界清晰（Allowed/Forbidden/Shared）
- [ ] 全部任务具备 TDD 与 QA 证据
- [ ] Final Verification Wave 全通过
