#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${CELLSCRIPT_0_14_SCOPE_AUDIT_DIR:-$ROOT_DIR/target/strict-0-14-scope-audit/$STAMP}"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/tmp/cellscript-ckb-release-gate-target}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"

cd "$ROOT_DIR"

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'missing required command: %s\n' "$1" >&2
        exit 127
    fi
}

run() {
    printf '\n==> %s\n' "$*"
    "$@"
}

require_doc_boundary() {
    local file="$1"
    local pattern="$2"
    if ! rg --quiet --fixed-strings "$pattern" "$file"; then
        printf '0.14 scope audit boundary missing in %s: %s\n' "$file" "$pattern" >&2
        exit 1
    fi
}

require_cmd cargo
require_cmd rg

if [[ -z "${CELLC_BIN:-}" ]]; then
    run cargo build --locked --bin cellc
    export CELLC_BIN="$CARGO_TARGET_DIR/debug/cellc"
fi

if [[ -f tests/v0_14.rs ]]; then
    run cargo test --locked -p cellscript --test v0_14 -- --test-threads=1
else
    printf '\n==> retired tests/v0_14.rs integration suite; continuing with scoped example audit\n'
fi
run cargo test --locked -p cellscript --test fuzzy_debug -- --test-threads=1

require_doc_boundary roadmap/CELLSCRIPT_0_14_ROADMAP.md 'v0.14 does not ship a source-level `max_cycles` spawn parameter'
require_doc_boundary roadmap/CELLSCRIPT_0_14_ROADMAP.md 'dedicated accepted/rejected CKB transaction fixture matrices to the later standard compatibility suite'
require_doc_boundary docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES.md 'Action Builder, CellFabric, CCC integration, or automatic transaction'
require_doc_boundary docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES.md 'a portable target profile; `ckb` is the implemented release profile'
require_doc_boundary README.md '0.14 release notes'

mkdir -p "$OUT_DIR"

examples=(
    examples/language/canonical_style.cell
    examples/language/v0_14_capacity_time.cell
    examples/language/v0_14_ckb_type_id_create.cell
    examples/language/v0_14_delegate_verify.cell
    examples/language/v0_14_hash_blake2b.cell
    examples/language/v0_14_multi_step_pipeline.cell
    examples/language/v0_14_witness_source.cell
)

metadata_files=()
for example in "${examples[@]}"; do
    base="$(basename "$example" .cell)"
    asm_out="$OUT_DIR/$base.s"
    elf_out="$OUT_DIR/$base.elf"
    run "$CELLC_BIN" "$example" --target riscv64-asm --target-profile ckb --output "$asm_out"
    run "$CELLC_BIN" "$example" --target riscv64-elf --target-profile ckb --output "$elf_out"
    metadata_files+=("$asm_out.meta.json")
done

run cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools -- \
    --root "$ROOT_DIR" scope014 "$OUT_DIR" "${metadata_files[@]}"

printf '\nCellScript 0.14 scope audit passed: %s\n' "$OUT_DIR"
