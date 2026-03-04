import io
import json
import sys
from pathlib import Path
from importlib.util import module_from_spec, spec_from_file_location
from typing import Any, Callable, TextIO, cast

_SERVER_PATH = (
    Path(__file__).resolve().parents[1] / "src" / "agentd_mcp_shell" / "server.py"
)
_SERVER_SPEC = spec_from_file_location("agentd_mcp_shell.server", _SERVER_PATH)
assert _SERVER_SPEC is not None and _SERVER_SPEC.loader is not None
_SERVER_MODULE = module_from_spec(_SERVER_SPEC)
sys.modules[_SERVER_SPEC.name] = _SERVER_MODULE
_SERVER_SPEC.loader.exec_module(_SERVER_MODULE)

build_initialize_result = cast(
    Callable[[], dict[str, Any]], _SERVER_MODULE.build_initialize_result
)
handle_request = cast(
    Callable[[dict[str, Any]], dict[str, Any]], _SERVER_MODULE.handle_request
)
run_stdio_server = cast(
    Callable[[TextIO | None, TextIO | None], int], _SERVER_MODULE.run_stdio_server
)


def _call_tool(tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    response = handle_request(
        {
            "jsonrpc": "2.0",
            "id": "test-call",
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": arguments,
            },
        }
    )
    assert "error" not in response, response

    result = response.get("result")
    assert isinstance(result, dict)
    structured = result.get("structuredContent")
    assert isinstance(structured, dict)
    return structured


def test_initialize_exposes_required_shell_tools() -> None:
    init_result = build_initialize_result()
    tools = init_result.get("tools")
    assert isinstance(tools, list)

    tool_names = {
        tool["name"]
        for tool in tools
        if isinstance(tool, dict) and isinstance(tool.get("name"), str)
    }
    assert {"execute_with_timeout", "get_output"} <= tool_names


def test_execute_timeout() -> None:
    payload = _call_tool(
        "execute_with_timeout",
        {
            "command": [sys.executable, "-c", "import time; time.sleep(1)"],
            "timeout_seconds": 0.1,
        },
    )

    assert payload["timed_out"] is True
    assert payload["return_code"] == -1
    assert isinstance(payload["execution_id"], str)
    assert payload["execution_id"]


def test_execute_and_get_output_roundtrip() -> None:
    execute_payload = _call_tool(
        "execute_with_timeout",
        {
            "command": [sys.executable, "-c", "print('shell-ok')"],
            "timeout_seconds": 3,
        },
    )
    assert execute_payload["timed_out"] is False
    execution_id = execute_payload["execution_id"]
    assert isinstance(execution_id, str)

    output_payload = _call_tool("get_output", {"execution_id": execution_id})
    assert "shell-ok" in output_payload["stdout"]
    assert output_payload["timed_out"] is False


def test_initialize_discoverable_via_stdio() -> None:
    request_stream = io.StringIO(
        "\n".join(
            [
                json.dumps(
                    {
                        "jsonrpc": "2.0",
                        "id": 1,
                        "method": "initialize",
                        "params": {},
                    }
                ),
                json.dumps(
                    {
                        "jsonrpc": "2.0",
                        "id": 2,
                        "method": "shutdown",
                        "params": {},
                    }
                ),
            ]
        )
        + "\n"
    )
    response_stream = io.StringIO()

    exit_code = run_stdio_server(request_stream, response_stream)
    assert exit_code == 0

    responses = [
        json.loads(line)
        for line in response_stream.getvalue().splitlines()
        if line.strip()
    ]
    assert len(responses) == 2
    init_result = responses[0]["result"]
    tools = init_result["tools"]
    assert isinstance(tools, list)
    names = {
        tool["name"]
        for tool in tools
        if isinstance(tool, dict) and isinstance(tool.get("name"), str)
    }
    assert "execute_with_timeout" in names
    assert "get_output" in names
