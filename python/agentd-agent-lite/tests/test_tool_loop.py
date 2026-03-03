import argparse
import json
from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from typing import Any

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


def _make_args() -> argparse.Namespace:
    return argparse.Namespace(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-tool-loop",
        prompt="make hello uppercase",
        model="gpt-4o-mini",
        tool="builtin.lite.upper",
        base_url="http://localhost:3000/v1",
        api_key="token-123",
        timeout=15,
        max_iterations=5,
        max_retries=1,
        dry_run=False,
    )


def test_tool_calling_loop_returns_final_answer(monkeypatch, capsys) -> None:
    state = {"attempt": 0}

    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        state["attempt"] += 1
        if state["attempt"] == 1:
            return {
                "output": "",
                "input_tokens": 10,
                "output_tokens": 5,
                "total_tokens": 15,
                "provider_request_id": "req-1",
                "request_id_source": "response._request_id",
                "provider_model": "gpt-4o-mini",
                "usage_source": "provider",
                "transport_mode": "real",
                "tool_calls": [
                    {
                        "id": "call-1",
                        "name": "builtin.lite.upper",
                        "arguments": '{"prompt":"hello"}',
                    }
                ],
            }
        return {
            "output": "HELLO",
            "input_tokens": 8,
            "output_tokens": 3,
            "total_tokens": 11,
            "provider_request_id": "req-2",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        if method == "RecordUsage":
            return {"accepted": True}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "completed"
    assert payload["llm"]["output"] == "HELLO"
    assert len(payload["tool"]["calls"]) == 1
    assert payload["tool"]["calls"][0]["output"] == "HELLO"


def test_max_iterations_reached_returns_stable_error(monkeypatch, capsys) -> None:
    args = _make_args()
    args.max_iterations = 1

    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        return {
            "output": "",
            "input_tokens": 10,
            "output_tokens": 5,
            "total_tokens": 15,
            "provider_request_id": "req-loop",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [
                {
                    "id": "call-loop",
                    "name": "builtin.lite.upper",
                    "arguments": '{"prompt":"loop"}',
                }
            ],
        }

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        if method == "RecordUsage":
            return {"accepted": True}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(args)
    assert exit_code == 1
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "failed"
    assert payload["error"] == "MAX_ITERATIONS_REACHED"


def test_policy_deny_blocks_before_provider_call(monkeypatch, capsys) -> None:
    invoked = {"provider": False}

    def fail_if_provider_called(**_: Any) -> dict[str, Any]:
        invoked["provider"] = True
        raise AssertionError("provider should not be called when policy denies")

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            raise _CLI_MODULE.RpcError(-32016, "policy.deny: tool blocked")
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fail_if_provider_called)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 2
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "blocked"
    assert payload["error"] == "policy.deny"
    assert invoked["provider"] is False
