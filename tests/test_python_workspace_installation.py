from __future__ import annotations

import tomllib
from pathlib import Path
from typing import TypedDict, cast


REPO_ROOT = Path(__file__).resolve().parents[1]
PYPROJECT_PATH = REPO_ROOT / "pyproject.toml"
WORKSPACE_PACKAGES = {
    "agentd-agent-lite",
    "agentd-mcp-fs",
    "agentd-mcp-shell",
    "agentd-mcp-search",
    "agentd-mcp-git",
}


class WorkspaceSource(TypedDict):
    workspace: bool


class UvConfig(TypedDict):
    sources: dict[str, WorkspaceSource]


class ToolConfig(TypedDict):
    uv: UvConfig


class ProjectConfig(TypedDict):
    dependencies: list[str]


class RootPyproject(TypedDict):
    project: ProjectConfig
    tool: ToolConfig


def _load_pyproject() -> RootPyproject:
    with PYPROJECT_PATH.open("rb") as handle:
        return cast(RootPyproject, cast(object, tomllib.load(handle)))


def test_root_uv_sync_installs_runtime_workspace_packages() -> None:
    pyproject = _load_pyproject()

    dependencies = set(pyproject["project"]["dependencies"])
    assert dependencies.issuperset(WORKSPACE_PACKAGES)

    sources = pyproject["tool"]["uv"]["sources"]
    for package_name in WORKSPACE_PACKAGES:
        assert sources[package_name] == {"workspace": True}
