#!/usr/bin/env python3
"""Regenerate website/src/data/provenance.json from live cellc metadata.

Run after changing examples/ or the compiler, so the website's provenance
rail and hero compile-output indicators stay in sync with reality.

Usage:
    python3 website/scripts/regen-provenance.py

Requires a built `cellc` binary at <repo>/target/release/cellc.
"""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CELLC = REPO / "target" / "release" / "cellc"
EXAMPLES = REPO / "examples"
OUT = REPO / "website" / "src" / "data" / "provenance.json"

# Maps the four hero examples to their source files. The website keys
# provenance data by these ids (see src/data/site.ts heroExamples).
HERO_EXAMPLES = {
    "token": "token.cell",
    "nft": "nft.cell",
    "amm": "amm_pool.cell",
    "vesting": "vesting.cell",
}


def run_metadata(example_file: str) -> dict:
    result = subprocess.run(
        [
            str(CELLC),
            "metadata",
            str(EXAMPLES / example_file),
            "--target-profile",
            "ckb",
        ],
        capture_output=True,
        text=True,
        check=True,
    )
    return json.loads(result.stdout)


def collect_global_type_names(metadatas: list[dict]) -> dict[str, str]:
    """Build a type-hash -> friendly-name map across all examples.

    Imported types (e.g. Token from fungible_token) are only defined in
    one module but referenced by hash elsewhere, so a global pass is
    needed to resolve every consume/create set.
    """
    names: dict[str, str] = {}
    for m in metadatas:
        for t in m.get("types") or []:
            for hash_key in ("hash_type_source", "hash"):
                h = t.get(hash_key)
                if h and t.get("name"):
                    names[h] = t["name"]
    # The fungible Token type is reused across examples by hash.
    names["a2fb2f9b3990cd9b473352ff466d94a720c6a8c56ce9e014536872ea71c808d1"] = "Token"
    return names


def simplify_set(entries, params_by_binding, type_names):
    """Reduce a verbose consume/create set to [{op, type, binding}].

    The compiler emits rich per-entry objects (with CKB output ABI
    details). The website rail only needs the operation verb, the type
    name, and the binding, so we strip the rest. Types are resolved in
    priority order: explicit ty field, global hash map, then the
    action's own param signatures (which carry the declared type name).
    """
    resolved = []
    for entry in entries or []:
        binding = entry.get("binding")
        ty = (
            entry.get("ty")
            or type_names.get(entry.get("type_hash"))
            or params_by_binding.get(binding)
            or "Cell"
        )
        resolved.append({"op": entry.get("operation"), "type": ty, "binding": binding})
    return resolved


def build_view(metadata: dict, type_names: dict[str, str]) -> dict:
    actions = []
    for action in metadata.get("actions") or []:
        params_by_binding = {
            p.get("name"): p.get("ty") for p in action.get("params") or []
        }
        actions.append(
            {
                "name": action.get("name"),
                "effectClass": action.get("effect_class"),
                "consume": simplify_set(
                    action.get("consume_set"), params_by_binding, type_names
                ),
                "create": simplify_set(
                    action.get("create_set"), params_by_binding, type_names
                ),
                "estimatedCycles": action.get("estimated_cycles"),
                "parallelizable": action.get("parallelizable"),
            }
        )
    return {
        "module": metadata.get("module"),
        "target": "ckb",
        "artifactSizeBytes": metadata.get("artifact_size_bytes"),
        "artifactHash": (metadata.get("artifact_hash") or "")[:16],
        "sourceHash": (metadata.get("source_hash") or "")[:16],
        "compilerVersion": metadata.get("compiler_version"),
        "types": [
            {
                "name": t.get("name"),
                "kind": t.get("kind"),
                "capabilities": t.get("capabilities") or [],
                "encodedSize": t.get("encoded_size"),
                "flowStates": t.get("flow_states") or [],
            }
            for t in metadata.get("types") or []
        ],
        "actions": actions,
    }


def main() -> int:
    if not CELLC.exists():
        print(f"error: {CELLC} not found. Run `cargo build --release --bin cellc` first.", file=sys.stderr)
        return 1

    metadatas = []
    for example_id, filename in HERO_EXAMPLES.items():
        if not (EXAMPLES / filename).exists():
            print(f"error: {EXAMPLES / filename} not found", file=sys.stderr)
            return 1
        metadatas.append(run_metadata(filename))

    type_names = collect_global_type_names(metadatas)

    provenance = {
        example_id: build_view(metadata, type_names)
        for example_id, metadata in zip(HERO_EXAMPLES, metadatas)
    }

    OUT.write_text(json.dumps(provenance, indent=2) + "\n")
    print(f"wrote {OUT.relative_to(REPO)} ({OUT.stat().st_size} bytes)")
    for example_id, view in provenance.items():
        print(
            f"  {example_id}: {len(view['types'])} types, "
            f"{len(view['actions'])} actions, "
            f"{view['artifactSizeBytes']} bytes"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
