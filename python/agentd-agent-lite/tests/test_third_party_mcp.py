from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
import argparse

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


def test_third_party_mcp_onboarding_contract_matrix(monkeypatch) -> None:
    def fake_call_rpc(
        _: str, method: str, params: dict[str, object]
    ) -> dict[str, object]:
        if method == "OnboardMcpServer":
            assert params["name"] == "mcp-figma"
            assert params["transport"] == "stdio"
            assert params["trust_level"] == "community"
            return {
                "status": "onboarded",
                "server": {
                    "server": "mcp-figma",
                    "capabilities": ["figma.export_frame"],
                    "trust_level": "community",
                    "health": "healthy",
                },
            }

        if method == "ListAvailableTools":
            return {
                "tools": [
                    {
                        "server": "mcp-fs",
                        "tool": "fs.read_file",
                        "policy_tool": "mcp.fs.read_file",
                        "description": "Read file",
                        "input_schema": {
                            "type": "object",
                            "properties": {"path": {"type": "string"}},
                            "required": ["path"],
                        },
                    },
                    {
                        "server": "mcp-figma",
                        "tool": "figma.export_frame",
                        "policy_tool": "mcp.figma.export_frame",
                        "description": "Export figma frame",
                        "input_schema": {
                            "type": "object",
                            "properties": {"frame_id": {"type": "string"}},
                            "required": ["frame_id"],
                        },
                    },
                ]
            }

        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    result = _CLI_MODULE.onboard_third_party_mcp_server(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-for-task28",
        name="mcp-figma",
        command="npx -y @modelcontextprotocol/server-figma",
        args=["--readonly"],
    )

    assert result["status"] == "onboarded"
    assert result["onboarding_error"] is None
    assert result["builtin_tools_intact"] is True
    matrix = result["contract_matrix"]
    assert matrix["daemon_to_agent_lite"]["status"] == "compatible"
    assert matrix["daemon_to_web"]["status"] == "compatible"
    tools = result["tools"]
    assert any(item["policy_tool"] == "mcp.figma.export_frame" for item in tools)


def test_third_party_mcp_handshake_failure_isolated(monkeypatch) -> None:
    def fake_call_rpc(_: str, method: str, __: dict[str, object]) -> dict[str, object]:
        if method == "OnboardMcpServer":
            raise _CLI_MODULE.RpcError(
                -32027,
                "onboard mcp server failed: initialize handshake timed out for mcp-figma",
            )

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
                            "properties": {"path": {"type": "string"}},
                            "required": ["path"],
                        },
                    }
                ]
            }

        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    result = _CLI_MODULE.onboard_third_party_mcp_server(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-for-task28",
        name="mcp-figma",
        command="npx -y @modelcontextprotocol/server-figma --fail",
        args=[],
    )

    assert result["status"] == "failed"
    assert result["onboarding_error"]["code"] == -32027
    assert result["builtin_tools_intact"] is True

    session = _CLI_MODULE.AgentSession("agent-for-task28")
    schemas = _CLI_MODULE.discover_openai_tools(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-for-task28",
        fallback_tool_name="builtin.lite.echo",
        session=session,
    )
    names = [item["function"]["name"] for item in schemas]
    assert "mcp.fs.read_file" in names
    assert "mcp.figma.export_frame" not in names


def test_run_once_supports_real_onboard_mode(monkeypatch, capsys) -> None:
    def fake_onboard(**kwargs: object) -> dict[str, object]:
        assert kwargs["socket_path"] == "/tmp/agentd.sock"
        assert kwargs["agent_id"] == "agent-for-task28"
        assert kwargs["name"] == "mcp-figma"
        assert kwargs["command"] == "npx"
        assert kwargs["args"] == ["-y", "@modelcontextprotocol/server-figma"]
        assert kwargs["transport"] == "stdio"
        assert kwargs["trust_level"] == "community"
        return {
            "status": "onboarded",
            "builtin_tools_intact": True,
            "tools": [],
            "contract_matrix": {},
        }

    monkeypatch.setattr(_CLI_MODULE, "onboard_third_party_mcp_server", fake_onboard)

    exit_code = _CLI_MODULE.run_once(
        argparse.Namespace(
            mode="onboard-mcp",
            socket_path="/tmp/agentd.sock",
            agent_id="agent-for-task28",
            prompt=None,
            model=None,
            tool="builtin.lite.echo",
            base_url=None,
            api_key=None,
            timeout=None,
            dry_run=False,
            max_iterations=5,
            max_retries=1,
            max_context_tokens=0,
            session_load=None,
            session_save=None,
            onboard_name="mcp-figma",
            onboard_command="npx",
            onboard_arg=["-y", "@modelcontextprotocol/server-figma"],
            onboard_transport="stdio",
            onboard_trust_level="community",
        )
    )

    captured = capsys.readouterr()
    assert exit_code == 0
    assert '"status": "onboarded"' in captured.out


def test_run_once_requires_prompt_in_chat_mode(capsys) -> None:
    exit_code = _CLI_MODULE.run_once(
        argparse.Namespace(
            mode="chat",
            socket_path="/tmp/agentd.sock",
            agent_id="agent-for-task28",
            prompt=None,
            model=None,
            tool="builtin.lite.echo",
            base_url=None,
            api_key=None,
            timeout=None,
            dry_run=False,
            max_iterations=5,
            max_retries=1,
            max_context_tokens=0,
            session_load=None,
            session_save=None,
            onboard_name=None,
            onboard_command=None,
            onboard_arg=None,
            onboard_transport="stdio",
            onboard_trust_level="community",
        )
    )

    captured = capsys.readouterr()
    assert exit_code == 1
    assert '"error": "missing_prompt"' in captured.out
