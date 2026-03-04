import argparse
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
        agent_id="agent-multi-turn",
        prompt="placeholder",
        model="gpt-4o-mini",
        tool="builtin.lite.upper",
        base_url="http://localhost:3000/v1",
        api_key="token-123",
        timeout=15,
        max_iterations=5,
        max_retries=1,
        max_context_tokens=0,
        dry_run=False,
    )


def _make_llm_config() -> argparse.Namespace:
    return argparse.Namespace(
        base_url="http://localhost:3000/v1",
        api_key="token-123",
        model="gpt-4o-mini",
        timeout=15,
    )


def test_chat_keeps_context_across_turns(monkeypatch) -> None:
    state = {"attempt": 0}

    def fake_invoke_with_retry(**kwargs: Any) -> dict[str, Any]:
        state["attempt"] += 1
        messages = kwargs["messages"]
        if state["attempt"] == 1:
            assert any(item["content"] == "remember: sky is blue" for item in messages)
            return {
                "output": "noted",
                "input_tokens": 4,
                "output_tokens": 2,
                "total_tokens": 6,
                "provider_request_id": "req-turn-1",
                "request_id_source": "response._request_id",
                "provider_model": "gpt-4o-mini",
                "usage_source": "provider",
                "transport_mode": "real",
                "tool_calls": [],
            }

        assert any(item["content"] == "remember: sky is blue" for item in messages)
        assert any(item["content"] == "noted" for item in messages)
        assert any(item["content"] == "what color is sky?" for item in messages)
        return {
            "output": "sky is blue",
            "input_tokens": 6,
            "output_tokens": 3,
            "total_tokens": 9,
            "provider_request_id": "req-turn-2",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "ListAvailableTools":
            return {"tools": []}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    session = _CLI_MODULE.AgentSession("agent-multi-turn")
    llm_config = _make_llm_config()
    args = _make_args()

    first = _CLI_MODULE.run_chat(
        args=args,
        llm_config=llm_config,
        session=session,
        user_input="remember: sky is blue",
        max_iterations=5,
        max_retries=1,
    )
    assert first["output"] == "noted"

    second = _CLI_MODULE.run_chat(
        args=args,
        llm_config=llm_config,
        session=session,
        user_input="what color is sky?",
        max_iterations=5,
        max_retries=1,
    )
    assert second["output"] == "sky is blue"


def test_tool_loop_reentrant_in_multi_turn(monkeypatch) -> None:
    state = {"attempt": 0}

    def fake_invoke_with_retry(**kwargs: Any) -> dict[str, Any]:
        state["attempt"] += 1
        messages = kwargs["messages"]

        if state["attempt"] == 1:
            return {
                "output": "",
                "input_tokens": 8,
                "output_tokens": 3,
                "total_tokens": 11,
                "provider_request_id": "req-r1",
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
            assert any(
                item["role"] == "tool" and item["content"] == "HELLO"
                for item in messages
            )
            return {
                "output": "HELLO acknowledged",
                "input_tokens": 5,
                "output_tokens": 2,
                "total_tokens": 7,
                "provider_request_id": "req-r2",
                "request_id_source": "response._request_id",
                "provider_model": "gpt-4o-mini",
                "usage_source": "provider",
                "transport_mode": "real",
                "tool_calls": [],
            }

        assert any("HELLO acknowledged" == item.get("content") for item in messages)
        assert any("second turn" == item.get("content") for item in messages)
        return {
            "output": "second turn complete",
            "input_tokens": 4,
            "output_tokens": 2,
            "total_tokens": 6,
            "provider_request_id": "req-r3",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
        if method == "ListAvailableTools":
            return {"tools": []}
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        raise AssertionError(f"unexpected method {method}, params={params}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    session = _CLI_MODULE.AgentSession("agent-reentrant")
    llm_config = _make_llm_config()
    args = _make_args()

    first = _CLI_MODULE.run_chat(
        args=args,
        llm_config=llm_config,
        session=session,
        user_input="first turn",
        max_iterations=5,
        max_retries=1,
    )
    assert first["output"] == "HELLO acknowledged"
    assert first["tool_calls"][0]["output"] == "HELLO"

    second = _CLI_MODULE.run_chat(
        args=args,
        llm_config=llm_config,
        session=session,
        user_input="second turn",
        max_iterations=5,
        max_retries=1,
    )
    assert second["output"] == "second turn complete"


def test_chat_triggers_compact_on_budget_threshold(monkeypatch) -> None:
    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        return {
            "output": "ok",
            "input_tokens": 3,
            "output_tokens": 1,
            "total_tokens": 4,
            "provider_request_id": "req-compact",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "ListAvailableTools":
            return {"tools": []}
        raise AssertionError(f"unexpected method {method}")

    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)
    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    session = _CLI_MODULE.AgentSession("agent-compact", max_context_tokens=5)
    llm_config = _make_llm_config()
    args = _make_args()

    result = _CLI_MODULE.run_chat(
        args=args,
        llm_config=llm_config,
        session=session,
        user_input="this input is long enough to exceed threshold",
        max_iterations=5,
        max_retries=1,
    )

    assert result["compact_triggered"] is True
    assert any(
        message["role"] == "system" and "compact hook" in message["content"]
        for message in session.messages
    )
