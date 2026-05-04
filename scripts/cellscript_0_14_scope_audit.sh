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
require_cmd python3
require_cmd rg

if [[ -z "${CELLC_BIN:-}" ]]; then
    run cargo build --locked --bin cellc
    export CELLC_BIN="$CARGO_TARGET_DIR/debug/cellc"
fi

run cargo test --locked -p cellscript --test v0_14 -- --test-threads=1
run cargo test --locked -p cellscript --test fuzzy_debug -- --test-threads=1

require_doc_boundary roadmap/CELLSCRIPT_0_14_ROADMAP.md 'v0.14 does not ship a source-level `max_cycles` spawn parameter'
require_doc_boundary roadmap/CELLSCRIPT_0_14_ROADMAP.md 'dedicated accepted/rejected CKB transaction fixture matrices to the later standard compatibility suite'
require_doc_boundary docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES_DRAFT.md 'Action Builder, CellFabric, CCC integration, or automatic transaction'
require_doc_boundary docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES_DRAFT.md 'a portable target profile; `ckb` is the implemented release profile'
require_doc_boundary docs/CELLSCRIPT_0_14_SUPER_AUDIT.md 'Stable-ready after docs downgrade'
require_doc_boundary docs/releases/CELLSCRIPT_0_14_COMMUNITY_UPDATE.md 'Spawn/IPC is bounded verifier reuse, not full protocol composability'
require_doc_boundary README.md '0.14 community update'
require_doc_boundary README_CH.md '0.14 community update'

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

python3 - "$OUT_DIR" "${metadata_files[@]}" <<'PY'
import json
import sys
from pathlib import Path

out_dir = Path(sys.argv[1])
paths = [Path(path) for path in sys.argv[2:]]

def fail(message):
    raise SystemExit(f"0.14 scope metadata oracle failed: {message}")

def require(condition, message):
    if not condition:
        fail(message)

def collect_accesses(metadata):
    accesses = list(metadata.get("runtime", {}).get("ckb_runtime_accesses", []))
    for entry in metadata.get("actions", []):
        accesses.extend(entry.get("ckb_runtime_accesses", []))
    for entry in metadata.get("locks", []):
        accesses.extend(entry.get("ckb_runtime_accesses", []))
    return accesses

def collect_create_set(metadata):
    create_set = []
    for entry in metadata.get("actions", []):
        create_set.extend(entry.get("create_set", []))
    for entry in metadata.get("locks", []):
        create_set.extend(entry.get("create_set", []))
    return create_set

require(len(paths) == 7, f"expected 7 v0.14 language metadata files, got {len(paths)}")

features = set()
operations = set()
script_reference_purposes = set()
capacity_floor_types = set()
has_type_id_plan = False
has_output_data_binding = False
metadata_names = []

for path in paths:
    require(path.exists(), f"missing metadata file {path}")
    metadata = json.loads(path.read_text())
    metadata_names.append(path.name)
    target_profile = metadata.get("target_profile", {})
    require(target_profile.get("name") == "ckb", f"{path} did not compile under ckb profile")
    require(target_profile.get("source_encoding") == "ckb-source-group-high-bit", f"{path} missing CKB Source encoding")
    require(target_profile.get("witness_abi") == "ckb-molecule-witness-args+cellscript-entry-witness-v1", f"{path} missing WitnessArgs ABI")
    require(target_profile.get("spawn_ipc_abi") == "ckb-vm-v2-spawn-ipc-syscalls-2601-2608", f"{path} missing Spawn/IPC ABI")
    require(target_profile.get("output_data_abi") == "ckb-outputs-and-outputs-data-index-aligned", f"{path} missing outputs_data ABI")
    require(target_profile.get("type_id_abi") == "ckb-type-id-v1", f"{path} missing TYPE_ID ABI")
    require(metadata.get("artifact_hash"), f"{path} missing artifact hash")
    require(metadata.get("artifact_size_bytes", 0) > 0, f"{path} missing artifact size")

    ckb_constraints = metadata.get("constraints", {}).get("ckb")
    require(isinstance(ckb_constraints, dict), f"{path} missing constraints.ckb")
    abi = ckb_constraints.get("profile_abi_contract", {})
    require(abi.get("witness_abi") == target_profile.get("witness_abi"), f"{path} profile ABI witness drift")
    require(abi.get("output_data_abi") == target_profile.get("output_data_abi"), f"{path} profile ABI output_data drift")

    features.update(metadata.get("runtime", {}).get("ckb_runtime_features", []))
    for access in collect_accesses(metadata):
        operations.add(access.get("operation"))
    for reference in ckb_constraints.get("script_references", []):
        script_reference_purposes.add(reference.get("purpose"))
        if reference.get("purpose") == "spawn-target":
            require(reference.get("dep_source") == "CellDep-or-DepGroup", f"{path} spawn target dep_source overclaimed")
            require(reference.get("status") == "runtime-required-builder-resolved", f"{path} spawn target status drift")
            require(reference.get("code_hash") is None and reference.get("hash_type") is None and reference.get("args") is None, f"{path} spawn target must remain builder-resolved")
    for floor in ckb_constraints.get("declared_capacity_floors", []):
        capacity_floor_types.add(floor.get("type_name"))
        require(floor.get("source") == "dsl-with_capacity_floor", f"{path} capacity floor source drift")
        require(floor.get("shannons", 0) > 0, f"{path} non-positive capacity floor")
    for create in collect_create_set(metadata):
        has_type_id_plan = has_type_id_plan or create.get("ckb_type_id") is not None
        has_output_data_binding = has_output_data_binding or create.get("ckb_output_data") is not None

required_features = {
    "ckb-spawn-ipc",
    "ckb-source-view",
    "ckb-witness-args",
    "ckb-lock-args",
    "ckb-sighash-all",
    "ckb-declarative-since",
    "ckb-declarative-capacity",
    "ckb-blake2b",
}
missing_features = sorted(required_features - features)
require(not missing_features, f"missing runtime features: {missing_features}")

required_operations = {
    "spawn",
    "wait",
    "pipe",
    "pipe-write",
    "pipe-read",
    "close-fd",
    "source-group-input",
    "witness-lock",
    "lock-args",
    "sighash-all",
    "require-maturity",
    "require-time",
    "require-epoch-after",
    "require-epoch-relative",
    "occupied-capacity",
    "hash-blake2b",
}
missing_operations = sorted(required_operations - operations)
require(not missing_operations, f"missing runtime operations: {missing_operations}")

require("spawn-target" in script_reference_purposes, "missing spawn target script-reference obligation")
require("type-id-create-output" in script_reference_purposes, "missing TYPE_ID create script-reference obligation")
require("TimedToken" in capacity_floor_types, "missing TimedToken capacity floor")
require(has_type_id_plan, "missing TYPE_ID output plan in language examples")
require(has_output_data_binding, "missing outputs_data binding in language examples")

report = {
    "status": "passed",
    "metadata_files": metadata_names,
    "features": sorted(features),
    "operations": sorted(operation for operation in operations if operation),
    "script_reference_purposes": sorted(purpose for purpose in script_reference_purposes if purpose),
    "capacity_floor_types": sorted(kind for kind in capacity_floor_types if kind),
}
report_path = out_dir / "cellscript-0-14-scope-audit-report.json"
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
print(f"valid CellScript 0.14 scope audit: {report_path}")
PY

printf '\nCellScript 0.14 scope audit passed: %s\n' "$OUT_DIR"
