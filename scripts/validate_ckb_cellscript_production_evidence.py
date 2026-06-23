#!/usr/bin/env python3
"""Validate CKB CellScript production acceptance evidence before release."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from typing import Any


SOURCE_PROVENANCE_SCHEMA = "cellscript-ckb-acceptance-source-provenance-v0.1"
BUILD_REPORT_SCHEMA = "cellscript-ckb-build-report-v0.20"
SOURCE_PROVENANCE_PATHS = [
    "Cargo.lock",
    "Cargo.toml",
    "src",
    "examples",
    "scripts/cellscript_gate.sh",
    "scripts/cellscript_ckb_release_gate.sh",
    "scripts/ckb_cellscript_acceptance.sh",
    "scripts/validate_ckb_cellscript_production_evidence.py",
]

EXPECTED_EXAMPLES = [
    "amm_pool.cell",
    "launch.cell",
    "multisig.cell",
    "nft.cell",
    "timelock.cell",
    "token.cell",
    "vesting.cell",
]
EXPECTED_NON_PRODUCTION_EXAMPLES = ["registry.cell"]
EXPECTED_LANGUAGE_EXAMPLES = [
    "canonical_style.cell",
    "order_book.cell",
    "registry.cell",
    "stdlib.cell",
    "v0_14_capacity_time.cell",
    "v0_14_ckb_type_id_create.cell",
    "v0_14_delegate_verify.cell",
    "v0_14_hash_blake2b.cell",
    "v0_14_multi_step_pipeline.cell",
    "v0_14_witness_source.cell",
    "v0_15_identity_lifecycle.cell",
    "v0_15_scoped_invariant.cell",
]
EXPECTED_ACTION_COUNT = 44
EXPECTED_STATUS = "passed"
EXPECTED_MODE = "production"
EXPECTED_LOCK_SPEND_MATRIX = {
    "multisig.cell": ["is_signer_lock", "can_execute", "can_cancel", "has_enough_signatures", "not_expired"],
    "nft.cell": ["nft_ownership", "listing_seller", "offer_buyer", "valid_royalty", "collection_creator"],
    "timelock.cell": ["can_unlock_lock", "is_owner", "lock_id_commitment", "asset_matches", "not_expired", "emergency_approved"],
    "vesting.cell": ["vesting_admin"],
}
EXPECTED_LOCK_COUNT = sum(len(locks) for locks in EXPECTED_LOCK_SPEND_MATRIX.values())
EXPECTED_LOCK_NAMES = [
    f"{example}:{lock}"
    for example, locks in EXPECTED_LOCK_SPEND_MATRIX.items()
    for lock in locks
]
EXPECTED_CRITICAL_ELF_ABI_EXAMPLES = ["launch.cell", "token.cell", "amm_pool.cell"]

ACTION_RUN_KEYS = [
    "token_action_runs",
    "nft_action_runs",
    "timelock_action_runs",
    "multisig_action_runs",
    "vesting_action_runs",
    "amm_action_runs",
    "launch_action_runs",
]

EXPECTED_ACTIONS_BY_RUN_KEY = {
    "token_action_runs": ["mint_with_authority", "transfer_token", "burn", "merge"],
    "nft_action_runs": [
        "create_collection",
        "mint",
        "transfer",
        "create_listing",
        "cancel_listing",
        "buy_from_listing",
        "create_offer",
        "accept_offer",
        "burn",
        "batch_mint",
    ],
    "timelock_action_runs": [
        "create_absolute_lock",
        "create_relative_lock",
        "lock_asset",
        "request_release",
        "request_emergency_release",
        "approve_emergency_release",
        "extend_lock",
        "execute_release",
        "execute_emergency_release",
        "batch_create_locks",
    ],
    "multisig_action_runs": [
        "create_wallet",
        "propose_transfer",
        "add_signature",
        "execute_proposal",
        "cancel_proposal",
        "propose_add_signer",
        "propose_remove_signer",
        "propose_change_threshold",
    ],
    "vesting_action_runs": ["create_vesting_config", "grant_vesting", "claim_vested", "revoke_grant"],
    "amm_action_runs": ["seed_pool", "swap_a_for_b", "add_liquidity", "remove_liquidity", "isqrt", "min"],
    "launch_action_runs": ["launch_token", "bootstrap_token"],
}


def load_json(path: Path) -> dict[str, Any]:
    try:
        with path.open("r", encoding="utf-8") as fh:
            value = json.load(fh)
    except FileNotFoundError as exc:
        raise SystemExit(f"missing CKB production evidence: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from exc
    if not isinstance(value, dict):
        raise SystemExit(f"{path} must contain a JSON object")
    return value


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SystemExit(f"invalid CKB CellScript production evidence: {message}")


def require_field(mapping: dict[str, Any], key: str, expected: Any, context: str = "") -> None:
    actual = mapping.get(key)
    prefix = f"{context}." if context else ""
    require(actual == expected, f"{prefix}{key} must be {expected!r}, got {actual!r}")


def require_empty(mapping: dict[str, Any], key: str, context: str = "") -> None:
    value = mapping.get(key)
    prefix = f"{context}." if context else ""
    require(value == [], f"{prefix}{key} must be empty, got {value!r}")


def require_positive_int(value: Any, context: str) -> int:
    require(isinstance(value, int) and value > 0, f"{context} must be a positive integer, got {value!r}")
    return value


def require_bool(value: Any, context: str) -> bool:
    require(isinstance(value, bool), f"{context} must be a boolean, got {value!r}")
    return value

def require_hex_hash(value: Any, context: str) -> str:
    require(
        isinstance(value, str)
        and value.startswith("0x")
        and len(value) == 66
        and all(ch in "0123456789abcdefABCDEF" for ch in value[2:]),
        f"{context} must be a 32-byte 0x-prefixed hex hash, got {value!r}",
    )
    return value


def validate_elf_entry_abi_gate(report: dict[str, Any]) -> None:
    gate = report.get("ckb_elf_entry_abi_gate")
    require(isinstance(gate, dict), "ckb_elf_entry_abi_gate must be an object")
    require_field(gate, "schema", "cellscript-ckb-elf-entry-abi-gate-v0.20", "ckb_elf_entry_abi_gate")
    require_field(gate, "status", EXPECTED_STATUS, "ckb_elf_entry_abi_gate")
    require_field(gate, "requires_ckb_vm_stack_pointer_preserved", True, "ckb_elf_entry_abi_gate")
    require_field(gate, "requires_entry_trampoline_call_sequence", True, "ckb_elf_entry_abi_gate")
    require_field(gate, "requires_rx_only_executable_segment", True, "ckb_elf_entry_abi_gate")
    require_field(gate, "requires_no_fake_stack_load_segment", True, "ckb_elf_entry_abi_gate")
    require_field(gate, "critical_examples", EXPECTED_CRITICAL_ELF_ABI_EXAMPLES, "ckb_elf_entry_abi_gate")
    require_empty(gate, "failures", "ckb_elf_entry_abi_gate")
    require_positive_int(gate.get("audited_artifact_count"), "ckb_elf_entry_abi_gate.audited_artifact_count")

    critical = gate.get("critical_example_gate")
    require(isinstance(critical, dict), "ckb_elf_entry_abi_gate.critical_example_gate must be an object")
    for example in EXPECTED_CRITICAL_ELF_ABI_EXAMPLES:
        row = critical.get(example)
        require(isinstance(row, dict), f"ckb_elf_entry_abi_gate.critical_example_gate.{example} must be an object")
        require_field(row, "status", EXPECTED_STATUS, f"ckb_elf_entry_abi_gate.critical_example_gate.{example}")
        require_field(row, "missing", False, f"ckb_elf_entry_abi_gate.critical_example_gate.{example}")
        require_empty(row, "failures", f"ckb_elf_entry_abi_gate.critical_example_gate.{example}")
        require_positive_int(row.get("artifact_count"), f"ckb_elf_entry_abi_gate.critical_example_gate.{example}.artifact_count")

    rows = gate.get("rows")
    require(isinstance(rows, list) and rows, "ckb_elf_entry_abi_gate.rows must be a non-empty list")
    for index, row in enumerate(rows):
        require(isinstance(row, dict), f"ckb_elf_entry_abi_gate.rows[{index}] must be an object")
        context = f"ckb_elf_entry_abi_gate.rows[{index}]"
        require_field(row, "status", EXPECTED_STATUS, context)
        require_field(row, "preserves_ckb_vm_stack_pointer", True, context)
        require_field(row, "entry_trampoline_calls_with_ra", True, context)
        require_field(row, "executable_segment_rx_only", True, context)
        require_field(row, "executable_segment_file_size_equals_memory_size", True, context)
        require(isinstance(row.get("artifact"), str) and row["artifact"], f"{context}.artifact must be a non-empty string")
        require(isinstance(row.get("first_instruction_le_hex"), str), f"{context}.first_instruction_le_hex must be present")


def git_stdout(repo_root: Path, args: list[str]) -> str:
    try:
        return subprocess.check_output(["git", *args], cwd=repo_root, text=True).strip()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise SystemExit(f"failed to query git source provenance in {repo_root}: {exc}") from exc


def tracked_source_files(repo_root: Path) -> list[str]:
    output = git_stdout(repo_root, ["ls-files", "--", *SOURCE_PROVENANCE_PATHS])
    return [
        line
        for line in output.splitlines()
        if line and (repo_root / line).is_file()
    ]


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()

def ckb_data_hash_hex(data: bytes) -> str:
    return "0x" + hashlib.blake2b(data, digest_size=32, person=b"ckb-default-hash").hexdigest()


def tracked_source_sha256(repo_root: Path, files: list[str]) -> str:
    h = hashlib.sha256()
    for rel in files:
        h.update(rel.encode("utf-8"))
        h.update(b"\0")
        h.update(file_sha256(repo_root / rel).encode("ascii"))
        h.update(b"\n")
    return "0x" + h.hexdigest()


def current_source_provenance(repo_root: Path) -> dict[str, Any]:
    files = tracked_source_files(repo_root)
    return {
        "repo_commit": git_stdout(repo_root, ["rev-parse", "HEAD"]),
        "git_dirty": bool(git_stdout(repo_root, ["status", "--porcelain", "--untracked-files=no"])),
        "tracked_source_paths": SOURCE_PROVENANCE_PATHS,
        "tracked_source_files": files,
        "tracked_source_file_count": len(files),
        "tracked_source_sha256": tracked_source_sha256(repo_root, files),
        "acceptance_script_sha256": "0x" + file_sha256(repo_root / "scripts/ckb_cellscript_acceptance.sh"),
        "validator_script_sha256": "0x" + file_sha256(repo_root / "scripts/validate_ckb_cellscript_production_evidence.py"),
    }


def validate_source_provenance(report: dict[str, Any], repo_root: Path) -> None:
    provenance = report.get("source_provenance")
    require(isinstance(provenance, dict), "source_provenance must be an object")
    require_field(provenance, "schema", SOURCE_PROVENANCE_SCHEMA, "source_provenance")
    require(isinstance(provenance.get("generated_at_utc"), str), "source_provenance.generated_at_utc must be a timestamp string")
    require_bool(provenance.get("git_dirty"), "source_provenance.git_dirty")

    current = current_source_provenance(repo_root)
    for key in (
        "repo_commit",
        "git_dirty",
        "tracked_source_paths",
        "tracked_source_files",
        "tracked_source_file_count",
        "tracked_source_sha256",
        "acceptance_script_sha256",
        "validator_script_sha256",
    ):
        require_field(provenance, key, current[key], "source_provenance")

def validate_build_reports(report: dict[str, Any], *, compile_only: bool) -> None:
    build_index = report.get("cellscript_build_reports")
    require(isinstance(build_index, dict), "cellscript_build_reports must be an object")
    require_field(build_index, "schema", "cellscript-ckb-build-report-index-v0.20", "cellscript_build_reports")
    require_field(build_index, "target_profile", "ckb", "cellscript_build_reports")
    require_field(build_index, "vm_profile", "ckb-vm", "cellscript_build_reports")
    require_field(build_index, "artifact_format", "riscv64-elf", "cellscript_build_reports")
    require_field(build_index, "artifact_hash_algorithm", "ckb-blake2b256", "cellscript_build_reports")
    require_field(build_index, "requires_exact_artifact_hash", True, "cellscript_build_reports")
    require_field(build_index, "requires_elf_entry_abi_gate", True, "cellscript_build_reports")
    require_field(build_index, "requires_live_code_cell_data_hash_match", True, "cellscript_build_reports")
    require_field(build_index, "status", EXPECTED_STATUS, "cellscript_build_reports")

    rows = build_index.get("reports")
    require(isinstance(rows, list) and rows, "cellscript_build_reports.reports must be a non-empty list")
    require_field(build_index, "artifact_count", len(rows), "cellscript_build_reports")

    elf_gate = report.get("ckb_elf_entry_abi_gate") or {}
    require_field(build_index, "artifact_count", elf_gate.get("audited_artifact_count"), "cellscript_build_reports")

    seen_artifacts: set[str] = set()
    for index, row in enumerate(rows):
        require(isinstance(row, dict), f"cellscript_build_reports.reports[{index}] must be an object")
        context = f"cellscript_build_reports.reports[{index}]"
        require_field(row, "schema", BUILD_REPORT_SCHEMA, context)
        require_field(row, "target_profile", "ckb", context)
        require_field(row, "vm_profile", "ckb-vm", context)
        require_field(row, "artifact_format", "riscv64-elf", context)
        require_field(row, "artifact_hash_algorithm", "ckb-blake2b256", context)
        require_field(row, "deployment_hash_type_used_by_gate", "data1", context)
        require_field(row, "verify_artifact_status", "passed", context)
        require_field(row, "verify_target_profile", "ckb", context)
        require_field(row, "elf_entry_abi_status", "passed", context)
        require_field(row, "abi_trailer_stripped", True, context)
        require_positive_int(row.get("artifact_size_bytes"), f"{context}.artifact_size_bytes")
        require_hex_hash(row.get("deployable_elf_hash"), f"{context}.deployable_elf_hash")
        require_hex_hash(row.get("artifact_sha256"), f"{context}.artifact_sha256")
        artifact_path = row.get("artifact_path")
        require(isinstance(artifact_path, str) and artifact_path, f"{context}.artifact_path must be present")
        require(artifact_path not in seen_artifacts, f"duplicate build report artifact_path: {artifact_path}")
        seen_artifacts.add(artifact_path)
        artifact = Path(artifact_path)
        require(artifact.exists(), f"{context}.artifact_path does not exist: {artifact}")
        artifact_bytes = artifact.read_bytes()
        require(len(artifact_bytes) == row["artifact_size_bytes"], f"{context}.artifact_size_bytes does not match artifact")
        require(ckb_data_hash_hex(artifact_bytes) == row["deployable_elf_hash"], f"{context}.deployable_elf_hash does not match artifact")
        require("0x" + hashlib.sha256(artifact_bytes).hexdigest() == row["artifact_sha256"], f"{context}.artifact_sha256 does not match artifact")
        onchain_deployments = row.get("onchain_deployments")
        require(isinstance(onchain_deployments, list), f"{context}.onchain_deployments must be a list")
        if compile_only:
            require(onchain_deployments == [], f"{context}.onchain_deployments must be empty for compile-only reports")
        else:
            require(onchain_deployments, f"{context}.onchain_deployments must contain live deployment evidence")
            for deployment_index, deployment in enumerate(onchain_deployments):
                deployment_context = f"{context}.onchain_deployments[{deployment_index}]"
                require(isinstance(deployment, dict), f"{deployment_context} must be an object")
                require_field(deployment, "code_cell_live", True, deployment_context)
                require_field(deployment, "live_code_cell_data_hash_matches_artifact", True, deployment_context)
                require_field(
                    deployment,
                    "artifact_ckb_data_hash_blake2b",
                    row["deployable_elf_hash"],
                    deployment_context,
                )
                require_field(
                    deployment,
                    "live_code_cell_data_hash",
                    row["deployable_elf_hash"],
                    deployment_context,
                )
                out_point = deployment.get("out_point")
                require(isinstance(out_point, dict), f"{deployment_context}.out_point must be an object")
                require(isinstance(out_point.get("tx_hash"), str) and out_point["tx_hash"].startswith("0x"), f"{deployment_context}.out_point.tx_hash must be hex")
                require(isinstance(out_point.get("index"), str) and out_point["index"].startswith("0x"), f"{deployment_context}.out_point.index must be hex")

    if compile_only:
        require(build_index.get("onchain_deployed_artifact_count") in (None, 0), "compile-only build reports must not record onchain deployments")
    else:
        require_field(build_index, "onchain_deployed_artifact_count", len(rows), "cellscript_build_reports")
        require_field(build_index, "live_code_cell_data_hash_match_count", len(rows), "cellscript_build_reports")
        require_empty(build_index, "missing_onchain_deployments", "cellscript_build_reports")
        require_empty(build_index, "live_code_cell_data_hash_mismatches", "cellscript_build_reports")
        require_empty(build_index, "unexpected_onchain_artifacts", "cellscript_build_reports")


def all_action_runs(report: dict[str, Any]) -> list[dict[str, Any]]:
    onchain = report.get("onchain")
    require(isinstance(onchain, dict), "onchain section must be present")
    runs: list[dict[str, Any]] = []
    for key in ACTION_RUN_KEYS:
        value = onchain.get(key)
        require(isinstance(value, list), f"onchain.{key} must be a list")
        expected_actions = EXPECTED_ACTIONS_BY_RUN_KEY[key]
        actual_actions = [row.get("action") for row in value if isinstance(row, dict)]
        require(
            sorted(actual_actions) == sorted(expected_actions) and len(actual_actions) == len(expected_actions),
            f"onchain.{key} actions must be {expected_actions!r}, got {actual_actions!r}",
        )
        require(
            len(set(actual_actions)) == len(actual_actions),
            f"onchain.{key} must not contain duplicate actions, got {actual_actions!r}",
        )
        for row in value:
            require(isinstance(row, dict), f"onchain.{key} entries must be objects")
            runs.append(row)
    return runs


def validate_compile_gate(report: dict[str, Any], *, compile_only: bool = False) -> None:
    require_field(report, "acceptance_mode", EXPECTED_MODE)
    require_field(report, "status", EXPECTED_STATUS)
    if compile_only:
        require_field(report, "production_ready", False)
    else:
        require_field(report, "production_ready", True)
    require_field(report, "bundled_examples_count", len(EXPECTED_EXAMPLES))
    require_field(report, "bundled_examples_exact_order", EXPECTED_EXAMPLES)
    require_field(report, "non_production_examples", EXPECTED_NON_PRODUCTION_EXAMPLES)
    require_field(report, "language_examples_count", len(EXPECTED_LANGUAGE_EXAMPLES))
    require_field(report, "language_examples_exact_order", EXPECTED_LANGUAGE_EXAMPLES)
    require_field(report, "original_scoped_action_count", EXPECTED_ACTION_COUNT)
    require_field(report, "original_scoped_lock_count", EXPECTED_LOCK_COUNT)
    require_field(report, "original_scoped_action_fail_closed_count", 0)
    require_field(report, "original_scoped_lock_fail_closed_count", 0)
    require_empty(report, "strict_original_ckb_compile_policy_fail_closed")
    require_empty(report, "strict_original_ckb_compile_unexpected_failures")
    require_empty(report, "original_scoped_action_fail_closed")
    require_empty(report, "original_scoped_lock_fail_closed")

    gate = report.get("production_gate")
    require(isinstance(gate, dict), "production_gate must be an object")
    require_field(gate, "status", EXPECTED_STATUS, "production_gate")
    require_empty(gate, "failures", "production_gate")
    require_field(gate, "requires_original_scoped_harnesses", True, "production_gate")
    require_field(gate, "requires_no_expected_fail_closed_entries", True, "production_gate")
    require_field(gate, "requires_all_bundled_examples_strict_original_ckb", True, "production_gate")
    require_field(gate, "requires_ckb_elf_entry_abi_gate", True, "production_gate")
    require_field(gate, "requires_cellscript_build_reports", True, "production_gate")
    validate_elf_entry_abi_gate(report)
    validate_build_reports(report, compile_only=compile_only)

    coverage = report.get("ckb_business_coverage")
    require(isinstance(coverage, dict), "ckb_business_coverage must be an object")
    require_field(coverage, "strict_compile_coverage_complete", True, "ckb_business_coverage")
    require_field(coverage, "expected_fail_closed_action_count", 0, "ckb_business_coverage")
    require_field(coverage, "expected_fail_closed_lock_count", 0, "ckb_business_coverage")
    if compile_only:
        require_field(coverage, "status", "incomplete", "ckb_business_coverage")
        require_field(coverage, "onchain_action_coverage_complete", False, "ckb_business_coverage")
        require_field(coverage, "ckb_onchain_action_count", 0, "ckb_business_coverage")
        onchain = report.get("onchain")
        require(isinstance(onchain, dict), "onchain section must be present")
        require_field(onchain, "status", "skipped", "onchain")
        require_field(onchain, "reason", "compile-only", "onchain")
    else:
        require_field(coverage, "status", "complete", "ckb_business_coverage")
        require_field(coverage, "onchain_action_coverage_complete", True, "ckb_business_coverage")
        require_field(coverage, "ckb_onchain_action_count", EXPECTED_ACTION_COUNT, "ckb_business_coverage")
        missing = coverage.get("missing_ckb_onchain_actions")
        require(missing in ({}, None), f"ckb_business_coverage.missing_ckb_onchain_actions must be empty, got {missing!r}")

    example_scope = report.get("example_scope")
    require(isinstance(example_scope, dict), "example_scope must be an object")
    require_field(example_scope, "production_bundled_examples", EXPECTED_EXAMPLES, "example_scope")
    require_field(example_scope, "non_production_top_level_examples", EXPECTED_NON_PRODUCTION_EXAMPLES, "example_scope")
    require_field(example_scope, "non_production_language_examples", EXPECTED_LANGUAGE_EXAMPLES, "example_scope")
    scope_note = example_scope.get("production_scope_note")
    require(
        isinstance(scope_note, str)
        and "Only production_bundled_examples" in scope_note
        and "non_production_top_level_examples" in scope_note
        and "non_production_language_examples" in scope_note,
        "example_scope.production_scope_note must state the production/non-production example boundary",
    )
    source_layout = report.get("example_source_layout")
    require(isinstance(source_layout, dict), "example_source_layout must be an object")
    require(isinstance(source_layout.get("canonical_bundled_examples"), str), "example_source_layout must record canonical_bundled_examples")
    require(isinstance(source_layout.get("language_examples"), str), "example_source_layout must record language_examples")
    require(
        "production_acceptance_examples" not in source_layout
        and "canonical_business_examples" not in source_layout
        and "flat_business_compatibility_examples" not in source_layout,
        "example_source_layout must not advertise the removed business/acceptance split",
    )
    layout_note = source_layout.get("canonical_examples_note")
    require(
        isinstance(layout_note, str)
        and "top-level examples/*.cell directly" in layout_note
        and "examples/business and examples/acceptance" in layout_note,
        "example_source_layout.canonical_examples_note must state the single-source example layout",
    )

    lock_scope = report.get("lock_acceptance_scope")
    require(isinstance(lock_scope, dict), "lock_acceptance_scope must be an object")
    if lock_scope.get("onchain_lock_spend_matrix") is True:
        require_field(lock_scope, "strict_compile_only", False, "lock_acceptance_scope")
        require_field(lock_scope, "onchain_lock_spend_matrix_scope", EXPECTED_LOCK_SPEND_MATRIX, "lock_acceptance_scope")
        require_field(lock_scope, "required_cases_per_lock", ["valid_spend", "invalid_spend"], "lock_acceptance_scope")
    else:
        require_field(lock_scope, "strict_compile_only", True, "lock_acceptance_scope")
        require_field(lock_scope, "onchain_lock_spend_matrix", False, "lock_acceptance_scope")
        require_field(lock_scope, "pending_onchain_lock_spend_matrix", EXPECTED_LOCK_SPEND_MATRIX, "lock_acceptance_scope")
        require_field(
            lock_scope,
            "required_cases_per_lock_when_promoted",
            ["valid_spend", "invalid_spend"],
            "lock_acceptance_scope",
        )
    lock_scope_note = lock_scope.get("scope_note")
    require(isinstance(lock_scope_note, str) and "strict-compiled" in lock_scope_note, "lock_acceptance_scope.scope_note must mention strict compilation")


def validate_onchain_gate(report: dict[str, Any]) -> None:
    onchain = report.get("onchain")
    require(isinstance(onchain, dict), "onchain section must be present")
    require_field(onchain, "status", EXPECTED_STATUS, "onchain")
    require_field(onchain, "all_artifacts_deployed_and_spent", True, "onchain")
    require_field(onchain, "all_bundled_examples_deployed", True, "onchain")
    require_field(onchain, "bundled_examples_deployed", EXPECTED_EXAMPLES, "onchain")
    require_field(onchain, "all_token_actions_exercised", True, "onchain")
    require_field(onchain, "all_nft_actions_exercised", True, "onchain")
    require_field(onchain, "all_timelock_actions_exercised", True, "onchain")
    require_field(onchain, "all_multisig_actions_exercised", True, "onchain")
    require_field(onchain, "all_vesting_actions_exercised", True, "onchain")
    require_field(onchain, "all_amm_actions_exercised", True, "onchain")
    require_field(onchain, "all_launch_actions_exercised", True, "onchain")
    require_field(onchain, "builder_backed_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "handwritten_harness_action_count", 0, "onchain")
    require_field(onchain, "measured_cycles_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "tx_size_measured_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "occupied_capacity_measured_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "lock_spend_matrix_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "builder_backed_lock_spend_matrix_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "lock_valid_spend_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "lock_invalid_spend_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "measured_cycles_lock_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "tx_size_measured_lock_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "occupied_capacity_measured_lock_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "all_locks_behavior_exercised", True, "onchain")

    deployment_runs = onchain.get("bundled_example_deployment_runs")
    require(isinstance(deployment_runs, list), "onchain.bundled_example_deployment_runs must be a list")
    require(
        len(deployment_runs) == len(EXPECTED_EXAMPLES),
        f"expected {len(EXPECTED_EXAMPLES)} bundled example deployment runs, got {len(deployment_runs)}",
    )
    deployment_names = [run.get("name") for run in deployment_runs if isinstance(run, dict)]
    require(
        deployment_names == EXPECTED_EXAMPLES,
        f"bundled example deployment order must be {EXPECTED_EXAMPLES!r}, got {deployment_names!r}",
    )
    for run in deployment_runs:
        require(isinstance(run, dict), "bundled example deployment run entries must be objects")
        name = run.get("name")
        require(isinstance(name, str) and name, "bundled example deployment run is missing name")
        require_field(run, "status", EXPECTED_STATUS, name)
        require_field(run, "kind", "bundled-example-strict-original", name)
        require_bool(run.get("code_cell_live"), f"{name}.code_cell_live")
        require_positive_int(run.get("artifact_size_bytes"), f"{name}.artifact_size_bytes")
        require_field(run, "live_code_cell_data_hash_matches_artifact", True, name)
        require_hex_hash(run.get("artifact_ckb_data_hash_blake2b"), f"{name}.artifact_ckb_data_hash_blake2b")
        require_field(run, "live_code_cell_data_hash", run["artifact_ckb_data_hash_blake2b"], name)
        valid_deploy_dry_run = run.get("valid_deploy_dry_run")
        require(isinstance(valid_deploy_dry_run, dict), f"{name} missing valid_deploy_dry_run")
        require(
            isinstance(valid_deploy_dry_run.get("cycles"), str) and valid_deploy_dry_run["cycles"].startswith("0x"),
            f"{name} missing hex deploy dry-run cycles",
        )

    final_gate = report.get("final_production_hardening_gate")
    require(isinstance(final_gate, dict), "final_production_hardening_gate must be an object")
    require_field(final_gate, "status", EXPECTED_STATUS, "final_production_hardening_gate")
    require_field(final_gate, "ready", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_builder_generated_transactions", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_measured_cycles", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_consensus_serialized_tx_size", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_exact_occupied_capacity", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_build_report_live_artifact_linkage", True, "final_production_hardening_gate")
    require_empty(final_gate, "failures", "final_production_hardening_gate")

    runs = all_action_runs(report)
    require(len(runs) == EXPECTED_ACTION_COUNT, f"expected {EXPECTED_ACTION_COUNT} action runs, got {len(runs)}")
    seen_names: set[str] = set()
    for run in runs:
        name = run.get("name")
        require(isinstance(name, str) and name, "action run is missing name")
        require(name not in seen_names, f"duplicate action run name: {name}")
        seen_names.add(name)
        action = run.get("action")
        require(isinstance(action, str) and action, f"{name} is missing action")
        require(name.endswith(f":{action}"), f"{name} must end with action suffix :{action}")
        require_field(run, "status", EXPECTED_STATUS, name)
        require(run.get("builder_backed") is True, f"{name} is not builder-backed")
        require(isinstance(run.get("builder_name"), str) and run["builder_name"], f"{name} missing builder_name")
        require(isinstance(run.get("harness_origin"), str) and run["harness_origin"], f"{name} missing harness_origin")

        code = run.get("code")
        require(isinstance(code, dict), f"{name} missing code section")
        require_bool(code.get("code_cell_live"), f"{name}.code.code_cell_live")
        require_positive_int(code.get("artifact_size_bytes"), f"{name}.code.artifact_size_bytes")
        require_field(code, "live_code_cell_data_hash_matches_artifact", True, f"{name}.code")
        require_hex_hash(code.get("artifact_ckb_data_hash_blake2b"), f"{name}.code.artifact_ckb_data_hash_blake2b")
        require_field(code, "live_code_cell_data_hash", code["artifact_ckb_data_hash_blake2b"], f"{name}.code")

        valid_dry_run = run.get("valid_dry_run")
        require(isinstance(valid_dry_run, dict), f"{name} missing valid_dry_run")
        require(isinstance(valid_dry_run.get("cycles"), str) and valid_dry_run["cycles"].startswith("0x"), f"{name} missing hex dry-run cycles")
        require(isinstance(run.get("valid_commit"), dict), f"{name} missing valid_commit")

        malformed = run.get("malformed_transaction")
        require(isinstance(malformed, dict), f"{name} missing malformed_transaction evidence")
        require_field(malformed, "status", "rejected", f"{name}.malformed_transaction")
        require_field(malformed, "expected_reason_matched", True, f"{name}.malformed_transaction")
        require_field(malformed, "policy_or_capacity_reason", False, f"{name}.malformed_transaction")

        measured = run.get("measured_constraints")
        require(isinstance(measured, dict), f"{name} missing measured_constraints")
        require_field(measured, "cycles_status", "dry-run-measured", f"{name}.measured_constraints")
        require_field(measured, "tx_size_status", "measured-by-cellscript-ckb-tx-measure", f"{name}.measured_constraints")
        require_field(
            measured,
            "occupied_capacity_status",
            "derived-by-cellscript-ckb-tx-measure",
            f"{name}.measured_constraints",
        )
        require_positive_int(measured.get("measured_cycles"), f"{name}.measured_constraints.measured_cycles")
        require_positive_int(
            measured.get("consensus_serialized_tx_size_bytes"),
            f"{name}.measured_constraints.consensus_serialized_tx_size_bytes",
        )
        occupied = require_positive_int(
            measured.get("occupied_capacity_shannons"),
            f"{name}.measured_constraints.occupied_capacity_shannons",
        )
        output_capacity = require_positive_int(
            measured.get("output_capacity_shannons"),
            f"{name}.measured_constraints.output_capacity_shannons",
        )
        require(output_capacity >= occupied, f"{name} output capacity is below occupied capacity")
        output_count = require_positive_int(measured.get("output_count"), f"{name}.measured_constraints.output_count")
        output_caps = measured.get("measured_output_capacity_shannons")
        output_occupied = measured.get("output_occupied_capacity_shannons")
        require(isinstance(output_caps, list), f"{name}.measured_constraints.measured_output_capacity_shannons must be a list")
        require(isinstance(output_occupied, list), f"{name}.measured_constraints.output_occupied_capacity_shannons must be a list")
        require(len(output_caps) == output_count, f"{name} measured output capacity count does not match output_count")
        require(len(output_occupied) == output_count, f"{name} occupied output capacity count does not match output_count")
        for index, (cap, occ) in enumerate(zip(output_caps, output_occupied)):
            cap_int = require_positive_int(cap, f"{name}.measured_constraints.measured_output_capacity_shannons[{index}]")
            occ_int = require_positive_int(occ, f"{name}.measured_constraints.output_occupied_capacity_shannons[{index}]")
            require(cap_int >= occ_int, f"{name} output {index} capacity is below occupied capacity")
        require(measured.get("capacity_is_sufficient") is True, f"{name} has insufficient capacity")
        require(measured.get("under_capacity_output_indexes") == [], f"{name} has under-capacity outputs")

    lock_runs = onchain.get("lock_spend_matrix_runs")
    require(isinstance(lock_runs, list), "onchain.lock_spend_matrix_runs must be a list")
    lock_names = [run.get("name") for run in lock_runs if isinstance(run, dict)]
    require(
        sorted(lock_names) == sorted(EXPECTED_LOCK_NAMES) and len(lock_names) == EXPECTED_LOCK_COUNT,
        f"lock spend matrix must cover {EXPECTED_LOCK_NAMES!r}, got {lock_names!r}",
    )
    require(len(set(lock_names)) == len(lock_names), f"lock spend matrix must not contain duplicates, got {lock_names!r}")
    for run in lock_runs:
        require(isinstance(run, dict), "lock spend matrix entries must be objects")
        name = run.get("name")
        require(isinstance(name, str) and name, "lock run is missing name")
        lock = run.get("lock")
        require(isinstance(lock, str) and lock, f"{name} is missing lock")
        require(name.endswith(f":{lock}"), f"{name} must end with lock suffix :{lock}")
        require_field(run, "status", EXPECTED_STATUS, name)
        require(run.get("builder_backed") is True, f"{name} lock run is not builder-backed")
        require(isinstance(run.get("builder_name"), str) and run["builder_name"], f"{name} missing builder_name")
        require(isinstance(run.get("harness_origin"), str) and run["harness_origin"], f"{name} missing harness_origin")

        code = run.get("code")
        require(isinstance(code, dict), f"{name} missing code section")
        require_bool(code.get("code_cell_live"), f"{name}.code.code_cell_live")
        require_positive_int(code.get("artifact_size_bytes"), f"{name}.code.artifact_size_bytes")
        require_field(code, "live_code_cell_data_hash_matches_artifact", True, f"{name}.code")
        require_hex_hash(code.get("artifact_ckb_data_hash_blake2b"), f"{name}.code.artifact_ckb_data_hash_blake2b")
        require_field(code, "live_code_cell_data_hash", code["artifact_ckb_data_hash_blake2b"], f"{name}.code")

        valid_spend = run.get("valid_spend")
        require(isinstance(valid_spend, dict), f"{name} missing valid_spend evidence")
        require_field(valid_spend, "status", EXPECTED_STATUS, f"{name}.valid_spend")
        require_field(valid_spend, "output_live", True, f"{name}.valid_spend")
        valid_dry_run = valid_spend.get("dry_run")
        require(isinstance(valid_dry_run, dict), f"{name}.valid_spend missing dry_run")
        require(
            isinstance(valid_dry_run.get("cycles"), str) and valid_dry_run["cycles"].startswith("0x"),
            f"{name}.valid_spend missing hex dry-run cycles",
        )
        require(isinstance(valid_spend.get("commit"), dict), f"{name}.valid_spend missing commit")

        invalid_spend = run.get("invalid_spend")
        require(isinstance(invalid_spend, dict), f"{name} missing invalid_spend evidence")
        require_field(invalid_spend, "status", "rejected", f"{name}.invalid_spend")
        rejection = invalid_spend.get("rejection")
        require(isinstance(rejection, dict), f"{name}.invalid_spend missing rejection")
        require_field(rejection, "status", "rejected", f"{name}.invalid_spend.rejection")
        require_field(rejection, "expected_reason_matched", True, f"{name}.invalid_spend.rejection")
        require_field(rejection, "policy_or_capacity_reason", False, f"{name}.invalid_spend.rejection")
        reason = rejection.get("reason")
        require(isinstance(reason, str) and reason, f"{name}.invalid_spend.rejection missing reason")
        for fragment in ("source: Inputs[0].Lock", "ValidationFailure", "error code 5"):
            require(fragment in reason, f"{name}.invalid_spend.rejection must show lock predicate error fragment {fragment!r}")
        live_after_rejection = invalid_spend.get("input_cells_live_after_rejection")
        require(
            isinstance(live_after_rejection, list) and live_after_rejection and all(value is True for value in live_after_rejection),
            f"{name}.invalid_spend must keep rejected input cells live",
        )

        measured = run.get("measured_constraints")
        require(isinstance(measured, dict), f"{name} missing measured_constraints")
        require_field(measured, "cycles_status", "dry-run-measured", f"{name}.measured_constraints")
        require_field(measured, "tx_size_status", "measured-by-cellscript-ckb-tx-measure", f"{name}.measured_constraints")
        require_field(
            measured,
            "occupied_capacity_status",
            "derived-by-cellscript-ckb-tx-measure",
            f"{name}.measured_constraints",
        )
        require_positive_int(measured.get("measured_cycles"), f"{name}.measured_constraints.measured_cycles")
        require_positive_int(
            measured.get("consensus_serialized_tx_size_bytes"),
            f"{name}.measured_constraints.consensus_serialized_tx_size_bytes",
        )
        occupied = require_positive_int(
            measured.get("occupied_capacity_shannons"),
            f"{name}.measured_constraints.occupied_capacity_shannons",
        )
        output_capacity = require_positive_int(
            measured.get("output_capacity_shannons"),
            f"{name}.measured_constraints.output_capacity_shannons",
        )
        require(output_capacity >= occupied, f"{name} output capacity is below occupied capacity")
        require(measured.get("capacity_is_sufficient") is True, f"{name} has insufficient capacity")
        require(measured.get("under_capacity_output_indexes") == [], f"{name} has under-capacity outputs")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate production CKB CellScript acceptance evidence emitted by CellScript scripts/ckb_cellscript_acceptance.sh.",
    )
    parser.add_argument("report", type=Path, help="Path to ckb-cellscript-acceptance-report.json")
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=Path(__file__).resolve().parents[1],
        help="CellScript checkout used to recompute source provenance. Defaults to this script's repository.",
    )
    parser.add_argument(
        "--compile-only",
        action="store_true",
        help="Only validate strict compile and scoped-entry production gates. This is not sufficient for external release.",
    )
    args = parser.parse_args()

    report_path = args.report.resolve()
    repo_root = args.repo_root.resolve()
    report = load_json(report_path)
    validate_source_provenance(report, repo_root)
    validate_compile_gate(report, compile_only=args.compile_only)
    if not args.compile_only:
        validate_onchain_gate(report)

    mode = "compile-only " if args.compile_only else ""
    print(f"valid CKB CellScript {mode}production evidence: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
