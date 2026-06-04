#!/usr/bin/env python3
"""Run NovaSeal v0 fixture JSON files against a narrow source-model harness.

This is not a CKB VM transaction runner. It executes the explicit guard
semantics declared in `src/nova_state_type.cell` and attaches the current
audit-surface facts so the result cannot be mistaken for production evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from copy import deepcopy
from pathlib import Path
from typing import Any


SCHEMA = "novaseal-fixture-harness-report-v0.1"

DEFAULT_FIXTURES = Path("fixtures")
DEFAULT_SOURCE = Path("src/nova_state_type.cell")
DEFAULT_AUDIT_SURFACE = Path("target/novaseal-audit-surface.json")
DEFAULT_CANONICAL_VECTORS = Path("target/novaseal-canonical-vectors.json")
DEFAULT_BTC_VERIFIER_VECTORS = Path("target/novaseal-btc-verifier-vectors.json")
DEFAULT_WALLET_SIGNING_ALIGNMENT_REPORT = Path("target/novaseal-wallet-signing-alignment.json")
DEFAULT_BTC_VERIFIER_IPC_VECTORS = Path("target/novaseal-btc-verifier-ipc-vectors.json")
DEFAULT_BTC_VERIFIER_SHELL_REPORT = Path("target/novaseal-btc-verifier-shell-report.json")
DEFAULT_CKB_VM_CHILD_VERIFIER_REPORT = Path("target/novaseal-ckb-vm-child-verifier-report.json")
DEFAULT_PARENT_LOCK_ABI_PREFLIGHT_REPORT = Path("target/novaseal-parent-lock-abi-preflight.json")
DEFAULT_PARENT_LOCK_CKB_VM_REPORT = Path("target/novaseal-parent-lock-ckb-vm-report.json")
DEFAULT_STATE_TYPE_CKB_VM_REPORT = Path("target/novaseal-state-type-ckb-vm-report.json")
DEFAULT_COMBINED_TX_REPORT = Path("target/novaseal-combined-tx-report.json")
DEFAULT_OUTPUT = Path("target/novaseal-fixture-report.json")
TEST_AUTHORITY_PUBKEY = "0xc89fe99d72fcfa969434ddd87bb186a48213e9df3ec4b8a77042cf9559fc5765"
U64_MAX = (1 << 64) - 1

BASELINE = {
    "old_cell": {
        "version": 0,
        "btc_authority_hash": TEST_AUTHORITY_PUBKEY,
        "state_hash": "0xstate-old",
        "policy_hash": "0xpolicy",
        "latest_receipt_hash": "0xreceipt-root",
        "nonce": 42,
        "expiry": 999_999,
    },
    "intent": {
        "protocol_id": "0xnovaseal-domain",
        "package_hash": "0xpackage",
        "action": 1,
        "terminal_path": 0,
        "old_cell": "0xoutpoint",
        "old_state_hash": "0xstate-old",
        "new_state_hash": "0xstate-new",
        "policy_hash": "0xpolicy",
        "expected_receipt_hash": "0xreceipt",
        "old_nonce": 42,
        "new_nonce": 43,
        "expiry": 1_000,
    },
    "current_timepoint": 200,
    "actual_old_cell": "0xoutpoint",
    "btc_signature": "valid (source-model delegate success)",
    "btc_authority_pubkey_matches": True,
    "lock_args_authority_matches": True,
    "proposed_new_cell": {},
}

REQUIRED_SOURCE_SNIPPETS = [
    "let intent_core_hash = hash_blake2b_packed(intent.core)",
    "let signed_intent_hash = hash_blake2b_packed(intent)",
    "let materialized_receipt_hash = hash_blake2b_packed(receipt_commitment)",
    "require intent.core.old_cell.tx_hash == actual_old_tx_hash",
    "require intent.core.old_cell.index == actual_old_index",
    "require intent.core.old_state_hash == old_cell.state_hash",
    "require actual_state_hash_commitment == state_hash_commitment",
    "require intent.core.policy_hash == old_cell.policy_hash",
    "require sig.pubkey == old_cell.btc_authority_hash",
    "require intent.core.new_nonce == old_cell.nonce + 1",
    "require old_cell.nonce < U64_MAX",
    "require now <= intent.core.expiry",
    "require intent.expected_receipt_hash == materialized_receipt_hash",
    "verifier::btc::bip340::require_signature(signed_intent_hash, sig.pubkey, sig.signature)",
    "require sig.pubkey == cell.btc_authority_hash",
]


def model_blake2b_byte32(value: Any) -> str:
    h = hashlib.blake2b(digest_size=32, person=b"NovaSealModel")
    h.update(str(value).encode("utf-8"))
    return "0x" + h.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"missing JSON file: {path}") from None
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from None


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise SystemExit(f"missing source file: {path}") from None


def merge_dict(base: dict[str, Any], patch: dict[str, Any]) -> dict[str, Any]:
    merged = deepcopy(base)
    for key, value in patch.items():
        if isinstance(value, dict) and isinstance(merged.get(key), dict):
            merged[key] = merge_dict(merged[key], value)
        else:
            merged[key] = value
    return merged


def signature_ok(inputs: dict[str, Any]) -> bool:
    explicit = inputs.get("btc_signature_result")
    if isinstance(explicit, bool):
        return explicit
    text = str(inputs.get("btc_signature", BASELINE["btc_signature"])).lower()
    if "invalid" in text or "failure" in text or "reject" in text:
        return False
    if "valid" in text or "success" in text:
        return True
    return False


def normalise_fixture_inputs(fixture: dict[str, Any]) -> dict[str, Any]:
    raw_inputs = fixture.get("inputs", {})
    if not isinstance(raw_inputs, dict):
        raw_inputs = {}
    model = merge_dict(BASELINE, raw_inputs)
    old = model["old_cell"]
    intent = model["intent"]
    raw_intent = raw_inputs.get("intent", {})
    if not isinstance(raw_intent, dict):
        raw_intent = {}
    if "nonce" in raw_intent and "new_nonce" not in raw_intent:
        intent["new_nonce"] = raw_intent["nonce"]
    intent.setdefault("old_nonce", old["nonce"])
    intent.setdefault("new_nonce", old["nonce"] + 1)
    intent.setdefault("protocol_id", intent.get("domain", BASELINE["intent"]["protocol_id"]))
    intent.setdefault("package_hash", BASELINE["intent"]["package_hash"])
    intent.setdefault("terminal_path", BASELINE["intent"]["terminal_path"])
    if "receipt_hash" in raw_intent and "expected_receipt_hash" not in raw_intent:
        intent["expected_receipt_hash"] = raw_intent["receipt_hash"]
    materialized_receipt_hash = raw_inputs.get("actual_receipt_hash", intent["expected_receipt_hash"])
    state_hash_commitment = raw_inputs.get("state_hash_commitment", model_blake2b_byte32(intent["new_state_hash"]))
    model["materialized_receipt_hash"] = materialized_receipt_hash
    model["state_hash_commitment"] = state_hash_commitment
    model["signature_ok"] = signature_ok(raw_inputs)
    model["btc_authority_pubkey_matches"] = bool(raw_inputs.get("btc_authority_pubkey_matches", BASELINE["btc_authority_pubkey_matches"]))
    model["lock_args_authority_matches"] = bool(raw_inputs.get("lock_args_authority_matches", BASELINE["lock_args_authority_matches"]))
    return model


def outpoint_components(value: Any) -> tuple[Any, int]:
    if isinstance(value, dict):
        return value.get("tx_hash"), int(value.get("index", 0))
    return f"{value}:tx_hash", 0


def run_model(model: dict[str, Any]) -> dict[str, Any]:
    old = model["old_cell"]
    intent = model["intent"]
    now = model["current_timepoint"]

    checks = []

    def add(name: str, passed: bool, failure_mode: str) -> bool:
        checks.append({"name": name, "passed": passed, "failure_mode": None if passed else failure_mode})
        return passed

    if not add("btc_signature_delegate", bool(model["signature_ok"]), "btc_signature_verification_failed"):
        return rejected(checks)
    if not add("lock_args_authority_matches", bool(model["lock_args_authority_matches"]), "authority_hash_mapping_mismatch"):
        return rejected(checks)
    if not add("btc_authority_pubkey_bound", bool(model["btc_authority_pubkey_matches"]), "btc_authority_pubkey_mismatch"):
        return rejected(checks)
    intent_tx_hash, intent_index = outpoint_components(intent["old_cell"])
    actual_tx_hash, actual_index = outpoint_components(model.get("actual_old_cell", intent["old_cell"]))
    if not add("old_outpoint_tx_hash_matches", intent_tx_hash == actual_tx_hash, "old_outpoint_tx_hash_mismatch"):
        return rejected(checks)
    if not add("old_outpoint_index_matches", intent_index == actual_index, "old_outpoint_index_mismatch"):
        return rejected(checks)
    if not add("old_state_hash_matches", intent["old_state_hash"] == old["state_hash"], "state_hash_mismatch"):
        return rejected(checks)
    if not add(
        "state_hash_commitment_matches",
        model["state_hash_commitment"] == model_blake2b_byte32(intent["new_state_hash"]),
        "state_hash_commitment_mismatch",
    ):
        return rejected(checks)
    if not add("policy_hash_matches", intent["policy_hash"] == old["policy_hash"], "policy_hash_mismatch"):
        return rejected(checks)
    if not add("old_nonce_matches", intent["old_nonce"] == old["nonce"], "old_nonce_mismatch"):
        return rejected(checks)
    if not add("nonce_not_at_u64_max", int(old["nonce"]) < U64_MAX, "nonce_overflow"):
        return rejected(checks)
    if not add("nonce_increments", intent["new_nonce"] == old["nonce"] + 1, "nonce_must_increment"):
        return rejected(checks)
    if not add("intent_not_expired", now <= intent["expiry"], "intent_expired"):
        return rejected(checks)
    if not add(
        "receipt_hash_matches",
        model["materialized_receipt_hash"] == intent["expected_receipt_hash"],
        "receipt_hash_mismatch",
    ):
        return rejected(checks)
    proposed_new_cell = model.get("proposed_new_cell") if isinstance(model.get("proposed_new_cell"), dict) else {}
    proposed_authority = proposed_new_cell.get("btc_authority_hash", old["btc_authority_hash"])
    if not add("authority_not_rotated_implicitly", proposed_authority == old["btc_authority_hash"], "implicit_authority_rotation"):
        return rejected(checks)

    new_cell = {
        "version": old["version"],
        "btc_authority_hash": old["btc_authority_hash"],
        "state_hash": intent["new_state_hash"],
        "policy_hash": old["policy_hash"],
        "latest_receipt_hash": model["materialized_receipt_hash"],
        "nonce": intent["new_nonce"],
        "expiry": intent["expiry"],
    }
    return {"result": "accepted", "failure_mode": None, "checks": checks, "new_cell": new_cell}


def rejected(checks: list[dict[str, Any]]) -> dict[str, Any]:
    failed = next(check for check in checks if not check["passed"])
    return {"result": "rejected", "failure_mode": failed["failure_mode"], "checks": checks, "new_cell": None}


def artifact_checks(audit_surface: dict[str, Any]) -> dict[str, Any]:
    summary = audit_surface.get("summary", {})
    actions = audit_surface.get("actions", [])
    runtime_gaps = audit_surface.get("runtime_gaps", [])
    first_action = actions[0] if actions else {}
    runtime_accesses = first_action.get("runtime_accesses", [])
    has_old_consume = any(access.get("operation") == "consume" and access.get("binding") == "old_cell" for access in runtime_accesses)
    has_new_output = any(access.get("operation") == "output" and access.get("binding") == "new_cell" for access in runtime_accesses)
    return {
        "execution_level": "source_model_plus_audit_surface",
        "is_ckb_vm_execution": False,
        "actions": summary.get("actions"),
        "locks": summary.get("locks"),
        "runtime_gaps": [gap.get("feature") for gap in runtime_gaps],
        "consume_old_cell_visible": has_old_consume,
        "create_new_cell_visible": has_new_output,
        "authority_lock_generated_visible": summary.get("locks") not in (None, 0),
    }


def optional_json(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    return load_json(path)


def canonical_vector_checks(path: Path) -> dict[str, Any]:
    vectors = optional_json(path)
    if vectors is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "receipt_commitment_status": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": vectors.get("summary"),
        "receipt_commitment_status": vectors.get("receipt_commitment_analysis", {}).get("status"),
    }


def btc_verifier_vector_checks(path: Path) -> dict[str, Any]:
    vectors = optional_json(path)
    if vectors is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "scheme": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": vectors.get("summary"),
        "scheme": vectors.get("scheme"),
    }


def wallet_signing_alignment_checks(path: Path) -> dict[str, Any]:
    report = optional_json(path)
    if report is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "classification": None,
            "message_rules": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": report.get("summary"),
        "classification": report.get("classification"),
        "message_rules": report.get("message_rules"),
    }


def btc_verifier_ipc_vector_checks(path: Path) -> dict[str, Any]:
    vectors = optional_json(path)
    if vectors is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "ipc_contract": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": vectors.get("summary"),
        "ipc_contract": vectors.get("ipc_contract"),
    }


def btc_verifier_shell_checks(path: Path) -> dict[str, Any]:
    report = optional_json(path)
    if report is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "classification": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": report.get("summary"),
        "classification": report.get("classification"),
    }


def ckb_vm_child_verifier_checks(path: Path) -> dict[str, Any]:
    report = optional_json(path)
    if report is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "classification": None,
            "elf": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": report.get("summary"),
        "classification": report.get("classification"),
        "elf": report.get("elf"),
    }


def parent_lock_abi_preflight_checks(path: Path) -> dict[str, Any]:
    report = optional_json(path)
    if report is None:
        return {
            "artifact": str(path),
            "available": False,
            "classification": None,
            "status": None,
            "checks": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "classification": report.get("classification"),
        "status": report.get("status"),
        "checks": report.get("checks"),
    }


def parent_lock_ckb_vm_checks(path: Path) -> dict[str, Any]:
    report = optional_json(path)
    if report is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "classification": None,
            "parent_elf": None,
            "child_elf": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": report.get("summary"),
        "classification": report.get("classification"),
        "parent_elf": report.get("parent_elf"),
        "child_elf": report.get("child_elf"),
        "cases": report.get("cases"),
    }


def state_type_ckb_vm_checks(path: Path) -> dict[str, Any]:
    report = optional_json(path)
    if report is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "classification": None,
            "action_elf": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": report.get("summary"),
        "classification": report.get("classification"),
        "action_elf": report.get("action_elf"),
        "cases": report.get("cases"),
    }


def combined_tx_checks(path: Path) -> dict[str, Any]:
    report = optional_json(path)
    if report is None:
        return {
            "artifact": str(path),
            "available": False,
            "summary": None,
            "classification": None,
            "parent_elf": None,
            "type_elf": None,
            "child_elf": None,
        }
    return {
        "artifact": str(path),
        "available": True,
        "summary": report.get("summary"),
        "classification": report.get("classification"),
        "parent_elf": report.get("parent_elf"),
        "type_elf": report.get("type_elf"),
        "child_elf": report.get("child_elf"),
        "cases": report.get("cases"),
    }


def source_guard_checks(source_text: str) -> dict[str, Any]:
    missing = [snippet for snippet in REQUIRED_SOURCE_SNIPPETS if snippet not in source_text]
    return {
        "required_snippets": REQUIRED_SOURCE_SNIPPETS,
        "missing_snippets": missing,
        "all_present": not missing,
    }


def fixture_paths(fixtures_dir: Path) -> list[Path]:
    return sorted(fixtures_dir.glob("*.json"))


def run_fixture(path: Path) -> dict[str, Any]:
    fixture = load_json(path)
    model = normalise_fixture_inputs(fixture)
    actual = run_model(model)
    expected = fixture.get("expected", {})
    if not isinstance(expected, dict):
        expected = {}
    expected_result = expected.get("result")
    expected_failure = expected.get("failure_mode")
    matched = actual["result"] == expected_result and (
        actual["result"] == "accepted" or actual["failure_mode"] == expected_failure
    )
    return {
        "fixture": path.name,
        "name": fixture.get("name", path.stem),
        "category": fixture.get("category"),
        "criteria": fixture.get("acceptance_criteria_covered", []),
        "expected": {"result": expected_result, "failure_mode": expected_failure},
        "actual": actual,
        "matched": matched,
    }


def build_report(
    fixtures_dir: Path,
    source_path: Path,
    audit_surface_path: Path,
    canonical_vectors_path: Path,
    btc_verifier_vectors_path: Path,
    wallet_signing_alignment_report_path: Path,
    btc_verifier_ipc_vectors_path: Path,
    btc_verifier_shell_report_path: Path,
    ckb_vm_child_verifier_report_path: Path,
    parent_lock_abi_preflight_report_path: Path,
    parent_lock_ckb_vm_report_path: Path,
    state_type_ckb_vm_report_path: Path,
    combined_tx_report_path: Path,
) -> dict[str, Any]:
    source_text = read_text(source_path)
    audit_surface = load_json(audit_surface_path)
    results = [run_fixture(path) for path in fixture_paths(fixtures_dir)]
    passed = [result for result in results if result["matched"]]
    failed = [result for result in results if not result["matched"]]
    criteria_seen = sorted({criterion for result in results for criterion in result.get("criteria", [])})
    child_checks = ckb_vm_child_verifier_checks(ckb_vm_child_verifier_report_path)
    child_summary = child_checks.get("summary", {})
    if not isinstance(child_summary, dict):
        child_summary = {}
    parent_preflight_checks = parent_lock_abi_preflight_checks(parent_lock_abi_preflight_report_path)
    parent_preflight_status = parent_preflight_checks.get("status", {})
    if not isinstance(parent_preflight_status, dict):
        parent_preflight_status = {}
    parent_vm_checks = parent_lock_ckb_vm_checks(parent_lock_ckb_vm_report_path)
    parent_vm_summary = parent_vm_checks.get("summary", {})
    if not isinstance(parent_vm_summary, dict):
        parent_vm_summary = {}
    parent_vm_cases = parent_vm_checks.get("cases", [])
    if not isinstance(parent_vm_cases, list):
        parent_vm_cases = []
    state_type_checks = state_type_ckb_vm_checks(state_type_ckb_vm_report_path)
    state_type_summary = state_type_checks.get("summary", {})
    if not isinstance(state_type_summary, dict):
        state_type_summary = {}
    state_type_cases = state_type_checks.get("cases", [])
    if not isinstance(state_type_cases, list):
        state_type_cases = []
    parent_witness_sizes = {
        case.get("transaction_shape", {}).get("witness_size_bytes")
        for case in parent_vm_cases
        if isinstance(case, dict) and isinstance(case.get("transaction_shape"), dict)
    }
    state_witness_sizes = {
        case.get("witness_size_bytes")
        for case in state_type_cases
        if isinstance(case, dict)
    }
    parent_witness_sizes.discard(None)
    state_witness_sizes.discard(None)
    shared_witness_sizes = parent_witness_sizes & state_witness_sizes
    combined_checks = combined_tx_checks(combined_tx_report_path)
    combined_summary = combined_checks.get("summary", {})
    if not isinstance(combined_summary, dict):
        combined_summary = {}
    wallet_alignment_checks = wallet_signing_alignment_checks(wallet_signing_alignment_report_path)
    wallet_alignment_summary = wallet_alignment_checks.get("summary", {})
    if not isinstance(wallet_alignment_summary, dict):
        wallet_alignment_summary = {}
    return {
        "schema": SCHEMA,
        "fixture_dir": str(fixtures_dir),
        "source": str(source_path),
        "audit_surface": str(audit_surface_path),
        "summary": {
            "fixtures": len(results),
            "matched": len(passed),
            "mismatched": len(failed),
            "criteria_seen": criteria_seen,
            "ckb_vm_executed": False,
            "child_verifier_ckb_vm_executed": bool(child_summary.get("child_verifier_ckb_vm_executed")),
            "parent_lock_abi_preflight_passed": bool(parent_preflight_status.get("preflight_passed")),
            "parent_lock_ckb_vm_executed": bool(parent_vm_summary.get("parent_lock_ckb_vm_executed")),
            "parent_lock_spawn_executed": bool(parent_vm_summary.get("parent_spawn_executed")),
            "parent_lock_transaction_shape_constructed": bool(parent_vm_summary.get("transaction_shape_constructed")),
            "parent_lock_consensus_packed_tx_constructed": bool(parent_vm_summary.get("consensus_packed_tx_constructed")),
            "parent_lock_resolved_transaction_constructed": bool(parent_vm_summary.get("resolved_transaction_constructed")),
            "parent_lock_resolved_script_verifier_executed": bool(parent_vm_summary.get("resolved_script_verifier_executed")),
            "parent_lock_resolved_script_verifier_matched_expected": bool(
                parent_vm_summary.get("resolved_script_verifier_matched_expected")
            ),
            "parent_lock_resolved_script_verifier_max_cycles": parent_vm_summary.get("resolved_script_verifier_max_cycles"),
            "parent_lock_full_transaction_executed": bool(parent_vm_summary.get("full_transaction_executed")),
            "parent_lock_full_transaction_verifier_matched_expected": bool(
                parent_vm_summary.get("full_transaction_verifier_matched_expected")
            ),
            "parent_lock_full_transaction_verifier_max_cycles": parent_vm_summary.get(
                "full_transaction_verifier_max_cycles"
            ),
            "parent_lock_max_consensus_tx_size_bytes": parent_vm_summary.get("max_consensus_tx_size_bytes"),
            "parent_lock_max_output_occupied_capacity_shannons": parent_vm_summary.get(
                "max_output_occupied_capacity_shannons"
            ),
            "parent_lock_capacity_shape_checks_passed": bool(parent_vm_summary.get("capacity_shape_checks_passed")),
            "parent_lock_under_capacity_shape_rejects": bool(parent_vm_summary.get("under_capacity_shape_rejects")),
            "parent_child_ckb_vm_matched_expected": parent_vm_summary.get("matched_expected") == parent_vm_summary.get("total_cases")
            if parent_vm_summary
            else False,
            "state_type_action_ckb_vm_executed": bool(state_type_summary.get("state_type_action_ckb_vm_executed")),
            "state_type_action_matched_expected": state_type_summary.get("state_type_matched_expected")
            == state_type_summary.get("total_cases")
            if state_type_summary
            else False,
            "state_type_source_fixture_matched_by_state_type_only": state_type_summary.get(
                "source_fixture_matched_by_state_type_only"
            ),
            "state_type_source_fixture_requires_lock_or_external_context": state_type_summary.get(
                "source_fixture_requires_lock_or_external_context"
            ),
            "state_type_schema_cell_intent_mismatch_detected": bool(
                state_type_summary.get("schema_cell_intent_mismatch_detected")
            ),
            "state_type_schema_cell_intent_aligned": bool(state_type_summary.get("schema_cell_intent_aligned")),
            "shared_lock_type_witness_abi": "CSARGv1:NovaSealSignedIntentV0,state_hash_commitment,SignaturePayload",
            "shared_lock_type_witness_abi_aligned": bool(shared_witness_sizes),
            "shared_lock_type_witness_size_bytes": min(shared_witness_sizes) if shared_witness_sizes else None,
            "combined_full_transaction_executed": bool(combined_summary.get("combined_full_transaction_executed")),
            "combined_full_transaction_matched_expected": combined_summary.get("matched_expected")
            == combined_summary.get("total_cases")
            if combined_summary
            else False,
            "combined_full_transaction_total_cases": combined_summary.get("total_cases"),
            "combined_full_transaction_accepted": combined_summary.get("accepted"),
            "combined_full_transaction_rejected": combined_summary.get("rejected"),
            "combined_lock_and_type_script_groups_present": bool(combined_summary.get("lock_and_type_script_groups_present")),
            "combined_shared_witness_abi_aligned": bool(combined_summary.get("shared_witness_abi_aligned")),
            "combined_builder_shape_checks_passed": bool(combined_summary.get("builder_shape_checks_passed")),
            "combined_fee_shape_checks_passed": bool(combined_summary.get("fee_shape_checks_passed")),
            "combined_under_capacity_shape_rejects": bool(combined_summary.get("under_capacity_shape_rejects")),
            "combined_min_fee_shannons": combined_summary.get("min_fee_shannons"),
            "combined_max_fee_shannons": combined_summary.get("max_fee_shannons"),
            "combined_full_transaction_max_cycles": combined_summary.get("max_full_transaction_cycles"),
            "combined_max_consensus_tx_size_bytes": combined_summary.get("max_consensus_tx_size_bytes"),
            "combined_max_output_occupied_capacity_shannons": combined_summary.get("max_output_occupied_capacity_shannons"),
            "wallet_signing_alignment_report_available": bool(wallet_alignment_checks.get("available")),
            "wallet_lock_alignment_ready": bool(wallet_alignment_summary.get("wallet_lock_alignment_ready")),
            "wallet_current_lock_digest_matches_canonical": wallet_alignment_summary.get(
                "current_lock_digest_matches_canonical"
            ),
            "wallet_current_lock_digest_mismatches": wallet_alignment_summary.get("current_lock_digest_mismatches"),
            "classification": "model_level_fixture_evidence",
        },
        "artifact_checks": artifact_checks(audit_surface),
        "canonical_vector_checks": canonical_vector_checks(canonical_vectors_path),
        "btc_verifier_vector_checks": btc_verifier_vector_checks(btc_verifier_vectors_path),
        "wallet_signing_alignment_checks": wallet_alignment_checks,
        "btc_verifier_ipc_vector_checks": btc_verifier_ipc_vector_checks(btc_verifier_ipc_vectors_path),
        "btc_verifier_shell_checks": btc_verifier_shell_checks(btc_verifier_shell_report_path),
        "ckb_vm_child_verifier_checks": child_checks,
        "parent_lock_abi_preflight_checks": parent_preflight_checks,
        "parent_lock_ckb_vm_checks": parent_vm_checks,
        "state_type_ckb_vm_checks": state_type_checks,
        "combined_tx_checks": combined_checks,
        "source_guard_checks": source_guard_checks(source_text),
        "results": results,
        "limitations": [
            "The source-model portion of this fixture harness does not execute the parent lock in CKB VM or construct a transaction.",
            "BTC signature verification in the source-model portion is represented by fixture-declared delegate success/failure.",
            "Child-verifier CKB VM evidence is attached separately when target/novaseal-ckb-vm-child-verifier-report.json exists; it is not per-fixture parent-lock execution.",
            "Parent-lock ELF/ASM ABI preflight is attached separately when target/novaseal-parent-lock-abi-preflight.json exists; it is not parent-lock CKB VM execution.",
            "Parent-lock CKB VM evidence is attached separately when target/novaseal-parent-lock-ckb-vm-report.json exists; it now includes consensus-packed transaction shape, tx-size, occupied-capacity, under-capacity shape checks, resolved ckb-script lock-group verifier execution, and full ckb-script transaction script verification for the four parent authority cases, but it is not the full fixture transaction runner.",
            "State-type CKB VM evidence is attached separately when target/novaseal-state-type-ckb-vm-report.json exists; it executes the key_auth_transition action over the fixture set at action/type scope, not lock scope.",
            "The state-type CKB VM harness uses the canonical 213-byte NovaSealIntentV0 old_cell: OutPoint shape without an intent-shortening adapter.",
            "The parent-lock and state-type CKB VM harnesses now parse the same CSARGv1 witness payload order: intent, receipt_hash, state_hash_commitment, SignaturePayload.",
            "Combined lock+type full transaction script-verifier evidence is attached separately when target/novaseal-combined-tx-report.json exists; it is still an in-memory harness ResolvedTransaction flow, not production builder/full-node acceptance.",
            "Wallet signing alignment evidence is attached separately when target/novaseal-wallet-signing-alignment.json exists; it currently keeps production_wallet_ready=false when the lock digest is still the domain-hash compatibility rule.",
            "The generated authority lock surface covers Script.args binding and spawn/IPC shell wiring; parent-lock CKB VM evidence is still harness-level, not generated ProofPlan transaction coverage.",
            "Passing source-model fixtures do not replace builder-backed/full-node acceptance evidence.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, default=DEFAULT_FIXTURES)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--audit-surface", type=Path, default=DEFAULT_AUDIT_SURFACE)
    parser.add_argument("--canonical-vectors", type=Path, default=DEFAULT_CANONICAL_VECTORS)
    parser.add_argument("--btc-verifier-vectors", type=Path, default=DEFAULT_BTC_VERIFIER_VECTORS)
    parser.add_argument("--wallet-signing-alignment-report", type=Path, default=DEFAULT_WALLET_SIGNING_ALIGNMENT_REPORT)
    parser.add_argument("--btc-verifier-ipc-vectors", type=Path, default=DEFAULT_BTC_VERIFIER_IPC_VECTORS)
    parser.add_argument("--btc-verifier-shell-report", type=Path, default=DEFAULT_BTC_VERIFIER_SHELL_REPORT)
    parser.add_argument("--ckb-vm-child-verifier-report", type=Path, default=DEFAULT_CKB_VM_CHILD_VERIFIER_REPORT)
    parser.add_argument("--parent-lock-abi-preflight-report", type=Path, default=DEFAULT_PARENT_LOCK_ABI_PREFLIGHT_REPORT)
    parser.add_argument("--parent-lock-ckb-vm-report", type=Path, default=DEFAULT_PARENT_LOCK_CKB_VM_REPORT)
    parser.add_argument("--state-type-ckb-vm-report", type=Path, default=DEFAULT_STATE_TYPE_CKB_VM_REPORT)
    parser.add_argument("--combined-tx-report", type=Path, default=DEFAULT_COMBINED_TX_REPORT)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report(
        args.fixtures,
        args.source,
        args.audit_surface,
        args.canonical_vectors,
        args.btc_verifier_vectors,
        args.wallet_signing_alignment_report,
        args.btc_verifier_ipc_vectors,
        args.btc_verifier_shell_report,
        args.ckb_vm_child_verifier_report,
        args.parent_lock_abi_preflight_report,
        args.parent_lock_ckb_vm_report,
        args.state_type_ckb_vm_report,
        args.combined_tx_report,
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(report, indent=indent, sort_keys=True) + "\n", encoding="utf-8")

    summary = report["summary"]
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"fixtures={summary['fixtures']} "
        f"matched={summary['matched']} "
        f"mismatched={summary['mismatched']} "
        f"ckb_vm_executed={summary['ckb_vm_executed']} "
        f"child_verifier_ckb_vm_executed={summary['child_verifier_ckb_vm_executed']} "
        f"parent_lock_abi_preflight_passed={summary['parent_lock_abi_preflight_passed']} "
        f"parent_lock_ckb_vm_executed={summary['parent_lock_ckb_vm_executed']} "
        f"parent_lock_spawn_executed={summary['parent_lock_spawn_executed']} "
        f"parent_lock_tx_shape_constructed={summary['parent_lock_transaction_shape_constructed']} "
        f"parent_lock_resolved_script_verifier_executed={summary['parent_lock_resolved_script_verifier_executed']} "
        f"parent_lock_resolved_script_verifier_matched_expected={summary['parent_lock_resolved_script_verifier_matched_expected']} "
        f"parent_lock_full_tx_executed={summary['parent_lock_full_transaction_executed']} "
        f"parent_lock_full_tx_matched_expected={summary['parent_lock_full_transaction_verifier_matched_expected']} "
        f"state_type_vm_executed={summary['state_type_action_ckb_vm_executed']} "
        f"state_type_matched_expected={summary['state_type_action_matched_expected']} "
        f"shared_witness_abi_aligned={summary['shared_lock_type_witness_abi_aligned']} "
        f"combined_full_tx_executed={summary['combined_full_transaction_executed']} "
        f"combined_full_tx_matched_expected={summary['combined_full_transaction_matched_expected']} "
        f"wallet_lock_alignment_ready={summary['wallet_lock_alignment_ready']}"
    )
    return 0 if summary["mismatched"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
