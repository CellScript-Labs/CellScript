# CellScript Gate Policy

CellScript uses one top-level gate entry point:

```bash
./scripts/cellscript_gate.sh <dev|ci|backend|release|release-quick>
```

The lower-level audit scripts remain available for focused debugging, but they
are implementation details of the gate policy. Prefer the unified gate when
deciding whether a change is ready.

## Gate Modes

| Mode | When to run | Evidence boundary |
|---|---|---|
| `dev` | Local development before pushing | Formatting, Rust check, strict backend quick audit, syntax-combination quick audit, skill-pack freshness, README-linked CellScript doc Status freshness, local markdown link check, whitespace diff check |
| `ci` | Pull requests, pushes, and routine merge readiness | Full Rust tests, clippy, strict backend CI audit, syntax-combination CI audit through the strict backend runner, package verification, skill-pack and CellScript doc Status freshness, local markdown link check, script syntax checks |
| `backend` | Changes touching IR, codegen, assembler, ABI, ELF, or RISC-V behavior | Full Rust tests, clippy, and strict backend full audit, including stateful CKB scenarios |
| `release` | Nightly/stable release candidates and any production CKB claim | `ci` plus tooling/docs boundary checks, VS Code validation, builder-backed CKB production acceptance, and stateful scenario/action coverage |
| `release-quick` | Wrapper compatibility and local compile-only preflight | `ci` plus compile-only production acceptance; not external live/devnet evidence |

`release-quick` is kept for `scripts/cellscript_ckb_release_gate.sh quick`.
Use `release` for any production or external live/devnet claim.

### Nightly 0.22 compiler evidence

The `nightly-0.22` line adds compile-time callable-effect contracts and
transaction-local terminal-flow evidence. These remain inside the existing gate
modes; they do not create a new gate command:

- `dev` and `ci` reject underdeclared `fn` effects, including transitive calls
  through source-authenticated package imports.
- invariant `reads` and aggregate operands share the closed `SourceView` /
  typed-target model; parser, type checking, IR, ProofPlan, formatter, and
  xUDT helper selection no longer reparse source-view strings independently;
- canonical 0.22 flows use an enum-backed state field, exactly one `initial`
  state, at least one `terminal` state, and no outgoing terminal edge;
- terminal discharge is currently only `terminal-by-output-state`, backed by
  generated state-transition checks and emitted as `checked-runtime` ProofPlan
  evidence;
- every ProofPlan record carries exactly one evidence tier:
  `checked-static`, `checked-runtime`, `runtime-helper-required`,
  `builder-evidence-required`, `metadata-only`, or
  `chain-evidence-required`;
- `--production` rejects `metadata-only` records whose invariant, terminal, or
  assert/check/enforce/require/validate/verify naming claims executable
  enforcement;
- legacy flows without initial/terminal declarations and numeric state fields
  remain accepted for migration, but metadata carries explicit audit warnings;
- none of this metadata proves that every live on-chain Cell eventually reaches
  a terminal state. `release` still requires exact-artifact and chain evidence
  before a production claim.

Metadata schema `47` carries declared/inferred/effective function effects, the
initial, terminal, discharge, state-model, and audit-warning fields for flows,
the canonical evidence tier on ProofPlan, flow, and function metadata, and
typed transaction-view handle records under
`runtime.transaction_view_handles`. Handle records must remain
`ownership = read-only-view`, carry `lifecycle_authority = false`, and report
checked-static typing plus checked-runtime read evidence.
Consumers must reject unsupported schema versions instead of silently dropping
these fields.

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

# Compile-only release preflight; not external live/devnet evidence
./scripts/cellscript_gate.sh release-quick
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
```

`./scripts/cellscript_0_14_scope_audit.sh` is a historical standalone audit
from the 0.14 release line. It is not invoked by any current gate mode and is
retained for manual 0.14-compat debugging only; it is not part of the 0.21
release-evidence boundary.

The following ecosystem/bridge scripts are standalone manual tools that are
**not** wired into any gate mode and are **not** part of the release-evidence
boundary. They require sibling checkouts (`../ckb`, `../CellFabric`) or external
runtimes and are documented in their respective guides for focused, opt-in use:

- `./scripts/cellscript_ckb_ecosystem_reuse_gate.sh` — CKB-ecosystem reuse
  checks; see `docs/CELLSCRIPT_CKB_ADAPTER.md`.
- `./scripts/cellscript_ckb_adapter_acceptance.sh` — adapter acceptance against
  a sibling CKB checkout; see `docs/CELLSCRIPT_CKB_STD_COMPAT.md`.
- `./scripts/cellscript_cellfabric_bridge_smoke.sh` — CellFabric bridge smoke
  test; see `docs/CELLSCRIPT_CELLFABRIC_BRIDGE.md`.

These must not be described as gating evidence, and passing one does not imply
any release-gate mode passed.

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
