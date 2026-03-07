# Problems — post-mvp-chat-agent-loop

- 初始化：记录问题复盘与根因；只追加，不覆盖。

- 2026-03-08 (Iteration 1): 用户感知到“必须等所有工具做完才有内容”的根因并不完全在前端，而是 agent-lite 事件排序错误：`tool-input-available` 与 `tool-output-available` 旧实现都发生在工具执行后，前端自然无法显示真实的开始/运行过程。
- 2026-03-08 (Iteration 1): 现有仓库只收录了部分 ai-elements 组件副本，导致很多官方能力（如 sources / reasoning / suggestions / confirmation / preview）尚未进入产品层；后续迭代应优先引入受认可组件，而非继续手写零散替代物。
