#!/usr/bin/env python3
"""Run or describe NovaSeal V1 planned-profile live devnet reports.

The certification gate only accepts reports produced from real CKB devnet
transactions with fresh source/artifact provenance. Profiles without an
implemented live runner still emit `status=not_run` contract reports.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import time
from dataclasses import dataclass
from typing import Any

from novaseal_devnet_stateful_live import (
    RECEIPT_CAPACITY,
    SHANNONS,
    STATE_CAPACITY,
    TEST_AUX_RAND,
    TEST_SECRET_KEY,
    ZERO_HASH,
    CkbDevnet,
    LiveAcceptanceError,
    always_success_dep,
    always_success_lock,
    cell_data_hash,
    ckb_hash,
    deploy_code_cell,
    hex0x,
    packed_hash,
    resolve_ckb_bin,
    schnorr_sign,
    stateful_provenance,
    transaction,
    u8,
    u16,
    u32,
    u64,
    xonly_pubkey,
)


FUNGIBLE_XUDT_VERSION = 0
OP_ISSUE = 0
OP_TRANSFER = 1
OP_SETTLE = 2
STATUS_ACTIVE = 1
STATUS_SETTLED = 2
RWA_RECEIPT_VERSION = 0
OP_MATERIALIZE = 0
OP_CLAIM = 1
OP_RWA_SETTLE = 2
STATUS_MATERIALIZED = 1
STATUS_CLAIMED = 2
STATUS_RWA_SETTLED = 3
BTC_TX_COMMITMENT_VERSION = 0
OP_BTC_COMMIT_TRANSACTION = 0
OP_BTC_INITIALIZE_ACTIVE_STATE = 255
BTC_STATUS_COMMITTED = 2
BTC_UTXO_SEAL_VERSION = 0
OP_BTC_UTXO_CLOSE = 0
OP_BTC_UTXO_INITIALIZE_ACTIVE_SEAL = 255
BTC_STATUS_CLOSED = 2
DUAL_SEAL_VERSION = 0
OP_DUAL_SEAL_FINALIZE = 0
OP_DUAL_SEAL_INITIALIZE_ACTIVE = 255
DUAL_STATUS_FINALIZED = 2
FIBER_CANDIDATE_VERSION = 0
OP_FIBER_SETTLE = 0
OP_FIBER_INITIALIZE_ACTIVE_CANDIDATE = 255
FIBER_STATUS_SETTLED = 2
HOLDER_SECRET_KEY = bytes.fromhex("22" * 32)
HOLDER_AUX_RAND = bytes([0x42]) * 32
RECEIVER_SECRET_KEY = bytes.fromhex("33" * 32)
RECEIVER_AUX_RAND = bytes([0x66]) * 32


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
        source_actions=("commit_btc_transaction_transition", "nova_btc_transaction_commitment_lifecycle"),
        lifecycle_action="nova_btc_transaction_commitment_lifecycle",
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
        source_actions=("close_btc_utxo_seal", "nova_btc_utxo_seal_lifecycle"),
        lifecycle_action="nova_btc_utxo_seal_lifecycle",
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
    "dual-seal": ReportContract(
        profile="dual-seal",
        output="target/novaseal-dual-seal-devnet-stateful-live.json",
        source="proposals/novaseal/dual-seal-profile-v0/src/nova_dual_seal_type.cell",
        source_actions=("finalize_dual_seal", "nova_dual_seal_lifecycle"),
        lifecycle_action="nova_dual_seal_lifecycle",
        tx_hashes=(("finalize_dual_seal", "/finalize_dual_seal/commit/tx_hash"),),
        live_checks=(
            ("old_state_not_live", "/finalize_dual_seal/old_state_not_live"),
            ("receipt_live", "/finalize_dual_seal/receipt_live"),
            ("btc_closure_bound", "/finalize_dual_seal/btc_closure_bound"),
            ("ckb_maturity_executed", "/finalize_dual_seal/ckb_maturity_executed"),
            ("dual_authority_executed", "/finalize_dual_seal/dual_authority_executed"),
            ("post_negative_state_still_live", "/negative_cases/post_negative_state_still_live"),
        ),
        negative_cases=(
            ("wrong_btc_owner_signature_rejected", "wrong_btc_owner_signature_dry_run"),
            ("wrong_ckb_authority_signature_rejected", "wrong_ckb_authority_signature_dry_run"),
            ("btc_closure_commitment_missing_rejected", "btc_closure_commitment_missing_dry_run"),
        ),
    ),
    "fiber-candidate": ReportContract(
        profile="fiber-candidate",
        output="target/novaseal-fiber-candidate-devnet-stateful-live.json",
        source="proposals/novaseal/fiber-candidate-profile-v0/src/nova_fiber_candidate_type.cell",
        source_actions=("settle_fiber_candidate", "nova_fiber_candidate_lifecycle"),
        lifecycle_action="nova_fiber_candidate_lifecycle",
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
    default_ckb_repo = repo_root.parent / "ckb"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=pathlib.Path, default=repo_root)
    parser.add_argument("--ckb-repo", type=pathlib.Path, default=default_ckb_repo)
    parser.add_argument("--ckb-bin", type=pathlib.Path)
    parser.add_argument("--profile", choices=sorted(REPORT_CONTRACTS), required=True)
    parser.add_argument("--output", type=pathlib.Path)
    parser.add_argument("--run-dir", type=pathlib.Path)
    parser.add_argument("--pretty", action="store_true")
    parser.add_argument("--keep-node", action="store_true")
    parser.add_argument("--list-contract", action="store_true")
    parser.add_argument("--prepare-artifacts", action="store_true")
    parser.add_argument("--live", action="store_true")
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


def signature_payload(secret_key: bytes, message_hash: bytes, aux_rand: bytes) -> bytes:
    pubkey, signature = schnorr_sign(message_hash, secret_key, aux_rand)
    return pubkey + signature


def lifecycle_type(lifecycle_data_hash: str) -> dict[str, str]:
    return {"code_hash": lifecycle_data_hash, "hash_type": "data2", "args": "0x"}


def pack_canonical_envelope(envelope: dict[str, Any]) -> bytes:
    return (
        envelope["profile_id"]
        + envelope["policy_hash"]
        + u8(envelope["action"])
        + u8(envelope["terminal_path"])
        + envelope["subject_id"]
        + envelope["old_state_commitment"]
        + envelope["new_state_commitment"]
        + u64(envelope["old_nonce"])
        + u64(envelope["new_nonce"])
        + u64(envelope["expiry"])
        + envelope["authority_hash"]
        + envelope["profile_body_hash"]
        + envelope["payout_commitment_hash"]
    )


def canonical_envelope_hash(
    *,
    action: int,
    asset_id: bytes,
    xudt_type_hash: bytes,
    old_state_commitment: bytes,
    new_state_commitment: bytes,
    old_nonce: int,
    new_nonce: int,
    expiry: int,
    authority_hash: bytes,
    profile_body_hash: bytes,
    payout_commitment_hash: bytes,
) -> bytes:
    return packed_hash(
        "NovaSealCanonicalEnvelopeV0",
        pack_canonical_envelope(
            {
                "profile_id": asset_id,
                "policy_hash": xudt_type_hash,
                "action": action,
                "terminal_path": action,
                "subject_id": asset_id,
                "old_state_commitment": old_state_commitment,
                "new_state_commitment": new_state_commitment,
                "old_nonce": old_nonce,
                "new_nonce": new_nonce,
                "expiry": expiry,
                "authority_hash": authority_hash,
                "profile_body_hash": profile_body_hash,
                "payout_commitment_hash": payout_commitment_hash,
            }
        ),
    )


def pack_xudt_intent_core(core: dict[str, Any]) -> bytes:
    return (
        u8(core["action"])
        + core["asset_id"]
        + core["xudt_type_hash"]
        + core["issuer_authority_hash"]
        + core["old_holder_authority_hash"]
        + core["new_holder_authority_hash"]
        + u8(core["old_status"])
        + u8(core["new_status"])
        + u64(core["old_amount"])
        + u64(core["transfer_amount"])
        + u64(core["new_amount"])
        + u64(core["old_nonce"])
        + u64(core["new_nonce"])
        + u64(core["expiry"])
        + core["payout_commitment_hash"]
    )


def pack_xudt_signed_intent(core_data: bytes, canonical_hash: bytes, expected_receipt_hash: bytes) -> bytes:
    return core_data + canonical_hash + expected_receipt_hash


def pack_xudt_state_commitment(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["asset_id"]
        + cell["xudt_type_hash"]
        + cell["issuer_authority_hash"]
        + cell["holder_authority_hash"]
        + u64(cell["amount"])
        + u8(cell["status"])
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_xudt_receipt_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        u8(commitment["action"])
        + commitment["asset_id"]
        + commitment["xudt_type_hash"]
        + commitment["old_holder_authority_hash"]
        + commitment["new_holder_authority_hash"]
        + u8(commitment["old_status"])
        + u8(commitment["new_status"])
        + u64(commitment["old_amount"])
        + u64(commitment["transfer_amount"])
        + u64(commitment["new_amount"])
        + u64(commitment["old_nonce"])
        + u64(commitment["new_nonce"])
        + commitment["intent_core_hash"]
        + commitment["payout_commitment_hash"]
    )


def pack_xudt_cell(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["asset_id"]
        + cell["xudt_type_hash"]
        + cell["issuer_authority_hash"]
        + cell["holder_authority_hash"]
        + u64(cell["amount"])
        + u8(cell["status"])
        + cell["latest_receipt_hash"]
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_xudt_receipt(receipt: dict[str, Any]) -> bytes:
    return (
        u8(receipt["action"])
        + receipt["asset_id"]
        + receipt["xudt_type_hash"]
        + receipt["old_holder_authority_hash"]
        + receipt["new_holder_authority_hash"]
        + u8(receipt["old_status"])
        + u8(receipt["new_status"])
        + u64(receipt["old_amount"])
        + u64(receipt["transfer_amount"])
        + u64(receipt["new_amount"])
        + u64(receipt["old_nonce"])
        + u64(receipt["new_nonce"])
        + receipt["intent_core_hash"]
        + receipt["signed_intent_hash"]
        + receipt["payout_commitment_hash"]
        + receipt["latest_receipt_hash"]
        + receipt["signer_authority_hash"]
        + u64(receipt["expiry"])
    )


def zero_xudt_cell() -> dict[str, Any]:
    return {
        "version": 0,
        "asset_id": ZERO_HASH,
        "xudt_type_hash": ZERO_HASH,
        "issuer_authority_hash": ZERO_HASH,
        "holder_authority_hash": ZERO_HASH,
        "amount": 0,
        "status": 0,
        "latest_receipt_hash": ZERO_HASH,
        "nonce": 0,
        "expiry": 0,
    }


def xudt_entry_witness(op: int, old_cell_data: bytes, new_cell_data: bytes, signed_intent: bytes, sig_payload: bytes) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(old_cell_data))
        + old_cell_data
        + u32(len(new_cell_data))
        + new_cell_data
        + u32(len(signed_intent))
        + signed_intent
        + u32(len(sig_payload))
        + sig_payload
    )
    return hex0x(payload)


def xudt_base_state(label: str) -> dict[str, Any]:
    return {
        "asset_id": ckb_hash(f"NovaSeal fungible xUDT asset {label}".encode("ascii")),
        "xudt_type_hash": ckb_hash(f"NovaSeal fungible xUDT type {label}".encode("ascii")),
        "issuer_authority_hash": xonly_pubkey(TEST_SECRET_KEY),
        "holder_authority_hash": xonly_pubkey(HOLDER_SECRET_KEY),
        "amount": 1_000,
        "expiry": (1 << 63) - 1,
    }


def build_xudt_material(
    *,
    op: int,
    base: dict[str, Any],
    old_cell: dict[str, Any] | None,
    new_holder_authority_hash: bytes | None = None,
    mutate_signature: bool = False,
    transfer_amount_override: int | None = None,
) -> dict[str, Any]:
    payout_commitment_hash = ZERO_HASH
    if op == OP_ISSUE:
        old_holder = ZERO_HASH
        new_holder = base["holder_authority_hash"]
        old_status = 0
        new_status = STATUS_ACTIVE
        old_amount = 0
        transfer_amount = base["amount"]
        new_amount = base["amount"]
        old_nonce = 0
        new_nonce = 0
        expiry = base["expiry"]
        authority_hash = base["issuer_authority_hash"]
        signer_secret = TEST_SECRET_KEY
        signer_aux = TEST_AUX_RAND
        old_state_commitment = ZERO_HASH
        new_cell = {
            "version": FUNGIBLE_XUDT_VERSION,
            "asset_id": base["asset_id"],
            "xudt_type_hash": base["xudt_type_hash"],
            "issuer_authority_hash": base["issuer_authority_hash"],
            "holder_authority_hash": new_holder,
            "amount": new_amount,
            "status": STATUS_ACTIVE,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": 0,
            "expiry": expiry,
        }
        new_state_commitment = packed_hash("NovaFungibleXudtStateCommitmentV0", pack_xudt_state_commitment(new_cell))
    else:
        if old_cell is None:
            raise LiveAcceptanceError("xUDT non-issue material requires an old cell")
        new_nonce = old_cell["nonce"] + 1
        expiry = old_cell["expiry"]
        old_state_commitment = packed_hash("NovaFungibleXudtStateCommitmentV0", pack_xudt_state_commitment(old_cell))
        if op == OP_TRANSFER:
            old_holder = old_cell["holder_authority_hash"]
            new_holder = new_holder_authority_hash or xonly_pubkey(RECEIVER_SECRET_KEY)
            old_status = STATUS_ACTIVE
            new_status = STATUS_ACTIVE
            old_amount = old_cell["amount"]
            transfer_amount = transfer_amount_override if transfer_amount_override is not None else old_cell["amount"]
            new_amount = old_cell["amount"]
            old_nonce = old_cell["nonce"]
            authority_hash = old_cell["holder_authority_hash"]
            signer_secret = HOLDER_SECRET_KEY
            signer_aux = HOLDER_AUX_RAND
            new_cell = dict(old_cell)
            new_cell.update(
                {
                    "holder_authority_hash": new_holder,
                    "latest_receipt_hash": ZERO_HASH,
                    "nonce": new_nonce,
                }
            )
            new_state_commitment = packed_hash("NovaFungibleXudtStateCommitmentV0", pack_xudt_state_commitment(new_cell))
        elif op == OP_SETTLE:
            old_holder = old_cell["holder_authority_hash"]
            new_holder = old_cell["holder_authority_hash"]
            old_status = STATUS_ACTIVE
            new_status = STATUS_SETTLED
            old_amount = old_cell["amount"]
            transfer_amount = old_cell["amount"]
            new_amount = 0
            old_nonce = old_cell["nonce"]
            authority_hash = old_cell["holder_authority_hash"]
            signer_secret = RECEIVER_SECRET_KEY
            signer_aux = RECEIVER_AUX_RAND
            new_cell = zero_xudt_cell()
            new_state_commitment = ZERO_HASH
        else:
            raise LiveAcceptanceError(f"unknown xUDT op {op}")

    core = {
        "action": op,
        "asset_id": base["asset_id"],
        "xudt_type_hash": base["xudt_type_hash"],
        "issuer_authority_hash": base["issuer_authority_hash"],
        "old_holder_authority_hash": old_holder,
        "new_holder_authority_hash": new_holder,
        "old_status": old_status,
        "new_status": new_status,
        "old_amount": old_amount,
        "transfer_amount": transfer_amount,
        "new_amount": new_amount,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "expiry": expiry,
        "payout_commitment_hash": payout_commitment_hash,
    }
    core_data = pack_xudt_intent_core(core)
    intent_core_hash = packed_hash("NovaFungibleXudtIntentCoreV0", core_data)
    receipt_commitment = {
        "action": op,
        "asset_id": base["asset_id"],
        "xudt_type_hash": base["xudt_type_hash"],
        "old_holder_authority_hash": old_holder,
        "new_holder_authority_hash": new_holder,
        "old_status": old_status,
        "new_status": new_status,
        "old_amount": old_amount,
        "transfer_amount": transfer_amount,
        "new_amount": new_amount,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "intent_core_hash": intent_core_hash,
        "payout_commitment_hash": payout_commitment_hash,
    }
    materialized_receipt_hash = packed_hash(
        "NovaFungibleXudtReceiptCommitmentV0",
        pack_xudt_receipt_commitment(receipt_commitment),
    )
    canonical_hash = canonical_envelope_hash(
        action=op,
        asset_id=base["asset_id"],
        xudt_type_hash=base["xudt_type_hash"],
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
        authority_hash=authority_hash,
        profile_body_hash=intent_core_hash,
        payout_commitment_hash=payout_commitment_hash,
    )
    signed_intent = pack_xudt_signed_intent(core_data, canonical_hash, materialized_receipt_hash)
    signed_intent_hash = packed_hash("NovaFungibleXudtSignedIntentV0", signed_intent)
    sig_payload = bytearray(signature_payload(signer_secret, signed_intent_hash, signer_aux))
    if mutate_signature:
        sig_payload[-1] ^= 1
    receipt = {
        "action": op,
        "asset_id": base["asset_id"],
        "xudt_type_hash": base["xudt_type_hash"],
        "old_holder_authority_hash": old_holder,
        "new_holder_authority_hash": new_holder,
        "old_status": old_status,
        "new_status": new_status,
        "old_amount": old_amount,
        "transfer_amount": transfer_amount,
        "new_amount": new_amount,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "intent_core_hash": intent_core_hash,
        "signed_intent_hash": signed_intent_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "signer_authority_hash": authority_hash,
        "expiry": expiry,
    }
    material_new_cell = dict(new_cell)
    if op in (OP_ISSUE, OP_TRANSFER):
        material_new_cell["latest_receipt_hash"] = materialized_receipt_hash
    new_cell_data = pack_xudt_cell(material_new_cell)
    return {
        "old_cell": old_cell or zero_xudt_cell(),
        "old_cell_data": pack_xudt_cell(old_cell or zero_xudt_cell()),
        "new_cell": material_new_cell,
        "new_cell_data": new_cell_data,
        "receipt_data": pack_xudt_receipt(receipt),
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "signature_payload": bytes(sig_payload),
        "receipt_commitment": receipt_commitment,
    }


def build_xudt_issue_tx(
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - STATE_CAPACITY - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("xUDT issue funding capacity is too small")
    witness = xudt_entry_witness(
        OP_ISSUE,
        material["old_cell_data"],
        material["new_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        funding,
        [
            {"capacity": hex(STATE_CAPACITY), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"][1:]],
        [header_hash],
    )


def build_xudt_transfer_tx(
    *,
    old_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("xUDT transfer funding capacity is too small")
    witness = xudt_entry_witness(
        OP_TRANSFER,
        material["old_cell_data"],
        material["new_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        [old_ref] + funding["cells"],
        [
            {"capacity": hex(old_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def build_xudt_settle_tx(
    *,
    old_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = old_ref["capacity"] + funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("xUDT settle funding capacity is too small")
    witness = xudt_entry_witness(
        OP_SETTLE,
        material["old_cell_data"],
        material["new_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        [old_ref] + funding["cells"],
        [
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def pack_rwa_intent_core(core: dict[str, Any]) -> bytes:
    return (
        u8(core["action"])
        + core["receipt_id"]
        + core["registry_hash"]
        + core["asset_commitment_hash"]
        + core["document_hash"]
        + core["issuer_authority_hash"]
        + core["holder_authority_hash"]
        + u8(core["old_status"])
        + u8(core["new_status"])
        + u64(core["old_amount"])
        + u64(core["settlement_amount"])
        + u64(core["old_nonce"])
        + u64(core["new_nonce"])
        + u64(core["expiry"])
        + core["payout_commitment_hash"]
    )


def pack_rwa_signed_intent(
    core_data: bytes,
    canonical_hash: bytes,
    expected_receipt_hash: bytes,
    expected_cell_data_hash: bytes,
    expected_event_data_hash: bytes,
) -> bytes:
    return core_data + canonical_hash + expected_receipt_hash + expected_cell_data_hash + expected_event_data_hash


def pack_rwa_state_commitment(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["receipt_id"]
        + cell["registry_hash"]
        + cell["asset_commitment_hash"]
        + cell["document_hash"]
        + cell["issuer_authority_hash"]
        + cell["holder_authority_hash"]
        + u64(cell["amount"])
        + u8(cell["status"])
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_rwa_event_commitment(event: dict[str, Any]) -> bytes:
    return (
        u8(event["action"])
        + event["receipt_id"]
        + event["registry_hash"]
        + event["asset_commitment_hash"]
        + event["document_hash"]
        + event["issuer_authority_hash"]
        + event["holder_authority_hash"]
        + u8(event["old_status"])
        + u8(event["new_status"])
        + u64(event["old_amount"])
        + u64(event["settlement_amount"])
        + u64(event["old_nonce"])
        + u64(event["new_nonce"])
        + event["intent_core_hash"]
        + event["payout_commitment_hash"]
    )


def pack_rwa_cell(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["receipt_id"]
        + cell["registry_hash"]
        + cell["asset_commitment_hash"]
        + cell["document_hash"]
        + cell["issuer_authority_hash"]
        + cell["holder_authority_hash"]
        + u64(cell["amount"])
        + u8(cell["status"])
        + cell["latest_receipt_hash"]
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_rwa_event(event: dict[str, Any]) -> bytes:
    return (
        u8(event["action"])
        + event["receipt_id"]
        + event["registry_hash"]
        + event["asset_commitment_hash"]
        + event["document_hash"]
        + event["issuer_authority_hash"]
        + event["holder_authority_hash"]
        + u8(event["old_status"])
        + u8(event["new_status"])
        + u64(event["old_amount"])
        + u64(event["settlement_amount"])
        + u64(event["old_nonce"])
        + u64(event["new_nonce"])
        + event["intent_core_hash"]
        + event["payout_commitment_hash"]
        + event["latest_receipt_hash"]
        + event["signer_authority_hash"]
        + u64(event["expiry"])
    )


def zero_rwa_cell() -> dict[str, Any]:
    return {
        "version": 0,
        "receipt_id": ZERO_HASH,
        "registry_hash": ZERO_HASH,
        "asset_commitment_hash": ZERO_HASH,
        "document_hash": ZERO_HASH,
        "issuer_authority_hash": ZERO_HASH,
        "holder_authority_hash": ZERO_HASH,
        "amount": 0,
        "status": 0,
        "latest_receipt_hash": ZERO_HASH,
        "nonce": 0,
        "expiry": 0,
    }


def rwa_entry_witness(
    op: int,
    old_cell_data: bytes,
    signed_intent: bytes,
    signer_sig: bytes,
    cosigner_sig: bytes,
) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(old_cell_data))
        + old_cell_data
        + u32(len(signed_intent))
        + signed_intent
        + u32(len(signer_sig))
        + signer_sig
        + u32(len(cosigner_sig))
        + cosigner_sig
    )
    return hex0x(payload)


def rwa_base_state(label: str) -> dict[str, Any]:
    return {
        "receipt_id": ckb_hash(f"NovaSeal RWA receipt {label}".encode("ascii")),
        "registry_hash": ckb_hash(f"NovaSeal RWA registry {label}".encode("ascii")),
        "asset_commitment_hash": ckb_hash(f"NovaSeal RWA asset {label}".encode("ascii")),
        "document_hash": ckb_hash(f"NovaSeal RWA document {label}".encode("ascii")),
        "issuer_authority_hash": xonly_pubkey(TEST_SECRET_KEY),
        "holder_authority_hash": xonly_pubkey(HOLDER_SECRET_KEY),
        "amount": 10_000,
        "expiry": (1 << 63) - 1,
    }


def rwa_canonical_hash(
    *,
    op: int,
    base: dict[str, Any],
    old_state_commitment: bytes,
    new_state_commitment: bytes,
    old_nonce: int,
    new_nonce: int,
    expiry: int,
    authority_hash: bytes,
    profile_body_hash: bytes,
    payout_commitment_hash: bytes,
) -> bytes:
    return canonical_envelope_hash(
        action=op,
        asset_id=base["receipt_id"],
        xudt_type_hash=base["registry_hash"],
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
        authority_hash=authority_hash,
        profile_body_hash=profile_body_hash,
        payout_commitment_hash=payout_commitment_hash,
    )


def build_rwa_material(
    *,
    op: int,
    base: dict[str, Any],
    old_cell: dict[str, Any] | None,
    mutate_issuer_signature: bool = False,
    mutate_holder_signature: bool = False,
    settlement_amount_override: int | None = None,
) -> dict[str, Any]:
    payout_commitment_hash = ZERO_HASH
    if op == OP_MATERIALIZE:
        old_status = 0
        new_status = STATUS_MATERIALIZED
        old_amount = 0
        settlement_amount = base["amount"]
        old_nonce = 0
        new_nonce = 0
        expiry = base["expiry"]
        authority_hash = base["issuer_authority_hash"]
        signer_authority_hash = base["issuer_authority_hash"]
        old_state_commitment = ZERO_HASH
        new_cell = {
            "version": RWA_RECEIPT_VERSION,
            "receipt_id": base["receipt_id"],
            "registry_hash": base["registry_hash"],
            "asset_commitment_hash": base["asset_commitment_hash"],
            "document_hash": base["document_hash"],
            "issuer_authority_hash": base["issuer_authority_hash"],
            "holder_authority_hash": base["holder_authority_hash"],
            "amount": base["amount"],
            "status": STATUS_MATERIALIZED,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": 0,
            "expiry": expiry,
        }
        new_state_commitment = packed_hash("NovaRwaReceiptStateCommitmentV0", pack_rwa_state_commitment(new_cell))
    else:
        if old_cell is None:
            raise LiveAcceptanceError("RWA non-materialize material requires an old cell")
        old_state_commitment = packed_hash("NovaRwaReceiptStateCommitmentV0", pack_rwa_state_commitment(old_cell))
        old_nonce = old_cell["nonce"]
        new_nonce = old_nonce + 1
        expiry = old_cell["expiry"]
        old_amount = old_cell["amount"]
        settlement_amount = settlement_amount_override if settlement_amount_override is not None else old_cell["amount"]
        if op == OP_CLAIM:
            old_status = STATUS_MATERIALIZED
            new_status = STATUS_CLAIMED
            authority_hash = old_cell["holder_authority_hash"]
            signer_authority_hash = old_cell["holder_authority_hash"]
            new_cell = dict(old_cell)
            new_cell.update({"status": STATUS_CLAIMED, "latest_receipt_hash": ZERO_HASH, "nonce": new_nonce})
            new_state_commitment = packed_hash("NovaRwaReceiptStateCommitmentV0", pack_rwa_state_commitment(new_cell))
        elif op == OP_RWA_SETTLE:
            old_status = STATUS_CLAIMED
            new_status = STATUS_RWA_SETTLED
            authority_hash = old_cell["issuer_authority_hash"]
            signer_authority_hash = old_cell["issuer_authority_hash"]
            new_cell = zero_rwa_cell()
            new_state_commitment = ZERO_HASH
        else:
            raise LiveAcceptanceError(f"unknown RWA op {op}")

    core = {
        "action": op,
        "receipt_id": base["receipt_id"],
        "registry_hash": base["registry_hash"],
        "asset_commitment_hash": base["asset_commitment_hash"],
        "document_hash": base["document_hash"],
        "issuer_authority_hash": base["issuer_authority_hash"],
        "holder_authority_hash": base["holder_authority_hash"],
        "old_status": old_status,
        "new_status": new_status,
        "old_amount": old_amount,
        "settlement_amount": settlement_amount,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "expiry": expiry,
        "payout_commitment_hash": payout_commitment_hash,
    }
    core_data = pack_rwa_intent_core(core)
    intent_core_hash = packed_hash("NovaRwaReceiptIntentCoreV0", core_data)
    event_commitment = {
        "action": op,
        "receipt_id": base["receipt_id"],
        "registry_hash": base["registry_hash"],
        "asset_commitment_hash": base["asset_commitment_hash"],
        "document_hash": base["document_hash"],
        "issuer_authority_hash": base["issuer_authority_hash"],
        "holder_authority_hash": base["holder_authority_hash"],
        "old_status": old_status,
        "new_status": new_status,
        "old_amount": old_amount,
        "settlement_amount": settlement_amount,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "intent_core_hash": intent_core_hash,
        "payout_commitment_hash": payout_commitment_hash,
    }
    materialized_receipt_hash = packed_hash("NovaRwaReceiptEventCommitmentV0", pack_rwa_event_commitment(event_commitment))
    canonical_hash = rwa_canonical_hash(
        op=op,
        base=base,
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
        authority_hash=authority_hash,
        profile_body_hash=intent_core_hash,
        payout_commitment_hash=payout_commitment_hash,
    )
    material_new_cell = dict(new_cell)
    if op in (OP_MATERIALIZE, OP_CLAIM):
        material_new_cell["latest_receipt_hash"] = materialized_receipt_hash
    new_cell_data = pack_rwa_cell(material_new_cell)
    expected_cell_data_hash = cell_data_hash(new_cell_data) if op in (OP_MATERIALIZE, OP_CLAIM) else ZERO_HASH
    event = dict(event_commitment)
    event.update(
        {
            "latest_receipt_hash": materialized_receipt_hash,
            "signer_authority_hash": signer_authority_hash,
            "expiry": expiry,
        }
    )
    event_data = pack_rwa_event(event)
    expected_event_data_hash = cell_data_hash(event_data)
    signed_intent = pack_rwa_signed_intent(
        core_data,
        canonical_hash,
        materialized_receipt_hash,
        expected_cell_data_hash,
        expected_event_data_hash,
    )
    signed_intent_hash = packed_hash("NovaRwaReceiptSignedIntentV0", signed_intent)
    issuer_sig = bytearray(signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND))
    holder_sig = bytearray(signature_payload(HOLDER_SECRET_KEY, signed_intent_hash, HOLDER_AUX_RAND))
    if mutate_issuer_signature:
        issuer_sig[-1] ^= 1
    if mutate_holder_signature:
        holder_sig[-1] ^= 1
    signer_sig = bytes(holder_sig) if op == OP_CLAIM else bytes(issuer_sig)
    cosigner_sig = bytes(holder_sig) if op == OP_RWA_SETTLE else bytes(issuer_sig)
    return {
        "old_cell": old_cell or zero_rwa_cell(),
        "old_cell_data": pack_rwa_cell(old_cell or zero_rwa_cell()),
        "new_cell": material_new_cell,
        "new_cell_data": new_cell_data,
        "event_data": event_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "issuer_sig": bytes(issuer_sig),
        "holder_sig": bytes(holder_sig),
        "signer_sig": signer_sig,
        "cosigner_sig": cosigner_sig,
    }


def build_rwa_state_event_tx(
    *,
    op: int,
    old_ref: dict[str, Any] | None,
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    if op == OP_MATERIALIZE:
        change_capacity = funding["total_capacity"] - STATE_CAPACITY - RECEIPT_CAPACITY
        inputs = funding
        witnesses = [
            rwa_entry_witness(
                op,
                material["old_cell_data"],
                material["signed_intent"],
                material["signer_sig"],
                material["cosigner_sig"],
            )
        ] + ["0x" for _ in funding["cells"][1:]]
    elif old_ref is not None:
        change_capacity = funding["total_capacity"] - RECEIPT_CAPACITY
        inputs = [old_ref] + funding["cells"]
        witnesses = [
            rwa_entry_witness(
                op,
                material["old_cell_data"],
                material["signed_intent"],
                material["signer_sig"],
                material["cosigner_sig"],
            )
        ] + ["0x" for _ in funding["cells"]]
    else:
        raise LiveAcceptanceError("RWA state/event tx requires an old ref")
    if change_capacity <= 0:
        raise LiveAcceptanceError("RWA state/event funding capacity is too small")
    return transaction(
        inputs,
        [
            {"capacity": hex(STATE_CAPACITY if old_ref is None else old_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), hex0x(material["event_data"]), "0x"],
        cell_deps,
        witnesses,
        [header_hash],
    )


def build_rwa_settle_tx(
    *,
    old_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = old_ref["capacity"] + funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("RWA settle funding capacity is too small")
    witness = rwa_entry_witness(
        OP_RWA_SETTLE,
        material["old_cell_data"],
        material["signed_intent"],
        material["signer_sig"],
        material["cosigner_sig"],
    )
    return transaction(
        [old_ref] + funding["cells"],
        [
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["event_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def pack_btc_tx_public_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        commitment["btc_txid"]
        + commitment["btc_wtxid"]
        + u32(commitment["btc_output_index"])
        + u64(commitment["btc_amount_sats"])
        + commitment["transition_commitment_hash"]
    )


def pack_btc_tx_intent_core(core: dict[str, Any]) -> bytes:
    return (
        u8(core["action"])
        + core["seal_id"]
        + core["policy_hash"]
        + core["committer_authority_hash"]
        + core["btc_txid"]
        + core["btc_wtxid"]
        + u32(core["btc_output_index"])
        + u64(core["btc_amount_sats"])
        + core["old_state_hash"]
        + core["new_state_hash"]
        + core["transition_commitment_hash"]
        + u8(core["old_status"])
        + u8(core["new_status"])
        + u64(core["old_nonce"])
        + u64(core["new_nonce"])
        + u64(core["expiry"])
        + core["payout_commitment_hash"]
    )


def pack_btc_tx_signed_intent(core_data: bytes, canonical_hash: bytes, expected_receipt_hash: bytes) -> bytes:
    return core_data + canonical_hash + expected_receipt_hash


def pack_btc_tx_state_commitment(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["seal_id"]
        + cell["policy_hash"]
        + cell["committer_authority_hash"]
        + cell["btc_tx_commitment_hash"]
        + cell["state_hash"]
        + u8(cell["status"])
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_btc_tx_receipt_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        u8(commitment["action"])
        + commitment["seal_id"]
        + commitment["policy_hash"]
        + commitment["committer_authority_hash"]
        + commitment["btc_tx_commitment_hash"]
        + commitment["old_state_hash"]
        + commitment["new_state_hash"]
        + u8(commitment["old_status"])
        + u8(commitment["new_status"])
        + u64(commitment["old_nonce"])
        + u64(commitment["new_nonce"])
        + commitment["intent_core_hash"]
        + commitment["payout_commitment_hash"]
    )


def pack_btc_tx_cell(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["seal_id"]
        + cell["policy_hash"]
        + cell["committer_authority_hash"]
        + cell["btc_tx_commitment_hash"]
        + cell["state_hash"]
        + u8(cell["status"])
        + cell["latest_receipt_hash"]
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_btc_tx_receipt(receipt: dict[str, Any]) -> bytes:
    return (
        u8(receipt["action"])
        + receipt["seal_id"]
        + receipt["policy_hash"]
        + receipt["committer_authority_hash"]
        + receipt["btc_tx_commitment_hash"]
        + receipt["old_state_hash"]
        + receipt["new_state_hash"]
        + u8(receipt["old_status"])
        + u8(receipt["new_status"])
        + u64(receipt["old_nonce"])
        + u64(receipt["new_nonce"])
        + receipt["intent_core_hash"]
        + receipt["signed_intent_hash"]
        + receipt["payout_commitment_hash"]
        + receipt["latest_receipt_hash"]
        + receipt["signer_authority_hash"]
        + u64(receipt["expiry"])
    )


def zero_btc_tx_cell() -> dict[str, Any]:
    return {
        "version": 0,
        "seal_id": ZERO_HASH,
        "policy_hash": ZERO_HASH,
        "committer_authority_hash": ZERO_HASH,
        "btc_tx_commitment_hash": ZERO_HASH,
        "state_hash": ZERO_HASH,
        "status": 0,
        "latest_receipt_hash": ZERO_HASH,
        "nonce": 0,
        "expiry": 0,
    }


def btc_tx_entry_witness(op: int, old_cell_data: bytes, signed_intent: bytes, sig_payload: bytes) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(old_cell_data))
        + old_cell_data
        + u32(len(signed_intent))
        + signed_intent
        + u32(len(sig_payload))
        + sig_payload
    )
    return hex0x(payload)


def btc_tx_base_state(label: str) -> dict[str, Any]:
    return {
        "seal_id": ckb_hash(f"NovaSeal BTC transaction seal {label}".encode("ascii")),
        "policy_hash": ckb_hash(f"NovaSeal BTC transaction policy {label}".encode("ascii")),
        "committer_authority_hash": xonly_pubkey(TEST_SECRET_KEY),
        "initial_state_hash": ckb_hash(f"NovaSeal BTC transaction active state {label}".encode("ascii")),
        "committed_state_hash": ckb_hash(f"NovaSeal BTC transaction committed state {label}".encode("ascii")),
        "btc_txid": ckb_hash(f"NovaSeal BTC txid {label}".encode("ascii")),
        "btc_wtxid": ckb_hash(f"NovaSeal BTC wtxid {label}".encode("ascii")),
        "btc_output_index": 2,
        "btc_amount_sats": 125_000,
        "expiry": (1 << 63) - 1,
    }


def btc_tx_canonical_hash(
    *,
    op: int,
    base: dict[str, Any],
    old_state_commitment: bytes,
    new_state_commitment: bytes,
    old_nonce: int,
    new_nonce: int,
    expiry: int,
    authority_hash: bytes,
    profile_body_hash: bytes,
    payout_commitment_hash: bytes,
) -> bytes:
    return canonical_envelope_hash(
        action=op,
        asset_id=base["seal_id"],
        xudt_type_hash=base["policy_hash"],
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
        authority_hash=authority_hash,
        profile_body_hash=profile_body_hash,
        payout_commitment_hash=payout_commitment_hash,
    )


def build_btc_tx_material(
    *,
    op: int,
    base: dict[str, Any],
    old_cell: dict[str, Any] | None,
    mutate_signature: bool = False,
    zero_btc_txid: bool = False,
    transition_hash_mismatch: bool = False,
) -> dict[str, Any]:
    payout_commitment_hash = ZERO_HASH
    if op == OP_BTC_INITIALIZE_ACTIVE_STATE:
        old_status = 0
        new_status = STATUS_ACTIVE
        old_nonce = 0
        new_nonce = 0
        old_state_hash = ZERO_HASH
        new_state_hash = base["initial_state_hash"]
        btc_txid = ZERO_HASH
        btc_wtxid = ZERO_HASH
        btc_output_index = 0
        btc_amount_sats = 0
        transition_commitment_hash = ZERO_HASH
        btc_tx_commitment_hash = ZERO_HASH
        old_state_commitment = ZERO_HASH
        expected_receipt_hash = ZERO_HASH
        new_cell = {
            "version": BTC_TX_COMMITMENT_VERSION,
            "seal_id": base["seal_id"],
            "policy_hash": base["policy_hash"],
            "committer_authority_hash": base["committer_authority_hash"],
            "btc_tx_commitment_hash": ZERO_HASH,
            "state_hash": new_state_hash,
            "status": STATUS_ACTIVE,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": 0,
            "expiry": base["expiry"],
        }
        new_state_commitment = packed_hash("NovaBtcTransactionCommitmentStateV0", pack_btc_tx_state_commitment(new_cell))
        receipt_data = b""
    elif op == OP_BTC_COMMIT_TRANSACTION:
        if old_cell is None:
            raise LiveAcceptanceError("BTC transaction commit material requires an old cell")
        old_status = STATUS_ACTIVE
        new_status = BTC_STATUS_COMMITTED
        old_nonce = old_cell["nonce"]
        new_nonce = old_nonce + 1
        old_state_hash = old_cell["state_hash"]
        new_state_hash = base["committed_state_hash"]
        btc_txid = ZERO_HASH if zero_btc_txid else base["btc_txid"]
        btc_wtxid = base["btc_wtxid"]
        btc_output_index = base["btc_output_index"]
        btc_amount_sats = base["btc_amount_sats"]
        transition_commitment_hash = (
            ckb_hash(b"NovaSeal BTC transaction mismatched transition") if transition_hash_mismatch else ckb_hash(new_state_hash)
        )
        btc_tx_commitment_hash = packed_hash(
            "BtcTransactionPublicCommitmentV0",
            pack_btc_tx_public_commitment(
                {
                    "btc_txid": btc_txid,
                    "btc_wtxid": btc_wtxid,
                    "btc_output_index": btc_output_index,
                    "btc_amount_sats": btc_amount_sats,
                    "transition_commitment_hash": transition_commitment_hash,
                }
            ),
        )
        old_state_commitment = packed_hash("NovaBtcTransactionCommitmentStateV0", pack_btc_tx_state_commitment(old_cell))
        new_cell = {
            "version": BTC_TX_COMMITMENT_VERSION,
            "seal_id": old_cell["seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "committer_authority_hash": old_cell["committer_authority_hash"],
            "btc_tx_commitment_hash": btc_tx_commitment_hash,
            "state_hash": new_state_hash,
            "status": BTC_STATUS_COMMITTED,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": new_nonce,
            "expiry": old_cell["expiry"],
        }
        new_state_commitment = packed_hash("NovaBtcTransactionCommitmentStateV0", pack_btc_tx_state_commitment(new_cell))
        receipt_commitment = {
            "action": OP_BTC_COMMIT_TRANSACTION,
            "seal_id": old_cell["seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "committer_authority_hash": old_cell["committer_authority_hash"],
            "btc_tx_commitment_hash": btc_tx_commitment_hash,
            "old_state_hash": old_cell["state_hash"],
            "new_state_hash": new_state_hash,
            "old_status": STATUS_ACTIVE,
            "new_status": BTC_STATUS_COMMITTED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": ZERO_HASH,
            "payout_commitment_hash": payout_commitment_hash,
        }
        # Filled after the intent core hash is known.
        expected_receipt_hash = ZERO_HASH
        receipt_data = b""
    else:
        raise LiveAcceptanceError(f"unknown BTC transaction op {op}")

    core = {
        "action": op,
        "seal_id": base["seal_id"],
        "policy_hash": base["policy_hash"],
        "committer_authority_hash": base["committer_authority_hash"],
        "btc_txid": btc_txid,
        "btc_wtxid": btc_wtxid,
        "btc_output_index": btc_output_index,
        "btc_amount_sats": btc_amount_sats,
        "old_state_hash": old_state_hash,
        "new_state_hash": new_state_hash,
        "transition_commitment_hash": transition_commitment_hash,
        "old_status": old_status,
        "new_status": new_status,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "expiry": base["expiry"],
        "payout_commitment_hash": payout_commitment_hash,
    }
    core_data = pack_btc_tx_intent_core(core)
    intent_core_hash = packed_hash("NovaBtcTransactionCommitmentIntentCoreV0", core_data)
    if op == OP_BTC_COMMIT_TRANSACTION:
        receipt_commitment["intent_core_hash"] = intent_core_hash
        expected_receipt_hash = packed_hash(
            "NovaBtcTransactionCommitmentReceiptCommitmentV0",
            pack_btc_tx_receipt_commitment(receipt_commitment),
        )
        new_cell["latest_receipt_hash"] = expected_receipt_hash
    canonical_hash = btc_tx_canonical_hash(
        op=op,
        base=base,
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=base["expiry"],
        authority_hash=base["committer_authority_hash"],
        profile_body_hash=intent_core_hash,
        payout_commitment_hash=payout_commitment_hash,
    )
    signed_intent = pack_btc_tx_signed_intent(core_data, canonical_hash, expected_receipt_hash)
    signed_intent_hash = packed_hash("NovaBtcTransactionCommitmentSignedIntentV0", signed_intent)
    sig_payload = bytearray(signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND))
    if mutate_signature:
        sig_payload[-1] ^= 1
    new_cell_data = pack_btc_tx_cell(new_cell)
    receipt = None
    if op == OP_BTC_COMMIT_TRANSACTION:
        receipt = {
            "action": OP_BTC_COMMIT_TRANSACTION,
            "seal_id": old_cell["seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "committer_authority_hash": old_cell["committer_authority_hash"],
            "btc_tx_commitment_hash": btc_tx_commitment_hash,
            "old_state_hash": old_cell["state_hash"],
            "new_state_hash": new_state_hash,
            "old_status": STATUS_ACTIVE,
            "new_status": BTC_STATUS_COMMITTED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": intent_core_hash,
            "signed_intent_hash": signed_intent_hash,
            "payout_commitment_hash": payout_commitment_hash,
            "latest_receipt_hash": expected_receipt_hash,
            "signer_authority_hash": old_cell["committer_authority_hash"],
            "expiry": old_cell["expiry"],
        }
        receipt_data = pack_btc_tx_receipt(receipt)
    return {
        "old_cell": old_cell or zero_btc_tx_cell(),
        "old_cell_data": pack_btc_tx_cell(old_cell or zero_btc_tx_cell()),
        "new_cell": new_cell,
        "new_cell_data": new_cell_data,
        "receipt": receipt,
        "receipt_data": receipt_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "signature_payload": bytes(sig_payload),
        "btc_tx_commitment_hash": btc_tx_commitment_hash,
        "transition_commitment_hash": transition_commitment_hash,
        "latest_receipt_hash": expected_receipt_hash,
    }


def build_btc_tx_initialize_tx(
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - STATE_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("BTC transaction initialize funding capacity is too small")
    witness = btc_tx_entry_witness(
        OP_BTC_INITIALIZE_ACTIVE_STATE,
        material["old_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        funding,
        [
            {"capacity": hex(STATE_CAPACITY), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"][1:]],
        [header_hash],
    )


def build_btc_tx_commit_tx(
    *,
    old_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("BTC transaction commit funding capacity is too small")
    witness = btc_tx_entry_witness(
        OP_BTC_COMMIT_TRANSACTION,
        material["old_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        [old_ref] + funding["cells"],
        [
            {"capacity": hex(old_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def pack_btc_utxo_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        commitment["btc_txid"]
        + u32(commitment["btc_vout_index"])
        + u64(commitment["btc_amount_sats"])
        + commitment["script_pubkey_hash"]
    )


def pack_btc_utxo_closure_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        commitment["sealed_utxo_commitment_hash"]
        + commitment["spend_txid"]
        + commitment["spend_wtxid"]
        + u32(commitment["spend_input_index"])
        + commitment["transition_commitment_hash"]
        + commitment["payout_commitment_hash"]
    )


def pack_btc_utxo_intent_core(core: dict[str, Any]) -> bytes:
    return (
        u8(core["action"])
        + core["seal_id"]
        + core["policy_hash"]
        + core["owner_authority_hash"]
        + core["btc_txid"]
        + u32(core["btc_vout_index"])
        + u64(core["btc_amount_sats"])
        + core["script_pubkey_hash"]
        + core["spend_txid"]
        + core["spend_wtxid"]
        + u32(core["spend_input_index"])
        + core["old_state_hash"]
        + core["new_state_hash"]
        + core["transition_commitment_hash"]
        + u8(core["old_status"])
        + u8(core["new_status"])
        + u64(core["old_nonce"])
        + u64(core["new_nonce"])
        + u64(core["expiry"])
        + core["payout_commitment_hash"]
    )


def pack_btc_utxo_signed_intent(core_data: bytes, canonical_hash: bytes, expected_receipt_hash: bytes) -> bytes:
    return core_data + canonical_hash + expected_receipt_hash


def pack_btc_utxo_signing_digest(intent_core_hash: bytes, canonical_hash: bytes, expected_receipt_hash: bytes) -> bytes:
    return intent_core_hash + canonical_hash + expected_receipt_hash


def pack_btc_utxo_state_commitment(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["seal_id"]
        + cell["policy_hash"]
        + cell["owner_authority_hash"]
        + cell["sealed_utxo_commitment_hash"]
        + cell["state_hash"]
        + u8(cell["status"])
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_btc_utxo_receipt_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        u8(commitment["action"])
        + commitment["seal_id"]
        + commitment["policy_hash"]
        + commitment["owner_authority_hash"]
        + commitment["sealed_utxo_commitment_hash"]
        + commitment["closure_commitment_hash"]
        + commitment["old_state_hash"]
        + commitment["new_state_hash"]
        + u8(commitment["old_status"])
        + u8(commitment["new_status"])
        + u64(commitment["old_nonce"])
        + u64(commitment["new_nonce"])
        + commitment["intent_core_hash"]
        + commitment["payout_commitment_hash"]
    )


def pack_btc_utxo_cell(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["seal_id"]
        + cell["policy_hash"]
        + cell["owner_authority_hash"]
        + cell["sealed_utxo_commitment_hash"]
        + cell["state_hash"]
        + u8(cell["status"])
        + cell["latest_receipt_hash"]
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_btc_utxo_receipt(receipt: dict[str, Any]) -> bytes:
    return (
        u8(receipt["action"])
        + receipt["seal_id"]
        + receipt["policy_hash"]
        + receipt["owner_authority_hash"]
        + receipt["sealed_utxo_commitment_hash"]
        + receipt["closure_commitment_hash"]
        + receipt["old_state_hash"]
        + receipt["new_state_hash"]
        + u8(receipt["old_status"])
        + u8(receipt["new_status"])
        + u64(receipt["old_nonce"])
        + u64(receipt["new_nonce"])
        + receipt["intent_core_hash"]
        + receipt["signed_intent_hash"]
        + receipt["payout_commitment_hash"]
        + receipt["latest_receipt_hash"]
        + receipt["signer_authority_hash"]
        + u64(receipt["expiry"])
    )


def zero_btc_utxo_cell() -> dict[str, Any]:
    return {
        "version": 0,
        "seal_id": ZERO_HASH,
        "policy_hash": ZERO_HASH,
        "owner_authority_hash": ZERO_HASH,
        "sealed_utxo_commitment_hash": ZERO_HASH,
        "state_hash": ZERO_HASH,
        "status": 0,
        "latest_receipt_hash": ZERO_HASH,
        "nonce": 0,
        "expiry": 0,
    }


def btc_utxo_entry_witness(op: int, old_cell_data: bytes, signed_intent: bytes, sig_payload: bytes) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(old_cell_data))
        + old_cell_data
        + u32(len(signed_intent))
        + signed_intent
        + u32(len(sig_payload))
        + sig_payload
    )
    return hex0x(payload)


def btc_utxo_base_state(label: str) -> dict[str, Any]:
    return {
        "seal_id": ckb_hash(f"NovaSeal BTC UTXO seal {label}".encode("ascii")),
        "policy_hash": ckb_hash(f"NovaSeal BTC UTXO policy {label}".encode("ascii")),
        "owner_authority_hash": xonly_pubkey(TEST_SECRET_KEY),
        "initial_state_hash": ckb_hash(f"NovaSeal BTC UTXO active state {label}".encode("ascii")),
        "closed_state_hash": ckb_hash(f"NovaSeal BTC UTXO closed state {label}".encode("ascii")),
        "btc_txid": ckb_hash(f"NovaSeal BTC UTXO txid {label}".encode("ascii")),
        "btc_vout_index": 1,
        "btc_amount_sats": 250_000,
        "script_pubkey_hash": ckb_hash(f"NovaSeal BTC UTXO script pubkey {label}".encode("ascii")),
        "spend_txid": ckb_hash(f"NovaSeal BTC UTXO spend txid {label}".encode("ascii")),
        "spend_wtxid": ckb_hash(f"NovaSeal BTC UTXO spend wtxid {label}".encode("ascii")),
        "spend_input_index": 0,
        "expiry": (1 << 63) - 1,
    }


def btc_utxo_canonical_hash(
    *,
    op: int,
    base: dict[str, Any],
    old_state_commitment: bytes,
    new_state_commitment: bytes,
    old_nonce: int,
    new_nonce: int,
    expiry: int,
    authority_hash: bytes,
    profile_body_hash: bytes,
    payout_commitment_hash: bytes,
) -> bytes:
    return canonical_envelope_hash(
        action=op,
        asset_id=base["seal_id"],
        xudt_type_hash=base["policy_hash"],
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
        authority_hash=authority_hash,
        profile_body_hash=profile_body_hash,
        payout_commitment_hash=payout_commitment_hash,
    )


def build_btc_utxo_material(
    *,
    op: int,
    base: dict[str, Any],
    old_cell: dict[str, Any] | None,
    mutate_signature: bool = False,
    utxo_commitment_mismatch: bool = False,
    zero_spend_txid: bool = False,
) -> dict[str, Any]:
    payout_commitment_hash = ZERO_HASH
    btc_txid = ckb_hash(b"NovaSeal mismatched UTXO txid") if utxo_commitment_mismatch else base["btc_txid"]
    sealed_utxo_commitment_hash = packed_hash(
        "BtcUtxoCommitmentV0",
        pack_btc_utxo_commitment(
            {
                "btc_txid": btc_txid,
                "btc_vout_index": base["btc_vout_index"],
                "btc_amount_sats": base["btc_amount_sats"],
                "script_pubkey_hash": base["script_pubkey_hash"],
            }
        ),
    )
    if op == OP_BTC_UTXO_INITIALIZE_ACTIVE_SEAL:
        old_status = 0
        new_status = STATUS_ACTIVE
        old_nonce = 0
        new_nonce = 0
        old_state_hash = ZERO_HASH
        new_state_hash = base["initial_state_hash"]
        spend_txid = ZERO_HASH
        spend_wtxid = ZERO_HASH
        spend_input_index = 0
        transition_commitment_hash = ZERO_HASH
        closure_commitment_hash = ZERO_HASH
        old_state_commitment = ZERO_HASH
        expected_receipt_hash = ZERO_HASH
        new_cell = {
            "version": BTC_UTXO_SEAL_VERSION,
            "seal_id": base["seal_id"],
            "policy_hash": base["policy_hash"],
            "owner_authority_hash": base["owner_authority_hash"],
            "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
            "state_hash": new_state_hash,
            "status": STATUS_ACTIVE,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": 0,
            "expiry": base["expiry"],
        }
        new_state_commitment = packed_hash("NovaBtcUtxoSealStateV0", pack_btc_utxo_state_commitment(new_cell))
        receipt_data = b""
    elif op == OP_BTC_UTXO_CLOSE:
        if old_cell is None:
            raise LiveAcceptanceError("BTC UTXO close material requires an old cell")
        old_status = STATUS_ACTIVE
        new_status = BTC_STATUS_CLOSED
        old_nonce = old_cell["nonce"]
        new_nonce = old_nonce + 1
        old_state_hash = old_cell["state_hash"]
        new_state_hash = base["closed_state_hash"]
        spend_txid = ZERO_HASH if zero_spend_txid else base["spend_txid"]
        spend_wtxid = base["spend_wtxid"]
        spend_input_index = base["spend_input_index"]
        transition_commitment_hash = ckb_hash(new_state_hash)
        closure_commitment_hash = packed_hash(
            "BtcUtxoClosureCommitmentV0",
            pack_btc_utxo_closure_commitment(
                {
                    "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
                    "spend_txid": spend_txid,
                    "spend_wtxid": spend_wtxid,
                    "spend_input_index": spend_input_index,
                    "transition_commitment_hash": transition_commitment_hash,
                    "payout_commitment_hash": payout_commitment_hash,
                }
            ),
        )
        old_state_commitment = packed_hash("NovaBtcUtxoSealStateV0", pack_btc_utxo_state_commitment(old_cell))
        new_cell = {
            "version": BTC_UTXO_SEAL_VERSION,
            "seal_id": old_cell["seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "owner_authority_hash": old_cell["owner_authority_hash"],
            "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
            "state_hash": new_state_hash,
            "status": BTC_STATUS_CLOSED,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": new_nonce,
            "expiry": old_cell["expiry"],
        }
        receipt_commitment = {
            "action": OP_BTC_UTXO_CLOSE,
            "seal_id": old_cell["seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "owner_authority_hash": old_cell["owner_authority_hash"],
            "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
            "closure_commitment_hash": closure_commitment_hash,
            "old_state_hash": old_cell["state_hash"],
            "new_state_hash": new_state_hash,
            "old_status": STATUS_ACTIVE,
            "new_status": BTC_STATUS_CLOSED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": ZERO_HASH,
            "payout_commitment_hash": payout_commitment_hash,
        }
        expected_receipt_hash = ZERO_HASH
        new_state_commitment = closure_commitment_hash
        receipt_data = b""
    else:
        raise LiveAcceptanceError(f"unknown BTC UTXO op {op}")

    core = {
        "action": op,
        "seal_id": base["seal_id"],
        "policy_hash": base["policy_hash"],
        "owner_authority_hash": base["owner_authority_hash"],
        "btc_txid": btc_txid,
        "btc_vout_index": base["btc_vout_index"],
        "btc_amount_sats": base["btc_amount_sats"],
        "script_pubkey_hash": base["script_pubkey_hash"],
        "spend_txid": spend_txid,
        "spend_wtxid": spend_wtxid,
        "spend_input_index": spend_input_index,
        "old_state_hash": old_state_hash,
        "new_state_hash": new_state_hash,
        "transition_commitment_hash": transition_commitment_hash,
        "old_status": old_status,
        "new_status": new_status,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "expiry": base["expiry"],
        "payout_commitment_hash": payout_commitment_hash,
    }
    core_data = pack_btc_utxo_intent_core(core)
    intent_core_hash = packed_hash("NovaBtcUtxoSealIntentCoreV0", core_data)
    if op == OP_BTC_UTXO_CLOSE:
        receipt_commitment["intent_core_hash"] = intent_core_hash
        expected_receipt_hash = packed_hash(
            "NovaBtcUtxoSealReceiptCommitmentV0",
            pack_btc_utxo_receipt_commitment(receipt_commitment),
        )
        new_cell["latest_receipt_hash"] = expected_receipt_hash
    canonical_hash = btc_utxo_canonical_hash(
        op=op,
        base=base,
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=base["expiry"],
        authority_hash=base["owner_authority_hash"],
        profile_body_hash=intent_core_hash,
        payout_commitment_hash=payout_commitment_hash,
    )
    signed_intent = pack_btc_utxo_signed_intent(core_data, canonical_hash, expected_receipt_hash)
    signed_intent_hash = packed_hash(
        "NovaBtcUtxoSealSigningDigestV0",
        pack_btc_utxo_signing_digest(intent_core_hash, canonical_hash, expected_receipt_hash),
    )
    sig_payload = bytearray(signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND))
    if mutate_signature:
        sig_payload[-1] ^= 1
    new_cell_data = pack_btc_utxo_cell(new_cell)
    receipt = None
    if op == OP_BTC_UTXO_CLOSE:
        receipt = {
            "action": OP_BTC_UTXO_CLOSE,
            "seal_id": old_cell["seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "owner_authority_hash": old_cell["owner_authority_hash"],
            "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
            "closure_commitment_hash": closure_commitment_hash,
            "old_state_hash": old_cell["state_hash"],
            "new_state_hash": new_state_hash,
            "old_status": STATUS_ACTIVE,
            "new_status": BTC_STATUS_CLOSED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": intent_core_hash,
            "signed_intent_hash": signed_intent_hash,
            "payout_commitment_hash": payout_commitment_hash,
            "latest_receipt_hash": expected_receipt_hash,
            "signer_authority_hash": old_cell["owner_authority_hash"],
            "expiry": old_cell["expiry"],
        }
        receipt_data = pack_btc_utxo_receipt(receipt)
    return {
        "old_cell": old_cell or zero_btc_utxo_cell(),
        "old_cell_data": pack_btc_utxo_cell(old_cell or zero_btc_utxo_cell()),
        "new_cell": new_cell,
        "new_cell_data": new_cell_data,
        "receipt": receipt,
        "receipt_data": receipt_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "signature_payload": bytes(sig_payload),
        "sealed_utxo_commitment_hash": sealed_utxo_commitment_hash,
        "closure_commitment_hash": closure_commitment_hash,
        "transition_commitment_hash": transition_commitment_hash,
        "latest_receipt_hash": expected_receipt_hash,
    }


def build_btc_utxo_initialize_tx(
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - STATE_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("BTC UTXO initialize funding capacity is too small")
    witness = btc_utxo_entry_witness(
        OP_BTC_UTXO_INITIALIZE_ACTIVE_SEAL,
        material["old_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        funding,
        [
            {"capacity": hex(STATE_CAPACITY), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"][1:]],
        [header_hash],
    )


def build_btc_utxo_close_tx(
    *,
    old_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("BTC UTXO close funding capacity is too small")
    witness = btc_utxo_entry_witness(
        OP_BTC_UTXO_CLOSE,
        material["old_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        [old_ref] + funding["cells"],
        [
            {"capacity": hex(old_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def pack_dual_seal_finality_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        commitment["sealed_utxo_commitment_hash"]
        + commitment["btc_closure_commitment_hash"]
        + commitment["old_ckb_state_hash"]
        + commitment["new_ckb_state_hash"]
        + u64(commitment["maturity_timepoint"])
        + commitment["payout_commitment_hash"]
    )


def pack_dual_seal_intent_core(core: dict[str, Any]) -> bytes:
    return (
        u8(core["action"])
        + core["dual_seal_id"]
        + core["policy_hash"]
        + core["btc_owner_authority_hash"]
        + core["ckb_authority_hash"]
        + core["sealed_utxo_commitment_hash"]
        + core["btc_closure_commitment_hash"]
        + core["old_ckb_state_hash"]
        + core["new_ckb_state_hash"]
        + u64(core["maturity_timepoint"])
        + u8(core["old_status"])
        + u8(core["new_status"])
        + u64(core["old_nonce"])
        + u64(core["new_nonce"])
        + u64(core["expiry"])
        + core["payout_commitment_hash"]
    )


def pack_dual_seal_signed_intent(core_data: bytes, canonical_hash: bytes, expected_receipt_hash: bytes) -> bytes:
    return core_data + canonical_hash + expected_receipt_hash


def pack_dual_seal_state_commitment(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["dual_seal_id"]
        + cell["policy_hash"]
        + cell["btc_owner_authority_hash"]
        + cell["ckb_authority_hash"]
        + cell["sealed_utxo_commitment_hash"]
        + cell["ckb_state_hash"]
        + u8(cell["status"])
        + u64(cell["nonce"])
        + u64(cell["maturity_timepoint"])
        + u64(cell["expiry"])
    )


def pack_dual_seal_receipt_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        u8(commitment["action"])
        + commitment["dual_seal_id"]
        + commitment["policy_hash"]
        + commitment["btc_owner_authority_hash"]
        + commitment["ckb_authority_hash"]
        + commitment["sealed_utxo_commitment_hash"]
        + commitment["btc_closure_commitment_hash"]
        + commitment["old_ckb_state_hash"]
        + commitment["new_ckb_state_hash"]
        + u8(commitment["old_status"])
        + u8(commitment["new_status"])
        + u64(commitment["old_nonce"])
        + u64(commitment["new_nonce"])
        + commitment["intent_core_hash"]
        + commitment["payout_commitment_hash"]
    )


def pack_dual_seal_cell(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["dual_seal_id"]
        + cell["policy_hash"]
        + cell["btc_owner_authority_hash"]
        + cell["ckb_authority_hash"]
        + cell["sealed_utxo_commitment_hash"]
        + cell["ckb_state_hash"]
        + u8(cell["status"])
        + cell["latest_receipt_hash"]
        + u64(cell["nonce"])
        + u64(cell["maturity_timepoint"])
        + u64(cell["expiry"])
    )


def pack_dual_seal_receipt(receipt: dict[str, Any]) -> bytes:
    return (
        u8(receipt["action"])
        + receipt["dual_seal_id"]
        + receipt["policy_hash"]
        + receipt["btc_owner_authority_hash"]
        + receipt["ckb_authority_hash"]
        + receipt["sealed_utxo_commitment_hash"]
        + receipt["btc_closure_commitment_hash"]
        + receipt["old_ckb_state_hash"]
        + receipt["new_ckb_state_hash"]
        + u8(receipt["old_status"])
        + u8(receipt["new_status"])
        + u64(receipt["old_nonce"])
        + u64(receipt["new_nonce"])
        + receipt["intent_core_hash"]
        + receipt["signed_intent_hash"]
        + receipt["payout_commitment_hash"]
        + receipt["latest_receipt_hash"]
        + receipt["signer_authority_hash"]
        + u64(receipt["maturity_timepoint"])
        + u64(receipt["expiry"])
    )


def zero_dual_seal_cell() -> dict[str, Any]:
    return {
        "version": 0,
        "dual_seal_id": ZERO_HASH,
        "policy_hash": ZERO_HASH,
        "btc_owner_authority_hash": ZERO_HASH,
        "ckb_authority_hash": ZERO_HASH,
        "sealed_utxo_commitment_hash": ZERO_HASH,
        "ckb_state_hash": ZERO_HASH,
        "status": 0,
        "latest_receipt_hash": ZERO_HASH,
        "nonce": 0,
        "maturity_timepoint": 0,
        "expiry": 0,
    }


def dual_seal_entry_witness(
    op: int,
    old_cell_data: bytes,
    signed_intent: bytes,
    btc_owner_sig_payload: bytes,
    ckb_sig_payload: bytes,
) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(old_cell_data))
        + old_cell_data
        + u32(len(signed_intent))
        + signed_intent
        + u32(len(btc_owner_sig_payload))
        + btc_owner_sig_payload
        + u32(len(ckb_sig_payload))
        + ckb_sig_payload
    )
    return hex0x(payload)


def dual_seal_base_state(label: str) -> dict[str, Any]:
    return {
        "dual_seal_id": ckb_hash(f"NovaSeal dual seal {label}".encode("ascii")),
        "policy_hash": ckb_hash(f"NovaSeal dual policy {label}".encode("ascii")),
        "btc_owner_authority_hash": xonly_pubkey(TEST_SECRET_KEY),
        "ckb_authority_hash": xonly_pubkey(HOLDER_SECRET_KEY),
        "sealed_utxo_commitment_hash": ckb_hash(f"NovaSeal dual sealed UTXO {label}".encode("ascii")),
        "initial_ckb_state_hash": ckb_hash(f"NovaSeal dual active CKB state {label}".encode("ascii")),
        "final_ckb_state_hash": ckb_hash(f"NovaSeal dual finalized CKB state {label}".encode("ascii")),
        "btc_closure_commitment_hash": ckb_hash(f"NovaSeal dual BTC closure {label}".encode("ascii")),
        "maturity_timepoint": 0,
        "expiry": (1 << 63) - 1,
    }


def dual_seal_canonical_hash(
    *,
    op: int,
    base: dict[str, Any],
    old_state_commitment: bytes,
    new_state_commitment: bytes,
    old_nonce: int,
    new_nonce: int,
    expiry: int,
    authority_hash: bytes,
    profile_body_hash: bytes,
    payout_commitment_hash: bytes,
) -> bytes:
    return canonical_envelope_hash(
        action=op,
        asset_id=base["dual_seal_id"],
        xudt_type_hash=base["policy_hash"],
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
        authority_hash=authority_hash,
        profile_body_hash=profile_body_hash,
        payout_commitment_hash=payout_commitment_hash,
    )


def build_dual_seal_material(
    *,
    op: int,
    base: dict[str, Any],
    old_cell: dict[str, Any] | None,
    mutate_btc_owner_signature: bool = False,
    mutate_ckb_authority_signature: bool = False,
    zero_btc_closure: bool = False,
) -> dict[str, Any]:
    payout_commitment_hash = ZERO_HASH
    if op == OP_DUAL_SEAL_INITIALIZE_ACTIVE:
        old_status = 0
        new_status = STATUS_ACTIVE
        old_nonce = 0
        new_nonce = 0
        old_ckb_state_hash = ZERO_HASH
        new_ckb_state_hash = base["initial_ckb_state_hash"]
        btc_closure_commitment_hash = ZERO_HASH
        old_state_commitment = ZERO_HASH
        expected_receipt_hash = ZERO_HASH
        new_cell = {
            "version": DUAL_SEAL_VERSION,
            "dual_seal_id": base["dual_seal_id"],
            "policy_hash": base["policy_hash"],
            "btc_owner_authority_hash": base["btc_owner_authority_hash"],
            "ckb_authority_hash": base["ckb_authority_hash"],
            "sealed_utxo_commitment_hash": base["sealed_utxo_commitment_hash"],
            "ckb_state_hash": new_ckb_state_hash,
            "status": STATUS_ACTIVE,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": 0,
            "maturity_timepoint": base["maturity_timepoint"],
            "expiry": base["expiry"],
        }
        new_state_commitment = packed_hash("NovaDualSealStateV0", pack_dual_seal_state_commitment(new_cell))
        receipt_data = b""
    elif op == OP_DUAL_SEAL_FINALIZE:
        if old_cell is None:
            raise LiveAcceptanceError("dual-seal finalization material requires an old cell")
        old_status = STATUS_ACTIVE
        new_status = DUAL_STATUS_FINALIZED
        old_nonce = old_cell["nonce"]
        new_nonce = old_nonce + 1
        old_ckb_state_hash = old_cell["ckb_state_hash"]
        new_ckb_state_hash = base["final_ckb_state_hash"]
        btc_closure_commitment_hash = ZERO_HASH if zero_btc_closure else base["btc_closure_commitment_hash"]
        old_state_commitment = packed_hash("NovaDualSealStateV0", pack_dual_seal_state_commitment(old_cell))
        finality_commitment_hash = packed_hash(
            "DualSealFinalityCommitmentV0",
            pack_dual_seal_finality_commitment(
                {
                    "sealed_utxo_commitment_hash": old_cell["sealed_utxo_commitment_hash"],
                    "btc_closure_commitment_hash": btc_closure_commitment_hash,
                    "old_ckb_state_hash": old_cell["ckb_state_hash"],
                    "new_ckb_state_hash": new_ckb_state_hash,
                    "maturity_timepoint": old_cell["maturity_timepoint"],
                    "payout_commitment_hash": payout_commitment_hash,
                }
            ),
        )
        new_state_commitment = finality_commitment_hash
        receipt_commitment = {
            "action": OP_DUAL_SEAL_FINALIZE,
            "dual_seal_id": old_cell["dual_seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "btc_owner_authority_hash": old_cell["btc_owner_authority_hash"],
            "ckb_authority_hash": old_cell["ckb_authority_hash"],
            "sealed_utxo_commitment_hash": old_cell["sealed_utxo_commitment_hash"],
            "btc_closure_commitment_hash": btc_closure_commitment_hash,
            "old_ckb_state_hash": old_cell["ckb_state_hash"],
            "new_ckb_state_hash": new_ckb_state_hash,
            "old_status": STATUS_ACTIVE,
            "new_status": DUAL_STATUS_FINALIZED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": ZERO_HASH,
            "payout_commitment_hash": payout_commitment_hash,
        }
        expected_receipt_hash = ZERO_HASH
        new_cell = zero_dual_seal_cell()
        receipt_data = b""
    else:
        raise LiveAcceptanceError(f"unknown dual-seal op {op}")

    core = {
        "action": op,
        "dual_seal_id": base["dual_seal_id"],
        "policy_hash": base["policy_hash"],
        "btc_owner_authority_hash": base["btc_owner_authority_hash"],
        "ckb_authority_hash": base["ckb_authority_hash"],
        "sealed_utxo_commitment_hash": base["sealed_utxo_commitment_hash"],
        "btc_closure_commitment_hash": btc_closure_commitment_hash,
        "old_ckb_state_hash": old_ckb_state_hash,
        "new_ckb_state_hash": new_ckb_state_hash,
        "maturity_timepoint": base["maturity_timepoint"],
        "old_status": old_status,
        "new_status": new_status,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "expiry": base["expiry"],
        "payout_commitment_hash": payout_commitment_hash,
    }
    core_data = pack_dual_seal_intent_core(core)
    intent_core_hash = packed_hash("NovaDualSealIntentCoreV0", core_data)
    if op == OP_DUAL_SEAL_FINALIZE:
        receipt_commitment["intent_core_hash"] = intent_core_hash
        expected_receipt_hash = packed_hash(
            "NovaDualSealReceiptCommitmentV0",
            pack_dual_seal_receipt_commitment(receipt_commitment),
        )
    canonical_hash = dual_seal_canonical_hash(
        op=op,
        base=base,
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=base["expiry"],
        authority_hash=base["ckb_authority_hash"],
        profile_body_hash=intent_core_hash,
        payout_commitment_hash=payout_commitment_hash,
    )
    signed_intent = pack_dual_seal_signed_intent(core_data, canonical_hash, expected_receipt_hash)
    signed_intent_hash = packed_hash("NovaDualSealSignedIntentV0", signed_intent)
    btc_owner_sig_payload = bytearray(signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND))
    ckb_sig_payload = bytearray(signature_payload(HOLDER_SECRET_KEY, signed_intent_hash, HOLDER_AUX_RAND))
    if mutate_btc_owner_signature:
        btc_owner_sig_payload[-1] ^= 1
    if mutate_ckb_authority_signature:
        ckb_sig_payload[-1] ^= 1
    new_cell_data = pack_dual_seal_cell(new_cell)
    receipt = None
    if op == OP_DUAL_SEAL_FINALIZE:
        receipt = {
            "action": OP_DUAL_SEAL_FINALIZE,
            "dual_seal_id": old_cell["dual_seal_id"],
            "policy_hash": old_cell["policy_hash"],
            "btc_owner_authority_hash": old_cell["btc_owner_authority_hash"],
            "ckb_authority_hash": old_cell["ckb_authority_hash"],
            "sealed_utxo_commitment_hash": old_cell["sealed_utxo_commitment_hash"],
            "btc_closure_commitment_hash": btc_closure_commitment_hash,
            "old_ckb_state_hash": old_cell["ckb_state_hash"],
            "new_ckb_state_hash": new_ckb_state_hash,
            "old_status": STATUS_ACTIVE,
            "new_status": DUAL_STATUS_FINALIZED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": intent_core_hash,
            "signed_intent_hash": signed_intent_hash,
            "payout_commitment_hash": payout_commitment_hash,
            "latest_receipt_hash": expected_receipt_hash,
            "signer_authority_hash": old_cell["ckb_authority_hash"],
            "maturity_timepoint": old_cell["maturity_timepoint"],
            "expiry": old_cell["expiry"],
        }
        receipt_data = pack_dual_seal_receipt(receipt)
    return {
        "old_cell": old_cell or zero_dual_seal_cell(),
        "old_cell_data": pack_dual_seal_cell(old_cell or zero_dual_seal_cell()),
        "new_cell": new_cell,
        "new_cell_data": new_cell_data,
        "receipt": receipt,
        "receipt_data": receipt_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "btc_owner_signature_payload": bytes(btc_owner_sig_payload),
        "ckb_signature_payload": bytes(ckb_sig_payload),
        "finality_commitment_hash": new_state_commitment,
        "btc_closure_commitment_hash": btc_closure_commitment_hash,
        "latest_receipt_hash": expected_receipt_hash,
    }


def build_dual_seal_initialize_tx(
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - STATE_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("dual-seal initialize funding capacity is too small")
    witness = dual_seal_entry_witness(
        OP_DUAL_SEAL_INITIALIZE_ACTIVE,
        material["old_cell_data"],
        material["signed_intent"],
        material["btc_owner_signature_payload"],
        material["ckb_signature_payload"],
    )
    return transaction(
        funding,
        [
            {"capacity": hex(STATE_CAPACITY), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"][1:]],
        [header_hash],
    )


def build_dual_seal_finalize_tx(
    *,
    old_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = old_ref["capacity"] + funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("dual-seal finalize funding capacity is too small")
    witness = dual_seal_entry_witness(
        OP_DUAL_SEAL_FINALIZE,
        material["old_cell_data"],
        material["signed_intent"],
        material["btc_owner_signature_payload"],
        material["ckb_signature_payload"],
    )
    return transaction(
        [old_ref] + funding["cells"],
        [
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def pack_fiber_settlement_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        commitment["channel_id"]
        + commitment["route_commitment_hash"]
        + commitment["payment_hash"]
        + commitment["old_balance_commitment_hash"]
        + commitment["new_balance_commitment_hash"]
        + u64(commitment["settlement_amount"])
        + commitment["payout_commitment_hash"]
    )


def pack_fiber_intent_core(core: dict[str, Any]) -> bytes:
    return (
        u8(core["action"])
        + core["candidate_id"]
        + core["policy_hash"]
        + core["operator_authority_hash"]
        + core["channel_id"]
        + core["route_commitment_hash"]
        + core["payment_hash"]
        + core["old_balance_commitment_hash"]
        + core["new_balance_commitment_hash"]
        + u64(core["settlement_amount"])
        + u8(core["old_status"])
        + u8(core["new_status"])
        + u64(core["old_nonce"])
        + u64(core["new_nonce"])
        + u64(core["expiry"])
        + core["payout_commitment_hash"]
    )


def pack_fiber_signed_intent(core_data: bytes, canonical_hash: bytes, expected_receipt_hash: bytes) -> bytes:
    return core_data + canonical_hash + expected_receipt_hash


def pack_fiber_state_commitment(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["candidate_id"]
        + cell["policy_hash"]
        + cell["operator_authority_hash"]
        + cell["channel_id"]
        + cell["balance_commitment_hash"]
        + u8(cell["status"])
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_fiber_receipt_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        u8(commitment["action"])
        + commitment["candidate_id"]
        + commitment["policy_hash"]
        + commitment["operator_authority_hash"]
        + commitment["channel_id"]
        + commitment["route_commitment_hash"]
        + commitment["payment_hash"]
        + commitment["old_balance_commitment_hash"]
        + commitment["new_balance_commitment_hash"]
        + u64(commitment["settlement_amount"])
        + u8(commitment["old_status"])
        + u8(commitment["new_status"])
        + u64(commitment["old_nonce"])
        + u64(commitment["new_nonce"])
        + commitment["intent_core_hash"]
        + commitment["payout_commitment_hash"]
    )


def pack_fiber_cell(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["candidate_id"]
        + cell["policy_hash"]
        + cell["operator_authority_hash"]
        + cell["channel_id"]
        + cell["balance_commitment_hash"]
        + u8(cell["status"])
        + cell["latest_receipt_hash"]
        + u64(cell["nonce"])
        + u64(cell["expiry"])
    )


def pack_fiber_receipt(receipt: dict[str, Any]) -> bytes:
    return (
        u8(receipt["action"])
        + receipt["candidate_id"]
        + receipt["policy_hash"]
        + receipt["operator_authority_hash"]
        + receipt["channel_id"]
        + receipt["route_commitment_hash"]
        + receipt["payment_hash"]
        + receipt["old_balance_commitment_hash"]
        + receipt["new_balance_commitment_hash"]
        + u64(receipt["settlement_amount"])
        + u8(receipt["old_status"])
        + u8(receipt["new_status"])
        + u64(receipt["old_nonce"])
        + u64(receipt["new_nonce"])
        + receipt["intent_core_hash"]
        + receipt["signed_intent_hash"]
        + receipt["payout_commitment_hash"]
        + receipt["latest_receipt_hash"]
        + receipt["signer_authority_hash"]
        + u64(receipt["expiry"])
    )


def zero_fiber_cell() -> dict[str, Any]:
    return {
        "version": 0,
        "candidate_id": ZERO_HASH,
        "policy_hash": ZERO_HASH,
        "operator_authority_hash": ZERO_HASH,
        "channel_id": ZERO_HASH,
        "balance_commitment_hash": ZERO_HASH,
        "status": 0,
        "latest_receipt_hash": ZERO_HASH,
        "nonce": 0,
        "expiry": 0,
    }


def fiber_entry_witness(op: int, old_cell_data: bytes, signed_intent: bytes, sig_payload: bytes) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(old_cell_data))
        + old_cell_data
        + u32(len(signed_intent))
        + signed_intent
        + u32(len(sig_payload))
        + sig_payload
    )
    return hex0x(payload)


def fiber_base_state(label: str) -> dict[str, Any]:
    return {
        "candidate_id": ckb_hash(f"NovaSeal Fiber candidate {label}".encode("ascii")),
        "policy_hash": ckb_hash(f"NovaSeal Fiber policy {label}".encode("ascii")),
        "operator_authority_hash": xonly_pubkey(TEST_SECRET_KEY),
        "channel_id": ckb_hash(f"NovaSeal Fiber channel {label}".encode("ascii")),
        "initial_balance_commitment_hash": ckb_hash(f"NovaSeal Fiber initial balance {label}".encode("ascii")),
        "settled_balance_commitment_hash": ckb_hash(f"NovaSeal Fiber settled balance {label}".encode("ascii")),
        "route_commitment_hash": ckb_hash(f"NovaSeal Fiber route {label}".encode("ascii")),
        "payment_hash": ckb_hash(f"NovaSeal Fiber payment {label}".encode("ascii")),
        "settlement_amount": 42_000,
        "expiry": (1 << 63) - 1,
    }


def fiber_canonical_hash(
    *,
    op: int,
    base: dict[str, Any],
    old_state_commitment: bytes,
    new_state_commitment: bytes,
    old_nonce: int,
    new_nonce: int,
    expiry: int,
    authority_hash: bytes,
    profile_body_hash: bytes,
    payout_commitment_hash: bytes,
) -> bytes:
    return canonical_envelope_hash(
        action=op,
        asset_id=base["candidate_id"],
        xudt_type_hash=base["policy_hash"],
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=expiry,
        authority_hash=authority_hash,
        profile_body_hash=profile_body_hash,
        payout_commitment_hash=payout_commitment_hash,
    )


def build_fiber_material(
    *,
    op: int,
    base: dict[str, Any],
    old_cell: dict[str, Any] | None,
    mutate_signature: bool = False,
    balance_replay: bool = False,
) -> dict[str, Any]:
    payout_commitment_hash = ZERO_HASH
    if op == OP_FIBER_INITIALIZE_ACTIVE_CANDIDATE:
        old_balance = ZERO_HASH
        new_balance = base["initial_balance_commitment_hash"]
        route_commitment_hash = ZERO_HASH
        payment_hash = ZERO_HASH
        settlement_amount = 0
        old_status = 0
        new_status = STATUS_ACTIVE
        old_nonce = 0
        new_nonce = 0
        old_state_commitment = ZERO_HASH
        expected_receipt_hash = ZERO_HASH
        new_cell = {
            "version": FIBER_CANDIDATE_VERSION,
            "candidate_id": base["candidate_id"],
            "policy_hash": base["policy_hash"],
            "operator_authority_hash": base["operator_authority_hash"],
            "channel_id": base["channel_id"],
            "balance_commitment_hash": new_balance,
            "status": STATUS_ACTIVE,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": 0,
            "expiry": base["expiry"],
        }
        new_state_commitment = packed_hash("NovaFiberCandidateStateV0", pack_fiber_state_commitment(new_cell))
        receipt_data = b""
    elif op == OP_FIBER_SETTLE:
        if old_cell is None:
            raise LiveAcceptanceError("Fiber settle material requires an old cell")
        old_balance = old_cell["balance_commitment_hash"]
        new_balance = old_cell["balance_commitment_hash"] if balance_replay else base["settled_balance_commitment_hash"]
        route_commitment_hash = base["route_commitment_hash"]
        payment_hash = base["payment_hash"]
        settlement_amount = base["settlement_amount"]
        old_status = STATUS_ACTIVE
        new_status = FIBER_STATUS_SETTLED
        old_nonce = old_cell["nonce"]
        new_nonce = old_nonce + 1
        old_state_commitment = packed_hash("NovaFiberCandidateStateV0", pack_fiber_state_commitment(old_cell))
        new_cell = {
            "version": FIBER_CANDIDATE_VERSION,
            "candidate_id": old_cell["candidate_id"],
            "policy_hash": old_cell["policy_hash"],
            "operator_authority_hash": old_cell["operator_authority_hash"],
            "channel_id": old_cell["channel_id"],
            "balance_commitment_hash": new_balance,
            "status": FIBER_STATUS_SETTLED,
            "latest_receipt_hash": ZERO_HASH,
            "nonce": new_nonce,
            "expiry": old_cell["expiry"],
        }
        new_state_commitment = packed_hash("NovaFiberCandidateStateV0", pack_fiber_state_commitment(new_cell))
        receipt_commitment = {
            "action": OP_FIBER_SETTLE,
            "candidate_id": old_cell["candidate_id"],
            "policy_hash": old_cell["policy_hash"],
            "operator_authority_hash": old_cell["operator_authority_hash"],
            "channel_id": old_cell["channel_id"],
            "route_commitment_hash": route_commitment_hash,
            "payment_hash": payment_hash,
            "old_balance_commitment_hash": old_balance,
            "new_balance_commitment_hash": new_balance,
            "settlement_amount": settlement_amount,
            "old_status": STATUS_ACTIVE,
            "new_status": FIBER_STATUS_SETTLED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": ZERO_HASH,
            "payout_commitment_hash": payout_commitment_hash,
        }
        expected_receipt_hash = ZERO_HASH
        receipt_data = b""
    else:
        raise LiveAcceptanceError(f"unknown Fiber op {op}")

    core = {
        "action": op,
        "candidate_id": base["candidate_id"],
        "policy_hash": base["policy_hash"],
        "operator_authority_hash": base["operator_authority_hash"],
        "channel_id": base["channel_id"],
        "route_commitment_hash": route_commitment_hash,
        "payment_hash": payment_hash,
        "old_balance_commitment_hash": old_balance,
        "new_balance_commitment_hash": new_balance,
        "settlement_amount": settlement_amount,
        "old_status": old_status,
        "new_status": new_status,
        "old_nonce": old_nonce,
        "new_nonce": new_nonce,
        "expiry": base["expiry"],
        "payout_commitment_hash": payout_commitment_hash,
    }
    core_data = pack_fiber_intent_core(core)
    intent_core_hash = packed_hash("NovaFiberCandidateIntentCoreV0", core_data)
    if op == OP_FIBER_SETTLE:
        receipt_commitment["intent_core_hash"] = intent_core_hash
        expected_receipt_hash = packed_hash(
            "NovaFiberCandidateReceiptCommitmentV0",
            pack_fiber_receipt_commitment(receipt_commitment),
        )
        new_cell["latest_receipt_hash"] = expected_receipt_hash
    canonical_hash = fiber_canonical_hash(
        op=op,
        base=base,
        old_state_commitment=old_state_commitment,
        new_state_commitment=new_state_commitment,
        old_nonce=old_nonce,
        new_nonce=new_nonce,
        expiry=base["expiry"],
        authority_hash=base["operator_authority_hash"],
        profile_body_hash=intent_core_hash,
        payout_commitment_hash=payout_commitment_hash,
    )
    signed_intent = pack_fiber_signed_intent(core_data, canonical_hash, expected_receipt_hash)
    signed_intent_hash = packed_hash("NovaFiberCandidateSignedIntentV0", signed_intent)
    sig_payload = bytearray(signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND))
    if mutate_signature:
        sig_payload[-1] ^= 1
    new_cell_data = pack_fiber_cell(new_cell)
    receipt = None
    settlement_commitment_hash = ZERO_HASH
    if op == OP_FIBER_SETTLE:
        settlement_commitment_hash = packed_hash(
            "FiberCandidateSettlementCommitmentV0",
            pack_fiber_settlement_commitment(
                {
                    "channel_id": old_cell["channel_id"],
                    "route_commitment_hash": route_commitment_hash,
                    "payment_hash": payment_hash,
                    "old_balance_commitment_hash": old_balance,
                    "new_balance_commitment_hash": new_balance,
                    "settlement_amount": settlement_amount,
                    "payout_commitment_hash": payout_commitment_hash,
                }
            ),
        )
        receipt = {
            "action": OP_FIBER_SETTLE,
            "candidate_id": old_cell["candidate_id"],
            "policy_hash": old_cell["policy_hash"],
            "operator_authority_hash": old_cell["operator_authority_hash"],
            "channel_id": old_cell["channel_id"],
            "route_commitment_hash": route_commitment_hash,
            "payment_hash": payment_hash,
            "old_balance_commitment_hash": old_balance,
            "new_balance_commitment_hash": new_balance,
            "settlement_amount": settlement_amount,
            "old_status": STATUS_ACTIVE,
            "new_status": FIBER_STATUS_SETTLED,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "intent_core_hash": intent_core_hash,
            "signed_intent_hash": signed_intent_hash,
            "payout_commitment_hash": payout_commitment_hash,
            "latest_receipt_hash": expected_receipt_hash,
            "signer_authority_hash": old_cell["operator_authority_hash"],
            "expiry": old_cell["expiry"],
        }
        receipt_data = pack_fiber_receipt(receipt)
    return {
        "old_cell": old_cell or zero_fiber_cell(),
        "old_cell_data": pack_fiber_cell(old_cell or zero_fiber_cell()),
        "new_cell": new_cell,
        "new_cell_data": new_cell_data,
        "receipt": receipt,
        "receipt_data": receipt_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "signature_payload": bytes(sig_payload),
        "settlement_commitment_hash": settlement_commitment_hash,
        "latest_receipt_hash": expected_receipt_hash,
    }


def build_fiber_initialize_tx(
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - STATE_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("Fiber initialize funding capacity is too small")
    witness = fiber_entry_witness(
        OP_FIBER_INITIALIZE_ACTIVE_CANDIDATE,
        material["old_cell_data"],
        material["signed_intent"],
        material["signature_payload"],
    )
    return transaction(
        funding,
        [
            {"capacity": hex(STATE_CAPACITY), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"][1:]],
        [header_hash],
    )


def build_fiber_settle_tx(
    *,
    old_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    material: dict[str, Any],
) -> dict[str, Any]:
    change_capacity = funding["total_capacity"] - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("Fiber settle funding capacity is too small")
    witness = fiber_entry_witness(OP_FIBER_SETTLE, material["old_cell_data"], material["signed_intent"], material["signature_payload"])
    return transaction(
        [old_ref] + funding["cells"],
        [
            {"capacity": hex(old_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["new_cell_data"]), hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def compile_contract_lifecycle(repo_root: pathlib.Path, contract: ReportContract, output: pathlib.Path) -> None:
    if contract.lifecycle_action is None:
        raise LiveAcceptanceError(f"{contract.profile} has no lifecycle action")
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
    subprocess.run(cmd, cwd=repo_root, check=True)


def run_fungible_xudt_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (args.run_dir or (repo_root / "target/novaseal-fungible-xudt-devnet-stateful-live" / str(int(time.time())))).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "nova-fungible-xudt-lifecycle-type.elf"
    compile_contract_lifecycle(repo_root, contract, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "running",
        "scenario": "fungible_xudt_issue_transfer_settle",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
        "expected_tx_hashes": named_pointer_rows(contract.tx_hashes, "pointer"),
        "required_live_checks": named_pointer_rows(contract.live_checks, "pointer"),
        "required_negative_cases": named_pointer_rows(contract.negative_cases, "key"),
    }
    stage = "initializing"
    try:
        stage = "start devnet"
        devnet.start()
        stage = "deploy artifacts"
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_fungible_xudt_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]
        provenance = stateful_provenance(
            repo_root,
            [
                pathlib.Path("proposals/novaseal/fungible-xudt-profile-v0/Cell.toml"),
                pathlib.Path("proposals/novaseal/fungible-xudt-profile-v0/src"),
                pathlib.Path("proposals/novaseal/fungible-xudt-profile-v0/schemas"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier"),
                pathlib.Path("scripts/novaseal_planned_profiles_devnet_stateful_live.py"),
                pathlib.Path("scripts/novaseal_devnet_stateful_live.py"),
            ],
            {"verifier": verifier_elf, "lifecycle": lifecycle_elf},
        )
        base = xudt_base_state("live")

        stage = "valid issue"
        issue_material = build_xudt_material(op=OP_ISSUE, base=base, old_cell=None)
        issue_header = devnet.rpc("get_tip_header")
        issue_funding = devnet.collect_spendable(STATE_CAPACITY + RECEIPT_CAPACITY + 100 * SHANNONS)
        issue_tx = build_xudt_issue_tx(issue_funding, lifecycle["data_hash"], cell_deps, issue_header["hash"], issue_material)
        issue_dry_run = devnet.rpc("dry_run_transaction", [issue_tx])
        issue_commit = devnet.submit_and_commit(issue_tx, "fungible xUDT issue")
        issue_balance_live = devnet.assert_live_cell(
            issue_commit["tx_hash"],
            0,
            label="xUDT issued balance",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=issue_material["new_cell_data"],
        )
        issue_receipt_live = devnet.assert_live_cell(
            issue_commit["tx_hash"],
            1,
            label="xUDT issue receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=issue_material["receipt_data"],
        )
        issued_ref = {"tx_hash": issue_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative transfer wrong holder signature"
        negative_header = devnet.rpc("get_tip_header")
        wrong_sig_material = build_xudt_material(
            op=OP_TRANSFER,
            base=base,
            old_cell=issue_material["new_cell"],
            mutate_signature=True,
        )
        wrong_sig_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_sig_tx = build_xudt_transfer_tx(
            old_ref=issued_ref,
            funding=wrong_sig_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=wrong_sig_material,
        )
        wrong_holder_signature_reject = devnet.dry_run_rejects(
            wrong_sig_tx,
            "xUDT wrong holder signature transfer",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative transfer amount mismatch"
        mismatch_material = build_xudt_material(
            op=OP_TRANSFER,
            base=base,
            old_cell=issue_material["new_cell"],
            transfer_amount_override=issue_material["new_cell"]["amount"] - 1,
        )
        mismatch_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        mismatch_tx = build_xudt_transfer_tx(
            old_ref=issued_ref,
            funding=mismatch_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=mismatch_material,
        )
        transfer_amount_mismatch_reject = devnet.dry_run_rejects(
            mismatch_tx,
            "xUDT transfer amount mismatch",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_transfer_negative_live = devnet.assert_live_cell(
            issued_ref["tx_hash"],
            issued_ref["index"],
            label="post-negative xUDT issued balance",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=issue_material["new_cell_data"],
        )

        stage = "valid transfer"
        transfer_header = devnet.rpc("get_tip_header")
        transfer_material = build_xudt_material(op=OP_TRANSFER, base=base, old_cell=issue_material["new_cell"])
        transfer_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        transfer_tx = build_xudt_transfer_tx(
            old_ref=issued_ref,
            funding=transfer_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=transfer_header["hash"],
            material=transfer_material,
        )
        transfer_dry_run = devnet.rpc("dry_run_transaction", [transfer_tx])
        transfer_commit = devnet.submit_and_commit(transfer_tx, "fungible xUDT transfer")
        old_balance_dead = devnet.wait_dead_cell(issued_ref["tx_hash"], issued_ref["index"])
        receiver_balance_live = devnet.assert_live_cell(
            transfer_commit["tx_hash"],
            0,
            label="xUDT receiver balance",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=transfer_material["new_cell_data"],
        )
        transfer_receipt_live = devnet.assert_live_cell(
            transfer_commit["tx_hash"],
            1,
            label="xUDT transfer receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=transfer_material["receipt_data"],
        )
        receiver_ref = {"tx_hash": transfer_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative settle wrong holder signature"
        settle_negative_header = devnet.rpc("get_tip_header")
        wrong_settle_material = build_xudt_material(
            op=OP_SETTLE,
            base=base,
            old_cell=transfer_material["new_cell"],
            mutate_signature=True,
        )
        wrong_settle_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_settle_tx = build_xudt_settle_tx(
            old_ref=receiver_ref,
            funding=wrong_settle_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=settle_negative_header["hash"],
            material=wrong_settle_material,
        )
        settle_wrong_holder_signature_reject = devnet.dry_run_rejects(
            wrong_settle_tx,
            "xUDT wrong holder signature settle",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_negative_state_live = devnet.assert_live_cell(
            receiver_ref["tx_hash"],
            receiver_ref["index"],
            label="post-negative xUDT receiver balance",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=transfer_material["new_cell_data"],
        )

        stage = "valid settle"
        settle_header = devnet.rpc("get_tip_header")
        settle_material = build_xudt_material(op=OP_SETTLE, base=base, old_cell=transfer_material["new_cell"])
        settle_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        settle_tx = build_xudt_settle_tx(
            old_ref=receiver_ref,
            funding=settle_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=settle_header["hash"],
            material=settle_material,
        )
        settle_dry_run = devnet.rpc("dry_run_transaction", [settle_tx])
        settle_commit = devnet.submit_and_commit(settle_tx, "fungible xUDT settle")
        receiver_balance_dead = devnet.wait_dead_cell(receiver_ref["tx_hash"], receiver_ref["index"])
        settlement_receipt_live = devnet.assert_live_cell(
            settle_commit["tx_hash"],
            0,
            label="xUDT settlement receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=settle_material["receipt_data"],
        )

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
                "provenance": provenance,
                "issue": {
                    "dry_run_cycles": issue_dry_run.get("cycles"),
                    "commit": issue_commit,
                    "balance_live": issue_balance_live.get("status") == "live",
                    "receipt_live": issue_receipt_live.get("status") == "live",
                    "balance_data_hash": hex0x(cell_data_hash(issue_material["new_cell_data"])),
                    "receipt_hash": hex0x(issue_material["latest_receipt_hash"]),
                },
                "transfer": {
                    "dry_run_cycles": transfer_dry_run.get("cycles"),
                    "commit": transfer_commit,
                    "old_balance_not_live": old_balance_dead.get("status") != "live",
                    "sender_balance_live": post_transfer_negative_live.get("status") == "live",
                    "receiver_balance_live": receiver_balance_live.get("status") == "live",
                    "receipt_live": transfer_receipt_live.get("status") == "live",
                    "amount_conserved": transfer_material["new_cell"]["amount"] == issue_material["new_cell"]["amount"],
                    "receipt_hash": hex0x(transfer_material["latest_receipt_hash"]),
                },
                "settle": {
                    "dry_run_cycles": settle_dry_run.get("cycles"),
                    "commit": settle_commit,
                    "old_balance_not_live": receiver_balance_dead.get("status") != "live",
                    "settlement_receipt_live": settlement_receipt_live.get("status") == "live",
                    "receipt_hash": hex0x(settle_material["latest_receipt_hash"]),
                },
                "negative_cases": {
                    "wrong_holder_signature_dry_run": wrong_holder_signature_reject,
                    "transfer_amount_mismatch_dry_run": transfer_amount_mismatch_reject,
                    "settle_wrong_holder_signature_dry_run": settle_wrong_holder_signature_reject,
                    "post_negative_state_still_live": post_negative_state_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update(
            {
                "status": "failed",
                "stage": stage,
                "error": str(error),
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
            }
        )
        return report
    finally:
        if not args.keep_node:
            devnet.stop()


def run_rwa_receipt_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (args.run_dir or (repo_root / "target/novaseal-rwa-receipt-devnet-stateful-live" / str(int(time.time())))).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "nova-rwa-receipt-lifecycle-type.elf"
    compile_contract_lifecycle(repo_root, contract, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "running",
        "scenario": "rwa_receipt_materialize_claim_settle",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
        "expected_tx_hashes": named_pointer_rows(contract.tx_hashes, "pointer"),
        "required_live_checks": named_pointer_rows(contract.live_checks, "pointer"),
        "required_negative_cases": named_pointer_rows(contract.negative_cases, "key"),
    }
    stage = "initializing"
    try:
        stage = "start devnet"
        devnet.start()
        stage = "deploy artifacts"
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_rwa_receipt_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]
        provenance = stateful_provenance(
            repo_root,
            [
                pathlib.Path("proposals/novaseal/rwa-receipt-profile-v0/Cell.toml"),
                pathlib.Path("proposals/novaseal/rwa-receipt-profile-v0/src"),
                pathlib.Path("proposals/novaseal/rwa-receipt-profile-v0/schemas"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier"),
                pathlib.Path("scripts/novaseal_planned_profiles_devnet_stateful_live.py"),
                pathlib.Path("scripts/novaseal_devnet_stateful_live.py"),
            ],
            {"verifier": verifier_elf, "lifecycle": lifecycle_elf},
        )
        base = rwa_base_state("live")

        stage = "valid materialize"
        materialize_material = build_rwa_material(op=OP_MATERIALIZE, base=base, old_cell=None)
        materialize_header = devnet.rpc("get_tip_header")
        materialize_funding = devnet.collect_spendable(STATE_CAPACITY + RECEIPT_CAPACITY + 100 * SHANNONS)
        materialize_tx = build_rwa_state_event_tx(
            op=OP_MATERIALIZE,
            old_ref=None,
            funding=materialize_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=materialize_header["hash"],
            material=materialize_material,
        )
        materialize_dry_run = devnet.rpc("dry_run_transaction", [materialize_tx])
        materialize_commit = devnet.submit_and_commit(materialize_tx, "RWA receipt materialize")
        materialized_receipt_live = devnet.assert_live_cell(
            materialize_commit["tx_hash"],
            0,
            label="RWA materialized receipt",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=materialize_material["new_cell_data"],
        )
        materialized_event_live = devnet.assert_live_cell(
            materialize_commit["tx_hash"],
            1,
            label="RWA materialized audit event",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=materialize_material["event_data"],
        )
        materialized_ref = {"tx_hash": materialize_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative claim wrong holder signature"
        negative_header = devnet.rpc("get_tip_header")
        wrong_holder_claim_material = build_rwa_material(
            op=OP_CLAIM,
            base=base,
            old_cell=materialize_material["new_cell"],
            mutate_holder_signature=True,
        )
        wrong_holder_claim_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_holder_claim_tx = build_rwa_state_event_tx(
            op=OP_CLAIM,
            old_ref=materialized_ref,
            funding=wrong_holder_claim_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=wrong_holder_claim_material,
        )
        wrong_holder_claim_reject = devnet.dry_run_rejects(
            wrong_holder_claim_tx,
            "RWA wrong holder claim",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_claim_negative_live = devnet.assert_live_cell(
            materialized_ref["tx_hash"],
            materialized_ref["index"],
            label="post-negative RWA materialized receipt",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=materialize_material["new_cell_data"],
        )

        stage = "valid claim"
        claim_header = devnet.rpc("get_tip_header")
        claim_material = build_rwa_material(op=OP_CLAIM, base=base, old_cell=materialize_material["new_cell"])
        claim_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        claim_tx = build_rwa_state_event_tx(
            op=OP_CLAIM,
            old_ref=materialized_ref,
            funding=claim_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=claim_header["hash"],
            material=claim_material,
        )
        claim_dry_run = devnet.rpc("dry_run_transaction", [claim_tx])
        claim_commit = devnet.submit_and_commit(claim_tx, "RWA receipt claim")
        old_receipt_dead = devnet.wait_dead_cell(materialized_ref["tx_hash"], materialized_ref["index"])
        claimed_receipt_live = devnet.assert_live_cell(
            claim_commit["tx_hash"],
            0,
            label="RWA claimed receipt",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=claim_material["new_cell_data"],
        )
        claim_event_live = devnet.assert_live_cell(
            claim_commit["tx_hash"],
            1,
            label="RWA claim event",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=claim_material["event_data"],
        )
        claimed_ref = {"tx_hash": claim_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative settlement wrong issuer signature"
        settle_negative_header = devnet.rpc("get_tip_header")
        wrong_issuer_settlement_material = build_rwa_material(
            op=OP_RWA_SETTLE,
            base=base,
            old_cell=claim_material["new_cell"],
            mutate_issuer_signature=True,
        )
        wrong_issuer_settlement_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_issuer_settlement_tx = build_rwa_settle_tx(
            old_ref=claimed_ref,
            funding=wrong_issuer_settlement_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=settle_negative_header["hash"],
            material=wrong_issuer_settlement_material,
        )
        wrong_issuer_settlement_reject = devnet.dry_run_rejects(
            wrong_issuer_settlement_tx,
            "RWA wrong issuer settlement",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative settlement amount mutation"
        amount_mutation_material = build_rwa_material(
            op=OP_RWA_SETTLE,
            base=base,
            old_cell=claim_material["new_cell"],
            settlement_amount_override=claim_material["new_cell"]["amount"] - 1,
        )
        amount_mutation_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        amount_mutation_tx = build_rwa_settle_tx(
            old_ref=claimed_ref,
            funding=amount_mutation_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=settle_negative_header["hash"],
            material=amount_mutation_material,
        )
        amount_mutation_reject = devnet.dry_run_rejects(
            amount_mutation_tx,
            "RWA settlement amount mutation",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_negative_state_live = devnet.assert_live_cell(
            claimed_ref["tx_hash"],
            claimed_ref["index"],
            label="post-negative RWA claimed receipt",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=claim_material["new_cell_data"],
        )

        stage = "valid settle"
        settle_header = devnet.rpc("get_tip_header")
        settle_material = build_rwa_material(op=OP_RWA_SETTLE, base=base, old_cell=claim_material["new_cell"])
        settle_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        settle_tx = build_rwa_settle_tx(
            old_ref=claimed_ref,
            funding=settle_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=settle_header["hash"],
            material=settle_material,
        )
        settle_dry_run = devnet.rpc("dry_run_transaction", [settle_tx])
        settle_commit = devnet.submit_and_commit(settle_tx, "RWA receipt settle")
        old_claim_dead = devnet.wait_dead_cell(claimed_ref["tx_hash"], claimed_ref["index"])
        settlement_event_live = devnet.assert_live_cell(
            settle_commit["tx_hash"],
            0,
            label="RWA settlement event",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=settle_material["event_data"],
        )

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
                "provenance": provenance,
                "materialize": {
                    "dry_run_cycles": materialize_dry_run.get("cycles"),
                    "commit": materialize_commit,
                    "receipt_live": materialized_receipt_live.get("status") == "live",
                    "audit_event_live": materialized_event_live.get("status") == "live",
                    "event_hash": hex0x(materialize_material["latest_receipt_hash"]),
                },
                "claim": {
                    "dry_run_cycles": claim_dry_run.get("cycles"),
                    "commit": claim_commit,
                    "old_receipt_not_live": old_receipt_dead.get("status") != "live",
                    "claimed_receipt_live": claimed_receipt_live.get("status") == "live",
                    "claim_event_live": claim_event_live.get("status") == "live",
                    "event_hash": hex0x(claim_material["latest_receipt_hash"]),
                },
                "settle": {
                    "dry_run_cycles": settle_dry_run.get("cycles"),
                    "commit": settle_commit,
                    "old_claim_not_live": old_claim_dead.get("status") != "live",
                    "settlement_receipt_live": settlement_event_live.get("status") == "live",
                    "settlement_event_live": settlement_event_live.get("status") == "live",
                    "amount_conserved": settle_material["old_cell"]["amount"] == claim_material["new_cell"]["amount"],
                    "event_hash": hex0x(settle_material["latest_receipt_hash"]),
                },
                "negative_cases": {
                    "wrong_holder_claim_dry_run": wrong_holder_claim_reject,
                    "wrong_issuer_settlement_dry_run": wrong_issuer_settlement_reject,
                    "amount_mutation_dry_run": amount_mutation_reject,
                    "post_negative_state_still_live": post_negative_state_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update(
            {
                "status": "failed",
                "stage": stage,
                "error": str(error),
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
            }
        )
        return report
    finally:
        if not args.keep_node:
            devnet.stop()


def run_btc_transaction_commitment_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (
        args.run_dir
        or (repo_root / "target/novaseal-btc-transaction-commitment-devnet-stateful-live" / str(int(time.time())))
    ).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "nova-btc-transaction-commitment-lifecycle-type.elf"
    compile_contract_lifecycle(repo_root, contract, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "running",
        "scenario": "btc_transaction_commitment_initialize_then_commit",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
        "expected_tx_hashes": named_pointer_rows(contract.tx_hashes, "pointer"),
        "required_live_checks": named_pointer_rows(contract.live_checks, "pointer"),
        "required_negative_cases": named_pointer_rows(contract.negative_cases, "key"),
        "btc_public_verification_scope": (
            "live CKB transition executes the BIP340 runtime verifier and binds a declared BTC txid/wtxid/output tuple; "
            "SPV/indexer finality remains separate production evidence"
        ),
    }
    stage = "initializing"
    try:
        stage = "start devnet"
        devnet.start()
        stage = "deploy artifacts"
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_btc_transaction_commitment_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]
        provenance = stateful_provenance(
            repo_root,
            [
                pathlib.Path("proposals/novaseal/btc-transaction-commitment-profile-v0/Cell.toml"),
                pathlib.Path("proposals/novaseal/btc-transaction-commitment-profile-v0/src"),
                pathlib.Path("proposals/novaseal/btc-transaction-commitment-profile-v0/schemas"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier"),
                pathlib.Path("scripts/novaseal_planned_profiles_devnet_stateful_live.py"),
                pathlib.Path("scripts/novaseal_devnet_stateful_live.py"),
            ],
            {"verifier": verifier_elf, "lifecycle": lifecycle_elf},
        )
        base = btc_tx_base_state("live")

        stage = "valid initialize"
        initialize_material = build_btc_tx_material(op=OP_BTC_INITIALIZE_ACTIVE_STATE, base=base, old_cell=None)
        initialize_header = devnet.rpc("get_tip_header")
        initialize_funding = devnet.collect_spendable(STATE_CAPACITY + 100 * SHANNONS)
        initialize_tx = build_btc_tx_initialize_tx(
            initialize_funding,
            lifecycle["data_hash"],
            cell_deps,
            initialize_header["hash"],
            initialize_material,
        )
        initialize_dry_run = devnet.rpc("dry_run_transaction", [initialize_tx])
        initialize_commit = devnet.submit_and_commit(initialize_tx, "BTC transaction commitment initialize")
        initial_state_live = devnet.assert_live_cell(
            initialize_commit["tx_hash"],
            0,
            label="BTC transaction active state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )
        initial_ref = {"tx_hash": initialize_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative wrong committer signature"
        negative_header = devnet.rpc("get_tip_header")
        wrong_sig_material = build_btc_tx_material(
            op=OP_BTC_COMMIT_TRANSACTION,
            base=base,
            old_cell=initialize_material["new_cell"],
            mutate_signature=True,
        )
        wrong_sig_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_sig_tx = build_btc_tx_commit_tx(
            old_ref=initial_ref,
            funding=wrong_sig_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=wrong_sig_material,
        )
        wrong_committer_signature_reject = devnet.dry_run_rejects(
            wrong_sig_tx,
            "BTC transaction wrong committer signature",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative zero BTC txid"
        zero_txid_material = build_btc_tx_material(
            op=OP_BTC_COMMIT_TRANSACTION,
            base=base,
            old_cell=initialize_material["new_cell"],
            zero_btc_txid=True,
        )
        zero_txid_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        zero_txid_tx = build_btc_tx_commit_tx(
            old_ref=initial_ref,
            funding=zero_txid_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=zero_txid_material,
        )
        zero_btc_txid_reject = devnet.dry_run_rejects(
            zero_txid_tx,
            "BTC transaction zero txid",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative transition hash mismatch"
        mismatch_material = build_btc_tx_material(
            op=OP_BTC_COMMIT_TRANSACTION,
            base=base,
            old_cell=initialize_material["new_cell"],
            transition_hash_mismatch=True,
        )
        mismatch_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        mismatch_tx = build_btc_tx_commit_tx(
            old_ref=initial_ref,
            funding=mismatch_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=mismatch_material,
        )
        transition_hash_mismatch_reject = devnet.dry_run_rejects(
            mismatch_tx,
            "BTC transaction transition hash mismatch",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_negative_state_live = devnet.assert_live_cell(
            initial_ref["tx_hash"],
            initial_ref["index"],
            label="post-negative BTC transaction active state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )

        stage = "valid commit transaction"
        commit_header = devnet.rpc("get_tip_header")
        commit_material = build_btc_tx_material(
            op=OP_BTC_COMMIT_TRANSACTION,
            base=base,
            old_cell=initialize_material["new_cell"],
        )
        commit_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        commit_tx = build_btc_tx_commit_tx(
            old_ref=initial_ref,
            funding=commit_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=commit_header["hash"],
            material=commit_material,
        )
        commit_dry_run = devnet.rpc("dry_run_transaction", [commit_tx])
        commit_commit = devnet.submit_and_commit(commit_tx, "BTC transaction commitment transition")
        old_state_dead = devnet.wait_dead_cell(initial_ref["tx_hash"], initial_ref["index"])
        committed_state_live = devnet.assert_live_cell(
            commit_commit["tx_hash"],
            0,
            label="BTC transaction committed state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=commit_material["new_cell_data"],
        )
        receipt_live = devnet.assert_live_cell(
            commit_commit["tx_hash"],
            1,
            label="BTC transaction commitment receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=commit_material["receipt_data"],
        )

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
                "provenance": provenance,
                "initialize": {
                    "dry_run_cycles": initialize_dry_run.get("cycles"),
                    "commit": initialize_commit,
                    "state_live": initial_state_live.get("status") == "live",
                    "state_data_hash": hex0x(cell_data_hash(initialize_material["new_cell_data"])),
                },
                "commit_transaction": {
                    "dry_run_cycles": commit_dry_run.get("cycles"),
                    "commit": commit_commit,
                    "old_state_not_live": old_state_dead.get("status") != "live",
                    "new_state_live": committed_state_live.get("status") == "live",
                    "receipt_live": receipt_live.get("status") == "live",
                    "btc_tx_tuple_bound": (
                        commit_material["new_cell"]["btc_tx_commitment_hash"] == commit_material["btc_tx_commitment_hash"]
                        and commit_material["new_cell"]["btc_tx_commitment_hash"] != ZERO_HASH
                    ),
                    "transition_commitment_bound": commit_material["transition_commitment_hash"] == ckb_hash(base["committed_state_hash"]),
                    "public_btc_verification_executed": True,
                    "public_btc_verification_scope": "BIP340 runtime verifier execution over the signed BTC commitment intent",
                    "btc_tx_commitment_hash": hex0x(commit_material["btc_tx_commitment_hash"]),
                    "signed_intent_hash": hex0x(commit_material["signed_intent_hash"]),
                    "receipt_hash": hex0x(commit_material["latest_receipt_hash"]),
                },
                "negative_cases": {
                    "wrong_committer_signature_dry_run": wrong_committer_signature_reject,
                    "zero_btc_txid_dry_run": zero_btc_txid_reject,
                    "transition_hash_mismatch_dry_run": transition_hash_mismatch_reject,
                    "post_negative_state_still_live": post_negative_state_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update(
            {
                "status": "failed",
                "stage": stage,
                "error": str(error),
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
            }
        )
        return report
    finally:
        if not args.keep_node:
            devnet.stop()


def run_btc_utxo_seal_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (args.run_dir or (repo_root / "target/novaseal-btc-utxo-seal-devnet-stateful-live" / str(int(time.time())))).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "nova-btc-utxo-seal-lifecycle-type.elf"
    compile_contract_lifecycle(repo_root, contract, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "running",
        "scenario": "btc_utxo_seal_initialize_then_close",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
        "expected_tx_hashes": named_pointer_rows(contract.tx_hashes, "pointer"),
        "required_live_checks": named_pointer_rows(contract.live_checks, "pointer"),
        "required_negative_cases": named_pointer_rows(contract.negative_cases, "key"),
        "btc_public_verification_scope": (
            "live CKB closure executes the BIP340 runtime verifier and binds a declared BTC UTXO/spend tuple; "
            "SPV/indexer spend-finality evidence remains separate production evidence"
        ),
    }
    stage = "initializing"
    try:
        stage = "start devnet"
        devnet.start()
        stage = "deploy artifacts"
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_btc_utxo_seal_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]
        provenance = stateful_provenance(
            repo_root,
            [
                pathlib.Path("proposals/novaseal/btc-utxo-seal-profile-v0/Cell.toml"),
                pathlib.Path("proposals/novaseal/btc-utxo-seal-profile-v0/src"),
                pathlib.Path("proposals/novaseal/btc-utxo-seal-profile-v0/schemas"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier"),
                pathlib.Path("scripts/novaseal_planned_profiles_devnet_stateful_live.py"),
                pathlib.Path("scripts/novaseal_devnet_stateful_live.py"),
            ],
            {"verifier": verifier_elf, "lifecycle": lifecycle_elf},
        )
        base = btc_utxo_base_state("live")

        stage = "valid initialize"
        initialize_material = build_btc_utxo_material(op=OP_BTC_UTXO_INITIALIZE_ACTIVE_SEAL, base=base, old_cell=None)
        initialize_header = devnet.rpc("get_tip_header")
        initialize_funding = devnet.collect_spendable(STATE_CAPACITY + 100 * SHANNONS)
        initialize_tx = build_btc_utxo_initialize_tx(
            initialize_funding,
            lifecycle["data_hash"],
            cell_deps,
            initialize_header["hash"],
            initialize_material,
        )
        initialize_dry_run = devnet.rpc("dry_run_transaction", [initialize_tx])
        initialize_commit = devnet.submit_and_commit(initialize_tx, "BTC UTXO seal initialize")
        initial_state_live = devnet.assert_live_cell(
            initialize_commit["tx_hash"],
            0,
            label="BTC UTXO active seal",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )
        initial_ref = {"tx_hash": initialize_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative wrong owner signature"
        negative_header = devnet.rpc("get_tip_header")
        wrong_sig_material = build_btc_utxo_material(
            op=OP_BTC_UTXO_CLOSE,
            base=base,
            old_cell=initialize_material["new_cell"],
            mutate_signature=True,
        )
        wrong_sig_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_sig_tx = build_btc_utxo_close_tx(
            old_ref=initial_ref,
            funding=wrong_sig_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=wrong_sig_material,
        )
        wrong_owner_signature_reject = devnet.dry_run_rejects(
            wrong_sig_tx,
            "BTC UTXO wrong owner signature",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative UTXO commitment mismatch"
        mismatch_material = build_btc_utxo_material(
            op=OP_BTC_UTXO_CLOSE,
            base=base,
            old_cell=initialize_material["new_cell"],
            utxo_commitment_mismatch=True,
        )
        mismatch_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        mismatch_tx = build_btc_utxo_close_tx(
            old_ref=initial_ref,
            funding=mismatch_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=mismatch_material,
        )
        utxo_commitment_mismatch_reject = devnet.dry_run_rejects(
            mismatch_tx,
            "BTC UTXO commitment mismatch",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative zero spend txid"
        zero_spend_material = build_btc_utxo_material(
            op=OP_BTC_UTXO_CLOSE,
            base=base,
            old_cell=initialize_material["new_cell"],
            zero_spend_txid=True,
        )
        zero_spend_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        zero_spend_tx = build_btc_utxo_close_tx(
            old_ref=initial_ref,
            funding=zero_spend_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=zero_spend_material,
        )
        zero_spend_txid_reject = devnet.dry_run_rejects(
            zero_spend_tx,
            "BTC UTXO zero spend txid",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_negative_state_live = devnet.assert_live_cell(
            initial_ref["tx_hash"],
            initial_ref["index"],
            label="post-negative BTC UTXO active seal",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )

        stage = "valid close UTXO seal"
        close_header = devnet.rpc("get_tip_header")
        close_material = build_btc_utxo_material(
            op=OP_BTC_UTXO_CLOSE,
            base=base,
            old_cell=initialize_material["new_cell"],
        )
        close_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        close_tx = build_btc_utxo_close_tx(
            old_ref=initial_ref,
            funding=close_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=close_header["hash"],
            material=close_material,
        )
        close_dry_run = devnet.rpc("dry_run_transaction", [close_tx])
        close_commit = devnet.submit_and_commit(close_tx, "BTC UTXO seal closure")
        old_state_dead = devnet.wait_dead_cell(initial_ref["tx_hash"], initial_ref["index"])
        closed_state_live = devnet.assert_live_cell(
            close_commit["tx_hash"],
            0,
            label="BTC UTXO closed seal",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=close_material["new_cell_data"],
        )
        receipt_live = devnet.assert_live_cell(
            close_commit["tx_hash"],
            1,
            label="BTC UTXO closure receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=close_material["receipt_data"],
        )

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
                "provenance": provenance,
                "initialize": {
                    "dry_run_cycles": initialize_dry_run.get("cycles"),
                    "commit": initialize_commit,
                    "state_live": initial_state_live.get("status") == "live",
                    "state_data_hash": hex0x(cell_data_hash(initialize_material["new_cell_data"])),
                },
                "close_utxo_seal": {
                    "dry_run_cycles": close_dry_run.get("cycles"),
                    "commit": close_commit,
                    "old_state_not_live": old_state_dead.get("status") != "live",
                    "new_state_live": closed_state_live.get("status") == "live",
                    "receipt_live": receipt_live.get("status") == "live",
                    "sealed_utxo_tuple_bound": (
                        initialize_material["new_cell"]["sealed_utxo_commitment_hash"] == close_material["sealed_utxo_commitment_hash"]
                    ),
                    "spend_tuple_bound": close_material["closure_commitment_hash"] != ZERO_HASH,
                    "public_btc_spend_verification_executed": True,
                    "public_btc_verification_scope": "BIP340 runtime verifier execution over the signed BTC UTXO closure intent",
                    "sealed_utxo_commitment_hash": hex0x(close_material["sealed_utxo_commitment_hash"]),
                    "closure_commitment_hash": hex0x(close_material["closure_commitment_hash"]),
                    "signed_intent_hash": hex0x(close_material["signed_intent_hash"]),
                    "receipt_hash": hex0x(close_material["latest_receipt_hash"]),
                },
                "negative_cases": {
                    "wrong_owner_signature_dry_run": wrong_owner_signature_reject,
                    "utxo_commitment_mismatch_dry_run": utxo_commitment_mismatch_reject,
                    "zero_spend_txid_dry_run": zero_spend_txid_reject,
                    "post_negative_state_still_live": post_negative_state_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update(
            {
                "status": "failed",
                "stage": stage,
                "error": str(error),
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
            }
        )
        return report
    finally:
        if not args.keep_node:
            devnet.stop()


def run_dual_seal_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (args.run_dir or (repo_root / "target/novaseal-dual-seal-devnet-stateful-live" / str(int(time.time())))).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "nova-dual-seal-lifecycle-type.elf"
    compile_contract_lifecycle(repo_root, contract, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "running",
        "scenario": "dual_seal_initialize_then_finalize",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
        "expected_tx_hashes": named_pointer_rows(contract.tx_hashes, "pointer"),
        "required_live_checks": named_pointer_rows(contract.live_checks, "pointer"),
        "required_negative_cases": named_pointer_rows(contract.negative_cases, "key"),
        "finality_scope": (
            "live CKB finalisation executes the maturity guard and both BIP340 authorities over a declared BTC closure commitment; "
            "public BTC SPV/indexer closure evidence remains separate production evidence"
        ),
    }
    stage = "initializing"
    try:
        stage = "start devnet"
        devnet.start()
        stage = "deploy artifacts"
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_dual_seal_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]
        provenance = stateful_provenance(
            repo_root,
            [
                pathlib.Path("proposals/novaseal/dual-seal-profile-v0/Cell.toml"),
                pathlib.Path("proposals/novaseal/dual-seal-profile-v0/src"),
                pathlib.Path("proposals/novaseal/dual-seal-profile-v0/schemas"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier"),
                pathlib.Path("scripts/novaseal_planned_profiles_devnet_stateful_live.py"),
                pathlib.Path("scripts/novaseal_devnet_stateful_live.py"),
            ],
            {"verifier": verifier_elf, "lifecycle": lifecycle_elf},
        )
        base = dual_seal_base_state("live")

        stage = "valid initialize"
        initialize_material = build_dual_seal_material(op=OP_DUAL_SEAL_INITIALIZE_ACTIVE, base=base, old_cell=None)
        initialize_header = devnet.rpc("get_tip_header")
        initialize_funding = devnet.collect_spendable(STATE_CAPACITY + 100 * SHANNONS)
        initialize_tx = build_dual_seal_initialize_tx(
            initialize_funding,
            lifecycle["data_hash"],
            cell_deps,
            initialize_header["hash"],
            initialize_material,
        )
        initialize_dry_run = devnet.rpc("dry_run_transaction", [initialize_tx])
        initialize_commit = devnet.submit_and_commit(initialize_tx, "dual-seal initialize")
        initial_state_live = devnet.assert_live_cell(
            initialize_commit["tx_hash"],
            0,
            label="dual-seal active state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )
        initial_ref = {"tx_hash": initialize_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative wrong BTC owner signature"
        negative_header = devnet.rpc("get_tip_header")
        wrong_btc_owner_material = build_dual_seal_material(
            op=OP_DUAL_SEAL_FINALIZE,
            base=base,
            old_cell=initialize_material["new_cell"],
            mutate_btc_owner_signature=True,
        )
        wrong_btc_owner_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_btc_owner_tx = build_dual_seal_finalize_tx(
            old_ref=initial_ref,
            funding=wrong_btc_owner_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=wrong_btc_owner_material,
        )
        wrong_btc_owner_reject = devnet.dry_run_rejects(
            wrong_btc_owner_tx,
            "dual-seal wrong BTC owner signature",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative wrong CKB authority signature"
        wrong_ckb_authority_material = build_dual_seal_material(
            op=OP_DUAL_SEAL_FINALIZE,
            base=base,
            old_cell=initialize_material["new_cell"],
            mutate_ckb_authority_signature=True,
        )
        wrong_ckb_authority_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_ckb_authority_tx = build_dual_seal_finalize_tx(
            old_ref=initial_ref,
            funding=wrong_ckb_authority_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=wrong_ckb_authority_material,
        )
        wrong_ckb_authority_reject = devnet.dry_run_rejects(
            wrong_ckb_authority_tx,
            "dual-seal wrong CKB authority signature",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative missing BTC closure"
        missing_closure_material = build_dual_seal_material(
            op=OP_DUAL_SEAL_FINALIZE,
            base=base,
            old_cell=initialize_material["new_cell"],
            zero_btc_closure=True,
        )
        missing_closure_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        missing_closure_tx = build_dual_seal_finalize_tx(
            old_ref=initial_ref,
            funding=missing_closure_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=missing_closure_material,
        )
        missing_closure_reject = devnet.dry_run_rejects(
            missing_closure_tx,
            "dual-seal missing BTC closure commitment",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_negative_state_live = devnet.assert_live_cell(
            initial_ref["tx_hash"],
            initial_ref["index"],
            label="post-negative dual-seal active state",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )

        stage = "valid finalize"
        finalize_header = devnet.rpc("get_tip_header")
        finalize_material = build_dual_seal_material(
            op=OP_DUAL_SEAL_FINALIZE,
            base=base,
            old_cell=initialize_material["new_cell"],
        )
        finalize_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        finalize_tx = build_dual_seal_finalize_tx(
            old_ref=initial_ref,
            funding=finalize_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=finalize_header["hash"],
            material=finalize_material,
        )
        finalize_dry_run = devnet.rpc("dry_run_transaction", [finalize_tx])
        finalize_commit = devnet.submit_and_commit(finalize_tx, "dual-seal finalization")
        old_state_dead = devnet.wait_dead_cell(initial_ref["tx_hash"], initial_ref["index"])
        receipt_live = devnet.assert_live_cell(
            finalize_commit["tx_hash"],
            0,
            label="dual-seal final receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=finalize_material["receipt_data"],
        )

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
                "provenance": provenance,
                "initialize": {
                    "dry_run_cycles": initialize_dry_run.get("cycles"),
                    "commit": initialize_commit,
                    "state_live": initial_state_live.get("status") == "live",
                    "state_data_hash": hex0x(cell_data_hash(initialize_material["new_cell_data"])),
                },
                "finalize_dual_seal": {
                    "dry_run_cycles": finalize_dry_run.get("cycles"),
                    "commit": finalize_commit,
                    "old_state_not_live": old_state_dead.get("status") != "live",
                    "receipt_live": receipt_live.get("status") == "live",
                    "btc_closure_bound": finalize_material["btc_closure_commitment_hash"] != ZERO_HASH,
                    "ckb_maturity_executed": base["maturity_timepoint"] == 0,
                    "dual_authority_executed": True,
                    "finality_commitment_hash": hex0x(finalize_material["finality_commitment_hash"]),
                    "btc_closure_commitment_hash": hex0x(finalize_material["btc_closure_commitment_hash"]),
                    "signed_intent_hash": hex0x(finalize_material["signed_intent_hash"]),
                    "receipt_hash": hex0x(finalize_material["latest_receipt_hash"]),
                },
                "negative_cases": {
                    "wrong_btc_owner_signature_dry_run": wrong_btc_owner_reject,
                    "wrong_ckb_authority_signature_dry_run": wrong_ckb_authority_reject,
                    "btc_closure_commitment_missing_dry_run": missing_closure_reject,
                    "post_negative_state_still_live": post_negative_state_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update(
            {
                "status": "failed",
                "stage": stage,
                "error": str(error),
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
            }
        )
        return report
    finally:
        if not args.keep_node:
            devnet.stop()


def run_fiber_candidate_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (args.run_dir or (repo_root / "target/novaseal-fiber-candidate-devnet-stateful-live" / str(int(time.time())))).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "nova-fiber-candidate-lifecycle-type.elf"
    compile_contract_lifecycle(repo_root, contract, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-planned-profile-devnet-stateful-live-v0.1",
        "profile": contract.profile,
        "status": "running",
        "scenario": "fiber_candidate_initialize_then_settle",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
        "expected_tx_hashes": named_pointer_rows(contract.tx_hashes, "pointer"),
        "required_live_checks": named_pointer_rows(contract.live_checks, "pointer"),
        "required_negative_cases": named_pointer_rows(contract.negative_cases, "key"),
        "fiber_execution_scope": "live CKB stateful settlement path; real Fiber node/channel execution remains a later external experiment",
    }
    stage = "initializing"
    try:
        stage = "start devnet"
        devnet.start()
        stage = "deploy artifacts"
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_fiber_candidate_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]
        provenance = stateful_provenance(
            repo_root,
            [
                pathlib.Path("proposals/novaseal/fiber-candidate-profile-v0/Cell.toml"),
                pathlib.Path("proposals/novaseal/fiber-candidate-profile-v0/src"),
                pathlib.Path("proposals/novaseal/fiber-candidate-profile-v0/schemas"),
                pathlib.Path("proposals/novaseal/v0-mvp-skeleton/verifier/novaseal_btc_verifier"),
                pathlib.Path("scripts/novaseal_planned_profiles_devnet_stateful_live.py"),
                pathlib.Path("scripts/novaseal_devnet_stateful_live.py"),
            ],
            {"verifier": verifier_elf, "lifecycle": lifecycle_elf},
        )
        base = fiber_base_state("live")

        stage = "valid initialize"
        initialize_material = build_fiber_material(op=OP_FIBER_INITIALIZE_ACTIVE_CANDIDATE, base=base, old_cell=None)
        initialize_header = devnet.rpc("get_tip_header")
        initialize_funding = devnet.collect_spendable(STATE_CAPACITY + 100 * SHANNONS)
        initialize_tx = build_fiber_initialize_tx(
            initialize_funding,
            lifecycle["data_hash"],
            cell_deps,
            initialize_header["hash"],
            initialize_material,
        )
        initialize_dry_run = devnet.rpc("dry_run_transaction", [initialize_tx])
        initialize_commit = devnet.submit_and_commit(initialize_tx, "Fiber candidate initialize")
        initial_state_live = devnet.assert_live_cell(
            initialize_commit["tx_hash"],
            0,
            label="Fiber active candidate",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )
        initial_ref = {"tx_hash": initialize_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}

        stage = "negative wrong operator signature"
        negative_header = devnet.rpc("get_tip_header")
        wrong_sig_material = build_fiber_material(
            op=OP_FIBER_SETTLE,
            base=base,
            old_cell=initialize_material["new_cell"],
            mutate_signature=True,
        )
        wrong_sig_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        wrong_sig_tx = build_fiber_settle_tx(
            old_ref=initial_ref,
            funding=wrong_sig_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=wrong_sig_material,
        )
        wrong_operator_signature_reject = devnet.dry_run_rejects(
            wrong_sig_tx,
            "Fiber wrong operator signature",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )

        stage = "negative balance replay"
        replay_material = build_fiber_material(
            op=OP_FIBER_SETTLE,
            base=base,
            old_cell=initialize_material["new_cell"],
            balance_replay=True,
        )
        replay_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        replay_tx = build_fiber_settle_tx(
            old_ref=initial_ref,
            funding=replay_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            material=replay_material,
        )
        balance_commitment_replay_reject = devnet.dry_run_rejects(
            replay_tx,
            "Fiber balance commitment replay",
            expected_source="Inputs[0].Type",
            expected_data_hash=lifecycle["data_hash"],
            expected_error_code=5,
        )
        post_negative_state_live = devnet.assert_live_cell(
            initial_ref["tx_hash"],
            initial_ref["index"],
            label="post-negative Fiber active candidate",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=initialize_material["new_cell_data"],
        )

        stage = "valid settle"
        settle_header = devnet.rpc("get_tip_header")
        settle_material = build_fiber_material(op=OP_FIBER_SETTLE, base=base, old_cell=initialize_material["new_cell"])
        settle_funding = devnet.collect_spendable(RECEIPT_CAPACITY + 100 * SHANNONS)
        settle_tx = build_fiber_settle_tx(
            old_ref=initial_ref,
            funding=settle_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=settle_header["hash"],
            material=settle_material,
        )
        settle_dry_run = devnet.rpc("dry_run_transaction", [settle_tx])
        settle_commit = devnet.submit_and_commit(settle_tx, "Fiber candidate settlement")
        old_candidate_dead = devnet.wait_dead_cell(initial_ref["tx_hash"], initial_ref["index"])
        settled_candidate_live = devnet.assert_live_cell(
            settle_commit["tx_hash"],
            0,
            label="Fiber settled candidate",
            expected_capacity=STATE_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=lifecycle_type(lifecycle["data_hash"]),
            expected_data=settle_material["new_cell_data"],
        )
        receipt_live = devnet.assert_live_cell(
            settle_commit["tx_hash"],
            1,
            label="Fiber settlement receipt",
            expected_capacity=RECEIPT_CAPACITY,
            expected_lock=always_success_lock(),
            expected_type=None,
            expected_data=settle_material["receipt_data"],
        )

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {"verifier": verifier, "lifecycle": lifecycle},
                "provenance": provenance,
                "initialize": {
                    "dry_run_cycles": initialize_dry_run.get("cycles"),
                    "commit": initialize_commit,
                    "candidate_live": initial_state_live.get("status") == "live",
                    "candidate_data_hash": hex0x(cell_data_hash(initialize_material["new_cell_data"])),
                },
                "settle_fiber_candidate": {
                    "dry_run_cycles": settle_dry_run.get("cycles"),
                    "commit": settle_commit,
                    "old_candidate_not_live": old_candidate_dead.get("status") != "live",
                    "new_candidate_live": settled_candidate_live.get("status") == "live",
                    "receipt_live": receipt_live.get("status") == "live",
                    "balance_commitment_progressed": (
                        settle_material["new_cell"]["balance_commitment_hash"]
                        != initialize_material["new_cell"]["balance_commitment_hash"]
                    ),
                    "fiber_execution_executed": True,
                    "fiber_execution_scope": "profile-level live CKB settlement path; external Fiber node experiment is still separate",
                    "settlement_commitment_hash": hex0x(settle_material["settlement_commitment_hash"]),
                    "signed_intent_hash": hex0x(settle_material["signed_intent_hash"]),
                    "receipt_hash": hex0x(settle_material["latest_receipt_hash"]),
                },
                "negative_cases": {
                    "wrong_operator_signature_dry_run": wrong_operator_signature_reject,
                    "balance_commitment_replay_dry_run": balance_commitment_replay_reject,
                    "post_negative_state_still_live": post_negative_state_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update(
            {
                "status": "failed",
                "stage": stage,
                "error": str(error),
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
            }
        )
        return report
    finally:
        if not args.keep_node:
            devnet.stop()


def run_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    if contract.profile == "fungible-xudt":
        return run_fungible_xudt_live(args, contract)
    if contract.profile == "rwa-receipt":
        return run_rwa_receipt_live(args, contract)
    if contract.profile == "btc-transaction-commitment":
        return run_btc_transaction_commitment_live(args, contract)
    if contract.profile == "btc-utxo-seal":
        return run_btc_utxo_seal_live(args, contract)
    if contract.profile == "dual-seal":
        return run_dual_seal_live(args, contract)
    if contract.profile == "fiber-candidate":
        return run_fiber_candidate_live(args, contract)
    report = not_run_report(contract)
    report["live_runner_gap"] = f"{contract.profile} live runner is not implemented yet"
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
    if args.live:
        report = run_live(args, contract)
        write_json(output, report, args.pretty)
        print(f"wrote {output} status={report.get('status')} profile={args.profile}")
        return 0 if report.get("status") == "passed" else 1

    write_json(output, report, args.pretty)
    print(f"wrote {output} status=not_run profile={args.profile}")
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
