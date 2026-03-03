import argparse
import importlib
import json
import socket
import sys
import uuid
from dataclasses import dataclass
from typing import Any

from .config import load_config

OpenAI: Any | None = None
AuthenticationError: type[Exception] | None = None
APIConnectionError: type[Exception] | None = None
APIStatusError: type[Exception] | None = None


def _ensure_openai_types() -> None:
    global OpenAI, AuthenticationError, APIConnectionError, APIStatusError
    if OpenAI is not None:
        return

    openai_mod = importlib.import_module("openai")
    OpenAI = getattr(openai_mod, "OpenAI")
    AuthenticationError = getattr(openai_mod, "AuthenticationError")
    APIConnectionError = getattr(openai_mod, "APIConnectionError")
    APIStatusError = getattr(openai_mod, "APIStatusError")


@dataclass(slots=True)
class RpcError(Exception):
    code: int
    message: str

    def __str__(self) -> str:
        return f"RPC error {self.code}: {self.message}"


def call_rpc(socket_path: str, method: str, params: dict[str, Any]) -> dict[str, Any]:
    payload = {
        "jsonrpc": "2.0",
        "id": f"lite-{uuid.uuid4()}",
        "method": method,
        "params": params,
    }

    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as conn:
        conn.connect(socket_path)
        conn.sendall(json.dumps(payload).encode("utf-8"))
        conn.shutdown(socket.SHUT_WR)

        chunks: list[bytes] = []
        while True:
            chunk = conn.recv(4096)
            if not chunk:
                break
            chunks.append(chunk)

    response_raw = b"".join(chunks)
    if not response_raw:
        raise RuntimeError(f"empty response from daemon for method={method}")

    response = json.loads(response_raw.decode("utf-8"))
    error = response.get("error")
    if error is not None:
        raise RpcError(
            int(error.get("code", -1)), str(error.get("message", "unknown rpc error"))
        )

    result = response.get("result")
    if not isinstance(result, dict):
        raise RuntimeError(f"invalid rpc result for method={method}")
    return result


def estimate_tokens(text: str) -> int:
    count = len(text.split())
    return count if count > 0 else 1


def run_builtin_tool(tool_name: str, prompt: str) -> str:
    if tool_name == "builtin.lite.echo":
        return prompt
    if tool_name == "builtin.lite.upper":
        return prompt.upper()
    return prompt


def _extract_message_text(completion: Any) -> str:
    choices = getattr(completion, "choices", None)
    if isinstance(choices, list) and choices:
        first_choice = choices[0]
        message = getattr(first_choice, "message", None)
        content = getattr(message, "content", None)
        if isinstance(content, str) and content:
            return content
    return ""


def _extract_provider_request_id(
    completion: Any, headers: dict[str, str]
) -> tuple[str, str]:
    request_id = getattr(completion, "_request_id", None)
    if isinstance(request_id, str) and request_id:
        return request_id, "response._request_id"

    header_request_id = headers.get("x-request-id") or headers.get("X-Request-ID")
    if isinstance(header_request_id, str) and header_request_id:
        return header_request_id, "header.x-request-id"

    completion_id = getattr(completion, "id", None)
    if isinstance(completion_id, str) and completion_id:
        return completion_id, "body.id"

    return "", "unavailable"


def _invoke_real_single_turn(
    *, base_url: str, api_key: str, model: str, timeout: int, prompt: str
) -> dict[str, Any]:
    _ensure_openai_types()
    if OpenAI is None:
        raise RuntimeError("openai client unavailable")

    client = OpenAI(base_url=base_url, api_key=api_key, timeout=timeout)
    raw_response = client.chat.completions.with_raw_response.create(
        model=model,
        messages=[{"role": "user", "content": prompt}],
    )
    completion = raw_response.parse()
    response_text = _extract_message_text(completion)
    if not response_text:
        raise RuntimeError("provider returned empty assistant content")

    usage = getattr(completion, "usage", None)
    prompt_tokens = getattr(usage, "prompt_tokens", None)
    completion_tokens = getattr(usage, "completion_tokens", None)
    total_tokens = getattr(usage, "total_tokens", None)

    usage_source = "provider"
    if not isinstance(prompt_tokens, int) or not isinstance(completion_tokens, int):
        prompt_tokens = estimate_tokens(prompt)
        completion_tokens = estimate_tokens(response_text)
        total_tokens = prompt_tokens + completion_tokens
        usage_source = "estimated"
    elif not isinstance(total_tokens, int):
        total_tokens = prompt_tokens + completion_tokens

    raw_headers = dict(raw_response.headers.items()) if raw_response.headers else {}
    provider_request_id, request_id_source = _extract_provider_request_id(
        completion, raw_headers
    )

    provider_model = getattr(completion, "model", None)
    return {
        "output": response_text,
        "input_tokens": prompt_tokens,
        "output_tokens": completion_tokens,
        "total_tokens": total_tokens,
        "provider_request_id": provider_request_id,
        "request_id_source": request_id_source,
        "provider_model": provider_model if isinstance(provider_model, str) else model,
        "usage_source": usage_source,
        "transport_mode": "real",
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog="agentd-agent-lite")
    parser.add_argument("--socket-path", default="/tmp/agentd.sock")
    parser.add_argument("--agent-id", required=True)
    parser.add_argument("--prompt", required=True)
    parser.add_argument("--model", default=None)
    parser.add_argument("--tool", default="builtin.lite.echo")
    parser.add_argument(
        "--base-url", default=None, help="OpenAI-compatible API base URL"
    )
    parser.add_argument("--api-key", default=None, help="OpenAI-compatible API key")
    parser.add_argument(
        "--timeout", type=int, default=None, help="Request timeout in seconds"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Dry run mode - skip LLM call, for config validation only",
    )
    return parser.parse_args()


def run_once(args: argparse.Namespace) -> int:
    try:
        llm_config = load_config(
            base_url=args.base_url,
            api_key=args.api_key,
            model=args.model,
            timeout=args.timeout,
        )
    except ValueError as err:
        err_text = str(err)
        reason_code = "INVALID_CONFIG"
        if "base_url" in err_text:
            reason_code = "INVALID_BASE_URL"
        elif "api_key" in err_text:
            reason_code = "MISSING_API_KEY"
        elif "timeout" in err_text:
            reason_code = "INVALID_TIMEOUT"
        print(
            json.dumps(
                {
                    "status": "failed",
                    "stage": "config",
                    "error": "invalid_config",
                    "reason_code": reason_code,
                    "message": str(err),
                },
                ensure_ascii=False,
            )
        )
        return 1

    if args.dry_run:
        print(
            json.dumps(
                {
                    "status": "dry_run",
                    "config": {
                        "base_url": llm_config.base_url,
                        "model": llm_config.model,
                        "timeout": llm_config.timeout,
                    },
                },
                ensure_ascii=False,
            )
        )
        return 0

    try:
        authorization = call_rpc(
            args.socket_path,
            "AuthorizeTool",
            {
                "tool": args.tool,
                "agent_id": args.agent_id,
            },
        )
    except RpcError as err:
        if err.code == -32016:
            print(
                json.dumps(
                    {
                        "status": "blocked",
                        "agent_id": args.agent_id,
                        "tool": args.tool,
                        "error": "policy.deny",
                        "code": err.code,
                        "message": err.message,
                    },
                    ensure_ascii=False,
                )
            )
            return 2
        print(
            json.dumps(
                {
                    "status": "failed",
                    "stage": "authorize",
                    "code": err.code,
                    "message": err.message,
                },
                ensure_ascii=False,
            )
        )
        return 1

    decision = str(authorization.get("decision", "ask"))
    if decision == "deny":
        print(
            json.dumps(
                {
                    "status": "blocked",
                    "agent_id": args.agent_id,
                    "tool": args.tool,
                    "error": "policy.deny",
                    "message": "tool denied by policy engine",
                },
                ensure_ascii=False,
            )
        )
        return 2

    tool_output = run_builtin_tool(args.tool, args.prompt)

    try:
        llm_result = _invoke_real_single_turn(
            base_url=llm_config.base_url,
            api_key=llm_config.api_key,
            model=llm_config.model,
            timeout=llm_config.timeout,
            prompt=args.prompt,
        )
    except Exception as err:
        auth_error = AuthenticationError
        conn_error = APIConnectionError
        status_error = APIStatusError

        if auth_error is not None and isinstance(err, auth_error):
            provider_request_id = getattr(err, "request_id", None)
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "llm",
                        "error": "provider.auth",
                        "message": str(err),
                        "provider_request_id": provider_request_id,
                        "transport_mode": "real",
                    },
                    ensure_ascii=False,
                )
            )
            return 1

        if conn_error is not None and isinstance(err, conn_error):
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "llm",
                        "error": "provider.network",
                        "message": str(err),
                        "provider_request_id": None,
                        "transport_mode": "real",
                    },
                    ensure_ascii=False,
                )
            )
            return 1

        if status_error is not None and isinstance(err, status_error):
            provider_request_id = getattr(err, "request_id", None)
            category = (
                "provider.auth"
                if getattr(err, "status_code", None) == 401
                else "provider.http"
            )
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "llm",
                        "error": category,
                        "message": str(err),
                        "provider_request_id": provider_request_id,
                        "transport_mode": "real",
                    },
                    ensure_ascii=False,
                )
            )
            return 1

        print(
            json.dumps(
                {
                    "status": "failed",
                    "stage": "llm",
                    "error": "provider.unknown",
                    "message": str(err),
                    "provider_request_id": getattr(err, "request_id", None),
                    "transport_mode": "real",
                },
                ensure_ascii=False,
            )
        )
        return 1

    response_text = str(llm_result["output"])
    input_tokens = int(llm_result["input_tokens"])
    output_tokens = int(llm_result["output_tokens"])
    total_tokens = int(llm_result["total_tokens"])
    cost_usd = round(total_tokens * 0.000001, 8)

    try:
        usage = call_rpc(
            args.socket_path,
            "RecordUsage",
            {
                "agent_id": args.agent_id,
                "model_name": llm_config.model,
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "cost_usd": cost_usd,
            },
        )
    except RpcError as err:
        print(
            json.dumps(
                {
                    "status": "failed",
                    "stage": "record_usage",
                    "code": err.code,
                    "message": err.message,
                },
                ensure_ascii=False,
            )
        )
        return 3

    print(
        json.dumps(
            {
                "status": "completed",
                "agent_id": args.agent_id,
                "prompt": args.prompt,
                "model": llm_config.model,
                "tool": {
                    "name": args.tool,
                    "decision": decision,
                    "output": tool_output,
                },
                "llm": {
                    "output": response_text,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "total_tokens": total_tokens,
                    "estimated_cost_usd": cost_usd,
                    "provider_request_id": llm_result["provider_request_id"],
                    "request_id_source": llm_result["request_id_source"],
                    "provider_model": llm_result["provider_model"],
                    "usage_source": llm_result["usage_source"],
                    "transport_mode": llm_result["transport_mode"],
                },
                "usage": usage,
            },
            ensure_ascii=False,
        )
    )
    return 0


def main() -> int:
    args = parse_args()
    return run_once(args)


if __name__ == "__main__":
    sys.exit(main())
