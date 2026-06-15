#!/usr/bin/env python3
"""Build and inspect the NovaSeal parent lock ELF/ASM ABI surface.

This preflight is intentionally narrower than a CKB VM transaction dry-run. It
checks that the generated parent lock artifact is ready for a parent/child VM
harness: Script.args is used for lock_args, lock_args are not rebound from input
cell data, the protected cell is still bound from Input#0, and the spawn_with_fd
path remains visible.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


SCHEMA = "novaseal-parent-lock-abi-preflight-v0.1"
DEFAULT_CELLC = Path(__file__).resolve().parents[4] / "target/debug/cellc"
DEFAULT_SOURCE = Path("src/nova_btc_authority_lock.cell")
DEFAULT_OUTPUT = Path("target/novaseal-parent-lock-abi-preflight.json")


def run_cellc(cellc: Path, source: Path, target: str) -> dict[str, Any]:
    artifact = Path("target") / f"novaseal-parent-lock-abi-preflight.{target_suffix(target)}"
    completed = subprocess.run(
        [
            str(cellc),
            str(source),
            "--entry-lock",
            "btc_authority",
            "--target-profile",
            "ckb",
            "--target",
            target,
            "-o",
            str(artifact),
        ],
        text=True,
        capture_output=True,
        check=False,
    )
    result: dict[str, Any] = {
        "target": target,
        "source": str(source),
        "status_code": completed.returncode,
        "stdout": completed.stdout,
        "stderr": completed.stderr,
    }
    if completed.returncode == 0:
        data = artifact.read_bytes()
        result["summary"] = {
            "artifact": str(artifact),
            "artifact_format": target_suffix(target),
            "artifact_hash": "0x" + hashlib.blake2b(data, digest_size=32).hexdigest(),
            "artifact_size_bytes": len(data),
            "status": "ok",
        }
    return result


def target_suffix(target: str) -> str:
    if target.endswith("-asm"):
        return "s"
    if target.endswith("-elf"):
        return "elf"
    return target.replace("/", "_").replace("-", "_")


def read_artifact_text(build_result: dict[str, Any]) -> str:
    artifact = build_result.get("summary", {}).get("artifact")
    if not isinstance(artifact, str):
        return ""
    try:
        return Path(artifact).read_text(encoding="utf-8")
    except OSError:
        return ""


def parent_lock_checks(asm: str) -> dict[str, Any]:
    script_args_decode = asm.split("# cellscript entry abi: lock_args param expected_btc_authority_hash consumes").pop(0)
    checks = {
        "load_script_args_visible": "# cellscript abi: LOAD_SCRIPT reason=entry_lock_args" in asm,
        "expected_btc_authority_hash_from_lock_args": (
            "# cellscript entry abi: lock_args param expected_btc_authority_hash consumes 32 script arg byte(s)" in asm
        ),
        "script_args_u32_decoder_pointer_safe": "lbu t0, 1(t0)" not in script_args_decode,
        "lock_args_not_rebound_from_input_cell_data": (
            "bind read-only param expected_btc_authority_hash to Input#" not in asm
            and "bind read-only param expected_btc_authority_hash to CellDep#" not in asm
        ),
        "protected_cell_bound_from_input0": "bind read-only param cell to Input#0 cell data" in asm,
        "spawn_with_fd_helper_visible": "__ckb_spawn_with_fd1" in asm,
        "vm2_spawn_syscall_visible": "li a7, 2601" in asm,
        "vm2_wait_syscall_visible": "li a7, 2602" in asm,
        "vm2_pipe_syscalls_visible": all(marker in asm for marker in ["li a7, 2604", "li a7, 2605", "li a7, 2608"]),
    }
    return {
        **checks,
        "passed": all(checks.values()),
    }


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    asm_result = run_cellc(args.cellc, args.source, "riscv64-asm")
    elf_result = run_cellc(args.cellc, args.source, "riscv64-elf")
    asm = read_artifact_text(asm_result)
    checks = parent_lock_checks(asm)
    builds_ok = asm_result["status_code"] == 0 and elf_result["status_code"] == 0
    return {
        "schema": SCHEMA,
        "classification": "parent_lock_elf_abi_preflight",
        "cellc": str(args.cellc),
        "source": str(args.source),
        "builds": {
            "asm": asm_result.get("summary", {"status_code": asm_result["status_code"], "stderr": asm_result["stderr"]}),
            "elf": elf_result.get("summary", {"status_code": elf_result["status_code"], "stderr": elf_result["stderr"]}),
        },
        "checks": checks,
        "status": {
            "preflight_passed": builds_ok and checks["passed"],
            "parent_lock_elf_built": elf_result["status_code"] == 0,
            "parent_lock_asm_built": asm_result["status_code"] == 0,
            "parent_lock_ckb_vm_executed": False,
            "parent_spawn_executed": False,
            "ready_for_parent_child_ckb_vm_harness": builds_ok and checks["passed"],
            "production_ready": False,
        },
        "limits": [
            "This preflight inspects generated parent lock artifacts; it does not execute CKB VM bytecode.",
            "The child verifier CKB VM harness is separate and does not prove parent lock spawn/wait behaviour.",
            "No transaction, capacity, tx-size, or full parent/child execution transcript is produced here.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cellc", type=Path, default=DEFAULT_CELLC)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n", encoding="utf-8")

    status = report["status"]
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"preflight_passed={status['preflight_passed']} "
        f"parent_lock_elf_built={status['parent_lock_elf_built']} "
        f"ready_for_parent_child_ckb_vm_harness={status['ready_for_parent_child_ckb_vm_harness']} "
        f"parent_lock_ckb_vm_executed={status['parent_lock_ckb_vm_executed']}"
    )
    return 0 if status["preflight_passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
