# Problems — post-mvp-chat-agent-loop

- 初始化：记录问题复盘与根因；只追加，不覆盖。

- 2026-03-08 (Iteration 1): 用户感知到“必须等所有工具做完才有内容”的根因并不完全在前端，而是 agent-lite 事件排序错误：`tool-input-available` 与 `tool-output-available` 旧实现都发生在工具执行后，前端自然无法显示真实的开始/运行过程。
- 2026-03-08 (Iteration 1): 现有仓库只收录了部分 ai-elements 组件副本，导致很多官方能力（如 sources / reasoning / suggestions / confirmation / preview）尚未进入产品层；后续迭代应优先引入受认可组件，而非继续手写零散替代物。
- 2026-03-08 (Iteration 2): approval 旧体验的问题不只是样式丑，而是信息架构错误：等待批准的动作显示在聊天区外，用户需要在“阅读 agent 输出”和“进行审批”之间来回切换。
- 2026-03-08 (Iteration 2): 现阶段 approval queue 与具体 tool part 还没有稳定的 message-level 绑定字段，因此本轮先实现 conversation-level approval inbox；后续若补上 trace/toolCall 映射，可继续把审批卡片进一步贴到具体 Tool 节点内。
- 2026-03-08 (Iteration 3): reasoning / source 旧体验的问题不只是外观简陋，而是层级混乱：元信息与正文没有明确区分，导致 assistant 输出既难扫读，也难按需展开上下文。
- 2026-03-08 (Iteration 3): 当前 sources 仍然是 message 级 disclosure，而不是 inline citation；若后续需要 OpenCode/OpenClaw 那种更强的引用体验，应继续补 `inline-citation` 或 message-level source anchoring，而不是退回手写链接列表。
- 2026-03-08 (Iteration 4): approval inbox 虽然已经在会话内，但如果不贴回具体 Tool，用户仍然要自己猜“这一条审批到底对应哪个调用”；这对持续 coding 场景是认知负担。
- 2026-03-08 (Iteration 4): 当前 inline approval 仍受限于 fuzzy matching：一旦同名工具在单轮消息里多次出现，前端无法 100% 判断审批属于哪个 call。真正的根治方案仍是 daemon/stream 层补 `toolCallId` 或等价关联字段到 approval payload。
- 2026-03-08 (Iteration 5): 当前 chat 即使已有 regenerate/copy，也仍然缺少“顺着上一条回复继续推进任务”的低摩擦入口；用户必须自己重新组织提示词，会打断持续 coding 的节奏。
- 2026-03-08 (Iteration 5): 目前 follow-up suggestions 还是启发式生成，并没有读取模型原生的 suggested-reply chunk；如果后续接入更标准的 suggestion part，应优先接入流式数据而不是继续手写更多 heuristics。
