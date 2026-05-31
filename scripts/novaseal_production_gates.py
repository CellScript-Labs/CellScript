#!/usr/bin/env python3
"""Evaluate NovaSeal production gates without inventing external facts."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10 fallback is not expected in CI.
    tomllib = None  # type: ignore[assignment]


ROOT = Path(__file__).resolve().parents[1]
CORE_ROOT = ROOT / "proposals/novaseal/v0-mvp-skeleton"
AGREEMENT_ROOT = ROOT / "proposals/novaseal/agreement-profile-v0"
TARGET = ROOT / "target"
DEFAULT_OUTPUT = TARGET / "novaseal-production-gates.json"

CORE_MANIFEST = CORE_ROOT / "Cell.toml"
AGREEMENT_MANIFEST = AGREEMENT_ROOT / "Cell.toml"
CORE_LIVE = TARGET / "novaseal-devnet-stateful-live.json"
AGREEMENT_LIVE = TARGET / "novaseal-agreement-devnet-stateful-live.json"
STATEFUL_ACCEPTANCE = TARGET / "novaseal-devnet-stateful-acceptance.json"
WALLET_VECTORS = TARGET / "novaseal-wallet-signing-vectors.json"
TCB_REVIEW = TARGET / "novaseal-bip340-tcb-review.json"
PUBLIC_CELLDEP_ATTESTATION = CORE_ROOT / "proofs/public_shared_cell_dep_attestation.json"
EXTERNAL_TCB_ATTESTATION = CORE_ROOT / "proofs/bip340_external_tcb_review_attestation.json"

EXPECTED_VERIFIER = {
    "name": "cellscript_btc_bip340_verifier_riscv",
    "role": "runtime_verifier",
    "verifier_id": "btc.bip340.v0",
    "ipc_abi": "cellscript-btc-bip340-ipc-v0",
    "dep_type": "code",
    "hash_type": "data1",
}


def json_load(path: Path) -> dict[str, Any]:
    if not path.exists():
        return {"missing": True, "path": str(path.relative_to(ROOT))}
    return json.loads(path.read_text(encoding="utf-8"))


def toml_load(path: Path) -> dict[str, Any]:
    if tomllib is None:
        raise RuntimeError("Python tomllib is required for production gate validation")
    return tomllib.loads(path.read_text(encoding="utf-8"))


def normalize_hex(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    raw = value.lower()
    return raw if raw.startswith("0x") else "0x" + raw


def parse_out_point(value: Any) -> dict[str, Any]:
    if not isinstance(value, str) or ":" not in value:
        return {"valid": False, "raw": value}
    tx_hash, index = value.split(":", 1)
    return {"valid": bool(re.fullmatch(r"0x[0-9a-fA-F]{64}", tx_hash) and index.isdigit()), "tx_hash": tx_hash.lower(), "index": int(index)}


def placeholder_hash(value: str | None) -> bool:
    if not isinstance(value, str) or not re.fullmatch(r"0x[0-9a-fA-F]{64}", value):
        return True
    raw = value[2:].lower()
    return len(set(raw)) == 1


def live_verifier_facts(path: Path) -> dict[str, Any]:
    payload = json_load(path)
    verifier = payload.get("artifacts", {}).get("verifier", {})
    out_point = verifier.get("cell_dep", {}).get("out_point", {})
    index_raw = out_point.get("index")
    index = int(index_raw, 16) if isinstance(index_raw, str) and index_raw.startswith("0x") else index_raw
    return {
        "status": payload.get("status"),
        "live_devnet_rpc_executed": payload.get("live_devnet_rpc_executed"),
        "name": verifier.get("name"),
        "tx_hash": normalize_hex(out_point.get("tx_hash")),
        "index": index,
        "dep_type": verifier.get("cell_dep", {}).get("dep_type"),
        "data_hash": normalize_hex(verifier.get("data_hash")),
        "artifact_size_bytes": verifier.get("artifact_size_bytes"),
    }


def runtime_dep(manifest_path: Path) -> dict[str, Any]:
    manifest = toml_load(manifest_path)
    deps = manifest.get("deploy", {}).get("ckb", {}).get("cell_deps", [])
    matches = [dep for dep in deps if dep.get("role") == "runtime_verifier" or dep.get("name") == EXPECTED_VERIFIER["name"]]
    if len(matches) != 1:
        return {"valid": False, "error": f"expected exactly one runtime verifier dep, found {len(matches)}"}
    dep = dict(matches[0])
    parsed = parse_out_point(dep.get("out_point"))
    dep["parsed_out_point"] = parsed
    dep["production"] = manifest.get("policy", {}).get("production")
    return dep


def gate(name: str, status: str, evidence: str, detail: dict[str, Any] | None = None) -> dict[str, Any]:
    return {"name": name, "status": status, "evidence": evidence, "detail": detail or {}}


def compare_manifest_dep(manifest_path: Path, live: dict[str, Any], artifact_hash: str | None) -> dict[str, Any]:
    dep = runtime_dep(manifest_path)
    parsed = dep.get("parsed_out_point", {})
    expected = dict(EXPECTED_VERIFIER)
    checks = {
        "expected_metadata": all(dep.get(key) == value for key, value in expected.items()),
        "out_point_valid": parsed.get("valid") is True,
        "out_point_non_placeholder": not placeholder_hash(parsed.get("tx_hash")),
        "data_hash_non_placeholder": not placeholder_hash(normalize_hex(dep.get("data_hash"))),
        "artifact_hash_non_placeholder": not placeholder_hash(normalize_hex(dep.get("artifact_hash"))),
        "matches_live_tx_hash": parsed.get("tx_hash") == live.get("tx_hash"),
        "matches_live_index": parsed.get("index") == live.get("index"),
        "matches_live_data_hash": normalize_hex(dep.get("data_hash")) == live.get("data_hash"),
        "matches_live_dep_type": dep.get("dep_type") == live.get("dep_type"),
        "matches_artifact_hash": normalize_hex(dep.get("artifact_hash")) == artifact_hash,
        "production_false_until_public_attestation": dep.get("production") is False,
    }
    return {"manifest": str(manifest_path.relative_to(ROOT)), "checks": checks, "dep": dep, "live": live}


def validate_public_attestation(path: Path, artifact_hash: str | None) -> dict[str, Any]:
    if not path.exists():
        return {"status": "external_required", "reason": "missing public/shared CellDep attestation"}
    payload = json_load(path)
    verifier = payload.get("runtime_verifier", {})
    required = {
        "schema": payload.get("schema") == "novaseal-public-shared-cell-dep-attestation-v0.1",
        "status": payload.get("status") == "attested",
        "network_not_local_devnet": payload.get("network") not in {None, "", "local-devnet"},
        "artifact_hash": normalize_hex(verifier.get("artifact_hash")) == artifact_hash,
        "data_hash_non_placeholder": not placeholder_hash(normalize_hex(verifier.get("data_hash"))),
        "out_point_non_placeholder": not placeholder_hash(parse_out_point(verifier.get("out_point")).get("tx_hash")),
        "verifier_id": verifier.get("verifier_id") == EXPECTED_VERIFIER["verifier_id"],
        "ipc_abi": verifier.get("ipc_abi") == EXPECTED_VERIFIER["ipc_abi"],
    }
    return {"status": "passed" if all(required.values()) else "failed", "checks": required, "attestation": payload}


def validate_external_review(path: Path, artifact_hash: str | None) -> dict[str, Any]:
    if not path.exists():
        return {"status": "external_required", "reason": "missing external BIP340 TCB review attestation"}
    payload = json_load(path)
    required = {
        "schema": payload.get("schema") == "novaseal-bip340-external-tcb-review-attestation-v0.1",
        "status": payload.get("status") == "accepted",
        "artifact_hash": normalize_hex(payload.get("artifact_hash")) == artifact_hash,
        "verifier_id": payload.get("verifier_id") == EXPECTED_VERIFIER["verifier_id"],
        "ipc_abi": payload.get("ipc_abi") == EXPECTED_VERIFIER["ipc_abi"],
        "reviewer_present": bool(payload.get("reviewer")),
        "review_date_present": bool(payload.get("review_date")),
    }
    return {"status": "passed" if all(required.values()) else "failed", "checks": required, "attestation": payload}


def build_report() -> dict[str, Any]:
    core_live = live_verifier_facts(CORE_LIVE)
    agreement_live = live_verifier_facts(AGREEMENT_LIVE)
    wallet = json_load(WALLET_VECTORS)
    tcb = json_load(TCB_REVIEW)
    stateful_acceptance = json_load(STATEFUL_ACCEPTANCE)
    artifact_hash = normalize_hex(tcb.get("runtime_artifact", {}).get("artifact_hash"))

    core_manifest = compare_manifest_dep(CORE_MANIFEST, core_live, artifact_hash)
    agreement_manifest = compare_manifest_dep(AGREEMENT_MANIFEST, agreement_live, artifact_hash)
    public_attestation = validate_public_attestation(PUBLIC_CELLDEP_ATTESTATION, artifact_hash)
    external_review = validate_external_review(EXTERNAL_TCB_ATTESTATION, artifact_hash)

    gates = [
        gate(
            "core_manifest_local_devnet_verifier_pin",
            "passed" if all(core_manifest["checks"].values()) else "failed",
            str(CORE_MANIFEST.relative_to(ROOT)),
            core_manifest,
        ),
        gate(
            "agreement_manifest_local_devnet_verifier_pin",
            "passed" if all(agreement_manifest["checks"].values()) else "failed",
            str(AGREEMENT_MANIFEST.relative_to(ROOT)),
            agreement_manifest,
        ),
        gate(
            "wallet_molecule_signing_vectors",
            "passed" if wallet.get("status") == "passed" and wallet.get("summary", {}).get("total", 0) >= 11 else "failed",
            str(WALLET_VECTORS.relative_to(ROOT)),
            wallet.get("summary", {}),
        ),
        gate(
            "bip340_runtime_verifier_local_tcb_review",
            "passed" if str(tcb.get("status", "")).startswith("passed_local_review") else "failed",
            str(TCB_REVIEW.relative_to(ROOT)),
            {
                "status": tcb.get("status"),
                "artifact_hash": artifact_hash,
                "external_review_required": tcb.get("external_review", {}).get("required_for_production"),
            },
        ),
        gate(
            "live_local_devnet_stateful_core_and_agreement",
            "passed"
            if stateful_acceptance.get("status") == "passed"
            and stateful_acceptance.get("blocker_count") == 0
            and stateful_acceptance.get("live_devnet_rpc_executed") is True
            and stateful_acceptance.get("stateful_lifecycle_executed") is True
            else "failed",
            (
                "target/novaseal-devnet-stateful-acceptance.json + "
                "target/novaseal-devnet-stateful-live.json + "
                "target/novaseal-agreement-devnet-stateful-live.json"
            ),
            {
                "acceptance": {
                    "status": stateful_acceptance.get("status"),
                    "blocker_count": stateful_acceptance.get("blocker_count"),
                    "live_devnet_rpc_executed": stateful_acceptance.get("live_devnet_rpc_executed"),
                    "stateful_lifecycle_executed": stateful_acceptance.get("stateful_lifecycle_executed"),
                    "missing": stateful_acceptance.get("missing"),
                },
                "core": core_live,
                "agreement": agreement_live,
            },
        ),
        gate(
            "public_shared_cell_dep_pinning_attestation",
            public_attestation["status"],
            str(PUBLIC_CELLDEP_ATTESTATION.relative_to(ROOT)),
            public_attestation,
        ),
        gate(
            "external_bip340_runtime_verifier_tcb_review_attestation",
            external_review["status"],
            str(EXTERNAL_TCB_ATTESTATION.relative_to(ROOT)),
            external_review,
        ),
    ]
    local_gates = [row for row in gates if row["status"] != "external_required" and not row["name"].startswith("public_shared") and not row["name"].startswith("external_bip340")]
    local_ready = all(row["status"] == "passed" for row in local_gates)
    production_ready = all(row["status"] == "passed" for row in gates)
    if production_ready:
        status = "production_ready"
    elif local_ready and any(row["status"] == "external_required" for row in gates):
        status = "local_production_prep_ready_external_attestation_required"
    else:
        status = "failed"
    return {
        "schema": "novaseal-production-gates-v0.1",
        "status": status,
        "production_ready": production_ready,
        "local_production_prep_ready": local_ready,
        "runtime_artifact_hash": artifact_hash,
        "gates": gates,
        "policy": {
            "no_placeholder_closure": "production remains false until public/shared CellDep and external TCB attestations are present",
            "attestation_templates": [
                "proposals/novaseal/v0-mvp-skeleton/proofs/public_shared_cell_dep_attestation.template.json",
                "proposals/novaseal/v0-mvp-skeleton/proofs/bip340_external_tcb_review_attestation.template.json",
            ],
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    parser.add_argument("--require-production", action="store_true")
    args = parser.parse_args()
    report = build_report()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.pretty:
        print(
            f"wrote {args.output} status={report['status']} "
            f"local_ready={report['local_production_prep_ready']} production_ready={report['production_ready']}"
        )
        for row in report["gates"]:
            print(f"- {row['name']}: {row['status']}")
    if args.require_production:
        return 0 if report["production_ready"] else 1
    return 0 if report["local_production_prep_ready"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
