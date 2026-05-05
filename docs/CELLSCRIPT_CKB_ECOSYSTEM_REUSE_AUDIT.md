# CellScript CKB Ecosystem Reuse Audit

**Status**: 0.19 scope implementation landed for `ckb-std` ABI parity,
inline runtime alignment, the formal headless CKB adapter crate, and focused
local-node adapter acceptance.

**Audit date**: 2026-05-05.

This document records where CellScript is correctly reusing the CKB ecosystem,
where overlap is an acceptable compiler boundary, and where the project is at
risk of maintaining duplicate infrastructure that should belong to `ckb-std`,
`ckb-sdk-rust`, or `cellscript-ckb-adapter`.

This is not 0.18 protocol-equivalence evidence. It is a 0.19 planning contract
for the registry, deployment, adapter, and Action Builder layer.

## Summary

CellScript is not broadly replacing `ckb-sdk-rust`.

The current `solve-tx`, `deploy-plan`, `lock-deps`, and `action build` surfaces
mostly emit metadata, intent, evidence schemas, and unresolved transaction
plans. That is appropriate compiler output. They do not perform live-cell
selection, CellDep/HeaderDep resolution, fee/change calculation, signing,
tx-pool acceptance, or submission.

`cellc action build --json` now makes that boundary machine-readable through an
adapter contract and packed-materialization requirements, while still marking
the draft as not submittable and not CKB-VM executed.

The real duplication risk is on the contract-side CKB runtime boundary:

```text
ckb-std owns the contract-side ABI vocabulary.
ckb-sdk-rust owns off-chain transaction realisation.
CellScript should own semantic compilation and generated verifier intent.
```

CellScript may still emit low-level RISC-V helpers because it currently
generates self-contained artifacts. That is implementation duplication, not a
separate semantic standard. Runtime observation semantics must stay aligned
with `ckb-std` and CKB VM behavior.

## Findings

| Area | Current CellScript behavior | Ecosystem owner | Risk | Decision |
|---|---|---|---|---|
| Transaction construction | Emits `solve-tx` template and `action build` plan only | `ckb-sdk-rust` / adapter | Low | Keep as intent output; do not promote to builder. |
| Live-cell selection | Not implemented in compiler | `ckb-sdk-rust` cell collectors | Low | Keep out of compiler core. |
| CellDep/HeaderDep resolution | Emits metadata slots and unresolved deps | `ckb-sdk-rust` resolvers / adapter | Low | Keep compiler output declarative. |
| Signing and lock unlocking | Emits explicit signer/witness requirements | `ckb-sdk-rust` signers and wallets | Low | Keep signer authority outside compiler. |
| RPC acceptance | Not implemented by compiler | CKB node via `ckb-sdk-rust` RPC | Low | Adapter must run `estimate_cycles`, `test_tx_pool_accept`, and optional `send_transaction`. |
| CKB syscall/source constants | Centralized in `src/ckb_abi.rs`, consumed by codegen and generated stdlib | `ckb-std::ckb_constants` | Low | Keep inline ABI table parity-tested against `ckb-std`; Rust backend should import `ckb-std` constants. |
| WitnessArgs parsing | Hand-written RISC-V parser with `ckb-types` layout fixtures | `ckb-std` / `ckb-types` for `WitnessArgs`; CellScript for `CSARGv1` | Medium | Keep inline parser only as implementation duplication; Rust backend should use `ckb-std` loaders and keep CellScript-specific payload decoding. |
| TYPE_ID evidence | Emits builder plans and metadata validation | `ckb-std::type_id`, adapter, SDK | Medium | Keep metadata plan; test against `ckb-std` semantics and adapter outputs. |
| Since/epoch encoding | Hand-written helpers with `ckb-std::since` parity tests | `ckb-std::since` | Low | Keep compiler helpers and parity tests. |
| Occupied capacity | Inline helper reads CKB `CellField::OccupiedCapacity` through `LOAD_CELL_BY_FIELD` | CKB `CellField::OccupiedCapacity`, `ckb-std`, `ckb-types` | Low | Rust backend should use `load_cell_occupied_capacity`; inline backend now uses the same field id. |
| Generated syscall stdlib | `StdLib::generate_syscalls` now uses the same `src/ckb_abi.rs` table as codegen | `ckb-std` and main codegen runtime helpers | Medium | Keep internal/debug-only until removed or replaced by generated shim output. |

## Safe Boundaries

These CellScript surfaces are not duplicate transaction infrastructure:

- `cellc action build` is a semantic action plan, not a transaction builder.
- `cellc deploy-plan` is a deployment intent and manifest seed, not a code-cell
  deployment transaction.
- `cellc lock-deps` is a dependency declaration surface, not a resolver.
- `cellc solve-tx` is a debugging template and should remain explicitly
  non-submittable.
- `cellc entry-witness` emits CellScript entry payload bytes, not final CKB
  `WitnessArgs` placement or lock signatures.
- `cellc validate-tx` validates metadata and builder evidence, not CKB VM,
  consensus, live-cell availability, cycles, or tx-pool acceptance.

This division is correct. `crates/cellscript-ckb-adapter` consumes these
outputs and uses `ckb-sdk-rust` for the chain-facing materialization boundary.

## Runtime Policy

`ckb-std` is not a codegen layer. It is the contract-side runtime ABI and helper
library for Rust contracts. CellScript still owns codegen because only
CellScript knows its AST, action wrapper, `CSARGv1` payload, resource schema,
transition obligations, generated errors, metadata, and evidence model.

The CKB backend should make runtime reuse explicit:

| Runtime policy | Meaning | Correct reuse |
|---|---|---|
| `ckb_backend_runtime = "ckb-std"` | Generated Rust verifier or shim source observes CKB through `ckb-std` | Use `ckb_std::high_level`, constants, TYPE_ID, since, occupied-capacity, exec/spawn helpers |
| `ckb_backend_runtime = "inline"` | Self-contained RISC-V/ELF verifier observes CKB through emitted syscall wrappers | Keep helpers small, generated, documented, and parity-tested against `ckb-std` |

The current RISC-V/ELF path is inline mode. That mode is valid, but it is an
artifact strategy. It must not become a second runtime standard. If a Rust
verifier/shim backend is added, `ckb-std` mode should be preferred for ordinary
CKB workflows, while inline mode remains available for self-contained output,
bootstrap, special profiles, or size/cycle-sensitive artifacts.

The rule for wheel avoidance is:

```text
Reuse ckb-std for observing CKB.
Generate CellScript code for enforcing CellScript semantics.
```

## Duplicate Runtime Constants

CellScript now keeps CKB syscall numbers, source values, field ids, and since
flags in `src/ckb_abi.rs`. `src/codegen/mod.rs` and
`StdLib::generate_syscalls` consume that table.

This is acceptable only as inline-backend implementation duplication. The
current RISC-V/ELF output cannot call a Rust `ckb-std` function at runtime, but
the semantics still belong to `ckb-std`'s authoritative contract-side
constants:

```text
ckb-std/src/ckb_constants.rs
```

Implemented mitigation:

- constant parity tests for syscall numbers;
- parity tests for `Source::{Input, Output, CellDep, HeaderDep,
  GroupInput, GroupOutput}`;
- parity tests for `CellField`, `HeaderField`, and `InputField`;
- parity tests for since encoding used by CellScript epoch helpers;
- `SourceView` encode/decode tests proving decoded CKB source values match
  `ckb-std`.
- `cellc ckb-std-compat --json` emits the ABI source, runtime policy,
  compatibility evidence, and adapter boundary as a machine-readable report.

Remaining rule:

- keep CellScript's source-level `SourceView` encoding documented as
  CellScript ABI, while proving its decoded CKB source values match `ckb-std`.

If adding `ckb-std` as a normal dependency is too heavy, use it as a dev-dep or
generate a checked compatibility table in tests. Do not rely on comments alone.
A future Rust verifier/shim backend should avoid this duplication by importing
the `ckb-std` constants directly.

## Occupied Capacity

This is the clearest repeated wheel.

`ckb-std` exposes `CellField::OccupiedCapacity = 6` and
`high_level::load_cell_occupied_capacity`. `ckb-types` and `ckb-sdk-rust` also
use packed `CellOutput::occupied_capacity(...)` for builder-side capacity
measurement.

CellScript previously recomputed occupied capacity from lock/type/data byte
lengths. That path has been retired from the inline helper. The inline backend
now reads `CellField::OccupiedCapacity` with `LOAD_CELL_BY_FIELD`, matching
`ckb-std::high_level::load_cell_occupied_capacity`.

Required mitigation:

- in a Rust verifier/shim backend, use `ckb_std::high_level::load_cell_occupied_capacity`;
- in the inline backend, use `LOAD_CELL_BY_FIELD` with
  `CellField::OccupiedCapacity`;
- keep the retired multi-syscall computation out of production claims;
- add fixtures proving `ckb-types` packed occupied-capacity measurement agrees
  with the CKB field contract;
- make the adapter use `ckb-types` / `ckb-sdk-rust` packed capacity APIs for
  final output capacity and under-capacity rejection;
- keep compiler metadata limited to capacity floors and evidence requirements.

The compiler may say "capacity planning is required". It must not claim final
capacity correctness without builder or node evidence.

## Generated Stdlib Syscalls

`StdLib::generate_syscalls` emits another assembly syscall wrapper surface,
separate from both `ckb-std` and CellScript's main codegen runtime helpers.

That is the highest-maintenance overlap:

- it repeats CKB syscall numbers;
- it repeats helper symbols already handled by main codegen;
- it can drift from the actual codegen ABI;
- it looks like a standalone CKB runtime library, which is not CellScript's
  role.

Implemented mitigation:

- generated syscall wrappers use the same `src/ckb_abi.rs` table used by
  `src/codegen/mod.rs`;
- tests assert the generated stdlib surface contains the same syscall values
  and no longer uses the old GroupInput pseudo-value.

Remaining mitigation:

- deprecate or remove standalone generated syscall wrappers if they stop being
  useful as internal/debug output.

The long-term target is one CKB ABI source of truth inside CellScript, tested
against `ckb-std`.

## WitnessArgs

There are two separate layers here:

```text
WitnessArgs layout and loading: ckb-std / ckb-types
CSARGv1 payload and action ABI: CellScript
```

CellScript hand-parses Molecule `WitnessArgs` in generated RISC-V for
witness-field helpers. That duplicates `ckb-std::high_level::load_witness_args`
and `ckb-types` reader validation, but only as inline-backend implementation
duplication: current self-contained artifacts cannot call a Rust `ckb-std`
function at runtime.

A Rust verifier/shim backend should use `ckb-std` to load `WitnessArgs`, then
let generated CellScript code decode and validate the `CSARGv1` payload and
action-specific arguments.

Implemented mitigation:

- differential fixtures against `ckb-types::packed::WitnessArgs`;
- coverage for valid fields, `BytesOpt::None`, short tables,
  non-monotonic offsets, offsets beyond total size, and trailing bytes;

Remaining mitigation:

- **Done as machine-readable policy**: `cellc ckb-std-compat --json` and
  `cellc action build --json` expose the entry payload ABI, adapter-owned final
  placement, default action `input_type` placement, and do-not-overwrite lock
  signature policy;
- treat `CSARGv1` decoding as CellScript-specific ABI, not as a `ckb-std`
  responsibility;
- keep final witness placement in the adapter, not compiler core.

The rule is:

```text
ckb-std owns WitnessArgs observation semantics.
CellScript owns entry payload bytes and CSARGv1 decoding.
The adapter owns final WitnessArgs placement.
```

## TYPE_ID

CellScript's TYPE_ID metadata and builder plans overlap with `ckb-std`'s
`type_id::validate_type_id` semantics.

That overlap is acceptable if CellScript only emits intent:

- type identity metadata;
- output-index requirements;
- expected args evidence;
- deployment and action plans that force the builder to provide concrete
  first-input and output-index evidence.

It becomes a repeated wheel if the compiler claims to have validated a real
TYPE_ID transaction without the builder or CKB VM.

Required mitigation:

- keep TYPE_ID creation evidence builder-owned;
- test CellScript TYPE_ID fixtures against `ckb-std` create, transfer,
  duplicate, and burn semantics;
- make the adapter compute and check TYPE_ID args using CKB packed input and
  output data;
- keep `cellc validate-tx` as metadata/evidence validation only.

## Since And Epoch

CellScript has CKB epoch since helpers and DAO maturity helpers. These overlap
with `ckb-std::since`.

This is acceptable in generated verifier code, but needs parity tests:

- absolute epoch encoding;
- relative epoch encoding;
- malformed flags;
- malformed epoch fraction shape;
- maturity success and immature rejection.

Builder workflows must still set the concrete input `since` field and any
required HeaderDeps. That belongs to the adapter and `ckb-sdk-rust`.

## `validate-tx` Naming Risk

`cellc validate-tx` validates a JSON transaction shape against CellScript
metadata and required builder evidence. It does not:

- execute CKB VM;
- run CKB consensus validation;
- select live cells;
- check cell availability;
- estimate cycles;
- test tx-pool acceptance;
- submit a transaction.

The implementation should keep this scope, but user-facing language should
avoid implying node acceptance.

Recommended wording:

```text
cellc validate-tx performs CellScript metadata/evidence validation.
For CKB acceptance, use the adapter with ckb-sdk-rust estimate_cycles and
test_tx_pool_accept.
```

If a future CLI keeps the name `validate-tx`, its JSON output should include a
clear field such as:

```json
{
  "validation_level": "cellscript-metadata-evidence",
  "ckb_vm_execution": false,
  "tx_pool_acceptance": false
}
```

Implemented mitigation: `cellc validate-tx --json` now emits these evidence
boundary fields at the top level of the report.

## Low-Risk Utilities

`cellc ckb-hash` duplicates a small ecosystem utility, but the risk is low. It
uses the CKB default Blake2b personalization and is useful for artifact,
metadata, manifest, and release evidence workflows.

Keep it if:

- it stays a convenience command;
- test vectors remain pinned;
- it does not become a replacement for packed CKB hash APIs where packed
  transaction or script hashing is required.

## Adapter Ownership

`cellscript-ckb-adapter` absorbs the first reusable chain-reality boundary:

- action-plan parsing and schema checks;
- CKB packed transaction materialization;
- explicit CellDep/HeaderDep/output/witness assembly;
- occupied-capacity measurement and under-capacity rejection before RPC;
- final `WitnessArgs` placement helpers;
- TYPE_ID args and script construction helpers;
- `estimate_cycles`, `test_tx_pool_accept`, and optional `send_transaction`
  wrappers;
- machine-readable preview and acceptance reports.

Those are already available in `ckb-sdk-rust` through RPC clients, transaction
builders, input iterators, cell collectors, dep resolvers, signers, and packed
capacity APIs. CellScript should integrate with those APIs rather than grow
parallel infrastructure.

The checked-in cookbook lives at `examples/ckb-sdk-builder`, but it is now only
a wrapper around the formal crate. It must not become a second implementation.

## Prioritized Cleanup

### P0

1. **Done**: `tests/ckb_std_compat.rs` compares CKB syscall numbers, sources,
   field ids, and since encodings against `ckb-std`.
2. **Done**: inline occupied-capacity lowering uses
   `CellField::OccupiedCapacity`.
3. **Done for drift prevention**: `StdLib::generate_syscalls` uses the same
   `src/ckb_abi.rs` table as main codegen. Full removal/deprecation can remain
   a cleanup task if the generated stdlib command stops being useful.

### P1

1. **Done for layout parity**: `WitnessArgs` fixtures compare against
   `ckb-types::packed::WitnessArgs` and malformed table cases.
2. **Done for contract parity**: TYPE_ID lifecycle and args-hash tests are
   pinned to the `ckb-std::type_id` API contract.
3. **Done**: since/epoch parity fixtures cover valid and malformed
   `ckb-std::since` cases.
4. **Done**: `validate-tx` output/docs say metadata/evidence validation, not
   node acceptance.

### P2

1. **Done as formal headless crate**: `crates/cellscript-ckb-adapter` uses
   local `ckb-sdk-rust` for packed transaction materialization, capacity
   checks, CellDep/HeaderDep/output/witness assembly, TYPE_ID args checks,
   script construction, signer boundary types, and RPC acceptance/submission
   methods. `examples/ckb-sdk-builder` is now a cookbook wrapper.
2. **Done**: `cellc ckb-std-compat --json` emits a ckb-std compatibility
   report for CI and release evidence.
3. **Done as focused local-node evidence**:
   `scripts/cellscript_ckb_adapter_acceptance.sh` starts a local CKB devnet,
   checks a compiler action plan, verifies adapter materialization tests, and
   records `estimate_cycles` plus `test_tx_pool_accept` evidence in a JSON
   report.
4. Consider an optional generated Rust shim using
   `ckb_backend_runtime = "ckb-std"` for mixed Rust/CellScript projects.

Focused validation:

```text
./scripts/cellscript_ckb_ecosystem_reuse_gate.sh quick
./scripts/cellscript_ckb_ecosystem_reuse_gate.sh full
```

This gate is scoped to CKB ecosystem reuse and adapter boundary evidence. It is
not the package/deployment registry production gate.

Evidence is layered:

```text
compiler ABI parity          -> tests/ckb_std_compat.rs
adapter materialization      -> crates/cellscript-ckb-adapter tests
local CKB adapter acceptance -> scripts/cellscript_ckb_adapter_acceptance.sh
stateful business flows      -> scripts/ckb_cellscript_acceptance.sh
```

Known limitations remain explicit: no wallet UI, no CellFabric intent DAG, no
external audit claim, and no exhaustive adversarial state-space proof.

## Final Boundary

The mature split is:

```text
CellScript compiler:
  semantic artifacts, ABI, metadata, deploy plans, action plans, witness bytes,
  constraints, and evidence requirements.

CellScript CKB backend:
  ckb-std runtime mode for generated Rust verifier or shim source;
  inline runtime mode for current self-contained RISC-V/ELF output.

ckb-std:
  contract-side syscall, witness, source, field, TYPE_ID, since, exec/spawn,
  debug, and no-std Rust runtime vocabulary.

cellscript-ckb-adapter + ckb-sdk-rust:
  deployment, live cells, CellDeps, HeaderDeps, capacity, fees, signing,
  acceptance, submission, and reports.
```

CellScript should not be a second `ckb-std`, and it should not become a second
`ckb-sdk-rust`. Its strongest position is to emit precise semantic intent and
prove that the generated verifier artifacts stay compatible with the CKB
ecosystem's existing runtime and builder infrastructure.

In short:

```text
Reuse ckb-std for observing CKB.
Generate CellScript code for enforcing CellScript semantics.
Use ckb-sdk-rust for making accepted transactions real.
```
