# CellScript Gate Policy

CellScript uses one top-level gate entry point:

```bash
./scripts/cellscript_gate.sh <dev|ci|backend|release>
```

The lower-level audit scripts remain available for focused debugging, but they
are implementation details of the gate policy. Prefer the unified gate when
deciding whether a change is ready.

## Gate Modes

| Mode | When to run | Evidence boundary |
|---|---|---|
| `dev` | Local development before pushing | Formatting, Rust check, strict backend quick audit, syntax-combination quick audit, whitespace diff check |
| `ci` | Pull requests, pushes, and routine merge readiness | Full Rust tests, clippy, strict backend CI audit, syntax-combination CI audit through the strict backend runner, package verification, script syntax checks |
| `backend` | Changes touching IR, codegen, assembler, ABI, ELF, or RISC-V behavior | Full Rust tests, clippy, and strict backend full audit, including stateful CKB scenarios |
| `release` | Nightly/stable release candidates and any production CKB claim | `ci` plus tooling/docs boundary checks, VS Code validation, builder-backed CKB production acceptance, and stateful scenario/action coverage |

`release-quick` is an internal compatibility mode used by
`scripts/cellscript_ckb_release_gate.sh quick`; it runs the unified CI gate plus
compile-only production acceptance.

## Command Cheatsheet

```bash
# Local fast path
./scripts/cellscript_gate.sh dev

# Default CI/PR gate
./scripts/cellscript_gate.sh ci

# Strict compiler-contract gate for backend work
./scripts/cellscript_gate.sh backend

# Release-facing CKB production gate
./scripts/cellscript_gate.sh release
```

The old release wrapper remains supported:

```bash
./scripts/cellscript_ckb_release_gate.sh quick  # delegates to cellscript_gate.sh release-quick
./scripts/cellscript_ckb_release_gate.sh full   # delegates to cellscript_gate.sh release
```

## Lower-Level Components

Use these only when you need a focused failure:

```bash
./scripts/cellscript_syntax_combo_audit.sh quick
./scripts/cellscript_syntax_combo_audit.sh ci
./scripts/cellscript_strict_backend_audit.sh quick
./scripts/cellscript_strict_backend_audit.sh ci
./scripts/cellscript_strict_backend_audit.sh full
./scripts/ckb_cellscript_acceptance.sh --production --stateful-scenarios
./scripts/cellscript_0_14_scope_audit.sh
```

Passing one component does not imply the corresponding higher-level gate passed.
For example, CKB acceptance proves selected transaction behavior, while the
syntax-combination and strict backend audits prove compiler-layer edge cases and
structural invariants.

## Artifact Reports

The gates write machine-readable reports under `target/`:

- `target/syntax-combo-audit/`
- `target/cellscript-strict-backend-audit/`
- `target/ckb-cellscript-acceptance/`
- `target/cellscript-backend-shape/`
- `target/cellscript-schema-manifest/`

For release evidence, keep the JSON report paths in the release checklist rather
than copying long logs into review threads.

## CellScript Build Report

`scripts/ckb_cellscript_acceptance.sh --production` emits
`cellscript_build_reports` inside `target/ckb-cellscript-acceptance/` reports.
This is the exact-artifact bridge between compiler output, ELF ABI evidence,
and live CKB code-cell evidence. It does not replace the acceptance report,
production gate, or ELF entry ABI gate; it binds their artifact identities
together.

The top-level index is:

```text
cellscript_build_reports {
  schema = "cellscript-ckb-build-report-index-v0.20"
  status = "passed"
  artifact_count
  target_profile = "ckb"
  vm_profile = "ckb-vm"
  artifact_format = "riscv64-elf"
  artifact_hash_algorithm = "ckb-blake2b256"
  requires_exact_artifact_hash = true
  requires_elf_entry_abi_gate = true
  requires_live_code_cell_data_hash_match = true
  reports = [CellScriptBuildReport]
}
```

Each `CellScriptBuildReport` row records:

```text
CellScriptBuildReport {
  schema = "cellscript-ckb-build-report-v0.20"
  name
  kind
  source
  original_source
  example
  entry_flag
  entry
  target_profile = "ckb"
  vm_profile = "ckb-vm"
  artifact_format = "riscv64-elf"
  artifact_path
  metadata_sidecar
  artifact_packaging
  artifact_size_bytes
  artifact_hash_algorithm = "ckb-blake2b256"
  deployable_elf_hash
  artifact_sha256
  deployment_hash_type_used_by_gate = "data1"
  verify_artifact_status = "passed"
  verify_target_profile = "ckb"
  elf_entry_abi_status = "passed"
  abi_trailer_stripped = true
  onchain_deployments
}
```

For full devnet acceptance, every row must have at least one
`onchain_deployments` entry whose `live_code_cell_data_hash` equals
`deployable_elf_hash`. Compile-only production evidence keeps
`onchain_deployments` empty and is therefore not external release evidence.

Package identity must carry the same codec boundary explicitly. `Cell.lock`
`[package.build]`, `Deployed.toml` `[build]`, deployment records, and generated
builder identity checks include `cell_data_codec_manifest_hash` alongside
`artifact_hash`, `metadata_hash`, `schema_hash`, `abi_hash`, and
`constraints_hash`. Registry and builder verification fail closed when this
hash is missing or disagrees, so raw cell-data layouts cannot be hidden behind a
Molecule-only schema identity.
