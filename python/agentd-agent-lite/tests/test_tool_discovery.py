import argparse
import json
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from typing import Any

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


def _make_args() -> argparse.Namespace:
    return argparse.Namespace(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-tool-discovery",
        prompt="read file",
        model="gpt-4o-mini",
        tool="builtin.lite.echo",
        base_url="http://localhost:3000/v1",
        api_key="token-123",
        timeout=15,
        max_iterations=3,
        max_retries=1,
        dry_run=False,
    )


def test_dynamic_discover_tools(monkeypatch, capsys) -> None:
    state = {"llm_calls": 0}

    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        state["llm_calls"] += 1
        if state["llm_calls"] == 1:
            return {
                "output": "",
                "input_tokens": 10,
                "output_tokens": 4,
                "total_tokens": 14,
                "provider_request_id": "req-discover-1",
                "request_id_source": "response._request_id",
                "provider_model": "gpt-4o-mini",
                "usage_source": "provider",
                "transport_mode": "real",
                "tool_calls": [
                    {
                        "id": "call-1",
                        "name": "mcp.fs.read_file",
                        "arguments": '{"path":"README.md"}',
                    }
                ],
            }

        return {
            "output": "done",
            "input_tokens": 5,
            "output_tokens": 3,
            "total_tokens": 8,
            "provider_request_id": "req-discover-2",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        if method == "ListAvailableTools":
            return {
                "tools": [
                    {
                        "server": "mcp-fs",
                        "tool": "fs.read_file",
                        "policy_tool": "mcp.fs.read_file",
                        "description": "Read file from workspace",
                        "input_schema": {
                            "type": "object",
                            "properties": {
                                "path": {"type": "string"},
                            },
                            "required": ["path"],
                        },
                    }
                ]
            }
        if method == "InvokeSkill":
            assert params["server"] == "mcp-fs"
            assert params["tool"] == "fs.read_file"
            return {
                "status": "forwarded",
                "downstream": {
                    "path": params["args"]["path"],
                    "content": "# title",
                },
            }
        if method == "RecordUsage":
            assert params["provider_request_id"] == "req-discover-2"
            return {"accepted": True}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "completed"
    assert payload["tool"]["calls"][0]["name"] == "mcp.fs.read_file"
    assert payload["tool"]["calls"][0]["output"]["status"] == "forwarded"


def test_policy_filtered_tools_not_exposed(monkeypatch) -> None:
    session = _CLI_MODULE.AgentSession("agent-filtered")

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "ListAvailableTools":
            return {
                "tools": [
                    {
                        "server": "mcp-search",
                        "tool": "search.ripgrep",
                        "policy_tool": "mcp.search.ripgrep",
                        "description": "Search code",
                    }
                ]
            }
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    schemas = _CLI_MODULE.discover_openai_tools(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-filtered",
        fallback_tool_name="builtin.lite.echo",
        session=session,
    )

    names = [item["function"]["name"] for item in schemas]
    assert "mcp.search.ripgrep" in names
    assert "mcp.fs.read_file" not in names
