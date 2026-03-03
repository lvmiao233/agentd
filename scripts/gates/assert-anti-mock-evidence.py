#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import cast

ALLOWED_USAGE_SOURCES = {"provider", "estimated"}
ALLOWED_TRANSPORT_MODES = {"real", "simulated"}


@dataclass(frozen=True)
class CliArgs:
    evidence_json: str
    real_path: bool
    error_evidence: str


@dataclass(frozen=True)
class ValidationError:
    code: str
    message: str


def validate_anti_mock_evidence(
    payload: dict[str, object],
    *,
    require_real_path: bool,
) -> list[ValidationError]:
    errors: list[ValidationError] = []

    provider_request_id_obj = payload.get("provider_request_id")
    provider_model_obj = payload.get("provider_model")
    usage_source_obj = payload.get("usage_source")
    transport_mode_obj = payload.get("transport_mode")

    provider_request_id = (
        provider_request_id_obj if isinstance(provider_request_id_obj, str) else None
    )
    provider_model = provider_model_obj if isinstance(provider_model_obj, str) else None
    usage_source = usage_source_obj if isinstance(usage_source_obj, str) else None
    transport_mode = transport_mode_obj if isinstance(transport_mode_obj, str) else None

    if provider_request_id is None:
        errors.append(
            ValidationError(
                "INVALID_PROVIDER_REQUEST_ID_TYPE",
                "provider_request_id must be a string",
            )
        )
    if provider_model is None or not provider_model.strip():
        errors.append(
            ValidationError(
                "INVALID_PROVIDER_MODEL", "provider_model must be a non-empty string"
            )
        )
    if usage_source not in ALLOWED_USAGE_SOURCES:
        errors.append(
            ValidationError(
                "INVALID_USAGE_SOURCE",
                "usage_source must be one of: provider|estimated",
            )
        )
    if transport_mode not in ALLOWED_TRANSPORT_MODES:
        errors.append(
            ValidationError(
                "INVALID_TRANSPORT_MODE",
                "transport_mode must be one of: real|simulated",
            )
        )

    if require_real_path:
        if provider_request_id is None or not provider_request_id.strip():
            errors.append(
                ValidationError(
                    "MISSING_PROVIDER_REQUEST_ID",
                    "real-path requires non-empty provider_request_id",
                )
            )
        if usage_source != "provider":
            errors.append(
                ValidationError(
                    "REAL_PATH_USAGE_SOURCE_MISMATCH",
                    "real-path requires usage_source=provider",
                )
            )
        if transport_mode != "real":
            errors.append(
                ValidationError(
                    "MOCK_EVIDENCE_REJECTED",
                    "MOCK_EVIDENCE_REJECTED: transport_mode must be real",
                )
            )

    return errors


def parse_args() -> CliArgs:
    parser = argparse.ArgumentParser(
        prog="assert-anti-mock-evidence",
        description="Validate anti-mock evidence schema and real-path constraints",
    )
    _ = parser.add_argument("--evidence-json", required=True)
    _ = parser.add_argument("--real-path", action="store_true")
    _ = parser.add_argument(
        "--error-evidence",
        default=".sisyphus/evidence/task-4-anti-mock-simulated-error.txt",
    )
    ns = parser.parse_args()
    return CliArgs(
        evidence_json=cast(str, ns.evidence_json),
        real_path=cast(bool, ns.real_path),
        error_evidence=cast(str, ns.error_evidence),
    )


def load_json_payload(path: Path) -> object:
    return cast(object, json.loads(path.read_text(encoding="utf-8")))


def main() -> int:
    args = parse_args()
    evidence_json = args.evidence_json
    real_path = args.real_path
    error_evidence = args.error_evidence

    evidence_path = Path(evidence_json)
    loaded = load_json_payload(evidence_path)
    if not isinstance(loaded, dict):
        Path(error_evidence).parent.mkdir(parents=True, exist_ok=True)
        _ = Path(error_evidence).write_text(
            "evidence payload must be a JSON object\n", encoding="utf-8"
        )
        print("ASSERT anti_mock_schema=FAIL")
        print("ASSERT anti_mock_reason=INVALID_EVIDENCE_PAYLOAD")
        print("ASSERT anti_mock_error=evidence payload must be a JSON object")
        return 1
    payload = cast(dict[str, object], loaded)

    errors = validate_anti_mock_evidence(payload, require_real_path=real_path)

    if errors:
        Path(error_evidence).parent.mkdir(parents=True, exist_ok=True)
        error_lines = [f"{error.code}: {error.message}" for error in errors]
        _ = Path(error_evidence).write_text(
            "\n".join(error_lines) + "\n", encoding="utf-8"
        )
        print("ASSERT anti_mock_schema=FAIL")
        for error in errors:
            print(f"ASSERT anti_mock_reason={error.code}")
            print(f"ASSERT anti_mock_error={error.message}")
        return 1

    print("ASSERT anti_mock_schema=PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
