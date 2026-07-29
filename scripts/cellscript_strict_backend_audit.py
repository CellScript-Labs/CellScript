#!/usr/bin/env python3
import json
import os
import subprocess
import sys
import time
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


FEATURE_IDS = [
    "ir.cfg.block-id-uniqueness",
    "ir.cfg.terminator-targets",
    "ir.cfg.reachability",
    "ir.defs.must-define-before-use",
    "ir.abi.call-arg-types",
    "ir.abi.return-types",
    "codegen.psabi.sp-delta-alignment",
    "codegen.psabi.outgoing-stack-args-0-through-20",
    "codegen.tuple-return-register-contract",
    "codegen.runtime-fail-closed-syscall-contracts",
    "riscv.oracle.core-instruction-bytes",
    "riscv.oracle.immediate-boundaries",
    "riscv.branch-relaxation.near-and-far",
    "riscv.machine-cfg.layout-coverage",
    "riscv.elf.header-and-segment-layout",
    "edge.match-wildcard-order",
    "edge.tuple-projection-through-branching",
    "edge.bytestring-length",
    "edge.import-alias-callable-rename",
    "metamorphic.numeric-type-equality-commutative",
    "acceptance.syntax-combo",
    "acceptance.ckb-stateful-scenarios",
]


def command_plan(mode: str) -> list[dict]:
    commands = [
        {
            "id": "strict-rust-contract-tests",
            "feature_ids": [
                "ir.cfg.block-id-uniqueness",
                "ir.cfg.terminator-targets",
                "ir.cfg.reachability",
                "ir.defs.must-define-before-use",
                "ir.abi.call-arg-types",
                "ir.abi.return-types",
                "codegen.psabi.sp-delta-alignment",
                "riscv.oracle.core-instruction-bytes",
                "riscv.oracle.immediate-boundaries",
                "riscv.elf.header-and-segment-layout",
            ],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "strict_audit", "--", "--nocapture"],
        },
        {
            "id": "outgoing-stack-abi-matrix",
            "feature_ids": ["codegen.psabi.outgoing-stack-args-0-through-20"],
            "argv": [
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "outgoing_stack_arg_area_is_16_byte_aligned_at_call_boundaries",
                "--",
                "--nocapture",
            ],
        },
        {
            "id": "assembler-emitted-surface",
            "feature_ids": ["riscv.machine-cfg.layout-coverage"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "internal_assembler_encodes_emitted_instruction_surface", "--", "--nocapture"],
        },
        {
            "id": "branch-relaxation-contracts",
            "feature_ids": ["riscv.branch-relaxation.near-and-far"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "relaxes", "--", "--nocapture"],
        },
        {
            "id": "tuple-return-abi-contracts",
            "feature_ids": ["codegen.tuple-return-register-contract"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "tuple_return_abi_rejects_more_than_eight_fields", "--", "--nocapture"],
        },
        {
            "id": "runtime-fail-closed-contracts",
            "feature_ids": ["codegen.runtime-fail-closed-syscall-contracts"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "ckb_u64_syscall_helpers_check_return_code_and_size", "--", "--nocapture"],
        },
        {
            "id": "backend-shape-contracts",
            "feature_ids": ["riscv.machine-cfg.layout-coverage"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "bundled_examples_stay_within_backend_shape_budgets", "--", "--nocapture"],
        },
        {
            "id": "wildcard-match-order-contract",
            "feature_ids": ["edge.match-wildcard-order"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "compile_rejects_invalid_enum_match_patterns", "--", "--nocapture"],
        },
        {
            "id": "tuple-projection-branching-contracts",
            "feature_ids": ["edge.tuple-projection-through-branching"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "compile_preserves_", "--", "--nocapture"],
        },
        {
            "id": "bytestring-length-contracts",
            "feature_ids": ["edge.bytestring-length"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "byte_string", "--", "--nocapture"],
        },
        {
            "id": "import-alias-callable-rename-contract",
            "feature_ids": ["edge.import-alias-callable-rename"],
            "argv": [
                "cargo",
                "test",
                "--locked",
                "-p",
                "cellscript",
                "compile_package_import_alias_emits_matching_external_callable",
                "--",
                "--nocapture",
            ],
        },
        {
            "id": "numeric-type-equality-metamorphic-contract",
            "feature_ids": ["metamorphic.numeric-type-equality-commutative"],
            "argv": ["cargo", "test", "--locked", "-p", "cellscript", "numeric_named_type_equality_is_commutative", "--", "--nocapture"],
        },
    ]
    if mode in {"ci", "full", "nightly"}:
        commands.append(
            {
                "id": "syntax-combo-audit",
                "feature_ids": ["acceptance.syntax-combo"],
                "argv": ["scripts/cellscript_syntax_combo_audit.sh", "ci"],
            }
        )
    if mode in {"full", "nightly"}:
        commands.append(
            {
                "id": "ckb-stateful-scenarios",
                "feature_ids": ["acceptance.ckb-stateful-scenarios"],
                "argv": ["scripts/cellscript_ckb_stateful_scenarios.sh"],
            }
        )
    return commands


def run_command(spec: dict) -> dict:
    started = time.time()
    proc = subprocess.run(spec["argv"], cwd=ROOT, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    duration = round(time.time() - started, 3)
    output = (proc.stdout + "\n" + proc.stderr).strip()
    return {
        "id": spec["id"],
        "feature_ids": spec["feature_ids"],
        "argv": spec["argv"],
        "status": "passed" if proc.returncode == 0 else "failed",
        "exit_code": proc.returncode,
        "duration_seconds": duration,
        "output_tail": output[-12000:],
    }


def default_report_path(mode: str) -> Path:
    stamp = time.strftime("%Y%m%d-%H%M%S")
    return ROOT / "target" / "cellscript-strict-backend-audit" / f"strict-backend-audit-{mode}-{stamp}.json"


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "quick"
    if mode not in {"quick", "ci", "full", "nightly"}:
        print("usage: cellscript_strict_backend_audit.py [quick|ci|full|nightly]", file=sys.stderr)
        return 2

    report_path = Path(os.environ.get("CELLSCRIPT_STRICT_BACKEND_AUDIT_REPORT", default_report_path(mode)))
    report_path.parent.mkdir(parents=True, exist_ok=True)

    commands = command_plan(mode)
    results = []
    tested = set()
    for spec in commands:
        print(f"==> {spec['id']}: {' '.join(spec['argv'])}", flush=True)
        result = run_command(spec)
        results.append(result)
        if result["status"] == "passed":
            tested.update(result["feature_ids"])

    missing = sorted(set(FEATURE_IDS) - tested)
    failed = [result["id"] for result in results if result["status"] != "passed"]
    report = {
        "audit": "cellscript-strict-codegen-ir-riscv",
        "mode": mode,
        "status": "failed" if failed else "passed",
        "feature_ids": FEATURE_IDS,
        "tested_feature_ids": sorted(tested),
        "missing_feature_ids": missing,
        "failed_commands": failed,
        "artifact_hashes": [],
        "ckb_vm": {"cycles": None, "transaction_size_bytes": None},
        "commands": results,
    }
    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(f"strict backend audit report: {report_path}")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
