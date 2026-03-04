from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
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


def git_status(repo_path: str = ".") -> dict[str, Any]:
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


def git_diff(repo_path: str = ".", revision: str | None = None) -> dict[str, Any]:
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


def git_log(repo_path: str = ".", max_count: int = 20) -> dict[str, Any]:
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


def git_apply_patch(
    repo_path: str, patch: str, check_only: bool = False
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


def _build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="agentd-mcp-git")
    subparsers = parser.add_subparsers(dest="tool", required=True)

    status_parser = subparsers.add_parser("git-status")
    status_parser.add_argument("--repo-path", default=".")

    diff_parser = subparsers.add_parser("git-diff")
    diff_parser.add_argument("--repo-path", default=".")
    diff_parser.add_argument("--revision")

    log_parser = subparsers.add_parser("git-log")
    log_parser.add_argument("--repo-path", default=".")
    log_parser.add_argument("--max-count", type=int, default=20)

    patch_parser = subparsers.add_parser("git-apply-patch")
    patch_parser.add_argument("--repo-path", required=True)
    patch_parser.add_argument("--patch", required=True)
    patch_parser.add_argument("--check-only", action="store_true")

    return parser


def main() -> int:
    parser = _build_parser()
    args = parser.parse_args()
    if args.tool == "git-status":
        payload = git_status(repo_path=args.repo_path)
    elif args.tool == "git-diff":
        payload = git_diff(repo_path=args.repo_path, revision=args.revision)
    elif args.tool == "git-log":
        payload = git_log(repo_path=args.repo_path, max_count=args.max_count)
    else:
        payload = git_apply_patch(
            repo_path=args.repo_path,
            patch=args.patch,
            check_only=args.check_only,
        )
    print(json.dumps(payload, ensure_ascii=False))
    return 0 if payload.get("ok") else 1


if __name__ == "__main__":
    raise SystemExit(main())
