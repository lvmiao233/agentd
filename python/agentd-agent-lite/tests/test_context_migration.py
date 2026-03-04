from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


def test_build_migration_summary_includes_key_fact() -> None:
    session = _CLI_MODULE.AgentSession("agent-migrate-summary")
    session._append_message("user", "remember codename atlas")
    session._append_message("assistant", "atlas migration checklist captured")

    summary = _CLI_MODULE.build_migration_summary(
        session,
        key_files=["README.md", "crates/agentd-daemon/src/main.rs"],
    )

    assert summary["message_count"] == 2
    assert summary["source_head_id"] == session.head_id
    assert "atlas" in summary["text"]
    assert summary["key_files"] == [
        "README.md",
        "crates/agentd-daemon/src/main.rs",
    ]


def test_snapshot_export_restore_roundtrip() -> None:
    session = _CLI_MODULE.AgentSession("agent-migrate-snapshot", max_context_tokens=64)
    root = session._append_message("system", "boot")
    session._append_message("user", "continue task 26")
    head = session._append_message("assistant", "task 26 context prepared")
    session.tool_results_cache["call-1"] = {"status": "ok"}

    snapshot = _CLI_MODULE.export_session_snapshot(
        session,
        working_directory={
            "README.md": "# migrated",
            "notes/task26.txt": "l2 snapshot",
        },
    )
    restored = _CLI_MODULE.restore_session_from_snapshot(
        snapshot,
        max_context_tokens=96,
    )

    assert restored.agent_id == "agent-migrate-snapshot"
    assert restored.max_context_tokens == 96
    assert restored.head_id == head["id"]
    assert len(restored.messages) == 3
    assert restored.messages[0]["id"] == root["id"]
    assert restored.messages[-1]["id"] == head["id"]
    assert restored.tool_results_cache["call-1"]["status"] == "ok"
