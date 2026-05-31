#!/usr/bin/env python3
"""Generate NovaSeal wallet signing vectors.

The output is a wallet-facing companion to the packed canonical vectors. It
freezes the exact 32-byte BIP340 message, the typed preimage, and the
fixed-width Molecule-equivalent byte layout a wallet must display/sign.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
CORE_ROOT = ROOT / "proposals/novaseal/v0-mvp-skeleton"
AGREEMENT_ROOT = ROOT / "proposals/novaseal/agreement-profile-v0"
DEFAULT_CORE_VECTORS = CORE_ROOT / "target/novaseal-canonical-vectors.json"
DEFAULT_OUTPUT = ROOT / "target/novaseal-wallet-signing-vectors.json"

PACKED_HASH_DOMAIN = b"CellScriptPackedHashV0\x00"
CKB_HASH_PERSONAL = b"ckb-default-hash"
VECTOR_PERSON = b"NovaSealWalletV0"
ZERO_HASH = "0x" + "00" * 32

CKB = 100_000_000
BORROWER_AUTHORITY = "0x" + "11" * 32
LENDER_AUTHORITY = "0x" + "22" * 32
COLLATERAL_AMOUNT = 1_000 * CKB
PRINCIPAL_AMOUNT = 700 * CKB
FIXED_FEE_AMOUNT = 30 * CKB
EXPIRY_TIMEPOINT = 200


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def ckb_blake2b256(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=32, person=CKB_HASH_PERSONAL).digest()


def stable_hash(label: str, value: Any) -> str:
    h = hashlib.blake2b(digest_size=32, person=VECTOR_PERSON)
    h.update(label.encode("utf-8"))
    h.update(b"\x00")
    h.update(str(value).encode("utf-8"))
    return hex0x(h.digest())


def as_bytes32(value: str) -> bytes:
    raw = value[2:] if value.startswith("0x") else value
    data = bytes.fromhex(raw)
    if len(data) != 32:
        raise ValueError(f"expected Byte32, got {len(data)} bytes")
    return data


def uint(value: int, size: int) -> bytes:
    if value < 0 or value >= 1 << (size * 8):
        raise ValueError(f"{value} does not fit u{size * 8}")
    return value.to_bytes(size, "little")


def packed_hash_preimage(type_name: str, packed_bytes: bytes) -> bytes:
    return PACKED_HASH_DOMAIN + type_name.encode("utf-8") + b"\x00" + len(packed_bytes).to_bytes(4, "little") + packed_bytes


def packed_hash(type_name: str, packed_bytes: bytes) -> tuple[str, str]:
    preimage = packed_hash_preimage(type_name, packed_bytes)
    return hex0x(preimage), hex0x(ckb_blake2b256(preimage))


def field_map(encoded: dict[str, Any]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for field in encoded.get("fields", []):
        if "value" in field:
            result[field["name"]] = field["value"]
        elif field.get("type") in {"Byte32", "Hash"}:
            result[field["name"]] = field["hex"]
        elif field.get("type") == "OutPoint":
            components = {component["name"]: component for component in field.get("components", [])}
            result[field["name"]] = {
                "tx_hash": components.get("tx_hash", {}).get("hex"),
                "index": components.get("index", {}).get("value"),
            }
        elif "nested" in field:
            result[field["name"]] = field_map(field["nested"])
    return result


def wallet_record(
    *,
    suite: str,
    name: str,
    action: str,
    signers: list[str],
    signed_intent: dict[str, Any],
    display: dict[str, Any],
    expected_receipt_hash: str,
) -> dict[str, Any]:
    preimage = signed_intent["hash_preimage_hex"]
    message = signed_intent["digest_blake2b_256"]
    recomputed = hex0x(ckb_blake2b256(bytes.fromhex(preimage[2:])))
    status = "passed" if recomputed == message else "failed"
    return {
        "suite": suite,
        "name": name,
        "action": action,
        "signers": signers,
        "status": status,
        "bip340_message_hash": message,
        "signed_type": signed_intent["type"],
        "signed_intent_packed_hex": signed_intent["hex"],
        "signed_intent_hash_preimage_hex": preimage,
        "molecule_fixed_equivalent_hex": signed_intent["hex"],
        "molecule_profile": "fixed-width CellScript schema; equivalent to declared-field concatenation for these v0 structs",
        "expected_receipt_hash": expected_receipt_hash,
        "wallet_display": display,
    }


def core_vectors(path: Path) -> list[dict[str, Any]]:
    payload = json.loads(path.read_text(encoding="utf-8"))
    vectors: list[dict[str, Any]] = []
    for vector in payload.get("vectors", []):
        encoded = vector.get("encoded", {})
        resolved = encoded.get("resolved")
        if not isinstance(resolved, dict):
            continue
        signed_intent = resolved.get("signed_intent")
        if not isinstance(signed_intent, dict):
            continue
        core = field_map(signed_intent["fields"][0]["nested"])
        display = {
            "protocol": "NovaSeal Core v0",
            "fixture": vector.get("fixture"),
            "action": core.get("action"),
            "terminal_path": core.get("terminal_path"),
            "old_cell": core.get("old_cell"),
            "old_state_hash": core.get("old_state_hash"),
            "new_state_hash": core.get("new_state_hash"),
            "old_nonce": core.get("old_nonce"),
            "new_nonce": core.get("new_nonce"),
            "expiry": core.get("expiry"),
            "policy_hash": core.get("policy_hash"),
        }
        vectors.append(
            wallet_record(
                suite="novaseal-core-v0",
                name=str(vector.get("name") or vector.get("fixture")),
                action="key_auth_transition",
                signers=["btc_authority"],
                signed_intent=signed_intent,
                display=display,
                expected_receipt_hash=field_map(signed_intent).get("expected_receipt_hash"),
            )
        )
    return vectors


def encode_native_payout(action: int, role: int, recipient: str, amount: int, terms_hash: str, agreement_id: str, nonce: int) -> dict[str, Any]:
    packed = b"".join(
        [
            uint(action, 1),
            as_bytes32(agreement_id),
            uint(role, 1),
            as_bytes32(recipient),
            uint(0, 1),
            as_bytes32(ZERO_HASH),
            uint(amount, 8),
            as_bytes32(terms_hash),
            uint(nonce, 8),
        ]
    )
    preimage, digest = packed_hash("NativeCkbPayoutV0", packed)
    return {"type": "NativeCkbPayoutV0", "hex": hex0x(packed), "hash_preimage_hex": preimage, "digest_blake2b_256": digest}


def encode_agreement_intent_core(
    action: int,
    agreement_id: str,
    terms_hash: str,
    old_status: int,
    new_status: int,
    old_nonce: int,
    new_nonce: int,
    terminal_amount: int,
    payout_commitment_hash: str,
) -> dict[str, Any]:
    packed = b"".join(
        [
            uint(action, 1),
            as_bytes32(agreement_id),
            as_bytes32(terms_hash),
            as_bytes32(BORROWER_AUTHORITY),
            as_bytes32(LENDER_AUTHORITY),
            uint(old_status, 1),
            uint(new_status, 1),
            uint(old_nonce, 8),
            uint(new_nonce, 8),
            uint(terminal_amount, 8),
            as_bytes32(payout_commitment_hash),
            uint(EXPIRY_TIMEPOINT, 8),
        ]
    )
    preimage, digest = packed_hash("NovaAgreementIntentCoreV0", packed)
    return {"type": "NovaAgreementIntentCoreV0", "hex": hex0x(packed), "hash_preimage_hex": preimage, "digest_blake2b_256": digest}


def encode_agreement_receipt_commitment(
    action: int,
    agreement_id: str,
    terms_hash: str,
    old_status: int,
    new_status: int,
    terminal_amount: int,
    old_nonce: int,
    new_nonce: int,
    intent_core_hash: str,
    payout_commitment_hash: str,
) -> dict[str, Any]:
    packed = b"".join(
        [
            uint(action, 1),
            as_bytes32(agreement_id),
            uint(old_status, 1),
            uint(new_status, 1),
            as_bytes32(terms_hash),
            as_bytes32(BORROWER_AUTHORITY),
            as_bytes32(LENDER_AUTHORITY),
            uint(terminal_amount, 8),
            uint(old_nonce, 8),
            uint(new_nonce, 8),
            as_bytes32(intent_core_hash),
            as_bytes32(payout_commitment_hash),
        ]
    )
    preimage, digest = packed_hash("NovaAgreementReceiptCommitmentV0", packed)
    return {"type": "NovaAgreementReceiptCommitmentV0", "hex": hex0x(packed), "hash_preimage_hex": preimage, "digest_blake2b_256": digest}


def encode_agreement_signed_intent(core: dict[str, Any], expected_receipt_hash: str) -> dict[str, Any]:
    packed = bytes.fromhex(core["hex"][2:]) + as_bytes32(expected_receipt_hash)
    preimage, digest = packed_hash("NovaAgreementSignedIntentV0", packed)
    return {"type": "NovaAgreementSignedIntentV0", "hex": hex0x(packed), "hash_preimage_hex": preimage, "digest_blake2b_256": digest}


def agreement_case(name: str, action: int, old_status: int, new_status: int, old_nonce: int, new_nonce: int, terminal_amount: int, signers: list[str]) -> dict[str, Any]:
    agreement_id = stable_hash("agreement_id", "mvb-starter-v0")
    terms_hash = stable_hash("terms_hash", "ckb-ckb-fixed-fee-v0")
    if action == 0:
        payout_hash = encode_native_payout(action, 0, BORROWER_AUTHORITY, PRINCIPAL_AMOUNT, terms_hash, agreement_id, 0)[
            "digest_blake2b_256"
        ]
    elif action == 1:
        lender = encode_native_payout(action, 1, LENDER_AUTHORITY, PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT, terms_hash, agreement_id, 1)
        borrower = encode_native_payout(action, 2, BORROWER_AUTHORITY, COLLATERAL_AMOUNT, terms_hash, agreement_id, 1)
        packed = as_bytes32(lender["digest_blake2b_256"]) + as_bytes32(borrower["digest_blake2b_256"])
        _, payout_hash = packed_hash("RepayPayoutCommitmentV0", packed)
    else:
        payout_hash = encode_native_payout(action, 3, LENDER_AUTHORITY, COLLATERAL_AMOUNT, terms_hash, agreement_id, 1)[
            "digest_blake2b_256"
        ]
    core = encode_agreement_intent_core(
        action, agreement_id, terms_hash, old_status, new_status, old_nonce, new_nonce, terminal_amount, payout_hash
    )
    receipt = encode_agreement_receipt_commitment(
        action, agreement_id, terms_hash, old_status, new_status, terminal_amount, old_nonce, new_nonce, core["digest_blake2b_256"], payout_hash
    )
    signed = encode_agreement_signed_intent(core, receipt["digest_blake2b_256"])
    action_name = {0: "originate_agreement", 1: "repay_before_expiry", 2: "claim_after_expiry"}[action]
    return wallet_record(
        suite="novaseal-agreement-profile-v0",
        name=name,
        action=action_name,
        signers=signers,
        signed_intent=signed,
        expected_receipt_hash=receipt["digest_blake2b_256"],
        display={
            "protocol": "NovaSeal Agreement Profile v0",
            "action": action_name,
            "agreement_id": agreement_id,
            "terms_hash": terms_hash,
            "borrower_authority_hash": BORROWER_AUTHORITY,
            "lender_authority_hash": LENDER_AUTHORITY,
            "old_status": old_status,
            "new_status": new_status,
            "old_nonce": old_nonce,
            "new_nonce": new_nonce,
            "terminal_amount_shannons": terminal_amount,
            "payout_commitment_hash": payout_hash,
            "expiry_timepoint": EXPIRY_TIMEPOINT,
        },
    )


def agreement_vectors() -> list[dict[str, Any]]:
    return [
        agreement_case("originate_valid", 0, 0, 1, 0, 0, PRINCIPAL_AMOUNT, ["borrower", "lender"]),
        agreement_case("repay_before_expiry_valid", 1, 1, 2, 0, 1, PRINCIPAL_AMOUNT + FIXED_FEE_AMOUNT, ["borrower"]),
        agreement_case("claim_after_expiry_valid", 2, 1, 3, 0, 1, COLLATERAL_AMOUNT, ["lender"]),
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--core-vectors", type=Path, default=DEFAULT_CORE_VECTORS)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    vectors = core_vectors(args.core_vectors) + agreement_vectors()
    status = "passed" if vectors and all(vector["status"] == "passed" for vector in vectors) else "failed"
    payload = {
        "schema": "novaseal-wallet-signing-vectors-v0.1",
        "status": status,
        "hash_algorithm": "ckb_blake2b_256",
        "signature_scheme": "BIP340 Schnorr over 32-byte signed intent hash",
        "molecule_alignment": "fixed-width v0 structs use declared-field little-endian concatenation; no dynamic tables/vectors in these signing objects",
        "summary": {
            "total": len(vectors),
            "core_vectors": len([vector for vector in vectors if vector["suite"] == "novaseal-core-v0"]),
            "agreement_vectors": len([vector for vector in vectors if vector["suite"] == "novaseal-agreement-profile-v0"]),
            "matched": len([vector for vector in vectors if vector["status"] == "passed"]),
        },
        "vectors": vectors,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if args.pretty:
        print(
            f"wrote {args.output} status={payload['status']} total={payload['summary']['total']} "
            f"core={payload['summary']['core_vectors']} agreement={payload['summary']['agreement_vectors']}"
        )
    return 0 if status == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
