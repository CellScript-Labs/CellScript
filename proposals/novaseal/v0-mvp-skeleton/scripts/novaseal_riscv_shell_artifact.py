#!/usr/bin/env python3
"""Stage and verify the NovaSeal RISC-V verifier shell artifact.

This preflight deliberately does not claim CKB VM execution. It freezes the
exact verifier ELF that the current lock wiring spawns, checks that the shell
matches the BIP340 IPC vectors, and confirms the generated audit surface exposes
the intended spawn/IPC records.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
from pathlib import Path
from typing import Any


SCHEMA = "novaseal-riscv-shell-artifact-v0.1"

DEFAULT_RELEASE_ELF = Path(
    "verifier/novaseal_btc_verifier_riscv/target/"
    "riscv64imac-unknown-none-elf/release/novaseal_btc_verifier_riscv"
)
DEFAULT_STAGED_ELF = Path("target/novaseal-btc-verifier-riscv-shell-release.elf")
DEFAULT_STAGED_SHA256 = Path("target/novaseal-btc-verifier-riscv-shell-release.elf.sha256")
DEFAULT_SHELL_REPORT = Path("target/novaseal-btc-verifier-shell-report.json")
DEFAULT_AUDIT_SURFACE = Path("target/novaseal-audit-surface.json")
DEFAULT_OUTPUT = Path("target/novaseal-riscv-shell-artifact.json")


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"missing JSON input: {path}") from None
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from None
    if not isinstance(value, dict):
        raise SystemExit(f"expected JSON object in {path}")
    return value


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    try:
        with path.open("rb") as fh:
            for chunk in iter(lambda: fh.read(1024 * 1024), b""):
                digest.update(chunk)
    except FileNotFoundError:
        raise SystemExit(f"missing ELF input: {path}") from None
    return digest.hexdigest()


def file_info(path: Path) -> dict[str, Any]:
    return {
        "path": str(path),
        "size_bytes": path.stat().st_size,
        "sha256": file_sha256(path),
    }


def write_sha256_file(path: Path, elf_path: Path, digest: str) -> None:
    path.write_text(f"{digest}  {elf_path}\n", encoding="utf-8")


def compact_record(record: dict[str, Any], keys: list[str]) -> dict[str, Any]:
    return {key: record.get(key) for key in keys if key in record}


def generated_spawn_records(audit_surface: dict[str, Any]) -> dict[str, Any]:
    plan_records = []
    for record in audit_surface.get("proof_plan", []):
        if not isinstance(record, dict):
            continue
        haystack = json.dumps(record, sort_keys=True)
        if "spawn" in haystack or "btc-verifier" in haystack or "bip340" in haystack:
            plan_records.append(
                compact_record(
                    record,
                    [
                        "origin",
                        "category",
                        "feature",
                        "status",
                        "codegen_coverage_status",
                        "on_chain_checked",
                        "detail",
                    ],
                )
            )

    runtime_accesses = []
    for section in ("actions", "locks"):
        for entry in audit_surface.get(section, []):
            if not isinstance(entry, dict):
                continue
            for access in entry.get("runtime_accesses", []):
                if not isinstance(access, dict):
                    continue
                haystack = json.dumps(access, sort_keys=True)
                if "spawn" in haystack or "pipe" in haystack or "wait" in haystack:
                    runtime_accesses.append({"entry_section": section, "entry_name": entry.get("name"), **access})

    return {
        "generated_spawn_or_crypto_proof_plan_records": plan_records,
        "generated_spawn_or_pipe_runtime_accesses": runtime_accesses,
        "proof_plan_record_count": len(plan_records),
        "runtime_access_count": len(runtime_accesses),
    }


def shell_report_summary(report: dict[str, Any]) -> dict[str, Any]:
    summary = report.get("summary", {})
    spawn_input = report.get("spawn_input", {})
    exit_codes = report.get("exit_codes", {})
    if not isinstance(summary, dict):
        summary = {}
    if not isinstance(spawn_input, dict):
        spawn_input = {}
    if not isinstance(exit_codes, dict):
        exit_codes = {}
    return {
        "classification": report.get("classification"),
        "summary": compact_record(
            summary,
            [
                "total_vectors",
                "parse_ok",
                "parse_rejected",
                "spawn_word_representable",
                "spawn_word_roundtrip",
                "spawn_io_rejects",
                "accepted",
                "rejected",
                "expected_accept",
                "expected_reject",
                "matched_expected",
                "all_expected_matched",
            ],
        ),
        "spawn_input": compact_record(spawn_input, ["fd_index", "word_count", "word_width_bytes", "blob_len", "endianness"]),
        "exit_codes": compact_record(exit_codes, ["accept", "reject_crypto", "reject_envelope", "reject_spawn_io"]),
    }


def build_report(args: argparse.Namespace) -> dict[str, Any]:
    release_info = file_info(args.release_elf)

    if args.sync:
        args.staged_elf.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(args.release_elf, args.staged_elf)

    staged_info = file_info(args.staged_elf)
    staged_matches_release = (
        release_info["size_bytes"] == staged_info["size_bytes"] and release_info["sha256"] == staged_info["sha256"]
    )

    if args.sync:
        write_sha256_file(args.staged_sha256, args.staged_elf, staged_info["sha256"])

    shell_report = load_json(args.shell_report)
    audit_surface = load_json(args.audit_surface)
    shell_summary = shell_report_summary(shell_report)
    shell_vectors_match = (
        bool(shell_summary["summary"].get("all_expected_matched"))
        and shell_summary["summary"].get("accepted") == shell_summary["summary"].get("expected_accept")
    )
    spawn_surface = generated_spawn_records(audit_surface)
    audit_summary = audit_surface.get("summary", {})
    if not isinstance(audit_summary, dict):
        audit_summary = {}

    strict_surface_clean = audit_summary.get("runtime_gaps") == 0 and audit_summary.get("strict_prediction_errors") == 0
    generated_spawn_visible = spawn_surface["proof_plan_record_count"] > 0 and spawn_surface["runtime_access_count"] > 0

    return {
        "schema": SCHEMA,
        "classification": "riscv_shell_artifact_preflight",
        "source_release_elf": release_info,
        "staged_release_elf": staged_info,
        "staged_sha256_file": str(args.staged_sha256),
        "staged_matches_release": staged_matches_release,
        "shell_report": shell_summary,
        "audit_surface": {
            "source": str(args.audit_surface),
            "summary": compact_record(
                audit_summary,
                ["actions", "locks", "proof_plan_records", "runtime_gaps", "strict_prediction_errors"],
            ),
            "strict_surface_clean": strict_surface_clean,
            "generated_spawn_visible": generated_spawn_visible,
            **spawn_surface,
        },
        "status": {
            "preflight_passed": staged_matches_release and shell_vectors_match and strict_surface_clean and generated_spawn_visible,
            "lock_wiring_status": "wired_to_bip340_shell",
            "ready_for_ckb_vm_dry_run": staged_matches_release and shell_vectors_match and generated_spawn_visible,
            "production_ready": False,
        },
        "limits": [
            "The staged ELF matches BIP340 IPC vectors at model/unit-test level, not through a CKB VM transaction.",
            "The generated CellScript audit surface has lock spawn/pipe/wait records, but this artifact preflight is not parent-lock CKB VM execution evidence.",
            "This artifact preflight does not execute the staged ELF through inherited fd/pipe IPC; use harness/ckb_vm for child-verifier VM evidence.",
            "No occupied capacity or tx-size evidence is produced by this artifact preflight.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release-elf", type=Path, default=DEFAULT_RELEASE_ELF)
    parser.add_argument("--staged-elf", type=Path, default=DEFAULT_STAGED_ELF)
    parser.add_argument("--staged-sha256", type=Path, default=DEFAULT_STAGED_SHA256)
    parser.add_argument("--shell-report", type=Path, default=DEFAULT_SHELL_REPORT)
    parser.add_argument("--audit-surface", type=Path, default=DEFAULT_AUDIT_SURFACE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--sync", action="store_true", help="Copy the current release ELF into target/ before hashing")
    parser.add_argument("--pretty", action="store_true", help="Pretty-print JSON output")
    args = parser.parse_args()

    report = build_report(args)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n", encoding="utf-8")

    print(f"wrote {args.output}")
    print(
        "summary: "
        f"preflight_passed={report['status']['preflight_passed']} "
        f"staged_matches_release={report['staged_matches_release']} "
        f"size={report['staged_release_elf']['size_bytes']} "
        f"sha256={report['staged_release_elf']['sha256']} "
        f"generated_spawn_visible={report['audit_surface']['generated_spawn_visible']}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
