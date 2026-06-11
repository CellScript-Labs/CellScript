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
PRODUCTION_ANCHOR_SOURCES = {
    "btc-transaction-commitment-profile-v0": "external_public_btc_transaction",
    "btc-utxo-seal-profile-v0": "external_public_btc_spend",
    "dual-seal-profile-v0": "external_public_btc_spend",
}
REQUIRED_PUBLIC_FIELDS = [
    "network",
    "generated_at",
    "evidence_provider",
    "required_profiles",
    "profile",
    "scenario",
    "ckb_live_tx_hash",
    "live_report_hash",
    "service_builder_case_hash",
    "service_builder_tx_skeleton_hash",
    "service_builder_receipt_binding_hash",
    "ckb_btc_commitment_hash",
    "btc_txid",
    "btc_wtxid",
    "btc_tx_hex",
    "btc_block_hash",
    "btc_block_header",
    "btc_merkle_proof.tx_index",
    "btc_merkle_proof.merkle_branch",
    "btc_merkle_proof.merkle_root",
    "btc_merkle_proof.block_height",
    "btc_merkle_proof.observed_tip_height",
    "btc_transaction_binding.kind",
    "btc_transaction_binding.btc_output_index",
    "btc_transaction_binding.btc_amount_sats",
    "btc_transaction_binding.spend_input_index",
    "btc_transaction_binding.sealed_btc_txid",
    "btc_transaction_binding.sealed_btc_vout_index",
    "btc_transaction_binding.sealed_btc_amount_sats",
    "btc_transaction_binding.script_pubkey_hash",
    "btc_transaction_binding.sealed_btc_tx_hex",
    "btc_transaction_binding.sealed_utxo_commitment_hash",
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
]
FIELD_CONSTRAINTS = {
    "network": "explicit public mainnet/testnet name; placeholders and local/devnet/regtest/simnet/private/fake labels are rejected",
    "generated_at": "UTC timestamp in YYYY-MM-DDTHH:MM:SSZ form; future timestamps are rejected",
    "evidence_provider": "real external provider identity; placeholder, local/devnet/fake/internal, example, and unknown tokens are rejected",
    "ckb_live_tx_hash": "0x-prefixed 32-byte CKB live transaction hash matching the current NovaSeal service-builder case",
    "live_report_hash": "0x-prefixed 32-byte hash of the current NovaSeal live devnet report for this profile",
    "service_builder_case_hash": "0x-prefixed 32-byte hash of the current NovaSeal service-builder case for this profile",
    "service_builder_tx_skeleton_hash": "0x-prefixed 32-byte service-builder transaction skeleton hash for this profile",
    "service_builder_receipt_binding_hash": "0x-prefixed 32-byte service-builder receipt binding hash for this profile",
    "ckb_btc_commitment_hash": "0x-prefixed 32-byte CKB-side BTC commitment hash from the current live profile report",
    "btc_txid": "0x-prefixed 32-byte non-placeholder Bitcoin transaction id",
    "btc_wtxid": "0x-prefixed 32-byte Bitcoin witness transaction id derived from btc_tx_hex",
    "btc_tx_hex": "0x-prefixed raw Bitcoin transaction bytes whose txid/wtxid match the public evidence case",
    "btc_block_hash": "0x-prefixed 32-byte non-placeholder Bitcoin block hash anchoring the SPV proof",
    "btc_block_header": "0x-prefixed 80-byte Bitcoin block header whose double-SHA256 hash matches btc_block_hash",
    "btc_merkle_proof.tx_index": "zero-based transaction index used to orient the Merkle branch",
    "btc_merkle_proof.merkle_branch": (
        "array of 0x-prefixed 32-byte Bitcoin sibling hashes in display order; "
        "empty only for tx_index 0 in a single-transaction block"
    ),
    "btc_merkle_proof.merkle_root": "0x-prefixed 32-byte Bitcoin Merkle root matching the block header",
    "btc_merkle_proof.block_height": "public Bitcoin block height containing btc_txid",
    "btc_merkle_proof.observed_tip_height": "public Bitcoin tip height used to compute confirmations",
    "btc_transaction_binding.kind": "profile-specific binding kind: btc_transaction_output, btc_utxo_spend, or dual_seal_btc_closure",
    "btc_transaction_binding.btc_output_index": "BTC transaction commitment output index; required for btc-transaction-commitment-profile-v0",
    "btc_transaction_binding.btc_amount_sats": "BTC transaction commitment output amount in sats; required for btc-transaction-commitment-profile-v0",
    "btc_transaction_binding.spend_input_index": "Bitcoin spend input index; required for UTXO and dual-seal closure profiles",
    "btc_transaction_binding.sealed_btc_txid": "sealed Bitcoin transaction id whose output is spent; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    "btc_transaction_binding.sealed_btc_vout_index": "sealed Bitcoin output index; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    "btc_transaction_binding.sealed_btc_amount_sats": "sealed Bitcoin output amount in sats; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    "btc_transaction_binding.script_pubkey_hash": "0x-prefixed CKB Blake2b-256 hash of the sealed output scriptPubKey bytes; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    "btc_transaction_binding.sealed_btc_tx_hex": "0x-prefixed raw sealed Bitcoin transaction bytes; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    "btc_transaction_binding.sealed_utxo_commitment_hash": "0x-prefixed 32-byte CKB-side sealed UTXO commitment hash; required for btc-utxo-seal-profile-v0 and dual-seal-profile-v0",
    "spv_proof_hash": "0x-prefixed SHA-256 hash of the canonical BTC SPV proof material carried in this case",
    "minimum_confirmations": "integer confirmation floor; at least 6",
    "confirmations": "integer observed confirmations meeting minimum_confirmations",
    "spv_client_cell_dep.out_point": "0x-prefixed 32-byte CKB transaction hash plus numeric output index",
    "spv_client_cell_dep.data_hash": "0x-prefixed 32-byte non-placeholder SPV client data hash",
    "spv_client_cell_dep.dep_type": "code",
    "spv_client_cell_dep.hash_type": "data, data1, or type CKB script hash type",
    "source_service.name": "real external SPV service identity; placeholder, local/devnet/fake/internal, example, and unknown tokens are rejected",
    "source_service.commit": "40-character hex service source commit",
    "source_service.report_hash": "0x-prefixed 32-byte non-placeholder SPV service report hash",
    "request_handoff.bundle": "target/novaseal-external-evidence-handoff-bundle.json",
    "request_handoff.bundle_hash": "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle",
    "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
    "request_handoff.group": "public_btc_spv_evidence",
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


def is_hex32(value: Any) -> bool:
    return (
        isinstance(value, str)
        and len(value) == 66
        and value.startswith("0x")
        and all(char in "0123456789abcdefABCDEF" for char in value[2:])
    )


def is_non_negative_int(value: Any) -> bool:
    return type(value) is int and value >= 0


def is_positive_int(value: Any) -> bool:
    return type(value) is int and value > 0


def anchor_source_production_eligible(profile: str, value: Any) -> bool:
    return isinstance(value, str) and value == PRODUCTION_ANCHOR_SOURCES.get(profile)


def profile_cases(service_builder: dict[str, Any], template: dict[str, Any]) -> list[dict[str, Any]]:
    builder_cases = service_builder.get("cases", [])
    template_cases = template.get("cases", [])
    cases = []
    for profile in REQUIRED_PROFILES:
        builder_case = next((case for case in builder_cases if case.get("profile") == profile), None)
        template_case = next((case for case in template_cases if case.get("profile") == profile), None)
        external_inputs = builder_case.get("request", {}).get("production_external_inputs", []) if builder_case else []
        required_live_inputs = builder_case.get("request", {}).get("required_live_inputs", {}) if builder_case else {}
        public_btc_anchor = required_live_inputs.get("public_btc_anchor", {}) if isinstance(required_live_inputs, dict) else {}
        if not isinstance(public_btc_anchor, dict):
            public_btc_anchor = {}
        request = {
            "profile": profile,
            "scenario": template_case.get("scenario") if template_case else None,
            "minimum_confirmations": template_case.get("minimum_confirmations") if template_case else 6,
            "required_public_fields": REQUIRED_PUBLIC_FIELDS,
            "field_constraints": FIELD_CONSTRAINTS,
            "required_external_inputs": external_inputs,
            "ckb_live_tx_hash": required_live_inputs.get("live_devnet_tx_hash"),
            "live_report_hash": required_live_inputs.get("live_report_hash"),
            "service_builder_case_hash": report_hash("service_builder_case", builder_case),
            "service_builder_tx_skeleton_hash": builder_case.get("response", {}).get("tx_skeleton_hash") if builder_case else None,
            "service_builder_receipt_binding_hash": builder_case.get("response", {}).get("receipt_binding_hash") if builder_case else None,
            "local_anchor_source": public_btc_anchor.get("anchor_source"),
            "expected_anchor_source": PRODUCTION_ANCHOR_SOURCES.get(profile),
            "ckb_btc_commitment_hash": public_btc_anchor.get("ckb_btc_commitment_hash"),
            "expected_btc_txid": public_btc_anchor.get("btc_txid"),
            "expected_btc_wtxid": public_btc_anchor.get("btc_wtxid"),
            "expected_btc_output_index": public_btc_anchor.get("btc_output_index"),
            "expected_btc_amount_sats": public_btc_anchor.get("btc_amount_sats"),
            "expected_sealed_btc_txid": public_btc_anchor.get("sealed_btc_txid"),
            "expected_sealed_btc_vout_index": public_btc_anchor.get("sealed_btc_vout_index"),
            "expected_sealed_btc_amount_sats": public_btc_anchor.get("sealed_btc_amount_sats"),
            "expected_script_pubkey_hash": public_btc_anchor.get("script_pubkey_hash"),
            "expected_spend_input_index": public_btc_anchor.get("spend_input_index"),
            "expected_sealed_utxo_commitment_hash": public_btc_anchor.get("sealed_utxo_commitment_hash"),
            "template_case_hash": report_hash("template_case", template_case),
        }
        tx_profile = profile == "btc-transaction-commitment-profile-v0"
        utxo_profile = profile == "btc-utxo-seal-profile-v0"
        dual_profile = profile == "dual-seal-profile-v0"
        checks = {
            "service_builder_case_present": builder_case is not None,
            "template_case_present": template_case is not None,
            "scenario_matches_required_profile": request["scenario"] == REQUIRED_SCENARIOS[profile],
            "public_btc_spv_external_input_named": "public_btc_spv_evidence" in external_inputs,
            "minimum_confirmations_at_least_six": is_non_negative_int(request["minimum_confirmations"])
            and request["minimum_confirmations"] >= 6,
            "live_binding_hashes_present": is_hex32(request["ckb_live_tx_hash"]) and is_hex32(request["live_report_hash"]),
            "service_builder_hashes_present": is_hex32(request["service_builder_tx_skeleton_hash"])
            and is_hex32(request["service_builder_receipt_binding_hash"]),
            "expected_anchor_source_production_eligible": anchor_source_production_eligible(
                profile, request["expected_anchor_source"]
            ),
            "local_anchor_source_present": bool(request["local_anchor_source"]),
            "ckb_btc_commitment_hash_present": is_hex32(request["ckb_btc_commitment_hash"]),
            "expected_btc_txid_present": is_hex32(request["expected_btc_txid"]),
            "expected_btc_wtxid_present": is_hex32(request["expected_btc_wtxid"]),
            "expected_output_fields_present": (not tx_profile)
            or (
                is_non_negative_int(request["expected_btc_output_index"])
                and is_positive_int(request["expected_btc_amount_sats"])
            ),
            "expected_utxo_fields_present": (not utxo_profile)
            or (
                is_hex32(request["expected_sealed_btc_txid"])
                and is_non_negative_int(request["expected_sealed_btc_vout_index"])
                and is_positive_int(request["expected_sealed_btc_amount_sats"])
                and is_hex32(request["expected_script_pubkey_hash"])
                and is_non_negative_int(request["expected_spend_input_index"])
                and is_hex32(request["expected_sealed_utxo_commitment_hash"])
            ),
            "expected_dual_sealed_utxo_fields_present": (not dual_profile)
            or (
                is_hex32(request["expected_sealed_btc_txid"])
                and is_non_negative_int(request["expected_sealed_btc_vout_index"])
                and is_positive_int(request["expected_sealed_btc_amount_sats"])
                and is_hex32(request["expected_script_pubkey_hash"])
                and is_non_negative_int(request["expected_spend_input_index"])
                and is_hex32(request["expected_sealed_utxo_commitment_hash"])
            ),
            "required_public_fields_complete": len(request["required_public_fields"]) == len(REQUIRED_PUBLIC_FIELDS),
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
