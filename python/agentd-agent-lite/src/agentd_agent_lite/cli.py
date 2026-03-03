import argparse
import json
import socket
import sys
import uuid
from dataclasses import dataclass
from typing import Any


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


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(prog="agentd-agent-lite")
    parser.add_argument("--socket-path", default="/tmp/agentd.sock")
    parser.add_argument("--agent-id", required=True)
    parser.add_argument("--prompt", required=True)
    parser.add_argument("--model", default="claude-4-sonnet")
    parser.add_argument("--tool", default="builtin.lite.echo")
    return parser.parse_args()


def run_once(args: argparse.Namespace) -> int:
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
    response_text = f"lite:{tool_output}"

    input_tokens = estimate_tokens(args.prompt)
    output_tokens = estimate_tokens(response_text)
    total_tokens = input_tokens + output_tokens
    cost_usd = round(total_tokens * 0.000001, 8)

    try:
        usage = call_rpc(
            args.socket_path,
            "RecordUsage",
            {
                "agent_id": args.agent_id,
                "model_name": args.model,
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
                "model": args.model,
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
