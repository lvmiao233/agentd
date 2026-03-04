import io
import json
import sys
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from typing import Any, Callable, TextIO, cast

_SERVER_PATH = (
    Path(__file__).resolve().parents[1] / "src" / "agentd_mcp_fs" / "server.py"
)
_SERVER_SPEC = spec_from_file_location("agentd_mcp_fs.server", _SERVER_PATH)
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


def test_initialize_exposes_required_fs_tools() -> None:
    init_result = build_initialize_result()
    tools = init_result.get("tools")
    assert isinstance(tools, list)

    tool_names = {
        tool["name"]
        for tool in tools
        if isinstance(tool, dict) and isinstance(tool.get("name"), str)
    }
    assert {
        "read_file",
        "list_directory",
        "search_files",
        "patch_file",
        "tree",
    } <= tool_names


def test_read_and_tree(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    file_path = workspace / "notes.txt"
    file_path.write_text("hello\nworld\n", encoding="utf-8")

    read_payload = _call_tool("read_file", {"path": str(file_path)})
    assert read_payload["path"] == str(file_path)
    assert read_payload["content"] == "hello\nworld\n"

    tree_payload = _call_tool("tree", {"path": str(workspace), "max_depth": 3})
    root = tree_payload["tree"]
    assert root["type"] == "directory"
    children = root.get("children", [])
    assert isinstance(children, list)
    assert any(item.get("name") == "notes.txt" for item in children)


def test_search_and_patch_file(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    target = workspace / "target.txt"
    target.write_text("alpha\nbeta\ngamma\n", encoding="utf-8")

    search_payload = _call_tool(
        "search_files",
        {
            "path": str(workspace),
            "pattern": "beta",
            "max_matches": 5,
        },
    )
    matches = search_payload["matches"]
    assert isinstance(matches, list)
    assert len(matches) == 1
    assert matches[0]["line"] == 2

    patch_payload = _call_tool(
        "patch_file",
        {
            "path": str(target),
            "search": "beta",
            "replace": "BETA",
        },
    )
    assert patch_payload["replacements"] == 1
    assert target.read_text(encoding="utf-8") == "alpha\nBETA\ngamma\n"


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
    assert "read_file" in names
    assert "tree" in names
