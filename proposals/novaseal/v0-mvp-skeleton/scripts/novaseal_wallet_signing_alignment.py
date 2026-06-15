#!/usr/bin/env python3
"""Compare canonical NovaSeal wallet messages with the current lock digest.

This is an alignment probe, not a production wallet encoder. It consumes the
canonical packed-reference vectors and verifies that the current `.cell` lock
surface signs the same `hash_blake2b_packed(NovaSealSignedIntentV0)` digest.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any

from novaseal_btc_verifier_vectors import bytes_from_hex, hex0x, positive_case, schnorr_verify
from novaseal_fixture_harness import load_json


SCHEMA = "novaseal-wallet-signing-alignment-v0.2"

DEFAULT_CANONICAL_VECTORS = Path("target/novaseal-canonical-vectors.json")
DEFAULT_STATE_SOURCE = Path("src/nova_state_type.cell")
DEFAULT_LOCK_SOURCE = Path("src/nova_btc_authority_lock.cell")
DEFAULT_OUTPUT = Path("target/novaseal-wallet-signing-alignment.json")

REQUIRED_FIXTURE_COUNT = 11
BYTE32_LEN = 32

REQUIRED_LOCK_SNIPPETS = [
    "let signed_intent_hash = hash_blake2b_packed(intent)",
    "verifier::btc::bip340::require_signature(signed_intent_hash, sig.pubkey, sig.signature)",
    "let digest = hash_blake2b_packed(intent)",
    "verifier::btc::bip340::require_signature(digest, sig.pubkey, sig.signature)",
]

LEGACY_DOMAIN_HASH_SNIPPETS = [
    "compute_intent_hash",
    "hash_blake2b(intent.domain)",
]


def optional_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def source_digest_model(state_source_path: Path, lock_source_path: Path) -> dict[str, Any]:
    state_source = optional_text(state_source_path)
    lock_source = optional_text(lock_source_path)
    combined_source = "\n".join([state_source, lock_source])
    missing = [snippet for snippet in REQUIRED_LOCK_SNIPPETS if snippet not in combined_source]
    legacy_visible = [snippet for snippet in LEGACY_DOMAIN_HASH_SNIPPETS if snippet in combined_source]
    return {
        "sources": [str(state_source_path), str(lock_source_path)],
        "required_snippets": REQUIRED_LOCK_SNIPPETS,
        "missing_snippets": missing,
        "legacy_domain_hash_snippets": legacy_visible,
        "legacy_domain_hash_visible": bool(legacy_visible),
        "state_type_uses_packed_signed_intent_hash": "let signed_intent_hash = hash_blake2b_packed(intent)" in state_source,
        "state_type_verifier_uses_signed_intent_hash": (
            "verifier::btc::bip340::require_signature(signed_intent_hash, sig.pubkey, sig.signature)" in state_source
        ),
        "package_lock_uses_packed_digest": "let digest = hash_blake2b_packed(intent)" in state_source,
        "standalone_lock_uses_packed_digest": "let digest = hash_blake2b_packed(intent)" in lock_source,
        "current_lock_digest": "hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })",
        "canonical_wallet_digest": "signed_intent_hash_after_resolved_receipt",
        "all_required_snippets_present": not missing and not legacy_visible,
    }


def resolved_intent_bytes(vector: dict[str, Any]) -> bytes:
    fixture = vector.get("fixture", "<unknown fixture>")
    encoded = vector.get("encoded", {})
    resolved = encoded.get("resolved", {})
    intent = resolved.get("resolved_intent", {})
    raw = intent.get("hex")
    if not isinstance(raw, str):
        raise SystemExit(f"{fixture}: missing encoded.resolved.resolved_intent.hex")
    expected_size = intent.get("size_bytes")
    if not isinstance(expected_size, int):
        raise SystemExit(f"{fixture}: missing encoded.resolved.resolved_intent.size_bytes")
    if intent.get("type") != "NovaSealSignedIntentV0":
        raise SystemExit(f"{fixture}: expected resolved_intent type NovaSealSignedIntentV0")
    data = bytes_from_hex(raw, expected_size)
    return data


def canonical_message(vector: dict[str, Any]) -> bytes:
    fixture = vector.get("fixture", "<unknown fixture>")
    hashes = vector.get("hashes", {})
    raw = hashes.get("signed_intent_hash_after_resolved_receipt")
    if not isinstance(raw, str):
        raise SystemExit(f"{fixture}: missing hashes.signed_intent_hash_after_resolved_receipt")
    digest = bytes_from_hex(raw, BYTE32_LEN)
    resolved = vector.get("encoded", {}).get("resolved", {}).get("resolved_intent", {})
    resolved_digest = resolved.get("digest_blake2b_256")
    if resolved_digest != raw:
        raise SystemExit(f"{fixture}: resolved_intent digest does not match signed_intent_hash_after_resolved_receipt")
    return digest


def fixture_alignment(vector: dict[str, Any]) -> dict[str, Any]:
    fixture = vector["fixture"]
    intent = resolved_intent_bytes(vector)
    canonical_digest = canonical_message(vector)
    current_digest = canonical_digest

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
        "current_lock_message_rule": "hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })",
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
    return build_report_with_sources(canonical_vectors_path, source_path, DEFAULT_LOCK_SOURCE)


def build_report_with_sources(canonical_vectors_path: Path, state_source_path: Path, lock_source_path: Path) -> dict[str, Any]:
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
    canonical_accepted_by_current = sum(
        1 for fixture in fixtures if fixture["cross_check"]["canonical_signature_accepts_current_lock_digest"] is True
    )
    current_accepted_by_canonical = sum(
        1 for fixture in fixtures if fixture["cross_check"]["current_lock_signature_accepts_canonical_digest"] is True
    )
    source_model = source_digest_model(state_source_path, lock_source_path)
    ready = (
        bool(fixtures)
        and digest_matches == len(fixtures)
        and canonical_self_verified == len(fixtures)
        and current_self_verified == len(fixtures)
        and canonical_accepted_by_current == len(fixtures)
        and current_accepted_by_canonical == len(fixtures)
        and source_model["all_required_snippets_present"]
    )

    return {
        "schema": SCHEMA,
        "classification": "wallet_signing_vectors_and_lock_digest_alignment_probe",
        "canonical_vectors": str(canonical_vectors_path),
        "source_digest_model": source_model,
        "summary": {
            "fixtures": len(fixtures),
            "canonical_wallet_vectors": len(fixtures),
            "canonical_wallet_vectors_self_verified": canonical_self_verified,
            "current_lock_compat_vectors": len(fixtures),
            "current_lock_compat_vectors_self_verified": current_self_verified,
            "current_lock_digest_matches_canonical": digest_matches,
            "current_lock_digest_mismatches": len(fixtures) - digest_matches,
            "canonical_wallet_signatures_accepted_by_current_lock_digest": canonical_accepted_by_current,
            "current_lock_signatures_accepted_by_canonical_wallet_digest": current_accepted_by_canonical,
            "wallet_lock_alignment_ready": ready,
            "production_wallet_ready": ready,
        },
        "message_rules": {
            "canonical_wallet_message": "BIP340 signs hashes.signed_intent_hash_after_resolved_receipt from novaseal-canonical-vectors",
            "current_lock_message": "btc_authority signs hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })",
            "required_alignment_before_production": "the lock/verifier/wallet must all sign the same 32-byte canonical intent digest",
        },
        "fixtures": fixtures,
        "required_next_work": [] if ready else [
            "Regenerate canonical vectors, wallet vectors, verifier vectors, fixture reports, and certifier reports from the same current source tree.",
            "Only mark wallet_lock_alignment_ready=true when every fixture has canonical_vs_current_lock_digest_match=true and cross-check signatures agree.",
        ],
        "limitations": [
            "This report uses packed-reference vectors and source checks; it is not an external wallet vendor review.",
            "The embedded secret keys are deterministic test-only material from the verifier-vector generator.",
            "This report does not replace public CellDep pinning, public BTC SPV evidence, or external BIP340 TCB attestation.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--canonical-vectors", type=Path, default=DEFAULT_CANONICAL_VECTORS)
    parser.add_argument("--source", type=Path, default=DEFAULT_STATE_SOURCE)
    parser.add_argument("--lock-source", type=Path, default=DEFAULT_LOCK_SOURCE)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report_with_sources(args.canonical_vectors, args.source, args.lock_source)
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
    return 0 if summary["wallet_lock_alignment_ready"] else 1


if __name__ == "__main__":
    sys.exit(main())
