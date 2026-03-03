from importlib.util import module_from_spec, spec_from_file_location
from pathlib import Path
from typing import Any

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
    except _RetryableConnectionError:
        pass

    assert attempts["count"] == 2
