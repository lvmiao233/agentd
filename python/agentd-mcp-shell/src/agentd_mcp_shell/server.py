from __future__ import annotations

import json
import os
import subprocess
import time
import uuid
from importlib import import_module
from dataclasses import dataclass
from pathlib import Path
from typing import Any

FastMCP = import_module("mcp.server.fastmcp").FastMCP

SERVER_NAME = "agentd-mcp-shell"
SERVER_VERSION = "0.1.0"

DEFAULT_TIMEOUT_SECONDS = 30.0
DEFAULT_MAX_OUTPUT_CHARS = 12_000
MAX_STORED_EXECUTIONS = 200

_EXECUTIONS: dict[str, dict[str, Any]] = {}
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


def _as_positive_float(value: Any, field_name: str, default: float) -> float:
    if value is None:
        return default
    if not isinstance(value, (int, float)):
        raise ToolError(-32602, f"invalid params: `{field_name}` must be a number")
    if float(value) <= 0:
        raise ToolError(
            -32602, f"invalid params: `{field_name}` must be greater than 0"
        )
    return float(value)


def _truncate_text(text: str, max_chars: int) -> tuple[str, bool]:
    if len(text) <= max_chars:
        return text, False
    return text[:max_chars], True


def _coerce_subprocess_output(value: str | bytes | None) -> str:
    if value is None:
        return ""
    if isinstance(value, bytes):
        return value.decode("utf-8", errors="replace")
    return value


def _resolve_optional_cwd(arguments: dict[str, Any]) -> Path | None:
    cwd_raw = arguments.get("cwd")
    if cwd_raw is None:
        return None

    cwd_text = _require_string(cwd_raw, "cwd")
    cwd_path = Path(cwd_text).expanduser()
    if not cwd_path.is_absolute():
        cwd_path = Path.cwd() / cwd_path
    cwd_path = cwd_path.resolve()

    if not cwd_path.exists():
        raise ToolError(-32004, f"working directory not found: {cwd_path}")
    if not cwd_path.is_dir():
        raise ToolError(-32005, f"path is not a directory: {cwd_path}")

    return cwd_path


def _resolve_optional_env(arguments: dict[str, Any]) -> dict[str, str] | None:
    env_raw = arguments.get("env")
    if env_raw is None:
        return None

    if not isinstance(env_raw, dict):
        raise ToolError(-32602, "invalid params: `env` must be an object")

    env = os.environ.copy()
    for key, value in env_raw.items():
        if not isinstance(key, str) or not key:
            raise ToolError(
                -32602, "invalid params: env key must be a non-empty string"
            )
        if not isinstance(value, str):
            raise ToolError(
                -32602, f"invalid params: env value for `{key}` must be string"
            )
        env[key] = value
    return env


def _normalize_command(command_raw: Any) -> tuple[str | list[str], bool]:
    if isinstance(command_raw, str):
        command_text = command_raw.strip()
        if not command_text:
            raise ToolError(-32602, "invalid params: `command` cannot be empty")
        return command_text, True

    if isinstance(command_raw, list) and command_raw:
        command: list[str] = []
        for idx, item in enumerate(command_raw):
            if not isinstance(item, str) or not item:
                raise ToolError(
                    -32602,
                    f"invalid params: `command[{idx}]` must be a non-empty string",
                )
            command.append(item)
        return command, False

    raise ToolError(
        -32602,
        "invalid params: `command` must be a non-empty string or array of strings",
    )


def _store_execution(record: dict[str, Any]) -> None:
    execution_id = record.get("execution_id")
    if not isinstance(execution_id, str):
        raise ToolError(-32603, "internal error: invalid execution record")

    _EXECUTIONS[execution_id] = record
    while len(_EXECUTIONS) > MAX_STORED_EXECUTIONS:
        oldest = next(iter(_EXECUTIONS))
        _EXECUTIONS.pop(oldest)


def _render_execution(record: dict[str, Any], max_output_chars: int) -> dict[str, Any]:
    stdout, stdout_truncated = _truncate_text(
        _require_string(record.get("stdout", ""), "stdout", allow_empty=True),
        max_output_chars,
    )
    stderr, stderr_truncated = _truncate_text(
        _require_string(record.get("stderr", ""), "stderr", allow_empty=True),
        max_output_chars,
    )

    return {
        "execution_id": record["execution_id"],
        "command": record["command"],
        "cwd": record["cwd"],
        "return_code": record["return_code"],
        "timed_out": record["timed_out"],
        "duration_ms": record["duration_ms"],
        "stdout": stdout,
        "stderr": stderr,
        "stdout_truncated": stdout_truncated,
        "stderr_truncated": stderr_truncated,
        "output_truncated": stdout_truncated or stderr_truncated,
    }


def _execute_with_timeout_impl(arguments: dict[str, Any]) -> dict[str, Any]:
    command, shell_mode = _normalize_command(arguments.get("command"))
    timeout_seconds = _as_positive_float(
        arguments.get("timeout_seconds"),
        "timeout_seconds",
        DEFAULT_TIMEOUT_SECONDS,
    )
    max_output_chars = _as_positive_int(
        arguments.get("max_output_chars"),
        "max_output_chars",
        DEFAULT_MAX_OUTPUT_CHARS,
    )

    cwd = _resolve_optional_cwd(arguments)
    env = _resolve_optional_env(arguments)

    started_at = time.monotonic()
    stdout = ""
    stderr = ""
    return_code = 0
    timed_out = False

    try:
        completed = subprocess.run(
            command,
            shell=shell_mode,
            capture_output=True,
            text=True,
            timeout=timeout_seconds,
            cwd=str(cwd) if cwd is not None else None,
            env=env,
            check=False,
        )
        stdout = completed.stdout or ""
        stderr = completed.stderr or ""
        return_code = int(completed.returncode)
    except subprocess.TimeoutExpired as err:
        timed_out = True
        return_code = -1
        stdout = _coerce_subprocess_output(err.stdout)
        stderr = _coerce_subprocess_output(err.stderr)
    except OSError as err:
        raise ToolError(-32012, f"failed to execute command: {err}") from err

    duration_ms = int((time.monotonic() - started_at) * 1000)
    execution_id = str(uuid.uuid4())
    record = {
        "execution_id": execution_id,
        "command": command,
        "cwd": str(cwd) if cwd is not None else str(Path.cwd()),
        "return_code": return_code,
        "timed_out": timed_out,
        "duration_ms": duration_ms,
        "stdout": stdout,
        "stderr": stderr,
    }
    _store_execution(record)
    return _render_execution(record, max_output_chars)


def _get_output_impl(arguments: dict[str, Any]) -> dict[str, Any]:
    execution_id = _require_string(arguments.get("execution_id"), "execution_id")
    max_output_chars = _as_positive_int(
        arguments.get("max_output_chars"),
        "max_output_chars",
        DEFAULT_MAX_OUTPUT_CHARS,
    )

    record = _EXECUTIONS.get(execution_id)
    if record is None:
        raise ToolError(-32004, f"execution not found: {execution_id}")

    return _render_execution(record, max_output_chars)


@mcp.tool()
def execute_with_timeout(
    command: str | list[str],
    timeout_seconds: float = DEFAULT_TIMEOUT_SECONDS,
    max_output_chars: int = DEFAULT_MAX_OUTPUT_CHARS,
    cwd: str | None = None,
    env: dict[str, str] | None = None,
) -> str:
    """Execute a command with timeout and output capture."""
    result = _execute_with_timeout_impl(
        {
            "command": command,
            "timeout_seconds": timeout_seconds,
            "max_output_chars": max_output_chars,
            "cwd": cwd,
            "env": env,
        }
    )
    return json.dumps(result, ensure_ascii=False)


@mcp.tool()
def get_output(
    execution_id: str,
    max_output_chars: int = DEFAULT_MAX_OUTPUT_CHARS,
) -> str:
    """Get captured output for a previous execution."""
    result = _get_output_impl(
        {
            "execution_id": execution_id,
            "max_output_chars": max_output_chars,
        }
    )
    return json.dumps(result, ensure_ascii=False)


def main() -> None:
    mcp.run()


if __name__ == "__main__":
    main()
