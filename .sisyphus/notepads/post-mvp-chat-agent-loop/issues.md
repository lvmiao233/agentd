# Issues — post-mvp-chat-agent-loop

- 初始化：记录阻塞、异常与修复状态；只追加，不覆盖。

- 2026-03-08 (Iteration 1): 系统内置 `sg` 命令实际是 `newgrp` 而不是 ast-grep 二进制，无法按预期执行结构化搜索；当前改用内置 `ast_grep_*` 工具继续完成搜索，不阻塞实现。
- 2026-03-08 (Iteration 1): Python 文件 `python/agentd-agent-lite/src/agentd_agent_lite/cli.py` 存在大量历史 basedpyright warning，并非本轮引入；本轮要求确保新增改动不引入 error，并通过运行测试验证行为正确。
- 2026-03-08 (Iteration 2): 仓库自定义 web spec runner 只是把 `*.spec.ts` 复制为 `.compiled.mjs`，不会转译 TypeScript 类型语法；因此测试辅助库需采用 `js/mjs + d.ts` 模式，不能直接在 spec 中引入带类型语法的 `.ts` 实现。
- 2026-03-08 (Iteration 3): Playwright MCP 在同一浏览器上下文中会保留旧的 Next.js chunk URL 缓存；为避免 `ChunkLoadError` 干扰验证，本轮改用新的 dev 端口 `4174` 进行真实回放。
- 2026-03-08 (Iteration 4): Playwright 在 Next.js dev server 上仍会伴随一些 RSC/static asset 400 噪音，但不影响本轮 chat submit 与 inline approval 验证；后续若要做更稳定的浏览器回放，最好固定独立端口与干净浏览器上下文。
