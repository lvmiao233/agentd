import argparse
import importlib
import json
import re
import socket
import sys
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

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
    policy_name: str
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
        self.provider_tool_name_map: dict[str, str] = {}

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

    def _refresh_context_window_tokens(self) -> int:
        total = 0
        for message in self._get_active_branch():
            content = message.get("content")
            if isinstance(content, str) and content:
                total += estimate_tokens(content)
            tool_calls = message.get("tool_calls")
            if isinstance(tool_calls, list) and tool_calls:
                total += estimate_tokens(json.dumps(tool_calls, ensure_ascii=False))
        self.context_window_tokens = total
        return total

    def _compact_context(self) -> None:
        branch = self._get_active_branch()
        if not branch:
            return

        keep_tail_size = min(4, len(branch))
        keep_tail = branch[-keep_tail_size:]
        summary_source = branch[:-keep_tail_size]
        if not summary_source and len(branch) > 1:
            keep_tail = branch[-1:]
            summary_source = branch[:-1]

        summary_text = self._build_compact_summary(summary_source)
        previous_head_id = self.head_id

        compact_root: dict[str, Any] = {
            "id": str(uuid.uuid4()),
            "parent_id": None,
            "role": "system",
            "content": summary_text,
            "compact": {
                "kind": "auto_compact_summary",
                "source_head_id": previous_head_id,
            },
        }
        self.messages.append(compact_root)
        new_head_id = str(compact_root["id"])

        for original in keep_tail:
            role = original.get("role")
            if not isinstance(role, str):
                continue

            content = original.get("content")
            if not isinstance(content, str):
                content = ""

            copied: dict[str, Any] = {
                "id": str(uuid.uuid4()),
                "parent_id": new_head_id,
                "role": role,
                "content": content,
            }

            tool_calls = original.get("tool_calls")
            if isinstance(tool_calls, list):
                copied["tool_calls"] = tool_calls

            tool_call_id = original.get("tool_call_id")
            if isinstance(tool_call_id, str) and tool_call_id:
                copied["tool_call_id"] = tool_call_id

            self.messages.append(copied)
            new_head_id = str(copied["id"])

        self.head_id = new_head_id
        self._append_message(
            "system",
            "context budget threshold reached, compact hook triggered with summary backfill",
        )
        self._refresh_context_window_tokens()

    def _build_compact_summary(self, messages: list[dict[str, Any]]) -> str:
        if not messages:
            return "context summary: no prior messages to compact"

        facts: list[str] = []
        seen: set[tuple[str, str]] = set()

        for item in messages:
            role = item.get("role")
            if not isinstance(role, str):
                continue

            content = item.get("content")
            if not isinstance(content, str):
                continue

            normalized = " ".join(content.strip().split())
            if not normalized:
                continue

            if len(normalized) > 180:
                normalized = f"{normalized[:177]}..."

            key = (role, normalized)
            if key in seen:
                continue
            seen.add(key)

            facts.append(f"- {role}: {normalized}")
            if len(facts) >= 8:
                break

        if not facts:
            facts.append("- no key facts extracted")

        return "context summary (auto-compact):\n" + "\n".join(facts)

    def _maybe_trigger_compact(self) -> bool:
        if self.max_context_tokens <= 0:
            return False

        self._refresh_context_window_tokens()
        threshold = max(1, int(self.max_context_tokens * 0.8))
        if self.context_window_tokens <= threshold:
            return False

        self._compact_context()
        return True

    def chat(
        self, user_input: str, *, run_turn: Callable[[], dict[str, Any]]
    ) -> dict[str, Any]:
        self._append_message("user", user_input)
        compact_triggered = self._maybe_trigger_compact()
        result = run_turn()
        result["compact_triggered"] = compact_triggered
        result["context_window_tokens"] = self.context_window_tokens
        return result


def _normalize_loaded_message(raw_message: dict[str, Any]) -> dict[str, Any]:
    message_id = raw_message.get("id")
    if not isinstance(message_id, str) or not message_id:
        raise ValueError("invalid session message: id is required")

    parent_id = raw_message.get("parent_id")
    if parent_id is not None and not isinstance(parent_id, str):
        raise ValueError("invalid session message: parent_id must be null or string")

    role = raw_message.get("role")
    if not isinstance(role, str) or not role:
        raise ValueError("invalid session message: role is required")

    content = raw_message.get("content")
    if not isinstance(content, str):
        content = ""

    normalized: dict[str, Any] = {
        "id": message_id,
        "parent_id": parent_id,
        "role": role,
        "content": content,
    }

    tool_calls = raw_message.get("tool_calls")
    if isinstance(tool_calls, list):
        normalized["tool_calls"] = tool_calls

    tool_call_id = raw_message.get("tool_call_id")
    if isinstance(tool_call_id, str) and tool_call_id:
        normalized["tool_call_id"] = tool_call_id

    compact = raw_message.get("compact")
    if isinstance(compact, dict):
        normalized["compact"] = compact

    return normalized


def save_session_jsonl(session: AgentSession, file_path: str) -> None:
    target = Path(file_path)
    try:
        if target.parent != Path(""):
            target.parent.mkdir(parents=True, exist_ok=True)

        with target.open("w", encoding="utf-8") as handle:
            metadata = {
                "kind": "session",
                "agent_id": session.agent_id,
                "head_id": session.head_id,
                "max_context_tokens": session.max_context_tokens,
                "tool_results_cache": session.tool_results_cache,
            }
            handle.write(json.dumps(metadata, ensure_ascii=False) + "\n")
            for message in session.messages:
                record = {"kind": "message", **message}
                handle.write(json.dumps(record, ensure_ascii=False) + "\n")
    except OSError as err:
        raise ValueError(f"session save failed: {err}") from err


def load_session_jsonl(
    file_path: str,
    *,
    agent_id: str | None = None,
    max_context_tokens: int = 0,
) -> AgentSession:
    source = Path(file_path)
    try:
        lines = source.read_text(encoding="utf-8").splitlines()
    except OSError as err:
        raise ValueError(f"session load failed: {err}") from err

    metadata: dict[str, Any] = {}
    loaded_messages: list[dict[str, Any]] = []

    for line_number, line in enumerate(lines, start=1):
        stripped = line.strip()
        if not stripped:
            continue

        try:
            record = json.loads(stripped)
        except json.JSONDecodeError as err:
            raise ValueError(
                f"session parse error at line {line_number}: {err.msg}"
            ) from err

        if not isinstance(record, dict):
            raise ValueError(
                f"session parse error at line {line_number}: invalid object"
            )

        kind = record.get("kind")
        if kind == "session":
            metadata = record
            continue
        if kind == "message":
            loaded_messages.append(_normalize_loaded_message(record))
            continue

        loaded_messages.append(_normalize_loaded_message(record))

    loaded_agent_id = metadata.get("agent_id")
    if not isinstance(loaded_agent_id, str) or not loaded_agent_id:
        loaded_agent_id = agent_id
    if not isinstance(loaded_agent_id, str) or not loaded_agent_id:
        raise ValueError("session load failed: agent_id is required")

    metadata_max_context = metadata.get("max_context_tokens")
    if not isinstance(metadata_max_context, int) or metadata_max_context < 0:
        metadata_max_context = 0

    effective_max_context_tokens = metadata_max_context
    if max_context_tokens > 0:
        effective_max_context_tokens = max_context_tokens

    session = AgentSession(
        loaded_agent_id,
        max_context_tokens=effective_max_context_tokens,
    )
    session.messages = loaded_messages

    message_ids = {
        item["id"] for item in loaded_messages if isinstance(item.get("id"), str)
    }
    metadata_head_id = metadata.get("head_id")
    if (
        isinstance(metadata_head_id, str)
        and metadata_head_id
        and metadata_head_id in message_ids
    ):
        session.head_id = metadata_head_id
    elif loaded_messages:
        session.head_id = loaded_messages[-1]["id"]

    tool_results_cache = metadata.get("tool_results_cache")
    if isinstance(tool_results_cache, dict):
        session.tool_results_cache = tool_results_cache

    session._refresh_context_window_tokens()
    return session


def run_session_command(
    *,
    command: str,
    file_path: str,
    session: AgentSession | None = None,
    agent_id: str | None = None,
    max_context_tokens: int = 0,
) -> AgentSession:
    normalized_command = command.strip().lower()
    if normalized_command == "save":
        if session is None:
            raise ValueError("session save failed: session is required")
        save_session_jsonl(session, file_path)
        return session

    if normalized_command == "load":
        return load_session_jsonl(
            file_path,
            agent_id=agent_id,
            max_context_tokens=max_context_tokens,
        )

    raise ValueError(f"unsupported session command: {command}")


def build_migration_summary(
    session: AgentSession, *, key_files: list[str] | None = None
) -> dict[str, Any]:
    branch = session._get_active_branch()
    summary_text = session._build_compact_summary(branch)
    normalized_key_files = [
        item for item in (key_files or []) if isinstance(item, str) and item
    ]
    return {
        "text": summary_text,
        "key_files": normalized_key_files,
        "message_count": len(branch),
        "source_head_id": session.head_id,
    }


def export_session_snapshot(
    session: AgentSession, *, working_directory: dict[str, str] | None = None
) -> dict[str, Any]:
    normalized_working_directory: dict[str, str] = {}
    if isinstance(working_directory, dict):
        for key, value in working_directory.items():
            if isinstance(key, str) and key and isinstance(value, str):
                normalized_working_directory[key] = value

    return {
        "agent_id": session.agent_id,
        "head_id": session.head_id,
        "messages": [dict(message) for message in session.messages],
        "tool_results_cache": dict(session.tool_results_cache),
        "working_directory": normalized_working_directory,
    }


def restore_session_from_snapshot(
    snapshot: dict[str, Any], *, max_context_tokens: int = 0
) -> AgentSession:
    if not isinstance(snapshot, dict):
        raise ValueError("snapshot restore failed: snapshot must be an object")

    loaded_agent_id = snapshot.get("agent_id")
    if not isinstance(loaded_agent_id, str) or not loaded_agent_id:
        raise ValueError("snapshot restore failed: agent_id is required")

    session = AgentSession(
        loaded_agent_id, max_context_tokens=max(0, max_context_tokens)
    )

    raw_messages = snapshot.get("messages")
    if not isinstance(raw_messages, list):
        raise ValueError("snapshot restore failed: messages must be an array")

    loaded_messages: list[dict[str, Any]] = []
    for item in raw_messages:
        if not isinstance(item, dict):
            raise ValueError("snapshot restore failed: invalid message entry")
        loaded_messages.append(_normalize_loaded_message(item))
    session.messages = loaded_messages

    loaded_head_id = snapshot.get("head_id")
    valid_ids = {
        item["id"] for item in loaded_messages if isinstance(item.get("id"), str)
    }
    if isinstance(loaded_head_id, str) and loaded_head_id in valid_ids:
        session.head_id = loaded_head_id
    elif loaded_messages:
        session.head_id = loaded_messages[-1]["id"]

    tool_results_cache = snapshot.get("tool_results_cache")
    if isinstance(tool_results_cache, dict):
        session.tool_results_cache = tool_results_cache

    session._refresh_context_window_tokens()
    return session


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


def _provider_safe_tool_name(policy_tool_name: str) -> str:
    sanitized = re.sub(r"[^a-zA-Z0-9_-]", "_", policy_tool_name)
    sanitized = sanitized.strip("_")
    if not sanitized:
        return "tool"
    if len(sanitized) > 64:
        sanitized = sanitized[:64]
    return sanitized


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

    policy_name = tool_entry.get("policy_tool")
    if not isinstance(policy_name, str) or not policy_name:
        policy_name = f"mcp.{server}.{tool}"

    openai_name = policy_name

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
        policy_name=policy_name,
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
    session.provider_tool_name_map = {fallback_tool_name: fallback_tool_name}
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
        session.provider_tool_name_map = {fallback_tool_name: fallback_tool_name}
        return fallback_tools

    raw_tools = [item for item in tools_value if isinstance(item, dict)]
    signature = _serialize_tool_discovery_signature(raw_tools)
    if signature != session.discovered_signature:
        converted: list[DiscoveredTool] = []
        unique_counter: dict[str, int] = {}
        provider_tool_name_map: dict[str, str] = {}
        for raw_tool in raw_tools:
            tool = _convert_discovered_tool(raw_tool)
            if tool is None:
                continue

            duplicate_count = unique_counter.get(tool.openai_name, 0)
            unique_counter[tool.openai_name] = duplicate_count + 1
            if duplicate_count > 0:
                tool.openai_name = f"{tool.openai_name}__{duplicate_count + 1}"

            provider_tool_name_map[tool.openai_name] = tool.policy_name
            converted.append(tool)

        session.discovered_tools = converted
        session.discovered_signature = signature
        if provider_tool_name_map:
            session.provider_tool_name_map = provider_tool_name_map
        else:
            session.provider_tool_name_map = {fallback_tool_name: fallback_tool_name}

    if not session.discovered_tools:
        session.provider_tool_name_map = {fallback_tool_name: fallback_tool_name}
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


def _extract_policy_tool_names(raw_tools: list[dict[str, Any]]) -> list[str]:
    names: list[str] = []
    for item in raw_tools:
        policy_tool = item.get("policy_tool")
        if isinstance(policy_tool, str) and policy_tool:
            names.append(policy_tool)
            continue

        tool_name = item.get("tool")
        if isinstance(tool_name, str) and tool_name:
            if tool_name.startswith("mcp."):
                names.append(tool_name)
            else:
                names.append(f"mcp.{tool_name}")

    return sorted(set(names))


def build_cross_language_contract_matrix(
    *,
    daemon_tools: list[dict[str, Any]],
    openai_tools: list[dict[str, Any]],
) -> dict[str, Any]:
    daemon_names = _extract_policy_tool_names(daemon_tools)

    openai_names: list[str] = []
    for item in openai_tools:
        if not isinstance(item, dict):
            continue
        function = item.get("function")
        if not isinstance(function, dict):
            continue
        name = function.get("name")
        if isinstance(name, str) and name:
            openai_names.append(name)

    normalized_openai_names = sorted(set(openai_names))
    missing_in_agent_lite = [
        name for name in daemon_names if name not in normalized_openai_names
    ]

    return {
        "daemon_to_agent_lite": {
            "status": "compatible" if not missing_in_agent_lite else "incompatible",
            "daemon_tools": daemon_names,
            "agent_lite_tools": normalized_openai_names,
            "missing_in_agent_lite": missing_in_agent_lite,
        },
        "agent_lite_to_web": {
            "status": "compatible",
            "required_fields": ["name", "description", "parameters"],
            "render_contract": "web settings and dashboard can render tool metadata",
        },
        "daemon_to_web": {
            "status": "compatible",
            "required_rpc": [
                "ListAvailableTools",
                "InvokeSkill",
                "OnboardMcpServer",
                "ListMcpServers",
            ],
        },
    }


def onboard_third_party_mcp_server(
    *,
    socket_path: str,
    agent_id: str,
    name: str,
    command: str,
    args: list[str] | None = None,
    transport: str = "stdio",
    trust_level: str = "community",
) -> dict[str, Any]:
    request_payload = {
        "name": name,
        "command": command,
        "args": [item for item in (args or []) if isinstance(item, str)],
        "transport": transport,
        "trust_level": trust_level,
    }

    onboarding_result: dict[str, Any] = {}
    onboarding_error: dict[str, Any] | None = None
    try:
        onboarding_result = call_rpc(socket_path, "OnboardMcpServer", request_payload)
    except RpcError as err:
        onboarding_error = {"code": err.code, "message": err.message}
    except Exception as err:
        onboarding_error = {"code": -1, "message": str(err)}

    tools_result: dict[str, Any] = {}
    tools_error: dict[str, Any] | None = None
    available_tools: list[dict[str, Any]] = []
    try:
        tools_result = call_rpc(
            socket_path,
            "ListAvailableTools",
            {
                "agent_id": agent_id,
            },
        )
        tools_value = tools_result.get("tools")
        if isinstance(tools_value, list):
            available_tools = [item for item in tools_value if isinstance(item, dict)]
    except RpcError as err:
        tools_error = {"code": err.code, "message": err.message}
    except Exception as err:
        tools_error = {"code": -1, "message": str(err)}

    openai_tools: list[dict[str, Any]] = []
    for tool_entry in available_tools:
        converted = _convert_discovered_tool(tool_entry)
        if converted is None:
            continue
        openai_tools.append(
            {
                "type": "function",
                "function": {
                    "name": converted.openai_name,
                    "description": converted.description,
                    "parameters": converted.parameters,
                },
            }
        )

    builtin_servers = {"mcp-fs", "mcp-search", "mcp-shell", "mcp-git"}
    builtin_tools_intact = any(
        isinstance(tool.get("server"), str) and tool.get("server") in builtin_servers
        for tool in available_tools
    )

    return {
        "status": "onboarded" if onboarding_error is None else "failed",
        "request": request_payload,
        "onboarding": onboarding_result,
        "onboarding_error": onboarding_error,
        "tools": available_tools,
        "tools_error": tools_error,
        "builtin_tools_intact": builtin_tools_intact,
        "contract_matrix": build_cross_language_contract_matrix(
            daemon_tools=available_tools,
            openai_tools=openai_tools,
        ),
    }


def _resolve_discovered_tool(
    session: AgentSession, openai_name: str
) -> DiscoveredTool | None:
    for tool in session.discovered_tools:
        if tool.openai_name == openai_name:
            return tool
    return None


def _resolve_policy_tool_name(session: AgentSession, provider_name: str) -> str:
    return session.provider_tool_name_map.get(provider_name, provider_name)


def _prepare_provider_tools(
    request_tools: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], dict[str, str]]:
    provider_tools: list[dict[str, Any]] = []
    provider_to_policy: dict[str, str] = {}
    name_counts: dict[str, int] = {}

    for tool in request_tools:
        if not isinstance(tool, dict):
            continue
        function = tool.get("function")
        if not isinstance(function, dict):
            continue
        policy_name = function.get("name")
        if not isinstance(policy_name, str) or not policy_name:
            continue

        provider_name = _provider_safe_tool_name(policy_name)
        duplicate_count = name_counts.get(provider_name, 0)
        name_counts[provider_name] = duplicate_count + 1
        if duplicate_count > 0:
            provider_name = f"{provider_name}__{duplicate_count + 1}"

        transformed_function = dict(function)
        transformed_function["name"] = provider_name
        transformed_tool = dict(tool)
        transformed_tool["function"] = transformed_function

        provider_tools.append(transformed_tool)
        provider_to_policy[provider_name] = policy_name

    if not provider_tools:
        return request_tools, {}

    return provider_tools, provider_to_policy


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

    provider_tools, provider_to_policy = _prepare_provider_tools(request_tools)

    client = OpenAI(base_url=base_url, api_key=api_key, timeout=timeout)
    raw_response = client.chat.completions.with_raw_response.create(
        model=model,
        messages=messages,
        tools=provider_tools,
    )
    completion = raw_response.parse()
    response_text = _extract_message_text(completion)
    tool_calls = _extract_tool_calls(completion)
    if provider_to_policy and isinstance(tool_calls, list):
        for tool_call in tool_calls:
            if not isinstance(tool_call, dict):
                continue
            provider_name = tool_call.get("name")
            if not isinstance(provider_name, str):
                continue
            mapped = provider_to_policy.get(provider_name)
            if isinstance(mapped, str) and mapped:
                tool_call["name"] = mapped
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


def _provider_messages_from_session(session: AgentSession) -> list[dict[str, Any]]:
    messages: list[dict[str, Any]] = []
    for item in session._get_active_branch():
        role = item.get("role")
        content = item.get("content")
        if not isinstance(role, str):
            continue
        if not isinstance(content, str):
            content = ""

        normalized: dict[str, Any] = {
            "role": role,
            "content": content,
        }
        tool_calls = item.get("tool_calls")
        if role == "assistant" and isinstance(tool_calls, list) and tool_calls:
            normalized["tool_calls"] = tool_calls
        tool_call_id = item.get("tool_call_id")
        if role == "tool" and isinstance(tool_call_id, str) and tool_call_id:
            normalized["tool_call_id"] = tool_call_id

        messages.append(normalized)

    return messages


def _run_chat_turn(
    *,
    args: argparse.Namespace,
    llm_config: Any,
    session: AgentSession,
    max_iterations: int,
    max_retries: int,
) -> dict[str, Any]:
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

    for _ in range(max_iterations):
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
            messages=_provider_messages_from_session(session),
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
            assistant_message = session._append_message("assistant", response_text)
            assistant_message["tool_calls"] = []
            session._refresh_context_window_tokens()
            break

        assistant_message = session._append_message("assistant", response_text)
        assistant_message["tool_calls"] = []

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

            policy_tool_name = _resolve_policy_tool_name(session, tool_name)

            parsed_prompt = ""
            parsed_arguments: Any = {}
            if isinstance(arguments_text, str) and arguments_text:
                try:
                    parsed_arguments = json.loads(arguments_text)
                except json.JSONDecodeError:
                    parsed_arguments = {}
            if isinstance(parsed_arguments, dict):
                prompt_from_args = parsed_arguments.get("prompt")
                if isinstance(prompt_from_args, str) and prompt_from_args:
                    parsed_prompt = prompt_from_args
            if not parsed_prompt:
                parsed_prompt = args.prompt

            tool_authorization = call_rpc(
                args.socket_path,
                "AuthorizeTool",
                {
                    "tool": policy_tool_name,
                    "agent_id": args.agent_id,
                },
            )
            tool_decision = str(tool_authorization.get("decision", "ask"))
            if tool_decision == "deny":
                raise RpcError(-32016, "policy.deny: tool blocked")
            if tool_decision == "ask":
                raise RpcError(-32024, "policy.ask: approval required")

            discovered_tool = _resolve_discovered_tool(session, tool_name)
            if discovered_tool is not None:
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
                tool_output = _tool_output_to_text(invoke_result)
                tool_output_record: Any = invoke_result
            else:
                tool_output = run_builtin_tool(policy_tool_name, parsed_prompt)
                tool_output_record = tool_output

            tool_call_records.append(
                {
                    "id": call_id,
                    "name": policy_tool_name,
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

            tool_message = session._append_message("tool", tool_output)
            tool_message["tool_call_id"] = call_id
            session.tool_results_cache[call_id] = tool_output_record

        session._refresh_context_window_tokens()
    else:
        raise RuntimeError("MAX_ITERATIONS_REACHED")

    return {
        "output": response_text,
        "tool_calls": tool_call_records,
        "input_tokens": accumulated_input_tokens,
        "output_tokens": accumulated_output_tokens,
        "total_tokens": accumulated_total_tokens,
        "provider_request_id": last_provider_request_id,
        "request_id_source": last_request_id_source,
        "provider_model": last_provider_model,
        "usage_source": last_usage_source,
        "transport_mode": last_transport_mode,
    }


def run_chat(
    *,
    args: argparse.Namespace,
    llm_config: Any,
    session: AgentSession,
    user_input: str,
    max_iterations: int,
    max_retries: int,
) -> dict[str, Any]:
    return session.chat(
        user_input,
        run_turn=lambda: _run_chat_turn(
            args=args,
            llm_config=llm_config,
            session=session,
            max_iterations=max_iterations,
            max_retries=max_retries,
        ),
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog="agentd-agent-lite")
    parser.add_argument(
        "--mode",
        choices=("chat", "onboard-mcp"),
        default="chat",
        help="Execution mode: normal chat loop or third-party MCP onboarding",
    )
    parser.add_argument("--socket-path", default="/tmp/agentd.sock")
    parser.add_argument("--agent-id", required=True)
    parser.add_argument("--prompt", default=None)
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
    parser.add_argument(
        "--max-context-tokens",
        type=int,
        default=0,
        help="Session context token budget, 0 means disabled",
    )
    parser.add_argument(
        "--session-load",
        default=None,
        help="Load session JSONL before running the turn",
    )
    parser.add_argument(
        "--session-save",
        default=None,
        help="Save session JSONL after completing the turn",
    )
    parser.add_argument(
        "--onboard-name",
        default=None,
        help="Third-party MCP server name when --mode=onboard-mcp",
    )
    parser.add_argument(
        "--onboard-command",
        default=None,
        help="Third-party MCP launch command when --mode=onboard-mcp",
    )
    parser.add_argument(
        "--onboard-arg",
        action="append",
        default=None,
        help="Repeatable MCP launch argument when --mode=onboard-mcp",
    )
    parser.add_argument(
        "--onboard-transport",
        default="stdio",
        help="MCP transport for onboarding mode",
    )
    parser.add_argument(
        "--onboard-trust-level",
        default="community",
        help="Trust level for onboarding mode",
    )
    return parser.parse_args()


def run_once(args: argparse.Namespace) -> int:
    mode = str(getattr(args, "mode", "chat"))
    if mode == "onboard-mcp":
        onboard_name = getattr(args, "onboard_name", None)
        onboard_command = getattr(args, "onboard_command", None)
        if not isinstance(onboard_name, str) or not onboard_name.strip():
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "onboard_mcp",
                        "error": "invalid_onboard_request",
                        "message": "--onboard-name is required when --mode=onboard-mcp",
                    },
                    ensure_ascii=False,
                )
            )
            return 1
        if not isinstance(onboard_command, str) or not onboard_command.strip():
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "onboard_mcp",
                        "error": "invalid_onboard_request",
                        "message": "--onboard-command is required when --mode=onboard-mcp",
                    },
                    ensure_ascii=False,
                )
            )
            return 1

        result = onboard_third_party_mcp_server(
            socket_path=args.socket_path,
            agent_id=args.agent_id,
            name=onboard_name.strip(),
            command=onboard_command.strip(),
            args=[
                item
                for item in (getattr(args, "onboard_arg", None) or [])
                if isinstance(item, str) and item.strip()
            ],
            transport=str(getattr(args, "onboard_transport", "stdio")),
            trust_level=str(getattr(args, "onboard_trust_level", "community")),
        )
        print(json.dumps(result, ensure_ascii=False))
        return 0 if result.get("status") == "onboarded" else 1

    prompt = getattr(args, "prompt", None)
    if not isinstance(prompt, str) or not prompt.strip():
        print(
            json.dumps(
                {
                    "status": "failed",
                    "stage": "input",
                    "error": "missing_prompt",
                    "message": "--prompt is required when --mode=chat",
                },
                ensure_ascii=False,
            )
        )
        return 1

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

    max_context_tokens = max(0, int(getattr(args, "max_context_tokens", 0)))

    session: AgentSession
    session_load_path = getattr(args, "session_load", None)
    if isinstance(session_load_path, str) and session_load_path:
        try:
            session = run_session_command(
                command="load",
                file_path=session_load_path,
                agent_id=args.agent_id,
                max_context_tokens=max_context_tokens,
            )
        except ValueError as err:
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "session_load",
                        "error": "invalid_session",
                        "message": str(err),
                    },
                    ensure_ascii=False,
                )
            )
            return 1
    else:
        session = AgentSession(
            args.agent_id,
            max_context_tokens=max_context_tokens,
        )

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
    if decision == "ask":
        print(
            json.dumps(
                {
                    "status": "blocked",
                    "agent_id": args.agent_id,
                    "tool": args.tool,
                    "error": "policy.ask",
                    "message": "tool requires explicit approval",
                    "provider_call_attempted": False,
                },
                ensure_ascii=False,
            )
        )
        return 2

    legacy_tool_output = run_builtin_tool(args.tool, args.prompt)

    response_text = ""
    tool_call_records: list[dict[str, Any]] = []
    input_tokens = 0
    output_tokens = 0
    total_tokens = 0
    last_provider_request_id = ""
    last_request_id_source = "unavailable"
    last_provider_model = llm_config.model
    last_usage_source = "estimated"
    last_transport_mode = "real"
    compact_triggered = False
    provider_call_attempted = False

    try:
        chat_result = run_chat(
            args=args,
            llm_config=llm_config,
            session=session,
            user_input=args.prompt,
            max_iterations=max_iterations,
            max_retries=max_retries,
        )
        provider_call_attempted = True
        response_text = str(chat_result.get("output", ""))
        tool_call_values = chat_result.get("tool_calls")
        if isinstance(tool_call_values, list):
            tool_call_records = tool_call_values
        input_tokens = int(chat_result.get("input_tokens", 0))
        output_tokens = int(chat_result.get("output_tokens", 0))
        total_tokens = int(chat_result.get("total_tokens", 0))
        provider_request_id = chat_result.get("provider_request_id")
        if isinstance(provider_request_id, str):
            last_provider_request_id = provider_request_id
        request_id_source = chat_result.get("request_id_source")
        if isinstance(request_id_source, str):
            last_request_id_source = request_id_source
        provider_model = chat_result.get("provider_model")
        if isinstance(provider_model, str):
            last_provider_model = provider_model
        usage_source = chat_result.get("usage_source")
        if isinstance(usage_source, str):
            last_usage_source = usage_source
        transport_mode = chat_result.get("transport_mode")
        if isinstance(transport_mode, str):
            last_transport_mode = transport_mode
        compact_triggered = bool(chat_result.get("compact_triggered", False))
    except RuntimeError as err:
        if str(err) == "MAX_ITERATIONS_REACHED":
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "llm",
                        "error": "MAX_ITERATIONS_REACHED",
                        "message": "tool-calling loop exceeded max_iterations",
                        "max_iterations": max_iterations,
                        "provider_call_attempted": True,
                    },
                    ensure_ascii=False,
                )
            )
            return 1
        raise
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
                        "provider_call_attempted": provider_call_attempted,
                    },
                    ensure_ascii=False,
                )
            )
            return 2
        if err.code == -32024:
            print(
                json.dumps(
                    {
                        "status": "blocked",
                        "agent_id": args.agent_id,
                        "tool": args.tool,
                        "error": "policy.ask",
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
                    "tool": args.tool,
                    "code": err.code,
                    "message": err.message,
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

    session_save_path = getattr(args, "session_save", None)
    if isinstance(session_save_path, str) and session_save_path:
        try:
            run_session_command(
                command="save",
                file_path=session_save_path,
                session=session,
            )
        except ValueError as err:
            print(
                json.dumps(
                    {
                        "status": "failed",
                        "stage": "session_save",
                        "error": "persist_failed",
                        "message": str(err),
                    },
                    ensure_ascii=False,
                )
            )
            return 1

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
                    "context_window_tokens": session.context_window_tokens,
                    "max_context_tokens": session.max_context_tokens,
                    "compact_triggered": compact_triggered,
                },
                "session": {
                    "head_id": session.head_id,
                    "message_count": len(session.messages),
                    "load_path": session_load_path,
                    "save_path": session_save_path,
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
