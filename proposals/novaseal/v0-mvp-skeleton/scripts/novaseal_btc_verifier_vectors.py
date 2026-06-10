#!/usr/bin/env python3
"""Generate NovaSeal v0 BIP340/secp256k1 verifier test vectors.

This script emits reference test vectors for the external
`novaseal_btc_verifier` TCB. It is not the verifier implementation and is not
used by the .cell source.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from novaseal_fixture_harness import load_json


SCHEMA = "novaseal-btc-verifier-vectors-v0.1"
DEFAULT_CANONICAL_VECTORS = Path("target/novaseal-canonical-vectors.json")
DEFAULT_OUTPUT = Path("target/novaseal-btc-verifier-vectors.json")

P = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFC2F
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
G = (
    0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798,
    0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8,
)
INF = None


def bytes_from_hex(value: str, expected_len: int) -> bytes:
    raw = value[2:] if value.startswith("0x") else value
    data = bytes.fromhex(raw)
    if len(data) != expected_len:
        raise ValueError(f"expected {expected_len} bytes, got {len(data)}")
    return data


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def int_from_bytes(data: bytes) -> int:
    return int.from_bytes(data, "big")


def bytes_from_int(value: int) -> bytes:
    return value.to_bytes(32, "big")


def tagged_hash(tag: str, data: bytes) -> bytes:
    tag_hash = hashlib.sha256(tag.encode("ascii")).digest()
    return hashlib.sha256(tag_hash + tag_hash + data).digest()


def has_even_y(point: tuple[int, int]) -> bool:
    return point[1] % 2 == 0


def point_add(a: tuple[int, int] | None, b: tuple[int, int] | None) -> tuple[int, int] | None:
    if a is INF:
        return b
    if b is INF:
        return a
    ax, ay = a
    bx, by = b
    if ax == bx and (ay + by) % P == 0:
        return INF
    if ax == bx and ay == by:
        slope = (3 * ax * ax) * pow(2 * ay, P - 2, P)
    else:
        slope = (by - ay) * pow(bx - ax, P - 2, P)
    slope %= P
    x = (slope * slope - ax - bx) % P
    y = (slope * (ax - x) - ay) % P
    return (x, y)


def point_neg(point: tuple[int, int] | None) -> tuple[int, int] | None:
    if point is INF:
        return INF
    return (point[0], (-point[1]) % P)


def point_mul(scalar: int, point: tuple[int, int] | None = G) -> tuple[int, int] | None:
    scalar %= N
    result = INF
    addend = point
    while scalar:
        if scalar & 1:
            result = point_add(result, addend)
        addend = point_add(addend, addend)
        scalar >>= 1
    return result


def lift_x(x: int) -> tuple[int, int] | None:
    if x >= P:
        return None
    y_sq = (pow(x, 3, P) + 7) % P
    y = pow(y_sq, (P + 1) // 4, P)
    if (y * y) % P != y_sq:
        return None
    if y % 2 != 0:
        y = P - y
    return (x, y)


def derive_secret(label: str) -> int:
    counter = 0
    while True:
        digest = hashlib.blake2b(f"{label}:{counter}".encode("utf-8"), digest_size=32, person=b"NovaSealKeyV0").digest()
        secret = int_from_bytes(digest) % N
        if secret != 0:
            return secret
        counter += 1


def pubkey_xonly(secret: int) -> bytes:
    point = point_mul(secret)
    if point is INF:
        raise ValueError("invalid secret produced infinity")
    return bytes_from_int(point[0])


def schnorr_sign(message: bytes, secret: int, aux_rand: bytes) -> bytes:
    if len(message) != 32:
        raise ValueError("BIP340 message must be 32 bytes")
    if secret <= 0 or secret >= N:
        raise ValueError("secret key out of range")
    point = point_mul(secret)
    if point is INF:
        raise ValueError("invalid secret")
    d = secret if has_even_y(point) else N - secret
    pubkey = bytes_from_int(point[0])
    t = bytes(a ^ b for a, b in zip(bytes_from_int(d), tagged_hash("BIP0340/aux", aux_rand)))
    k0 = int_from_bytes(tagged_hash("BIP0340/nonce", t + pubkey + message)) % N
    if k0 == 0:
        raise ValueError("invalid nonce")
    r_point = point_mul(k0)
    if r_point is INF:
        raise ValueError("invalid nonce point")
    k = k0 if has_even_y(r_point) else N - k0
    r = bytes_from_int(r_point[0])
    e = int_from_bytes(tagged_hash("BIP0340/challenge", r + pubkey + message)) % N
    s = (k + e * d) % N
    signature = r + bytes_from_int(s)
    if not schnorr_verify(message, pubkey, signature):
        raise ValueError("generated signature failed self-verification")
    return signature


def schnorr_verify(message: bytes, pubkey: bytes, signature: bytes) -> bool:
    if len(message) != 32 or len(pubkey) != 32 or len(signature) != 64:
        return False
    px = int_from_bytes(pubkey)
    r = int_from_bytes(signature[:32])
    s = int_from_bytes(signature[32:])
    point = lift_x(px)
    if point is None or r >= P or s >= N:
        return False
    e = int_from_bytes(tagged_hash("BIP0340/challenge", signature[:32] + pubkey + message)) % N
    r_point = point_add(point_mul(s), point_neg(point_mul(e, point)))
    if r_point is INF or not has_even_y(r_point):
        return False
    return r_point[0] == r


def aux_for(label: str) -> bytes:
    return hashlib.blake2b(label.encode("utf-8"), digest_size=32, person=b"NovaSealAuxV0").digest()


def positive_case(fixture: str, message: bytes, signer_index: int) -> dict[str, Any]:
    secret = derive_secret(f"{fixture}:signer:{signer_index}")
    pubkey = pubkey_xonly(secret)
    signature = schnorr_sign(message, secret, aux_for(f"{fixture}:signer:{signer_index}:aux"))
    return {
        "id": f"{fixture}:positive:signer:{signer_index}",
        "fixture": fixture,
        "case": "positive",
        "signer_index": signer_index,
        "message32": hex0x(message),
        "xonly_pubkey": hex0x(pubkey),
        "signature64": hex0x(signature),
        "test_secret_key": hex0x(bytes_from_int(secret)),
        "expected": "accept",
        "self_verified": schnorr_verify(message, pubkey, signature),
    }


def flip_first_byte(data: bytes) -> bytes:
    mutable = bytearray(data)
    mutable[0] ^= 0x01
    return bytes(mutable)


def flip_last_byte(data: bytes) -> bytes:
    mutable = bytearray(data)
    mutable[-1] ^= 0x01
    return bytes(mutable)


def negative_cases(fixture: str, message: bytes, positive: dict[str, Any]) -> list[dict[str, Any]]:
    pubkey = bytes_from_hex(positive["xonly_pubkey"], 32)
    signature = bytes_from_hex(positive["signature64"], 64)
    wrong_secret = derive_secret(f"{fixture}:wrong-pubkey")
    wrong_pubkey = pubkey_xonly(wrong_secret)
    r = signature[:32]
    s = signature[32:]
    cases = [
        {
            "id": f"{fixture}:negative:wrong_message",
            "fixture": fixture,
            "case": "negative",
            "mutation": "message first byte flipped",
            "message32": hex0x(flip_first_byte(message)),
            "xonly_pubkey": hex0x(pubkey),
            "signature64": hex0x(signature),
            "expected": "reject",
        },
        {
            "id": f"{fixture}:negative:wrong_pubkey",
            "fixture": fixture,
            "case": "negative",
            "mutation": "pubkey replaced",
            "message32": hex0x(message),
            "xonly_pubkey": hex0x(wrong_pubkey),
            "signature64": hex0x(signature),
            "expected": "reject",
        },
        {
            "id": f"{fixture}:negative:signature_bitflip",
            "fixture": fixture,
            "case": "negative",
            "mutation": "signature last byte flipped",
            "message32": hex0x(message),
            "xonly_pubkey": hex0x(pubkey),
            "signature64": hex0x(flip_last_byte(signature)),
            "expected": "reject",
        },
        {
            "id": f"{fixture}:negative:s_out_of_range",
            "fixture": fixture,
            "case": "negative",
            "mutation": "s set to curve order N",
            "message32": hex0x(message),
            "xonly_pubkey": hex0x(pubkey),
            "signature64": hex0x(r + bytes_from_int(N)),
            "expected": "reject",
        },
        {
            "id": f"{fixture}:negative:r_out_of_range",
            "fixture": fixture,
            "case": "negative",
            "mutation": "r set to field prime P",
            "message32": hex0x(message),
            "xonly_pubkey": hex0x(pubkey),
            "signature64": hex0x(bytes_from_int(P) + s),
            "expected": "reject",
        },
    ]
    for case in cases:
        ok = schnorr_verify(
            bytes_from_hex(case["message32"], 32),
            bytes_from_hex(case["xonly_pubkey"], 32),
            bytes_from_hex(case["signature64"], 64),
        )
        case["self_verified"] = ok
    return cases


def vector_message(vector: dict[str, Any]) -> bytes:
    return bytes_from_hex(vector["hashes"]["signed_intent_hash_after_resolved_receipt"], 32)


def build_report(canonical_vectors_path: Path) -> dict[str, Any]:
    canonical = load_json(canonical_vectors_path)
    positive = []
    negative = []
    for vector in canonical.get("vectors", []):
        fixture = vector["fixture"]
        message = vector_message(vector)
        positives_for_fixture = [positive_case(fixture, message, signer_index) for signer_index in range(4)]
        positive.extend(positives_for_fixture)
        negative.extend(negative_cases(fixture, message, positives_for_fixture[0]))

    positive_ok = sum(1 for case in positive if case["self_verified"] is True)
    negative_ok = sum(1 for case in negative if case["self_verified"] is False)
    return {
        "schema": SCHEMA,
        "canonical_vectors": str(canonical_vectors_path),
        "scheme": {
            "name": "bip340_schnorr_secp256k1",
            "curve": "secp256k1",
            "pubkey_format": "x-only 32-byte",
            "signature_format": "64-byte r||s",
            "message_format": "32-byte signed_intent_hash_after_resolved_receipt from novaseal-canonical-vectors",
            "low_s_rule": "not applicable to BIP340 Schnorr; reject s >= curve order",
            "malleability_rules": [
                "reject r >= field prime",
                "reject s >= curve order",
                "lift x-only pubkey to even-y point",
                "verify resulting R has even y",
            ],
        },
        "summary": {
            "positive_vectors": len(positive),
            "negative_vectors": len(negative),
            "positive_self_verified": positive_ok,
            "negative_self_rejected": negative_ok,
            "classification": "reference_bip340_vectors",
        },
        "positive": positive,
        "negative": negative,
        "limitations": [
            "Generated by a Python reference implementation, not the RISC-V verifier binary.",
            "Uses deterministic test-only secret keys derived from fixture names.",
            "Does not wire nova_btc_authority_lock.cell to spawn the verifier.",
            "Does not cover ECDSA or multisig descriptors.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--canonical-vectors", type=Path, default=DEFAULT_CANONICAL_VECTORS)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report(args.canonical_vectors)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(report, indent=indent, sort_keys=True) + "\n", encoding="utf-8")

    summary = report["summary"]
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"positive={summary['positive_vectors']} "
        f"negative={summary['negative_vectors']} "
        f"positive_self_verified={summary['positive_self_verified']} "
        f"negative_self_rejected={summary['negative_self_rejected']}"
    )
    return 0 if summary["positive_self_verified"] == summary["positive_vectors"] and summary["negative_self_rejected"] == summary["negative_vectors"] else 1


if __name__ == "__main__":
    sys.exit(main())
