from __future__ import annotations

import json
import os
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any, TextIO

SERVER_NAME = "agentd-mcp-fs"
SERVER_VERSION = "0.1.0"
PROTOCOL_VERSION = "2025-03-26"

DEFAULT_MAX_MATCHES = 50
DEFAULT_MAX_TREE_DEPTH = 3
DEFAULT_MAX_TREE_ENTRIES = 200
DEFAULT_MAX_READ_CHARS = 200_000
DEFAULT_MAX_FILE_BYTES = 2_000_000


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


def read_file(arguments: dict[str, Any]) -> dict[str, Any]:
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


def list_directory(arguments: dict[str, Any]) -> dict[str, Any]:
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


def search_files(arguments: dict[str, Any]) -> dict[str, Any]:
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


def patch_file(arguments: dict[str, Any]) -> dict[str, Any]:
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


def tree(arguments: dict[str, Any]) -> dict[str, Any]:
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


TOOL_DECLARATIONS: list[dict[str, Any]] = [
    {
        "name": "read_file",
        "description": "Read text content from a file.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "encoding": {"type": "string", "default": "utf-8"},
                "max_chars": {"type": "integer", "minimum": 1},
            },
            "required": ["path"],
        },
    },
    {
        "name": "list_directory",
        "description": "List entries in a directory.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "default": "."},
                "include_hidden": {"type": "boolean", "default": True},
            },
        },
    },
    {
        "name": "search_files",
        "description": "Search text across files recursively.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "default": "."},
                "pattern": {"type": "string"},
                "use_regex": {"type": "boolean", "default": False},
                "case_sensitive": {"type": "boolean", "default": False},
                "max_matches": {"type": "integer", "minimum": 1},
                "max_file_bytes": {"type": "integer", "minimum": 1},
            },
            "required": ["pattern"],
        },
    },
    {
        "name": "patch_file",
        "description": "Patch a file using one or more search/replace edits.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string"},
                "search": {"type": "string"},
                "replace": {"type": "string"},
                "replace_all": {"type": "boolean", "default": False},
                "edits": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "search": {"type": "string"},
                            "replace": {"type": "string"},
                            "replace_all": {"type": "boolean", "default": False},
                        },
                        "required": ["search", "replace"],
                    },
                },
            },
            "required": ["path"],
        },
    },
    {
        "name": "tree",
        "description": "Return a structured directory tree.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": {"type": "string", "default": "."},
                "max_depth": {"type": "integer", "minimum": 1},
                "max_entries": {"type": "integer", "minimum": 1},
            },
        },
    },
]


TOOL_HANDLERS: dict[str, Any] = {
    "read_file": read_file,
    "list_directory": list_directory,
    "search_files": search_files,
    "patch_file": patch_file,
    "tree": tree,
}


def list_tools() -> list[dict[str, Any]]:
    return [dict(tool) for tool in TOOL_DECLARATIONS]


def call_tool(tool_name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    handler = TOOL_HANDLERS.get(tool_name)
    if handler is None:
        raise ToolError(-32601, f"unknown tool: {tool_name}")
    if not isinstance(arguments, dict):
        raise ToolError(-32602, "invalid params: `arguments` must be an object")
    return handler(arguments)


def build_initialize_result() -> dict[str, Any]:
    return {
        "protocolVersion": PROTOCOL_VERSION,
        "serverInfo": {
            "name": SERVER_NAME,
            "version": SERVER_VERSION,
        },
        "capabilities": {
            "tools": {
                "listChanged": False,
            }
        },
        "tools": list_tools(),
    }


def _success_response(request_id: Any, result: dict[str, Any]) -> dict[str, Any]:
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "result": result,
    }


def _error_response(
    request_id: Any,
    code: int,
    message: str,
    details: dict[str, Any] | None = None,
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "code": code,
        "message": message,
    }
    if details is not None:
        payload["data"] = details
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": payload,
    }


def handle_request(request: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(request, dict):
        return _error_response(None, -32600, "invalid request")

    request_id = request.get("id")
    method = request.get("method")
    params = request.get("params", {})

    try:
        if method == "initialize":
            return _success_response(request_id, build_initialize_result())

        if method == "tools/list":
            return _success_response(request_id, {"tools": list_tools()})

        if method == "tools/call":
            if not isinstance(params, dict):
                raise ToolError(-32602, "invalid params: object required")

            tool_name = _require_string(params.get("name"), "params.name")
            arguments = params.get("arguments", {})
            if arguments is None:
                arguments = {}
            if not isinstance(arguments, dict):
                raise ToolError(-32602, "invalid params: `arguments` must be an object")

            tool_result = call_tool(tool_name, arguments)
            return _success_response(
                request_id,
                {
                    "content": [
                        {
                            "type": "text",
                            "text": json.dumps(tool_result, ensure_ascii=False),
                        }
                    ],
                    "structuredContent": tool_result,
                    "isError": False,
                },
            )

        if method == "ping":
            return _success_response(request_id, {"status": "ok"})

        if method == "shutdown":
            return _success_response(request_id, {"status": "bye"})

        return _error_response(request_id, -32601, f"method not found: {method}")
    except ToolError as err:
        return _error_response(request_id, err.code, err.message, err.details)
    except Exception as err:
        return _error_response(request_id, -32603, f"internal error: {err}")


def run_stdio_server(
    input_stream: TextIO | None = None, output_stream: TextIO | None = None
) -> int:
    source = input_stream if input_stream is not None else sys.stdin
    sink = output_stream if output_stream is not None else sys.stdout

    for raw_line in source:
        line = raw_line.strip()
        if not line:
            continue

        request: dict[str, Any] | None
        try:
            parsed = json.loads(line)
            request = parsed if isinstance(parsed, dict) else None
        except json.JSONDecodeError as err:
            response = _error_response(None, -32700, f"parse error: {err.msg}")
            sink.write(json.dumps(response, ensure_ascii=False) + "\n")
            sink.flush()
            continue

        if request is None:
            response = _error_response(None, -32600, "invalid request")
        else:
            response = handle_request(request)

        sink.write(json.dumps(response, ensure_ascii=False) + "\n")
        sink.flush()

        if request is not None and request.get("method") == "shutdown":
            break

    return 0


def main() -> int:
    return run_stdio_server()


if __name__ == "__main__":
    raise SystemExit(main())
