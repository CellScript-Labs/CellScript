#!/usr/bin/env python3
"""Validate CKB CellScript production acceptance evidence before release."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from typing import Any


SOURCE_PROVENANCE_SCHEMA = "cellscript-ckb-acceptance-source-provenance-v0.22"
BUILD_REPORT_SCHEMA = "cellscript-ckb-build-report-v0.20"
SOURCE_PROVENANCE_PATHS = [
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain.toml",
    ".github/workflows/release.yml",
    "src",
    "examples",
    "scripts/cellscript_gate.sh",
    "scripts/cellscript_ckb_release_gate.sh",
    "scripts/ckb_acceptance_pin.json",
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
EXPECTED_NON_PRODUCTION_EXAMPLES = ["registry.cell", "atomic_swap.cell", "multi_phase_dao.cell"]
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
    "v0_22_borrow.cell",
    "v0_22_bounded_lifecycle.cell",
    "v0_22_transaction_views.cell",
]
EXPECTED_ACTION_COUNT = 43
EXPECTED_STATUS = "passed"
EXPECTED_MODE = "production"
EXPECTED_LOCK_SPEND_MATRIX = {
    "multisig.cell": ["is_signer_lock", "can_execute", "can_cancel", "has_enough_approvals", "not_expired"],
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
        "record_approval",
        "execute_proposal",
        "cancel_proposal",
        "propose_add_signer",
        "propose_remove_signer",
        "propose_change_threshold",
    ],
    "vesting_action_runs": ["create_vesting_config", "grant_vesting", "claim_vested", "claim_fully_vested", "revoke_grant"],
    "amm_action_runs": ["seed_pool", "swap_a_for_b", "add_liquidity", "remove_liquidity"],
    "launch_action_runs": ["launch_token", "bootstrap_token"],
}
EXPECTED_ACTION_IDS = sorted(
    f"{example}:{action}"
    for run_key, actions in EXPECTED_ACTIONS_BY_RUN_KEY.items()
    for example in [{
        "token_action_runs": "token.cell",
        "nft_action_runs": "nft.cell",
        "timelock_action_runs": "timelock.cell",
        "multisig_action_runs": "multisig.cell",
        "vesting_action_runs": "vesting.cell",
        "amm_action_runs": "amm_pool.cell",
        "launch_action_runs": "launch.cell",
    }[run_key]]
    for action in actions
)
EXPECTED_PUBLIC_ACTIONS_BY_EXAMPLE = {
    "token.cell": EXPECTED_ACTIONS_BY_RUN_KEY["token_action_runs"],
    "nft.cell": EXPECTED_ACTIONS_BY_RUN_KEY["nft_action_runs"],
    "timelock.cell": [
        "create_absolute_lock",
        "create_relative_lock",
        "lock_asset",
        "request_release",
        "execute_release",
        "request_emergency_release",
        "approve_emergency_release",
        "execute_emergency_release",
        "extend_lock",
        "batch_create_locks",
    ],
    "multisig.cell": EXPECTED_ACTIONS_BY_RUN_KEY["multisig_action_runs"],
    "vesting.cell": EXPECTED_ACTIONS_BY_RUN_KEY["vesting_action_runs"],
    "amm_pool.cell": EXPECTED_ACTIONS_BY_RUN_KEY["amm_action_runs"],
    "launch.cell": EXPECTED_ACTIONS_BY_RUN_KEY["launch_action_runs"],
}
EXPECTED_END_TO_END_STATEFUL_SCENARIOS = [
    "token.mint-with-authority-transfer-mint-with-authority-merge-burn",
    "nft.mint-list-transfer-by-listing",
    "timelock.create-lock-lock-asset-request-release-execute",
    "launch.launch-token-then-mint-with-authority",
    "amm.seed-add-swap-remove",
    "vesting.create-config-grant-revoke",
    "multisig.create-propose-approve-approve-execute",
]


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
    require_field(gate, "schema", "cellscript-ckb-elf-entry-abi-gate-v0.22", "ckb_elf_entry_abi_gate")
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
        require_field(row, "first_instruction_le_hex", "0x00000097", context)
        require_field(
            row,
            "trampoline_instructions_le_hex",
            ["0x00000097", "0x014080e7", "0x000008b7", "0x05d88893", "0x00000073"],
            context,
        )
        require_field(row, "trampoline_bytes_hex", "97000000e7804001b70800009388d80573000000", context)
        require_field(row, "call_target", row.get("expected_call_target"), context)
        require_field(row, "exit_syscall_number", 93, context)
        require_field(row, "exit_sequence_exact", True, context)


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
        "git_dirty": bool(git_stdout(repo_root, ["status", "--porcelain", "--untracked-files=all"])),
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
    require_field(provenance, "git_dirty", False, "source_provenance")

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


def validate_public_builder_contracts(report: dict[str, Any]) -> None:
    gate = report.get("public_builder_contracts")
    require(isinstance(gate, dict), "public_builder_contracts must be an object")
    require_field(gate, "schema", "cellscript-public-builder-contract-gate-v0.22", "public_builder_contracts")
    require_field(gate, "status", EXPECTED_STATUS, "public_builder_contracts")
    require_field(gate, "example_count", len(EXPECTED_EXAMPLES), "public_builder_contracts")
    require_field(gate, "action_count", EXPECTED_ACTION_COUNT, "public_builder_contracts")
    require_field(gate, "requires_gen_builder", True, "public_builder_contracts")
    require_field(gate, "requires_action_build", True, "public_builder_contracts")
    require_field(
        gate,
        "transaction_origin_claim",
        "acceptance-python-harness-not-generated-builder",
        "public_builder_contracts",
    )
    contracts = gate.get("contracts")
    require(isinstance(contracts, list), "public_builder_contracts.contracts must be a list")
    require([contract.get("example") for contract in contracts] == EXPECTED_EXAMPLES, "public builder examples must match exact release scope")
    seen_action_ids: list[str] = []
    for contract in contracts:
        example = contract["example"]
        context = f"public_builder_contracts.{example}"
        expected_actions = EXPECTED_PUBLIC_ACTIONS_BY_EXAMPLE[example]
        require_field(contract, "status", EXPECTED_STATUS, context)
        require_field(contract, "generator_schema", "cellscript-generated-builder-summary-v0.20", context)
        require_field(contract, "builder_manifest_schema", "cellscript-generated-action-builder-v0.20", context)
        require_field(contract, "target", "typescript", context)
        require_field(contract, "target_profile", "ckb", context)
        require_field(contract, "actions", expected_actions, context)
        require_field(contract, "action_count", len(expected_actions), context)
        require_field(contract, "runtime_adapter_execution", "not-proven-by-this-contract-gate", context)
        require_hex_hash(contract.get("manifest_sha256"), f"{context}.manifest_sha256")
        require_hex_hash(contract.get("generated_tree_sha256"), f"{context}.generated_tree_sha256")
        require_positive_int(contract.get("generated_file_count"), f"{context}.generated_file_count")
        manifest_path = Path(contract.get("manifest_path", ""))
        require(manifest_path.is_file(), f"{context}.manifest_path does not exist: {manifest_path}")
        require("0x" + file_sha256(manifest_path) == contract["manifest_sha256"], f"{context}.manifest_sha256 does not match file")
        manifest = load_json(manifest_path)
        require([action.get("name") for action in manifest.get("actions", [])] == expected_actions, f"{context} manifest action mismatch")
        generated_files = sorted(path for path in manifest_path.parent.rglob("*") if path.is_file())
        tree_hash = hashlib.sha256()
        for path in generated_files:
            relative = path.relative_to(manifest_path.parent).as_posix()
            tree_hash.update(relative.encode("utf-8"))
            tree_hash.update(b"\0")
            tree_hash.update(hashlib.sha256(path.read_bytes()).digest())
        require_field(contract, "generated_file_count", len(generated_files), context)
        require_field(contract, "generated_tree_sha256", "0x" + tree_hash.hexdigest(), context)
        plans = contract.get("action_plans")
        require(isinstance(plans, list) and len(plans) == len(expected_actions), f"{context}.action_plans must cover every action")
        for plan, action in zip(plans, expected_actions, strict=True):
            plan_context = f"{context}.action_plans.{action}"
            require_field(plan, "action", action, plan_context)
            require_field(plan, "contract_id", f"{example}:{action}", plan_context)
            require_field(plan, "policy", "cellscript-action-builder-plan-v1", plan_context)
            require_field(plan, "status", EXPECTED_STATUS, plan_context)
            require_hex_hash(plan.get("plan_sha256"), f"{plan_context}.plan_sha256")
            plan_path = Path(plan.get("plan_path", ""))
            require(plan_path.is_file(), f"{plan_context}.plan_path does not exist: {plan_path}")
            require_field(plan, "plan_sha256", "0x" + file_sha256(plan_path), plan_context)
            plan_json = load_json(plan_path)
            require_field(plan_json, "status", "ok", f"{plan_context}.file")
            require_field(plan_json, "policy", "cellscript-action-builder-plan-v1", f"{plan_context}.file")
            require_field(plan_json, "action", action, f"{plan_context}.file")
            require_field(plan_json, "target_profile", "ckb", f"{plan_context}.file")
            seen_action_ids.append(plan["contract_id"])
    require(sorted(seen_action_ids) == EXPECTED_ACTION_IDS, "public builder action contracts must match the exact production action matrix")


def validate_ckb_runtime_provenance(report: dict[str, Any], repo_root: Path, report_dir: Path) -> None:
    pin_path = repo_root / "scripts/ckb_acceptance_pin.json"
    pin = load_json(pin_path)
    require_field(pin, "schema", "cellscript-ckb-acceptance-pin-v0.22", "ckb_acceptance_pin")
    provenance = report.get("ckb_runtime_provenance")
    require(isinstance(provenance, dict), "ckb_runtime_provenance must be an object")
    context = "ckb_runtime_provenance"
    require_field(provenance, "schema", "cellscript-ckb-runtime-provenance-v0.22", context)
    require_field(provenance, "pin_schema", pin["schema"], context)
    require_field(provenance, "pin_file_sha256", "0x" + file_sha256(pin_path), context)
    require_field(provenance, "repository", pin["repository"], context)
    require_field(provenance, "revision", pin["revision"], context)
    require_field(provenance, "repo_head", pin["revision"], context)
    require_field(provenance, "repo_dirty", False, context)
    require_field(provenance, "version", pin["version"], context)
    require_field(provenance, "build_mode", "fresh-dedicated-cargo-target", context)
    require_field(provenance, "binary_archived_with_report", True, context)
    version_output = provenance.get("version_output")
    require(
        isinstance(version_output, str)
        and pin["version"] in version_output
        and pin["revision"][:7] in version_output,
        f"{context}.version_output must bind version and revision, got {version_output!r}",
    )

    ckb_repo = Path(report.get("ckb_repo", "")).resolve()
    require(ckb_repo.is_dir(), f"ckb_repo does not exist: {ckb_repo}")
    require(git_stdout(ckb_repo, ["rev-parse", "HEAD"]) == pin["revision"], "current CKB checkout does not match pin")
    require(not git_stdout(ckb_repo, ["status", "--porcelain", "--untracked-files=all"]), "current CKB checkout must be clean")
    binary_path = Path(provenance.get("binary_path", "")).resolve()
    require(binary_path.is_file(), f"{context}.binary_path does not exist: {binary_path}")
    require_field(provenance, "binary_path", str((report_dir / "ckb-runtime" / "ckb").resolve()), context)
    require_field(provenance, "binary_sha256", "0x" + file_sha256(binary_path), context)
    require_field(provenance, "version_output", subprocess.check_output([binary_path, "--version"], text=True).strip(), context)

    expected_paths = {
        "source_template_path": ckb_repo / pin["template_paths"][0],
        "source_spec_path": ckb_repo / pin["template_paths"][1],
    }
    for key, path in expected_paths.items():
        require_field(provenance, key, str(path), context)
        require(path.is_file(), f"{context}.{key} does not exist: {path}")
        require_field(provenance, key.replace("_path", "_sha256"), "0x" + file_sha256(path), context)
    for key in ("effective_config", "effective_spec"):
        path = Path(provenance.get(f"{key}_path", ""))
        require(path.is_file(), f"{context}.{key}_path does not exist: {path}")
        require_field(provenance, f"{key}_sha256", "0x" + file_sha256(path), context)
    require_hex_hash(provenance.get("genesis_hash"), f"{context}.genesis_hash")
    require_field(provenance, "genesis_hash", report.get("onchain", {}).get("genesis_hash"), context)

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
    require_field(gate, "requires_public_builder_contracts", True, "production_gate")
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
    require_field(onchain, "builder_backed_action_count", 0, "onchain")
    require_field(onchain, "acceptance_harness_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "public_builder_contract_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "measured_cycles_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "tx_size_measured_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "occupied_capacity_measured_action_count", EXPECTED_ACTION_COUNT, "onchain")
    require_field(onchain, "lock_spend_matrix_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "builder_backed_lock_spend_matrix_count", 0, "onchain")
    require_field(onchain, "acceptance_harness_lock_spend_matrix_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "lock_valid_spend_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "lock_invalid_spend_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "measured_cycles_lock_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "tx_size_measured_lock_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "occupied_capacity_measured_lock_count", EXPECTED_LOCK_COUNT, "onchain")
    require_field(onchain, "all_locks_behavior_exercised", True, "onchain")
    resource_scope = onchain.get("resource_identity_evidence_scope")
    require(isinstance(resource_scope, dict), "onchain.resource_identity_evidence_scope must be an object")
    require_field(resource_scope, "status", "fixture-only", "onchain.resource_identity_evidence_scope")
    require_field(resource_scope, "always_success_resource_types", True, "onchain.resource_identity_evidence_scope")
    require_field(resource_scope, "production_resource_identity_proven", False, "onchain.resource_identity_evidence_scope")

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
    require_field(final_gate, "requires_builder_generated_transactions", False, "final_production_hardening_gate")
    require_field(final_gate, "requires_public_builder_contracts", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_acceptance_harness_transactions", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_measured_cycles", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_consensus_serialized_tx_size", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_exact_occupied_capacity", True, "final_production_hardening_gate")
    require_field(final_gate, "requires_stateful_action_coverage", True, "final_production_hardening_gate")
    require_field(final_gate, "production_resource_identity_claim", False, "final_production_hardening_gate")
    require_field(final_gate, "resource_identity_evidence_scope", "always-success-fixture-only", "final_production_hardening_gate")
    require_field(final_gate, "requires_build_report_live_artifact_linkage", True, "final_production_hardening_gate")
    require_empty(final_gate, "failures", "final_production_hardening_gate")

    stateful = onchain.get("stateful_scenarios")
    require(isinstance(stateful, dict), "onchain.stateful_scenarios must be an object")
    require_field(stateful, "status", EXPECTED_STATUS, "onchain.stateful_scenarios")
    require_positive_int(stateful.get("scenario_count"), "onchain.stateful_scenarios.scenario_count")
    require_positive_int(stateful.get("step_count"), "onchain.stateful_scenarios.step_count")
    require_field(
        stateful,
        "end_to_end_scenario_count",
        len(EXPECTED_END_TO_END_STATEFUL_SCENARIOS),
        "onchain.stateful_scenarios",
    )
    require_field(
        stateful,
        "action_branch_scenario_count",
        stateful["scenario_count"] - len(EXPECTED_END_TO_END_STATEFUL_SCENARIOS),
        "onchain.stateful_scenarios",
    )
    coverage = stateful.get("stateful_action_coverage")
    require(isinstance(coverage, dict), "onchain.stateful_scenarios.stateful_action_coverage must be an object")
    require_field(coverage, "status", EXPECTED_STATUS, "stateful_action_coverage")
    require_field(coverage, "required_action_count", EXPECTED_ACTION_COUNT, "stateful_action_coverage")
    require_field(coverage, "covered_action_count", EXPECTED_ACTION_COUNT, "stateful_action_coverage")
    require_field(coverage, "required_action_ids", EXPECTED_ACTION_IDS, "stateful_action_coverage")
    require_field(coverage, "covered_action_ids", EXPECTED_ACTION_IDS, "stateful_action_coverage")
    require_empty(coverage, "missing_action_ids", "stateful_action_coverage")
    require_empty(coverage, "missing_artifact_ids", "stateful_action_coverage")
    require_empty(coverage, "unexpected_artifact_ids", "stateful_action_coverage")
    stateful_runs = stateful.get("runs")
    require(isinstance(stateful_runs, list) and len(stateful_runs) == stateful["scenario_count"], "stateful scenario runs must match scenario_count")
    require(
        [run.get("name") for run in stateful_runs[: len(EXPECTED_END_TO_END_STATEFUL_SCENARIOS)]]
        == EXPECTED_END_TO_END_STATEFUL_SCENARIOS,
        "stateful end-to-end scenario names/order must match the production matrix",
    )
    seen_stateful_names: set[str] = set()
    main_action_ids: set[str] = set()
    branch_action_ids: list[str] = []
    observed_step_count = 0
    for index, stateful_run in enumerate(stateful_runs):
        context = f"onchain.stateful_scenarios.runs[{index}]"
        require(isinstance(stateful_run, dict), f"{context} must be an object")
        name = stateful_run.get("name")
        require(isinstance(name, str) and name, f"{context}.name must be a non-empty string")
        require(name not in seen_stateful_names, f"duplicate stateful scenario name: {name}")
        seen_stateful_names.add(name)
        require_field(stateful_run, "status", EXPECTED_STATUS, context)
        require_field(stateful_run, "builder_backed", False, context)
        require_field(stateful_run, "transaction_origin", "acceptance-python-harness", context)
        require_field(stateful_run, "harness_origin", "handwritten-python-acceptance-transaction", context)
        require(isinstance(stateful_run.get("acceptance_harness_name"), str) and stateful_run["acceptance_harness_name"], f"{context} missing acceptance_harness_name")
        action_ids = stateful_run.get("action_ids")
        require(isinstance(action_ids, list) and action_ids, f"{context}.action_ids must be a non-empty list")
        require(set(action_ids).issubset(EXPECTED_ACTION_IDS), f"{context}.action_ids contains actions outside the production matrix")
        steps = stateful_run.get("steps")
        require(isinstance(steps, list) and steps, f"{context}.steps must be a non-empty list")
        observed_step_count += len(steps)
        if index < len(EXPECTED_END_TO_END_STATEFUL_SCENARIOS):
            require_field(stateful_run, "kind", "stateful-scenario", context)
            require(len(steps) >= 2, f"{context} end-to-end scenario must contain at least two committed steps")
            main_action_ids.update(action_ids)
        else:
            require_field(stateful_run, "kind", "stateful-action-branch", context)
            require(len(action_ids) == 1 and len(steps) == 1, f"{context} branch scenario must bind exactly one action and one step")
            branch_action_ids.extend(action_ids)

        for step_index, step in enumerate(steps):
            step_context = f"{context}.steps[{step_index}]"
            require(isinstance(step, dict), f"{step_context} must be an object")
            require(isinstance(step.get("step"), str) and step["step"], f"{step_context}.step must be a non-empty string")
            require_field(step, "status", EXPECTED_STATUS, step_context)
            dry_run = step.get("dry_run")
            require(isinstance(dry_run, dict), f"{step_context}.dry_run must be an object")
            require(
                isinstance(dry_run.get("cycles"), str) and dry_run["cycles"].startswith("0x"),
                f"{step_context}.dry_run.cycles must be a hex quantity",
            )
            commit = step.get("commit")
            require(isinstance(commit, dict), f"{step_context}.commit must be an object")
            require_hex_hash(commit.get("tx_hash"), f"{step_context}.commit.tx_hash")
            commit_status = commit.get("status")
            require(isinstance(commit_status, dict), f"{step_context}.commit.status must be an object")
            require_field(commit_status, "status", "committed", f"{step_context}.commit.status")
            constraints = step.get("measured_constraints")
            require(isinstance(constraints, dict), f"{step_context}.measured_constraints must be an object")
            require_positive_int(constraints.get("measured_cycles"), f"{step_context}.measured_constraints.measured_cycles")
            require_positive_int(
                constraints.get("consensus_serialized_tx_size_bytes"),
                f"{step_context}.measured_constraints.consensus_serialized_tx_size_bytes",
            )
            require_positive_int(
                constraints.get("occupied_capacity_shannons"),
                f"{step_context}.measured_constraints.occupied_capacity_shannons",
            )
            require_field(constraints, "capacity_is_sufficient", True, f"{step_context}.measured_constraints")
            require_empty(constraints, "under_capacity_output_indexes", f"{step_context}.measured_constraints")
            consumed_inputs = step.get("consumed_inputs")
            require(isinstance(consumed_inputs, list), f"{step_context}.consumed_inputs must be a list")
            require(
                all(isinstance(cell, dict) and cell.get("status") != "live" for cell in consumed_inputs),
                f"{step_context}.consumed_inputs contains a still-live or malformed cell",
            )
            outputs_live = step.get("outputs_live")
            require(isinstance(outputs_live, dict), f"{step_context}.outputs_live must be an object")
            require(all(value is True for value in outputs_live.values()), f"{step_context}.outputs_live contains a dead output")

    require_field(stateful, "step_count", observed_step_count, "onchain.stateful_scenarios")
    expected_branch_ids = sorted(set(EXPECTED_ACTION_IDS) - main_action_ids)
    require(sorted(branch_action_ids) == expected_branch_ids, "stateful branch scenarios must cover every action absent from end-to-end flows exactly once")

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
        require_field(run, "builder_backed", False, name)
        require_field(run, "transaction_origin", "acceptance-python-harness", name)
        require_field(run, "harness_origin", "handwritten-python-acceptance-transaction", name)
        require(isinstance(run.get("acceptance_harness_name"), str) and run["acceptance_harness_name"], f"{name} missing acceptance_harness_name")
        require(isinstance(run.get("acceptance_harness_implementation"), str) and run["acceptance_harness_implementation"], f"{name} missing acceptance_harness_implementation")
        require_field(run, "public_builder_contract_id", name, name)
        require_field(run, "public_builder_contract_verified", True, name)

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
        require_field(run, "builder_backed", False, name)
        require_field(run, "transaction_origin", "acceptance-python-harness", name)
        require_field(run, "harness_origin", "handwritten-python-acceptance-transaction", name)
        require(isinstance(run.get("acceptance_harness_name"), str) and run["acceptance_harness_name"], f"{name} missing acceptance_harness_name")
        require(isinstance(run.get("acceptance_harness_implementation"), str) and run["acceptance_harness_implementation"], f"{name} missing acceptance_harness_implementation")

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
    validate_public_builder_contracts(report)
    validate_compile_gate(report, compile_only=args.compile_only)
    if not args.compile_only:
        validate_ckb_runtime_provenance(report, repo_root, report_path.parent)
        validate_onchain_gate(report)

    mode = "compile-only " if args.compile_only else ""
    print(f"valid CKB CellScript {mode}production evidence: {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
