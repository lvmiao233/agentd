from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from typing import Any
import argparse
import json

_CLI_PATH = Path(__file__).resolve().parents[1] / "src" / "agentd_agent_lite" / "cli.py"
_CLI_SPEC = spec_from_file_location("agentd_agent_lite.cli", _CLI_PATH)
assert _CLI_SPEC is not None and _CLI_SPEC.loader is not None
_CLI_MODULE = module_from_spec(_CLI_SPEC)
_CLI_SPEC.loader.exec_module(_CLI_MODULE)


class _RetryableConnectionError(Exception):
    pass


def test_invoke_real_with_retry_recovers_from_transient_error(monkeypatch) -> None:
    attempts = {"count": 0}

    def fake_invoke_once(**_: Any) -> dict[str, Any]:
        attempts["count"] += 1
        if attempts["count"] == 1:
            raise _RetryableConnectionError("temporary network issue")
        return {
            "output": "ok",
            "input_tokens": 1,
            "output_tokens": 1,
            "total_tokens": 2,
            "provider_request_id": "req-ok",
            "request_id_source": "response._request_id",
            "provider_model": "gpt-4o-mini",
            "usage_source": "provider",
            "transport_mode": "real",
            "tool_calls": [],
        }

    monkeypatch.setattr(_CLI_MODULE, "APIConnectionError", _RetryableConnectionError)
    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_chat_once", fake_invoke_once)
    monkeypatch.setattr(_CLI_MODULE.time, "sleep", lambda _: None)

    result = _CLI_MODULE._invoke_real_with_retry(
        base_url="http://localhost:3000/v1",
        api_key="token",
        model="gpt-4o-mini",
        timeout=10,
        messages=[{"role": "user", "content": "hi"}],
        tool_name="builtin.lite.upper",
        max_retries=2,
    )
    assert result["output"] == "ok"
    assert attempts["count"] == 2


def test_invoke_real_with_retry_fails_after_retry_budget(monkeypatch) -> None:
    attempts = {"count": 0}

    def always_fail(**_: Any) -> dict[str, Any]:
        attempts["count"] += 1
        raise _RetryableConnectionError("still failing")

    monkeypatch.setattr(_CLI_MODULE, "APIConnectionError", _RetryableConnectionError)
    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_chat_once", always_fail)
    monkeypatch.setattr(_CLI_MODULE.time, "sleep", lambda _: None)

    try:
        _CLI_MODULE._invoke_real_with_retry(
            base_url="http://localhost:3000/v1",
            api_key="token",
            model="gpt-4o-mini",
            timeout=10,
            messages=[{"role": "user", "content": "hi"}],
            tool_name="builtin.lite.upper",
            max_retries=1,
        )
        raise AssertionError("expected retry exhaustion error")
    except _CLI_MODULE.RetryExhaustedError as err:
        assert err.attempts == 2
        assert isinstance(err.last_error, _RetryableConnectionError)

    assert attempts["count"] == 2


class _FakeStatusError(Exception):
    def __init__(self, message: str, status_code: int) -> None:
        super().__init__(message)
        self.status_code = status_code


class _FakeTimeoutError(Exception):
    pass


def test_classify_llm_error_maps_rate_limit(monkeypatch) -> None:
    monkeypatch.setattr(_CLI_MODULE, "APIStatusError", _FakeStatusError)
    error_code, category = _CLI_MODULE._classify_llm_error(
        _FakeStatusError("rate limited", status_code=429)
    )
    assert error_code == "provider.rate_limit"
    assert category == "RATE_LIMIT"


def test_classify_llm_error_maps_timeout(monkeypatch) -> None:
    monkeypatch.setattr(_CLI_MODULE, "APITimeoutError", _FakeTimeoutError)
    error_code, category = _CLI_MODULE._classify_llm_error(
        _FakeTimeoutError("request timeout")
    )
    assert error_code == "provider.timeout"
    assert category == "TIMEOUT"


def test_run_once_surfaces_timeout_category_and_attempts(monkeypatch, capsys) -> None:
    args = argparse.Namespace(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-retry",
        prompt="hello",
        model="gpt-4o-mini",
        tool="builtin.lite.upper",
        base_url="http://localhost:3000/v1",
        api_key="token",
        timeout=10,
        max_iterations=5,
        max_retries=1,
        dry_run=False,
    )

    monkeypatch.setattr(_CLI_MODULE, "APITimeoutError", _FakeTimeoutError)

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        raise AssertionError("RecordUsage must not be called on llm failure")

    def fake_invoke_with_retry(**_: Any) -> dict[str, Any]:
        raise _CLI_MODULE.RetryExhaustedError(
            attempts=2, last_error=_FakeTimeoutError("deadline timed out")
        )

    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)
    monkeypatch.setattr(_CLI_MODULE, "_invoke_real_with_retry", fake_invoke_with_retry)

    exit_code = _CLI_MODULE.run_once(args)
    assert exit_code == 1
    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "failed"
    assert payload["stage"] == "llm"
    assert payload["error"] == "provider.timeout"
    assert payload["error_category"] == "TIMEOUT"
    assert payload["attempts"] == 2
    assert payload["max_retries"] == 1
