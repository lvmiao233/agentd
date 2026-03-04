from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


def test_save_load_roundtrip(tmp_path: Path) -> None:
    session = _CLI_MODULE.AgentSession("agent-persist", max_context_tokens=32)
    root = session._append_message("system", "root")
    session._append_message("user", "turn-1")
    head = session._append_message("assistant", "turn-1-answer")

    target = tmp_path / "session.jsonl"
    _CLI_MODULE.run_session_command(
        command="save",
        file_path=str(target),
        session=session,
    )

    loaded = _CLI_MODULE.run_session_command(
        command="load",
        file_path=str(target),
        agent_id="agent-persist",
    )

    assert loaded.agent_id == "agent-persist"
    assert loaded.head_id == head["id"]
    assert len(loaded.messages) == 3
    assert loaded.messages[0]["id"] == root["id"]
    assert loaded.messages[1]["parent_id"] == root["id"]
    assert loaded.messages[2]["id"] == head["id"]


def test_load_rejects_corrupted_session_file(tmp_path: Path) -> None:
    target = tmp_path / "session-corrupted.jsonl"
    target.write_text(
        '{"kind":"session","agent_id":"a"}\n{not-json}\n', encoding="utf-8"
    )

    try:
        _CLI_MODULE.run_session_command(
            command="load",
            file_path=str(target),
            agent_id="agent-persist",
        )
    except ValueError as err:
        assert "session parse error" in str(err)
    else:
        raise AssertionError("expected ValueError for corrupted session file")
