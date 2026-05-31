#!/usr/bin/env python3
"""Run a live CKB devnet NovaSeal Agreement originate -> repay lifecycle."""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import time
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
    ckb_hash,
    ckb_hash_hex,
    cell_data_hash,
    deploy_code_cell,
    hex0x,
    packed_hash,
    resolve_ckb_bin,
    schnorr_sign,
    transaction,
    u8,
    u16,
    u32,
    u64,
    xonly_pubkey,
)


AGREEMENT_VERSION = 0
ASSET_KIND_CKB = 0
EARLY_CLOSE_FIXED_FEE = 0
STATUS_OFFERED = 0
STATUS_ACTIVE = 1
STATUS_REPAID = 2
STATUS_DEFAULTED = 3
PATH_ORIGINATE = 0
PATH_REPAY_BEFORE_EXPIRY = 1
PATH_CLAIM_AFTER_EXPIRY = 2
PAYOUT_BORROWER_PRINCIPAL = 0
PAYOUT_LENDER_REPAYMENT = 1
PAYOUT_BORROWER_COLLATERAL_RETURN = 2
PAYOUT_LENDER_DEFAULT_CLAIM = 3
NATIVE_CKB_PAYOUT_OCCUPIED_CAPACITY = 300 * SHANNONS
LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE = 300 * SHANNONS
LENDER_SECRET_KEY = bytes.fromhex("11" * 32)
LENDER_AUX_RAND = bytes([0x24]) * 32


def parse_args() -> argparse.Namespace:
    repo_root = pathlib.Path(__file__).resolve().parents[1]
    default_ckb_repo = repo_root.parent / "ckb"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=pathlib.Path, default=repo_root)
    parser.add_argument("--ckb-repo", type=pathlib.Path, default=default_ckb_repo)
    parser.add_argument("--ckb-bin", type=pathlib.Path)
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        default=repo_root / "target/novaseal-agreement-devnet-stateful-live.json",
    )
    parser.add_argument("--run-dir", type=pathlib.Path)
    parser.add_argument("--pretty", action="store_true")
    parser.add_argument("--keep-node", action="store_true")
    return parser.parse_args()


def epoch_number_from_header(header: dict[str, Any]) -> int:
    # CKB encodes EpochNumberWithFraction as number:24 | index:16 | length:16.
    return int(header["epoch"], 16) & ((1 << 24) - 1)


def pack_agreement_terms(terms: dict[str, Any]) -> bytes:
    return (
        u16(terms["version"])
        + terms["agreement_id"]
        + terms["terms_hash"]
        + terms["borrower_authority_hash"]
        + terms["lender_authority_hash"]
        + u8(terms["collateral_asset_kind"])
        + terms["collateral_asset_hash"]
        + u64(terms["collateral_amount"])
        + u8(terms["principal_asset_kind"])
        + terms["principal_asset_hash"]
        + u64(terms["principal_amount"])
        + u64(terms["fixed_fee_amount"])
        + u64(terms["start_timepoint"])
        + u64(terms["expiry_timepoint"])
        + u8(terms["early_close_policy"])
    )


def pack_agreement_cell(cell: dict[str, Any]) -> bytes:
    return (
        u16(cell["version"])
        + cell["agreement_id"]
        + cell["terms_hash"]
        + cell["borrower_authority_hash"]
        + cell["lender_authority_hash"]
        + u8(cell["collateral_asset_kind"])
        + cell["collateral_asset_hash"]
        + u64(cell["collateral_amount"])
        + u8(cell["principal_asset_kind"])
        + cell["principal_asset_hash"]
        + u64(cell["principal_amount"])
        + u64(cell["fixed_fee_amount"])
        + u64(cell["expiry_timepoint"])
        + u8(cell["status"])
        + cell["latest_receipt_hash"]
        + u64(cell["nonce"])
    )


def pack_agreement_intent_core(core: dict[str, Any]) -> bytes:
    return (
        u8(core["action"])
        + core["agreement_id"]
        + core["terms_hash"]
        + core["borrower_authority_hash"]
        + core["lender_authority_hash"]
        + u8(core["old_status"])
        + u8(core["new_status"])
        + u64(core["old_nonce"])
        + u64(core["new_nonce"])
        + u64(core["terminal_amount"])
        + core["payout_commitment_hash"]
        + u64(core["expiry_timepoint"])
    )


def pack_agreement_signed_intent(core_bytes: bytes, expected_receipt_hash: bytes) -> bytes:
    return core_bytes + expected_receipt_hash


def pack_agreement_receipt_commitment(commitment: dict[str, Any]) -> bytes:
    return (
        u8(commitment["action"])
        + commitment["agreement_id"]
        + u8(commitment["old_status"])
        + u8(commitment["new_status"])
        + commitment["terms_hash"]
        + commitment["borrower_authority_hash"]
        + commitment["lender_authority_hash"]
        + u64(commitment["terminal_amount"])
        + u64(commitment["old_nonce"])
        + u64(commitment["new_nonce"])
        + commitment["intent_core_hash"]
        + commitment["payout_commitment_hash"]
    )


def pack_repay_payout_commitment(lender_repayment_hash: bytes, borrower_collateral_return_hash: bytes) -> bytes:
    return lender_repayment_hash + borrower_collateral_return_hash


def pack_agreement_receipt(receipt: dict[str, Any]) -> bytes:
    return (
        u8(receipt["action"])
        + receipt["agreement_id"]
        + u8(receipt["old_status"])
        + u8(receipt["new_status"])
        + receipt["terms_hash"]
        + receipt["borrower_authority_hash"]
        + receipt["lender_authority_hash"]
        + u64(receipt["collateral_amount"])
        + u64(receipt["principal_amount"])
        + u64(receipt["fixed_fee_amount"])
        + u64(receipt["terminal_amount"])
        + receipt["previous_receipt_hash"]
        + receipt["latest_receipt_hash"]
        + receipt["intent_core_hash"]
        + receipt["signed_intent_hash"]
        + receipt["payout_commitment_hash"]
        + u64(receipt["nonce"])
        + u64(receipt["timepoint"])
    )


def pack_native_ckb_payout(payout: dict[str, Any]) -> bytes:
    return (
        u8(payout["action"])
        + payout["agreement_id"]
        + u8(payout["role"])
        + payout["recipient_authority_hash"]
        + u8(payout["asset_kind"])
        + payout["asset_hash"]
        + u64(payout["amount"])
        + payout["terms_hash"]
        + u64(payout["nonce"])
    )


def signature_payload(secret_key: bytes, message_hash: bytes, aux_rand: bytes) -> bytes:
    pubkey, signature = schnorr_sign(message_hash, secret_key, aux_rand)
    return pubkey + signature


def entry_witness(
    op: int,
    terms_data: bytes,
    active_data: bytes,
    signed_intent: bytes,
    borrower_sig_payload: bytes,
    lender_sig_payload: bytes,
) -> str:
    payload = (
        b"CSARGv1\0"
        + u8(op)
        + u32(len(terms_data))
        + terms_data
        + u32(len(active_data))
        + active_data
        + u32(len(signed_intent))
        + signed_intent
        + u32(len(borrower_sig_payload))
        + borrower_sig_payload
        + u32(len(lender_sig_payload))
        + lender_sig_payload
    )
    return hex0x(payload)


def make_terms(now: int, label: str, *, expiry_timepoint: int | None = None) -> dict[str, Any]:
    borrower = xonly_pubkey(TEST_SECRET_KEY)
    lender = xonly_pubkey(LENDER_SECRET_KEY)
    agreement_id = ckb_hash(f"NovaSeal Agreement live devnet v0 {label}".encode("ascii"))
    terms_hash = ckb_hash(f"NovaSeal Agreement live devnet terms v0 {label}".encode("ascii"))
    return {
        "version": AGREEMENT_VERSION,
        "agreement_id": agreement_id,
        "terms_hash": terms_hash,
        "borrower_authority_hash": borrower,
        "lender_authority_hash": lender,
        "collateral_asset_kind": ASSET_KIND_CKB,
        "collateral_asset_hash": ZERO_HASH,
        "collateral_amount": 50 * SHANNONS,
        "principal_asset_kind": ASSET_KIND_CKB,
        "principal_asset_hash": ZERO_HASH,
        "principal_amount": 20 * SHANNONS,
        "fixed_fee_amount": 2 * SHANNONS,
        "start_timepoint": 0,
        "expiry_timepoint": expiry_timepoint if expiry_timepoint is not None else now + 1_000_000,
        "early_close_policy": EARLY_CLOSE_FIXED_FEE,
    }


def build_origin_material(
    terms: dict[str, Any],
    now: int,
    *,
    mutate_borrower_signature: bool = False,
    mutate_lender_signature: bool = False,
) -> dict[str, Any]:
    payout = {
        "action": PATH_ORIGINATE,
        "agreement_id": terms["agreement_id"],
        "role": PAYOUT_BORROWER_PRINCIPAL,
        "recipient_authority_hash": terms["borrower_authority_hash"],
        "asset_kind": terms["principal_asset_kind"],
        "asset_hash": terms["principal_asset_hash"],
        "amount": terms["principal_amount"],
        "terms_hash": terms["terms_hash"],
        "nonce": 0,
    }
    payout_data = pack_native_ckb_payout(payout)
    payout_commitment_hash = packed_hash("NativeCkbPayoutV0", payout_data)
    core = {
        "action": PATH_ORIGINATE,
        "agreement_id": terms["agreement_id"],
        "terms_hash": terms["terms_hash"],
        "borrower_authority_hash": terms["borrower_authority_hash"],
        "lender_authority_hash": terms["lender_authority_hash"],
        "old_status": STATUS_OFFERED,
        "new_status": STATUS_ACTIVE,
        "old_nonce": 0,
        "new_nonce": 0,
        "terminal_amount": terms["principal_amount"],
        "payout_commitment_hash": payout_commitment_hash,
        "expiry_timepoint": terms["expiry_timepoint"],
    }
    core_data = pack_agreement_intent_core(core)
    intent_core_hash = packed_hash("NovaAgreementIntentCoreV0", core_data)
    receipt_commitment = {
        "action": PATH_ORIGINATE,
        "agreement_id": terms["agreement_id"],
        "old_status": STATUS_OFFERED,
        "new_status": STATUS_ACTIVE,
        "terms_hash": terms["terms_hash"],
        "borrower_authority_hash": terms["borrower_authority_hash"],
        "lender_authority_hash": terms["lender_authority_hash"],
        "terminal_amount": terms["principal_amount"],
        "old_nonce": 0,
        "new_nonce": 0,
        "intent_core_hash": intent_core_hash,
        "payout_commitment_hash": payout_commitment_hash,
    }
    receipt_commitment_data = pack_agreement_receipt_commitment(receipt_commitment)
    materialized_receipt_hash = packed_hash("NovaAgreementReceiptCommitmentV0", receipt_commitment_data)
    signed_intent = pack_agreement_signed_intent(core_data, materialized_receipt_hash)
    signed_intent_hash = packed_hash("NovaAgreementSignedIntentV0", signed_intent)
    active_cell = {
        "version": AGREEMENT_VERSION,
        "agreement_id": terms["agreement_id"],
        "terms_hash": terms["terms_hash"],
        "borrower_authority_hash": terms["borrower_authority_hash"],
        "lender_authority_hash": terms["lender_authority_hash"],
        "collateral_asset_kind": terms["collateral_asset_kind"],
        "collateral_asset_hash": terms["collateral_asset_hash"],
        "collateral_amount": terms["collateral_amount"],
        "principal_asset_kind": terms["principal_asset_kind"],
        "principal_asset_hash": terms["principal_asset_hash"],
        "principal_amount": terms["principal_amount"],
        "fixed_fee_amount": terms["fixed_fee_amount"],
        "expiry_timepoint": terms["expiry_timepoint"],
        "status": STATUS_ACTIVE,
        "latest_receipt_hash": materialized_receipt_hash,
        "nonce": 0,
    }
    active_data = pack_agreement_cell(active_cell)
    receipt = {
        "action": PATH_ORIGINATE,
        "agreement_id": terms["agreement_id"],
        "old_status": STATUS_OFFERED,
        "new_status": STATUS_ACTIVE,
        "terms_hash": terms["terms_hash"],
        "borrower_authority_hash": terms["borrower_authority_hash"],
        "lender_authority_hash": terms["lender_authority_hash"],
        "collateral_amount": terms["collateral_amount"],
        "principal_amount": terms["principal_amount"],
        "fixed_fee_amount": terms["fixed_fee_amount"],
        "terminal_amount": terms["principal_amount"],
        "previous_receipt_hash": ZERO_HASH,
        "latest_receipt_hash": materialized_receipt_hash,
        "intent_core_hash": intent_core_hash,
        "signed_intent_hash": signed_intent_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "nonce": 0,
        "timepoint": now,
    }
    receipt_data = pack_agreement_receipt(receipt)
    borrower_sig = bytearray(signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND))
    lender_sig = bytearray(signature_payload(LENDER_SECRET_KEY, signed_intent_hash, LENDER_AUX_RAND))
    if mutate_borrower_signature:
        borrower_sig[-1] ^= 1
    if mutate_lender_signature:
        lender_sig[-1] ^= 1
    return {
        "terms_data": pack_agreement_terms(terms),
        "active_cell": active_cell,
        "active_data": active_data,
        "payout_data": payout_data,
        "receipt_data": receipt_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "intent_core_hash": intent_core_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "borrower_sig": bytes(borrower_sig),
        "lender_sig": bytes(lender_sig),
    }


def build_repay_material(
    terms: dict[str, Any],
    active_cell: dict[str, Any],
    previous_receipt_hash: bytes,
    now: int,
    *,
    mutate_borrower_signature: bool = False,
) -> dict[str, Any]:
    repayment_amount = active_cell["principal_amount"] + active_cell["fixed_fee_amount"]
    next_nonce = active_cell["nonce"] + 1
    lender_payout = {
        "action": PATH_REPAY_BEFORE_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "role": PAYOUT_LENDER_REPAYMENT,
        "recipient_authority_hash": active_cell["lender_authority_hash"],
        "asset_kind": active_cell["principal_asset_kind"],
        "asset_hash": active_cell["principal_asset_hash"],
        "amount": repayment_amount,
        "terms_hash": active_cell["terms_hash"],
        "nonce": next_nonce,
    }
    borrower_payout = {
        "action": PATH_REPAY_BEFORE_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "role": PAYOUT_BORROWER_COLLATERAL_RETURN,
        "recipient_authority_hash": active_cell["borrower_authority_hash"],
        "asset_kind": active_cell["collateral_asset_kind"],
        "asset_hash": active_cell["collateral_asset_hash"],
        "amount": active_cell["collateral_amount"],
        "terms_hash": active_cell["terms_hash"],
        "nonce": next_nonce,
    }
    lender_payout_data = pack_native_ckb_payout(lender_payout)
    borrower_payout_data = pack_native_ckb_payout(borrower_payout)
    payout_commitment_data = pack_repay_payout_commitment(
        packed_hash("NativeCkbPayoutV0", lender_payout_data),
        packed_hash("NativeCkbPayoutV0", borrower_payout_data),
    )
    payout_commitment_hash = packed_hash("RepayPayoutCommitmentV0", payout_commitment_data)
    core = {
        "action": PATH_REPAY_BEFORE_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "terms_hash": active_cell["terms_hash"],
        "borrower_authority_hash": active_cell["borrower_authority_hash"],
        "lender_authority_hash": active_cell["lender_authority_hash"],
        "old_status": STATUS_ACTIVE,
        "new_status": STATUS_REPAID,
        "old_nonce": active_cell["nonce"],
        "new_nonce": next_nonce,
        "terminal_amount": repayment_amount,
        "payout_commitment_hash": payout_commitment_hash,
        "expiry_timepoint": active_cell["expiry_timepoint"],
    }
    core_data = pack_agreement_intent_core(core)
    intent_core_hash = packed_hash("NovaAgreementIntentCoreV0", core_data)
    receipt_commitment = {
        "action": PATH_REPAY_BEFORE_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "old_status": STATUS_ACTIVE,
        "new_status": STATUS_REPAID,
        "terms_hash": active_cell["terms_hash"],
        "borrower_authority_hash": active_cell["borrower_authority_hash"],
        "lender_authority_hash": active_cell["lender_authority_hash"],
        "terminal_amount": repayment_amount,
        "old_nonce": active_cell["nonce"],
        "new_nonce": next_nonce,
        "intent_core_hash": intent_core_hash,
        "payout_commitment_hash": payout_commitment_hash,
    }
    materialized_receipt_hash = packed_hash(
        "NovaAgreementReceiptCommitmentV0",
        pack_agreement_receipt_commitment(receipt_commitment),
    )
    signed_intent = pack_agreement_signed_intent(core_data, materialized_receipt_hash)
    signed_intent_hash = packed_hash("NovaAgreementSignedIntentV0", signed_intent)
    closed_cell = dict(active_cell)
    closed_cell.update({"status": STATUS_REPAID, "latest_receipt_hash": materialized_receipt_hash, "nonce": next_nonce})
    closed_data = pack_agreement_cell(closed_cell)
    receipt = {
        "action": PATH_REPAY_BEFORE_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "old_status": STATUS_ACTIVE,
        "new_status": STATUS_REPAID,
        "terms_hash": active_cell["terms_hash"],
        "borrower_authority_hash": active_cell["borrower_authority_hash"],
        "lender_authority_hash": active_cell["lender_authority_hash"],
        "collateral_amount": active_cell["collateral_amount"],
        "principal_amount": active_cell["principal_amount"],
        "fixed_fee_amount": active_cell["fixed_fee_amount"],
        "terminal_amount": repayment_amount,
        "previous_receipt_hash": previous_receipt_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "intent_core_hash": intent_core_hash,
        "signed_intent_hash": signed_intent_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "nonce": next_nonce,
        "timepoint": now,
    }
    receipt_data = pack_agreement_receipt(receipt)
    borrower_sig = bytearray(signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND))
    if mutate_borrower_signature:
        borrower_sig[-1] ^= 1
    lender_sig = signature_payload(LENDER_SECRET_KEY, signed_intent_hash, LENDER_AUX_RAND)
    return {
        "terms_data": pack_agreement_terms(terms),
        "active_data": pack_agreement_cell(active_cell),
        "closed_cell": closed_cell,
        "closed_data": closed_data,
        "lender_payout": lender_payout,
        "borrower_payout": borrower_payout,
        "lender_payout_data": lender_payout_data,
        "borrower_payout_data": borrower_payout_data,
        "receipt_data": receipt_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "intent_core_hash": intent_core_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "borrower_sig": bytes(borrower_sig),
        "lender_sig": lender_sig,
        "repayment_amount": repayment_amount,
    }


def build_claim_material(
    terms: dict[str, Any],
    active_cell: dict[str, Any],
    previous_receipt_hash: bytes,
    now: int,
    *,
    mutate_lender_signature: bool = False,
) -> dict[str, Any]:
    claim_amount = active_cell["collateral_amount"]
    next_nonce = active_cell["nonce"] + 1
    claim_payout = {
        "action": PATH_CLAIM_AFTER_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "role": PAYOUT_LENDER_DEFAULT_CLAIM,
        "recipient_authority_hash": active_cell["lender_authority_hash"],
        "asset_kind": active_cell["collateral_asset_kind"],
        "asset_hash": active_cell["collateral_asset_hash"],
        "amount": claim_amount,
        "terms_hash": active_cell["terms_hash"],
        "nonce": next_nonce,
    }
    claim_payout_data = pack_native_ckb_payout(claim_payout)
    payout_commitment_hash = packed_hash("NativeCkbPayoutV0", claim_payout_data)
    core = {
        "action": PATH_CLAIM_AFTER_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "terms_hash": active_cell["terms_hash"],
        "borrower_authority_hash": active_cell["borrower_authority_hash"],
        "lender_authority_hash": active_cell["lender_authority_hash"],
        "old_status": STATUS_ACTIVE,
        "new_status": STATUS_DEFAULTED,
        "old_nonce": active_cell["nonce"],
        "new_nonce": next_nonce,
        "terminal_amount": claim_amount,
        "payout_commitment_hash": payout_commitment_hash,
        "expiry_timepoint": active_cell["expiry_timepoint"],
    }
    core_data = pack_agreement_intent_core(core)
    intent_core_hash = packed_hash("NovaAgreementIntentCoreV0", core_data)
    receipt_commitment = {
        "action": PATH_CLAIM_AFTER_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "old_status": STATUS_ACTIVE,
        "new_status": STATUS_DEFAULTED,
        "terms_hash": active_cell["terms_hash"],
        "borrower_authority_hash": active_cell["borrower_authority_hash"],
        "lender_authority_hash": active_cell["lender_authority_hash"],
        "terminal_amount": claim_amount,
        "old_nonce": active_cell["nonce"],
        "new_nonce": next_nonce,
        "intent_core_hash": intent_core_hash,
        "payout_commitment_hash": payout_commitment_hash,
    }
    materialized_receipt_hash = packed_hash(
        "NovaAgreementReceiptCommitmentV0",
        pack_agreement_receipt_commitment(receipt_commitment),
    )
    signed_intent = pack_agreement_signed_intent(core_data, materialized_receipt_hash)
    signed_intent_hash = packed_hash("NovaAgreementSignedIntentV0", signed_intent)
    closed_cell = dict(active_cell)
    closed_cell.update({"status": STATUS_DEFAULTED, "latest_receipt_hash": materialized_receipt_hash, "nonce": next_nonce})
    closed_data = pack_agreement_cell(closed_cell)
    receipt = {
        "action": PATH_CLAIM_AFTER_EXPIRY,
        "agreement_id": active_cell["agreement_id"],
        "old_status": STATUS_ACTIVE,
        "new_status": STATUS_DEFAULTED,
        "terms_hash": active_cell["terms_hash"],
        "borrower_authority_hash": active_cell["borrower_authority_hash"],
        "lender_authority_hash": active_cell["lender_authority_hash"],
        "collateral_amount": active_cell["collateral_amount"],
        "principal_amount": active_cell["principal_amount"],
        "fixed_fee_amount": active_cell["fixed_fee_amount"],
        "terminal_amount": claim_amount,
        "previous_receipt_hash": previous_receipt_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "intent_core_hash": intent_core_hash,
        "signed_intent_hash": signed_intent_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "nonce": next_nonce,
        "timepoint": now,
    }
    receipt_data = pack_agreement_receipt(receipt)
    borrower_sig = signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND)
    lender_sig = bytearray(signature_payload(LENDER_SECRET_KEY, signed_intent_hash, LENDER_AUX_RAND))
    if mutate_lender_signature:
        lender_sig[-1] ^= 1
    return {
        "terms_data": pack_agreement_terms(terms),
        "active_data": pack_agreement_cell(active_cell),
        "closed_cell": closed_cell,
        "closed_data": closed_data,
        "claim_payout": claim_payout,
        "claim_payout_data": claim_payout_data,
        "receipt_data": receipt_data,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "intent_core_hash": intent_core_hash,
        "latest_receipt_hash": materialized_receipt_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "borrower_sig": borrower_sig,
        "lender_sig": bytes(lender_sig),
        "claim_amount": claim_amount,
    }


def compile_agreement_lifecycle(repo_root: pathlib.Path, output: pathlib.Path) -> None:
    cmd = [
        "cargo",
        "run",
        "--quiet",
        "--",
        "proposals/novaseal/agreement-profile-v0/src/nova_agreement_lifecycle_type.cell",
        "--target-profile",
        "ckb",
        "--target",
        "riscv64-elf",
        "--entry-action",
        "nova_agreement_lifecycle",
        "-o",
        str(output),
    ]
    subprocess.run(cmd, cwd=repo_root, check=True)


def lifecycle_type(lifecycle_data_hash: str) -> dict[str, str]:
    return {"code_hash": lifecycle_data_hash, "hash_type": "data2", "args": "0x"}


def build_origin_tx(
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    terms: dict[str, Any],
    material: dict[str, Any],
) -> dict[str, Any]:
    principal_payout_capacity = LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + terms["principal_amount"]
    change_capacity = funding["total_capacity"] - STATE_CAPACITY - principal_payout_capacity - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("originate funding capacity is too small")
    witness = entry_witness(
        PATH_ORIGINATE,
        material["terms_data"],
        material["active_data"],
        material["signed_intent"],
        material["borrower_sig"],
        material["lender_sig"],
    )
    return transaction(
        funding,
        [
            {"capacity": hex(STATE_CAPACITY), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {
                "capacity": hex(principal_payout_capacity),
                "lock": always_success_lock(hex0x(terms["borrower_authority_hash"])),
                "type": None,
            },
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [hex0x(material["active_data"]), hex0x(material["payout_data"]), hex0x(material["receipt_data"]), "0x"],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"][1:]],
        [header_hash],
    )


def build_repay_tx(
    *,
    active_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    terms: dict[str, Any],
    material: dict[str, Any],
    repayment_capacity_delta: int = 0,
    repayment_lock_args_override: bytes | None = None,
    lender_payout_data_override: bytes | None = None,
) -> dict[str, Any]:
    repayment_payout_capacity = LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + material["repayment_amount"] + repayment_capacity_delta
    collateral_return_capacity = LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + terms["collateral_amount"]
    change_capacity = (
        funding["total_capacity"]
        + active_ref["capacity"]
        - active_ref["capacity"]
        - repayment_payout_capacity
        - collateral_return_capacity
        - RECEIPT_CAPACITY
    )
    if change_capacity <= 0:
        raise LiveAcceptanceError("repay funding capacity is too small")
    witness = entry_witness(
        PATH_REPAY_BEFORE_EXPIRY,
        material["terms_data"],
        material["active_data"],
        material["signed_intent"],
        material["borrower_sig"],
        material["lender_sig"],
    )
    return transaction(
        [active_ref] + funding["cells"],
        [
            {"capacity": hex(active_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {
                "capacity": hex(repayment_payout_capacity),
                "lock": always_success_lock(hex0x(repayment_lock_args_override or terms["lender_authority_hash"])),
                "type": None,
            },
            {
                "capacity": hex(collateral_return_capacity),
                "lock": always_success_lock(hex0x(terms["borrower_authority_hash"])),
                "type": None,
            },
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [
            hex0x(material["closed_data"]),
            hex0x(lender_payout_data_override or material["lender_payout_data"]),
            hex0x(material["borrower_payout_data"]),
            hex0x(material["receipt_data"]),
            "0x",
        ],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def build_claim_tx(
    *,
    active_ref: dict[str, Any],
    funding: dict[str, Any],
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    header_hash: str,
    terms: dict[str, Any],
    material: dict[str, Any],
    claim_capacity_delta: int = 0,
    claim_lock_args_override: bytes | None = None,
    claim_payout_data_override: bytes | None = None,
) -> dict[str, Any]:
    claim_payout_capacity = LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + material["claim_amount"] + claim_capacity_delta
    change_capacity = funding["total_capacity"] - claim_payout_capacity - RECEIPT_CAPACITY
    if change_capacity <= 0:
        raise LiveAcceptanceError("claim funding capacity is too small")
    witness = entry_witness(
        PATH_CLAIM_AFTER_EXPIRY,
        material["terms_data"],
        material["active_data"],
        material["signed_intent"],
        material["borrower_sig"],
        material["lender_sig"],
    )
    return transaction(
        [active_ref] + funding["cells"],
        [
            {"capacity": hex(active_ref["capacity"]), "lock": always_success_lock(), "type": lifecycle_type(lifecycle_data_hash)},
            {
                "capacity": hex(claim_payout_capacity),
                "lock": always_success_lock(hex0x(claim_lock_args_override or terms["lender_authority_hash"])),
                "type": None,
            },
            {"capacity": hex(RECEIPT_CAPACITY), "lock": always_success_lock(), "type": None},
            {"capacity": hex(change_capacity), "lock": always_success_lock(), "type": None},
        ],
        [
            hex0x(material["closed_data"]),
            hex0x(claim_payout_data_override or material["claim_payout_data"]),
            hex0x(material["receipt_data"]),
            "0x",
        ],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


def wait_epoch_after(devnet: CkbDevnet, expiry_timepoint: int, *, max_blocks: int = 3000) -> dict[str, Any]:
    for _ in range(max_blocks):
        header = devnet.rpc("get_tip_header")
        if epoch_number_from_header(header) > expiry_timepoint:
            return header
        devnet.rpc("generate_block")
    raise LiveAcceptanceError(f"devnet epoch did not advance past expiry {expiry_timepoint}")


def submit_origin(
    devnet: CkbDevnet,
    *,
    lifecycle_data_hash: str,
    cell_deps: list[dict[str, Any]],
    terms: dict[str, Any],
    label: str,
) -> dict[str, Any]:
    header = devnet.rpc("get_tip_header")
    now = epoch_number_from_header(header)
    material = build_origin_material(terms, now)
    required = STATE_CAPACITY + RECEIPT_CAPACITY + LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + terms["principal_amount"]
    funding = devnet.collect_spendable(required + 100 * SHANNONS)
    tx = build_origin_tx(
        funding,
        lifecycle_data_hash,
        cell_deps,
        header["hash"],
        terms,
        material,
    )
    dry_run = devnet.rpc("dry_run_transaction", [tx])
    commit = devnet.submit_and_commit(tx, label)
    active_live = devnet.wait_live_cell(commit["tx_hash"], 0)
    principal_payout_live = devnet.wait_live_cell(commit["tx_hash"], 1)
    receipt_live = devnet.wait_live_cell(commit["tx_hash"], 2)
    return {
        "header": header,
        "timepoint": now,
        "material": material,
        "active_ref": {"tx_hash": commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY},
        "dry_run": dry_run,
        "commit": commit,
        "active_live": active_live,
        "principal_payout_live": principal_payout_live,
        "receipt_live": receipt_live,
    }


def run_live(args: argparse.Namespace) -> dict[str, Any]:
    repo_root = args.repo_root.resolve()
    ckb_repo = args.ckb_repo.resolve()
    ckb_bin = resolve_ckb_bin(ckb_repo, args.ckb_bin)
    run_dir = (args.run_dir or (repo_root / "target/novaseal-agreement-devnet-stateful-live" / str(int(time.time())))).resolve()
    run_dir.mkdir(parents=True, exist_ok=True)
    lifecycle_elf = run_dir / "nova-agreement-lifecycle-type.elf"
    compile_agreement_lifecycle(repo_root, lifecycle_elf)
    verifier_elf = repo_root / "proposals/novaseal/v0-mvp-skeleton/target/novaseal-btc-verifier-riscv-shell-release.elf"
    if not verifier_elf.is_file():
        raise LiveAcceptanceError(f"missing verifier ELF: {verifier_elf}")

    devnet = CkbDevnet(ckb_repo, ckb_bin, run_dir)
    report: dict[str, Any] = {
        "schema": "novaseal-agreement-devnet-stateful-live-v0.1",
        "status": "running",
        "scenario": "agreement_profile_originate_repay_and_claim",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
    }
    stage = "initializing"
    try:
        stage = "start devnet"
        devnet.start()
        stage = "deploy artifacts"
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_agreement_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]

        stage = "negative originate wrong lender signature"
        negative_origin_header = devnet.rpc("get_tip_header")
        negative_origin_now = epoch_number_from_header(negative_origin_header)
        wrong_lender_terms = make_terms(negative_origin_now, "wrong-lender-signature")
        wrong_lender_origin_material = build_origin_material(
            wrong_lender_terms,
            negative_origin_now,
            mutate_lender_signature=True,
        )
        wrong_lender_origin_required = (
            STATE_CAPACITY
            + RECEIPT_CAPACITY
            + LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE
            + wrong_lender_terms["principal_amount"]
        )
        wrong_lender_origin_funding = devnet.collect_spendable(wrong_lender_origin_required + 100 * SHANNONS)
        wrong_lender_origin_tx = build_origin_tx(
            wrong_lender_origin_funding,
            lifecycle["data_hash"],
            cell_deps,
            negative_origin_header["hash"],
            wrong_lender_terms,
            wrong_lender_origin_material,
        )
        wrong_lender_origin_reject = devnet.dry_run_rejects(wrong_lender_origin_tx, "wrong lender signature originate")

        stage = "negative originate non-CKB asset kind"
        non_ckb_terms = make_terms(negative_origin_now, "non-ckb-asset-kind")
        non_ckb_terms["principal_asset_kind"] = 1
        non_ckb_origin_material = build_origin_material(non_ckb_terms, negative_origin_now)
        non_ckb_origin_funding = devnet.collect_spendable(wrong_lender_origin_required + 100 * SHANNONS)
        non_ckb_origin_tx = build_origin_tx(
            non_ckb_origin_funding,
            lifecycle["data_hash"],
            cell_deps,
            negative_origin_header["hash"],
            non_ckb_terms,
            non_ckb_origin_material,
        )
        non_ckb_asset_kind_reject = devnet.dry_run_rejects(non_ckb_origin_tx, "non-CKB asset kind originate")

        stage = "valid repay-path originate"
        repay_seed_header = devnet.rpc("get_tip_header")
        repay_terms = make_terms(epoch_number_from_header(repay_seed_header), "repay")
        repay_origin = submit_origin(
            devnet,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            terms=repay_terms,
            label="agreement repay-path originate",
        )
        origin_material = repay_origin["material"]
        active_ref = repay_origin["active_ref"]
        stage = "negative repay wrong borrower signature"
        negative_header = devnet.rpc("get_tip_header")
        negative_now = epoch_number_from_header(negative_header)
        negative_material = build_repay_material(
            repay_terms,
            origin_material["active_cell"],
            origin_material["latest_receipt_hash"],
            negative_now,
            mutate_borrower_signature=True,
        )
        repay_required = (
            RECEIPT_CAPACITY
            + LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE
            + negative_material["repayment_amount"]
            + LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE
            + repay_terms["collateral_amount"]
        )
        negative_funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)
        negative_tx = build_repay_tx(
            active_ref=active_ref,
            funding=negative_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            terms=repay_terms,
            material=negative_material,
        )
        wrong_borrower_signature_reject = devnet.dry_run_rejects(negative_tx, "wrong borrower signature repay")

        stage = "negative repay payout capacity short"
        repay_capacity_material = build_repay_material(
            repay_terms,
            origin_material["active_cell"],
            origin_material["latest_receipt_hash"],
            negative_now,
        )
        repay_capacity_funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)
        repay_capacity_short_tx = build_repay_tx(
            active_ref=active_ref,
            funding=repay_capacity_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            terms=repay_terms,
            material=repay_capacity_material,
            repayment_capacity_delta=-1,
        )
        repay_payout_capacity_short_reject = devnet.dry_run_rejects(
            repay_capacity_short_tx,
            "repay payout capacity short",
        )

        stage = "negative repay payout lock args mismatch"
        repay_lock_funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)
        repay_lock_args_mismatch_tx = build_repay_tx(
            active_ref=active_ref,
            funding=repay_lock_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            terms=repay_terms,
            material=repay_capacity_material,
            repayment_lock_args_override=ckb_hash(b"wrong lender payout lock args"),
        )
        repay_payout_lock_args_mismatch_reject = devnet.dry_run_rejects(
            repay_lock_args_mismatch_tx,
            "repay payout lock args mismatch",
        )

        stage = "negative repay wrong payout amount"
        wrong_lender_payout = dict(repay_capacity_material["lender_payout"])
        wrong_lender_payout["amount"] += 1
        repay_wrong_payout_funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)
        repay_wrong_payout_amount_tx = build_repay_tx(
            active_ref=active_ref,
            funding=repay_wrong_payout_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            terms=repay_terms,
            material=repay_capacity_material,
            lender_payout_data_override=pack_native_ckb_payout(wrong_lender_payout),
        )
        repay_wrong_payout_amount_reject = devnet.dry_run_rejects(
            repay_wrong_payout_amount_tx,
            "repay wrong payout amount",
        )
        active_still_live = devnet.wait_live_cell(active_ref["tx_hash"], active_ref["index"])

        stage = "valid repay"
        repay_header = devnet.rpc("get_tip_header")
        repay_now = epoch_number_from_header(repay_header)
        repay_material = build_repay_material(repay_terms, origin_material["active_cell"], origin_material["latest_receipt_hash"], repay_now)
        repay_funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)
        repay_tx = build_repay_tx(
            active_ref=active_ref,
            funding=repay_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=repay_header["hash"],
            terms=repay_terms,
            material=repay_material,
        )
        repay_dry_run = devnet.rpc("dry_run_transaction", [repay_tx])
        repay_commit = devnet.submit_and_commit(repay_tx, "agreement repay before expiry")
        active_dead = devnet.wait_dead_cell(active_ref["tx_hash"], active_ref["index"])
        closed_live = devnet.wait_live_cell(repay_commit["tx_hash"], 0)
        lender_repayment_live = devnet.wait_live_cell(repay_commit["tx_hash"], 1)
        borrower_collateral_return_live = devnet.wait_live_cell(repay_commit["tx_hash"], 2)
        repay_receipt_live = devnet.wait_live_cell(repay_commit["tx_hash"], 3)

        stage = "valid claim-path originate"
        claim_seed_header = devnet.rpc("get_tip_header")
        claim_seed_now = epoch_number_from_header(claim_seed_header)
        claim_terms = make_terms(claim_seed_now, "claim", expiry_timepoint=claim_seed_now + 1)
        claim_origin = submit_origin(
            devnet,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            terms=claim_terms,
            label="agreement claim-path originate",
        )
        claim_origin_material = claim_origin["material"]
        claim_active_ref = claim_origin["active_ref"]
        stage = "negative early claim"
        early_claim_header = devnet.rpc("get_tip_header")
        early_claim_now = epoch_number_from_header(early_claim_header)
        early_claim_material = build_claim_material(
            claim_terms,
            claim_origin_material["active_cell"],
            claim_origin_material["latest_receipt_hash"],
            early_claim_now,
        )
        claim_required = RECEIPT_CAPACITY + LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + early_claim_material["claim_amount"]
        early_claim_funding = devnet.collect_spendable(claim_required + 100 * SHANNONS)
        early_claim_tx = build_claim_tx(
            active_ref=claim_active_ref,
            funding=early_claim_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=early_claim_header["hash"],
            terms=claim_terms,
            material=early_claim_material,
        )
        early_claim_reject = devnet.dry_run_rejects(early_claim_tx, "early claim before expiry")

        stage = "wait claim expiry"
        claim_header = wait_epoch_after(devnet, claim_terms["expiry_timepoint"])
        claim_now = epoch_number_from_header(claim_header)
        stage = "negative claim wrong lender signature"
        wrong_lender_claim_material = build_claim_material(
            claim_terms,
            claim_origin_material["active_cell"],
            claim_origin_material["latest_receipt_hash"],
            claim_now,
            mutate_lender_signature=True,
        )
        wrong_lender_claim_funding = devnet.collect_spendable(claim_required + 100 * SHANNONS)
        wrong_lender_claim_tx = build_claim_tx(
            active_ref=claim_active_ref,
            funding=wrong_lender_claim_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=claim_header["hash"],
            terms=claim_terms,
            material=wrong_lender_claim_material,
        )
        wrong_lender_claim_reject = devnet.dry_run_rejects(wrong_lender_claim_tx, "wrong lender signature claim")
        claim_active_still_live = devnet.wait_live_cell(claim_active_ref["tx_hash"], claim_active_ref["index"])

        stage = "valid claim"
        claim_material = build_claim_material(
            claim_terms,
            claim_origin_material["active_cell"],
            claim_origin_material["latest_receipt_hash"],
            claim_now,
        )
        claim_funding = devnet.collect_spendable(claim_required + 100 * SHANNONS)
        claim_tx = build_claim_tx(
            active_ref=claim_active_ref,
            funding=claim_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=claim_header["hash"],
            terms=claim_terms,
            material=claim_material,
        )
        claim_dry_run = devnet.rpc("dry_run_transaction", [claim_tx])
        claim_commit = devnet.submit_and_commit(claim_tx, "agreement claim after expiry")
        claim_active_dead = devnet.wait_dead_cell(claim_active_ref["tx_hash"], claim_active_ref["index"])
        claim_closed_live = devnet.wait_live_cell(claim_commit["tx_hash"], 0)
        lender_default_claim_live = devnet.wait_live_cell(claim_commit["tx_hash"], 1)
        claim_receipt_live = devnet.wait_live_cell(claim_commit["tx_hash"], 2)

        report.update(
            {
                "status": "passed",
                "live_devnet_rpc_executed": True,
                "stateful_lifecycle_executed": True,
                "ckb_log": str(devnet.log_path),
                "rpc_url": devnet.rpc_url,
                "artifacts": {
                    "verifier": verifier,
                    "lifecycle": lifecycle,
                },
                "repay_terms": {
                    "agreement_id": hex0x(repay_terms["agreement_id"]),
                    "terms_hash": hex0x(repay_terms["terms_hash"]),
                    "borrower_authority_hash": hex0x(repay_terms["borrower_authority_hash"]),
                    "lender_authority_hash": hex0x(repay_terms["lender_authority_hash"]),
                    "principal_amount": repay_terms["principal_amount"],
                    "collateral_amount": repay_terms["collateral_amount"],
                    "fixed_fee_amount": repay_terms["fixed_fee_amount"],
                    "expiry_timepoint": repay_terms["expiry_timepoint"],
                },
                "claim_terms": {
                    "agreement_id": hex0x(claim_terms["agreement_id"]),
                    "terms_hash": hex0x(claim_terms["terms_hash"]),
                    "borrower_authority_hash": hex0x(claim_terms["borrower_authority_hash"]),
                    "lender_authority_hash": hex0x(claim_terms["lender_authority_hash"]),
                    "principal_amount": claim_terms["principal_amount"],
                    "collateral_amount": claim_terms["collateral_amount"],
                    "fixed_fee_amount": claim_terms["fixed_fee_amount"],
                    "expiry_timepoint": claim_terms["expiry_timepoint"],
                },
                "originate": {
                    "dry_run_cycles": repay_origin["dry_run"].get("cycles"),
                    "commit": repay_origin["commit"],
                    "active_live": repay_origin["active_live"].get("status") == "live",
                    "principal_payout_live": repay_origin["principal_payout_live"].get("status") == "live",
                    "receipt_live": repay_origin["receipt_live"].get("status") == "live",
                    "active_data_hash": hex0x(cell_data_hash(origin_material["active_data"])),
                    "principal_payout_data_hash": ckb_hash_hex(origin_material["payout_data"]),
                    "signed_intent_hash": hex0x(origin_material["signed_intent_hash"]),
                    "latest_receipt_hash": hex0x(origin_material["latest_receipt_hash"]),
                },
                "repay": {
                    "dry_run_cycles": repay_dry_run.get("cycles"),
                    "commit": repay_commit,
                    "old_active_not_live": active_dead.get("status") != "live",
                    "closed_live": closed_live.get("status") == "live",
                    "lender_repayment_live": lender_repayment_live.get("status") == "live",
                    "borrower_collateral_return_live": borrower_collateral_return_live.get("status") == "live",
                    "receipt_live": repay_receipt_live.get("status") == "live",
                    "closed_data_hash": hex0x(cell_data_hash(repay_material["closed_data"])),
                    "lender_payout_data_hash": ckb_hash_hex(repay_material["lender_payout_data"]),
                    "borrower_payout_data_hash": ckb_hash_hex(repay_material["borrower_payout_data"]),
                    "signed_intent_hash": hex0x(repay_material["signed_intent_hash"]),
                    "latest_receipt_hash": hex0x(repay_material["latest_receipt_hash"]),
                },
                "claim_originate": {
                    "dry_run_cycles": claim_origin["dry_run"].get("cycles"),
                    "commit": claim_origin["commit"],
                    "active_live": claim_origin["active_live"].get("status") == "live",
                    "principal_payout_live": claim_origin["principal_payout_live"].get("status") == "live",
                    "receipt_live": claim_origin["receipt_live"].get("status") == "live",
                    "latest_receipt_hash": hex0x(claim_origin_material["latest_receipt_hash"]),
                },
                "claim": {
                    "dry_run_cycles": claim_dry_run.get("cycles"),
                    "commit": claim_commit,
                    "old_active_not_live": claim_active_dead.get("status") != "live",
                    "closed_live": claim_closed_live.get("status") == "live",
                    "lender_default_claim_live": lender_default_claim_live.get("status") == "live",
                    "receipt_live": claim_receipt_live.get("status") == "live",
                    "closed_data_hash": hex0x(cell_data_hash(claim_material["closed_data"])),
                    "claim_payout_data_hash": ckb_hash_hex(claim_material["claim_payout_data"]),
                    "signed_intent_hash": hex0x(claim_material["signed_intent_hash"]),
                    "latest_receipt_hash": hex0x(claim_material["latest_receipt_hash"]),
                    "timepoint": claim_now,
                },
                "negative_cases": {
                    "wrong_lender_signature_dry_run": wrong_lender_origin_reject,
                    "non_ckb_asset_kind_dry_run": non_ckb_asset_kind_reject,
                    "wrong_borrower_signature_dry_run": wrong_borrower_signature_reject,
                    "repay_payout_capacity_short_dry_run": repay_payout_capacity_short_reject,
                    "repay_payout_lock_args_mismatch_dry_run": repay_payout_lock_args_mismatch_reject,
                    "repay_wrong_payout_amount_dry_run": repay_wrong_payout_amount_reject,
                    "early_claim_dry_run": early_claim_reject,
                    "wrong_lender_claim_signature_dry_run": wrong_lender_claim_reject,
                    "post_negative_active_still_live": active_still_live.get("status") == "live",
                    "post_claim_negative_active_still_live": claim_active_still_live.get("status") == "live",
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


def main() -> int:
    args = parse_args()
    report = run_live(args)
    output = args.output if args.output.is_absolute() else args.repo_root.resolve() / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2 if args.pretty else None, sort_keys=True) + "\n", encoding="utf-8")
    print(
        f"wrote {output} status={report['status']} "
        f"live_devnet_rpc_executed={report.get('live_devnet_rpc_executed', False)}"
    )
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
