from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


def test_append_message_sets_parent() -> None:
    session = _CLI_MODULE.AgentSession("agent-session-tree")

    root = session._append_message("user", "hello")
    child = session._append_message("assistant", "world")

    assert root["parent_id"] is None
    assert child["parent_id"] == root["id"]
    assert session.head_id == child["id"]


def test_get_active_branch_returns_ordered_chain() -> None:
    session = _CLI_MODULE.AgentSession("agent-session-tree")

    session._append_message("system", "boot")
    session._append_message("user", "step-1")
    tail = session._append_message("assistant", "step-2")

    branch = session._get_active_branch()

    assert [item["role"] for item in branch] == ["system", "user", "assistant"]
    assert branch[-1]["id"] == tail["id"]


def test_get_active_branch_follows_backtracked_head() -> None:
    session = _CLI_MODULE.AgentSession("agent-session-tree")

    root = session._append_message("system", "root")
    session._append_message("assistant", "first-branch")
    session.head_id = root["id"]
    alt = session._append_message("assistant", "second-branch")

    branch = session._get_active_branch()

    assert [item["content"] for item in branch] == ["root", "second-branch"]
    assert branch[-1]["id"] == alt["id"]


def test_get_active_branch_handles_missing_parent() -> None:
    session = _CLI_MODULE.AgentSession("agent-session-tree")
    session.messages.extend(
        [
            {
                "id": "root",
                "parent_id": None,
                "role": "system",
                "content": "root",
            },
            {
                "id": "child",
                "parent_id": "missing-parent",
                "role": "assistant",
                "content": "dangling",
            },
        ]
    )
    session.head_id = "child"

    branch = session._get_active_branch()

    assert [item["id"] for item in branch] == ["child"]
