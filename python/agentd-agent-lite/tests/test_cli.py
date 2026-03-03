from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from typing import Callable, cast

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)

estimate_tokens = cast(Callable[[str], int], _CLI_MODULE.estimate_tokens)
run_builtin_tool = cast(Callable[[str, str], str], _CLI_MODULE.run_builtin_tool)


def test_estimate_tokens_returns_one_for_empty_input() -> None:
    assert estimate_tokens("") == 1


def test_estimate_tokens_counts_words() -> None:
    assert estimate_tokens("alpha beta gamma") == 3


def test_run_builtin_tool_upper_mode() -> None:
    assert run_builtin_tool("builtin.lite.upper", "hello") == "HELLO"
