#!/usr/bin/env python3
"""Generate NovaSeal planned-profile operator signing fixtures.

This report is the profile-specific companion to the core/agreement wallet
vectors. It binds each planned profile action to its fixture, current source
tree, schema set, invariant matrix, signing witnesses, display payload, and
live-report transaction skeleton where local stateful evidence exists.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_OUTPUT = ROOT / "target/novaseal-profile-operator-fixtures.json"

CKB_HASH_PERSONAL = b"ckb-default-hash"
REPORT_PERSON = b"NovaProfileFxV0"
PACKED_DOMAIN = b"NovaSealProfileOperatorFixtureV0\x00"


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def ckb_blake2b256(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=32, person=CKB_HASH_PERSONAL).digest()


def report_hash(label: str, value: Any) -> str:
    h = hashlib.blake2b(digest_size=32, person=REPORT_PERSON)
    h.update(label.encode("utf-8"))
    h.update(b"\x00")
    h.update(canonical_json(value))
    return hex0x(h.digest())


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode("utf-8")


def json_file_hash(path: Path) -> str:
    return report_hash(path.name, json.loads(path.read_text(encoding="utf-8")))


def file_set_hash(paths: list[Path]) -> str:
    entries = []
    for path in sorted(paths):
        if path.is_symlink() or not path.is_file():
            continue
        entries.append({"path": str(path.relative_to(ROOT)), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    return report_hash("file_set", entries)


def source_tree_hash(root: Path) -> str:
    return file_set_hash(sorted(root.glob("*.cell")))


def schema_set_hash(root: Path) -> str:
    return file_set_hash(sorted(root.glob("*.schema")))


def packed_hash(type_name: str, packed: bytes) -> tuple[str, str]:
    preimage = PACKED_DOMAIN + type_name.encode("utf-8") + b"\x00" + len(packed).to_bytes(4, "little") + packed
    return hex0x(preimage), hex0x(ckb_blake2b256(preimage))


def json_pointer(value: Any, pointer: str) -> Any:
    current = value
    for raw in pointer.strip("/").split("/"):
        if raw == "":
            continue
        key = raw.replace("~1", "/").replace("~0", "~")
        if isinstance(current, dict):
            current = current.get(key)
        else:
            return None
    return current


PROFILE_CASES: list[dict[str, Any]] = [
    {
        "profile": "fungible-xudt-profile-v0",
        "root": "proposals/novaseal/fungible-xudt-profile-v0",
        "signed_type": "NovaFungibleXudtSignedIntentV0",
        "live_report": "target/novaseal-fungible-xudt-devnet-stateful-live.json",
        "cases": [
            ("issue_xudt", "issue_valid.json", ["issuer"], "/issue/commit/tx_hash"),
            ("transfer_xudt", "transfer_valid.json", ["holder"], "/transfer/commit/tx_hash"),
            ("settle_xudt", "settle_valid.json", ["holder"], "/settle/commit/tx_hash"),
        ],
    },
    {
        "profile": "rwa-receipt-profile-v0",
        "root": "proposals/novaseal/rwa-receipt-profile-v0",
        "signed_type": "NovaRwaReceiptSignedIntentV0",
        "live_report": "target/novaseal-rwa-receipt-devnet-stateful-live.json",
        "cases": [
            ("materialize_rwa_receipt", "materialize_valid.json", ["issuer"], "/materialize/commit/tx_hash"),
            ("claim_rwa_receipt", "claim_valid.json", ["holder"], "/claim/commit/tx_hash"),
            ("settle_rwa_receipt", "settle_valid.json", ["issuer", "holder"], "/settle/commit/tx_hash"),
        ],
    },
    {
        "profile": "btc-transaction-commitment-profile-v0",
        "root": "proposals/novaseal/btc-transaction-commitment-profile-v0",
        "signed_type": "NovaBtcTransactionCommitmentSignedIntentV0",
        "live_report": "target/novaseal-btc-transaction-commitment-devnet-stateful-live.json",
        "public_btc_anchor": "/commit_transaction/public_btc_anchor",
        "cases": [
            ("commit_btc_transaction_transition", "commit_transaction_valid.json", ["committer"], "/commit_transaction/commit/tx_hash"),
        ],
    },
    {
        "profile": "btc-utxo-seal-profile-v0",
        "root": "proposals/novaseal/btc-utxo-seal-profile-v0",
        "signed_type": "NovaBtcUtxoSealSignedIntentV0",
        "live_report": "target/novaseal-btc-utxo-seal-devnet-stateful-live.json",
        "public_btc_anchor": "/close_utxo_seal/public_btc_anchor",
        "cases": [
            ("close_btc_utxo_seal", "close_utxo_seal_valid.json", ["owner"], "/close_utxo_seal/commit/tx_hash"),
        ],
    },
    {
        "profile": "dual-seal-profile-v0",
        "root": "proposals/novaseal/dual-seal-profile-v0",
        "signed_type": "NovaDualSealSignedIntentV0",
        "live_report": "target/novaseal-dual-seal-devnet-stateful-live.json",
        "public_btc_anchor": "/finalize_dual_seal/public_btc_anchor",
        "cases": [
            ("finalize_dual_seal", "finalize_dual_seal_valid.json", ["btc_owner", "ckb_authority"], "/finalize_dual_seal/commit/tx_hash"),
        ],
    },
    {
        "profile": "fiber-candidate-profile-v0",
        "root": "proposals/novaseal/fiber-candidate-profile-v0",
        "signed_type": "NovaFiberCandidateSignedIntentV0",
        "live_report": "target/novaseal-fiber-candidate-devnet-stateful-live.json",
        "fiber_report": "target/novaseal-fiber-node-experiments.json",
        "cases": [
            ("settle_fiber_candidate", "settle_fiber_candidate_valid.json", ["operator"], "/settle_fiber_candidate/commit/tx_hash"),
        ],
    },
]


def build_case(profile: dict[str, Any], action: str, fixture_name: str, signers: list[str], tx_pointer: str | None) -> dict[str, Any]:
    profile_root = ROOT / profile["root"]
    fixture_path = profile_root / "fixtures" / fixture_name
    fixture = json.loads(fixture_path.read_text(encoding="utf-8"))
    source_hash = source_tree_hash(profile_root / "src")
    schemas_hash = schema_set_hash(profile_root / "schemas")
    proof_hash = json_file_hash(profile_root / "proofs/invariant_matrix.json")
    live_report_path = ROOT / profile["live_report"] if profile.get("live_report") else None
    live_report = json.loads(live_report_path.read_text(encoding="utf-8")) if live_report_path and live_report_path.is_file() else None
    fiber_report_path = ROOT / profile["fiber_report"] if profile.get("fiber_report") else None
    fiber_report = json.loads(fiber_report_path.read_text(encoding="utf-8")) if fiber_report_path and fiber_report_path.is_file() else None
    live_tx_hash = json_pointer(live_report, tx_pointer) if live_report and tx_pointer else None
    public_btc_anchor = json_pointer(live_report, profile.get("public_btc_anchor")) if live_report and profile.get("public_btc_anchor") else None
    public_btc_required = profile["profile"] in {
        "btc-transaction-commitment-profile-v0",
        "btc-utxo-seal-profile-v0",
        "dual-seal-profile-v0",
    }

    display = {
        "profile": profile["profile"],
        "action": action,
        "fixture": fixture_name,
        "fixture_description": fixture.get("description"),
        "signers": signers,
        "signed_type": profile["signed_type"],
        "source_tree_hash": source_hash,
        "schema_set_hash": schemas_hash,
        "proof_matrix_hash": proof_hash,
        "live_devnet_tx_hash": live_tx_hash,
        "public_btc_anchor": public_btc_anchor,
        "external_boundary": profile.get("external_boundary"),
    }
    witness_shape = {
        "signed_intent": profile["signed_type"],
        "signature_witnesses": [f"{signer}_sig" for signer in signers],
        "fixture_expected": fixture.get("expected"),
        "live_report": profile.get("live_report"),
        "fiber_report": profile.get("fiber_report"),
    }
    intent_body = {
        "schema": "novaseal-profile-operator-intent-v0.1",
        "profile": profile["profile"],
        "action": action,
        "fixture": fixture_name,
        "fixture_hash": json_file_hash(fixture_path),
        "source_tree_hash": source_hash,
        "schema_set_hash": schemas_hash,
        "proof_matrix_hash": proof_hash,
        "signers": signers,
        "witness_shape_hash": report_hash("witness_shape", witness_shape),
        "live_report_hash": report_hash(profile["live_report"], live_report) if live_report is not None else None,
        "fiber_report_hash": report_hash(profile["fiber_report"], fiber_report) if fiber_report is not None else None,
        "live_tx_hash": live_tx_hash,
        "public_btc_anchor": public_btc_anchor,
        "external_boundary": profile.get("external_boundary"),
    }
    packed = canonical_json(intent_body)
    preimage, digest = packed_hash(profile["signed_type"], packed)
    tx_skeleton = {
        "profile": profile["profile"],
        "action": action,
        "fixture": fixture_name,
        "live_tx_hash": live_tx_hash,
        "source_tree_hash": source_hash,
        "witness_shape_hash": intent_body["witness_shape_hash"],
        "public_btc_anchor": public_btc_anchor,
    }
    status_checks = {
        "fixture_expected_accepted": fixture.get("expected") == "accepted",
        "fixture_action_matches": fixture.get("action") == action,
        "live_status_passed_or_external_boundary": bool(live_report and live_report.get("status") == "passed")
        or profile.get("external_boundary") == "package_fixture_only_external_btc_and_ckb_finality_required",
        "fiber_execution_passed_when_required": not fiber_report
        or json_pointer(fiber_report, "/workflow_coverage/all_required_workflows_executed_passed") is True,
        "public_btc_anchor_present_when_required": (not public_btc_required) or bool(public_btc_anchor),
    }
    status = "passed" if all(status_checks.values()) else "failed"
    return {
        "profile": profile["profile"],
        "action": action,
        "fixture": fixture_name,
        "status": status,
        "checks": status_checks,
        "signers": signers,
        "signed_type": profile["signed_type"],
        "signed_intent_hash": digest,
        "signed_intent_hash_preimage_hex": preimage,
        "signed_intent_body_hex": hex0x(packed),
        "bip340_message_hash": digest,
        "witness_shape_hash": intent_body["witness_shape_hash"],
        "tx_skeleton_hash": report_hash("tx_skeleton", tx_skeleton),
        "fixture_hash": intent_body["fixture_hash"],
        "source_tree_hash": source_hash,
        "schema_set_hash": schemas_hash,
        "proof_matrix_hash": proof_hash,
        "live_report_hash": intent_body["live_report_hash"],
        "fiber_report_hash": intent_body["fiber_report_hash"],
        "live_devnet_tx_hash": live_tx_hash,
        "public_btc_anchor": public_btc_anchor,
        "wallet_display": display,
        "operator_witness_shape": witness_shape,
    }


def build_report() -> dict[str, Any]:
    cases = []
    for profile in PROFILE_CASES:
        for action, fixture_name, signers, tx_pointer in profile["cases"]:
            cases.append(build_case(profile, action, fixture_name, signers, tx_pointer))
    profiles = sorted({case["profile"] for case in cases})
    status = "passed" if cases and all(case["status"] == "passed" for case in cases) else "failed"
    return {
        "schema": "novaseal-profile-operator-fixtures-v0.1",
        "status": status,
        "hash_algorithm": "ckb_blake2b_256",
        "signature_scheme": "BIP340 Schnorr over 32-byte signed profile intent hash",
        "fixture_boundary": "wallet/service fixtures bind declared profile actions to source, schema, invariant, witness, and live-report evidence; external BTC/CellDep/TCB attestations remain separate production gates",
        "summary": {
            "total": len(cases),
            "matched": len([case for case in cases if case["status"] == "passed"]),
            "profile_count": len(profiles),
            "profiles": profiles,
        },
        "profiles": profiles,
        "cases": cases,
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
            f"profiles={report['summary']['profile_count']} cases={report['summary']['total']}"
        )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
