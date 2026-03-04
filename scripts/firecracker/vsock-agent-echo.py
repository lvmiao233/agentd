#!/usr/bin/env python3

"""Minimal guest-side vsock simulator for FirecrackerExecutor tests.

The script emulates an in-VM agent-lite process:
1. connect to host bridge socket from AGENTD_VSOCK_PATH
2. read line-delimited JSON requests
3. return line-delimited JSON responses
"""

from __future__ import annotations

import json
import os
import socket
import sys
import time


def main() -> int:
    vsock_path = os.environ.get("AGENTD_VSOCK_PATH", "").strip()
    if not vsock_path:
        print("AGENTD_VSOCK_PATH is required", file=sys.stderr)
        return 2

    delay_secs = float(os.environ.get("AGENTD_VSOCK_CONNECT_DELAY_SECS", "0") or "0")
    if delay_secs > 0:
        time.sleep(delay_secs)

    client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    client.connect(vsock_path)

    with client:
        reader = client.makefile("r", encoding="utf-8")
        writer = client.makefile("w", encoding="utf-8")

        for line in reader:
            payload_line = line.strip()
            if not payload_line:
                continue

            try:
                payload = json.loads(payload_line)
                response = {
                    "status": "ok",
                    "transport": "vsock-simulated",
                    "echo": payload,
                }
            except json.JSONDecodeError as exc:
                response = {
                    "status": "error",
                    "transport": "vsock-simulated",
                    "error": f"invalid-json:{exc.msg}",
                }

            writer.write(json.dumps(response, ensure_ascii=False) + "\n")
            writer.flush()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
