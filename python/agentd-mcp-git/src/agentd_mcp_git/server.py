from __future__ import annotations

import json
import subprocess
import tempfile
from importlib import import_module
from pathlib import Path
from typing import Any

FastMCP = import_module("mcp.server.fastmcp").FastMCP

SERVER_NAME = "agentd-mcp-git"

mcp = FastMCP(SERVER_NAME)


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


def _validate_repo_path(repo_path: str) -> Path:
    candidate = Path(repo_path)
    if not candidate.exists():
        raise FileNotFoundError(f"repository path does not exist: {repo_path}")
    if not candidate.is_dir():
        raise NotADirectoryError(f"repository path is not a directory: {repo_path}")
    git_dir = candidate / ".git"
    if not git_dir.exists():
        raise ValueError(f"repository path is not a git repository: {repo_path}")
    return candidate


def _run_git(repo_path: str, args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", repo_path, *args],
        capture_output=True,
        text=True,
        check=False,
    )


def _git_status_impl(repo_path: str = ".") -> dict[str, Any]:
    try:
        validated = _validate_repo_path(repo_path)
        completed = _run_git(str(validated), ["status", "--short", "--branch"])
        if completed.returncode != 0:
            return _error(
                "GIT_COMMAND_FAILED",
                "git status failed",
                {
                    "returncode": completed.returncode,
                    "stderr": completed.stderr.strip(),
                },
            )
        return _ok(
            {
                "tool": "git_status",
                "repo_path": str(validated),
                "output": completed.stdout,
            }
        )
    except Exception as exc:
        return _error("GIT_OPERATION_FAILED", str(exc), {"repo_path": repo_path})


def _git_diff_impl(repo_path: str = ".", revision: str | None = None) -> dict[str, Any]:
    try:
        validated = _validate_repo_path(repo_path)
        args = ["diff"]
        if revision:
            args.append(revision)
        completed = _run_git(str(validated), args)
        if completed.returncode != 0:
            return _error(
                "GIT_COMMAND_FAILED",
                "git diff failed",
                {
                    "returncode": completed.returncode,
                    "stderr": completed.stderr.strip(),
                },
            )
        return _ok(
            {
                "tool": "git_diff",
                "repo_path": str(validated),
                "revision": revision,
                "output": completed.stdout,
            }
        )
    except Exception as exc:
        return _error(
            "GIT_OPERATION_FAILED",
            str(exc),
            {"repo_path": repo_path, "revision": revision},
        )


def _git_log_impl(repo_path: str = ".", max_count: int = 20) -> dict[str, Any]:
    try:
        validated = _validate_repo_path(repo_path)
        bounded_count = max(1, max_count)
        completed = _run_git(
            str(validated),
            ["log", f"--max-count={bounded_count}", "--pretty=format:%h %s"],
        )
        if completed.returncode != 0:
            return _error(
                "GIT_COMMAND_FAILED",
                "git log failed",
                {
                    "returncode": completed.returncode,
                    "stderr": completed.stderr.strip(),
                },
            )
        lines = [line for line in completed.stdout.splitlines() if line.strip()]
        return _ok(
            {
                "tool": "git_log",
                "repo_path": str(validated),
                "max_count": bounded_count,
                "entries": lines,
            }
        )
    except Exception as exc:
        return _error(
            "GIT_OPERATION_FAILED",
            str(exc),
            {"repo_path": repo_path, "max_count": max_count},
        )


def _git_apply_patch_impl(
    repo_path: str,
    patch: str,
    check_only: bool = False,
) -> dict[str, Any]:
    try:
        validated = _validate_repo_path(repo_path)
        if not patch.strip():
            return _error(
                "INVALID_PATCH",
                "patch content must not be empty",
                {"field": "patch"},
            )
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False) as handle:
            handle.write(patch)
            patch_path = Path(handle.name)

        check_result = _run_git(str(validated), ["apply", "--check", str(patch_path)])
        if check_result.returncode != 0:
            return _error(
                "INVALID_PATCH",
                "git apply --check failed",
                {
                    "returncode": check_result.returncode,
                    "stderr": check_result.stderr.strip(),
                },
            )

        if not check_only:
            apply_result = _run_git(str(validated), ["apply", str(patch_path)])
            if apply_result.returncode != 0:
                return _error(
                    "GIT_COMMAND_FAILED",
                    "git apply failed",
                    {
                        "returncode": apply_result.returncode,
                        "stderr": apply_result.stderr.strip(),
                    },
                )

        return _ok(
            {
                "tool": "git_apply_patch",
                "repo_path": str(validated),
                "check_only": check_only,
                "applied": not check_only,
            }
        )
    except Exception as exc:
        return _error("GIT_OPERATION_FAILED", str(exc), {"repo_path": repo_path})


@mcp.tool()
def git_status(repo_path: str = ".") -> str:
    """Show short git status for a repository."""
    return json.dumps(_git_status_impl(repo_path=repo_path), ensure_ascii=False)


@mcp.tool()
def git_diff(repo_path: str = ".", revision: str | None = None) -> str:
    """Show git diff for working tree or a specific revision."""
    return json.dumps(
        _git_diff_impl(repo_path=repo_path, revision=revision),
        ensure_ascii=False,
    )


@mcp.tool()
def git_log(repo_path: str = ".", max_count: int = 20) -> str:
    """Show recent commit entries in one-line format."""
    return json.dumps(
        _git_log_impl(repo_path=repo_path, max_count=max_count),
        ensure_ascii=False,
    )


@mcp.tool()
def git_apply_patch(repo_path: str, patch: str, check_only: bool = False) -> str:
    """Validate and optionally apply a git patch."""
    return json.dumps(
        _git_apply_patch_impl(repo_path=repo_path, patch=patch, check_only=check_only),
        ensure_ascii=False,
    )


def main() -> None:
    mcp.run()


if __name__ == "__main__":
    main()
