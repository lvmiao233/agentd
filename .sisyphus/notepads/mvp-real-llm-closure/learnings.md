# Learnings — mvp-real-llm-closure

- 初始化：本 notepad 用于记录可复用实现/验证经验；只追加，不覆盖。

- 2026-03-03: 新增 `scripts/gates/preflight-real-oneapi.sh` 时，沿用 gate 脚本约定（`set -euo pipefail`、`SCRIPT_DIR/REPO_ROOT/EVIDENCE_DIR`、`require_cmd`、固定 evidence 路径）能显著降低集成摩擦。
- 2026-03-03: 真实 One-API 预检建议同时尝试 `/health` 与 `/api/status`，避免单端点差异导致误判；通过后再做 `/v1/models` 可见性校验更稳。
- 2026-03-03: 机器可读 marker 统一输出 `HEALTH`/`MODELS_CHECKED`/`ENV_READY`/`REASON_CODE`，可直接供后续 gate 解析。
- 2026-03-03: anti-mock 字段校验建议独立为可复用断言脚本（`scripts/gates/assert-anti-mock-evidence.py`），将 schema 合法性与 real-path 强约束（`usage_source=provider`、`transport_mode=real`）集中管理，后续 gate 可直接复用。
- 2026-03-03: real-path 拒绝分层为"通用 schema 错误 + 反模拟专用错误码文案（`MOCK_EVIDENCE_REJECTED`）"，便于机器解析失败原因并区分字段缺失 vs 模拟链路。
- 2026-03-03 (T2): agent-lite 配置注入遵循"env 优先 + CLI 覆盖"原则，环境变量命名采用 `ONE_API_*` 前缀（`ONE_API_BASE_URL`, `ONE_API_TOKEN`, `ONE_API_MODEL`, `ONE_API_TIMEOUT`），与 One-API 部署规范对齐。
- 2026-03-03 (T2): TDD 流程有效 - 先写 test_config.py 验证 RED 阶段（config.py 不存在时测试失败），再实现 config.py 使测试通过，确保测试覆盖 missing/invalid/valid/env-inject/cli-override 场景。
- 2026-03-03 (T2): `--dry-run` 模式支持配置验证但不触发网络调用，与 gate 脚本的 dry-run 约定保持一致（`--dry-run` 输出结构化 JSON，便于后续 gate 解析）。
- 2026-03-03 (T2): 现有 RPC 通道（AuthorizeTool/RecordUsage）保持兼容，仅在调用前增加 LLM 配置加载层，不改变已有接口契约。
- 2026-03-03 (T5): task-real-closure-gate.sh 沿用现有 gate 模式（`set -euo pipefail`、evidence 路径、require_cmd、cleanup trap），确保与其他 gate 脚本一致性。
- 2026-03-03 (T5): 机器可读断言统一使用 `ASSERT <step>=PASS|FAIL` 格式，负例模式额外输出 `EXPECTED_FAILURE <reason>`，便于 CI 自动解析结果。
- 2026-03-03 (T5): 负例模式（one_api_disabled/invalid_credentials）需在无真实 One-API 环境下也能生成有效 evidence，使用 `|| RUN_RESULT=$?` 捕获命令返回码而非依赖 exit 行为。
- 2026-03-03 (T5): 负例模式下 agent_id 可能因创建失败为 "unknown"，evidence 中保留此状态便于后续排查。
- 2026-03-03 (T3): 使用 OpenAI Python SDK `chat.completions.with_raw_response.create(...).parse()` 能同时拿到结构化 completion 与响应头，从而优先使用 `completion._request_id`，回退 `x-request-id`，再回退 body `id`，满足 provider request-id 证据提取需求。
- 2026-03-03 (T3): 单轮真实调用在 provider `usage` 缺失时可安全降级为本地 token 估算，但必须显式标记 `usage_source=estimated` 与 `transport_mode=real`，便于后续 anti-mock gate 区分证据来源。
- 2026-03-03 (T4): anti-mock 断言输出增加机器可解析 `ASSERT anti_mock_reason=<CODE>`，可稳定区分 `MISSING_PROVIDER_REQUEST_ID`、`INVALID_USAGE_SOURCE`、`MOCK_EVIDENCE_REJECTED` 等失败类型。
- 2026-03-03 (T4): `task-real-closure-gate.sh` 在 happy path 直接抽取 agent run 的 `llm` 证据并调用 `assert-anti-mock-evidence.py --real-path`，使 real gate 与 anti-mock schema 校验复用同一断言入口。
- 2026-03-03 (T1): 预检脚本补充 `--dry-run` 可在无网络/无凭据环境下做结构化输出自检，持续保留 `HEALTH/MODELS_CHECKED/ENV_READY/REASON_CODE` 机器标记。
