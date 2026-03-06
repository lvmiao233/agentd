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
    sample_text = """def greet(name: str) -> str:
    return f'hi {name}'

print(greet('world'))
"""
    _ = sample.write_text(sample_text, encoding="utf-8")

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


def test_semantic_search_returns_ranked_symbol_matches(tmp_path: Path) -> None:
    sample = tmp_path / "session.py"
    sample_text = """class AgentSession:
    async def chat(self, user_input: str) -> str:
        return user_input

def compact_context() -> None:
    return None
"""
    _ = sample.write_text(sample_text, encoding="utf-8")

    result = json.loads(semantic_search("agent session chat", root=str(tmp_path)))

    assert result["ok"] is True
    data = result["data"]
    assert data["tool"] == "semantic_search"
    assert data["strategy"] == "symbol-ranked-token-search"
    assert data["query_terms"] == ["agent", "session", "chat"]
    assert data["match_count"] >= 1
    first_match = data["matches"][0]
    assert first_match["file"].endswith("session.py")
    assert first_match["symbol"] == "AgentSession"
    assert first_match["kind"] == "class"
    assert first_match["score"] > 0


def test_semantic_search_matches_rust_symbols(tmp_path: Path) -> None:
    sample = tmp_path / "firecracker.rs"
    sample_text = """pub struct FirecrackerExecutor {
    ready: bool,
}

impl FirecrackerExecutor {
    pub async fn launch_agent(&self) {}
}
"""
    _ = sample.write_text(sample_text, encoding="utf-8")

    result = json.loads(semantic_search("firecracker executor", root=str(tmp_path)))

    assert result["ok"] is True
    matches = result["data"]["matches"]
    assert matches
    assert matches[0]["symbol"] == "FirecrackerExecutor"
    assert matches[0]["kind"] == "struct"


def test_semantic_search_prefers_source_symbols_over_tests(tmp_path: Path) -> None:
    src_dir = tmp_path / "src"
    tests_dir = tmp_path / "tests"
    src_dir.mkdir()
    tests_dir.mkdir()

    src_text = """class AgentSession:
    async def chat(self, user_input: str) -> str:
        return user_input
"""
    test_text = """class AgentSession:
    async def chat(self, user_input: str) -> str:
        return user_input
"""
    _ = (src_dir / "session.py").write_text(src_text, encoding="utf-8")
    _ = (tests_dir / "test_session.py").write_text(test_text, encoding="utf-8")

    result = json.loads(semantic_search("agent session chat", root=str(tmp_path)))

    assert result["ok"] is True
    matches = result["data"]["matches"]
    assert matches
    assert matches[0]["relative_path"] == "src/session.py"


def test_ripgrep_returns_structured_error_for_missing_root(tmp_path: Path) -> None:
    missing = tmp_path / "missing"
    result = json.loads(ripgrep("hello", root=str(missing)))

    assert result["ok"] is False
    error = result["error"]
    assert set(error.keys()) == {"code", "message", "details"}
    assert error["code"] == "SEARCH_FAILED"
    assert "does not exist" in error["message"]
