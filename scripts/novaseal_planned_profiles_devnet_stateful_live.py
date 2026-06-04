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


def run_live(args: argparse.Namespace, contract: ReportContract) -> dict[str, Any]:
    if contract.profile == "fungible-xudt":
        return run_fungible_xudt_live(args, contract)
    if contract.profile == "rwa-receipt":
        return run_rwa_receipt_live(args, contract)
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
