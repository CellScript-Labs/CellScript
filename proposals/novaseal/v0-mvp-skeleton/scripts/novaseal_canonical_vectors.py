#!/usr/bin/env python3
"""Generate NovaSeal v0 packed-reference canonical test vectors.

The vectors are deterministic test artefacts derived from fixture JSON plus
`target/novaseal-schema-layout.json`. They mirror CellScript's
`hash_blake2b_packed(value)` preimage rule; they are not Molecule output, not
wallet signing vectors, and not CKB VM transaction witnesses.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from novaseal_fixture_harness import fixture_paths, load_json, normalise_fixture_inputs, run_model


SCHEMA = "novaseal-canonical-vectors-v0.2"
ENCODING_PROFILE = "packed-fixed-v0-reference"
PACKED_HASH_DOMAIN = b"CellScriptPackedHashV0\x00"
CKB_HASH_PERSONAL = b"ckb-default-hash"

DEFAULT_FIXTURES = Path("fixtures")
DEFAULT_LAYOUT = Path("target/novaseal-schema-layout.json")
DEFAULT_OUTPUT = Path("target/novaseal-canonical-vectors.json")

VECTOR_PERSON = b"NovaSealVecV0"
ZERO_HASH = "0x" + "00" * 32


def hex0x(data: bytes) -> str:
    return "0x" + data.hex()


def ckb_blake2b256(data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=32, person=CKB_HASH_PERSONAL).digest()


def packed_hash_preimage(type_name: str, packed_bytes: bytes) -> bytes:
    return PACKED_HASH_DOMAIN + type_name.encode("utf-8") + b"\x00" + len(packed_bytes).to_bytes(4, "little") + packed_bytes


def packed_hash(type_name: str, packed_bytes: bytes) -> tuple[bytes, bytes]:
    preimage = packed_hash_preimage(type_name, packed_bytes)
    return preimage, ckb_blake2b256(preimage)


def blake32(label: str, value: Any) -> bytes:
    h = hashlib.blake2b(digest_size=32, person=VECTOR_PERSON)
    h.update(label.encode("utf-8"))
    h.update(b"\x00")
    h.update(str(value).encode("utf-8"))
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
        {"name": "tx_hash", "hex": hex0x(tx_hash), "source": tx_source},
        {"name": "index", "hex": hex0x(index), "source": index_source, "value": int(index_value)},
    ]
    return tx_hash + index, components, source


def layout_types(layout: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {ty["name"]: ty for ty in layout.get("types", [])}


def encode_field(
    field: dict[str, Any],
    value: Any,
    types: dict[str, dict[str, Any]],
    context: str,
) -> tuple[bytes, dict[str, Any]]:
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
    elif ty in types:
        if not isinstance(value, dict):
            raise ValueError(f"{context}: nested {ty} requires an object value")
        nested = encode_struct(ty, value, types, context)
        data = bytes.fromhex(nested["hex"][2:])
        detail = {"source": "nested_fixed_type", "nested": nested}
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
        data, record = encode_field(field, values[name], types, f"{context}.{name}")
        pieces.append(data)
        fields.append(record)
    encoded = b"".join(pieces)
    expected_size = int(layout["total_static_size_bytes"])
    if len(encoded) != expected_size:
        raise ValueError(f"{context}: encoded {len(encoded)} bytes, expected {expected_size}")
    preimage, digest = packed_hash(type_name, encoded)
    return {
        "type": type_name,
        "encoding_profile": ENCODING_PROFILE,
        "size_bytes": len(encoded),
        "hex": hex0x(encoded),
        "hash_preimage_rule": "CellScriptPackedHashV0\\0 || canonical_type_name || \\0 || u32_le(byte_len) || packed_bytes",
        "hash_preimage_hex": hex0x(preimage),
        "digest_blake2b_256": hex0x(digest),
        "fields": fields,
    }


def intent_core_values(model: dict[str, Any]) -> dict[str, Any]:
    intent = model["intent"]
    return {
        "protocol_id": intent["protocol_id"],
        "package_hash": intent["package_hash"],
        "policy_hash": intent["policy_hash"],
        "action": intent["action"],
        "terminal_path": intent["terminal_path"],
        "old_cell": intent["old_cell"],
        "old_state_hash": intent["old_state_hash"],
        "new_state_hash": intent["new_state_hash"],
        "old_nonce": intent["old_nonce"],
        "new_nonce": intent["new_nonce"],
        "expiry": intent["expiry"],
    }


def cell_commitment_values(old_cell: dict[str, Any], core: dict[str, Any]) -> dict[str, Any]:
    return {
        "version": old_cell["version"],
        "btc_authority_hash": old_cell["btc_authority_hash"],
        "state_hash": core["new_state_hash"],
        "policy_hash": old_cell["policy_hash"],
        "nonce": core["new_nonce"],
        "expiry": core["expiry"],
    }


def receipt_commitment_values(
    core: dict[str, Any],
    new_cell_commitment_hash: str,
    intent_core_hash: str,
    payout_commitment_hash: str = ZERO_HASH,
) -> dict[str, Any]:
    return {
        "protocol_id": core["protocol_id"],
        "package_hash": core["package_hash"],
        "policy_hash": core["policy_hash"],
        "action": core["action"],
        "terminal_path": core["terminal_path"],
        "old_cell": core["old_cell"],
        "new_cell_commitment": new_cell_commitment_hash,
        "old_state_hash": core["old_state_hash"],
        "new_state_hash": core["new_state_hash"],
        "old_nonce": core["old_nonce"],
        "new_nonce": core["new_nonce"],
        "intent_core_hash": intent_core_hash,
        "payout_commitment_hash": payout_commitment_hash,
    }


def receipt_values(
    model: dict[str, Any],
    core: dict[str, Any],
    new_cell_commitment_hash: str,
    intent_core_hash: str,
    signed_intent_hash: str,
    payout_commitment_hash: str = ZERO_HASH,
) -> dict[str, Any]:
    old_cell = model["old_cell"]
    return {
        "protocol_id": core["protocol_id"],
        "package_hash": core["package_hash"],
        "policy_hash": core["policy_hash"],
        "action": core["action"],
        "terminal_path": core["terminal_path"],
        "old_cell": core["old_cell"],
        "new_cell_commitment": new_cell_commitment_hash,
        "old_state_hash": core["old_state_hash"],
        "new_state_hash": core["new_state_hash"],
        "old_nonce": core["old_nonce"],
        "new_nonce": core["new_nonce"],
        "intent_core_hash": intent_core_hash,
        "signed_intent_hash": signed_intent_hash,
        "payout_commitment_hash": payout_commitment_hash,
        "signer_authority_hash": old_cell["btc_authority_hash"],
        "expiry": core["expiry"],
    }


def new_cell_values(model: dict[str, Any], materialized_receipt_hash: str) -> dict[str, Any]:
    old_cell = model["old_cell"]
    core = intent_core_values(model)
    return {
        "version": old_cell["version"],
        "btc_authority_hash": old_cell["btc_authority_hash"],
        "state_hash": core["new_state_hash"],
        "policy_hash": old_cell["policy_hash"],
        "latest_receipt_hash": materialized_receipt_hash,
        "nonce": core["new_nonce"],
        "expiry": core["expiry"],
    }


def resolved_transition_vectors(model: dict[str, Any], types: dict[str, dict[str, Any]], context: str) -> dict[str, Any]:
    core = intent_core_values(model)
    old_cell = model["old_cell"]

    intent_core = encode_struct("NovaSealIntentCoreV0", core, types, f"{context}.intent_core")
    intent_core_hash = intent_core["digest_blake2b_256"]

    new_cell_commitment = encode_struct(
        "NovaSealCellCommitmentV0",
        cell_commitment_values(old_cell, core),
        types,
        f"{context}.new_cell_commitment",
    )
    new_cell_commitment_hash = new_cell_commitment["digest_blake2b_256"]

    receipt_commitment = encode_struct(
        "ProofReceiptCommitmentV0",
        receipt_commitment_values(core, new_cell_commitment_hash, intent_core_hash),
        types,
        f"{context}.receipt_commitment",
    )
    materialized_receipt_hash = receipt_commitment["digest_blake2b_256"]

    signed_intent_values = {"core": core, "expected_receipt_hash": materialized_receipt_hash}
    signed_intent = encode_struct("NovaSealSignedIntentV0", signed_intent_values, types, f"{context}.signed_intent")
    signed_intent_hash = signed_intent["digest_blake2b_256"]

    receipt = encode_struct(
        "ProofReceiptV0",
        receipt_values(model, core, new_cell_commitment_hash, intent_core_hash, signed_intent_hash),
        types,
        f"{context}.receipt",
    )
    new_cell = encode_struct("NovaSealCellV0", new_cell_values(model, materialized_receipt_hash), types, f"{context}.new_cell")

    return {
        "rule": "latest_receipt_hash = hash_blake2b_packed(ProofReceiptCommitmentV0)",
        "intent_core": intent_core,
        "new_cell_commitment": new_cell_commitment,
        "receipt_commitment": receipt_commitment,
        "resolved_receipt_hash": materialized_receipt_hash,
        "resolved_intent": signed_intent,
        "signed_intent": signed_intent,
        "signed_intent_hash": signed_intent_hash,
        "resolved_receipt": receipt,
        "resolved_new_cell": new_cell,
        "receipt_hash_matches_intent": signed_intent_values["expected_receipt_hash"] == materialized_receipt_hash,
        "new_cell_latest_receipt_hash_matches": new_cell_values(model, materialized_receipt_hash)["latest_receipt_hash"]
        == materialized_receipt_hash,
    }


def run_fixture_vector(path: Path, types: dict[str, dict[str, Any]]) -> dict[str, Any]:
    fixture = load_json(path)
    model = normalise_fixture_inputs(fixture)
    model_result = run_model(model)

    old_cell_encoded = encode_struct("NovaSealCellV0", model["old_cell"], types, f"{path.stem}.old_cell")
    declared_core = intent_core_values(model)
    declared_intent = encode_struct(
        "NovaSealSignedIntentV0",
        {"core": declared_core, "expected_receipt_hash": model["intent"]["expected_receipt_hash"]},
        types,
        f"{path.stem}.declared_intent",
    )
    resolved = resolved_transition_vectors(model, types, f"{path.stem}.resolved")
    declared_expected_receipt_hash, declared_expected_receipt_hash_source = normalise_byte32(
        model["intent"]["expected_receipt_hash"],
        f"{path.stem}.intent.expected_receipt_hash",
    )

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
            "declared_intent": declared_intent,
            "new_cell": resolved["resolved_new_cell"] if model_result["new_cell"] is not None else None,
            "resolved": resolved,
        },
        "hashes": {
            "intent_core_hash": resolved["intent_core"]["digest_blake2b_256"],
            "declared_signed_intent_hash": declared_intent["digest_blake2b_256"],
            "declared_expected_receipt_hash": hex0x(declared_expected_receipt_hash),
            "declared_expected_receipt_hash_source": declared_expected_receipt_hash_source,
            "new_cell_commitment_hash": resolved["new_cell_commitment"]["digest_blake2b_256"],
            "resolved_receipt_hash": resolved["resolved_receipt_hash"],
            "latest_receipt_hash": resolved["resolved_receipt_hash"],
            "resolved_receipt_hash_matches_intent": resolved["receipt_hash_matches_intent"],
            "new_cell_latest_receipt_hash_matches": resolved["new_cell_latest_receipt_hash_matches"],
            "signed_intent_hash_after_resolved_receipt": resolved["signed_intent_hash"],
        },
        "notes": [
            "The declared_intent vector preserves the fixture-declared expected_receipt_hash for mismatch fixtures.",
            "The resolved vector uses the v0 split intent rule and CellScript hash_blake2b_packed preimage.",
            "Byte32 placeholders are deterministically derived for test-vector stability.",
        ],
    }


def receipt_commitment_analysis() -> dict[str, Any]:
    return {
        "status": "split_intent_and_explicit_receipt_commitment",
        "selected_rule": {
            "intent_core_hash": "hash_blake2b_packed(NovaSealIntentCoreV0)",
            "latest_receipt_hash": "hash_blake2b_packed(ProofReceiptCommitmentV0)",
            "signed_intent_hash": "hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })",
            "new_cell_commitment": "hash_blake2b_packed(NovaSealCellCommitmentV0), excluding latest_receipt_hash",
        },
        "why_this_breaks_the_cycle": [
            "ProofReceiptCommitmentV0 commits to intent_core_hash, not signed_intent_hash.",
            "NovaSealSignedIntentV0 commits to the expected receipt hash after the receipt commitment is materialized.",
            "NovaSealCellV0.latest_receipt_hash stores the current transition commitment only; it is not a rolling root.",
        ],
        "remaining_limits": [
            "This is still a packed-reference vector rule, not Molecule output.",
            "Wallet/verifier signing rules must adopt this exact preimage before production.",
        ],
    }


def build_report(fixtures_dir: Path, layout_path: Path) -> dict[str, Any]:
    layout = load_json(layout_path)
    types = layout_types(layout)
    vectors = [run_fixture_vector(path, types) for path in fixture_paths(fixtures_dir)]
    resolved_receipt_matches = sum(1 for vector in vectors if vector["hashes"]["resolved_receipt_hash_matches_intent"])
    latest_matches = sum(1 for vector in vectors if vector["hashes"]["new_cell_latest_receipt_hash_matches"])
    return {
        "schema": SCHEMA,
        "encoding_profile": ENCODING_PROFILE,
        "hash_preimage_rule": "CellScriptPackedHashV0\\0 || canonical_type_name || \\0 || u32_le(byte_len) || packed_bytes",
        "hash_algorithm": "blake2b-256(personal=ckb-default-hash)",
        "layout_artifact": str(layout_path),
        "layout_fingerprint_sha256": layout.get("layout_fingerprint_sha256"),
        "fixtures": str(fixtures_dir),
        "summary": {
            "vectors": len(vectors),
            "old_cell_vectors": len(vectors),
            "intent_core_vectors": len(vectors),
            "signed_intent_vectors": len(vectors),
            "receipt_commitment_vectors": len(vectors),
            "accepted_new_cell_vectors": sum(1 for vector in vectors if vector["encoded"]["new_cell"] is not None),
            "resolved_receipt_hash_matches_intent": resolved_receipt_matches,
            "new_cell_latest_receipt_hash_matches": latest_matches,
            "classification": "packed_reference_test_vectors",
        },
        "receipt_commitment_analysis": receipt_commitment_analysis(),
        "vectors": vectors,
        "limitations": [
            "Not Molecule output.",
            "Not CKB VM witness encoding.",
            "Not BTC wallet signing material.",
            "Placeholder Byte32 values are deterministic test derivations, not protocol constants.",
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
        f"signed_intent_vectors={summary['signed_intent_vectors']} "
        f"resolved_receipt_matches={summary['resolved_receipt_hash_matches_intent']} "
        f"latest_receipt_matches={summary['new_cell_latest_receipt_hash_matches']} "
        f"classification={summary['classification']}"
    )
    print(f"receipt_commitment_status={report['receipt_commitment_analysis']['status']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
