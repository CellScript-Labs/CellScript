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
PATH_ORIGINATE = 0
PATH_REPAY_BEFORE_EXPIRY = 1
PAYOUT_BORROWER_PRINCIPAL = 0
PAYOUT_LENDER_REPAYMENT = 1
PAYOUT_BORROWER_COLLATERAL_RETURN = 2
NATIVE_CKB_PAYOUT_OCCUPIED_CAPACITY = 4_000_000_000
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


def make_terms(now: int) -> dict[str, Any]:
    borrower = xonly_pubkey(TEST_SECRET_KEY)
    lender = xonly_pubkey(LENDER_SECRET_KEY)
    agreement_id = ckb_hash(b"NovaSeal Agreement live devnet v0")
    terms_hash = ckb_hash(b"NovaSeal Agreement live devnet terms v0")
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
        "expiry_timepoint": now + 1_000_000,
        "early_close_policy": EARLY_CLOSE_FIXED_FEE,
    }


def build_origin_material(terms: dict[str, Any], now: int) -> dict[str, Any]:
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
        "collateral_asset_kind": ASSET_KIND_CKB,
        "collateral_asset_hash": ZERO_HASH,
        "collateral_amount": terms["collateral_amount"],
        "principal_asset_kind": ASSET_KIND_CKB,
        "principal_asset_hash": ZERO_HASH,
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
    borrower_sig = signature_payload(TEST_SECRET_KEY, signed_intent_hash, TEST_AUX_RAND)
    lender_sig = signature_payload(LENDER_SECRET_KEY, signed_intent_hash, LENDER_AUX_RAND)
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
        "borrower_sig": borrower_sig,
        "lender_sig": lender_sig,
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
) -> dict[str, Any]:
    repayment_payout_capacity = LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + material["repayment_amount"]
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
                "lock": always_success_lock(hex0x(terms["lender_authority_hash"])),
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
            hex0x(material["lender_payout_data"]),
            hex0x(material["borrower_payout_data"]),
            hex0x(material["receipt_data"]),
            "0x",
        ],
        cell_deps,
        [witness] + ["0x" for _ in funding["cells"]],
        [header_hash],
    )


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
        "scenario": "agreement_profile_originate_then_repay",
        "repo_root": str(repo_root),
        "ckb_repo": str(ckb_repo),
        "ckb_bin": str(ckb_bin),
        "run_dir": str(run_dir),
    }
    try:
        devnet.start()
        genesis = devnet.get_block_by_number(0)
        always_dep = always_success_dep(genesis["transactions"][0]["hash"])
        verifier = deploy_code_cell(devnet, "cellscript_btc_bip340_verifier_riscv", verifier_elf.read_bytes(), always_dep)
        lifecycle = deploy_code_cell(devnet, "nova_agreement_lifecycle_type", lifecycle_elf.read_bytes(), always_dep)
        cell_deps = [verifier["cell_dep"], lifecycle["cell_dep"], always_dep]

        origin_header = devnet.rpc("get_tip_header")
        origin_now = epoch_number_from_header(origin_header)
        terms = make_terms(origin_now)
        origin_material = build_origin_material(terms, origin_now)
        origin_required = STATE_CAPACITY + RECEIPT_CAPACITY + LIVE_NATIVE_CKB_PAYOUT_CAPACITY_BASE + terms["principal_amount"]
        origin_funding = devnet.collect_spendable(origin_required + 100 * SHANNONS)
        origin_tx = build_origin_tx(
            origin_funding,
            lifecycle["data_hash"],
            cell_deps,
            origin_header["hash"],
            terms,
            origin_material,
        )
        origin_dry_run = devnet.rpc("dry_run_transaction", [origin_tx])
        origin_commit = devnet.submit_and_commit(origin_tx, "agreement originate")
        active_live = devnet.wait_live_cell(origin_commit["tx_hash"], 0)
        principal_payout_live = devnet.wait_live_cell(origin_commit["tx_hash"], 1)
        origin_receipt_live = devnet.wait_live_cell(origin_commit["tx_hash"], 2)

        active_ref = {"tx_hash": origin_commit["tx_hash"], "index": 0, "capacity": STATE_CAPACITY}
        negative_header = devnet.rpc("get_tip_header")
        negative_now = epoch_number_from_header(negative_header)
        negative_material = build_repay_material(
            terms,
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
            + terms["collateral_amount"]
        )
        negative_funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)
        negative_tx = build_repay_tx(
            active_ref=active_ref,
            funding=negative_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=negative_header["hash"],
            terms=terms,
            material=negative_material,
        )
        wrong_borrower_signature_reject = devnet.dry_run_rejects(negative_tx, "wrong borrower signature repay")
        active_still_live = devnet.wait_live_cell(origin_commit["tx_hash"], 0)

        repay_header = devnet.rpc("get_tip_header")
        repay_now = epoch_number_from_header(repay_header)
        repay_material = build_repay_material(terms, origin_material["active_cell"], origin_material["latest_receipt_hash"], repay_now)
        repay_funding = devnet.collect_spendable(repay_required + 100 * SHANNONS)
        repay_tx = build_repay_tx(
            active_ref=active_ref,
            funding=repay_funding,
            lifecycle_data_hash=lifecycle["data_hash"],
            cell_deps=cell_deps,
            header_hash=repay_header["hash"],
            terms=terms,
            material=repay_material,
        )
        repay_dry_run = devnet.rpc("dry_run_transaction", [repay_tx])
        repay_commit = devnet.submit_and_commit(repay_tx, "agreement repay before expiry")
        active_dead = devnet.wait_dead_cell(origin_commit["tx_hash"], 0)
        closed_live = devnet.wait_live_cell(repay_commit["tx_hash"], 0)
        lender_repayment_live = devnet.wait_live_cell(repay_commit["tx_hash"], 1)
        borrower_collateral_return_live = devnet.wait_live_cell(repay_commit["tx_hash"], 2)
        repay_receipt_live = devnet.wait_live_cell(repay_commit["tx_hash"], 3)

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
                "terms": {
                    "agreement_id": hex0x(terms["agreement_id"]),
                    "terms_hash": hex0x(terms["terms_hash"]),
                    "borrower_authority_hash": hex0x(terms["borrower_authority_hash"]),
                    "lender_authority_hash": hex0x(terms["lender_authority_hash"]),
                    "principal_amount": terms["principal_amount"],
                    "collateral_amount": terms["collateral_amount"],
                    "fixed_fee_amount": terms["fixed_fee_amount"],
                    "expiry_timepoint": terms["expiry_timepoint"],
                },
                "originate": {
                    "dry_run_cycles": origin_dry_run.get("cycles"),
                    "commit": origin_commit,
                    "active_live": active_live.get("status") == "live",
                    "principal_payout_live": principal_payout_live.get("status") == "live",
                    "receipt_live": origin_receipt_live.get("status") == "live",
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
                "negative_cases": {
                    "wrong_borrower_signature_dry_run": wrong_borrower_signature_reject,
                    "post_negative_active_still_live": active_still_live.get("status") == "live",
                },
            }
        )
        return report
    except Exception as error:
        report.update({"status": "failed", "error": str(error), "ckb_log": str(devnet.log_path), "rpc_url": devnet.rpc_url})
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
