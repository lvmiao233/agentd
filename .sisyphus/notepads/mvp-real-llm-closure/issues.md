# Issues — mvp-real-llm-closure

- 初始化：记录阻塞与修复状态；只追加，不覆盖。

-03-03: 2026- `pytest` 在当前环境中不可直接调用（`pytest: command not found`）；改用 `uv run pytest -q ...` 作为稳定执行方式。
- 2026-03-03: 新增测试初版触发 basedpyright 警告（未注解属性、override 标注等）；补齐类型注解与 `@override` 后清零 diagnostics。
- 2026-03-03: `argparse.Namespace` 在 basedpyright 下会引入 `Any` 污染，导致断言脚本出现诊断噪声；通过 `CliArgs` dataclass 显式收敛参数类型后已解决。
- 2026-03-03 (T2): config.py 使用 `urlparse` 验证 URL 格式，但不支持 `file://` 等非网络 URL；当前仅验证 scheme + netloc 存在即通过。
- 2026-03-03 (T2): LlmConfig dataclass 字段默认值设为空字符串以支持 Optional 构造，但 validate_config 阶段会捕获空值并抛出明确错误；TDD 测试覆盖此边界。
- 2026-03-03 (T5): 初始脚本编辑时出现逻辑错误（heredoc 嵌套导致代码块错位），通过完整重写脚本解决；建议后续 gate 编写使用独立代码块而非增量编辑。
- 2026-03-03 (T3): 基于静态导入 `from openai import ...` 在当前 basedpyright 环境会报 `reportMissingImports`；改为运行时 `importlib.import_module("openai")` 并懒加载类型后，兼顾测试可 monkeypatch 与诊断通过。
- 2026-03-03 (T4): 初版 anti-mock 测试仅校验自然语言错误文案，机器解析稳定性不足；已改为同时校验结构化 reason code 并在断言脚本中实现对应输出。
- 2026-03-03 (T1): 计划中的 pre-commit 命令依赖 `--dry-run`，脚本初版未实现该参数会导致 unknown option；已补齐并加测试覆盖。
