# Decisions — mvp-real-llm-closure

- 初始化：记录关键决策与理由；只追加，不覆盖。

- 2026-03-03: 预检脚本采用"严格真实环境就绪"策略：缺失 token 直接失败（`ONE_API_TOKEN_MISSING`），不沿用 `infra/one-api.sh health` 的 token 缺失跳过行为。
- 2026-03-03: 失败时统一写入 `.sisyphus/evidence/preflight-real-oneapi-error.txt`（可被 `--error-evidence` 覆盖），并携带请求响应片段用于排障。
- 2026-03-03: 模型可见性定义为 `/v1/models` 返回 200 且 `data` 为非空数组；空列表视为未就绪（`ONE_API_MODELS_EMPTY`）。
- 2026-03-03: anti-mock 枚举语义固定为 `usage_source(provider|estimated)` 与 `transport_mode(real|simulated)`，禁止扩展值在 gate 中"宽松通过"。
- 2026-03-03: real-path gate 采用强拒绝策略：`provider_request_id` 为空或 `transport_mode=simulated` 必失败，并输出 `ASSERT anti_mock_schema=FAIL` 及错误证据文件。
- 2026-03-03 (T2): 配置加载采用"先 env 后 CLI"优先级，CLI 参数直接覆盖环境变量，便于测试与脚本编排。
- 2026-03-03 (T2): 验证失败时输出结构化 JSON 错误（包含 `stage: config`, `error: invalid_config`, `message`），与 gate 断言格式保持一致。
- 2026-03-03 (T2): 使用 `openai>=1.0.0` 而非固定版本，兼容 One-API 及其他 OpenAI-compatible 端点。
- 2026-03-03 (T2): 默认超时 60 秒可通过 `ONE_API_TIMEOUT` 或 `--timeout` 覆盖，支持灵活配置。
- 2026-03-03 (T5): real-closure gate 独立于 mock gate 实现，不复用 `one_api.enabled=false` 路径作为 pass 条件，确保使用真实 LLM 调用。
- 2026-03-03 (T5): 负例模式支持 `--negative-one-api-disabled` 和 `--negative-invalid-credentials`，通过配置差异触发不同失败场景。
- 2026-03-03 (T5): 机器可读断言格式 `ASSERT <step>=PASS|FAIL` 统一所有 gate 输出，便于 CI 解析与自动化判定。
- 2026-03-03 (T5): happy path 要求 `total_tokens > 0` 作为 real LLM 调用的直接证据，与 mock 模式区分。
- 2026-03-03 (T3): agent-lite 单轮 real call 保留现有 `AuthorizeTool/RecordUsage` RPC 前后置顺序不变，仅替换 LLM 响应来源（由本地拼接改为 provider 返回），避免破坏既有治理链路。
- 2026-03-03 (T3): 错误分类在 CLI 输出中标准化为 `provider.auth` / `provider.network` / `provider.http` / `provider.unknown`，并在可用时透传 `provider_request_id` 以支持失败证据追踪。
- 2026-03-03 (T4): anti-mock 断言失败统一输出双轨信息：`anti_mock_reason`（机器码）+ `anti_mock_error`（可读消息），错误证据文件落盘格式固定为 `<CODE>: <message>`。
- 2026-03-03 (T4): real-path gate 在主流程内强制执行 anti-mock schema 断言；若断言失败，gate 以 `reason=anti_mock_schema_failed` 退出，避免未校验证据进入后续对账步骤。
- 2026-03-03 (T1): 预检 `--dry-run` 退出码固定为 0，`REASON_CODE=DRY_RUN` 且不触发网络访问，用于本地/CI 快速校验脚本连线。
- 2026-03-03: Rewrote the most recent local commits so the messages and bodies comply with the repository requirements without forbidden trailers or collaboration lines.
