#!/usr/bin/env python3
"""Compare canonical NovaSeal wallet messages with the current lock digest.

This is an alignment probe, not a production wallet encoder. It deliberately
keeps the current result fail-closed when the `.cell` lock signs a different
32-byte message from the canonical packed-reference vectors.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from novaseal_btc_verifier_vectors import bytes_from_hex, hex0x, positive_case, schnorr_verify
from novaseal_fixture_harness import load_json


SCHEMA = "novaseal-wallet-signing-alignment-v0.1"

DEFAULT_CANONICAL_VECTORS = Path("target/novaseal-canonical-vectors.json")
DEFAULT_SOURCE = Path("src/nova_state_type.cell")
DEFAULT_OUTPUT = Path("target/novaseal-wallet-signing-alignment.json")

REQUIRED_FIXTURE_COUNT = 6
INTENT_DOMAIN_OFFSET = 0
BYTE32_LEN = 32
CKB_BLAKE2B_PERSONAL = b"ckb-default-hash"

REQUIRED_LOCK_SNIPPETS = [
    "let digest = compute_intent_hash(&intent)",
    "hash_blake2b(intent.domain)",
    "pipe_write(write_fd, fixed_u64_le(digest, 0))",
    'spawn_with_fd("novaseal_btc_verifier_riscv", read_fd)',
]


def ckb_blake2b256(data: bytes) -> bytes:
    hasher = hashlib.blake2b(digest_size=32, person=CKB_BLAKE2B_PERSONAL)
    hasher.update(data)
    return hasher.digest()


def optional_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def source_digest_model(source_path: Path) -> dict[str, Any]:
    source = optional_text(source_path)
    missing = [snippet for snippet in REQUIRED_LOCK_SNIPPETS if snippet not in source]
    return {
        "source": str(source_path),
        "required_snippets": REQUIRED_LOCK_SNIPPETS,
        "missing_snippets": missing,
        "placeholder_domain_hash_visible": "hash_blake2b(intent.domain)" in source,
        "current_lock_digest": "ckb_blake2b_256(intent.domain)",
        "canonical_wallet_digest": "signed_intent_hash_after_resolved_receipt",
        "all_required_snippets_present": not missing,
    }


def resolved_intent_bytes(vector: dict[str, Any]) -> bytes:
    fixture = vector.get("fixture", "<unknown fixture>")
    encoded = vector.get("encoded", {})
    resolved = encoded.get("resolved", {})
    intent = resolved.get("resolved_intent", {})
    raw = intent.get("hex")
    if not isinstance(raw, str):
        raise SystemExit(f"{fixture}: missing encoded.resolved.resolved_intent.hex")
    data = bytes_from_hex(raw, 213)
    return data


def canonical_message(vector: dict[str, Any]) -> bytes:
    fixture = vector.get("fixture", "<unknown fixture>")
    hashes = vector.get("hashes", {})
    raw = hashes.get("signed_intent_hash_after_resolved_receipt")
    if not isinstance(raw, str):
        raise SystemExit(f"{fixture}: missing hashes.signed_intent_hash_after_resolved_receipt")
    return bytes_from_hex(raw, BYTE32_LEN)


def fixture_alignment(vector: dict[str, Any]) -> dict[str, Any]:
    fixture = vector["fixture"]
    intent = resolved_intent_bytes(vector)
    domain = intent[INTENT_DOMAIN_OFFSET : INTENT_DOMAIN_OFFSET + BYTE32_LEN]
    current_digest = ckb_blake2b256(domain)
    canonical_digest = canonical_message(vector)

    canonical_wallet_case = positive_case(fixture, canonical_digest, signer_index=0)
    current_lock_compat_case = positive_case(fixture, current_digest, signer_index=0)

    canonical_pubkey = bytes_from_hex(canonical_wallet_case["xonly_pubkey"], BYTE32_LEN)
    canonical_signature = bytes_from_hex(canonical_wallet_case["signature64"], 64)
    canonical_signature_accepts_current_lock_digest = schnorr_verify(current_digest, canonical_pubkey, canonical_signature)

    current_pubkey = bytes_from_hex(current_lock_compat_case["xonly_pubkey"], BYTE32_LEN)
    current_signature = bytes_from_hex(current_lock_compat_case["signature64"], 64)
    current_lock_signature_accepts_canonical_digest = schnorr_verify(canonical_digest, current_pubkey, current_signature)

    digests_match = canonical_digest == current_digest
    return {
        "fixture": fixture,
        "intent_encoding": "packed-fixed-v0-reference",
        "resolved_intent_size_bytes": len(intent),
        "canonical_wallet_message32": hex0x(canonical_digest),
        "current_lock_message32": hex0x(current_digest),
        "current_lock_message_rule": "ckb_blake2b_256(intent.domain)",
        "canonical_wallet_message_rule": "signed_intent_hash_after_resolved_receipt",
        "canonical_vs_current_lock_digest_match": digests_match,
        "canonical_wallet_positive": {
            "message32": canonical_wallet_case["message32"],
            "xonly_pubkey": canonical_wallet_case["xonly_pubkey"],
            "signature64": canonical_wallet_case["signature64"],
            "test_secret_key": canonical_wallet_case["test_secret_key"],
            "self_verified": canonical_wallet_case["self_verified"],
            "classification": "canonical_wallet_vector_test_only",
        },
        "current_lock_compat_positive": {
            "message32": current_lock_compat_case["message32"],
            "xonly_pubkey": current_lock_compat_case["xonly_pubkey"],
            "signature64": current_lock_compat_case["signature64"],
            "test_secret_key": current_lock_compat_case["test_secret_key"],
            "self_verified": current_lock_compat_case["self_verified"],
            "classification": "current_harness_compatibility_only",
        },
        "cross_check": {
            "canonical_signature_accepts_current_lock_digest": canonical_signature_accepts_current_lock_digest,
            "current_lock_signature_accepts_canonical_digest": current_lock_signature_accepts_canonical_digest,
        },
        "wallet_lock_alignment_ready": digests_match,
    }


def build_report(canonical_vectors_path: Path, source_path: Path) -> dict[str, Any]:
    canonical = load_json(canonical_vectors_path)
    vectors = canonical.get("vectors", [])
    if not isinstance(vectors, list):
        raise SystemExit(f"{canonical_vectors_path}: vectors must be an array")
    if len(vectors) != REQUIRED_FIXTURE_COUNT:
        raise SystemExit(
            f"{canonical_vectors_path}: expected exactly {REQUIRED_FIXTURE_COUNT} v0 fixtures, got {len(vectors)}"
        )
    fixtures = [fixture_alignment(vector) for vector in vectors]

    digest_matches = sum(1 for fixture in fixtures if fixture["canonical_vs_current_lock_digest_match"])
    canonical_self_verified = sum(1 for fixture in fixtures if fixture["canonical_wallet_positive"]["self_verified"])
    current_self_verified = sum(1 for fixture in fixtures if fixture["current_lock_compat_positive"]["self_verified"])
    canonical_rejected_by_current = sum(
        1 for fixture in fixtures if fixture["cross_check"]["canonical_signature_accepts_current_lock_digest"] is False
    )
    current_rejected_by_canonical = sum(
        1 for fixture in fixtures if fixture["cross_check"]["current_lock_signature_accepts_canonical_digest"] is False
    )
    ready = bool(fixtures) and digest_matches == len(fixtures)

    return {
        "schema": SCHEMA,
        "classification": "wallet_signing_vectors_and_lock_digest_alignment_probe",
        "canonical_vectors": str(canonical_vectors_path),
        "source_digest_model": source_digest_model(source_path),
        "summary": {
            "fixtures": len(fixtures),
            "canonical_wallet_vectors": len(fixtures),
            "canonical_wallet_vectors_self_verified": canonical_self_verified,
            "current_lock_compat_vectors": len(fixtures),
            "current_lock_compat_vectors_self_verified": current_self_verified,
            "current_lock_digest_matches_canonical": digest_matches,
            "current_lock_digest_mismatches": len(fixtures) - digest_matches,
            "canonical_wallet_signatures_rejected_by_current_lock_digest": canonical_rejected_by_current,
            "current_lock_signatures_rejected_by_canonical_wallet_digest": current_rejected_by_canonical,
            "wallet_lock_alignment_ready": ready,
            "production_wallet_ready": False,
        },
        "message_rules": {
            "canonical_wallet_message": "BIP340 signs hashes.signed_intent_hash_after_resolved_receipt from novaseal-canonical-vectors",
            "current_lock_message": "btc_authority compute_intent_hash currently hashes only intent.domain through CellScript hash_blake2b",
            "required_alignment_before_production": "the lock/verifier/wallet must all sign the same 32-byte canonical intent digest",
        },
        "fixtures": fixtures,
        "required_next_work": [
            "Add or expose a CellScript/CKB hashing path for the exact canonical NovaSealIntentV0 preimage, or intentionally freeze a different protocol message rule.",
            "Regenerate the combined lock+type harness so witness signatures are created over the production message, not the current domain-hash compatibility message.",
            "Only mark wallet_lock_alignment_ready=true when every fixture has canonical_vs_current_lock_digest_match=true and cross-check signatures agree.",
        ],
        "limitations": [
            "This report uses packed-reference vectors, not Molecule output.",
            "The embedded secret keys are deterministic test-only material from the verifier-vector generator.",
            "This report does not change .cell behaviour and does not claim wallet readiness.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--canonical-vectors", type=Path, default=DEFAULT_CANONICAL_VECTORS)
    parser.add_argument("--source", type=Path, default=DEFAULT_SOURCE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report(args.canonical_vectors, args.source)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(report, indent=indent, sort_keys=True) + "\n", encoding="utf-8")

    summary = report["summary"]
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"fixtures={summary['fixtures']} "
        f"canonical_wallet_vectors_self_verified={summary['canonical_wallet_vectors_self_verified']} "
        f"current_lock_digest_matches_canonical={summary['current_lock_digest_matches_canonical']} "
        f"current_lock_digest_mismatches={summary['current_lock_digest_mismatches']} "
        f"wallet_lock_alignment_ready={summary['wallet_lock_alignment_ready']}"
    )
    return 0 if summary["canonical_wallet_vectors_self_verified"] == summary["fixtures"] else 1


if __name__ == "__main__":
    sys.exit(main())
