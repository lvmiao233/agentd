from __future__ import annotations

import subprocess
import threading
from types import TracebackType
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import cast, final, override


REPO_ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = REPO_ROOT / "scripts" / "gates" / "preflight-real-oneapi.sh"


@final
class _FakeOneAPI:
    def __init__(self, *, health_ok: bool, models_payload: str) -> None:
        self.health_ok = health_ok
        self.models_payload = models_payload
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), self._handler())
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)

    def _handler(self) -> type[BaseHTTPRequestHandler]:
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self) -> None:  # noqa: N802
                if self.path == "/health":
                    if owner.health_ok:
                        self.send_response(200)
                        self.end_headers()
                        _ = self.wfile.write(b'{"status":"ok"}')
                    else:
                        self.send_response(503)
                        self.end_headers()
                        _ = self.wfile.write(b'{"status":"down"}')
                    return

                if self.path == "/api/status":
                    if owner.health_ok:
                        self.send_response(200)
                        self.end_headers()
                        _ = self.wfile.write(b'{"success":true}')
                    else:
                        self.send_response(503)
                        self.end_headers()
                        _ = self.wfile.write(b'{"success":false}')
                    return

                if self.path == "/v1/models":
                    auth = self.headers.get("Authorization")
                    if auth != "Bearer test-token":
                        self.send_response(401)
                        self.end_headers()
                        _ = self.wfile.write(b'{"error":"unauthorized"}')
                        return
                    self.send_response(200)
                    self.end_headers()
                    _ = self.wfile.write(owner.models_payload.encode("utf-8"))
                    return

                self.send_response(404)
                self.end_headers()

            @override
            def log_message(self, format: str, *args: object) -> None:  # noqa: A003
                return

        return Handler

    @property
    def base_url(self) -> str:
        host = cast(str, self.server.server_address[0])
        port = self.server.server_address[1]
        return f"http://{host}:{port}"

    def __enter__(self) -> _FakeOneAPI:
        self.thread.start()
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)


def _run_preflight(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["bash", str(SCRIPT_PATH), *args],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def test_preflight_fails_when_token_missing_and_writes_evidence(tmp_path: Path) -> None:
    evidence = tmp_path / "missing-token-evidence.txt"
    with _FakeOneAPI(
        health_ok=True, models_payload='{"data":[{"id":"gpt-4o"}]}'
    ) as api:
        result = _run_preflight(
            "--base-url", api.base_url, "--error-evidence", str(evidence)
        )

    assert result.returncode != 0
    assert "HEALTH=true" in result.stdout
    assert "MODELS_CHECKED=false" in result.stdout
    assert "ENV_READY=false" in result.stdout
    assert "REASON_CODE=ONE_API_TOKEN_MISSING" in result.stdout
    assert evidence.exists()
    assert "reason=ONE_API_TOKEN_MISSING" in evidence.read_text(encoding="utf-8")


def test_preflight_fails_when_health_unreachable_and_writes_evidence(
    tmp_path: Path,
) -> None:
    evidence = tmp_path / "health-failed-evidence.txt"
    with _FakeOneAPI(
        health_ok=False, models_payload='{"data":[{"id":"gpt-4o"}]}'
    ) as api:
        result = _run_preflight(
            "--base-url",
            api.base_url,
            "--token",
            "test-token",
            "--error-evidence",
            str(evidence),
        )

    assert result.returncode != 0
    assert "HEALTH=false" in result.stdout
    assert "MODELS_CHECKED=false" in result.stdout
    assert "ENV_READY=false" in result.stdout
    assert "REASON_CODE=ONE_API_HEALTH_UNREACHABLE" in result.stdout
    assert evidence.exists()
    assert "reason=ONE_API_HEALTH_UNREACHABLE" in evidence.read_text(encoding="utf-8")


def test_preflight_happy_path_emits_ready_markers(tmp_path: Path) -> None:
    evidence = tmp_path / "unused-error-evidence.txt"
    with _FakeOneAPI(
        health_ok=True,
        models_payload='{"data":[{"id":"gpt-4o"},{"id":"claude-4-sonnet"}]}',
    ) as api:
        result = _run_preflight(
            "--base-url",
            api.base_url,
            "--token",
            "test-token",
            "--error-evidence",
            str(evidence),
        )

    assert result.returncode == 0
    assert "HEALTH=true" in result.stdout
    assert "MODELS_CHECKED=true" in result.stdout
    assert "ENV_READY=true" in result.stdout
    assert "REASON_CODE=READY" in result.stdout
    assert "MODELS_VISIBLE_COUNT=2" in result.stdout
    assert not evidence.exists()


def test_preflight_dry_run_succeeds_without_network() -> None:
    result = _run_preflight("--dry-run")

    assert result.returncode == 0
    assert "HEALTH=false" in result.stdout
    assert "MODELS_CHECKED=false" in result.stdout
    assert "ENV_READY=false" in result.stdout
    assert "REASON_CODE=DRY_RUN" in result.stdout
