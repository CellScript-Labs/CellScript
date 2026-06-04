#!/usr/bin/env python3
"""Generate the NovaSeal external evidence handoff bundle.

This bundle is the machine-readable handoff contract for external production
evidence providers. It aggregates the BTC SPV evidence adapter and external
attestation adapter into one checked request package. It is deliberately not
production evidence: the public BTC SPV evidence, public/shared CellDep
attestation, and external BIP340 TCB review must still be supplied separately.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BTC_SPV_ADAPTER = ROOT / "target/novaseal-btc-spv-evidence-adapter.json"
DEFAULT_EXTERNAL_ATTESTATION_ADAPTER = ROOT / "target/novaseal-external-attestation-adapter.json"
DEFAULT_OUTPUT = ROOT / "target/novaseal-external-evidence-handoff-bundle.json"

REPORT_PERSON = b"NovaExtHandoff"

PUBLIC_BTC_SPV_EVIDENCE = "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json"
PUBLIC_CELLDEP_ATTESTATION = "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json"
EXTERNAL_TCB_ATTESTATION = "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json"

REQUIRED_BTC_SPV_PROFILES = [
    "btc-transaction-commitment-profile-v0",
    "btc-utxo-seal-profile-v0",
    "dual-seal-profile-v0",
]

REQUIRED_PUBLIC_CELLDEP_FIELDS = [
    "network",
    "attested_at",
    "attestor",
    "release.package",
    "release.version",
    "release.manifest_commit",
    "runtime_verifier.verifier_id",
    "runtime_verifier.ipc_abi",
    "runtime_verifier.out_point",
    "runtime_verifier.data_hash",
    "runtime_verifier.dep_type",
    "runtime_verifier.hash_type",
    "runtime_verifier.artifact_hash",
    "request_handoff.bundle",
    "request_handoff.bundle_hash",
    "request_handoff.bundle_hash_algorithm",
    "request_handoff.group",
]

REQUIRED_EXTERNAL_TCB_FIELDS = [
    "reviewer",
    "review_date",
    "review_scope",
    "verifier_id",
    "ipc_abi",
    "artifact_hash",
    "artifact_hash_algorithm",
    "source_tree_sha256",
    "report_uri",
    "request_handoff.bundle",
    "request_handoff.bundle_hash",
    "request_handoff.bundle_hash_algorithm",
    "request_handoff.group",
]


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


def required_field_set(case: dict[str, Any]) -> set[str]:
    fields = case.get("request", {}).get("required_public_fields", [])
    return {field for field in fields if isinstance(field, str)}


def btc_spv_handoff_case(adapter: dict[str, Any]) -> dict[str, Any]:
    cases = adapter.get("cases", [])
    profiles = {case.get("profile") for case in cases}
    checks = {
        "source_adapter_passed": adapter.get("status") == "passed",
        "source_adapter_status_request_ready": adapter.get("adapter_status") == "request_ready_external_evidence_required",
        "production_output_matches": adapter.get("production_output") == PUBLIC_BTC_SPV_EVIDENCE,
        "summary_counts_match": adapter.get("summary", {}).get("total") == len(REQUIRED_BTC_SPV_PROFILES)
        and adapter.get("summary", {}).get("matched") == adapter.get("summary", {}).get("total"),
        "required_profiles_complete": profiles == set(REQUIRED_BTC_SPV_PROFILES),
        "source_cases_passed": all(case.get("status") == "passed" for case in cases),
    }
    return {
        "group": "public_btc_spv_evidence",
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "source_adapter": str(DEFAULT_BTC_SPV_ADAPTER.relative_to(ROOT)),
        "source_adapter_hash": report_hash("btc_spv_adapter", adapter),
        "production_output": PUBLIC_BTC_SPV_EVIDENCE,
        "required_profiles": REQUIRED_BTC_SPV_PROFILES,
        "required_external_fields": [
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
    }


def attestation_case(
    adapter: dict[str, Any],
    *,
    case_name: str,
    group: str,
    production_output: str,
    required_fields: list[str],
) -> dict[str, Any]:
    cases = adapter.get("cases", [])
    source_case = next((case for case in cases if case.get("name") == case_name), {})
    fields = required_field_set(source_case)
    checks = {
        "source_adapter_passed": adapter.get("status") == "passed",
        "source_adapter_status_request_ready": adapter.get("adapter_status") == "request_ready_external_attestations_required",
        "source_case_passed": source_case.get("status") == "passed",
        "production_output_matches": source_case.get("request", {}).get("production_output") == production_output,
        "required_fields_complete": set(required_fields).issubset(fields),
    }
    return {
        "group": group,
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "source_adapter": str(DEFAULT_EXTERNAL_ATTESTATION_ADAPTER.relative_to(ROOT)),
        "source_adapter_hash": report_hash("external_attestation_adapter", adapter),
        "source_case": case_name,
        "production_output": production_output,
        "required_external_fields": required_fields,
    }


def build_report(btc_spv_adapter: dict[str, Any], external_attestation_adapter: dict[str, Any]) -> dict[str, Any]:
    cases = [
        btc_spv_handoff_case(btc_spv_adapter),
        attestation_case(
            external_attestation_adapter,
            case_name="public_shared_cell_dep_attestation",
            group="public_shared_cell_dep_attestation",
            production_output=PUBLIC_CELLDEP_ATTESTATION,
            required_fields=REQUIRED_PUBLIC_CELLDEP_FIELDS,
        ),
        attestation_case(
            external_attestation_adapter,
            case_name="external_bip340_tcb_review_attestation",
            group="external_bip340_tcb_review_attestation",
            production_output=EXTERNAL_TCB_ATTESTATION,
            required_fields=REQUIRED_EXTERNAL_TCB_FIELDS,
        ),
    ]
    production_outputs = [case["production_output"] for case in cases]
    status = "passed" if all(case["status"] == "passed" for case in cases) else "failed"
    return {
        "schema": "novaseal-external-evidence-handoff-bundle-v0.1",
        "status": status,
        "handoff_status": "request_bundle_ready_external_evidence_required",
        "source_btc_spv_adapter": str(DEFAULT_BTC_SPV_ADAPTER.relative_to(ROOT)),
        "source_btc_spv_adapter_hash": report_hash("btc_spv_adapter", btc_spv_adapter),
        "source_external_attestation_adapter": str(DEFAULT_EXTERNAL_ATTESTATION_ADAPTER.relative_to(ROOT)),
        "source_external_attestation_adapter_hash": report_hash(
            "external_attestation_adapter", external_attestation_adapter
        ),
        "production_outputs": production_outputs,
        "production_boundary": "This handoff proves external request completeness; it does not satisfy external production evidence.",
        "summary": {
            "total": len(cases),
            "matched": len([case for case in cases if case["status"] == "passed"]),
            "groups": [case["group"] for case in cases],
        },
        "cases": cases,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--btc-spv-adapter", type=Path, default=DEFAULT_BTC_SPV_ADAPTER)
    parser.add_argument("--external-attestation-adapter", type=Path, default=DEFAULT_EXTERNAL_ATTESTATION_ADAPTER)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    btc_spv_adapter = json.loads(args.btc_spv_adapter.read_text(encoding="utf-8"))
    external_attestation_adapter = json.loads(args.external_attestation_adapter.read_text(encoding="utf-8"))
    report = build_report(btc_spv_adapter, external_attestation_adapter)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.pretty:
        print(
            f"wrote {args.output} status={report['status']} "
            f"groups={report['summary']['matched']}/{report['summary']['total']}"
        )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
