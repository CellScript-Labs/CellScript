#!/usr/bin/env python3
"""Regenerate all website build-time data from live cellc output.

Produces:
  - website/src/data/provenance.json      (per-example typed-transition metadata)
  - website/src/data/pipeline-fragments.json  (5-stage compiler artifact excerpts)

Run after changing examples/, the compiler, or before deploying the
website, so every number shown to users is backed by a real compilation.

Usage:
    python3 website/scripts/regen-website-data.py

Requires a built `cellc` binary at <repo>/target/release/cellc.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
CELLC = REPO / "target" / "release" / "cellc"
EXAMPLES = REPO / "examples"
DATA_DIR = REPO / "website" / "src" / "data"
PROVENANCE_OUT = DATA_DIR / "provenance.json"
FRAGMENTS_OUT = DATA_DIR / "pipeline-fragments.json"

HERO_EXAMPLES = {
    "token": "token.cell",
    "nft": "nft.cell",
    "amm": "amm_pool.cell",
    "vesting": "vesting.cell",
}

# The example used for the compiler-workflow pipeline trace.
PIPELINE_EXAMPLE = "token"

# How many lines of each artifact to show. The landing page fragments
# are excerpts — full output would dominate the section. These are
# generous enough to be informative, tight enough to fit a viewport.
SOURCE_LINES = 8
AST_LINES = 24
ASM_LINES = 28
ELF_HEX_LINES = 8


def run_cellc(args: list[str], capture_output: bool = True) -> str:
    result = subprocess.run(
        [str(CELLC)] + args,
        capture_output=capture_output,
        text=True,
        check=True,
        cwd=str(REPO),
    )
    return result.stdout


def run_metadata(example_file: str) -> dict:
    result = subprocess.run(
        [str(CELLC), "metadata", str(EXAMPLES / example_file), "--target-profile", "ckb"],
        capture_output=True,
        text=True,
        check=True,
        cwd=str(REPO),
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


def build_provenance_view(metadata: dict, type_names: dict[str, str]) -> dict:
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


def generate_provenance() -> dict:
    """Generate provenance.json from live cellc metadata for all examples."""
    metadatas = []
    for example_id, filename in HERO_EXAMPLES.items():
        if not (EXAMPLES / filename).exists():
            print(f"error: {EXAMPLES / filename} not found", file=sys.stderr)
            sys.exit(1)
        metadatas.append(run_metadata(filename))

    type_names = collect_global_type_names(metadatas)

    provenance = {
        example_id: build_provenance_view(metadata, type_names)
        for example_id, metadata in zip(HERO_EXAMPLES, metadatas)
    }

    PROVENANCE_OUT.write_text(json.dumps(provenance, indent=2) + "\n")
    print(f"wrote {PROVENANCE_OUT.relative_to(REPO)} ({PROVENANCE_OUT.stat().st_size} bytes)")
    for example_id, view in provenance.items():
        print(
            f"  {example_id}: {len(view['types'])} types, "
            f"{len(view['actions'])} actions, "
            f"{view['artifactSizeBytes']} bytes"
        )
    return provenance


def generate_pipeline_fragments() -> dict:
    """Generate pipeline-fragments.json from live cellc output.

    Each stage shows a real excerpt from compiling the pipeline example:
      1. Source    — first N lines of the .cell file
      2. AST       --parse output (Rust debug format, trimmed)
      3. Metadata  — JSON from cellc metadata (trimmed to key fields)
      4. RISC-V    — assembly from cellc -t riscv64-asm
      5. ELF       — hex dump from cellc -t riscv64-elf
    """
    example_file = HERO_EXAMPLES[PIPELINE_EXAMPLE]
    example_path = EXAMPLES / example_file

    # Stage 1: Source
    source_lines = example_path.read_text().split("\n")[:SOURCE_LINES]
    source = "\n".join(source_lines).rstrip()

    # Stage 2: AST (--parse dumps the parsed module to stdout)
    ast_raw = run_cellc(["--parse", str(example_path)])
    # The --parse output starts with "success: parsed successfully\n",
    # then the AST. Trim the success line and limit lines.
    ast_lines = ast_raw.split("\n")[1:][:AST_LINES]
    ast = "\n".join(ast_lines).rstrip()

    # Stage 3: Metadata (compact JSON with key fields)
    metadata = run_metadata(example_file)
    meta_excerpt = {
        "module": metadata.get("module"),
        "artifact_format": metadata.get("artifact_format"),
        "types": [
            {"name": t.get("name"), "kind": t.get("kind")}
            for t in (metadata.get("types") or [])[:3]
        ],
        "actions": [
            {"name": a.get("name"), "effect": a.get("effect_class")}
            for a in (metadata.get("actions") or [])[:3]
        ],
    }
    meta_json = json.dumps(meta_excerpt, indent=2)

    # Stage 4: RISC-V assembly
    with tempfile.NamedTemporaryFile(suffix=".asm", delete=False, dir=str(REPO)) as f:
        asm_path = f.name
    try:
        run_cellc(["-t", "riscv64-asm", str(example_path), "-o", asm_path])
        asm_full = Path(asm_path).read_text()
        asm_lines = asm_full.split("\n")[:ASM_LINES]
        riscv = "\n".join(asm_lines).rstrip()
    finally:
        os.unlink(asm_path)

    # Stage 5: ELF hex dump
    with tempfile.NamedTemporaryFile(suffix=".elf", delete=False, dir=str(REPO)) as f:
        elf_path = f.name
    try:
        run_cellc(["-t", "riscv64-elf", str(example_path), "-o", elf_path])
        elf_size = Path(elf_path).stat().st_size
        # Use xxd to produce a hex dump (same tool that produces the
        # canonical hex view). Limit lines for display.
        xxd_result = subprocess.run(
            ["xxd", str(elf_path)],
            capture_output=True,
            text=True,
            check=True,
        )
        elf_lines = xxd_result.stdout.split("\n")[:ELF_HEX_LINES]
        elf_hex = "\n".join(elf_lines).rstrip()
        elf = f"ELF64 RISC-V · {elf_size:,} bytes\n\n{elf_hex}"
    finally:
        os.unlink(elf_path)

    fragments = {
        "source": source,
        "ast": ast,
        "metadata": meta_json,
        "riscv": riscv,
        "elf": elf,
    }

    FRAGMENTS_OUT.write_text(json.dumps(fragments, indent=2) + "\n")
    print(f"\nwrote {FRAGMENTS_OUT.relative_to(REPO)} ({FRAGMENTS_OUT.stat().st_size} bytes)")
    for stage, content in fragments.items():
        line_count = len(content.split("\n"))
        print(f"  {stage}: {line_count} lines")
    return fragments


def main() -> int:
    if not CELLC.exists():
        print(
            f"error: {CELLC} not found. Run `cargo build --release --bin cellc` first.",
            file=sys.stderr,
        )
        return 1

    DATA_DIR.mkdir(parents=True, exist_ok=True)

    print("=== Generating provenance.json ===")
    generate_provenance()

    print("\n=== Generating pipeline-fragments.json ===")
    generate_pipeline_fragments()

    print("\nDone. All website data regenerated from live cellc output.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
