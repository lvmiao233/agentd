from __future__ import annotations

import importlib.util
import json
from pathlib import Path

_module_path = (
    Path(__file__).resolve().parents[1] / "src" / "agentd_mcp_search" / "server.py"
)
_spec = importlib.util.spec_from_file_location("agentd_mcp_search_server", _module_path)
assert _spec is not None and _spec.loader is not None
_search = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_search)

find_definition = _search.find_definition
ripgrep = _search.ripgrep
semantic_search = _search.semantic_search


def test_ripgrep_and_find_definition(tmp_path: Path) -> None:
    sample = tmp_path / "sample.py"
    sample.write_text(
        "def greet(name: str) -> str:\n"
        "    return f'hi {name}'\n"
        "\n"
        "print(greet('world'))\n",
        encoding="utf-8",
    )

    rg_result = json.loads(ripgrep("greet", root=str(tmp_path)))
    assert rg_result["ok"] is True
    rg_matches = rg_result["data"]["matches"]
    assert rg_matches
    assert rg_matches[0]["file"].endswith("sample.py")
    assert isinstance(rg_matches[0]["line"], int)

    fd_result = json.loads(find_definition("greet", root=str(tmp_path)))
    assert fd_result["ok"] is True
    fd_matches = fd_result["data"]["matches"]
    assert fd_matches
    assert "def greet" in fd_matches[0]["text"]


def test_semantic_search_placeholder_payload(tmp_path: Path) -> None:
    (tmp_path / "x.py").write_text("x = 1\n", encoding="utf-8")
    result = json.loads(semantic_search("meaning of x", root=str(tmp_path)))

    assert result["ok"] is True
    assert result["data"]["tool"] == "semantic_search"
    assert result["data"]["placeholder"] is True
    assert result["data"]["extensible"] is True
    assert result["data"]["matches"] == []


def test_ripgrep_returns_structured_error_for_missing_root(tmp_path: Path) -> None:
    missing = tmp_path / "missing"
    result = json.loads(ripgrep("hello", root=str(missing)))

    assert result["ok"] is False
    error = result["error"]
    assert set(error.keys()) == {"code", "message", "details"}
    assert error["code"] == "SEARCH_FAILED"
    assert "does not exist" in error["message"]
