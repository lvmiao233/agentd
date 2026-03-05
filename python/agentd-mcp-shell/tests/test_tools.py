import json
import sys
from pathlib import Path
from importlib.util import module_from_spec, spec_from_file_location
from typing import Any, Callable, cast

_SERVER_PATH = (
    Path(__file__).resolve().parents[1] / "src" / "agentd_mcp_shell" / "server.py"
)
_SERVER_SPEC = spec_from_file_location("agentd_mcp_shell.server", _SERVER_PATH)
assert _SERVER_SPEC is not None and _SERVER_SPEC.loader is not None
_SERVER_MODULE = module_from_spec(_SERVER_SPEC)
sys.modules[_SERVER_SPEC.name] = _SERVER_MODULE
_SERVER_SPEC.loader.exec_module(_SERVER_MODULE)

execute_with_timeout = cast(Callable[..., str], _SERVER_MODULE.execute_with_timeout)
get_output = cast(Callable[..., str], _SERVER_MODULE.get_output)


def _decode(payload: str) -> dict[str, Any]:
    decoded = json.loads(payload)
    assert isinstance(decoded, dict)
    return decoded


def test_execute_timeout() -> None:
    payload = _decode(
        execute_with_timeout(
            command=[sys.executable, "-c", "import time; time.sleep(1)"],
            timeout_seconds=0.1,
        )
    )

    assert payload["timed_out"] is True
    assert payload["return_code"] == -1
    assert isinstance(payload["execution_id"], str)
    assert payload["execution_id"]


def test_execute_and_get_output_roundtrip() -> None:
    execute_payload = _decode(
        execute_with_timeout(
            command=[sys.executable, "-c", "print('shell-ok')"],
            timeout_seconds=3,
        )
    )
    assert execute_payload["timed_out"] is False
    execution_id = execute_payload["execution_id"]
    assert isinstance(execution_id, str)

    output_payload = _decode(get_output(execution_id=execution_id))
    assert "shell-ok" in output_payload["stdout"]
    assert output_payload["timed_out"] is False


def test_execute_with_cwd(tmp_path: Path) -> None:
    payload = _decode(
        execute_with_timeout(
            command=[
                sys.executable,
                "-c",
                "import pathlib; print(pathlib.Path.cwd().name)",
            ],
            cwd=str(tmp_path),
        )
    )
    assert payload["return_code"] == 0
    assert tmp_path.name in payload["stdout"]
