#!/usr/bin/env python3
"""Generate the NovaSeal public BTC SPV evidence adapter request.

This report is not public BTC evidence. It is the deterministic request
contract that tells an external BTC SPV operator exactly which NovaSeal
profiles, local builder evidence, and production fields must be supplied before
`public_btc_spv_evidence.json` may pass the production gate.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_SERVICE_BUILDER_FIXTURES = ROOT / "target/novaseal-service-builder-fixtures.json"
DEFAULT_TEMPLATE = ROOT / "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json"
DEFAULT_OUTPUT = ROOT / "target/novaseal-btc-spv-evidence-adapter.json"

REPORT_PERSON = b"NovaBtcSpvReqV0"
REQUIRED_PROFILES = [
    "btc-transaction-commitment-profile-v0",
    "btc-utxo-seal-profile-v0",
    "dual-seal-profile-v0",
]
REQUIRED_SCENARIOS = {
    "btc-transaction-commitment-profile-v0": "btc-transaction-commitment-transition",
    "btc-utxo-seal-profile-v0": "btc-utxo-seal-closure",
    "dual-seal-profile-v0": "dual-seal-finality",
}


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


def profile_cases(service_builder: dict[str, Any], template: dict[str, Any]) -> list[dict[str, Any]]:
    builder_cases = service_builder.get("cases", [])
    template_cases = template.get("cases", [])
    cases = []
    for profile in REQUIRED_PROFILES:
        builder_case = next((case for case in builder_cases if case.get("profile") == profile), None)
        template_case = next((case for case in template_cases if case.get("profile") == profile), None)
        external_inputs = builder_case.get("request", {}).get("production_external_inputs", []) if builder_case else []
        request = {
            "profile": profile,
            "scenario": template_case.get("scenario") if template_case else None,
            "minimum_confirmations": template_case.get("minimum_confirmations") if template_case else 6,
            "required_public_fields": [
                "network",
                "generated_at",
                "evidence_provider",
                "required_profiles",
                "profile",
                "scenario",
                "btc_txid",
                "btc_block_hash",
                "spv_proof_hash",
                "minimum_confirmations",
                "confirmations",
                "spv_client_cell_dep.out_point",
                "spv_client_cell_dep.data_hash",
                "spv_client_cell_dep.dep_type",
                "spv_client_cell_dep.hash_type",
                "source_service.name",
                "source_service.commit",
                "source_service.report_hash",
                "request_handoff.bundle",
                "request_handoff.bundle_hash",
                "request_handoff.bundle_hash_algorithm",
                "request_handoff.group",
            ],
            "field_constraints": {
                "network": "explicit public mainnet/testnet name; placeholders and local/devnet/regtest/simnet/private/fake labels are rejected",
                "generated_at": "UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form; future timestamps are rejected",
                "evidence_provider": "real external provider identity; placeholder, example, and unknown tokens are rejected",
                "btc_txid": "0x-prefixed 32-byte non-placeholder Bitcoin transaction id",
                "btc_block_hash": "0x-prefixed 32-byte non-placeholder Bitcoin block hash anchoring the SPV proof",
                "spv_proof_hash": "0x-prefixed 32-byte non-placeholder hash of the SPV proof material",
                "minimum_confirmations": "integer confirmation floor; at least 6",
                "confirmations": "integer observed confirmations meeting minimum_confirmations",
                "spv_client_cell_dep.out_point": "0x-prefixed 32-byte CKB transaction hash plus numeric output index",
                "spv_client_cell_dep.data_hash": "0x-prefixed 32-byte non-placeholder SPV client data hash",
                "spv_client_cell_dep.dep_type": "code",
                "spv_client_cell_dep.hash_type": "data, data1, or type CKB script hash type",
                "source_service.name": "real external SPV service identity; placeholder, example, and unknown tokens are rejected",
                "source_service.commit": "40-character hex service source commit",
                "source_service.report_hash": "0x-prefixed 32-byte non-placeholder SPV service report hash",
                "request_handoff.bundle": "target/novaseal-external-evidence-handoff-bundle.json",
                "request_handoff.bundle_hash": "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle",
                "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
                "request_handoff.group": "public_btc_spv_evidence",
            },
            "required_external_inputs": external_inputs,
            "service_builder_case_hash": report_hash("service_builder_case", builder_case),
            "service_builder_tx_skeleton_hash": builder_case.get("response", {}).get("tx_skeleton_hash") if builder_case else None,
            "service_builder_receipt_binding_hash": builder_case.get("response", {}).get("receipt_binding_hash") if builder_case else None,
            "template_case_hash": report_hash("template_case", template_case),
        }
        checks = {
            "service_builder_case_present": builder_case is not None,
            "template_case_present": template_case is not None,
            "scenario_matches_required_profile": request["scenario"] == REQUIRED_SCENARIOS[profile],
            "public_btc_spv_external_input_named": "public_btc_spv_evidence" in external_inputs,
            "minimum_confirmations_at_least_six": request["minimum_confirmations"] >= 6,
            "service_builder_hashes_present": bool(request["service_builder_tx_skeleton_hash"])
            and bool(request["service_builder_receipt_binding_hash"]),
            "required_public_fields_complete": len(request["required_public_fields"]) == 22,
        }
        cases.append(
            {
                "profile": profile,
                "status": "passed" if all(checks.values()) else "failed",
                "checks": checks,
                "request": request,
            }
        )
    return cases


def build_report(service_builder: dict[str, Any], template: dict[str, Any]) -> dict[str, Any]:
    cases = profile_cases(service_builder, template)
    status = "passed" if all(case["status"] == "passed" for case in cases) else "failed"
    return {
        "schema": "novaseal-btc-spv-evidence-adapter-v0.1",
        "status": status,
        "adapter_status": "request_ready_external_evidence_required",
        "source_service_builder_report": str(DEFAULT_SERVICE_BUILDER_FIXTURES.relative_to(ROOT)),
        "source_service_builder_report_hash": report_hash("service_builder_report", service_builder),
        "source_public_btc_spv_template": str(DEFAULT_TEMPLATE.relative_to(ROOT)),
        "source_public_btc_spv_template_hash": report_hash("public_btc_spv_template", template),
        "production_output": "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json",
        "production_boundary": "This adapter proves the request contract is complete; it does not prove BTC inclusion, spend validity, confirmation depth, or public SPV client deployment.",
        "summary": {
            "total": len(cases),
            "matched": len([case for case in cases if case["status"] == "passed"]),
            "required_profiles": REQUIRED_PROFILES,
        },
        "cases": cases,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--service-builder-fixtures", type=Path, default=DEFAULT_SERVICE_BUILDER_FIXTURES)
    parser.add_argument("--template", type=Path, default=DEFAULT_TEMPLATE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    service_builder = json.loads(args.service_builder_fixtures.read_text(encoding="utf-8"))
    template = json.loads(args.template.read_text(encoding="utf-8"))
    report = build_report(service_builder, template)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.pretty:
        print(
            f"wrote {args.output} status={report['status']} "
            f"profiles={report['summary']['matched']}/{report['summary']['total']}"
        )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
