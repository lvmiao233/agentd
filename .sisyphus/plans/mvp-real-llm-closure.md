# MVP 真实 LLM 闭环补齐与可信验收计划

## TL;DR

> **Quick Summary**: 先补齐 `agentd-agent-lite` 的真实 OpenAI-compatible LLM 调用循环，并引入必须通过的真实 One-API 端到端 gate，确保 MVP 核心命题可被“不可伪造证据”验证；再回补 inspect/delete/profile 文件体系。
>
> **Deliverables**:
> - 真实 LLM 调用闭环（agent-lite）
> - 真实 One-API E2E gate（含负例）
> - anti-mock 验收证据链（provider request id + usage 对账）
> - `agentctl agent inspect` / `agentctl agent delete`
> - `configs/agents/*.toml` profile 加载链路
>
> **Estimated Effort**: Large
> **Parallel Execution**: YES - 3 waves + Final
> **Critical Path**: T1 → T3 → T6 → T8 → T11 → T13

---

## Context

### Original Request
基于 `design/mvp-implementation-roadmap-v1.md` 对照当前实现，识别不足并给出下一步完善计划；优先真实测试，不依赖 mock。

### Interview Summary
**Key Discussions**:
- 用户选择“真实闭环优先”。
- 验收口径要求“必须由 agent-lite 完成真实闭环”。
- 自动化测试策略选择 **TDD**。
- 当前环境尚未准备真实 One-API/provider 凭据。

**Research Findings**:
- `agentd-agent-lite` 当前无真实 LLM SDK 依赖与调用路径。
- 现有 gate/demo 多为 `one_api.enabled=false`，可通过模拟记账。
- `inspect/delete` 在 CLI/RPC 未打通；store 已具备 delete 能力。
- `configs/agents/*.toml` 目录与加载链路缺失。

### Metis Review
**Identified Gaps（已吸收）**:
- 必须定义 anti-mock 验收标准（不是仅 `RecordUsage` 成功）。
- 必须加入真实路径负例（如 `one_api.enabled=false` / 凭据失效 / policy deny）。
- 必须锁 scope，防止把大量扩展项混入首个真实闭环里程碑。

---

## Work Objectives

### Core Objective
在不改变 agentd 核心治理方向的前提下，完成“agent-lite 真实 LLM 请求经过 agentd 管控并可审计计量”的可复现、可证明、可自动验收闭环。

### Concrete Deliverables
- Python `agentd-agent-lite` 真实 OpenAI-compatible 调用循环（含 tool-call roundtrip）。
- 真实 One-API E2E gate 脚本（正例 + 负例 + 对账）。
- provider 证据字段落盘/输出（request_id、model、usage 来源）。
- 新增 `agentctl agent inspect` / `agentctl agent delete`。
- 新增 `configs/agents/*.toml` profile 文件加载与校验。

### Definition of Done
- [ ] 真实 gate 在 `one_api.enabled=true` 时通过，且可验证 provider-origin 证据。
- [ ] 负例 gate（禁用 One-API/失效凭据/policy deny）按预期失败并给出机器可解析原因。
- [ ] TDD 相关测试全通过，且关键场景可复跑。

### Must Have
- 真实 provider 路径的“不可伪造证据”进入验收（request id + usage 对账）。
- 所有任务包含 agent-executed QA 场景（happy + negative）。

### Must NOT Have (Guardrails)
- 不允许仅凭 `RecordUsage` 成功判定“真实 LLM 路径通过”。
- 不允许在首个里程碑引入多 provider 抽象重构。
- 不允许将高阶扩展（UI/复杂调度）混入本计划。

---

## Verification Strategy (MANDATORY)

> **ZERO HUMAN INTERVENTION** — 全部由执行 agent 通过命令/脚本验证。

### Test Decision
- **Infrastructure exists**: YES
- **Automated tests**: **TDD**
- **Framework**: Rust `cargo test` + Python `pytest`
- **TDD Rule**: 每个实现任务遵循 RED → GREEN → REFACTOR

### QA Policy
每个任务必须包含 agent-executed QA 场景并产出证据到 `.sisyphus/evidence/`。

- **Frontend/UI**: N/A（本计划无 UI）
- **CLI/TUI**: `interactive_bash`（如需）或 Bash 运行 CLI
- **API/Backend**: Bash + `curl`/UDS RPC
- **Library/Module**: Bash 执行 `cargo test` / `pytest`

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Foundation & Real-Path Enablement):
├── T1: 真实 One-API 环境准备与预检脚本
├── T2: agent-lite 引入真实 LLM 客户端依赖与配置注入
├── T3: agent-lite 基础对话回路（单轮 real call）TDD
├── T4: 证据模型定义（provider request-id / usage source）
└── T5: 真实 gate 脚手架（独立于现有 mock gate）

Wave 2 (Core Loop & Anti-Mock Verification):
├── T6: tool-calling 循环（LLM→tool→LLM）TDD
├── T7: 失败治理（超时/重试/熔断/最大迭代）TDD
├── T8: 记账与审计对账链路（provider usage 对齐）
├── T9: 真实 gate 正例（enabled=true）+ 负例（disabled/invalid key）
└── T10: policy deny 负例（确认无 provider call）

Wave 3 (Roadmap Gap Backfill):
├── T11: `agentctl agent inspect` + RPC `GetAgent/Inspect`
├── T12: `agentctl agent delete` + RPC `DeleteAgent` wiring
└── T13: `configs/agents/*.toml` profile 加载与校验

Wave FINAL (Independent parallel review):
├── F1: Plan compliance audit (oracle)
├── F2: Code quality review
├── F3: Real manual QA replay (agent-executed)
└── F4: Scope fidelity check

Critical Path: T1 → T3 → T6 → T8 → T11 → T13
Parallel Speedup: ~60-70%
Max Concurrent: 5
```

### Dependency Matrix (FULL)

- **T1**: Blocked By: None → Blocks: T9
- **T2**: Blocked By: None → Blocks: T3, T6, T7
- **T3**: Blocked By: T2 → Blocks: T8
- **T4**: Blocked By: None → Blocks: T8, T9
- **T5**: Blocked By: None → Blocks: T9, T10
- **T6**: Blocked By: T2 → Blocks: T8, T10
- **T7**: Blocked By: T2 → Blocks: T9
- **T8**: Blocked By: T3, T4, T6 → Blocks: T9
- **T9**: Blocked By: T1, T4, T5, T7, T8 → Blocks: T11, T12, T13
- **T10**: Blocked By: T5, T6 → Blocks: T11, T12
- **T11**: Blocked By: T9, T10 → Blocks: Final Wave
- **T12**: Blocked By: T9, T10 → Blocks: Final Wave
- **T13**: Blocked By: T9 → Blocks: Final Wave

### Agent Dispatch Summary

- **Wave 1**: T1 `unspecified-high`, T2 `quick`, T3 `deep`, T4 `unspecified-high`, T5 `quick`
- **Wave 2**: T6 `deep`, T7 `deep`, T8 `unspecified-high`, T9 `unspecified-high`, T10 `quick`
- **Wave 3**: T11 `quick`, T12 `quick`, T13 `unspecified-high`
- **Final**: F1 `oracle`, F2 `unspecified-high`, F3 `unspecified-high`, F4 `deep`

---

## TODOs

- [x] 1. 真实 One-API 环境准备与预检脚本

  **What to do**:
  - 先新增“环境前置检查”脚本（不改现有 mock gate），验证 Docker/One-API/API key/模型可用性。
  - 增加机器可解析输出（`ENV_READY=true/false`、失败原因码）。
  - TDD：先写失败用例（缺少 key、health 不通）再实现通过路径。

  **Must NOT do**:
  - 不得把真实凭据硬编码到仓库。
  - 不得修改现有 phase gate 语义。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 涉及环境探测、错误分类、脚本鲁棒性。
  - **Skills**: [`git-master`]
    - `git-master`: 确保脚本与测试变更原子提交、便于回滚。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 本任务无浏览器交互。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with T2, T4, T5)
  - **Blocks**: T9
  - **Blocked By**: None

  **References**:
  - `scripts/gates/phase-a-gate.sh` - 现有 gate 框架与证据写入模式。
  - `configs/agentd.toml` - One-API 配置字段来源。
  - `crates/agentd-daemon/src/main.rs` (`default_one_api_health_url`, `one_api_supervisor`) - daemon 对健康与启动状态的实际判断逻辑。
  - One-API 官方 README（deployment/usage/env vars）- 真机部署与 API Base/token 使用规范。

  **Acceptance Criteria**:
  - [x] 新增预检测试先失败（无 key/health 不通）。
  - [x] 预检脚本在环境完整时输出 `ENV_READY=true`。
  - [x] 预检脚本在环境缺失时输出稳定错误码与说明。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: 环境就绪 happy path
    Tool: Bash (curl)
    Preconditions: One-API 可访问，TOKEN 已配置，模型已在 channel 中可用
    Steps:
      1. 运行预检脚本（例如 bash scripts/gates/preflight-real-oneapi.sh）
      2. 脚本调用 http://127.0.0.1:3000/api/status
      3. 脚本调用 /v1/models 并检查目标模型名存在
    Expected Result: 退出码 0，输出含 ENV_READY=true
    Failure Indicators: /api/status 非 2xx、/v1/models 不含目标模型、退出码非 0
    Evidence: .sisyphus/evidence/task-1-preflight-happy.txt

  Scenario: 缺失凭据 failure path
    Tool: Bash (curl)
    Preconditions: 清空 ONE_API_TOKEN
    Steps:
      1. 运行预检脚本
      2. 观察脚本返回错误码与错误分类
    Expected Result: 退出码非 0，输出 ENV_READY=false 与 MISSING_TOKEN
    Evidence: .sisyphus/evidence/task-1-preflight-missing-token-error.txt
  ```

  **Commit**: YES
  - Message: `test(gates): add real one-api preflight readiness checks`
  - Files: `scripts/gates/*`, `tests/*`
  - Pre-commit: `bash scripts/gates/preflight-real-oneapi.sh --dry-run`

- [x] 2. agent-lite 引入真实 LLM 客户端依赖与配置注入

  **What to do**:
  - 在 `python/agentd-agent-lite` 增加 OpenAI-compatible 客户端依赖与配置读取（base_url/api_key/model/timeout）。
  - 保持当前 RPC 通道（AuthorizeTool/RecordUsage）兼容。
  - TDD：先写配置缺失/格式错误测试，再实现配置层。

  **Must NOT do**:
  - 不引入多 provider 抽象层。
  - 不改变现有 CLI 参数语义（除新增必要参数外）。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 主要是依赖与配置注入，改动集中在 1-3 文件。
  - **Skills**: [`git-master`]
    - `git-master`: 小步提交，避免配置与逻辑混杂。
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: 无网页自动化需求。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with T1, T4, T5)
  - **Blocks**: T3, T6, T7
  - **Blocked By**: None

  **References**:
  - `python/agentd-agent-lite/pyproject.toml` - 当前依赖为空，需要在此引入 SDK。
  - `python/agentd-agent-lite/src/agentd_agent_lite/cli.py` - 现有参数与 RPC 调用入口。
  - One-API README `Usage` - OpenAI-compatible `base_url` / token 使用方式。

  **Acceptance Criteria**:
  - [x] 配置缺失测试先失败后通过。
  - [x] 支持通过环境变量与参数注入 base_url/api_key/model。
  - [x] 无真实请求时（dry-run）不触发网络调用。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: 配置注入 happy path
    Tool: Bash
    Preconditions: 设置 ONE_API_BASE_URL, ONE_API_TOKEN, MODEL
    Steps:
      1. 运行 python -m agentd_agent_lite.cli --help 或 --dry-run
      2. 检查输出配置摘要中 model/base_url 被正确读取
    Expected Result: 退出码 0，配置解析成功
    Failure Indicators: 参数冲突、缺失 key 仍误判成功
    Evidence: .sisyphus/evidence/task-2-config-happy.txt

  Scenario: 非法 base_url failure path
    Tool: Bash
    Preconditions: ONE_API_BASE_URL=not-a-url
    Steps:
      1. 运行配置校验入口
      2. 检查错误类别
    Expected Result: 退出码非 0，返回 INVALID_BASE_URL
    Evidence: .sisyphus/evidence/task-2-config-invalid-url-error.txt
  ```

  **Commit**: YES
  - Message: `feat(agent-lite): add openai-compatible config and sdk deps`
  - Files: `python/agentd-agent-lite/pyproject.toml`, `python/agentd-agent-lite/src/*`
  - Pre-commit: `pytest -q python/agentd-agent-lite/tests/test_config.py`

- [x] 3. agent-lite 单轮真实 LLM 调用回路（RED→GREEN→REFACTOR）

  **What to do**:
  - 先写失败测试：给定 prompt，必须走真实 OpenAI-compatible API 并返回 assistant 文本。
  - 实现单轮调用（不含 tool-call）并回填到 agent 输出结构。
  - 记录 provider request-id（若响应头/体可取）到输出证据。

  **Must NOT do**:
  - 不得用本地字符串拼装替代真实请求。
  - 不得吞掉网络/鉴权错误。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 需要真实网络调用、错误语义与测试约束一致性。
  - **Skills**: [`git-master`]
    - `git-master`: 保持测试与实现同步提交。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 无 UI。

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 1 (sequential after T2)
  - **Blocks**: T8
  - **Blocked By**: T2

  **References**:
  - `python/agentd-agent-lite/src/agentd_agent_lite/cli.py` - 当前 run_once 主流程。
  - `scripts/gates/task-18-lite-gate.sh` - 现有 lite gate 断言结构可复用。
  - OpenAI-compatible function calling 文档（librarian 输出）- 请求/响应字段规范。

  **Acceptance Criteria**:
  - [x] RED: 单轮真实调用测试初始失败。
  - [x] GREEN: 调用成功并返回 assistant 文本。
  - [x] REFACTOR: 错误映射与日志字段稳定，测试全绿。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: 单轮真实调用 happy path
    Tool: Bash
    Preconditions: One-API 就绪，模型可用，agent 已创建
    Steps:
      1. 运行 agentctl agent run --builtin lite --name real-lite --model <model> --json "say hi"
      2. 读取输出 JSON 中 llm.output/provider.request_id
      3. 调用 agentctl usage <agent_id> --json
    Expected Result: 输出非空，request_id 非空，usage total_tokens > 0
    Failure Indicators: 输出仍为本地拼接（如仅 lite: 前缀）、request_id 缺失、usage=0
    Evidence: .sisyphus/evidence/task-3-real-call-happy.json

  Scenario: 失效凭据 failure path
    Tool: Bash
    Preconditions: 使用无效 ONE_API_TOKEN
    Steps:
      1. 运行同一命令
      2. 捕获错误码和错误消息
    Expected Result: 退出码非 0，错误含 provider auth/network 分类
    Evidence: .sisyphus/evidence/task-3-real-call-invalid-cred-error.txt
  ```

  **Commit**: YES
  - Message: `feat(agent-lite): implement single-turn real llm invocation`
  - Files: `python/agentd-agent-lite/src/*`, `python/agentd-agent-lite/tests/*`
  - Pre-commit: `pytest -q python/agentd-agent-lite/tests/test_real_single_turn.py`

- [x] 4. anti-mock 证据模型定义（provider 元数据 + usage 来源）

  **What to do**:
  - 定义统一证据字段：`provider_request_id`、`provider_model`、`usage_source(provider|estimated)`、`transport_mode(real|simulated)`。
  - 调整 agent-lite 输出与 gate 断言，强制校验这些字段。
  - TDD：先写“缺字段即失败”测试。

  **Must NOT do**:
  - 不允许默认把 `usage_source` 标成 provider。
  - 不允许 request-id 为空仍判 pass。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 牵涉跨脚本/输出结构/验收逻辑一致性。
  - **Skills**: [`git-master`]
    - `git-master`: 便于分离“结构定义”与“消费断言”改动。
  - **Skills Evaluated but Omitted**:
    - `conventional-commits`: 非必须。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with T1, T2, T5)
  - **Blocks**: T8, T9
  - **Blocked By**: None

  **References**:
  - `scripts/gates/phase-a-gate.sh` - JSON 证据生成与断言模式。
  - `scripts/demo/e2e-demo.sh` - demo 汇总输出结构。
  - `crates/agentd-daemon/src/main.rs` (`RecordUsage`) - 当前 usage 计量入口，需与证据字段协同。

  **Acceptance Criteria**:
  - [ ] 证据 schema 测试覆盖“字段完整 + 字段合法值”。
  - [ ] 缺少 `provider_request_id` 或 `transport_mode=simulated` 时真实 gate 必失败。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: anti-mock 字段齐全 happy path
    Tool: Bash
    Preconditions: 真实调用已可用
    Steps:
      1. 执行真实 gate
      2. 解析输出证据 JSON 字段
      3. 校验 provider_request_id 非空，usage_source=provider，transport_mode=real
    Expected Result: 退出码 0，字段全部满足
    Failure Indicators: 任一字段缺失/值不合法
    Evidence: .sisyphus/evidence/task-4-anti-mock-happy.json

  Scenario: 强制模拟标记 failure path
    Tool: Bash
    Preconditions: 测试注入 transport_mode=simulated
    Steps:
      1. 执行 gate 的负例开关
      2. 观察断言失败
    Expected Result: 退出码非 0，错误含 MOCK_EVIDENCE_REJECTED
    Evidence: .sisyphus/evidence/task-4-anti-mock-simulated-error.txt
  ```

  **Commit**: YES
  - Message: `test(gates): enforce anti-mock evidence schema`
  - Files: `scripts/gates/*`, `tests/*`
  - Pre-commit: `pytest -q tests/test_anti_mock_schema.py`

- [x] 5. 真实 gate 脚手架（独立于现有 mock gate）

  **What to do**:
  - 新增 `task-real-closure-gate.sh`，不修改现有 phase-a/task-18/demo gate 行为。
  - 统一输出 `ASSERT ...=PASS/FAIL` 便于机器判定。
  - 增加 `--negative-one-api-disabled`、`--negative-invalid-credentials` 选项。

  **Must NOT do**:
  - 不复用 `one_api.enabled=false` 路径作为真实 gate pass 条件。
  - 不把失败原因写成非结构化日志。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 主要为脚本编排与断言框架。
  - **Skills**: [`git-master`]
    - `git-master`: 脚本改动易回滚、利于验收。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 无浏览器场景。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with T1, T2, T4)
  - **Blocks**: T9, T10
  - **Blocked By**: None

  **References**:
  - `scripts/gates/phase-a-gate.sh` - gate 框架参考。
  - `scripts/gates/task-18-lite-gate.sh` - lite 相关断言与 evidence 路径规范。
  - `scripts/demo/e2e-demo.sh` - 端到端执行骨架。

  **Acceptance Criteria**:
  - [ ] 新 gate 支持 happy + 两个负例参数。
  - [ ] 每次执行均产出 `.sisyphus/evidence/task-5-*.txt/json`。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: gate 脚手架 happy path
    Tool: Bash
    Preconditions: 仅运行 --dry-run
    Steps:
      1. bash scripts/gates/task-real-closure-gate.sh --dry-run
      2. 校验输出包含 ASSERT preflight=PASS
    Expected Result: 退出码 0
    Failure Indicators: 参数解析失败、断言格式不一致
    Evidence: .sisyphus/evidence/task-5-gate-dryrun-happy.txt

  Scenario: disabled 负例参数 failure path
    Tool: Bash
    Preconditions: one_api.enabled=false
    Steps:
      1. bash scripts/gates/task-real-closure-gate.sh --negative-one-api-disabled
      2. 检查 EXPECTED_FAILURE one_api_disabled
    Expected Result: 退出码非 0 且错误标记存在
    Evidence: .sisyphus/evidence/task-5-gate-disabled-error.txt
  ```

  **Commit**: YES
  - Message: `test(gates): add dedicated real-closure gate scaffold`
  - Files: `scripts/gates/task-real-closure-gate.sh`
  - Pre-commit: `bash scripts/gates/task-real-closure-gate.sh --dry-run`

- [x] 6. tool-calling 循环（LLM→tool→LLM）TDD

  **What to do**:
  - RED：编写测试，要求模型返回 tool_call 后 agent 正确执行工具并把结果回注给模型，最终得到 final answer。
  - GREEN：实现多步循环（含 `max_iterations` 上限）。
  - REFACTOR：抽离 tool execution 与 message append 的可测单元。

  **Must NOT do**:
  - 不允许无限循环。
  - 不允许工具错误导致进程崩溃（应结构化返回给模型或上层）。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 需要控制复杂状态机与错误分支。
  - **Skills**: [`git-master`]
    - `git-master`: 便于分阶段提交 RED/GREEN/REFACTOR。
  - **Skills Evaluated but Omitted**:
    - `dev-browser`: 无浏览器。

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 2 (core)
  - **Blocks**: T8, T10
  - **Blocked By**: T2

  **References**:
  - `python/agentd-agent-lite/src/agentd_agent_lite/cli.py` - 现有 run_once 主入口。
  - librarian 提供的 OpenAI-compatible tool-calling 参考模式 - 响应解析与 `tool_call_id` 回传。
  - `crates/agentd-daemon/src/main.rs` (`AuthorizeTool`) - 工具权限判定入口。

  **Acceptance Criteria**:
  - [ ] RED/GREEN/REFACTOR 三阶段提交可追溯。
  - [ ] 至少覆盖：单次 tool_call、连续两次 tool_call、无 tool_call 直接结束。
  - [ ] `max_iterations` 命中时返回稳定错误状态。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: tool-call 回路 happy path
    Tool: Bash
    Preconditions: 模型可返回工具调用，工具 builtin.lite.upper 可用
    Steps:
      1. 运行 agentctl agent run --builtin lite ... "请把 hello 变大写后总结"
      2. 读取输出 JSON，验证有 tool 调用记录与 final answer
      3. 查询 agent audit，确认 ToolApproved/ToolInvoked 事件
    Expected Result: 最终响应包含工具结果影响，audit 事件完整
    Failure Indicators: 仅首轮响应、无 tool 回注、无 final answer
    Evidence: .sisyphus/evidence/task-6-tool-loop-happy.json

  Scenario: 超过最大迭代 failure path
    Tool: Bash
    Preconditions: 将 max_iterations 设为 1，并触发至少一次 tool_call
    Steps:
      1. 运行命令
      2. 检查返回状态与错误码
    Expected Result: 退出码非 0 或状态 failed，错误为 MAX_ITERATIONS_REACHED
    Evidence: .sisyphus/evidence/task-6-tool-loop-max-iter-error.txt
  ```

  **Commit**: YES
  - Message: `feat(agent-lite): implement tool-calling loop with iteration guard`
  - Files: `python/agentd-agent-lite/src/*`, `python/agentd-agent-lite/tests/*`
  - Pre-commit: `pytest -q python/agentd-agent-lite/tests/test_tool_loop.py`

- [x] 7. 失败治理（超时/重试/熔断）TDD

  **What to do**:
  - RED：为超时、429/5xx、网络抖动写失败测试。
  - GREEN：实现指数退避重试、最大重试次数、可配置超时。
  - REFACTOR：引入统一错误分类（AUTH/NETWORK/TIMEOUT/RATE_LIMIT）。

  **Must NOT do**:
  - 不允许无限重试。
  - 不允许将所有错误吞并成 generic error。

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: 稳定性策略与错误语义影响全链路可信性。
  - **Skills**: [`git-master`]
    - `git-master`: 便于按错误类型拆分提交。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 无关。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with T8)
  - **Blocks**: T9
  - **Blocked By**: T2

  **References**:
  - `crates/agentd-daemon/src/main.rs` (`request_with_retry`) - 现有 daemon 侧重试思路可借鉴。
  - librarian 的 retry/backoff 建议（Tenacity/指数退避）- Python 实现策略参考。

  **Acceptance Criteria**:
  - [ ] 重试测试（429/5xx）由红转绿。
  - [ ] 超时后输出 TIMEOUT 分类且退出行为稳定。
  - [ ] 重试次数达上限后给出可解析错误。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: 可恢复错误重试 happy path
    Tool: Bash
    Preconditions: 注入一次临时 5xx，第二次恢复
    Steps:
      1. 运行 real gate 的 flaky 模式
      2. 观察日志含 retry attempt=1
      3. 最终请求成功
    Expected Result: 退出码 0，重试后成功
    Failure Indicators: 首次失败直接退出、或无限重试
    Evidence: .sisyphus/evidence/task-7-retry-happy.txt

  Scenario: 持续超时 failure path
    Tool: Bash
    Preconditions: 将上游指向不可达地址或注入超时
    Steps:
      1. 运行调用
      2. 检查错误类型
    Expected Result: 退出码非 0，错误类型 TIMEOUT，attempts 到上限
    Evidence: .sisyphus/evidence/task-7-timeout-error.txt
  ```

  **Commit**: YES
  - Message: `fix(agent-lite): add bounded retries and timeout error taxonomy`
  - Files: `python/agentd-agent-lite/src/*`, `python/agentd-agent-lite/tests/*`
  - Pre-commit: `pytest -q python/agentd-agent-lite/tests/test_retries.py`

- [x] 8. 记账与审计对账链路（provider usage 对齐）

  **What to do**:
  - 将 provider 返回 usage 与 daemon `RecordUsage` 写入建立对账关系。
  - 为 audit payload 增加对账关键字段（request_id, usage_source）。
  - 增加阈值断言（例如 token 总量偏差 <= 2%）。

  **Must NOT do**:
  - 不允许仅用本地估算值覆盖 provider usage。
  - 不允许缺 request_id 时继续记为“真实通过”。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 跨 Python/Rust 边界的证据一致性与审计模型调整。
  - **Skills**: [`git-master`]
    - `git-master`: 需要跨文件原子提交避免中间态。
  - **Skills Evaluated but Omitted**:
    - `conventional-commits`: 非必须。

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 2 (critical integration)
  - **Blocks**: T9
  - **Blocked By**: T3, T4, T6

  **References**:
  - `crates/agentd-daemon/src/main.rs` (`RecordUsage`, `record_audit_event`) - 当前记账与审计入口。
  - `crates/agentd-store/src/usage.rs` - 计量存储结构。
  - `crates/agentd-store/src/audit.rs` - 审计落库字段。

  **Acceptance Criteria**:
  - [ ] provider usage 与 usage 查询结果在阈值内一致。
  - [ ] audit 可检索到 request_id + usage_source。
  - [ ] 对账失败时 gate 必失败。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: usage 对账 happy path
    Tool: Bash
    Preconditions: 完成一次真实调用
    Steps:
      1. 获取 agent 运行输出中的 provider usage
      2. 调用 agentctl usage <agent_id> --json
      3. 比较 total_tokens 差异
    Expected Result: 差异 <= 2%，并且 audit 含 request_id
    Failure Indicators: 差异超阈值、audit 无 request_id
    Evidence: .sisyphus/evidence/task-8-usage-reconcile-happy.json

  Scenario: 对账缺字段 failure path
    Tool: Bash
    Preconditions: 注入 request_id 缺失（测试桩）
    Steps:
      1. 执行对账断言
      2. 检查失败标识
    Expected Result: 退出码非 0，错误为 MISSING_PROVIDER_REQUEST_ID
    Evidence: .sisyphus/evidence/task-8-usage-reconcile-missing-requestid-error.txt
  ```

  **Commit**: YES
  - Message: `feat(audit-usage): add provider-linked reconciliation evidence`
  - Files: `crates/agentd-daemon/src/main.rs`, `crates/agentd-store/src/*`, `python/agentd-agent-lite/src/*`
  - Pre-commit: `cargo test -p agentd-daemon && cargo test -p agentd-store`

- [x] 9. 真实 gate 正例 + 负例（disabled/invalid key）

  **What to do**:
  - 完成真实 gate 主流程：创建 agent → 真实调用 → 对账 → 事件校验。
  - 补齐负例：`one_api.enabled=false`、无效凭据，均应稳定失败。
  - 固化 evidence 命名规范，供 Final Wave 自动审查。

  **Must NOT do**:
  - 不得使用 mock 路径冒充正例。
  - 不得省略负例断言。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 涉及全链路编排与验收判定，是 MVP 关键 gate。
  - **Skills**: [`git-master`]
    - `git-master`: 关键 gate 改动应独立提交便于回滚。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 非 UI。

  **Parallelization**:
  - **Can Run In Parallel**: NO
  - **Parallel Group**: Wave 2 (end-of-wave integrator)
  - **Blocks**: T11, T12, T13
  - **Blocked By**: T1, T4, T5, T7, T8

  **References**:
  - `scripts/gates/phase-a-gate.sh` - 现有 gate 输出与错误处理模式。
  - `scripts/gates/task-18-lite-gate.sh` - lite 路径与 usage/audit 断言方式。
  - `scripts/demo/e2e-demo.sh` - 全链路 demo 编排。

  **Acceptance Criteria**:
  - [x] happy case：退出码 0 + provider 证据完整。
  - [x] disabled case：退出码非 0 + `EXPECTED_FAILURE one_api_disabled`。
  - [x] invalid key case：退出码非 0 + provider auth/network 错误标记。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: 真实 gate happy path
    Tool: Bash
    Preconditions: 环境预检已通过
    Steps:
      1. bash scripts/gates/task-real-closure-gate.sh
      2. 检查输出 ASSERT real_provider_path=PASS
      3. 检查 evidence 文件存在且包含 request_id
    Expected Result: 退出码 0，所有 ASSERT 为 PASS
    Failure Indicators: 任一 ASSERT FAIL、evidence 缺失
    Evidence: .sisyphus/evidence/task-9-real-gate-happy.txt

  Scenario: invalid credentials failure path
    Tool: Bash
    Preconditions: 设置无效 token
    Steps:
      1. bash scripts/gates/task-real-closure-gate.sh --negative-invalid-credentials
      2. 检查错误标记
    Expected Result: 退出码非 0，EXPECTED_FAILURE invalid_credentials
    Evidence: .sisyphus/evidence/task-9-real-gate-invalid-cred-error.txt
  ```

  **Commit**: YES
  - Message: `test(gates): finalize real-closure happy and negative gates`
  - Files: `scripts/gates/task-real-closure-gate.sh`, `scripts/gates/*`
  - Pre-commit: `bash scripts/gates/task-real-closure-gate.sh --dry-run`

- [x] 10. policy deny 负例（确保无 provider call 尝试）

  **What to do**:
  - 新增专用负例：对被 deny 的工具调用请求，必须在 `AuthorizeTool` 阶段拦截。
  - 在证据中记录 `provider_call_attempted=false`。
  - TDD：先写“被 deny 但仍有 provider 请求”应失败的测试。

  **Must NOT do**:
  - 不允许 deny 后仍触发外部 LLM 请求。
  - 不允许仅检查返回码，不检查审计事件。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 以 gate 断言补强为主，改动面较窄。
  - **Skills**: [`git-master`]
    - `git-master`: 便于保持负例断言变更独立。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 无 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 2 (with T9)
  - **Blocks**: T11, T12
  - **Blocked By**: T5, T6

  **References**:
  - `crates/agentd-daemon/src/main.rs` (`AuthorizeTool`) - deny 判定和 audit 记录逻辑。
  - `scripts/gates/task-18-lite-gate.sh` - deny 示例与 usage=0 检查可参考。

  **Acceptance Criteria**:
  - [ ] deny 场景下 provider 调用尝试标记为 false。
  - [ ] audit 中出现 `ToolDenied`，usage 不增长。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: policy deny happy path（应正确拦截）
    Tool: Bash
    Preconditions: agent 配置 deny 指定工具
    Steps:
      1. 运行 real gate 的 --negative-policy-deny
      2. 查询 audit 与 usage
    Expected Result: ToolDenied 存在，provider_call_attempted=false，total_tokens 不变
    Failure Indicators: total_tokens 增长、无 ToolDenied
    Evidence: .sisyphus/evidence/task-10-policy-deny-happy.txt

  Scenario: deny 误放行 failure path
    Tool: Bash
    Preconditions: 测试注入（模拟策略配置错误）
    Steps:
      1. 触发本应 deny 的工具
      2. 观察 gate 断言
    Expected Result: gate 失败，错误 POLICY_DENY_BYPASSED
    Evidence: .sisyphus/evidence/task-10-policy-deny-bypass-error.txt
  ```

  **Commit**: YES
  - Message: `test(policy): enforce deny path blocks provider requests`
  - Files: `scripts/gates/*`, `python/agentd-agent-lite/tests/*`
  - Pre-commit: `bash scripts/gates/task-real-closure-gate.sh --negative-policy-deny`

- [x] 11. `agentctl agent inspect` + RPC 单体查询

  **What to do**:
  - 在 daemon 增加单 Agent 查询 RPC（`GetAgent` 或 `InspectAgent`）。
  - 在 agentctl 增加 `agent inspect --agent-id <id>` 子命令。
  - 输出策略、预算、资源限制、最近审计摘要（最小必要字段）。

  **Must NOT do**:
  - 不改变现有 `agent list` 返回结构。
  - 不在 inspect 中暴露敏感 token 原文。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: CLI+RPC 小范围功能补齐。
  - **Skills**: [`git-master`]
    - `git-master`: 便于将协议/daemon/cli 三处同步提交。
  - **Skills Evaluated but Omitted**:
    - `conventional-commits`: 可选。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with T12, T13)
  - **Blocks**: Final Wave
  - **Blocked By**: T9, T10

  **References**:
  - `crates/agentctl/src/main.rs` - 现有子命令树，需新增 inspect。
  - `crates/agentd-daemon/src/main.rs` (`handle_rpc_request`) - RPC 路由扩展点。
  - `crates/agentd-protocol/src/v1.rs` (`GetAgentResponse`) - 可复用响应类型。
  - `crates/agentd-store/src/lib.rs` (`get_agent`) - 查询能力已存在。

  **Acceptance Criteria**:
  - [x] `agentctl agent inspect --agent-id <id> --json` 返回 200 等价成功结果。
  - [x] 不存在 agent 时返回明确错误码。
  - [x] 输出不包含敏感 access token。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: inspect happy path
    Tool: Bash
    Preconditions: 已存在 agent_id
    Steps:
      1. agentctl agent inspect --agent-id <id> --json
      2. 校验 JSON 包含 permissions/budget/model
    Expected Result: 退出码 0，字段齐全
    Failure Indicators: method not found 或字段缺失
    Evidence: .sisyphus/evidence/task-11-inspect-happy.json

  Scenario: inspect 不存在 agent failure path
    Tool: Bash
    Preconditions: 随机 UUID
    Steps:
      1. agentctl agent inspect --agent-id <random> --json
      2. 捕获错误码
    Expected Result: 退出码非 0，错误为 not found 类别
    Evidence: .sisyphus/evidence/task-11-inspect-notfound-error.txt
  ```

  **Commit**: YES
  - Message: `feat(agentctl): add agent inspect command and rpc handler`
  - Files: `crates/agentctl/src/main.rs`, `crates/agentd-daemon/src/main.rs`, `crates/agentd-protocol/src/*`
  - Pre-commit: `cargo test -p agentd-daemon && cargo test -p agentctl`

- [x] 12. `agentctl agent delete` + RPC/Store wiring

  **What to do**:
  - 在 daemon 新增 `DeleteAgent` RPC 并调用 store `delete_agent`。
  - 在 agentctl 增加 `agent delete --agent-id <id>`。
  - 定义删除前置（若存在 managed 进程先 stop 或拒绝删除并给出错误）。

  **Must NOT do**:
  - 不允许静默删除仍在运行的进程而不写审计。
  - 不允许 orphan 数据（usage/audit 引用失配）不处理。

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 现有 store 已有 delete，主要补 RPC+CLI 联通。
  - **Skills**: [`git-master`]
    - `git-master`: 删除语义需原子提交+可回退。
  - **Skills Evaluated but Omitted**:
    - `playwright`: 无 UI。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with T11, T13)
  - **Blocks**: Final Wave
  - **Blocked By**: T9, T10

  **References**:
  - `crates/agentd-store/src/lib.rs` / `agent.rs` - 已有 `delete_agent` 能力。
  - `crates/agentd-protocol/src/v1.rs` (`DeleteAgentResponse`) - 协议类型已存在可复用。
  - `crates/agentd-daemon/src/main.rs` (`handle_rpc_request`) - 新增 method 分支位置。
  - `crates/agentctl/src/main.rs` - 命令树扩展。

  **Acceptance Criteria**:
  - [x] delete 成功后 `agent list` 不再包含目标 agent。
  - [x] delete 不存在 agent 时返回稳定 not found。
  - [x] delete 事件进入 audit。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: delete happy path
    Tool: Bash
    Preconditions: 创建一个临时 agent
    Steps:
      1. agentctl agent delete --agent-id <id> --json
      2. agentctl agent list --json 检查不存在
      3. agentctl agent audit --agent-id <id> --json 检查删除事件（如可查询）
    Expected Result: 删除成功且列表无该 agent
    Failure Indicators: method not found、删除后仍在列表中
    Evidence: .sisyphus/evidence/task-12-delete-happy.json

  Scenario: delete 不存在 agent failure path
    Tool: Bash
    Preconditions: 随机 UUID
    Steps:
      1. 执行 delete
      2. 捕获错误
    Expected Result: 退出码非 0，NOT_FOUND
    Evidence: .sisyphus/evidence/task-12-delete-notfound-error.txt
  ```

  **Commit**: YES
  - Message: `feat(agentctl): wire delete agent rpc end-to-end`
  - Files: `crates/agentctl/src/main.rs`, `crates/agentd-daemon/src/main.rs`, `crates/agentd-protocol/src/*`
  - Pre-commit: `cargo test -p agentd-daemon && cargo test -p agentctl`

- [x] 13. `configs/agents/*.toml` profile 加载与校验

  **What to do**:
  - 增加目录扫描与 TOML 解析，把 profile 文件映射为 `AgentProfile`（至少覆盖路线图示例字段）。
  - 增加 schema 校验与错误定位（文件名+行号或字段路径）。
  - 提供 `configs/agents/example.toml`。

  **Must NOT do**:
  - 不覆盖现有 `CreateAgent` RPC 参数路径。
  - 不将 profile 解析错误降级为 warning 后继续运行。

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: 牵涉配置加载、校验与兼容策略。
  - **Skills**: [`git-master`]
    - `git-master`: 配置/解析/测试需要一致提交。
  - **Skills Evaluated but Omitted**:
    - `skill-creator`: 本任务非技能定义。

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 3 (with T11, T12)
  - **Blocks**: Final Wave
  - **Blocked By**: T9

  **References**:
  - `design/mvp-implementation-roadmap-v1.md`（Agent Profile TOML 样例）- 目标字段定义。
  - `crates/agentd-core/src/profile.rs` - 目标结构体映射。
  - `crates/agentd-daemon/src/main.rs` (`load_config`) - 现有 TOML 加载入口可扩展。
  - `configs/agentd.toml` - 当前配置风格参考。

  **Acceptance Criteria**:
  - [x] `configs/agents/example.toml` 可被成功加载并映射为 `AgentProfile`。
  - [x] 非法 profile 文件报错可定位。
  - [x] 现有启动流程在无 `configs/agents` 时行为不变。

  **QA Scenarios (MANDATORY)**:
  ```
  Scenario: profile 加载 happy path
    Tool: Bash
    Preconditions: 提供合法 example.toml
    Steps:
      1. 启动 daemon（指向含 profiles 的配置）
      2. 调用相应查询接口（或日志）确认 profile 被识别
    Expected Result: 退出码 0，profile 生效
    Failure Indicators: 启动失败或 profile 被忽略
    Evidence: .sisyphus/evidence/task-13-profile-load-happy.txt

  Scenario: 非法 profile failure path
    Tool: Bash
    Preconditions: 放置格式错误 toml
    Steps:
      1. 启动 daemon
      2. 捕获错误输出
    Expected Result: 退出码非 0，错误含文件路径与字段定位
    Evidence: .sisyphus/evidence/task-13-profile-load-invalid-error.txt
  ```

  **Commit**: YES
  - Message: `feat(config): add agent profile directory loading and validation`
  - Files: `configs/agents/example.toml`, `crates/agentd-daemon/src/main.rs`, `crates/agentd-core/src/*`
  - Pre-commit: `cargo test -p agentd-daemon`

---

## Final Verification Wave (MANDATORY)

- [ ] F1. **Plan Compliance Audit** — `oracle`
  对照本计划逐项验证 Must Have / Must NOT Have，检查 `.sisyphus/evidence/` 证据完整性。

- [ ] F2. **Code Quality Review** — `unspecified-high`
  运行 `cargo test` / `pytest` / lint，扫描临时代码、空 catch、无效重试与未使用依赖。

- [ ] F3. **Real Manual QA (Agent-Executed)** — `unspecified-high`
  复跑所有 task 的 QA scenarios（正负例全量），确认真实 provider 路径与 anti-mock 约束成立。

- [ ] F4. **Scope Fidelity Check** — `deep`
  核对实现与任务边界，识别 scope creep 与未登记变更。

---

## Commit Strategy

- **C1**: `feat(agent-lite): add real openai-compatible loop with tool-calling and retries`
- **C2**: `test(gates): add real one-api closure gate with anti-mock assertions`
- **C3**: `feat(agentctl): add inspect and delete commands with rpc wiring`
- **C4**: `feat(config): add agent profile toml directory loading and validation`

---

## Success Criteria

### Verification Commands
```bash
bash scripts/gates/task-real-closure-gate.sh
# Expected: PASS with provider evidence + usage reconciliation

bash scripts/gates/task-real-closure-gate.sh --negative-one-api-disabled
# Expected: non-zero exit + EXPECTED_FAILURE one_api_disabled

bash scripts/gates/task-real-closure-gate.sh --negative-invalid-credentials
# Expected: non-zero exit + provider auth/network failure marker

bash scripts/gates/task-real-closure-gate.sh --negative-policy-deny
# Expected: non-zero exit + provider_call_attempted=false
```

### Final Checklist
- [ ] All "Must Have" present
- [ ] All "Must NOT Have" absent
- [ ] Real-path gate and all negative gates behave as specified
- [ ] TDD tests pass across Rust/Python
