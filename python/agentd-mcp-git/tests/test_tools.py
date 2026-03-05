from __future__ import annotations

import importlib.util
import json
import subprocess
from pathlib import Path

_module_path = (
    Path(__file__).resolve().parents[1] / "src" / "agentd_mcp_git" / "server.py"
)
_spec = importlib.util.spec_from_file_location("agentd_mcp_git_server", _module_path)
assert _spec is not None and _spec.loader is not None
_git = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_git)

git_apply_patch = _git.git_apply_patch
git_diff = _git.git_diff
git_log = _git.git_log
git_status = _git.git_status


def _run(command: list[str], cwd: Path) -> None:
    subprocess.run(command, cwd=cwd, check=True, capture_output=True, text=True)


def _init_repo(repo: Path) -> None:
    _run(["git", "init"], cwd=repo)
    (repo / "notes.txt").write_text("line 1\n", encoding="utf-8")
    _run(["git", "add", "notes.txt"], cwd=repo)
    _run(
        [
            "git",
            "-c",
            "user.name=agentd",
            "-c",
            "user.email=agentd@example.com",
            "commit",
            "-m",
            "init",
        ],
        cwd=repo,
    )


def test_git_status_diff_log(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_repo(repo)

    (repo / "notes.txt").write_text("line 1\nline 2\n", encoding="utf-8")

    status_result = json.loads(git_status(repo_path=str(repo)))
    assert status_result["ok"] is True
    assert "notes.txt" in status_result["data"]["output"]

    diff_result = json.loads(git_diff(repo_path=str(repo)))
    assert diff_result["ok"] is True
    assert "+line 2" in diff_result["data"]["output"]

    log_result = json.loads(git_log(repo_path=str(repo), max_count=5))
    assert log_result["ok"] is True
    assert any("init" in entry for entry in log_result["data"]["entries"])


def test_git_apply_patch_rejects_invalid_patch(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_repo(repo)

    result = json.loads(git_apply_patch(repo_path=str(repo), patch="not a patch"))

    assert result["ok"] is False
    error = result["error"]
    assert set(error.keys()) == {"code", "message", "details"}
    assert error["code"] == "INVALID_PATCH"
    assert "failed" in error["message"]


def test_git_apply_patch_applies_valid_patch(tmp_path: Path) -> None:
    repo = tmp_path / "repo"
    repo.mkdir()
    _init_repo(repo)

    patch = (
        "diff --git a/notes.txt b/notes.txt\n"
        "index a29bdeb..c0d0fb4 100644\n"
        "--- a/notes.txt\n"
        "+++ b/notes.txt\n"
        "@@ -1 +1,2 @@\n"
        " line 1\n"
        "+line patched\n"
    )
    result = json.loads(git_apply_patch(repo_path=str(repo), patch=patch))

    assert result["ok"] is True
    assert result["data"]["applied"] is True
    assert "line patched" in (repo / "notes.txt").read_text(encoding="utf-8")
