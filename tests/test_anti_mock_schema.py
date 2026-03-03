from __future__ import annotations

import json
import subprocess
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
ASSERT_SCRIPT = REPO_ROOT / "scripts" / "gates" / "assert-anti-mock-evidence.py"


def _run_assert(
    evidence_path: Path, error_evidence: Path
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "python3",
            str(ASSERT_SCRIPT),
            "--evidence-json",
            str(evidence_path),
            "--real-path",
            "--error-evidence",
            str(error_evidence),
        ],
        cwd=REPO_ROOT,
        text=True,
        capture_output=True,
        check=False,
    )


def test_real_path_rejects_missing_provider_request_id(tmp_path: Path) -> None:
    evidence_path = tmp_path / "task-4-anti-mock-happy.json"
    error_evidence = tmp_path / "task-4-anti-mock-simulated-error.txt"
    _ = evidence_path.write_text(
        json.dumps(
            {
                "provider_request_id": "",
                "provider_model": "claude-4-sonnet",
                "usage_source": "provider",
                "transport_mode": "real",
            }
        ),
        encoding="utf-8",
    )

    result = _run_assert(evidence_path, error_evidence)

    assert result.returncode != 0
    assert "ASSERT anti_mock_schema=FAIL" in result.stdout
    assert "ASSERT anti_mock_reason=MISSING_PROVIDER_REQUEST_ID" in result.stdout
    assert error_evidence.exists()
    error_text = error_evidence.read_text(encoding="utf-8")
    assert "MISSING_PROVIDER_REQUEST_ID" in error_text


def test_real_path_rejects_simulated_transport_mode(tmp_path: Path) -> None:
    evidence_path = tmp_path / "task-4-anti-mock-happy.json"
    error_evidence = tmp_path / "task-4-anti-mock-simulated-error.txt"
    _ = evidence_path.write_text(
        json.dumps(
            {
                "provider_request_id": "req_123",
                "provider_model": "claude-4-sonnet",
                "usage_source": "provider",
                "transport_mode": "simulated",
            }
        ),
        encoding="utf-8",
    )

    result = _run_assert(evidence_path, error_evidence)

    assert result.returncode != 0
    assert "ASSERT anti_mock_schema=FAIL" in result.stdout
    assert "MOCK_EVIDENCE_REJECTED" in result.stdout
    assert "ASSERT anti_mock_reason=MOCK_EVIDENCE_REJECTED" in result.stdout
    assert error_evidence.exists()
    assert "MOCK_EVIDENCE_REJECTED" in error_evidence.read_text(encoding="utf-8")


def test_schema_rejects_invalid_usage_source_with_machine_code(tmp_path: Path) -> None:
    evidence_path = tmp_path / "task-4-anti-mock-invalid-usage.json"
    error_evidence = tmp_path / "task-4-anti-mock-simulated-error.txt"
    _ = evidence_path.write_text(
        json.dumps(
            {
                "provider_request_id": "req_123",
                "provider_model": "claude-4-sonnet",
                "usage_source": "mocked",
                "transport_mode": "real",
            }
        ),
        encoding="utf-8",
    )

    result = _run_assert(evidence_path, error_evidence)

    assert result.returncode != 0
    assert "ASSERT anti_mock_schema=FAIL" in result.stdout
    assert "ASSERT anti_mock_reason=INVALID_USAGE_SOURCE" in result.stdout
    assert error_evidence.exists()
    assert "INVALID_USAGE_SOURCE" in error_evidence.read_text(encoding="utf-8")


def test_real_path_passes_when_anti_mock_fields_are_valid(tmp_path: Path) -> None:
    evidence_path = tmp_path / "task-4-anti-mock-happy.json"
    error_evidence = tmp_path / "task-4-anti-mock-simulated-error.txt"
    _ = evidence_path.write_text(
        json.dumps(
            {
                "provider_request_id": "req_123",
                "provider_model": "claude-4-sonnet",
                "usage_source": "provider",
                "transport_mode": "real",
            }
        ),
        encoding="utf-8",
    )

    result = _run_assert(evidence_path, error_evidence)

    assert result.returncode == 0
    assert "ASSERT anti_mock_schema=PASS" in result.stdout
    assert not error_evidence.exists()
