#!/usr/bin/env python3
"""Generate RISC-V verifier shell evidence.

The shell now mirrors the no-std RISC-V verifier policy: well-formed IPC
envelopes are checked with BIP340, malformed envelopes reject before crypto,
and spawn-word reconstruction is checked against the fixed inherited-fd input
contract.
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from pathlib import Path
from typing import Any

from novaseal_btc_verifier_vectors import bytes_from_hex, schnorr_verify


SCHEMA = "novaseal-btc-verifier-shell-report-v0.2"
DEFAULT_IPC_VECTORS = Path("target/novaseal-btc-verifier-ipc-vectors.json")
DEFAULT_OUTPUT = Path("target/novaseal-btc-verifier-shell-report.json")

IPC_MAGIC = b"NSBV0IPC"
IPC_VERSION = 0
IPC_SCHEME_BIP340 = 1
IPC_FLAGS_NONE = 0
IPC_BLOB_LEN = 144
IPC_WORD_COUNT = IPC_BLOB_LEN // 8

EXIT_REJECT_ENVELOPE = 10
EXIT_REJECT_SPAWN_IO = 11
EXIT_REJECT_CRYPTO = 12
EXIT_ACCEPT = 0


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SystemExit(f"missing JSON file: {path}") from None
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid JSON in {path}: {exc}") from None


def parse_envelope(blob: bytes) -> tuple[bool, str | None]:
    if len(blob) != IPC_BLOB_LEN:
        return False, "blob_length"
    if blob[0:8] != IPC_MAGIC:
        return False, "magic"
    version = struct.unpack("<H", blob[8:10])[0]
    if version != IPC_VERSION:
        return False, "version"
    scheme = struct.unpack("<H", blob[10:12])[0]
    if scheme != IPC_SCHEME_BIP340:
        return False, "scheme"
    flags = struct.unpack("<I", blob[12:16])[0]
    if flags != IPC_FLAGS_NONE:
        return False, "flags"
    return True, None


def blob_to_le_words(blob: bytes) -> tuple[list[int], int]:
    chunks = len(blob) // 8
    words = [struct.unpack("<Q", blob[offset : offset + 8])[0] for offset in range(0, chunks * 8, 8)]
    return words, len(blob) % 8


def blob_from_le_words(words: list[int]) -> bytes | None:
    if len(words) != IPC_WORD_COUNT:
        return None
    return b"".join(struct.pack("<Q", word) for word in words)


def verify_bip340_envelope(blob: bytes) -> bool:
    return schnorr_verify(blob[16:48], blob[48:80], blob[80:144])


def shell_decision(case: dict[str, Any]) -> dict[str, Any]:
    blob = bytes_from_hex(case["ipc_blob"], case["ipc_blob_len"])
    words, partial_tail_bytes = blob_to_le_words(blob)
    spawn_word_count = len(words)
    spawn_word_canonical = spawn_word_count == IPC_WORD_COUNT and partial_tail_bytes == 0
    roundtrip = spawn_word_canonical and blob_from_le_words(words) == blob
    expected_accept = case.get("expected") == "accept"
    if not spawn_word_canonical:
        reason = "partial_tail" if partial_tail_bytes else "word_count"
        return {
            "id": case["id"],
            "parsed": False,
            "accepted": False,
            "expected": case.get("expected"),
            "matched_expected": not expected_accept,
            "exit_code": EXIT_REJECT_SPAWN_IO,
            "spawn_words_representable": partial_tail_bytes == 0,
            "spawn_word_count": spawn_word_count,
            "partial_tail_bytes": partial_tail_bytes,
            "spawn_word_roundtrip": roundtrip,
            "spawn_entry_exit_code": EXIT_REJECT_SPAWN_IO,
            "reason": reason,
        }

    parsed, failure = parse_envelope(blob)
    if parsed:
        accepted = verify_bip340_envelope(blob)
        exit_code = EXIT_ACCEPT if accepted else EXIT_REJECT_CRYPTO
        return {
            "id": case["id"],
            "parsed": True,
            "accepted": accepted,
            "expected": case.get("expected"),
            "matched_expected": accepted == expected_accept,
            "exit_code": exit_code,
            "spawn_words_representable": True,
            "spawn_word_count": spawn_word_count,
            "partial_tail_bytes": partial_tail_bytes,
            "spawn_word_roundtrip": roundtrip,
            "spawn_entry_exit_code": exit_code,
            "reason": "accepted" if accepted else "crypto_reject",
        }
    return {
        "id": case["id"],
        "parsed": False,
        "accepted": False,
        "expected": case.get("expected"),
        "matched_expected": not expected_accept,
        "exit_code": EXIT_REJECT_ENVELOPE,
        "spawn_words_representable": True,
        "spawn_word_count": spawn_word_count,
        "partial_tail_bytes": partial_tail_bytes,
        "spawn_word_roundtrip": roundtrip,
        "spawn_entry_exit_code": EXIT_REJECT_ENVELOPE,
        "reason": failure,
    }


def build_report(ipc_vectors_path: Path) -> dict[str, Any]:
    source = load_json(ipc_vectors_path)
    normal = source.get("vectors", [])
    malformed = source.get("malformed", [])
    decisions = [shell_decision(case) for case in normal + malformed]
    parsed = [decision for decision in decisions if decision["parsed"]]
    parse_rejected = [decision for decision in decisions if not decision["parsed"]]
    accepted = [decision for decision in decisions if decision["accepted"]]
    crypto_rejected = [decision for decision in decisions if decision["reason"] == "crypto_reject"]
    word_representable = [decision for decision in decisions if decision["spawn_words_representable"]]
    word_roundtrip = [decision for decision in decisions if decision["spawn_word_roundtrip"]]
    spawn_io_rejects = [decision for decision in decisions if decision["spawn_entry_exit_code"] == EXIT_REJECT_SPAWN_IO]
    matched_expected = [decision for decision in decisions if decision["matched_expected"]]
    expected_accept = [decision for decision in decisions if decision["expected"] == "accept"]
    expected_reject = [decision for decision in decisions if decision["expected"] == "reject"]

    return {
        "schema": SCHEMA,
        "shell_crate": "verifier/novaseal_btc_verifier_riscv",
        "source_ipc_vectors": str(ipc_vectors_path),
        "classification": "spawn_word_input_bip340_riscv_shell_evidence",
        "spawn_input": {
            "fd_index": 0,
            "word_count": IPC_WORD_COUNT,
            "word_width_bytes": 8,
            "blob_len": IPC_BLOB_LEN,
            "endianness": "little",
        },
        "exit_codes": {
            "accept": EXIT_ACCEPT,
            "reject_envelope": EXIT_REJECT_ENVELOPE,
            "reject_spawn_io": EXIT_REJECT_SPAWN_IO,
            "reject_crypto": EXIT_REJECT_CRYPTO,
        },
        "summary": {
            "total_vectors": len(decisions),
            "well_formed_vectors": len(normal),
            "malformed_vectors": len(malformed),
            "expected_accept": len(expected_accept),
            "expected_reject": len(expected_reject),
            "parse_ok": len(parsed),
            "parse_rejected": len(parse_rejected),
            "accepted": len(accepted),
            "rejected": len(decisions) - len(accepted),
            "crypto_rejects": len(crypto_rejected),
            "spawn_word_representable": len(word_representable),
            "spawn_word_roundtrip": len(word_roundtrip),
            "spawn_io_rejects": len(spawn_io_rejects),
            "matched_expected": len(matched_expected),
            "all_expected_matched": len(matched_expected) == len(decisions),
        },
        "decisions": decisions,
        "limits": [
            "Model-level shell report; it mirrors the no-std shell policy and the fixed u64 spawn-word adapter.",
            "The RISC-V entry requires inherited fd index 0 to contain exactly 18 little-endian u64 words, but this report does not execute CKB VM spawn.",
            "This report does not produce cycle, binary-size, or child-verifier CKB VM evidence; use the RISC-V artifact and ckb-vm harness reports for that.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ipc-vectors", type=Path, default=DEFAULT_IPC_VECTORS)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    report = build_report(args.ipc_vectors)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(report, indent=indent, sort_keys=True) + "\n", encoding="utf-8")

    summary = report["summary"]
    print(f"wrote {args.output}")
    print(
        "summary: "
        f"total={summary['total_vectors']} "
        f"parse_ok={summary['parse_ok']} "
        f"parse_rejected={summary['parse_rejected']} "
        f"accepted={summary['accepted']} "
        f"rejected={summary['rejected']} "
        f"matched_expected={summary['matched_expected']} "
        f"spawn_word_roundtrip={summary['spawn_word_roundtrip']} "
        f"all_expected_matched={summary['all_expected_matched']}"
    )
    return 0 if summary["all_expected_matched"] else 1


if __name__ == "__main__":
    sys.exit(main())
