# Decisions — post-mvp-chat-agent-loop

- 初始化：记录 chat / agent 持续改进过程中的关键决策与理由；只追加，不覆盖。

- 2026-03-08 (Iteration 1): 第一轮优先修复“工具生命周期不可感知”的根问题，而不是先堆更多视觉层组件；原因是当前最差体验来自 agent-lite 在工具执行完成后才发出 tool 事件，导致 Web 端只能后知后觉地刷新。
- 2026-03-08 (Iteration 1): 工具流事件语义优先对齐 Vercel AI SDK / ai-elements 现成能力，采用 `tool-input-start -> tool-input-available -> tool-output-available`，避免继续扩展私有事件协议。
- 2026-03-08 (Iteration 1): 保持现有 `createUIMessageStream` / `useChat` / ai-elements `Tool` 渲染链路不推翻重写，只在 agent-lite、daemon 转发层、Web 解析层做最小闭环改动，以降低回归风险。
- 2026-03-08 (Iteration 1): `input-streaming` 在 UI 中改标记为 `Preparing`，用于表达“工具已开始但结果未出”，比此前的 `Pending` 更贴近实际生命周期。
- 2026-03-08 (Iteration 1): 下一轮若继续增强 chat 体验，优先方向不是手写更多状态组件，而是引入/补齐 ai-elements 官方已存在的 `sources`、`reasoning`、`suggestions`、`confirmation`、`preview` 等组件。
- 2026-03-08 (Iteration 1): 工具执行失败时必须输出 `tool-output-error` 而不是继续复用 `tool-output-available`，否则 ai-elements `Tool` 会把失败误渲染为 Completed。
- 2026-03-08 (Iteration 1): 非终止 EOF 不允许降级成 `finishReason=stop`；若流已产生内容但缺失 terminal frame，必须显式标记为 error 并保留已见文本。
- 2026-03-08 (Iteration 2): approval UX 先不做全局横幅/管理面板增强，而是把审批入口塞回会话流本身；因为对持续 coding 场景来说，“正在等你批准什么”必须和聊天上下文处在同一滚动区域。
- 2026-03-08 (Iteration 2): 当前仓库未收录 ai-elements 官方 `confirmation` 组件，因此本轮按官方模式补入本地 `components/ai-elements/confirmation.tsx`，优先复用已有 Button / Message / Conversation 体系，避免再造一套独立审批 UI。
- 2026-03-08 (Iteration 2): 已批准/已拒绝状态需要在会话里短暂保留，而不是点击后直接消失；因此引入 `resolvedApprovals` 本地历史，让用户能看到 agent 流因自己决策而继续推进。
- 2026-03-08 (Iteration 3): reasoning / sources 先做成 message 级 disclosure，而不是继续散落在 `page.tsx` 里用 `<pre>` 和裸链接拼装；因为这类元信息需要“可折叠但不抢正文”。
- 2026-03-08 (Iteration 3): 当前仓库未收录 ai-elements 官方 `reasoning` / `sources` 组件，因此本轮参照官方结构补入本地组件，保持与现有 `message` / `conversation` 体系一致。
- 2026-03-08 (Iteration 3): `source-url` 与 `source-document` 统一通过 `collectSourceParts` 预处理，再交给 Sources 面板渲染，避免在主页面循环里堆更多类型分支。
