#!/usr/bin/env python3
"""Contract for NovaSeal V1 planned-profile live devnet reports.

The certification gate only accepts reports produced from real CKB devnet
transactions with fresh source/artifact provenance. This script records the
exact JSON shape required per planned workflow, but deliberately emits
`status=not_run` until profile-specific live runners are implemented.
"""

from __future__ import annotations

import argparse
import json
import pathlib
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class ReportContract:
    profile: str
    output: str
    source: str
    source_actions: tuple[str, ...]
    lifecycle_action: str | None
    tx_hashes: tuple[tuple[str, str], ...]
    live_checks: tuple[tuple[str, str], ...]
    negative_cases: tuple[tuple[str, str], ...]


REPORT_CONTRACTS = {
    "fungible-xudt": ReportContract(
        profile="fungible-xudt",
        output="target/novaseal-fungible-xudt-devnet-stateful-live.json",
        source="proposals/novaseal/fungible-xudt-profile-v0/src/nova_fungible_xudt_lifecycle_type.cell",
        source_actions=("issue_xudt", "transfer_xudt", "settle_xudt", "nova_fungible_xudt_lifecycle"),
        lifecycle_action="nova_fungible_xudt_lifecycle",
        tx_hashes=(
            ("issue", "/issue/commit/tx_hash"),
            ("transfer", "/transfer/commit/tx_hash"),
            ("settle", "/settle/commit/tx_hash"),
        ),
        live_checks=(
            ("issue_balance_live", "/issue/balance_live"),
            ("issue_receipt_live", "/issue/receipt_live"),
            ("transfer_old_balance_not_live", "/transfer/old_balance_not_live"),
            ("transfer_sender_balance_live", "/transfer/sender_balance_live"),
            ("transfer_receiver_balance_live", "/transfer/receiver_balance_live"),
            ("transfer_receipt_live", "/transfer/receipt_live"),
            ("transfer_amount_conserved", "/transfer/amount_conserved"),
            ("settle_old_balance_not_live", "/settle/old_balance_not_live"),
            ("settlement_receipt_live", "/settle/settlement_receipt_live"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ),
        negative_cases=(
            ("wrong_holder_signature_rejected", "wrong_holder_signature_dry_run"),
            ("transfer_amount_mismatch_rejected", "transfer_amount_mismatch_dry_run"),
            ("settle_wrong_holder_signature_rejected", "settle_wrong_holder_signature_dry_run"),
        ),
    ),
    "rwa-receipt": ReportContract(
        profile="rwa-receipt",
        output="target/novaseal-rwa-receipt-devnet-stateful-live.json",
        source="proposals/novaseal/rwa-receipt-profile-v0/src/nova_rwa_receipt_lifecycle_type.cell",
        source_actions=("materialize_rwa_receipt", "claim_rwa_receipt", "settle_rwa_receipt", "nova_rwa_receipt_lifecycle"),
        lifecycle_action="nova_rwa_receipt_lifecycle",
        tx_hashes=(
            ("materialize", "/materialize/commit/tx_hash"),
            ("claim", "/claim/commit/tx_hash"),
            ("settle", "/settle/commit/tx_hash"),
        ),
        live_checks=(
            ("materialized_receipt_live", "/materialize/receipt_live"),
            ("materialized_audit_event_live", "/materialize/audit_event_live"),
            ("claim_old_receipt_not_live", "/claim/old_receipt_not_live"),
            ("claimed_receipt_live", "/claim/claimed_receipt_live"),
            ("claim_event_live", "/claim/claim_event_live"),
            ("settle_old_claim_not_live", "/settle/old_claim_not_live"),
            ("settlement_receipt_live", "/settle/settlement_receipt_live"),
            ("settlement_event_live", "/settle/settlement_event_live"),
            ("amount_conserved", "/settle/amount_conserved"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ),
        negative_cases=(
            ("wrong_holder_claim_rejected", "wrong_holder_claim_dry_run"),
            ("wrong_issuer_settlement_rejected", "wrong_issuer_settlement_dry_run"),
            ("amount_mutation_rejected", "amount_mutation_dry_run"),
        ),
    ),
    "btc-transaction-commitment": ReportContract(
        profile="btc-transaction-commitment",
        output="target/novaseal-btc-transaction-commitment-devnet-stateful-live.json",
        source="proposals/novaseal/btc-transaction-commitment-profile-v0/src/nova_btc_transaction_commitment_type.cell",
        source_actions=("commit_btc_transaction_transition",),
        lifecycle_action="commit_btc_transaction_transition",
        tx_hashes=(("commit_transaction", "/commit_transaction/commit/tx_hash"),),
        live_checks=(
            ("old_state_not_live", "/commit_transaction/old_state_not_live"),
            ("new_state_live", "/commit_transaction/new_state_live"),
            ("receipt_live", "/commit_transaction/receipt_live"),
            ("btc_tx_tuple_bound", "/commit_transaction/btc_tx_tuple_bound"),
            ("transition_commitment_bound", "/commit_transaction/transition_commitment_bound"),
            ("public_btc_verification_executed", "/commit_transaction/public_btc_verification_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ),
        negative_cases=(
            ("wrong_committer_signature_rejected", "wrong_committer_signature_dry_run"),
            ("zero_btc_txid_rejected", "zero_btc_txid_dry_run"),
            ("transition_hash_mismatch_rejected", "transition_hash_mismatch_dry_run"),
        ),
    ),
    "btc-utxo-seal": ReportContract(
        profile="btc-utxo-seal",
        output="target/novaseal-btc-utxo-seal-devnet-stateful-live.json",
        source="proposals/novaseal/btc-utxo-seal-profile-v0/src/nova_btc_utxo_seal_type.cell",
        source_actions=("close_btc_utxo_seal",),
        lifecycle_action="close_btc_utxo_seal",
        tx_hashes=(("close_utxo_seal", "/close_utxo_seal/commit/tx_hash"),),
        live_checks=(
            ("old_state_not_live", "/close_utxo_seal/old_state_not_live"),
            ("new_state_live", "/close_utxo_seal/new_state_live"),
            ("receipt_live", "/close_utxo_seal/receipt_live"),
            ("sealed_utxo_tuple_bound", "/close_utxo_seal/sealed_utxo_tuple_bound"),
            ("spend_tuple_bound", "/close_utxo_seal/spend_tuple_bound"),
            ("public_btc_spend_verification_executed", "/close_utxo_seal/public_btc_spend_verification_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ),
        negative_cases=(
            ("wrong_owner_signature_rejected", "wrong_owner_signature_dry_run"),
            ("utxo_commitment_mismatch_rejected", "utxo_commitment_mismatch_dry_run"),
            ("zero_spend_txid_rejected", "zero_spend_txid_dry_run"),
        ),
    ),
    "fiber-candidate": ReportContract(
        profile="fiber-candidate",
        output="target/novaseal-fiber-candidate-devnet-stateful-live.json",
        source="proposals/novaseal/fiber-candidate-profile-v0/src/nova_fiber_candidate_type.cell",
        source_actions=("settle_fiber_candidate",),
        lifecycle_action="settle_fiber_candidate",
        tx_hashes=(("settle_fiber_candidate", "/settle_fiber_candidate/commit/tx_hash"),),
        live_checks=(
            ("old_candidate_not_live", "/settle_fiber_candidate/old_candidate_not_live"),
            ("new_candidate_live", "/settle_fiber_candidate/new_candidate_live"),
            ("receipt_live", "/settle_fiber_candidate/receipt_live"),
            ("balance_commitment_progressed", "/settle_fiber_candidate/balance_commitment_progressed"),
            ("fiber_execution_executed", "/settle_fiber_candidate/fiber_execution_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ),
        negative_cases=(
            ("wrong_operator_signature_rejected", "wrong_operator_signature_dry_run"),
            ("balance_commitment_replay_rejected", "balance_commitment_replay_dry_run"),
        ),
    ),
}


def parse_args() -> argparse.Namespace:
    repo_root = pathlib.Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=pathlib.Path, default=repo_root)
    parser.add_argument("--profile", choices=sorted(REPORT_CONTRACTS), required=True)
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--pretty", action="store_true")
    parser.add_argument("--list-contract", action="store_true")
    parser.add_argument("--prepare-artifacts", action="store_true")
    return parser.parse_args()


def named_pointer_rows(rows: tuple[tuple[str, str], ...], pointer_name: str) -> list[dict[str, str]]:
    return [{"name": name, pointer_name: pointer} for name, pointer in rows]


def not_run_report(contract: ReportContract) -> dict[str, Any]:
    return {
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "not_run",
        "live_devnet_rpc_executed": False,
        "stateful_lifecycle_executed": False,
        "artifact_contract": {
            "source": contract.source,
            "source_actions": list(contract.source_actions),
            "lifecycle_action": contract.lifecycle_action,
            "stable_lifecycle_artifact_required": True,
            "dispatcher_required": contract.lifecycle_action is None,
            "dispatcher_gap": (
                "multi-step workflow requires one stable lifecycle/dispatcher action before live CKB state can move across steps"
                if contract.lifecycle_action is None
                else None
            ),
        },
        "expected_tx_hashes": named_pointer_rows(contract.tx_hashes, "pointer"),
        "required_live_checks": named_pointer_rows(contract.live_checks, "pointer"),
        "required_negative_cases": named_pointer_rows(contract.negative_cases, "key"),
        "provenance": {
            "repo_commit": None,
            "source_tree": None,
            "artifacts": None,
        },
        "negative_cases": {
            key: {
                "status": "not_run",
                "matched_expected": False,
            }
            for _, key in contract.negative_cases
        },
        "next_engineering_step": (
            "Replace this contract report with profile-specific live CKB devnet "
            "transaction evidence, including fresh source/artifact provenance."
        ),
    }


def write_json(path: pathlib.Path, value: dict[str, Any], pretty: bool) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2 if pretty else None, sort_keys=True) + "\n", encoding="utf-8")


def prepare_lifecycle_artifact(repo_root: pathlib.Path, contract: ReportContract, pretty: bool) -> dict[str, Any]:
    if contract.lifecycle_action is None:
        return {
            "schema": "novaseal-planned-profile-artifact-prep-v0.1",
            "profile": contract.profile,
            "status": "blocked_missing_dispatcher",
            "source": contract.source,
            "source_actions": list(contract.source_actions),
            "required": "add a profile lifecycle/dispatcher action, then compile that single entry action for live devnet use",
        }

    output = repo_root / "target/novaseal-planned-profile-artifacts" / contract.profile / f"{contract.lifecycle_action}.elf"
    output.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--",
        contract.source,
        "--target-profile",
        "ckb",
        "--target",
        "riscv64-elf",
        "--entry-action",
        contract.lifecycle_action,
        "-o",
        str(output),
    ]
    import subprocess

    completed = subprocess.run(cmd, cwd=repo_root, text=True, capture_output=True)
    report: dict[str, Any] = {
        "schema": "novaseal-planned-profile-artifact-prep-v0.1",
        "profile": contract.profile,
        "source": contract.source,
        "lifecycle_action": contract.lifecycle_action,
        "artifact": output.as_posix(),
        "status": "passed" if completed.returncode == 0 else "failed",
        "command": cmd,
    }
    if completed.returncode != 0:
        report["stderr"] = completed.stderr
        report["stdout"] = completed.stdout
        return report
    report["size_bytes"] = output.stat().st_size
    return report


def main() -> int:
    args = parse_args()
    contract = REPORT_CONTRACTS[args.profile]
    report = not_run_report(contract)
    if args.prepare_artifacts:
        prep = prepare_lifecycle_artifact(args.repo_root, contract, args.pretty)
        print(json.dumps(prep, indent=2 if args.pretty else None, sort_keys=True))
        return 0 if prep["status"] == "passed" else 1
    if args.list_contract:
        print(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True))
        return 1

    output = args.output or args.repo_root / contract.output
    write_json(output, report, args.pretty)
    print(f"wrote {output} status=not_run profile={args.profile}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
