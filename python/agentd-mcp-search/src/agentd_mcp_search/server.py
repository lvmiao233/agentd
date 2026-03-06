from __future__ import annotations

import json
import re
import subprocess
from importlib import import_module
from pathlib import Path
from typing import Any

FastMCP = import_module("mcp.server.fastmcp").FastMCP

SERVER_NAME = "agentd-mcp-search"

mcp = FastMCP(SERVER_NAME)

DEFAULT_MAX_FILE_BYTES = 1_000_000
DEFAULT_CONTEXT_LINES = 2

CODE_EXTENSIONS = {
    ".py",
    ".rs",
    ".ts",
    ".tsx",
    ".js",
    ".jsx",
    ".go",
    ".java",
    ".c",
    ".cc",
    ".cpp",
    ".h",
    ".hpp",
}

SKIP_DIRECTORIES = {
    ".git",
    ".hg",
    ".svn",
    ".venv",
    "__pycache__",
    "node_modules",
    "target",
    "dist",
    "build",
    "logs",
}

SYMBOL_PATTERNS: tuple[tuple[str, re.Pattern[str]], ...] = (
    ("python.class", re.compile(r"^\s*class\s+(?P<name>[A-Za-z_][\w]*)\b")),
    (
        "python.function",
        re.compile(r"^\s*(?:async\s+)?def\s+(?P<name>[A-Za-z_][\w]*)\b"),
    ),
    (
        "rust.struct",
        re.compile(
            r"^\s*pub\s+struct\s+(?P<name>[A-Za-z_][\w]*)\b|^\s*struct\s+(?P<name2>[A-Za-z_][\w]*)\b"
        ),
    ),
    (
        "rust.enum",
        re.compile(
            r"^\s*pub\s+enum\s+(?P<name>[A-Za-z_][\w]*)\b|^\s*enum\s+(?P<name2>[A-Za-z_][\w]*)\b"
        ),
    ),
    (
        "rust.trait",
        re.compile(
            r"^\s*pub\s+trait\s+(?P<name>[A-Za-z_][\w]*)\b|^\s*trait\s+(?P<name2>[A-Za-z_][\w]*)\b"
        ),
    ),
    (
        "rust.function",
        re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+(?P<name>[A-Za-z_][\w]*)\b"),
    ),
    (
        "typescript.function",
        re.compile(
            r"^\s*(?:export\s+)?(?:async\s+)?function\s+(?P<name>[A-Za-z_$][\w$]*)\b"
        ),
    ),
    (
        "typescript.class",
        re.compile(r"^\s*(?:export\s+)?class\s+(?P<name>[A-Za-z_$][\w$]*)\b"),
    ),
    (
        "typescript.interface",
        re.compile(r"^\s*(?:export\s+)?interface\s+(?P<name>[A-Za-z_$][\w$]*)\b"),
    ),
    (
        "typescript.type",
        re.compile(r"^\s*(?:export\s+)?type\s+(?P<name>[A-Za-z_$][\w$]*)\b"),
    ),
    (
        "typescript.variable",
        re.compile(
            r"^\s*(?:export\s+)?(?:const|let|var)\s+(?P<name>[A-Za-z_$][\w$]*)\s*="
        ),
    ),
    (
        "go.type",
        re.compile(r"^\s*type\s+(?P<name>[A-Za-z_][\w]*)\b"),
    ),
    (
        "go.function",
        re.compile(r"^\s*func\s+(?:\([^)]*\)\s*)?(?P<name>[A-Za-z_][\w]*)\b"),
    ),
    (
        "c.function",
        re.compile(
            r"^\s*[A-Za-z_][\w\s\*]*\s+(?P<name>[A-Za-z_][\w]*)\s*\([^;]*\)\s*\{?$"
        ),
    ),
)


def _ok(data: dict[str, Any]) -> dict[str, Any]:
    return {"ok": True, "data": data}


def _error(
    code: str, message: str, details: dict[str, Any] | None = None
) -> dict[str, Any]:
    return {
        "ok": False,
        "error": {
            "code": code,
            "message": message,
            "details": details or {},
        },
    }


def _parse_rg_lines(lines: list[str]) -> list[dict[str, Any]]:
    matches: list[dict[str, Any]] = []
    for raw in lines:
        if not raw:
            continue
        parts = raw.split(":", 2)
        if len(parts) != 3:
            continue
        file_path, line, text = parts
        try:
            line_no = int(line)
        except ValueError:
            continue
        matches.append({"file": file_path, "line": line_no, "text": text})
    return matches


def _validate_root(root: str) -> Path:
    candidate = Path(root)
    if not candidate.exists():
        raise FileNotFoundError(f"search root does not exist: {root}")
    if not candidate.is_dir():
        raise NotADirectoryError(f"search root is not a directory: {root}")
    return candidate


def _normalize_text(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", " ", value.lower()).strip()


def _tokenize_query(query: str) -> list[str]:
    seen: set[str] = set()
    tokens: list[str] = []
    for raw in re.findall(r"[A-Za-z0-9_\-]+", query.lower()):
        token = raw.strip("_-")
        if len(token) < 2 or token in seen:
            continue
        seen.add(token)
        tokens.append(token)
    return tokens


def _iter_code_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for candidate in root.rglob("*"):
        if not candidate.is_file():
            continue
        if candidate.suffix.lower() not in CODE_EXTENSIONS:
            continue
        if any(part in SKIP_DIRECTORIES for part in candidate.parts):
            continue
        try:
            if candidate.stat().st_size > DEFAULT_MAX_FILE_BYTES:
                continue
        except OSError:
            continue
        files.append(candidate)
    return files


def _extract_symbol_name(match: re.Match[str]) -> str | None:
    for group_name in ("name", "name2"):
        value = match.groupdict().get(group_name)
        if value:
            return value
    return None


def _candidate_context(lines: list[str], line_index: int) -> str:
    start = max(0, line_index - DEFAULT_CONTEXT_LINES)
    end = min(len(lines), line_index + DEFAULT_CONTEXT_LINES + 1)
    context_lines = [line.strip() for line in lines[start:end] if line.strip()]
    return " ".join(context_lines)


def _score_candidate(
    *,
    query: str,
    query_tokens: list[str],
    path: Path,
    symbol_name: str,
    symbol_kind: str,
    line_text: str,
    context: str,
) -> tuple[int, int]:
    if not query_tokens:
        return (0, 0)

    normalized_query = _normalize_text(query)
    path_text = _normalize_text(str(path))
    name_text = _normalize_text(symbol_name)
    kind_text = _normalize_text(symbol_kind)
    line_normalized = _normalize_text(line_text)
    context_normalized = _normalize_text(context)
    full_text = " ".join(
        part
        for part in (
            path_text,
            name_text,
            kind_text,
            line_normalized,
            context_normalized,
        )
        if part
    )

    matched_terms = 0
    score = 0
    for token in query_tokens:
        if token in name_text:
            matched_terms += 1
            score += 10
            continue
        if token in line_normalized:
            matched_terms += 1
            score += 6
            continue
        if token in context_normalized:
            matched_terms += 1
            score += 4
            continue
        if token in path_text:
            matched_terms += 1
            score += 3

    if normalized_query and normalized_query in full_text:
        score += 8

    if matched_terms == 0:
        return (0, 0)

    if len(query_tokens) > 1 and matched_terms == 1:
        return (0, 1)

    if matched_terms == len(query_tokens):
        score += 5

    path_parts = {part.lower() for part in path.parts}
    if "src" in path_parts:
        score += 3
    if "tests" in path_parts or path.name.startswith("test_"):
        score -= 6

    return (score, matched_terms)


def _semantic_candidates(root: Path, query: str) -> list[dict[str, Any]]:
    query_tokens = _tokenize_query(query)
    candidates: list[dict[str, Any]] = []
    for path in _iter_code_files(root):
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        lines = text.splitlines()
        for line_index, line_text in enumerate(lines):
            stripped = line_text.strip()
            if not stripped:
                continue
            for raw_kind, pattern in SYMBOL_PATTERNS:
                match = pattern.match(line_text)
                if match is None:
                    continue
                symbol_name = _extract_symbol_name(match)
                if not symbol_name:
                    continue
                context = _candidate_context(lines, line_index)
                score, matched_terms = _score_candidate(
                    query=query,
                    query_tokens=query_tokens,
                    path=path,
                    symbol_name=symbol_name,
                    symbol_kind=raw_kind,
                    line_text=line_text,
                    context=context,
                )
                if score <= 0:
                    continue
                relative_path = (
                    path.relative_to(root) if path != root else Path(path.name)
                )
                kind = raw_kind.split(".", 1)[-1]
                candidates.append(
                    {
                        "file": str(path),
                        "relative_path": str(relative_path),
                        "line": line_index + 1,
                        "symbol": symbol_name,
                        "kind": kind,
                        "score": score,
                        "matched_terms": matched_terms,
                        "signature": stripped,
                        "context": context,
                    }
                )
                break
    candidates.sort(
        key=lambda item: (
            -int(item["score"]),
            -int(item["matched_terms"]),
            len(str(item["relative_path"])),
            str(item["relative_path"]),
            int(item["line"]),
        )
    )
    return candidates


def _ripgrep_impl(
    pattern: str, root: str = ".", max_results: int = 200
) -> dict[str, Any]:
    try:
        if not pattern:
            return _error(
                "INVALID_ARGUMENT",
                "pattern must not be empty",
                {"field": "pattern"},
            )
        search_root = _validate_root(root)
        completed = subprocess.run(
            [
                "rg",
                "--line-number",
                "--with-filename",
                "--color",
                "never",
                "--no-heading",
                pattern,
                str(search_root),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if completed.returncode not in (0, 1):
            return _error(
                "SEARCH_EXECUTION_FAILED",
                "ripgrep command failed",
                {
                    "returncode": completed.returncode,
                    "stderr": completed.stderr.strip(),
                },
            )
        lines = [line for line in completed.stdout.splitlines() if line.strip()]
        matches = _parse_rg_lines(lines)[: max(0, max_results)]
        return _ok(
            {
                "tool": "ripgrep",
                "query": pattern,
                "root": str(search_root),
                "match_count": len(matches),
                "matches": matches,
            }
        )
    except Exception as exc:
        return _error(
            "SEARCH_FAILED",
            str(exc),
            {"pattern": pattern, "root": root},
        )


def _find_definition_impl(
    symbol: str, root: str = ".", max_results: int = 50
) -> dict[str, Any]:
    try:
        if not symbol:
            return _error(
                "INVALID_ARGUMENT",
                "symbol must not be empty",
                {"field": "symbol"},
            )
        search_root = _validate_root(root)
        escaped = re.escape(symbol)
        pattern = (
            rf"^\s*(def|class)\s+{escaped}\b"
            rf"|^\s*(pub\s+)?(async\s+)?fn\s+{escaped}\b"
            rf"|^\s*(export\s+)?(async\s+)?function\s+{escaped}\b"
            rf"|^\s*(const|let|var)\s+{escaped}\s*=\s*\("
        )
        completed = subprocess.run(
            [
                "rg",
                "--line-number",
                "--with-filename",
                "--color",
                "never",
                "--no-heading",
                "--glob",
                "*.{py,rs,ts,tsx,js,jsx,go,java,c,cc,cpp,h,hpp}",
                pattern,
                str(search_root),
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if completed.returncode not in (0, 1):
            return _error(
                "SEARCH_EXECUTION_FAILED",
                "definition lookup failed",
                {
                    "returncode": completed.returncode,
                    "stderr": completed.stderr.strip(),
                },
            )
        lines = [line for line in completed.stdout.splitlines() if line.strip()]
        matches = _parse_rg_lines(lines)[: max(0, max_results)]
        return _ok(
            {
                "tool": "find_definition",
                "query": symbol,
                "root": str(search_root),
                "match_count": len(matches),
                "matches": matches,
            }
        )
    except Exception as exc:
        return _error(
            "SEARCH_FAILED",
            str(exc),
            {"symbol": symbol, "root": root},
        )


def _semantic_search_impl(
    query: str, root: str = ".", max_results: int = 20
) -> dict[str, Any]:
    try:
        search_root = _validate_root(root)
        if not query:
            return _error(
                "INVALID_ARGUMENT",
                "query must not be empty",
                {"field": "query"},
            )
        query_tokens = _tokenize_query(query)
        candidates = _semantic_candidates(search_root, query)
        limited_results = candidates[: max(0, max_results)]
        return _ok(
            {
                "tool": "semantic_search",
                "query": query,
                "query_terms": query_tokens,
                "root": str(search_root),
                "max_results": max_results,
                "match_count": len(limited_results),
                "matches": limited_results,
                "strategy": "symbol-ranked-token-search",
            }
        )
    except Exception as exc:
        return _error(
            "SEARCH_FAILED",
            str(exc),
            {"query": query, "root": root},
        )


@mcp.tool()
def ripgrep(pattern: str, root: str = ".", max_results: int = 200) -> str:
    """Search files using ripgrep and return structured match records."""
    return json.dumps(
        _ripgrep_impl(pattern=pattern, root=root, max_results=max_results),
        ensure_ascii=False,
    )


@mcp.tool()
def find_definition(symbol: str, root: str = ".", max_results: int = 50) -> str:
    """Find likely code definitions for a symbol using ripgrep patterns."""
    return json.dumps(
        _find_definition_impl(symbol=symbol, root=root, max_results=max_results),
        ensure_ascii=False,
    )


@mcp.tool()
def semantic_search(query: str, root: str = ".", max_results: int = 20) -> str:
    return json.dumps(
        _semantic_search_impl(query=query, root=root, max_results=max_results),
        ensure_ascii=False,
    )


def main() -> None:
    mcp.run()


if __name__ == "__main__":
    main()
