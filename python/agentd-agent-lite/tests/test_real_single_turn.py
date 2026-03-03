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


def _make_args(*, dry_run: bool = False) -> argparse.Namespace:
    return argparse.Namespace(
        socket_path="/tmp/agentd.sock",
        agent_id="agent-test",
        prompt="say hi",
        model="gpt-4o-mini",
        tool="builtin.lite.echo",
        base_url="http://localhost:3000/v1",
        api_key="token-123",
        timeout=15,
        dry_run=dry_run,
    )


class _FakeMessage:
    def __init__(self, content: str) -> None:
        self.content = content


class _FakeChoice:
    def __init__(self, content: str) -> None:
        self.message = _FakeMessage(content)


class _FakeUsage:
    def __init__(self, prompt_tokens: int, completion_tokens: int) -> None:
        self.prompt_tokens = prompt_tokens
        self.completion_tokens = completion_tokens
        self.total_tokens = prompt_tokens + completion_tokens


class _FakeCompletion:
    def __init__(self, content: str, request_id: str | None = None) -> None:
        self.choices = [_FakeChoice(content)]
        self.usage = _FakeUsage(prompt_tokens=3, completion_tokens=2)
        self._request_id = request_id


class _FakeRawResponse:
    def __init__(self, completion: _FakeCompletion) -> None:
        self.headers = {"x-request-id": "req-from-header"}
        self._completion = completion

    def parse(self) -> _FakeCompletion:
        return self._completion


class _FakeCreateEndpoint:
    def __init__(self, completion: _FakeCompletion) -> None:
        self._completion = completion

    def create(self, **_: Any) -> _FakeRawResponse:
        return _FakeRawResponse(self._completion)


class _FakeChatCompletions:
    def __init__(self, completion: _FakeCompletion) -> None:
        self.with_raw_response = _FakeCreateEndpoint(completion)


class _FakeChat:
    def __init__(self, completion: _FakeCompletion) -> None:
        self.completions = _FakeChatCompletions(completion)


class _FakeOpenAI:
    def __init__(self, **_: Any) -> None:
        self.chat = _FakeChat(
            _FakeCompletion(content="hello from provider", request_id="req-123")
        )


def test_real_single_turn_success_emits_provider_request_id(
    monkeypatch, capsys
) -> None:
    monkeypatch.setattr(_CLI_MODULE, "OpenAI", _FakeOpenAI)

    def fake_call_rpc(
        socket_path: str, method: str, params: dict[str, Any]
    ) -> dict[str, Any]:
        assert socket_path == "/tmp/agentd.sock"
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        assert method == "RecordUsage"
        assert params["input_tokens"] == 3
        assert params["output_tokens"] == 2
        assert params["provider_request_id"] == "req-123"
        assert params["usage_source"] == "provider"
        assert params["transport_mode"] == "real"
        return {"accepted": True}

    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 0

    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "completed"
    assert payload["llm"]["output"] == "hello from provider"
    assert payload["llm"]["provider_request_id"] == "req-123"
    assert payload["llm"]["request_id_source"] == "response._request_id"


class _FakeAuthenticationError(Exception):
    def __init__(self, message: str, request_id: str | None = None) -> None:
        super().__init__(message)
        self.request_id = request_id
        self.message = message


class _ErroringCreateEndpoint:
    def create(self, **_: Any) -> _FakeRawResponse:
        raise _FakeAuthenticationError("invalid token", request_id="req-auth")


class _ErroringChatCompletions:
    def __init__(self) -> None:
        self.with_raw_response = _ErroringCreateEndpoint()


class _ErroringChat:
    def __init__(self) -> None:
        self.completions = _ErroringChatCompletions()


class _ErroringOpenAI:
    def __init__(self, **_: Any) -> None:
        self.chat = _ErroringChat()


def test_real_single_turn_auth_error_is_structured(monkeypatch, capsys) -> None:
    monkeypatch.setattr(_CLI_MODULE, "OpenAI", _ErroringOpenAI)
    monkeypatch.setattr(_CLI_MODULE, "AuthenticationError", _FakeAuthenticationError)

    def fake_call_rpc(_: str, method: str, __: dict[str, Any]) -> dict[str, Any]:
        if method == "AuthorizeTool":
            return {"decision": "allow"}
        raise AssertionError("RecordUsage must not be called on auth error")

    monkeypatch.setattr(_CLI_MODULE, "call_rpc", fake_call_rpc)

    exit_code = _CLI_MODULE.run_once(_make_args())
    assert exit_code == 1

    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "failed"
    assert payload["stage"] == "llm"
    assert payload["error"] == "provider.auth"
    assert payload["provider_request_id"] == "req-auth"


def test_dry_run_does_not_instantiate_openai_client(monkeypatch, capsys) -> None:
    def fail_if_called(**_: Any) -> None:
        raise AssertionError("OpenAI client must not be created in dry-run")

    monkeypatch.setattr(_CLI_MODULE, "OpenAI", fail_if_called)

    exit_code = _CLI_MODULE.run_once(_make_args(dry_run=True))
    assert exit_code == 0

    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "dry_run"
    assert payload["config"]["base_url"] == "http://localhost:3000/v1"


def test_invalid_base_url_returns_stable_reason_code(capsys) -> None:
    args = _make_args(dry_run=True)
    args.base_url = "not-a-url"

    exit_code = _CLI_MODULE.run_once(args)
    assert exit_code == 1

    payload = json.loads(capsys.readouterr().out)
    assert payload["status"] == "failed"
    assert payload["stage"] == "config"
    assert payload["reason_code"] == "INVALID_BASE_URL"
