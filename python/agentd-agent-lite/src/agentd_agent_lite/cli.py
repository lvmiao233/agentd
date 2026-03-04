import argparse
import importlib
import json
import socket
import sys
import time
import uuid
from dataclasses import dataclass
from typing import Any

from .config import load_config

OpenAI: Any | None = None
AuthenticationError: type[Exception] | None = None
APIConnectionError: type[Exception] | None = None
APIStatusError: type[Exception] | None = None
APITimeoutError: type[Exception] | None = None


def _ensure_openai_types() -> None:
    global \
        OpenAI, \
        AuthenticationError, \
        APIConnectionError, \
        APIStatusError, \
        APITimeoutError
    if OpenAI is not None:
        return

    openai_mod = importlib.import_module("openai")
    OpenAI = getattr(openai_mod, "OpenAI")
    AuthenticationError = getattr(openai_mod, "AuthenticationError")
    APIConnectionError = getattr(openai_mod, "APIConnectionError")
    APIStatusError = getattr(openai_mod, "APIStatusError")
    APITimeoutError = getattr(openai_mod, "APITimeoutError", None)


@dataclass(slots=True)
class RpcError(Exception):
    code: int
    message: str

    def __str__(self) -> str:
        return f"RPC error {self.code}: {self.message}"


@dataclass(slots=True)
class RetryExhaustedError(Exception):
    attempts: int
    last_error: Exception

    def __str__(self) -> str:
        return (
            f"retry budget exhausted after {self.attempts} attempts: {self.last_error}"
        )


@dataclass(slots=True)
class DiscoveredTool:
    openai_name: str
    server: str
    tool: str
    description: str
    parameters: dict[str, Any]


class AgentSession:
    def __init__(self, agent_id: str, *, max_context_tokens: int = 0) -> None:
        self.agent_id = agent_id
        self.messages: list[dict[str, Any]] = []
        self.head_id: str | None = None
        self.tool_results_cache: dict[str, Any] = {}
        self.context_window_tokens = 0
        self.max_context_tokens = max_context_tokens
        self.discovered_tools: list[DiscoveredTool] = []
        self.discovered_signature = ""

    def _append_message(self, role: str, content: str) -> dict[str, Any]:
        message = {
            "id": str(uuid.uuid4()),
            "parent_id": self.head_id,
            "role": role,
            "content": content,
        }
        self.messages.append(message)
        self.head_id = message["id"]
        return message

    def _get_active_branch(self) -> list[dict[str, Any]]:
        branch: list[dict[str, Any]] = []
        index = {
            message["id"]: message
            for message in self.messages
            if isinstance(message.get("id"), str) and message["id"]
        }

        current_id = self.head_id
        visited: set[str] = set()
        while (
            isinstance(current_id, str)
            and current_id
            and current_id not in visited
            and current_id in index
        ):
            visited.add(current_id)
            message = index[current_id]
            branch.append(message)

            parent_id = message.get("parent_id")
            current_id = parent_id if isinstance(parent_id, str) else None

        return list(reversed(branch))


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


def _extract_tool_calls(completion: Any) -> list[dict[str, str]]:
    choices = getattr(completion, "choices", None)
    if not isinstance(choices, list) or not choices:
        return []

    first_choice = choices[0]
    message = getattr(first_choice, "message", None)
    raw_tool_calls = getattr(message, "tool_calls", None)
    if not isinstance(raw_tool_calls, list):
        return []

    normalized_calls: list[dict[str, str]] = []
    for item in raw_tool_calls:
        if item is None:
            continue

        item_id = getattr(item, "id", None)
        function = getattr(item, "function", None)
        name = getattr(function, "name", None)
        arguments = getattr(function, "arguments", None)

        if isinstance(item, dict):
            item_id = item.get("id", item_id)
            function_dict = item.get("function")
            if isinstance(function_dict, dict):
                name = function_dict.get("name", name)
                arguments = function_dict.get("arguments", arguments)
            else:
                name = item.get("name", name)
                arguments = item.get("arguments", arguments)

        if not isinstance(item_id, str) or not item_id:
            continue
        if not isinstance(name, str) or not name:
            continue

        if isinstance(arguments, str):
            arg_text = arguments
        else:
            arg_text = "{}"

        normalized_calls.append(
            {
                "id": item_id,
                "name": name,
                "arguments": arg_text,
            }
        )

    return normalized_calls


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
    return _invoke_real_chat_once(
        base_url=base_url,
        api_key=api_key,
        model=model,
        timeout=timeout,
        messages=[{"role": "user", "content": prompt}],
        tool_name="builtin.lite.echo",
    )


def _build_tool_schema(tool_name: str) -> list[dict[str, Any]]:
    return [
        {
            "type": "function",
            "function": {
                "name": tool_name,
                "description": "Execute builtin lite tool.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "Tool input text.",
                        }
                    },
                    "required": ["prompt"],
                },
            },
        }
    ]


def _normalize_parameters_schema(raw_schema: Any) -> dict[str, Any]:
    if not isinstance(raw_schema, dict):
        return {
            "type": "object",
            "properties": {},
            "additionalProperties": True,
        }

    normalized = dict(raw_schema)
    if normalized.get("type") != "object":
        normalized["type"] = "object"
    if not isinstance(normalized.get("properties"), dict):
        normalized["properties"] = {}

    required = normalized.get("required")
    if isinstance(required, list):
        normalized["required"] = [item for item in required if isinstance(item, str)]

    return normalized


def _convert_discovered_tool(tool_entry: dict[str, Any]) -> DiscoveredTool | None:
    server = tool_entry.get("server")
    tool = tool_entry.get("tool")
    if not isinstance(server, str) or not server:
        return None
    if not isinstance(tool, str) or not tool:
        return None

    openai_name = tool_entry.get("policy_tool")
    if not isinstance(openai_name, str) or not openai_name:
        openai_name = f"mcp.{server}.{tool}"

    description = tool_entry.get("description")
    if not isinstance(description, str) or not description:
        description = f"Invoke MCP tool {server}:{tool}."

    raw_schema = (
        tool_entry.get("input_schema")
        or tool_entry.get("parameters")
        or tool_entry.get("json_schema")
    )

    return DiscoveredTool(
        openai_name=openai_name,
        server=server,
        tool=tool,
        description=description,
        parameters=_normalize_parameters_schema(raw_schema),
    )


def _serialize_tool_discovery_signature(raw_tools: list[dict[str, Any]]) -> str:
    normalized_items = sorted(
        [
            {
                "server": item.get("server"),
                "tool": item.get("tool"),
                "policy_tool": item.get("policy_tool"),
                "input_schema": item.get("input_schema"),
                "parameters": item.get("parameters"),
                "json_schema": item.get("json_schema"),
            }
            for item in raw_tools
        ],
        key=lambda item: (
            str(item.get("server", "")),
            str(item.get("tool", "")),
            str(item.get("policy_tool", "")),
        ),
    )
    return json.dumps(normalized_items, ensure_ascii=False, sort_keys=True)


def discover_openai_tools(
    *,
    socket_path: str,
    agent_id: str,
    fallback_tool_name: str,
    session: AgentSession,
) -> list[dict[str, Any]]:
    fallback_tools = _build_tool_schema(fallback_tool_name)
    try:
        result = call_rpc(
            socket_path,
            "ListAvailableTools",
            {
                "agent_id": agent_id,
            },
        )
    except Exception:
        return fallback_tools

    tools_value = result.get("tools")
    if not isinstance(tools_value, list) or not tools_value:
        session.discovered_tools = []
        session.discovered_signature = ""
        return fallback_tools

    raw_tools = [item for item in tools_value if isinstance(item, dict)]
    signature = _serialize_tool_discovery_signature(raw_tools)
    if signature != session.discovered_signature:
        converted: list[DiscoveredTool] = []
        unique_counter: dict[str, int] = {}
        for raw_tool in raw_tools:
            tool = _convert_discovered_tool(raw_tool)
            if tool is None:
                continue

            duplicate_count = unique_counter.get(tool.openai_name, 0)
            unique_counter[tool.openai_name] = duplicate_count + 1
            if duplicate_count > 0:
                tool.openai_name = f"{tool.openai_name}__{duplicate_count + 1}"

            converted.append(tool)

        session.discovered_tools = converted
        session.discovered_signature = signature

    if not session.discovered_tools:
        return fallback_tools

    return [
        {
            "type": "function",
            "function": {
                "name": tool.openai_name,
                "description": tool.description,
                "parameters": tool.parameters,
            },
        }
        for tool in session.discovered_tools
    ]


def _resolve_discovered_tool(
    session: AgentSession, openai_name: str
) -> DiscoveredTool | None:
    for tool in session.discovered_tools:
        if tool.openai_name == openai_name:
            return tool
    return None


def _tool_output_to_text(value: Any) -> str:
    if isinstance(value, str):
        return value
    return json.dumps(value, ensure_ascii=False)


def _invoke_real_chat_once(
    *,
    base_url: str,
    api_key: str,
    model: str,
    timeout: int,
    messages: list[dict[str, Any]],
    tool_name: str | None = None,
    tools: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    _ensure_openai_types()
    if OpenAI is None:
        raise RuntimeError("openai client unavailable")

    request_tools = tools
    if request_tools is None:
        fallback_tool = (
            tool_name
            if isinstance(tool_name, str) and tool_name
            else "builtin.lite.echo"
        )
        request_tools = _build_tool_schema(fallback_tool)

    client = OpenAI(base_url=base_url, api_key=api_key, timeout=timeout)
    raw_response = client.chat.completions.with_raw_response.create(
        model=model,
        messages=messages,
        tools=request_tools,
    )
    completion = raw_response.parse()
    response_text = _extract_message_text(completion)
    tool_calls = _extract_tool_calls(completion)
    if not response_text and not tool_calls:
        raise RuntimeError("provider returned empty assistant content")

    prompt_text_parts: list[str] = []
    for message in messages:
        content = message.get("content")
        if isinstance(content, str) and content:
            prompt_text_parts.append(content)
    prompt_text = "\n".join(prompt_text_parts)

    usage = getattr(completion, "usage", None)
    prompt_tokens = getattr(usage, "prompt_tokens", None)
    completion_tokens = getattr(usage, "completion_tokens", None)
    total_tokens = getattr(usage, "total_tokens", None)

    usage_source = "provider"
    if not isinstance(prompt_tokens, int) or not isinstance(completion_tokens, int):
        prompt_tokens = estimate_tokens(prompt_text)
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
        "tool_calls": tool_calls,
    }


def _is_retryable_error(err: Exception) -> bool:
    if APIConnectionError is not None and isinstance(err, APIConnectionError):
        return True

    if APIStatusError is not None and isinstance(err, APIStatusError):
        status_code = getattr(err, "status_code", None)
        if isinstance(status_code, int) and (status_code == 429 or status_code >= 500):
            return True

    return False


def _invoke_real_with_retry(
    *,
    base_url: str,
    api_key: str,
    model: str,
    timeout: int,
    messages: list[dict[str, Any]],
    tool_name: str | None = None,
    tools: list[dict[str, Any]] | None = None,
    max_retries: int,
) -> dict[str, Any]:
    attempt = 0
    while True:
        try:
            return _invoke_real_chat_once(
                base_url=base_url,
                api_key=api_key,
                model=model,
                timeout=timeout,
                messages=messages,
                tool_name=tool_name,
                tools=tools,
            )
        except Exception as err:
            if not _is_retryable_error(err):
                raise

            if attempt >= max_retries:
                raise RetryExhaustedError(attempts=attempt + 1, last_error=err) from err

            delay = 0.2 * (2**attempt)
            time.sleep(delay)
            attempt += 1


def _classify_llm_error(err: Exception) -> tuple[str, str]:
    auth_error = AuthenticationError
    timeout_error = APITimeoutError
    conn_error = APIConnectionError
    status_error = APIStatusError

    if auth_error is not None and isinstance(err, auth_error):
        return "provider.auth", "AUTH"

    if timeout_error is not None and isinstance(err, timeout_error):
        return "provider.timeout", "TIMEOUT"

    if conn_error is not None and isinstance(err, conn_error):
        message = str(err).lower()
        if "timeout" in message or "timed out" in message:
            return "provider.timeout", "TIMEOUT"
        return "provider.network", "NETWORK"

    if status_error is not None and isinstance(err, status_error):
        status_code = getattr(err, "status_code", None)
        if status_code == 401:
            return "provider.auth", "AUTH"
        if status_code == 429:
            return "provider.rate_limit", "RATE_LIMIT"
        if isinstance(status_code, int) and status_code >= 500:
            return "provider.network", "NETWORK"
        return "provider.http", "UNKNOWN"

    return "provider.unknown", "UNKNOWN"


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
    parser.add_argument(
        "--max-iterations",
        type=int,
        default=5,
        help="Maximum tool-calling iterations",
    )
    parser.add_argument(
        "--max-retries",
        type=int,
        default=1,
        help="Maximum retries for retryable provider errors",
    )
    return parser.parse_args()


def run_once(args: argparse.Namespace) -> int:
    max_iterations = max(1, int(getattr(args, "max_iterations", 5)))
    max_retries = max(0, int(getattr(args, "max_retries", 1)))

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
                        "provider_call_attempted": False,
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
                    "provider_call_attempted": False,
                },
                ensure_ascii=False,
            )
        )
        return 2

    legacy_tool_output = run_builtin_tool(args.tool, args.prompt)
    session = AgentSession(args.agent_id)
    messages: list[dict[str, Any]] = [{"role": "user", "content": args.prompt}]
    tool_call_records: list[dict[str, Any]] = []

    accumulated_input_tokens = 0
    accumulated_output_tokens = 0
    accumulated_total_tokens = 0
    last_provider_request_id = ""
    last_request_id_source = "unavailable"
    last_provider_model = llm_config.model
    last_usage_source = "estimated"
    last_transport_mode = "real"

    response_text = ""
    provider_call_attempted = False

    try:
        for _ in range(max_iterations):
            provider_call_attempted = True
            openai_tools = discover_openai_tools(
                socket_path=args.socket_path,
                agent_id=args.agent_id,
                fallback_tool_name=args.tool,
                session=session,
            )
            llm_result = _invoke_real_with_retry(
                base_url=llm_config.base_url,
                api_key=llm_config.api_key,
                model=llm_config.model,
                timeout=llm_config.timeout,
                messages=messages,
                tools=openai_tools,
                max_retries=max_retries,
            )

            accumulated_input_tokens += int(llm_result["input_tokens"])
            accumulated_output_tokens += int(llm_result["output_tokens"])
            accumulated_total_tokens += int(llm_result["total_tokens"])

            provider_request_id = llm_result.get("provider_request_id")
            if isinstance(provider_request_id, str):
                last_provider_request_id = provider_request_id
            request_id_source = llm_result.get("request_id_source")
            if isinstance(request_id_source, str):
                last_request_id_source = request_id_source
            provider_model = llm_result.get("provider_model")
            if isinstance(provider_model, str):
                last_provider_model = provider_model
            usage_source = llm_result.get("usage_source")
            if isinstance(usage_source, str):
                last_usage_source = usage_source
            transport_mode = llm_result.get("transport_mode")
            if isinstance(transport_mode, str):
                last_transport_mode = transport_mode

            response_text = str(llm_result.get("output", ""))
            tool_calls = llm_result.get("tool_calls")
            if not isinstance(tool_calls, list) or not tool_calls:
                break

            assistant_message: dict[str, Any] = {
                "role": "assistant",
                "content": response_text,
                "tool_calls": [],
            }
            tool_response_messages: list[dict[str, Any]] = []

            for tool_call in tool_calls:
                if not isinstance(tool_call, dict):
                    continue
                call_id = tool_call.get("id")
                tool_name = tool_call.get("name")
                arguments_text = tool_call.get("arguments")

                if (
                    not isinstance(call_id, str)
                    or not call_id
                    or not isinstance(tool_name, str)
                    or not tool_name
                ):
                    continue

                parsed_prompt = args.prompt
                parsed_arguments: Any = {}
                if isinstance(arguments_text, str) and arguments_text:
                    try:
                        parsed_arguments = json.loads(arguments_text)
                        if isinstance(parsed_arguments, dict):
                            prompt_from_args = parsed_arguments.get("prompt")
                            if isinstance(prompt_from_args, str) and prompt_from_args:
                                parsed_prompt = prompt_from_args
                    except json.JSONDecodeError:
                        parsed_prompt = args.prompt
                        parsed_arguments = {}

                try:
                    tool_authorization = call_rpc(
                        args.socket_path,
                        "AuthorizeTool",
                        {
                            "tool": tool_name,
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
                                    "tool": tool_name,
                                    "error": "policy.deny",
                                    "code": err.code,
                                    "message": err.message,
                                    "provider_call_attempted": provider_call_attempted,
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
                                "tool": tool_name,
                                "code": err.code,
                                "message": err.message,
                                "provider_call_attempted": provider_call_attempted,
                            },
                            ensure_ascii=False,
                        )
                    )
                    return 1
                tool_decision = str(tool_authorization.get("decision", "ask"))
                if tool_decision == "deny":
                    print(
                        json.dumps(
                            {
                                "status": "blocked",
                                "agent_id": args.agent_id,
                                "tool": tool_name,
                                "error": "policy.deny",
                                "message": "tool denied by policy engine",
                                "provider_call_attempted": provider_call_attempted,
                            },
                            ensure_ascii=False,
                        )
                    )
                    return 2

                discovered_tool = _resolve_discovered_tool(session, tool_name)
                if discovered_tool is not None:
                    try:
                        invoke_result = call_rpc(
                            args.socket_path,
                            "InvokeSkill",
                            {
                                "agent_id": args.agent_id,
                                "server": discovered_tool.server,
                                "tool": discovered_tool.tool,
                                "args": parsed_arguments,
                            },
                        )
                    except RpcError as err:
                        if err.code == -32016:
                            print(
                                json.dumps(
                                    {
                                        "status": "blocked",
                                        "agent_id": args.agent_id,
                                        "tool": tool_name,
                                        "error": "policy.deny",
                                        "code": err.code,
                                        "message": err.message,
                                        "provider_call_attempted": provider_call_attempted,
                                    },
                                    ensure_ascii=False,
                                )
                            )
                            return 2

                        print(
                            json.dumps(
                                {
                                    "status": "failed",
                                    "stage": "invoke_skill",
                                    "tool": tool_name,
                                    "code": err.code,
                                    "message": err.message,
                                    "provider_call_attempted": provider_call_attempted,
                                },
                                ensure_ascii=False,
                            )
                        )
                        return 1
                    tool_output = _tool_output_to_text(invoke_result)
                    tool_output_record: Any = invoke_result
                else:
                    tool_output = run_builtin_tool(tool_name, parsed_prompt)
                    tool_output_record = tool_output

                tool_call_records.append(
                    {
                        "id": call_id,
                        "name": tool_name,
                        "arguments": arguments_text
                        if isinstance(arguments_text, str)
                        else "{}",
                        "decision": tool_decision,
                        "input": parsed_prompt,
                        "output": tool_output_record,
                    }
                )

                assistant_message["tool_calls"].append(
                    {
                        "id": call_id,
                        "type": "function",
                        "function": {
                            "name": tool_name,
                            "arguments": arguments_text
                            if isinstance(arguments_text, str)
                            else "{}",
                        },
                    }
                )
                tool_response_messages.append(
                    {
                        "role": "tool",
                        "tool_call_id": call_id,
                        "content": tool_output,
                    }
                )

            messages.append(assistant_message)
            messages.extend(tool_response_messages)
        else:
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "llm",
                        "error": "MAX_ITERATIONS_REACHED",
                        "message": "tool-calling loop exceeded max_iterations",
                        "max_iterations": max_iterations,
                        "provider_call_attempted": provider_call_attempted,
                    },
                    ensure_ascii=False,
                )
            )
            return 1
    except Exception as err:
        attempts = 1
        classified_error = err
        if isinstance(err, RetryExhaustedError):
            attempts = err.attempts
            classified_error = err.last_error

        error_code, error_category = _classify_llm_error(classified_error)
        print(
            json.dumps(
                {
                    "status": "failed",
                    "stage": "llm",
                    "error": error_code,
                    "error_category": error_category,
                    "message": str(classified_error),
                    "provider_request_id": getattr(
                        classified_error, "request_id", None
                    ),
                    "transport_mode": "real",
                    "provider_call_attempted": provider_call_attempted,
                    "attempts": attempts,
                    "max_retries": max_retries,
                },
                ensure_ascii=False,
            )
        )
        return 1

    input_tokens = accumulated_input_tokens
    output_tokens = accumulated_output_tokens
    total_tokens = accumulated_total_tokens
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
                "provider_request_id": last_provider_request_id,
                "usage_source": last_usage_source,
                "transport_mode": last_transport_mode,
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
                    "output": legacy_tool_output,
                    "calls": tool_call_records,
                },
                "llm": {
                    "output": response_text,
                    "input_tokens": input_tokens,
                    "output_tokens": output_tokens,
                    "total_tokens": total_tokens,
                    "estimated_cost_usd": cost_usd,
                    "provider_request_id": last_provider_request_id,
                    "request_id_source": last_request_id_source,
                    "provider_model": last_provider_model,
                    "usage_source": last_usage_source,
                    "transport_mode": last_transport_mode,
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
