#!/usr/bin/env python3
"""Generate NovaSeal v0 verifier IPC envelope vectors.

The output bridges the BIP340 verifier vectors to the exact fixed binary blob
that a future `nova_btc_authority_lock.cell` spawn path must pass to
`novaseal_btc_verifier`.
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path
from typing import Any

from novaseal_btc_verifier_vectors import bytes_from_hex, hex0x


SCHEMA = "novaseal-btc-verifier-ipc-vectors-v0.1"
DEFAULT_BTC_VECTORS = Path("target/novaseal-btc-verifier-vectors.json")
DEFAULT_OUTPUT = Path("target/novaseal-btc-verifier-ipc-vectors.json")

IPC_MAGIC = b"NSBV0IPC"
IPC_VERSION = 0
IPC_SCHEME_BIP340 = 1
IPC_FLAGS_NONE = 0
IPC_HEADER_LEN = 16
IPC_BLOB_LEN = 144


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"missing JSON file: {path}") from None
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from None


def build_blob(message32: str, xonly_pubkey: str, signature64: str) -> bytes:
    message = bytes_from_hex(message32, 32)
    pubkey = bytes_from_hex(xonly_pubkey, 32)
    signature = bytes_from_hex(signature64, 64)
    blob = (
        IPC_MAGIC
        + struct.pack("<H", IPC_VERSION)
        + struct.pack("<H", IPC_SCHEME_BIP340)
        + struct.pack("<I", IPC_FLAGS_NONE)
        + message
        + pubkey
        + signature
    )
    if len(blob) != IPC_BLOB_LEN:
        raise ValueError(f"internal IPC blob size mismatch: {len(blob)}")
    return blob


def vector_case(case: dict[str, Any]) -> dict[str, Any]:
    blob = build_blob(case["message32"], case["xonly_pubkey"], case["signature64"])
    return {
        "id": case["id"],
        "fixture": case.get("fixture"),
        "source_case": case.get("case"),
        "expected": case["expected"],
        "ipc_blob": hex0x(blob),
        "ipc_blob_len": len(blob),
        "message32": case["message32"],
        "xonly_pubkey": case["xonly_pubkey"],
        "signature64": case["signature64"],
    }


def malformed_cases(seed_blob: bytes) -> list[dict[str, Any]]:
    wrong_magic = bytearray(seed_blob)
    wrong_magic[0] ^= 0x01

    wrong_version = bytearray(seed_blob)
    wrong_version[8:10] = struct.pack("<H", 1)

    wrong_scheme = bytearray(seed_blob)
    wrong_scheme[10:12] = struct.pack("<H", 2)

    nonzero_flags = bytearray(seed_blob)
    nonzero_flags[12:16] = struct.pack("<I", 1)

    truncated = seed_blob[:-1]

    return [
        {
            "id": "malformed:wrong_magic",
            "mutation": "first magic byte flipped",
            "expected": "reject",
            "ipc_blob": hex0x(bytes(wrong_magic)),
            "ipc_blob_len": len(wrong_magic),
        },
        {
            "id": "malformed:unsupported_version",
            "mutation": "version set to 1",
            "expected": "reject",
            "ipc_blob": hex0x(bytes(wrong_version)),
            "ipc_blob_len": len(wrong_version),
        },
        {
            "id": "malformed:unsupported_scheme",
            "mutation": "scheme set to 2",
            "expected": "reject",
            "ipc_blob": hex0x(bytes(wrong_scheme)),
            "ipc_blob_len": len(wrong_scheme),
        },
        {
            "id": "malformed:nonzero_flags",
            "mutation": "flags set to 1",
            "expected": "reject",
            "ipc_blob": hex0x(bytes(nonzero_flags)),
            "ipc_blob_len": len(nonzero_flags),
        },
        {
            "id": "malformed:truncated",
            "mutation": "final byte removed",
            "expected": "reject",
            "ipc_blob": hex0x(truncated),
            "ipc_blob_len": len(truncated),
        },
    ]


def build_report(source_path: Path) -> dict[str, Any]:
    source = load_json(source_path)
    vectors = [vector_case(case) for case in source.get("positive", []) + source.get("negative", [])]
    if not vectors:
        raise SystemExit("source BTC verifier vector report contains no vectors")

    seed_blob = bytes_from_hex(vectors[0]["ipc_blob"], IPC_BLOB_LEN)
    malformed = malformed_cases(seed_blob)
    expected_accept = sum(1 for case in vectors if case["expected"] == "accept")
    expected_reject = sum(1 for case in vectors + malformed if case["expected"] == "reject")

    return {
        "schema": SCHEMA,
        "source_vector_report": str(source_path),
        "ipc_contract": {
            "magic_ascii": IPC_MAGIC.decode("ascii"),
            "version": IPC_VERSION,
            "scheme_bip340": IPC_SCHEME_BIP340,
            "flags": IPC_FLAGS_NONE,
            "endianness": "little",
            "blob_len": IPC_BLOB_LEN,
            "layout": [
                {"field": "magic", "offset": 0, "size": 8},
                {"field": "version_u16_le", "offset": 8, "size": 2},
                {"field": "scheme_u16_le", "offset": 10, "size": 2},
                {"field": "flags_u32_le", "offset": 12, "size": 4},
                {"field": "message32", "offset": 16, "size": 32},
                {"field": "xonly_pubkey", "offset": 48, "size": 32},
                {"field": "signature64", "offset": 80, "size": 64},
            ],
        },
        "vectors": vectors,
        "malformed": malformed,
        "summary": {
            "source_positive": len(source.get("positive", [])),
            "source_negative": len(source.get("negative", [])),
            "ipc_vectors": len(vectors),
            "malformed_vectors": len(malformed),
            "total_vectors": len(vectors) + len(malformed),
            "expected_accept": expected_accept,
            "expected_reject": expected_reject,
            "classification": "fixed_ipc_envelope_vectors",
        },
        "limits": [
            "Host-verifier IPC evidence only; no CKB spawn execution.",
            "No RISC-V verifier binary is built by this script.",
            "The .cell lock now constructs this blob for the RISC-V BIP340 shell; this script still does not execute CKB spawn.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--btc-vectors", type=Path, default=DEFAULT_BTC_VECTORS)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report(args.btc_vectors)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(report, indent=indent, sort_keys=True) + "\n", encoding="utf-8")

    summary = report["summary"]
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"ipc_vectors={summary['ipc_vectors']} "
        f"malformed={summary['malformed_vectors']} "
        f"total={summary['total_vectors']} "
        f"expected_accept={summary['expected_accept']} "
        f"expected_reject={summary['expected_reject']}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
