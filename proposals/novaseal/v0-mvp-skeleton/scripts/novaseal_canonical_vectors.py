#!/usr/bin/env python3
"""Generate NovaSeal v0 packed-reference canonical test vectors.

The vectors are deterministic test artefacts derived from fixture JSON plus
`target/novaseal-schema-layout.json`. They are not Molecule output, not wallet
signing vectors, and not CKB VM transaction witnesses.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from novaseal_fixture_harness import fixture_paths, load_json, normalise_fixture_inputs, run_model


SCHEMA = "novaseal-canonical-vectors-v0.1"
ENCODING_PROFILE = "packed-fixed-v0-reference"

DEFAULT_FIXTURES = Path("fixtures")
DEFAULT_LAYOUT = Path("target/novaseal-schema-layout.json")
DEFAULT_OUTPUT = Path("target/novaseal-canonical-vectors.json")

VECTOR_PERSON = b"NovaSealVecV0"
HASH_PERSON = b"NovaSealHashV0"
RECEIPT_COMMITMENT_LABEL = "ProofReceiptV0.receipt_commitment_without_intent_hash"


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def blake32(label: str, value: Any) -> bytes:
    h = hashlib.blake2b(digest_size=32, person=VECTOR_PERSON)
    h.update(label.encode("utf-8"))
    h.update(b"\x00")
    h.update(str(value).encode("utf-8"))
    return h.digest()


def digest32(label: str, data: bytes) -> bytes:
    h = hashlib.blake2b(digest_size=32, person=HASH_PERSON)
    h.update(label.encode("utf-8"))
    h.update(b"\x00")
    h.update(data)
    return h.digest()


def is_full_hex(value: Any, byte_len: int) -> bool:
    if not isinstance(value, str):
        return False
    raw = value[2:] if value.startswith("0x") else value
    if len(raw) != byte_len * 2:
        return False
    try:
        bytes.fromhex(raw)
    except ValueError:
        return False
    return True


def hex_to_bytes(value: str, byte_len: int) -> bytes:
    raw = value[2:] if value.startswith("0x") else value
    data = bytes.fromhex(raw)
    if len(data) != byte_len:
        raise ValueError(f"expected {byte_len} bytes, got {len(data)}")
    return data


def normalise_byte32(value: Any, context: str) -> tuple[bytes, str]:
    if is_full_hex(value, 32):
        return hex_to_bytes(str(value), 32), "literal_hex"
    return blake32("Byte32", value), "derived_from_placeholder"


def encode_uint(value: Any, size: int, context: str) -> tuple[bytes, str]:
    if isinstance(value, bool):
        raise ValueError(f"{context}: boolean is not a valid integer")
    try:
        number = int(value)
    except (TypeError, ValueError) as exc:
        raise ValueError(f"{context}: expected integer-compatible value, got {value!r}") from exc
    if number < 0 or number >= 1 << (size * 8):
        raise ValueError(f"{context}: integer {number} does not fit in {size} bytes")
    return number.to_bytes(size, "little"), "integer_literal"


def encode_outpoint(value: Any, context: str) -> tuple[bytes, list[dict[str, Any]], str]:
    if isinstance(value, dict):
        tx_hash_value = value.get("tx_hash", f"{context}:tx_hash")
        index_value = value.get("index", 0)
        source = "object"
    else:
        tx_hash_value = f"{value}:tx_hash"
        index_value = 0
        source = "derived_from_placeholder"
    tx_hash, tx_source = normalise_byte32(tx_hash_value, f"{context}.tx_hash")
    index, index_source = encode_uint(index_value, 4, f"{context}.index")
    components = [
        {
            "name": "tx_hash",
            "hex": hex0x(tx_hash),
            "source": tx_source,
        },
        {
            "name": "index",
            "hex": hex0x(index),
            "source": index_source,
            "value": int(index_value),
        },
    ]
    return tx_hash + index, components, source


def layout_types(layout: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {ty["name"]: ty for ty in layout.get("types", [])}


def encode_field(field: dict[str, Any], value: Any, context: str) -> tuple[bytes, dict[str, Any]]:
    ty = field["type"]
    if ty == "Byte32":
        data, source = normalise_byte32(value, context)
        detail = {"source": source}
    elif ty in {"u8", "u16", "u32", "u64"}:
        data, source = encode_uint(value, int(field["size_bytes"]), context)
        detail = {"source": source, "value": int(value)}
    elif ty == "OutPoint":
        data, components, source = encode_outpoint(value, context)
        detail = {"source": source, "components": components}
    else:
        raise ValueError(f"{context}: unsupported field type {ty}")
    record = {
        "name": field["name"],
        "type": ty,
        "offset": field["offset"],
        "size_bytes": field["size_bytes"],
        "hex": hex0x(data),
        **detail,
    }
    return data, record


def encode_struct(type_name: str, values: dict[str, Any], types: dict[str, dict[str, Any]], context: str) -> dict[str, Any]:
    if type_name not in types:
        raise ValueError(f"missing layout for {type_name}")
    layout = types[type_name]
    pieces = []
    fields = []
    for field in layout["fields"]:
        name = field["name"]
        if name not in values:
            raise ValueError(f"{context}: missing field {name}")
        data, record = encode_field(field, values[name], f"{context}.{name}")
        pieces.append(data)
        fields.append(record)
    encoded = b"".join(pieces)
    expected_size = int(layout["total_static_size_bytes"])
    if len(encoded) != expected_size:
        raise ValueError(f"{context}: encoded {len(encoded)} bytes, expected {expected_size}")
    return {
        "type": type_name,
        "encoding_profile": ENCODING_PROFILE,
        "size_bytes": len(encoded),
        "hex": hex0x(encoded),
        "digest_blake2b_256": hex0x(digest32(type_name, encoded)),
        "fields": fields,
    }


def encode_receipt_commitment_preimage(values: dict[str, Any], types: dict[str, dict[str, Any]], context: str) -> dict[str, Any]:
    layout = types["ProofReceiptV0"]
    pieces = []
    fields = []
    skipped_fields = []
    for field in layout["fields"]:
        name = field["name"]
        if name == "intent_hash":
            skipped_fields.append(name)
            continue
        if name not in values:
            raise ValueError(f"{context}: missing field {name}")
        data, record = encode_field(field, values[name], f"{context}.{name}")
        pieces.append(data)
        fields.append(record)
    encoded = b"".join(pieces)
    return {
        "type": "ProofReceiptV0.receipt_commitment_preimage",
        "encoding_profile": ENCODING_PROFILE,
        "rule": "concatenate ProofReceiptV0 fields in schema order, excluding intent_hash",
        "excluded_fields": skipped_fields,
        "size_bytes": len(encoded),
        "hex": hex0x(encoded),
        "digest_blake2b_256": hex0x(digest32(RECEIPT_COMMITMENT_LABEL, encoded)),
        "fields": fields,
    }


def receipt_values(fixture_name: str, model: dict[str, Any], intent_hash: str) -> dict[str, Any]:
    intent = model["intent"]
    old_cell = model["old_cell"]
    return {
        "protocol": "NovaSeal",
        "version": 0,
        "action": intent["action"],
        "old_cell": intent["old_cell"],
        "old_state_hash": intent["old_state_hash"],
        "new_state_hash": intent["new_state_hash"],
        "intent_hash": intent_hash,
        "policy_hash": intent["policy_hash"],
        "signer_authority_hash": old_cell["btc_authority_hash"],
        "tx_hash": f"fixture:{fixture_name}:tx_hash",
        "nonce": intent["nonce"],
        "expiry": intent["expiry"],
    }


def resolved_receipt_vectors(fixture_name: str, model: dict[str, Any], types: dict[str, dict[str, Any]]) -> dict[str, Any]:
    preimage_seed_values = receipt_values(fixture_name, model, "0x" + "00" * 32)
    preimage = encode_receipt_commitment_preimage(preimage_seed_values, types, f"{fixture_name}.resolved_receipt_preimage")
    receipt_hash = preimage["digest_blake2b_256"]

    resolved_intent_values = dict(model["intent"])
    resolved_intent_values["receipt_hash"] = receipt_hash
    resolved_intent = encode_struct("NovaSealIntentV0", resolved_intent_values, types, f"{fixture_name}.resolved_intent")

    resolved_receipt_values = receipt_values(fixture_name, model, resolved_intent["digest_blake2b_256"])
    resolved_receipt = encode_struct("ProofReceiptV0", resolved_receipt_values, types, f"{fixture_name}.resolved_receipt")
    verification_preimage = encode_receipt_commitment_preimage(
        resolved_receipt_values,
        types,
        f"{fixture_name}.resolved_receipt_verify_preimage",
    )

    return {
        "rule": "receipt_hash = blake2b-256(label=ProofReceiptV0.receipt_commitment_without_intent_hash, ProofReceiptV0 fields excluding intent_hash)",
        "receipt_commitment_preimage": preimage,
        "resolved_receipt_hash": receipt_hash,
        "resolved_intent": resolved_intent,
        "signed_intent_hash": resolved_intent["digest_blake2b_256"],
        "resolved_receipt": resolved_receipt,
        "verification_preimage_hash": verification_preimage["digest_blake2b_256"],
        "receipt_hash_matches_intent": resolved_intent_values["receipt_hash"] == receipt_hash,
        "verification_preimage_matches": verification_preimage["digest_blake2b_256"] == receipt_hash,
        "excluded_from_receipt_hash": ["intent_hash"],
    }


def run_fixture_vector(path: Path, types: dict[str, dict[str, Any]]) -> dict[str, Any]:
    fixture = load_json(path)
    model = normalise_fixture_inputs(fixture)
    model_result = run_model(model)

    old_cell_encoded = encode_struct("NovaSealCellV0", model["old_cell"], types, f"{path.stem}.old_cell")
    intent_encoded = encode_struct("NovaSealIntentV0", model["intent"], types, f"{path.stem}.intent")
    receipt_preimage = receipt_values(path.stem, model, intent_encoded["digest_blake2b_256"])
    receipt_encoded = encode_struct("ProofReceiptV0", receipt_preimage, types, f"{path.stem}.receipt")
    legacy_receipt_commitment_preimage = encode_receipt_commitment_preimage(
        receipt_preimage,
        types,
        f"{path.stem}.legacy_receipt_preimage",
    )
    resolved = resolved_receipt_vectors(path.stem, model, types)

    new_cell_encoded = None
    if model_result["new_cell"] is not None:
        new_cell_encoded = encode_struct("NovaSealCellV0", model_result["new_cell"], types, f"{path.stem}.new_cell")

    declared_receipt_hash, declared_receipt_hash_source = normalise_byte32(model["intent"]["receipt_hash"], f"{path.stem}.intent.receipt_hash")
    computed_receipt_hash = bytes.fromhex(receipt_encoded["digest_blake2b_256"][2:])

    return {
        "fixture": path.name,
        "name": fixture.get("name", path.stem),
        "category": fixture.get("category"),
        "source_model_result": {
            "result": model_result["result"],
            "failure_mode": model_result["failure_mode"],
        },
        "encoded": {
            "old_cell": old_cell_encoded,
            "intent": intent_encoded,
            "new_cell": new_cell_encoded,
            "receipt_candidate": receipt_encoded,
            "legacy_receipt_commitment_preimage": legacy_receipt_commitment_preimage,
            "resolved": resolved,
        },
        "hashes": {
            "intent_hash": intent_encoded["digest_blake2b_256"],
            "declared_receipt_hash": hex0x(declared_receipt_hash),
            "declared_receipt_hash_source": declared_receipt_hash_source,
            "computed_receipt_candidate_hash": receipt_encoded["digest_blake2b_256"],
            "computed_receipt_candidate_hash_matches_intent": computed_receipt_hash == declared_receipt_hash,
            "legacy_receipt_commitment_hash": legacy_receipt_commitment_preimage["digest_blake2b_256"],
            "resolved_receipt_hash": resolved["resolved_receipt_hash"],
            "resolved_receipt_hash_matches_intent": resolved["receipt_hash_matches_intent"],
            "resolved_receipt_verification_preimage_matches": resolved["verification_preimage_matches"],
            "signed_intent_hash_after_resolved_receipt": resolved["signed_intent_hash"],
        },
        "notes": [
            "The receipt candidate uses the fixture-declared intent.receipt_hash for legacy comparison.",
            "The resolved vector uses the v0 candidate rule that excludes ProofReceiptV0.intent_hash from the receipt_hash preimage.",
            "Byte32 placeholders are deterministically derived for test-vector stability.",
        ],
    }


def receipt_commitment_analysis() -> dict[str, Any]:
    return {
        "status": "resolved_candidate_without_intent_hash",
        "legacy_cycle": [
            "NovaSealIntentV0.receipt_hash is intended to commit to ProofReceiptV0",
            "ProofReceiptV0.intent_hash commits to the encoded NovaSealIntentV0",
            "The encoded NovaSealIntentV0 includes receipt_hash",
        ],
        "selected_rule": {
            "name": "receipt_commitment_without_intent_hash",
            "receipt_hash": "blake2b-256 over ProofReceiptV0 fields in schema order, excluding intent_hash",
            "signed_intent_hash": "blake2b-256 over full NovaSealIntentV0 including receipt_hash",
            "proof_receipt_intent_hash": "the signed_intent_hash",
            "excluded_from_receipt_hash": ["intent_hash"],
        },
        "why_this_breaks_the_cycle": [
            "receipt_hash is computed before signed_intent_hash and does not depend on intent_hash",
            "the signed intent then commits to receipt_hash",
            "the materialised ProofReceiptV0 can carry signed_intent_hash without changing receipt_hash",
        ],
        "remaining_limits": [
            "This is still a packed-reference vector rule, not Molecule output.",
            "The .cell source still checks only receipt_hash == intent.receipt_hash.",
            "Wallet/verifier signing rules must adopt the same preimage before production.",
        ],
    }


def build_report(fixtures_dir: Path, layout_path: Path) -> dict[str, Any]:
    layout = load_json(layout_path)
    types = layout_types(layout)
    vectors = [run_fixture_vector(path, types) for path in fixture_paths(fixtures_dir)]
    receipt_matches = sum(1 for vector in vectors if vector["hashes"]["computed_receipt_candidate_hash_matches_intent"])
    resolved_receipt_matches = sum(1 for vector in vectors if vector["hashes"]["resolved_receipt_hash_matches_intent"])
    resolved_preimage_matches = sum(1 for vector in vectors if vector["hashes"]["resolved_receipt_verification_preimage_matches"])
    return {
        "schema": SCHEMA,
        "encoding_profile": ENCODING_PROFILE,
        "layout_artifact": str(layout_path),
        "layout_fingerprint_sha256": layout.get("layout_fingerprint_sha256"),
        "fixtures": str(fixtures_dir),
        "summary": {
            "vectors": len(vectors),
            "old_cell_vectors": len(vectors),
            "intent_vectors": len(vectors),
            "receipt_candidate_vectors": len(vectors),
            "accepted_new_cell_vectors": sum(1 for vector in vectors if vector["encoded"]["new_cell"] is not None),
            "computed_receipt_candidate_hash_matches_intent": receipt_matches,
            "resolved_receipt_hash_matches_intent": resolved_receipt_matches,
            "resolved_receipt_verification_preimage_matches": resolved_preimage_matches,
            "classification": "packed_reference_test_vectors",
        },
        "receipt_commitment_analysis": receipt_commitment_analysis(),
        "vectors": vectors,
        "limitations": [
            "Not Molecule output.",
            "Not CKB VM witness encoding.",
            "Not BTC wallet signing material.",
            "Placeholder Byte32 values are deterministic test derivations, not protocol constants.",
            "Receipt hash materialisation is represented by a v0 candidate preimage rule and still requires wallet/verifier adoption.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, default=DEFAULT_FIXTURES)
    parser.add_argument("--layout", type=Path, default=DEFAULT_LAYOUT)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report(args.fixtures, args.layout)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(report, indent=indent, sort_keys=True) + "\n", encoding="utf-8")

    summary = report["summary"]
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"vectors={summary['vectors']} "
        f"intent_vectors={summary['intent_vectors']} "
        f"receipt_matches={summary['computed_receipt_candidate_hash_matches_intent']} "
        f"resolved_receipt_matches={summary['resolved_receipt_hash_matches_intent']} "
        f"classification={summary['classification']}"
    )
    print(f"receipt_commitment_status={report['receipt_commitment_analysis']['status']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
