# Issues — post-mvp-chat-agent-loop

- 初始化：记录阻塞、异常与修复状态；只追加，不覆盖。

- 2026-03-08 (Iteration 1): 系统内置 `sg` 命令实际是 `newgrp` 而不是 ast-grep 二进制，无法按预期执行结构化搜索；当前改用内置 `ast_grep_*` 工具继续完成搜索，不阻塞实现。
- 2026-03-08 (Iteration 1): Python 文件 `python/agentd-agent-lite/src/agentd_agent_lite/cli.py` 存在大量历史 basedpyright warning，并非本轮引入；本轮要求确保新增改动不引入 error，并通过运行测试验证行为正确。
