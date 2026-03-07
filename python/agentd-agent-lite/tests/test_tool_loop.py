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

    def fake_call_rpc(_: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        if method == "RecordUsage":
            assert params["provider_request_id"] == "req-2"
            assert params["usage_source"] == "provider"
            assert params["transport_mode"] == "real"
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


def test_provider_messages_sanitize_assistant_tool_call_names() -> None:
    session = _CLI_MODULE.AgentSession("agent-tool-provider-serialization")
    assistant = session._append_message("assistant", "")
    assistant["tool_calls"] = [
        {
            "id": "call-1",
            "type": "function",
            "function": {
                "name": "builtin.lite.upper",
                "arguments": '{"prompt":"hello"}',
            },
        }
    ]

    provider_messages = _CLI_MODULE._provider_messages_from_session(session)
    assert (
        provider_messages[0]["tool_calls"][0]["function"]["name"]
        == "builtin_lite_upper"
    )


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

    def fake_call_rpc(_: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        if method == "RecordUsage":
            assert params["provider_request_id"] == "req-loop"
            assert params["usage_source"] == "provider"
            assert params["transport_mode"] == "real"
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


def test_policy_ask_blocks_before_provider_call(monkeypatch, capsys) -> None:
    invoked = {"provider": False}

    def fail_if_provider_called(**_: Any) -> dict[str, Any]:
        invoked["provider"] = True
        raise AssertionError("provider should not be called when policy asks approval")

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "ask"}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fail_if_provider_called)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 2
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "blocked"
    assert payload["error"] == "policy.ask"
    assert invoked["provider"] is False


def test_tool_loop_policy_ask_blocks_after_provider_call(monkeypatch, capsys) -> None:
    state = {"authorize_calls": 0}

    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        return {
            "output": "",
            "input_tokens": 5,
            "output_tokens": 2,
            "total_tokens": 7,
            "provider_request_id": "req-ask-loop",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [
                {
                    "id": "call-1",
                    "name": "builtin.lite.upper",
                    "arguments": '{"prompt":"ask me"}',
                }
            ],
        }

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            state["authorize_calls"] += 1
            if state["authorize_calls"] == 1:
                return {"decision": "allow"}
            return {"decision": "ask"}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 2
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "blocked"
    assert payload["error"] == "policy.ask"
    assert payload["provider_call_attempted"] is False


def test_tool_calling_loop_handles_two_consecutive_tool_calls(
    monkeypatch, capsys
) -> None:
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
        if state["attempt"] == 2:
            return {
                "output": "",
                "input_tokens": 7,
                "output_tokens": 3,
                "total_tokens": 10,
                "provider_request_id": "req-2",
                "request_id_source": "response._request_id",
                "provider_model": "gpt-4o-mini",
                "usage_source": "provider",
                "transport_mode": "real",
                "tool_calls": [
                    {
                        "id": "call-2",
                        "name": "builtin.lite.upper",
                        "arguments": '{"prompt":"world"}',
                    }
                ],
            }
        return {
            "output": "HELLO WORLD",
            "input_tokens": 6,
            "output_tokens": 2,
            "total_tokens": 8,
            "provider_request_id": "req-3",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        if method == "RecordUsage":
            assert params["provider_request_id"] == "req-3"
            assert params["usage_source"] == "provider"
            assert params["transport_mode"] == "real"
            return {"accepted": True}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "completed"
    assert payload["llm"]["output"] == "HELLO WORLD"
    assert len(payload["tool"]["calls"]) == 2
    assert payload["tool"]["calls"][0]["output"] == "HELLO"
    assert payload["tool"]["calls"][1]["output"] == "WORLD"
    assert payload["llm"]["input_tokens"] == 23
    assert payload["llm"]["output_tokens"] == 10
    assert payload["llm"]["total_tokens"] == 33


def test_tool_calling_loop_finishes_when_provider_returns_no_tool_call(
    monkeypatch, capsys
) -> None:
    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        return {
            "output": "direct final answer",
            "input_tokens": 4,
            "output_tokens": 2,
            "total_tokens": 6,
            "provider_request_id": "req-direct",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        if method == "RecordUsage":
            assert params["provider_request_id"] == "req-direct"
            assert params["usage_source"] == "provider"
            assert params["transport_mode"] == "real"
            return {"accepted": True}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 0
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "completed"
    assert payload["llm"]["output"] == "direct final answer"
    assert payload["tool"]["calls"] == []


def test_tool_level_authorize_rpc_error_is_structured(monkeypatch, capsys) -> None:
    state = {"attempt": 0}
    rpc_state = {"authorize_calls": 0}

    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        state["attempt"] += 1
        if state["attempt"] == 1:
            return {
                "output": "",
                "input_tokens": 5,
                "output_tokens": 2,
                "total_tokens": 7,
                "provider_request_id": "req-err-1",
                "request_id_source": "response._request_id",
                "provider_model": "gpt-4o-mini",
                "usage_source": "provider",
                "transport_mode": "real",
                "tool_calls": [
                    {
                        "id": "call-auth-err",
                        "name": "builtin.lite.upper",
                        "arguments": '{"prompt":"hello"}',
                    }
                ],
            }
        raise AssertionError("provider should not be called twice")

    def fake_call_rpc(_: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            rpc_state["authorize_calls"] += 1
            if (
                rpc_state["authorize_calls"] >= 2
                and params["tool"] == "builtin.lite.upper"
            ):
                raise _CLI_MODULE.RpcError(-32008, "authorize transport failure")
            return {"decision": "allow"}
        if method == "RecordUsage":
            raise AssertionError(
                "RecordUsage should not be called on authorize failure"
            )
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 1
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "failed"
    assert payload["stage"] == "authorize"
    assert payload["tool"] == "builtin.lite.upper"
    assert payload["code"] == -32008


def test_tool_loop_emits_stream_lifecycle_before_tool_output(monkeypatch) -> None:
    state = {"attempt": 0}
    events: list[dict[str, Any]] = []

    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        state["attempt"] += 1
        if state["attempt"] == 1:
            return {
                "output": "",
                "input_tokens": 6,
                "output_tokens": 2,
                "total_tokens": 8,
                "provider_request_id": "req-stream-1",
                "request_id_source": "response._request_id",
                "provider_model": "gpt-4o-mini",
                "usage_source": "provider",
                "transport_mode": "real",
                "tool_calls": [
                    {
                        "id": "call-stream-1",
                        "name": "builtin.lite.upper",
                        "arguments": '{"prompt":"hello"}',
                    }
                ],
            }
        return {
            "output": "HELLO",
            "input_tokens": 4,
            "output_tokens": 2,
            "total_tokens": 6,
            "provider_request_id": "req-stream-2",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "ListAvailableTools":
            return {"tools": []}
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    result = _CLI_MODULE.run_chat(
        args=_make_args(),
        llm_config=argparse.Namespace(
            base_url="http://localhost:3000/v1",
            api_key="token-123",
            model="gpt-4o-mini",
            timeout=15,
        ),
        session=_CLI_MODULE.AgentSession("agent-stream-order"),
        user_input="make hello uppercase",
        max_iterations=5,
        max_retries=1,
        stream_handler=events.append,
    )

    assert result["output"] == "HELLO"
    assert [event["type"] for event in events] == [
        "tool-input-start",
        "tool-input-available",
        "tool-output-available",
    ]
    assert events[0] == {
        "type": "tool-input-start",
        "toolCallId": "call-stream-1",
        "toolName": "builtin.lite.upper",
    }
    assert events[1] == {
        "type": "tool-input-available",
        "toolCallId": "call-stream-1",
        "toolName": "builtin.lite.upper",
        "input": {"prompt": "hello"},
    }
    assert events[2] == {
        "type": "tool-output-available",
        "toolCallId": "call-stream-1",
        "toolName": "builtin.lite.upper",
        "input": {"prompt": "hello"},
        "output": "HELLO",
        "errorText": None,
    }
