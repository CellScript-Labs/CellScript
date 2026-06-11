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
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BTC_SPV_ADAPTER = ROOT / "target/novaseal-btc-spv-evidence-adapter.json"
DEFAULT_EXTERNAL_ATTESTATION_ADAPTER = ROOT / "target/novaseal-external-attestation-adapter.json"
DEFAULT_OUTPUT = ROOT / "target/novaseal-external-evidence-handoff-bundle.json"

REPORT_PERSON = b"NovaExtHandoff"
HANDOFF_HASH_ALGORITHM = "blake2b-256(person=NovaExtHandoff)"
HANDOFF_SELF_HASH_FIELDS = ("bundle_hash", "bundle_hash_algorithm")

PUBLIC_BTC_SPV_EVIDENCE = "proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json"
PUBLIC_CELLDEP_ATTESTATION = "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.json"
EXTERNAL_TCB_ATTESTATION = "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json"
RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE = (
    "proposals/novaseal/rwa-receipt-profile-v0/proofs/legal_registry_review_evidence.json"
)

REQUIRED_BTC_SPV_PROFILES = [
    "btc-transaction-commitment-profile-v0",
    "btc-utxo-seal-profile-v0",
    "dual-seal-profile-v0",
]
PRODUCTION_BTC_ANCHOR_SOURCES = {
    "btc-transaction-commitment-profile-v0": "external_public_btc_transaction",
    "btc-utxo-seal-profile-v0": "external_public_btc_spend",
    "dual-seal-profile-v0": "external_public_btc_spend",
}

BTC_SPV_COMMON_BINDING_REQUEST_FIELDS = {
    "anchor_source": "expected_anchor_source",
    "btc_txid": "expected_btc_txid",
    "btc_wtxid": "expected_btc_wtxid",
}
BTC_SPV_PROFILE_BINDING_REQUEST_FIELDS = {
    "btc-transaction-commitment-profile-v0": {
        "btc_output_index": "expected_btc_output_index",
        "btc_amount_sats": "expected_btc_amount_sats",
    },
    "btc-utxo-seal-profile-v0": {
        "spend_input_index": "expected_spend_input_index",
        "sealed_btc_txid": "expected_sealed_btc_txid",
        "sealed_btc_vout_index": "expected_sealed_btc_vout_index",
        "sealed_btc_amount_sats": "expected_sealed_btc_amount_sats",
        "script_pubkey_hash": "expected_script_pubkey_hash",
        "sealed_utxo_commitment_hash": "expected_sealed_utxo_commitment_hash",
    },
    "dual-seal-profile-v0": {
        "spend_input_index": "expected_spend_input_index",
    },
}

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

REQUIRED_RWA_LEGAL_REVIEW_FIELDS = [
    "profile",
    "reviewer",
    "review_date",
    "review_scope",
    "registry.authority",
    "registry.jurisdiction",
    "registry.registry_report_hash",
    "profile_source_tree_sha256",
    "report_uri",
    "request_handoff.bundle",
    "request_handoff.bundle_hash",
    "request_handoff.bundle_hash_algorithm",
    "request_handoff.group",
]

RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS = [
    "proposals/novaseal/rwa-receipt-profile-v0/Cell.toml",
    "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_type.cell",
    "proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell",
    "proposals/novaseal/rwa-receipt-profile-v0/schemas",
    "proposals/novaseal/rwa-receipt-profile-v0/fixtures",
    "proposals/novaseal/rwa-receipt-profile-v0/proofs/invariant_matrix.json",
]

RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS = {
    "profile": "rwa-receipt-profile-v0",
    "reviewer": "real external legal or registry reviewer identity; placeholder, local/devnet/fake/internal, example, and unknown tokens are rejected",
    "review_date": "UTC date in YYYY-MM-DD form; future dates are rejected",
    "review_scope": "exact RWA receipt legal-title, custody, registry-state, oracle-fact, and enforceability review scope",
    "registry.authority": "real registry or custodian authority identity; placeholder, local/devnet/fake/internal, example, and unknown tokens are rejected",
    "registry.jurisdiction": "explicit real-world jurisdiction; placeholder, local/devnet/fake/internal, example, and unknown tokens are rejected",
    "registry.registry_report_hash": "0x-prefixed 32-byte non-placeholder hash of the external registry/legal review report",
    "profile_source_tree_sha256": "0x-prefixed 32-byte non-placeholder SHA-256 hash of the RWA profile source tree",
    "report_uri": "HTTPS URI for the public legal/registry review report or source-controlled review commit; example, loopback, private, and reserved hosts are rejected",
    "request_handoff.bundle": "target/novaseal-external-evidence-handoff-bundle.json",
    "request_handoff.bundle_hash": "0x-prefixed 32-byte hash of the NovaSeal external evidence handoff bundle",
    "request_handoff.bundle_hash_algorithm": "blake2b-256(person=NovaExtHandoff)",
    "request_handoff.group": "rwa_legal_registry_review_evidence",
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


def is_non_placeholder_hex32(value: Any) -> bool:
    return is_hex32(value) and not value[2:].lower() == "00" * 32


def is_non_negative_int(value: Any) -> bool:
    return type(value) is int and value >= 0


def is_positive_int(value: Any) -> bool:
    return type(value) is int and value > 0


def handoff_reference_hash(value: dict[str, Any]) -> str:
    payload = {key: item for key, item in value.items() if key not in HANDOFF_SELF_HASH_FIELDS}
    return report_hash("external_evidence_handoff_bundle", payload)


def source_tree_hash(paths: list[str]) -> str:
    files: set[Path] = set()
    allowed_suffixes = {".cell", ".schema", ".toml", ".py", ".json", ".rs"}
    for raw in paths:
        path = ROOT / raw
        if path.is_symlink():
            raise ValueError(f"source tree path must not be a symlink: {path.relative_to(ROOT)}")
        if path.is_file():
            files.add(path)
        elif path.is_dir():
            for child in path.rglob("*"):
                rel_parts = child.relative_to(path).parts
                if any(part in {"target", "build", ".git", "__pycache__"} for part in rel_parts):
                    continue
                if child.is_symlink():
                    raise ValueError(f"source tree path must not be a symlink: {child.relative_to(ROOT)}")
                if child.is_file() and (child.name == "Cargo.lock" or child.suffix in allowed_suffixes):
                    files.add(child)
    h = hashlib.sha256()
    for path in sorted(files):
        rel_path = str(path.relative_to(ROOT))
        h.update(rel_path.encode("utf-8"))
        h.update(b"\x00")
        h.update(hashlib.sha256(path.read_bytes()).digest())
    return hex0x(h.digest())


def required_field_set(case: dict[str, Any]) -> set[str]:
    fields = case.get("request", {}).get("required_public_fields", [])
    return {field for field in fields if isinstance(field, str)}


def expected_btc_binding_fields(profile: str) -> set[str]:
    return {
        "ckb_live_tx_hash",
        "live_report_hash",
        "service_builder_case_hash",
        "service_builder_tx_skeleton_hash",
        "service_builder_receipt_binding_hash",
        "ckb_btc_commitment_hash",
        *BTC_SPV_COMMON_BINDING_REQUEST_FIELDS.keys(),
        *BTC_SPV_PROFILE_BINDING_REQUEST_FIELDS.get(profile, {}).keys(),
    }


def btc_binding_value_valid(profile: str, field: str, value: Any) -> bool:
    if field in {
        "ckb_live_tx_hash",
        "live_report_hash",
        "service_builder_case_hash",
        "service_builder_tx_skeleton_hash",
        "service_builder_receipt_binding_hash",
        "ckb_btc_commitment_hash",
        "btc_txid",
        "btc_wtxid",
        "sealed_btc_txid",
        "script_pubkey_hash",
        "sealed_utxo_commitment_hash",
    }:
        return is_non_placeholder_hex32(value)
    if field == "anchor_source":
        return isinstance(value, str) and value == PRODUCTION_BTC_ANCHOR_SOURCES.get(profile)
    if field in {
        "spend_input_index",
        "sealed_btc_vout_index",
        "btc_output_index",
    }:
        return is_non_negative_int(value)
    if field in {
        "btc_amount_sats",
        "sealed_btc_amount_sats",
    }:
        return is_positive_int(value)
    return False


def btc_spv_handoff_case(adapter: dict[str, Any]) -> dict[str, Any]:
    cases = adapter.get("cases", [])
    profiles = {case.get("profile") for case in cases}
    expected_scenarios = {
        case.get("profile"): case.get("request", {}).get("scenario")
        for case in cases
        if isinstance(case.get("profile"), str) and isinstance(case.get("request", {}).get("scenario"), str)
    }
    expected_case_bindings = {}
    for case in cases:
        profile = case.get("profile")
        if not isinstance(profile, str):
            continue
        request = case.get("request", {})
        binding = {
            "ckb_live_tx_hash": case.get("request", {}).get("ckb_live_tx_hash"),
            "live_report_hash": case.get("request", {}).get("live_report_hash"),
            "service_builder_case_hash": case.get("request", {}).get("service_builder_case_hash"),
            "service_builder_tx_skeleton_hash": case.get("request", {}).get("service_builder_tx_skeleton_hash"),
            "service_builder_receipt_binding_hash": case.get("request", {}).get("service_builder_receipt_binding_hash"),
            "ckb_btc_commitment_hash": request.get("ckb_btc_commitment_hash"),
        }
        for output_field, request_field in {
            **BTC_SPV_COMMON_BINDING_REQUEST_FIELDS,
            **BTC_SPV_PROFILE_BINDING_REQUEST_FIELDS.get(profile, {}),
        }.items():
            if request.get(request_field) is not None:
                binding[output_field] = request[request_field]
        expected_case_bindings[profile] = binding
    checks = {
        "source_adapter_passed": adapter.get("status") == "passed",
        "source_adapter_status_request_ready": adapter.get("adapter_status") == "request_ready_external_evidence_required",
        "production_output_matches": adapter.get("production_output") == PUBLIC_BTC_SPV_EVIDENCE,
        "summary_counts_match": adapter.get("summary", {}).get("total") == len(REQUIRED_BTC_SPV_PROFILES)
        and adapter.get("summary", {}).get("matched") == adapter.get("summary", {}).get("total"),
        "required_profiles_complete": profiles == set(REQUIRED_BTC_SPV_PROFILES),
        "expected_scenarios_complete": set(expected_scenarios) == set(REQUIRED_BTC_SPV_PROFILES)
        and all(expected_scenarios.values()),
        "expected_case_bindings_complete": set(expected_case_bindings) == set(REQUIRED_BTC_SPV_PROFILES)
        and all(
            set(binding.keys()) == expected_btc_binding_fields(profile)
            and all(btc_binding_value_valid(profile, field, value) for field, value in binding.items())
            for profile, binding in expected_case_bindings.items()
        ),
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
        "expected_scenarios": expected_scenarios,
        "expected_case_bindings": expected_case_bindings,
        "required_external_fields": [
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
        ],
        "field_constraints": {
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
            "btc_transaction_binding.sealed_btc_txid": "sealed Bitcoin transaction id whose output is spent; required for btc-utxo-seal-profile-v0",
            "btc_transaction_binding.sealed_btc_vout_index": "sealed Bitcoin output index; required for btc-utxo-seal-profile-v0",
            "btc_transaction_binding.sealed_btc_amount_sats": "sealed Bitcoin output amount in sats; required for btc-utxo-seal-profile-v0",
            "btc_transaction_binding.script_pubkey_hash": "0x-prefixed CKB Blake2b-256 hash of the sealed output scriptPubKey bytes; required for btc-utxo-seal-profile-v0",
            "btc_transaction_binding.sealed_btc_tx_hex": "0x-prefixed raw sealed Bitcoin transaction bytes; required for btc-utxo-seal-profile-v0",
            "btc_transaction_binding.sealed_utxo_commitment_hash": "0x-prefixed 32-byte CKB-side sealed UTXO commitment hash; required for btc-utxo-seal-profile-v0",
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
        },
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
    request = source_case.get("request", {})
    fields = required_field_set(source_case)
    checks = {
        "source_adapter_passed": adapter.get("status") == "passed",
        "source_adapter_status_request_ready": adapter.get("adapter_status") == "request_ready_external_attestations_required",
        "source_case_passed": source_case.get("status") == "passed",
        "production_output_matches": request.get("production_output") == production_output,
        "required_fields_complete": set(required_fields).issubset(fields),
    }
    expected_values = {}
    if request.get("expected_release_package"):
        expected_values["release.package"] = request["expected_release_package"]
    if request.get("expected_release_version"):
        expected_values["release.version"] = request["expected_release_version"]
    if request.get("expected_release_manifest_commit"):
        expected_values["release.manifest_commit"] = request["expected_release_manifest_commit"]
    if request.get("expected_dep_type"):
        expected_values["runtime_verifier.dep_type"] = request["expected_dep_type"]
    if request.get("expected_hash_type"):
        expected_values["runtime_verifier.hash_type"] = request["expected_hash_type"]
    if case_name == "public_shared_cell_dep_attestation" and request.get("ipc_abi"):
        expected_values["runtime_verifier.ipc_abi"] = request["ipc_abi"]
    if case_name == "public_shared_cell_dep_attestation" and request.get("verifier_id"):
        expected_values["runtime_verifier.verifier_id"] = request["verifier_id"]
    if case_name == "external_bip340_tcb_review_attestation" and request.get("ipc_abi"):
        expected_values["ipc_abi"] = request["ipc_abi"]
    if case_name == "external_bip340_tcb_review_attestation" and request.get("verifier_id"):
        expected_values["verifier_id"] = request["verifier_id"]
    if request.get("expected_artifact_hash"):
        expected_values["artifact_hash"] = request["expected_artifact_hash"]
    if request.get("expected_artifact_hash_algorithm"):
        expected_values["artifact_hash_algorithm"] = request["expected_artifact_hash_algorithm"]
    if request.get("expected_review_scope"):
        expected_values["review_scope"] = request["expected_review_scope"]
    if request.get("expected_source_tree_sha256"):
        expected_values["source_tree_sha256"] = request["expected_source_tree_sha256"]

    result = {
        "group": group,
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "source_adapter": str(DEFAULT_EXTERNAL_ATTESTATION_ADAPTER.relative_to(ROOT)),
        "source_adapter_hash": report_hash("external_attestation_adapter", adapter),
        "source_case": case_name,
        "production_output": production_output,
        "required_external_fields": required_fields,
        "field_constraints": source_case.get("request", {}).get("field_constraints", {}),
    }
    if expected_values:
        result["expected_values"] = expected_values
    return result


def rwa_legal_registry_review_case(external_attestation_adapter: dict[str, Any]) -> dict[str, Any]:
    source_hash = source_tree_hash(RWA_LEGAL_REVIEW_SOURCE_HASH_PATHS)
    checks = {
        "source_external_attestation_adapter_passed": external_attestation_adapter.get("status") == "passed",
        "source_external_attestation_adapter_status_request_ready": external_attestation_adapter.get("adapter_status")
        == "request_ready_external_attestations_required",
        "production_output_matches": RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE.endswith(
            "legal_registry_review_evidence.json"
        ),
        "profile_source_tree_hash_current": len(source_hash) == 66 and source_hash.startswith("0x"),
    }
    return {
        "group": "rwa_legal_registry_review_evidence",
        "status": "passed" if all(checks.values()) else "failed",
        "checks": checks,
        "source_adapter": str(DEFAULT_EXTERNAL_ATTESTATION_ADAPTER.relative_to(ROOT)),
        "source_adapter_hash": report_hash("external_attestation_adapter", external_attestation_adapter),
        "production_output": RWA_LEGAL_REGISTRY_REVIEW_EVIDENCE,
        "required_external_fields": REQUIRED_RWA_LEGAL_REVIEW_FIELDS,
        "field_constraints": RWA_LEGAL_REVIEW_FIELD_CONSTRAINTS,
        "expected_values": {
            "profile": "rwa-receipt-profile-v0",
            "profile_source_tree_sha256": source_hash,
            "review_scope": [
                "RWA receipt legal title boundary",
                "RWA receipt custody and registry-state provenance",
                "RWA receipt oracle-fact exclusion boundary",
                "RWA receipt enforceability and jurisdiction boundary",
            ],
        },
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
        rwa_legal_registry_review_case(external_attestation_adapter),
    ]
    production_outputs = [case["production_output"] for case in cases]
    status = "passed" if all(case["status"] == "passed" for case in cases) else "failed"
    report = {
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
    report["bundle_hash_algorithm"] = HANDOFF_HASH_ALGORITHM
    report["bundle_hash"] = handoff_reference_hash(report)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--btc-spv-adapter", type=Path, default=DEFAULT_BTC_SPV_ADAPTER)
    parser.add_argument("--external-attestation-adapter", type=Path, default=DEFAULT_EXTERNAL_ATTESTATION_ADAPTER)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    btc_spv_adapter = json.loads(args.btc_spv_adapter.read_text(encoding="utf-8"))
    external_attestation_adapter = json.loads(args.external_attestation_adapter.read_text(encoding="utf-8"))
    try:
        report = build_report(btc_spv_adapter, external_attestation_adapter)
    except ValueError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
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
