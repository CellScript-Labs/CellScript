#!/usr/bin/env python3
"""Extract packed fixed-field NovaSeal v0 schema layouts.

This is a reference layout extractor for the MVP schemas, not a full Molecule
compiler. It deliberately fails on unknown or dynamic field types.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


SCHEMA = "novaseal-schema-layout-v0.1"
ENCODING_PROFILE = "packed-fixed-v0-reference"

DEFAULT_OUTPUT = Path("target/novaseal-schema-layout.json")
SCHEMA_SOURCES = [
    ("NovaSealCellV0", Path("schemas/nova_seal_cell_v0.schema")),
    (None, Path("schemas/nova_intent_v0.schema")),
    (None, Path("schemas/proof_receipt_v0.schema")),
]

FIELD_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*:\s*([A-Za-z][A-Za-z0-9_]*)\b")
TYPE_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*:\s*$")

TYPE_INFO = {
    "u8": {"size_bytes": 1, "encoding": "little-endian unsigned integer"},
    "u16": {"size_bytes": 2, "encoding": "little-endian unsigned integer"},
    "u32": {"size_bytes": 4, "encoding": "little-endian unsigned integer"},
    "u64": {"size_bytes": 8, "encoding": "little-endian unsigned integer"},
    "Byte32": {"size_bytes": 32, "encoding": "exactly 32 bytes"},
    "OutPoint": {"size_bytes": 36, "encoding": "CKB OutPoint: tx_hash Byte32 || index u32 little-endian"},
}


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def read_schema(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except FileNotFoundError:
        raise SystemExit(f"missing schema file: {path}") from None


def parse_schema_types(default_type_name: str | None, path: Path, text: str) -> list[tuple[str, list[dict[str, Any]]]]:
    current_type = default_type_name
    fields_by_type: list[tuple[str, list[dict[str, Any]]]] = []
    current_fields: list[dict[str, Any]] = []

    def finish_current() -> None:
        nonlocal current_type, current_fields
        if current_type is not None:
            fields_by_type.append((current_type, current_fields))
        current_fields = []

    for line_number, line in enumerate(text.splitlines(), start=1):
        stripped = line.split("#", 1)[0].strip()
        if not stripped:
            continue
        type_match = TYPE_RE.match(stripped)
        if type_match:
            finish_current()
            current_type = type_match.group(1)
            continue
        match = FIELD_RE.match(stripped)
        if not match:
            raise SystemExit(f"unsupported schema syntax in {path}:{line_number}: {line}")
        if current_type is None:
            raise SystemExit(f"schema file {path}:{line_number} must declare TypeName: before fields")
        name, ty = match.groups()
        current_fields.append({"name": name, "type": ty, "line": line_number})
    finish_current()
    return fields_by_type


def field_components(field_name: str, field_type: str, offset: int) -> list[dict[str, Any]]:
    if field_type != "OutPoint":
        return []
    return [
        {
            "name": f"{field_name}.tx_hash",
            "type": "Byte32",
            "offset": offset,
            "size_bytes": 32,
            "encoding": TYPE_INFO["Byte32"]["encoding"],
        },
        {
            "name": f"{field_name}.index",
            "type": "u32",
            "offset": offset + 32,
            "size_bytes": 4,
            "encoding": TYPE_INFO["u32"]["encoding"],
        },
    ]


def layout_type(type_name: str, path: Path, source_bytes: bytes, parsed_fields: list[dict[str, Any]], known_types: dict[str, dict[str, Any]]) -> dict[str, Any]:
    offset = 0
    fields = []
    for parsed in parsed_fields:
        info = TYPE_INFO.get(parsed["type"]) or known_types.get(parsed["type"])
        if info is None:
            raise SystemExit(f"unsupported field type in {path}:{parsed['line']}: {parsed['type']}")
        size = int(info["size_bytes"])
        field = {
            "name": parsed["name"],
            "type": parsed["type"],
            "offset": offset,
            "size_bytes": size,
            "end_offset_exclusive": offset + size,
            "encoding": info["encoding"],
            "source_line": parsed["line"],
        }
        components = field_components(parsed["name"], parsed["type"], offset)
        if components:
            field["components"] = components
        fields.append(field)
        offset += size

    return {
        "name": type_name,
        "schema_path": str(path),
        "schema_sha256": sha256_hex(source_bytes),
        "encoding": ENCODING_PROFILE,
        "integer_endianness": "little",
        "padding": "none",
        "dynamic_fields": False,
        "field_count": len(fields),
        "total_static_size_bytes": offset,
        "fields": fields,
    }


def build_layout() -> dict[str, Any]:
    types = []
    known_types = dict(TYPE_INFO)
    for default_type_name, path in SCHEMA_SOURCES:
        text = read_schema(path)
        source_bytes = text.encode("utf-8")
        for type_name, fields in parse_schema_types(default_type_name, path, text):
            ty = layout_type(type_name, path, source_bytes, fields, known_types)
            known_types[type_name] = {"size_bytes": ty["total_static_size_bytes"], "encoding": f"packed {type_name}"}
            types.append(ty)
    layout_fingerprint_input = json.dumps(
        {
            "encoding_profile": ENCODING_PROFILE,
            "types": [
                {
                    "name": ty["name"],
                    "total_static_size_bytes": ty["total_static_size_bytes"],
                    "fields": [
                        {
                            "name": field["name"],
                            "type": field["type"],
                            "offset": field["offset"],
                            "size_bytes": field["size_bytes"],
                        }
                        for field in ty["fields"]
                    ],
                }
                for ty in types
            ],
        },
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return {
        "schema": SCHEMA,
        "encoding_profile": ENCODING_PROFILE,
        "molecule_status": "not_generated",
        "molecule_note": "This is a packed fixed-layout reference. It is not a Molecule table/schema compiler output.",
        "outpoint_encoding": "tx_hash Byte32 || index u32 little-endian",
        "integer_endianness": "little",
        "padding": "none",
        "layout_fingerprint_sha256": sha256_hex(layout_fingerprint_input),
        "types": types,
        "fiber_fungible_profile": {
            "status": "not_defined_in_v0_layout",
            "amount_offset": None,
            "note": "The xUDT/Fiber amount profile remains a future documented profile, not a field in NovaSealCellV0.",
        },
        "limitations": [
            "No dynamic Molecule table offsets are emitted.",
            "No canonical byte vectors are produced in this slice.",
            "No CellScript compiler ABI comparison is performed here.",
        ],
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--pretty", action="store_true")
    args = parser.parse_args()

    layout = build_layout()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    indent = 2 if args.pretty else None
    args.output.write_text(json.dumps(layout, indent=indent, sort_keys=True) + "\n", encoding="utf-8")

    print(f"wrote {args.output}")
    for ty in layout["types"]:
        print(f"{ty['name']}: fields={ty['field_count']} size={ty['total_static_size_bytes']} bytes")
    print(f"layout_fingerprint_sha256={layout['layout_fingerprint_sha256']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
