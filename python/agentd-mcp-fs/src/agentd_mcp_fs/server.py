from __future__ import annotations

import json
import os
import re
from importlib import import_module
from dataclasses import dataclass
from pathlib import Path
from typing import Any

FastMCP = import_module("mcp.server.fastmcp").FastMCP

SERVER_NAME = "agentd-mcp-fs"
SERVER_VERSION = "0.1.0"

DEFAULT_MAX_MATCHES = 50
DEFAULT_MAX_TREE_DEPTH = 3
DEFAULT_MAX_TREE_ENTRIES = 200
DEFAULT_MAX_READ_CHARS = 200_000
DEFAULT_MAX_FILE_BYTES = 2_000_000

mcp = FastMCP(SERVER_NAME)


@dataclass(slots=True)
class ToolError(Exception):
    code: int
    message: str
    details: dict[str, Any] | None = None


def _require_string(value: Any, field_name: str, *, allow_empty: bool = False) -> str:
    if not isinstance(value, str):
        raise ToolError(-32602, f"invalid params: `{field_name}` must be a string")
    if not allow_empty and not value.strip():
        raise ToolError(
            -32602, f"invalid params: `{field_name}` must be a non-empty string"
        )
    return value


def _as_positive_int(value: Any, field_name: str, default: int) -> int:
    if value is None:
        return default
    if not isinstance(value, int):
        raise ToolError(-32602, f"invalid params: `{field_name}` must be an integer")
    if value <= 0:
        raise ToolError(
            -32602, f"invalid params: `{field_name}` must be greater than 0"
        )
    return value


def _resolve_path(raw_path: str) -> Path:
    candidate = Path(raw_path).expanduser()
    if not candidate.is_absolute():
        candidate = Path.cwd() / candidate
    return candidate.resolve()


def _truncate_text(text: str, max_chars: int) -> tuple[str, bool]:
    if len(text) <= max_chars:
        return text, False
    return text[:max_chars], True


def _sorted_entries(path: Path) -> list[Path]:
    try:
        entries = list(path.iterdir())
    except OSError as err:
        raise ToolError(-32010, f"failed to list directory `{path}`: {err}") from err
    return sorted(
        entries, key=lambda item: (not item.is_dir(), item.name.lower(), item.name)
    )


def _read_file_impl(arguments: dict[str, Any]) -> dict[str, Any]:
    path_value = _require_string(arguments.get("path"), "path")
    encoding = _require_string(arguments.get("encoding", "utf-8"), "encoding")
    max_chars = _as_positive_int(
        arguments.get("max_chars"), "max_chars", DEFAULT_MAX_READ_CHARS
    )

    path = _resolve_path(path_value)
    if not path.exists():
        raise ToolError(-32004, f"file not found: {path}")
    if not path.is_file():
        raise ToolError(-32005, f"path is not a file: {path}")

    try:
        raw = path.read_bytes()
    except OSError as err:
        raise ToolError(-32010, f"failed to read `{path}`: {err}") from err

    try:
        content = raw.decode(encoding)
    except LookupError as err:
        raise ToolError(-32602, f"invalid encoding `{encoding}`") from err
    except UnicodeDecodeError:
        content = raw.decode(encoding, errors="replace")

    limited_content, truncated = _truncate_text(content, max_chars)
    return {
        "path": str(path),
        "encoding": encoding,
        "content": limited_content,
        "truncated": truncated,
        "size_bytes": len(raw),
    }


def _list_directory_impl(arguments: dict[str, Any]) -> dict[str, Any]:
    path_value = _require_string(arguments.get("path", "."), "path")
    include_hidden = bool(arguments.get("include_hidden", True))

    path = _resolve_path(path_value)
    if not path.exists():
        raise ToolError(-32004, f"directory not found: {path}")
    if not path.is_dir():
        raise ToolError(-32005, f"path is not a directory: {path}")

    entries: list[dict[str, Any]] = []
    for entry in _sorted_entries(path):
        if not include_hidden and entry.name.startswith("."):
            continue

        entry_type = "directory" if entry.is_dir() else "file"
        size_bytes: int | None = None
        if entry.is_file():
            try:
                size_bytes = entry.stat().st_size
            except OSError:
                size_bytes = None

        entries.append(
            {
                "name": entry.name,
                "path": str(entry),
                "type": entry_type,
                "size_bytes": size_bytes,
            }
        )

    return {
        "path": str(path),
        "entries": entries,
        "count": len(entries),
    }


def _search_files_impl(arguments: dict[str, Any]) -> dict[str, Any]:
    base_path_value = _require_string(arguments.get("path", "."), "path")
    pattern_text = _require_string(arguments.get("pattern"), "pattern")
    use_regex = bool(arguments.get("use_regex", False))
    case_sensitive = bool(arguments.get("case_sensitive", False))
    max_matches = _as_positive_int(
        arguments.get("max_matches"), "max_matches", DEFAULT_MAX_MATCHES
    )
    max_file_bytes = _as_positive_int(
        arguments.get("max_file_bytes"), "max_file_bytes", DEFAULT_MAX_FILE_BYTES
    )

    base_path = _resolve_path(base_path_value)
    if not base_path.exists():
        raise ToolError(-32004, f"directory not found: {base_path}")
    if not base_path.is_dir():
        raise ToolError(-32005, f"path is not a directory: {base_path}")

    regex_flags = 0 if case_sensitive else re.IGNORECASE
    try:
        matcher = (
            re.compile(pattern_text, regex_flags)
            if use_regex
            else re.compile(re.escape(pattern_text), regex_flags)
        )
    except re.error as err:
        raise ToolError(-32602, f"invalid regex pattern: {err}") from err

    matches: list[dict[str, Any]] = []
    scanned_files = 0

    for root, dirnames, filenames in os.walk(base_path):
        dirnames.sort(key=str.lower)
        filenames.sort(key=str.lower)
        root_path = Path(root)

        for filename in filenames:
            file_path = root_path / filename

            try:
                stat = file_path.stat()
            except OSError:
                continue

            if stat.st_size > max_file_bytes:
                continue

            try:
                text = file_path.read_text(encoding="utf-8", errors="replace")
            except OSError:
                continue

            scanned_files += 1
            for line_no, line in enumerate(text.splitlines(), start=1):
                if matcher.search(line) is None:
                    continue

                matches.append(
                    {
                        "path": str(file_path),
                        "line": line_no,
                        "snippet": line,
                    }
                )
                if len(matches) >= max_matches:
                    return {
                        "path": str(base_path),
                        "pattern": pattern_text,
                        "matches": matches,
                        "scanned_files": scanned_files,
                        "truncated": True,
                    }

    return {
        "path": str(base_path),
        "pattern": pattern_text,
        "matches": matches,
        "scanned_files": scanned_files,
        "truncated": False,
    }


def _apply_single_edit(
    text: str, *, search: str, replace: str, replace_all: bool
) -> tuple[str, int]:
    if search not in text:
        raise ToolError(-32011, "patch target not found")

    if replace_all:
        replacements = text.count(search)
        return text.replace(search, replace), replacements

    return text.replace(search, replace, 1), 1


def _patch_file_impl(arguments: dict[str, Any]) -> dict[str, Any]:
    path_value = _require_string(arguments.get("path"), "path")
    encoding = _require_string(arguments.get("encoding", "utf-8"), "encoding")

    edits_raw = arguments.get("edits")
    edits: list[dict[str, Any]] = []
    if isinstance(edits_raw, list) and edits_raw:
        for idx, item in enumerate(edits_raw):
            if not isinstance(item, dict):
                raise ToolError(
                    -32602,
                    f"invalid params: `edits[{idx}]` must be an object",
                )
            edits.append(item)
    else:
        edits = [arguments]

    path = _resolve_path(path_value)
    if not path.exists():
        raise ToolError(-32004, f"file not found: {path}")
    if not path.is_file():
        raise ToolError(-32005, f"path is not a file: {path}")

    try:
        text = path.read_text(encoding=encoding)
    except LookupError as err:
        raise ToolError(-32602, f"invalid encoding `{encoding}`") from err
    except OSError as err:
        raise ToolError(-32010, f"failed to read `{path}`: {err}") from err

    updated = text
    total_replacements = 0
    for idx, edit in enumerate(edits):
        search = _require_string(edit.get("search"), f"edits[{idx}].search")
        replace = _require_string(
            edit.get("replace", ""), f"edits[{idx}].replace", allow_empty=True
        )
        replace_all = bool(edit.get("replace_all", False))
        updated, replacement_count = _apply_single_edit(
            updated,
            search=search,
            replace=replace,
            replace_all=replace_all,
        )
        total_replacements += replacement_count

    try:
        path.write_text(updated, encoding=encoding)
    except OSError as err:
        raise ToolError(-32010, f"failed to write `{path}`: {err}") from err

    return {
        "path": str(path),
        "replacements": total_replacements,
        "bytes_written": len(updated.encode(encoding, errors="replace")),
    }


def _build_tree_node(
    path: Path, *, depth: int, max_depth: int, max_entries: int
) -> dict[str, Any]:
    if path.is_symlink():
        return {
            "name": path.name,
            "path": str(path),
            "type": "symlink",
        }

    if path.is_file():
        size_bytes: int | None
        try:
            size_bytes = path.stat().st_size
        except OSError:
            size_bytes = None
        return {
            "name": path.name,
            "path": str(path),
            "type": "file",
            "size_bytes": size_bytes,
        }

    node: dict[str, Any] = {
        "name": path.name or str(path),
        "path": str(path),
        "type": "directory",
    }

    if depth >= max_depth:
        return node

    entries = _sorted_entries(path)
    truncated = len(entries) > max_entries
    if truncated:
        entries = entries[:max_entries]

    children = [
        _build_tree_node(
            child,
            depth=depth + 1,
            max_depth=max_depth,
            max_entries=max_entries,
        )
        for child in entries
    ]
    node["children"] = children
    if truncated:
        node["truncated"] = True

    return node


def _tree_impl(arguments: dict[str, Any]) -> dict[str, Any]:
    path_value = _require_string(arguments.get("path", "."), "path")
    max_depth = _as_positive_int(
        arguments.get("max_depth"), "max_depth", DEFAULT_MAX_TREE_DEPTH
    )
    max_entries = _as_positive_int(
        arguments.get("max_entries"), "max_entries", DEFAULT_MAX_TREE_ENTRIES
    )

    path = _resolve_path(path_value)
    if not path.exists():
        raise ToolError(-32004, f"directory not found: {path}")
    if not path.is_dir():
        raise ToolError(-32005, f"path is not a directory: {path}")

    return {
        "path": str(path),
        "tree": _build_tree_node(
            path, depth=0, max_depth=max_depth, max_entries=max_entries
        ),
        "max_depth": max_depth,
        "max_entries": max_entries,
    }


@mcp.tool()
def read_file(
    path: str, encoding: str = "utf-8", max_chars: int = DEFAULT_MAX_READ_CHARS
) -> str:
    """Read text content from a file."""
    result = _read_file_impl(
        {
            "path": path,
            "encoding": encoding,
            "max_chars": max_chars,
        }
    )
    return json.dumps(result, ensure_ascii=False)


@mcp.tool()
def list_directory(path: str = ".", include_hidden: bool = True) -> str:
    """List entries in a directory."""
    result = _list_directory_impl(
        {
            "path": path,
            "include_hidden": include_hidden,
        }
    )
    return json.dumps(result, ensure_ascii=False)


@mcp.tool()
def search_files(
    pattern: str,
    path: str = ".",
    use_regex: bool = False,
    case_sensitive: bool = False,
    max_matches: int = DEFAULT_MAX_MATCHES,
    max_file_bytes: int = DEFAULT_MAX_FILE_BYTES,
) -> str:
    """Search text across files recursively."""
    result = _search_files_impl(
        {
            "path": path,
            "pattern": pattern,
            "use_regex": use_regex,
            "case_sensitive": case_sensitive,
            "max_matches": max_matches,
            "max_file_bytes": max_file_bytes,
        }
    )
    return json.dumps(result, ensure_ascii=False)


@mcp.tool()
def patch_file(
    path: str,
    search: str | None = None,
    replace: str = "",
    replace_all: bool = False,
    edits: list[dict[str, Any]] | None = None,
    encoding: str = "utf-8",
) -> str:
    """Patch a file using one or more search/replace edits."""
    arguments: dict[str, Any] = {
        "path": path,
        "encoding": encoding,
    }
    if edits is not None:
        arguments["edits"] = edits
    else:
        arguments["search"] = search
        arguments["replace"] = replace
        arguments["replace_all"] = replace_all

    result = _patch_file_impl(arguments)
    return json.dumps(result, ensure_ascii=False)


@mcp.tool()
def tree(
    path: str = ".",
    max_depth: int = DEFAULT_MAX_TREE_DEPTH,
    max_entries: int = DEFAULT_MAX_TREE_ENTRIES,
) -> str:
    """Return a structured directory tree."""
    result = _tree_impl(
        {
            "path": path,
            "max_depth": max_depth,
            "max_entries": max_entries,
        }
    )
    return json.dumps(result, ensure_ascii=False)


def main() -> None:
    mcp.run()


if __name__ == "__main__":
    main()
