import json
import sys
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from typing import Any, Callable, cast

_SERVER_PATH = (
    Path(__file__).resolve().parents[1] / "src" / "agentd_mcp_fs" / "server.py"
)
_SERVER_SPEC = spec_from_file_location("agentd_mcp_fs.server", _SERVER_PATH)
assert _SERVER_SPEC is not None and _SERVER_SPEC.loader is not None
_SERVER_MODULE = module_from_spec(_SERVER_SPEC)
sys.modules[_SERVER_SPEC.name] = _SERVER_MODULE
_SERVER_SPEC.loader.exec_module(_SERVER_MODULE)

list_directory = cast(Callable[..., str], _SERVER_MODULE.list_directory)
patch_file = cast(Callable[..., str], _SERVER_MODULE.patch_file)
read_file = cast(Callable[..., str], _SERVER_MODULE.read_file)
search_files = cast(Callable[..., str], _SERVER_MODULE.search_files)
tree = cast(Callable[..., str], _SERVER_MODULE.tree)


def _decode(payload: str) -> dict[str, Any]:
    decoded = json.loads(payload)
    assert isinstance(decoded, dict)
    return decoded


def test_read_and_tree(tmp_path: Path) -> None:
    workspace = tmp_path / "workspace"
    workspace.mkdir()
    file_path = workspace / "notes.txt"
    file_path.write_text("hello\nworld\n", encoding="utf-8")

    read_payload = _decode(read_file(path=str(file_path)))
    assert read_payload["path"] == str(file_path)
    assert read_payload["content"] == "hello\nworld\n"

    tree_payload = _decode(tree(path=str(workspace), max_depth=3))
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

    search_payload = _decode(
        search_files(
            path=str(workspace),
            pattern="beta",
            max_matches=5,
        )
    )
    matches = search_payload["matches"]
    assert isinstance(matches, list)
    assert len(matches) == 1
    assert matches[0]["line"] == 2

    patch_payload = _decode(
        patch_file(
            path=str(target),
            search="beta",
            replace="BETA",
        )
    )
    assert patch_payload["replacements"] == 1
    assert target.read_text(encoding="utf-8") == "alpha\nBETA\ngamma\n"


def test_list_directory(tmp_path: Path) -> None:
    (tmp_path / "a.txt").write_text("a", encoding="utf-8")
    (tmp_path / ".hidden.txt").write_text("h", encoding="utf-8")

    payload = _decode(list_directory(path=str(tmp_path), include_hidden=False))
    names = {entry["name"] for entry in payload["entries"]}
    assert "a.txt" in names
    assert ".hidden.txt" not in names
