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
- 2026-03-08 (Iteration 6): regenerate 旧行为会直接抹掉前一版 assistant 回复，导致用户无法比较 alternative answer / plan / patch direction；这和 coding-agent 的探索式工作流不匹配。
- 2026-03-08 (Iteration 6): 当前 branch history 只在前端内存里保留；刷新页面或切 agent 后会丢失。若后续要做真正的会话级 branch 持久化，需要在 daemon/session 层增加相应存储或 metadata 通道。
- 2026-03-08 (Iteration 7): 当前 artifact preview 仍然是从 assistant markdown 正文里做 regex 抽取，而不是第一类 stream part；这意味着 tool output、结构化文件产物、以及非 text assistant part 还无法被统一纳入 preview 管道。
- 2026-03-08 (Iteration 7): 由于 artifact 目前只是“从正文中提取再额外渲染”，原始 markdown code block 仍会继续出现在消息正文里，形成“preview card + code block”双份信息；后续若后端提供 artifact part，应考虑 suppress duplicate fence render。
- 2026-03-08 (Iteration 8): 之前 chat 页面对 PromptInput 的 attachments 能力几乎是“半接入”状态：输入组件支持文件，但页面 submit 逻辑直接丢弃 `message.files`，route 也完全忽略 file part，导致上传只能当装饰，不能成为 agent 上下文。
- 2026-03-08 (Iteration 8): 当前 attachment prompt serialization 仍然是字符串内联方案，虽然能立即服务 coding 场景，但对大文件、二进制、以及真正多模态模型输入都不够理想；后续如果 daemon 支持 structured file/message part，需要把这一层升级为第一类协议字段。
- 2026-03-08 (Iteration 9): 当前 chat 已经有不少局部增强（approval、reasoning、sources、artifact、attachments），但仍然缺少一个把这些信号汇总起来的“run-level”视图；用户必须自己从多处 UI 片段脑补 agent 当前状态，这是持续 coding 体验的主要认知负担。
- 2026-03-08 (Iteration 9): 仅靠最后一条 assistant 文本和 tool 卡片，用户很难快速回答“现在卡在哪、在做什么、下一步会继续什么”；因此需要一个比普通 message 更高层的 progress summary，而不是继续增加零散 chips 或提示文案。
- 2026-03-08 (Iteration 10): 即使已经有 follow-up suggestions，当前 chat 仍然缺一个“随时可调出”的命令入口；用户必须先滚到最后一条 assistant 消息附近，才能点击继续/验证类 prompt，这对长会话尤其别扭。
- 2026-03-08 (Iteration 10): 当前 PromptInput 工具区仍然偏“附件输入框”而不是“agent 控制台”；如果不把命令能力塞进输入侧，持续 coding 仍会被多处散落的 action 按钮打断。
- 2026-03-08 (Iteration 11): run overview 虽然已经能汇总当前 turn / tool / approval 状态，但如果用户看完还得自己在长会话里滚动定位对应节点，它就仍然只解决了一半问题；缺失的正是“从摘要回到现场”的导航能力。
- 2026-03-08 (Iteration 12): 现在用户虽然能继续提问、能 regenerate、能做分支、也能通过 run overview 找到上下文，但仍然缺少一个明确的“把整个会话恢复到之前某一步”的机制；这会让长任务探索在走偏后恢复成本偏高。
- 2026-03-08 (Iteration 13): 即使有 command palette 和 checkpoints，如果用户每次继续任务都还得滚到最新 assistant 消息附近才能点 suggestion，长会话的推进仍然不够顺滑；因此需要一个始终贴近 run overview 的顶部 continuation 入口。
- 2026-03-08 (Iteration 14): 有了 checkpoint 和 resume bar 之后，剩余摩擦变成“checkpoint 藏在消息中间，不够像会话级地图”；用户仍然缺一个顶部可扫描、可跳转、可恢复的阶段时间线。
