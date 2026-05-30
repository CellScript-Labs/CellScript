#!/usr/bin/env python3
"""Probe the current CellScript VM2 spawn/IPC backend boundary.

The NovaSeal lock wiring now depends on the generic
verifier::btc::bip340::require_signature capability. The compiler must still
emit executable VM2 syscall wrappers, model the spawn target soundly in strict
ProofPlan mode, expose a concrete inherited-fd contract for the child verifier,
and build a conservative fixed-word verifier envelope without learning
NovaSeal-specific field names. This probe records that compiler/backend state
using a temporary protocol-agnostic verifier action.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any


SCHEMA = "novaseal-spawn-backend-probe-v0.1"

DEFAULT_OUTPUT = Path("target/novaseal-spawn-backend-probe.json")
DEFAULT_AUDIT_SURFACE = Path("target/novaseal-audit-surface.json")

PROBE_SOURCE = """module novaseal::spawn_backend_probe

action probe(message: Hash, witness pubkey: [u8; 32], witness signature: [u8; 64]) -> u64
where
    verifier::btc::bip340::require_signature(message, pubkey, signature)
    return 0
"""

BOUND_CELL_TOML = """[package]
name = "novaseal_spawn_bound_probe"
version = "0.0.0"
entry = "src/main.cell"

[build]
target_profile = "ckb"

[[deploy.ckb.cell_deps]]
name = "cellscript_btc_bip340_verifier_riscv"
out_point = "0x4444444444444444444444444444444444444444444444444444444444444444:0"
dep_type = "code"
hash_type = "data1"
"""


def run_command(args: list[str], cwd: Path) -> dict[str, Any]:
    completed = subprocess.run(args, cwd=cwd, text=True, capture_output=True, check=False)
    return {
        "args": args,
        "returncode": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
        "passed": completed.returncode == 0,
    }


def load_optional_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        return {}
    if not isinstance(value, dict):
        return {}
    return value


def audit_surface_status(path: Path) -> dict[str, Any]:
    surface = load_optional_json(path)
    proof_plan = surface.get("proof_plan", [])
    if not isinstance(proof_plan, list):
        proof_plan = []
    receipt_output_materialised = any(
        isinstance(record, dict)
        and str(record.get("feature", "")).startswith("create-output:ProofReceiptV0")
        and record.get("status") == "checked-runtime"
        and record.get("on_chain_checked") is True
        for record in proof_plan
    )
    measurement = surface.get("transaction_measurement_evidence", {})
    if not isinstance(measurement, dict):
        measurement = {}
    combined = measurement.get("combined_tx_report", {})
    if not isinstance(combined, dict):
        combined = {}
    return {
        "source": str(path),
        "present": bool(surface),
        "receipt_output_materialised": receipt_output_materialised,
        "transaction_measurement_present": measurement.get("measured") is True,
        "measurement_layer": measurement.get("measurement_layer"),
        "node_verification_stack_verified": measurement.get("node_verification_stack_verified") is True,
        "ckb_node_verification_stack_executed": combined.get("ckb_node_verification_stack_executed") is True,
        "node_stack_matched_expected": combined.get("node_stack_matched_expected"),
        "node_stack_mismatched": combined.get("node_stack_mismatched"),
    }


def helper_body(assembly: str, label: str) -> str:
    marker = f"{label}:"
    if marker not in assembly:
        return ""
    body = assembly.split(marker, 1)[1]
    if "\n.global " in body:
        body = body.split("\n.global ", 1)[0]
    return body


def analyse_assembly(assembly_path: Path) -> dict[str, Any]:
    assembly = assembly_path.read_text(encoding="utf-8")
    spawn_body = helper_body(assembly, "__ckb_spawn")
    spawn_with_fd_body = helper_body(assembly, "__ckb_spawn_with_fd1")
    spawn_helper_lines = [line.strip() for line in spawn_body.splitlines()]
    spawn_with_fd_helper_lines = [line.strip() for line in spawn_with_fd_body.splitlines()]
    fixed_u64_le_markers = assembly.count("# call __cellscript_fixed_u64_le")
    fixed_u64_le_last_signature_word = "# cellscript abi: fixed_u64_le word=7 offset=56 width=64" in assembly
    pipe_write_instruction_count = assembly.count("\n    call __ckb_pipe_write")
    return {
        "assembly_path": "<temporary>/spawn_backend_probe.s",
        "calls": {
            "pipe": "call __ckb_pipe" in assembly,
            "pipe_write": "call __ckb_pipe_write" in assembly,
            "spawn": "call __ckb_spawn" in assembly,
            "spawn_with_fd": "call __ckb_spawn_with_fd1" in assembly,
            "wait": "call __ckb_wait" in assembly,
            "close": "call __ckb_close" in assembly,
        },
        "fixed_word_envelope": {
            "fixed_u64_le_markers": fixed_u64_le_markers,
            "pipe_write_instruction_count": pipe_write_instruction_count,
            "contains_last_signature_word": fixed_u64_le_last_signature_word,
        },
        "generic_verifier_surface": {
            "source_uses_btc_bip340_helper": "verifier::btc::bip340::require_signature" in PROBE_SOURCE,
            "lowers_to_spawn_with_fd": "call __ckb_spawn_with_fd1" in assembly,
            "lowers_to_fixed_18_word_envelope": fixed_u64_le_markers == 16
            and pipe_write_instruction_count == 18
            and fixed_u64_le_last_signature_word,
        },
        "spawn_helper": {
            "present": bool(spawn_body),
            "contains_withheld_raw_syscall_2601": "withheld raw syscall 2601" in spawn_body,
            "contains_ecall_instruction": any(line == "ecall" for line in spawn_helper_lines),
            "contains_status_return_path": "li a1," in spawn_body and "ret" in spawn_body,
            "contains_static_cell_dep0_no_inherited_fds": "CellDep#0 with no argv and no inherited fds" in spawn_body,
        },
        "spawn_with_fd_helper": {
            "present": bool(spawn_with_fd_body),
            "contains_withheld_raw_syscall_2601": "withheld raw syscall 2601" in spawn_with_fd_body,
            "contains_ecall_instruction": any(line == "ecall" for line in spawn_with_fd_helper_lines),
            "contains_status_return_path": "li a1," in spawn_with_fd_body and "ret" in spawn_with_fd_body,
            "contains_static_cell_dep0_one_inherited_fd": "one inherited fd from a1" in spawn_with_fd_body,
            "stores_fd_at_inherited_fds_zero": "sd a1, 8(sp)" in spawn_with_fd_body,
            "terminates_inherited_fds": "sd zero, 16(sp)" in spawn_with_fd_body,
        },
    }


def strict_error_summary(result: dict[str, Any]) -> dict[str, Any]:
    text = f"{result.get('stdout', '')}\n{result.get('stderr', '')}"
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    return {
        "passed": result["passed"],
        "returncode": result["returncode"],
        "mentions_pp0150": "PP0150" in text,
        "mentions_spawn_target": "spawn-target" in text,
        "first_lines": lines[:12],
    }


def spawn_plan_entries(bundle_path: Path) -> list[dict[str, Any]]:
    if not bundle_path.exists():
        return []
    bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
    entries: list[dict[str, Any]] = []
    for action in bundle.get("actions", []):
        for mapping in action.get("proof_plan_source_mappings", []):
            if str(mapping.get("feature", "")).startswith("spawn-target:"):
                entries.append(
                    {
                        "origin": mapping.get("origin"),
                        "feature": mapping.get("feature"),
                        "codegen_coverage_status": mapping.get("codegen_coverage_status"),
                    }
                )
    return entries


def build_manifest_bound_probe(cellc: str, tmpdir: Path) -> dict[str, Any]:
    package_root = tmpdir / "manifest_bound_spawn_probe"
    source_root = package_root / "src"
    source_root.mkdir(parents=True)
    (package_root / "Cell.toml").write_text(BOUND_CELL_TOML, encoding="utf-8")
    (source_root / "main.cell").write_text(PROBE_SOURCE, encoding="utf-8")

    strict_result = run_command(
        [cellc, "check", "--target-profile", "ckb", "--primitive-strict", "0.16"],
        cwd=package_root,
    )
    bundle_result = run_command(
        [cellc, "audit-bundle", "--target-profile", "ckb", "--json"],
        cwd=package_root,
    )
    entries = spawn_plan_entries(package_root / "target/cellscript-audit-bundle/audit-bundle.json")
    text = f"{strict_result.get('stdout', '')}\n{strict_result.get('stderr', '')}"
    lines = [line.strip() for line in text.splitlines() if line.strip()]
    return {
        "passed": strict_result["passed"],
        "returncode": strict_result["returncode"],
        "audit_bundle_passed": bundle_result["passed"],
        "audit_bundle_returncode": bundle_result["returncode"],
        "mentions_pp0150": "PP0150" in text,
        "spawn_plan_entries": entries,
        "first_lines": lines[:12],
    }


def build_report(cellc: str, output: Path, audit_surface_path: Path) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="novaseal-spawn-probe-") as tmp:
        tmpdir = Path(tmp)
        source_path = tmpdir / "spawn_backend_probe.cell"
        source_path.write_text(PROBE_SOURCE, encoding="utf-8")

        compile_result = run_command([cellc, str(source_path), "--target-profile", "ckb"], cwd=tmpdir)
        assembly_path = source_path.with_suffix(".s")
        assembly = analyse_assembly(assembly_path) if assembly_path.exists() else {}
        strict_result = run_command(
            [cellc, str(source_path), "--target-profile", "ckb", "--primitive-strict", "0.16"],
            cwd=tmpdir,
        )
        manifest_bound_strict = build_manifest_bound_probe(cellc, tmpdir)

    spawn_helper = assembly.get("spawn_helper", {})
    spawn_with_fd_helper = assembly.get("spawn_with_fd_helper", {})
    calls = assembly.get("calls", {})
    backend_executable = bool(spawn_with_fd_helper.get("contains_ecall_instruction")) and not bool(
        spawn_with_fd_helper.get("contains_withheld_raw_syscall_2601")
    )
    all_calls_present = all(bool(calls.get(name)) for name in ["pipe", "pipe_write", "spawn_with_fd", "wait", "close"])

    strict_summary = strict_error_summary(strict_result)
    strict_rejects_spawn_target = (
        (not strict_result["passed"])
        and strict_summary["mentions_pp0150"]
        and strict_summary["mentions_spawn_target"]
    )
    manifest_bound_builder_required = any(
        entry.get("codegen_coverage_status") == "builder-required"
        for entry in manifest_bound_strict["spawn_plan_entries"]
    )
    static_cell_dep0_one_fd = bool(spawn_with_fd_helper.get("contains_static_cell_dep0_one_inherited_fd"))
    fixed_word_envelope = assembly.get("fixed_word_envelope", {})
    fixed_word_envelope_lowered = (
        fixed_word_envelope.get("fixed_u64_le_markers") == 16
        and fixed_word_envelope.get("pipe_write_instruction_count") == 18
        and fixed_word_envelope.get("contains_last_signature_word") is True
    )
    classification = (
        "cellscript_btc_bip340_verifier_surface_ready_for_lock_wiring"
        if backend_executable and fixed_word_envelope_lowered
        else "cellscript_btc_bip340_verifier_surface_compiler_blocker"
    )
    surface_status = audit_surface_status(audit_surface_path)
    remaining_blockers = []
    if not surface_status["receipt_output_materialised"]:
        remaining_blockers.append("proof_receipt_v0_output_materialisation_missing")
    if not surface_status["transaction_measurement_present"]:
        remaining_blockers.append("production_cycle_capacity_tx_size_measurement_missing")
    if not surface_status["node_verification_stack_verified"]:
        remaining_blockers.append("ckb_node_verification_stack_missing")

    return {
        "schema": SCHEMA,
        "classification": classification,
        "cellc": cellc,
        "probe_source": PROBE_SOURCE,
        "compile": {
            "passed": compile_result["passed"],
            "returncode": compile_result["returncode"],
        },
        "assembly": assembly,
        "strict_0_16": strict_summary,
        "manifest_bound_strict_0_16": manifest_bound_strict,
        "novaseal_audit_surface": surface_status,
        "status": {
            "all_spawn_ipc_calls_lowered": all_calls_present,
            "backend_ecall_boundary_closed": backend_executable,
            "spawn_with_fd_helper_executable": backend_executable,
            "spawn_helper_fail_closed_stub": bool(spawn_helper.get("contains_withheld_raw_syscall_2601"))
            and not bool(spawn_helper.get("contains_ecall_instruction")),
            "spawn_with_fd_helper_fail_closed_stub": bool(spawn_with_fd_helper.get("contains_withheld_raw_syscall_2601"))
            and not bool(spawn_with_fd_helper.get("contains_ecall_instruction")),
            "spawn_with_fd_helper_uses_static_cell_dep0_with_one_inherited_fd": static_cell_dep0_one_fd,
            "fixed_word_envelope_lowered": fixed_word_envelope_lowered,
            "generic_btc_bip340_helper_lowered": bool(
                assembly.get("generic_verifier_surface", {}).get("source_uses_btc_bip340_helper")
                and assembly.get("generic_verifier_surface", {}).get("lowers_to_spawn_with_fd")
                and assembly.get("generic_verifier_surface", {}).get("lowers_to_fixed_18_word_envelope")
            ),
            "strict_rejects_spawn_target": strict_rejects_spawn_target,
            "manifest_bound_spawn_target_strict_passes": bool(manifest_bound_strict["passed"]),
            "manifest_bound_spawn_target_builder_required": manifest_bound_builder_required,
            "ready_for_novaseal_lock_spawn_wiring": bool(
                backend_executable
                and fixed_word_envelope_lowered
                and static_cell_dep0_one_fd
                and manifest_bound_strict["passed"]
                and manifest_bound_builder_required
            ),
            "ready_for_parent_child_ckb_vm_dry_run": False,
            "combined_ckb_node_verification_stack_verified": surface_status["node_verification_stack_verified"],
        },
        "remaining_runtime_evidence_blockers": remaining_blockers,
        "limits": [
            "This is a compiler/backend probe, not a NovaSeal lock implementation.",
            "A source-level spawn_with_fd call plus a generic fixed-word envelope is not CKB VM transaction execution evidence.",
            "The current compiler wrapper resolves the static spawn target to CellDep#0 with no argv and exactly one inherited fd.",
            "Strict mode must continue to reject unmanifested, nonzero-index, and dep-group spawn targets; first-CellDep code targets are represented as builder-required obligations.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cellc", default="cellc", help="cellc binary to execute")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--audit-surface", type=Path, default=DEFAULT_AUDIT_SURFACE)
    parser.add_argument("--pretty", action="store_true", help="Pretty-print JSON output")
    args = parser.parse_args()

    report = build_report(args.cellc, args.output, args.audit_surface)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n", encoding="utf-8")

    print(f"wrote {args.output}")
    print(
        "summary: "
        f"compile_passed={report['compile']['passed']} "
        f"all_calls_lowered={report['status']['all_spawn_ipc_calls_lowered']} "
        f"generic_btc_bip340_helper_lowered={report['status']['generic_btc_bip340_helper_lowered']} "
        f"spawn_with_fd_helper_executable={report['status']['spawn_with_fd_helper_executable']} "
        f"fail_closed_stub={report['status']['spawn_with_fd_helper_fail_closed_stub']} "
        f"strict_rejects_spawn_target={report['status']['strict_rejects_spawn_target']} "
        f"manifest_bound_strict_passes={report['status']['manifest_bound_spawn_target_strict_passes']}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
