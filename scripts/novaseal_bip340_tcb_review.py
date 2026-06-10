#!/usr/bin/env python3
"""Build the local NovaSeal BIP340 runtime-verifier TCB review bundle.

This report is deliberately not an external audit attestation. It collects the
local facts needed before asking a reviewer to sign off on the runtime verifier
TCB: source hashes, artifact hash, vector coverage, IPC coverage, and CKB VM
harness coverage.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
CORE_ROOT = ROOT / "proposals/novaseal/v0-mvp-skeleton"
TARGET = ROOT / "target"
DEFAULT_OUTPUT = TARGET / "novaseal-bip340-tcb-review.json"

VERIFIER_DIRS = [
    CORE_ROOT / "verifier/novaseal_btc_verifier_core",
    CORE_ROOT / "verifier/novaseal_btc_verifier_riscv",
    CORE_ROOT / "verifier/novaseal_btc_verifier",
]

REPORTS = {
    "reference_vectors": CORE_ROOT / "target/novaseal-btc-verifier-vectors.json",
    "ipc_vectors": CORE_ROOT / "target/novaseal-btc-verifier-ipc-vectors.json",
    "shell_report": CORE_ROOT / "target/novaseal-btc-verifier-shell-report.json",
    "riscv_artifact": CORE_ROOT / "target/novaseal-riscv-shell-artifact.json",
    "child_verifier_ckb_vm": CORE_ROOT / "target/novaseal-ckb-vm-child-verifier-report.json",
    "parent_lock_ckb_vm": CORE_ROOT / "target/novaseal-parent-lock-ckb-vm-report.json",
    "combined_tx_ckb_vm": CORE_ROOT / "target/novaseal-combined-tx-report.json",
    "core_live_devnet": TARGET / "novaseal-devnet-stateful-live.json",
    "agreement_live_devnet": TARGET / "novaseal-agreement-devnet-stateful-live.json",
}


def json_load(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"missing": True, "path": str(path.relative_to(ROOT))}
    return json.loads(path.read_text(encoding="utf-8"))


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            h.update(chunk)
    return "0x" + h.hexdigest()


def git_commit() -> str | None:
    try:
        return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def source_files() -> list[Path]:
    files: list[Path] = []
    for root in VERIFIER_DIRS:
        for path in root.rglob("*"):
            if not path.is_file() or "target" in path.parts:
                continue
            if path.suffix == ".rs" or path.name in {"Cargo.toml", "Cargo.lock", "README.md"}:
                files.append(path)
    return sorted(files)


def source_inventory() -> dict[str, Any]:
    files = source_files()
    file_rows = []
    tree_hash = hashlib.sha256()
    unsafe_hits = []
    review_hits = []
    for path in files:
        rel = path.relative_to(ROOT).as_posix()
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        text = data.decode("utf-8", errors="replace")
        line_count = text.count("\n") + (0 if text.endswith("\n") else 1)
        file_rows.append({"path": rel, "sha256": "0x" + digest, "lines": line_count})
        tree_hash.update(rel.encode("utf-8"))
        tree_hash.update(b"\0")
        tree_hash.update(bytes.fromhex(digest))
        for idx, line in enumerate(text.splitlines(), start=1):
            stripped = line.strip()
            if "unsafe" in stripped:
                unsafe_hits.append({"path": rel, "line": idx, "text": stripped})
            if any(token in stripped for token in ("TODO", "todo!", "unimplemented!", "panic!")):
                review_hits.append({"path": rel, "line": idx, "text": stripped})
    return {
        "source_tree_sha256": "0x" + tree_hash.hexdigest(),
        "files": file_rows,
        "total_files": len(file_rows),
        "total_lines": sum(row["lines"] for row in file_rows),
        "unsafe_hits": unsafe_hits,
        "review_hits": review_hits,
    }


def gate(name: str, passed: bool, evidence: str, detail: dict[str, Any] | None = None) -> dict[str, Any]:
    return {
        "name": name,
        "status": "passed" if passed else "failed",
        "evidence": evidence,
        "detail": detail or {},
    }


def build_report() -> dict[str, Any]:
    reports = {name: json_load(path) for name, path in REPORTS.items()}
    vectors = reports["reference_vectors"].get("summary", {})
    ipc = reports["ipc_vectors"].get("summary", {})
    shell = reports["shell_report"].get("summary", {})
    artifact = reports["riscv_artifact"]
    child = reports["child_verifier_ckb_vm"].get("summary", {})
    parent = reports["parent_lock_ckb_vm"].get("summary", {})
    combined = reports["combined_tx_ckb_vm"].get("summary", {})
    core_live = reports["core_live_devnet"]
    agreement_live = reports["agreement_live_devnet"]

    artifact_sha = artifact.get("staged_release_elf", {}).get("sha256")
    if artifact_sha and not artifact_sha.startswith("0x"):
        artifact_sha = "0x" + artifact_sha

    gates = [
        gate(
            "reference_bip340_vectors",
            vectors.get("positive_self_verified", 0) > 0
            and vectors.get("positive_self_verified") == vectors.get("positive_vectors")
            and vectors.get("negative_self_rejected") == vectors.get("negative_vectors"),
            "target/novaseal-btc-verifier-vectors.json",
            vectors,
        ),
        gate(
            "fixed_ipc_vectors",
            ipc.get("expected_accept", 0) > 0
            and ipc.get("expected_reject", 0) > 0
            and ipc.get("total_vectors") == ipc.get("expected_accept", 0) + ipc.get("expected_reject", 0),
            "target/novaseal-btc-verifier-ipc-vectors.json",
            ipc,
        ),
        gate(
            "riscv_shell_spawn_word_report",
            shell.get("all_expected_matched") is True and shell.get("matched_expected") == shell.get("total_vectors"),
            "target/novaseal-btc-verifier-shell-report.json",
            shell,
        ),
        gate(
            "riscv_artifact_preflight",
            artifact.get("staged_matches_release") is True
            and artifact.get("status", {}).get("preflight_passed") is True
            and artifact.get("status", {}).get("ready_for_ckb_vm_dry_run") is True,
            "target/novaseal-riscv-shell-artifact.json",
            {
                "artifact_hash": artifact_sha,
                "size_bytes": artifact.get("staged_release_elf", {}).get("size_bytes"),
                "production_ready_claim": artifact.get("status", {}).get("production_ready"),
            },
        ),
        gate(
            "child_verifier_ckb_vm",
            child.get("child_verifier_ckb_vm_executed") is True
            and child.get("matched_expected") == child.get("total_cases")
            and child.get("mismatched") == 0,
            "target/novaseal-ckb-vm-child-verifier-report.json",
            child,
        ),
        gate(
            "parent_lock_spawn_ckb_vm",
            parent.get("parent_spawn_executed") is True
            and parent.get("child_verifier_ckb_vm_executed") is True
            and parent.get("full_transaction_verifier_matched_expected") is True
            and parent.get("matched_expected") == parent.get("total_cases"),
            "target/novaseal-parent-lock-ckb-vm-report.json",
            parent,
        ),
        gate(
            "combined_lock_type_node_stack",
            (
                (
                    combined.get("ckb_node_verification_stack_executed") is True
                    and combined.get("node_stack_matched_expected") == combined.get("total_cases")
                )
                or (
                    combined.get("combined_full_transaction_executed") is True
                    and combined.get("matched_expected") == combined.get("total_cases")
                    and combined.get("lock_and_type_script_groups_present") is True
                )
            )
            and combined.get("child_spawn_target_cell_dep0_modelled") is True,
            "target/novaseal-combined-tx-report.json",
            combined,
        ),
        gate(
            "live_local_devnet_core_and_agreement",
            core_live.get("status") == "passed"
            and core_live.get("live_devnet_rpc_executed") is True
            and agreement_live.get("status") == "passed"
            and agreement_live.get("live_devnet_rpc_executed") is True,
            "target/novaseal-devnet-stateful-live.json + target/novaseal-agreement-devnet-stateful-live.json",
            {
                "core_status": core_live.get("status"),
                "agreement_status": agreement_live.get("status"),
                "core_verifier_data_hash": core_live.get("artifacts", {}).get("verifier", {}).get("data_hash"),
                "agreement_verifier_data_hash": agreement_live.get("artifacts", {}).get("verifier", {}).get("data_hash"),
            },
        ),
    ]

    local_passed = all(row["status"] == "passed" for row in gates)
    inventory = source_inventory()
    return {
        "schema": "novaseal-bip340-tcb-review-v0.1",
        "status": "passed_local_review_external_attestation_required" if local_passed else "failed",
        "repo_commit": git_commit(),
        "verifier_id": "btc.bip340.v0",
        "ipc_abi": "cellscript-btc-bip340-ipc-v0",
        "runtime_artifact": {
            "name": "cellscript_btc_bip340_verifier_riscv",
            "role": "runtime_verifier",
            "artifact_hash": artifact_sha,
            "artifact_hash_algorithm": "sha256",
            "size_bytes": artifact.get("staged_release_elf", {}).get("size_bytes"),
        },
        "local_review_gates": gates,
        "source_inventory": inventory,
        "tcb_boundary": {
            "included": [
                "BIP340 verifier core",
                "RISC-V spawn/pipe/wait shell",
                "IPC envelope parser",
                "artifact hash used by NovaSeal manifests",
            ],
            "excluded": [
                "NovaSeal .cell protocol code",
                "CKB node implementation",
                "test harness Rust used only to construct evidence",
                "wallet UI implementation",
            ],
        },
        "external_review": {
            "required_for_production": True,
            "attestation_file": "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.json",
            "template": "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json",
            "status": "missing_attestation",
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()
    report = build_report()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.pretty:
        print(
            f"wrote {args.output} status={report['status']} "
            f"artifact={report['runtime_artifact']['artifact_hash']} "
            f"local_gates={len(report['local_review_gates'])}"
        )
    return 0 if report["status"].startswith("passed_local_review") else 1


if __name__ == "__main__":
    raise SystemExit(main())
