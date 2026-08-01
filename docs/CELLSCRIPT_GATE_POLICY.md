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
| `dev` | Local development before pushing | Rust formatting, canonical CellScript example formatting, all workspace-package Rust checks (including `cellscript-tools`) plus the independent Registry verifier crate, strict backend quick audit, syntax-combination quick audit, parity-gated skill-pack freshness, README-linked CellScript doc Status freshness, local markdown link check, whitespace diff check |
| `ci` | Pull requests, pushes, and routine merge readiness | Canonical CellScript example formatting; tests and clippy for the compiler, Fiber adapter, CKB adapter, WASM crate, CKB SDK builder example, `cellscript-tools`, and the Registry verifier; Registry API typecheck/tests, Node API/verifier bundles, and dry-run Worker build; strict backend CI audit; package verification; parity-gated skill-pack/doc freshness; local-link and script syntax checks |
| `backend` | Changes touching IR, codegen, assembler, ABI, ELF, or RISC-V behavior | Full Rust tests, clippy, and strict backend full audit, including stateful CKB scenarios |
| `release` | Nightly/stable release candidates and any production CKB claim | Clean tagged source plus `ci`, a fresh size-gated website WASM rebuild, tooling/docs and VS Code checks, pinned-CKB acceptance harnesses, public builder-contract generation, and mandatory stateful scenario/action coverage |
| `release-quick` | Wrapper compatibility and local compile-only preflight | `ci` plus compile-only production acceptance; not external live/devnet evidence |

`release-quick` is kept for `scripts/cellscript_ckb_release_gate.sh quick`.
Use `release` for any production or external live/devnet claim.

`dev` and `ci` run `cellc fmt --check` against
`examples/language/canonical_style.cell`. The formatter's comma-terminated
field form is the canonical checked-in surface; the parser may continue to
accept comma-free fields as compatibility input. The same modes reject raw
`u64` maximum and `MAX - delta` magic literals in the checked NFT, timelock,
atomic-swap, and multi-phase-DAO example pairs; boundary arithmetic must use
their local `U64_MAX` constants.

Both release modes fail before doing expensive work unless the CellScript tree
is completely clean, including untracked files. CI additionally requires the
exact `v<workspace-version>` tag at `HEAD`; a manual release dispatch must name
the same version as the root `[package].version`. The GitHub Release workflow
runs the full `release` gate first, and binary builds plus publication depend on
that job succeeding.

The 0.23 tooling migration is complete. `cellscript-tools` owns the backend,
syntax-combination, skill-pack, tooling-release, CKB production-evidence,
NovaSeal, and Evolving-DOB gate logic. Website data generation is implemented
by Node scripts in `website/scripts/`. Dev, CI, backend, and release gates have
no Python runtime dependency and reject tracked Python source files.

The 0.23 line also has one edition contract: every package declares
`edition = "2026"`, and all emitted evidence binds the resolved compatibility
profile. The edition owns source semantics only; target, primitive assurance,
metadata schemas, and entry/witness ABIs remain independent profile axes.
Missing/non-2026 editions and superseded lock, deployment, receipt, builder, or
raw-witness placement identities are rejected rather than migrated. See
[`CELLSCRIPT_EDITION_POLICY.md`](CELLSCRIPT_EDITION_POLICY.md). Edition-owned
source changes require complete frontend closure. Independently versioned ABI
changes require the `backend` gate in addition to ordinary `dev` and `ci`
coverage.

The `ci` gate also typechecks/tests `services/registry-api`, builds both Node
entrypoints, performs its Wrangler dry-run build, and runs tests and clippy for
the independent real-compiler Registry verifier crate. `dev` at least checks that
verifier crate. This pins the publish contract, additive queue migration,
worker boundary, and database/static-object shape to the compiler-generated
registry entry. It is local service coverage, not evidence
that Cloudflare, R2, Hyperdrive, Neon, DNS, or a production deployment works.
The CLI coverage includes the explicit first-publish admission sequence:
`cellc auth capability submit`, `cellc auth namespace claim`, then
`cellc publish`. Capability registration does not silently claim a namespace;
the claim response must be `active` before the write API accepts a version.
Registry API tests pin both accepted publisher roots: JoyID signatures under
`principal_type = joyid_ckb` and recoverable CKB message signatures under
`principal_type = ckb_secp256k1`. CLI fixtures use the generic
`--wallet-signature` surface; the former `--joyid-signature` spelling remains a
visible compatibility alias and does not define a second request shape.
Explicit `--allow-unverified` and `--allow-quarantined` install choices are
persisted per dependency so the lock refresh and later builds exercise the
same auditable resolver policy.

The full gate reads `scripts/ckb_acceptance_pin.json` and rejects a CKB checkout
whose revision or worktree differs from the pin. Its report binds the CKB
version string, executable SHA-256, source-template hashes, effective devnet
configuration hashes, and genesis hash. Production on-chain acceptance always
rebuilds CKB from that source in a fresh dedicated Cargo target directory and
archives the executable with the report; supplied or cached binaries cannot
satisfy the production gate. It then runs the exact stateful 43-action matrix
and validates every step's commit, spent-input liveness, live outputs, cycles,
serialized size, and occupied capacity. `--stateful-scenarios` remains only as
an explicit option for bounded runs.

The transaction matrix is produced by the native Rust acceptance harness and
is intentionally labelled as recipe-replayer evidence, not generated-builder
output. Separately, the gate runs the public `cellc action build` and
`cellc gen-builder` surfaces for every production action and hashes their
generated contracts. Resource Type Scripts in these local transactions remain
`always_success` fixtures; the report records that this proves verifier
behaviour and transaction shape, not a production passive-resource-identity
deployment.

### Fiber integration evidence

The no-profile Fiber path has a separate, non-gating acceptance entry point:

```bash
./scripts/cellscript_fiber_acceptance.sh --static
```

Static mode runs the dedicated CKB-VM transaction matrix, adapter tests, and
adapter clippy. It proves only compiler/artifact compatibility; it does not
prove that a Fiber node loaded configuration, advertised an asset, opened a
channel, routed a payment, or settled on chain.

Full mode consumes externally produced `compatibility.json` and
`acceptance.json` reports from a pinned Fiber checkout. It validates exact
revision/fingerprint bindings and requires every declared positive and negative
matrix row. Every completed row and certified topology report must cite a
non-empty regular file beneath an explicit evidence root together with its CKB
Blake2b-256 digest. Absolute paths, parent traversal, symlinks, missing files,
and digest mismatches fail closed. These bindings prove evidence-bundle
integrity, not who produced it. Fiber's native `node_info` exposes a seven-hex
build abbreviation, so full mode also checks the selected checkout's complete
40-hex HEAD. The script does not start, restart, configure, sign for, or stop
operator-owned CKB/Fiber nodes. Until the live matrix is stable and explicitly
promoted, neither `dev`, `ci`, `backend`, nor `release` runs this external
integration boundary.

The ordinary `dev`, `ci`, and `backend` gates do compile the adapter; `ci` and
`backend` also run its unit tests and clippy. This is workspace-code coverage,
not external Fiber lifecycle evidence.

On 2026-07-20, bounded non-gating local-devnet runs passed Fiber's official
`udt-router-pay` and
`watchtower/force-close-with-pending-tlcs-and-udt` collections with the exact
CellScript artifact and generated native configuration. Those observations are
recorded in the roadmap, but do not satisfy full mode because the CKB executable
and Fiber source/build were observed only in a bounded local fixture, no signed
announcement report was captured, and the complete declared matrix was not
produced.

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

Metadata schema `54` carries declared/inferred/effective function effects, the
initial, terminal, discharge, state-model, and audit-warning fields for flows,
the canonical evidence tier on ProofPlan, flow, and function metadata, and
typed transaction-view handle records under
`runtime.transaction_view_handles`, `runtime.borrow_regions`,
`runtime.capability_proofs`, plus
`types[].validity_predicates`. Handle records must remain
`ownership = read-only-view`, carry `lifecycle_authority = false`, and report
checked-static typing plus checked-runtime read evidence.
Consumers must reject unsupported schema versions instead of silently dropping
these fields.

Schema 55 additionally carries
`runtime.fungible_type_group_entry`. That record is present only for the
dedicated, payload-free `fungible-type-group-v1` compilation path and binds the
selected type, 16-byte field, runtime helper, witness policy, the legacy
32-byte input-Lock authority and tagged 33-byte input-Type-Script authority,
and the unauthorised non-empty/conservation contract.
Ordinary action compilation must not emit it.

Concrete payload enum evidence is top-level under `enum_layouts`. Every record
pins the one-byte tag, packed variant field offsets, encoded width, storage
class, ownership, and ABI. Non-linear values use fixed bytes; pure-helper
returns up to 16 bytes use the `a0`/`a1` register-pair ABI. Enums containing a
Cell payload are local-only linear handles and cannot cross storage or entry
ABI boundaries. Dynamic, recursive, and generic payload ADTs fail before IR.
Quick syntax coverage pins exhaustive matching, dynamic rejection, and
arm-local linear discharge through the three `SCA-BUG-0.22-PAYLOAD-*` classes.

ProtocolGraph participant roles remain a derived audit view, never a verifier
or authorization condition. `actions[].protocol_role_candidates` preserves
the source of each candidate. A direct Address equality in a verification
predicate wins over witness/entry-witness or `lock_args` bindings, which win
over participant-like Address field names. Every candidate must carry
`evidence_tier = metadata-only` and `authorization_proven = false`; the
metadata validator rejects a `protocol-role` ProofPlan category. Graph edges
publish `role_source_used`, all candidates, and deterministic
`PG-ROLE-MISSING`, `PG-ROLE-WEAK-FIELD`, or `PG-ROLE-CONFLICT` lints. The quick
syntax gate pins the overclaim and conflict boundaries with the two
`SCA-BUG-0.22-PROTOCOLGRAPH-*` classes.

The top-level `capability_registry` is a closed, versioned audit contract.
Every `types[]` record carries the matching `capability_set_version`.
Composite operations emit `runtime.capability_proofs` with required, provided,
entailed, missing, identity-condition, capability-set-version, and
entailment-version fields. `destroy` is accepted by `consume + burn` (or a
labelled legacy compatibility alternative); `replace_unique` requires
`replace + identity-preservation`. Gates reject missing authority and any
attempt to borrow authority from another container/resource type. Quick syntax
coverage pins this with `SCA-BUG-0.22-CAPABILITY-OVERGRANT` and
`SCA-BUG-0.22-CAPABILITY-TRANSITIVE-GRANT`.

Bounded invariant quantifiers are finite-source declarations. Their ProofPlan
records use `bounded-source-quantifier`, identify the closed source view, and
record scan complexity, field reads, runtime cardinality, vacuous `forall`
status, and `u64` count overflow policy. Until a selected entry emits the named
bounded scan helper, their tier is `runtime-helper-required`, never
`checked-runtime` or `metadata-only`.

The explicit `ckb::require_bounded_cell_dep_data_hash` operation is a narrower
checked-runtime exception, not an automatic promotion of arbitrary
quantifiers. It has a compile-time `1..=64` bound, emits a real resolved
`Source::CellDep` `LOAD_CELL_BY_FIELD(DATA_HASH)` loop, and is covered by
positive/missing-dependency CKB-VM cases. Out point, dep type, and original
DepGroup identity remain manifest/builder evidence.

Bounded Cell collections use the same finite-evidence rule. An action may take
`input cells: BoundedCellSet<T, N>` and discharge its linear ownership exactly
once with `consume_each`. A fixed-width witness plan may use
`witness plans: BoundedList<P, N>` and exactly one `create` template per plan
element through `create_each`. `runtime.collection_instantiations` records the
explicit source, maximum cardinality, runtime-cardinality placeholder, vacuous
zero policy, and ownership. Consume iteration remains
`runtime-helper-required`; create iteration additionally emits
`builder-evidence-required` output-count and capacity obligations. The quick
syntax gate covers missing bounds, duplicate consumption, and
`Vec<Resource>` rejection.

Type validity uses one evidence contract across parser, type checking, IR,
metadata, ProofPlan, codegen, formatter, and LSP. A pure field predicate is
`checked-runtime` only when a concrete constructor/create instruction emits a
fail-closed guard before the output instruction on every selected create path.
`create_paths_selected` and `create_paths_checked` make partial coverage
auditable; partial, signature-only, or update paths without that lowering are
`runtime-helper-required`, and production gates must not promote them. Literal
`true` predicates may be `checked-static`.
`env::block_number()` is never treated as a compiler constant or an ambient
CKB-VM syscall: its record names both
`environment:env::block_number` and
`builder:header-dep-block-number-evidence`, with
`builder-evidence-required`. Every other `env::*` read fails closed. The quick
syntax audit requires positive evidence records and the unknown-environment
negative seed through `SCA-BUG-0.22-VALIDITY-EVIDENCE-MISSING` and
`SCA-BUG-0.22-VALIDITY-ENV-UNKNOWN`.

Explicit borrow blocks are a checked-static compiler contract. Each
`runtime.borrow_regions` record must use canonical `View<T>`, declare
`storage = none` and `abi = none`, and allow only `Pure` and `ReadOnly`
callees with a dedicated `&T` parameter. The matching `borrow-region`
ProofPlan entry records escape and root-lifecycle rejection. Quick syntax
coverage pins effect compatibility, escape rejection, and crossing
`consume`/`destroy` through the three `SCA-BUG-0.22-BORROW-*` classes.

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

For scripted gate wrappers, the global `--json` flag selects one command result
on stdout for either success or failure. Structured failures carry their
category and exit code in addition to source ranges and diagnostic codes.
`--message-format=json` remains a hidden deprecated alias for compatibility.

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
