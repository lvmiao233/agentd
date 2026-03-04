from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path
from typing import Any


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


def ripgrep(pattern: str, root: str = ".", max_results: int = 200) -> dict[str, Any]:
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


def find_definition(
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


def semantic_search(
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
        return _ok(
            {
                "tool": "semantic_search",
                "query": query,
                "root": str(search_root),
                "max_results": max_results,
                "matches": [],
                "placeholder": True,
                "extensible": True,
            }
        )
    except Exception as exc:
        return _error(
            "SEARCH_FAILED",
            str(exc),
            {"query": query, "root": root},
        )


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="agentd-mcp-search")
    subparsers = parser.add_subparsers(dest="tool", required=True)

    rg_parser = subparsers.add_parser("ripgrep")
    rg_parser.add_argument("pattern")
    rg_parser.add_argument("--root", default=".")
    rg_parser.add_argument("--max-results", type=int, default=200)

    fd_parser = subparsers.add_parser("find-definition")
    fd_parser.add_argument("symbol")
    fd_parser.add_argument("--root", default=".")
    fd_parser.add_argument("--max-results", type=int, default=50)

    semantic_parser = subparsers.add_parser("semantic-search")
    semantic_parser.add_argument("query")
    semantic_parser.add_argument("--root", default=".")
    semantic_parser.add_argument("--max-results", type=int, default=20)

    return parser


def main() -> int:
    parser = _build_parser()
    args = parser.parse_args()
    if args.tool == "ripgrep":
        payload = ripgrep(args.pattern, root=args.root, max_results=args.max_results)
    elif args.tool == "find-definition":
        payload = find_definition(
            args.symbol, root=args.root, max_results=args.max_results
        )
    else:
        payload = semantic_search(
            args.query, root=args.root, max_results=args.max_results
        )
    print(json.dumps(payload, ensure_ascii=False))
    return 0 if payload.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
