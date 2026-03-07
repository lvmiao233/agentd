# Learnings — post-mvp-chat-agent-loop

- 初始化：记录可复用实现经验与外部参考结论；只追加，不覆盖。

- 2026-03-08 (Iteration 1): 当前 Web chat 已经用上 `Conversation` / `Message` / `PromptInput` / `Tool`，但真正限制体验的不是这些组件不够，而是上游事件没有按生命周期尽早发出，导致 ai-elements 无法展示其状态优势。
- 2026-03-08 (Iteration 1): `components/ai-elements/tool.tsx` 已原生支持 `input-streaming` / `input-available` / `output-available` / `output-error`，这意味着只要上游改成标准事件序列，就能立刻获得更好的运行中状态展示，无需重复造轮子。
- 2026-03-08 (Iteration 1): Vercel AI SDK 官方流事件中明确存在 `tool-input-start`，这是让工具在“真正开始时”就被前端感知的正确扩展点，比自定义 loading message 更稳。
- 2026-03-08 (Iteration 1): agent-lite 当前根因位于 `_run_chat_turn`：旧实现把 `tool-input-available` 放在真实工具调用完成之后才发出，天然把“运行中”压缩成“瞬时完成态”。
- 2026-03-08 (Iteration 1): OpenCode 一类产品并不是靠把工具输出堆到最后才显得“聪明”，而是把 pending / running / completed 这些中间态做成一等 UI；这说明事件模型先行，比继续堆文案更重要。
- 2026-03-08 (Iteration 1): Cherry Studio 的 chunk 模型把 `pending -> in-progress -> complete` 拆成明确事件类型，说明如果后续需要更强的 agent 可视化，可以在现有 AI SDK 事件之外再加一层稳定 chunk 适配，而不是让页面自己猜状态。
- 2026-03-08 (Iteration 1): ai SDK 的 `tool-output-error` 是独立 chunk 类型；如果上游只塞 `errorText` 进 `tool-output-available`，UI 语义会被污染，状态徽标也会误导用户。
- 2026-03-08 (Iteration 1): `tool-input-start` 对应的 `input` 可能是 `undefined`，所以任何 ToolInput/CodeBlock 组件都不能假设“只要是工具块就必有参数 JSON”。
- 2026-03-08 (Iteration 2): 即使后端审批机制还不是 AI SDK 原生 `approval-requested` tool part，只要已有独立 approval queue，也可以先通过 feed 合并与会话内渲染把体验拉近到 ai-elements confirmation 模式。
- 2026-03-08 (Iteration 2): 浏览器侧用接口拦截验证 approval flow 非常高性价比：既能真实点按钮，又不依赖 daemon 当下有没有恰好挂起的审批任务。
- 2026-03-08 (Iteration 3): ai-elements 官方 reasoning 模式不是把每个 reasoning part 单独丢进消息流，而是把 reasoning 汇总到一个 disclosure 里，再把正文作为主要内容显示；这比裸 `<pre>` 更符合聊天阅读节奏。
- 2026-03-08 (Iteration 3): sources 适合放在 assistant message 之前或同级的 disclosure 区，而不是和正文混在同一段落里；这样既能保留引用，又不会破坏正文可读性。
- 2026-03-08 (Iteration 3): Playwright 做 chat SSE 回放时，必须返回真正的 SSE 帧分隔（`data: ...\n\n`）；只有单换行时，`useChat` 不会稳定消费 assistant parts。
- 2026-03-08 (Iteration 4): AI SDK 官方审批模式是 `approval-requested` state 直接挂在 tool part 上，但当前 agentd daemon 暂时只暴露 approval queue；因此前端需要一个“尽量正确、可回退”的 tool-name 分配层。
- 2026-03-08 (Iteration 4): `mcp.fs.read_file`、`fs.read_file`、`read_file` 这类 alias 足以覆盖很多真实场景；但只靠 name matching 天然会在“同名工具多次调用”时出现歧义，因此后续仍需后端补 call-level correlation。
- 2026-03-08 (Iteration 5): ai-elements suggestion 模式的关键价值不是视觉上的 chip，而是“把下一步 prompt 变成低摩擦动作”；对 coding-agent 来说，这比单纯的 regenerate 更能推动多轮执行。
- 2026-03-08 (Iteration 5): 将 suggestion 生成逻辑独立到 `buildFollowUpSuggestions` 后，可以按状态（ready/error/streaming）、是否有工具、是否有 pending approval 持续演进，而不用反复污染 `page.tsx`。
- 2026-03-08 (Iteration 6): ai-elements `MessageBranch` 非常适合接 regenerate history：只要把旧 assistant 版本在 regenerate 前存档，再把“旧版本 + 当前版本”作为 children 交给 `MessageBranchContent`，就能立刻获得 prev/next/page 控件。
- 2026-03-08 (Iteration 6): 在真实页面里，branch selector 与现有 regenerate/copy actions 可以并存；这比单独做一个“历史记录弹窗”更贴近 OpenCode/OpenClaw 的 inline 工作流。
- 2026-03-08 (Iteration 7): `useChat` / `createUIMessageStream` 只要收到原样 `text-delta`，前端就能保留 fenced markdown；因此在后端还没产出结构化 artifact part 之前，可以先做 message-text 级 preview 提取闭环。
- 2026-03-08 (Iteration 7): 对 chat UI 做 deterministic 浏览器回放时，用“真实 `next start` + mock daemon `/rpc`”比直接在浏览器里拦截 `/api/chat` 更稳，因为可以同时覆盖 `/api/agents`、`/api/approvals` 与 route 层的 streaming 适配。
- 2026-03-08 (Iteration 7): `@streamdown/code` 的 Shiki 插件虽然暴露了标准 highlighter 接口，但不认识 `svg` fence；在消息层包一层 alias plugin 是低成本兼容现有生态的好办法。
