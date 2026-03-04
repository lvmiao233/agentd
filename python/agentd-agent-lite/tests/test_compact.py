from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


def test_auto_compact_preserves_key_facts() -> None:
    session = _CLI_MODULE.AgentSession("agent-compact-facts", max_context_tokens=12)
    session._append_message("user", "remember fact: project codename is atlas")
    session._append_message("assistant", "confirmed, codename atlas")

    result = session.chat(
        "please continue with the atlas migration checklist now",
        run_turn=lambda: {
            "output": "ok",
            "tool_calls": [],
            "input_tokens": 1,
            "output_tokens": 1,
            "total_tokens": 2,
        },
    )

    assert result["compact_triggered"] is True
    active_branch = session._get_active_branch()
    compact_summaries = [
        message
        for message in active_branch
        if message.get("role") == "system"
        and isinstance(message.get("compact"), dict)
        and message["compact"].get("kind") == "auto_compact_summary"
    ]
    assert compact_summaries
    assert "atlas" in compact_summaries[0]["content"]
