#!/usr/bin/env python3
"""Generate NovaSeal service-builder fixtures from operator fixtures.

The report models the wallet/service request and response boundary for every
planned NovaSeal profile action. It intentionally remains a deterministic JSON
builder fixture, not a claim that public BTC SPV, public CellDep, or external
TCB attestations have been collected.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OPERATOR_FIXTURES = ROOT / "target/novaseal-profile-operator-fixtures.json"
DEFAULT_OUTPUT = ROOT / "target/novaseal-service-builder-fixtures.json"

REPORT_PERSON = b"NovaSvcBuildV0"


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def report_hash(label: str, value: Any) -> str:
    h = hashlib.blake2b(digest_size=32, person=REPORT_PERSON)
    h.update(label.encode("utf-8"))
    h.update(b"\x00")
    h.update(canonical_json(value))
    return hex0x(h.digest())


def is_hex32(value: Any) -> bool:
    if not isinstance(value, str) or not value.startswith("0x") or len(value) != 66:
        return False
    try:
        bytes.fromhex(value[2:])
    except ValueError:
        return False
    return True


def external_inputs(profile: str) -> list[str]:
    required = ["public_shared_cell_dep_attestation", "external_bip340_tcb_review_attestation"]
    if profile in {"btc-transaction-commitment-profile-v0", "btc-utxo-seal-profile-v0", "dual-seal-profile-v0"}:
        required.append("public_btc_spv_evidence")
    if profile == "rwa-receipt-profile-v0":
        required.append("legal_registry_review_evidence")
    return required


def build_case(operator_case: dict[str, Any]) -> dict[str, Any]:
    profile = operator_case["profile"]
    action = operator_case["action"]
    fixture = operator_case["fixture"]
    signers = operator_case["signers"]
    operator_fixture_hash = report_hash("operator_case", operator_case)
    request = {
        "schema": "novaseal-service-builder-request-v0.1",
        "builder_name": "novaseal-profile-service-builder-v0",
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "idempotency_key": report_hash("idempotency", [profile, action, fixture, operator_case["signed_intent_hash"]]),
        "operator_fixture_hash": operator_fixture_hash,
        "signers": signers,
        "required_profile_inputs": {
            "source_tree_hash": operator_case["source_tree_hash"],
            "schema_set_hash": operator_case["schema_set_hash"],
            "proof_matrix_hash": operator_case["proof_matrix_hash"],
            "fixture_hash": operator_case["fixture_hash"],
        },
        "required_live_inputs": {
            "live_report_hash": operator_case.get("live_report_hash"),
            "live_devnet_tx_hash": operator_case.get("live_devnet_tx_hash"),
            "fiber_report_hash": operator_case.get("fiber_report_hash"),
            "public_btc_anchor": operator_case.get("public_btc_anchor"),
        },
        "production_external_inputs": external_inputs(profile),
    }
    tx_skeleton = {
        "schema": "novaseal-service-builder-tx-skeleton-v0.1",
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "builder_name": request["builder_name"],
        "operator_fixture_hash": operator_fixture_hash,
        "signed_intent_hash": operator_case["signed_intent_hash"],
        "witness_shape_hash": operator_case["witness_shape_hash"],
        "source_tree_hash": operator_case["source_tree_hash"],
        "live_devnet_tx_hash": operator_case.get("live_devnet_tx_hash"),
        "public_btc_anchor": operator_case.get("public_btc_anchor"),
    }
    response = {
        "schema": "novaseal-service-builder-response-v0.1",
        "builder_name": request["builder_name"],
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "service_queue_key": report_hash("service_queue", [profile, action, fixture, request["idempotency_key"]]),
        "tx_skeleton_hash": report_hash("tx_skeleton", tx_skeleton),
        "witness_shape_hash": operator_case["witness_shape_hash"],
        "signed_intent_hash": operator_case["signed_intent_hash"],
        "bip340_message_hash": operator_case["bip340_message_hash"],
        "receipt_binding_hash": report_hash(
            "receipt_binding",
            {
                "profile": profile,
                "action": action,
                "fixture": fixture,
                "signed_intent_hash": operator_case["signed_intent_hash"],
                "tx_skeleton_hash": report_hash("tx_skeleton", tx_skeleton),
                "operator_fixture_hash": operator_fixture_hash,
            },
        ),
        "builder_trace_hash": report_hash("builder_trace", {"request": request, "tx_skeleton": tx_skeleton}),
    }
    checks = {
        "operator_case_passed": operator_case.get("status") == "passed",
        "request_hashes_present": all(is_hex32(value) for value in request["required_profile_inputs"].values()),
        "signed_intent_hash_bound": is_hex32(response["signed_intent_hash"])
        and response["signed_intent_hash"] == operator_case["signed_intent_hash"],
        "bip340_message_hash_bound": is_hex32(response["bip340_message_hash"])
        and response["bip340_message_hash"] == operator_case["bip340_message_hash"],
        "witness_shape_hash_bound": is_hex32(response["witness_shape_hash"])
        and response["witness_shape_hash"] == operator_case["witness_shape_hash"],
        "tx_skeleton_hash_present": is_hex32(response["tx_skeleton_hash"]),
        "receipt_binding_hash_present": is_hex32(response["receipt_binding_hash"]),
        "service_queue_key_present": is_hex32(response["service_queue_key"]),
        "external_requirements_named": bool(request["production_external_inputs"]),
        "public_btc_anchor_bound_when_required": (
            "public_btc_spv_evidence" not in request["production_external_inputs"]
            or bool(request["required_live_inputs"].get("public_btc_anchor"))
        ),
    }
    return {
        "profile": profile,
        "action": action,
        "fixture": fixture,
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "builder_name": request["builder_name"],
        "operator_fixture_hash": operator_fixture_hash,
        "signers": signers,
        "request": request,
        "response": response,
        "tx_skeleton": tx_skeleton,
    }


def build_report(operator_fixtures: dict[str, Any]) -> dict[str, Any]:
    cases = [build_case(case) for case in operator_fixtures.get("cases", [])]
    profiles = sorted({case["profile"] for case in cases})
    status = "passed" if cases and all(case["status"] == "passed" for case in cases) else "failed"
    return {
        "schema": "novaseal-service-builder-fixtures-v0.1",
        "status": status,
        "builder_name": "novaseal-profile-service-builder-v0",
        "source_operator_fixture_report": str(DEFAULT_OPERATOR_FIXTURES.relative_to(ROOT)),
        "source_operator_fixture_report_hash": report_hash("operator_report", operator_fixtures),
        "fixture_boundary": "builder fixtures model reproducible service request/response hashes for local profile evidence; public BTC SPV, public CellDep, external TCB, and legal registry evidence remain production inputs",
        "summary": {
            "total": len(cases),
            "matched": len([case for case in cases if case["status"] == "passed"]),
            "profile_count": len(profiles),
            "profiles": profiles,
        },
        "profiles": profiles,
        "cases": cases,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--operator-fixtures", type=Path, default=DEFAULT_OPERATOR_FIXTURES)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    operator_fixtures = json.loads(args.operator_fixtures.read_text(encoding="utf-8"))
    report = build_report(operator_fixtures)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.pretty:
        print(
            f"wrote {args.output} status={report['status']} "
            f"profiles={report['summary']['profile_count']} cases={report['summary']['total']}"
        )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
