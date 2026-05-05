# CellScript CKB Ecosystem Reuse Audit

**Status**: engineering audit of CellScript overlap with `ckb-std` and
`ckb-sdk-rust`.

**Audit date**: 2026-05-05.

This document records where CellScript is correctly reusing the CKB ecosystem,
where overlap is an acceptable compiler boundary, and where the project is at
risk of maintaining duplicate infrastructure that should belong to `ckb-std`,
`ckb-sdk-rust`, or the future `cellscript-ckb-adapter`.

## Summary

CellScript is not broadly replacing `ckb-sdk-rust`.

The current `solve-tx`, `deploy-plan`, `lock-deps`, and `action build` surfaces
mostly emit metadata, intent, evidence schemas, and unresolved transaction
plans. That is appropriate compiler output. They do not perform live-cell
selection, CellDep/HeaderDep resolution, fee/change calculation, signing,
tx-pool acceptance, or submission.

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
| CKB syscall/source constants | Hand-written in codegen | `ckb-std::ckb_constants` | Medium | Inline only with parity tests; Rust backend should import `ckb-std` constants. |
| WitnessArgs parsing | Hand-written RISC-V parser | `ckb-std` / `ckb-types` for `WitnessArgs`; CellScript for `CSARGv1` | Medium | Keep inline parser only as implementation duplication; Rust backend should use `ckb-std` loaders and keep CellScript-specific payload decoding. |
| TYPE_ID evidence | Emits builder plans and metadata validation | `ckb-std::type_id`, adapter, SDK | Medium | Keep metadata plan; test against `ckb-std` semantics and adapter outputs. |
| Since/epoch encoding | Hand-written helpers | `ckb-std::since` | Medium | Keep compiler helpers; add parity tests. |
| Occupied capacity | Hand-computes from lock/type/data bytes in runtime helper | CKB `CellField::OccupiedCapacity`, `ckb-std`, `ckb-types` | High | Rust backend should use `load_cell_occupied_capacity`; inline backend should prefer field 6 or prove fallback equivalence. |
| Generated syscall stdlib | `StdLib::generate_syscalls` emits a second assembly syscall surface | `ckb-std` and main codegen runtime helpers | High | Deprecate, remove, or generate from one ABI table. |

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

This division is correct. The future adapter should consume these outputs and
use `ckb-sdk-rust` for the chain-facing work.

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

CellScript currently hand-writes CKB syscall numbers, source values, field ids,
and since flags in `src/codegen/mod.rs`.

This is acceptable only as inline-backend implementation duplication. The
current RISC-V/ELF output cannot call a Rust `ckb-std` function at runtime, but
the semantics still belong to `ckb-std`'s authoritative contract-side
constants:

```text
ckb-std/src/ckb_constants.rs
```

Required mitigation:

- add constant parity tests for syscall numbers;
- add parity tests for `Source::{Input, Output, CellDep, HeaderDep,
  GroupInput, GroupOutput}`;
- add parity tests for `CellField`, `HeaderField`, and `InputField`;
- add parity tests for since flag masks used by CellScript epoch helpers;
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

CellScript currently has a runtime helper that recomputes occupied capacity
from:

```text
8 + lock script occupied bytes + optional type script occupied bytes + data_len
```

This helper is useful as an executable compatibility experiment, but it should
not become CellScript's independent capacity standard.

Required mitigation:

- in a Rust verifier/shim backend, use `ckb_std::high_level::load_cell_occupied_capacity`;
- in the inline backend, prefer `LOAD_CELL_BY_FIELD` with
  `CellField::OccupiedCapacity` when the CKB profile exposes that field;
- keep the multi-syscall computation only as a fallback or differential helper;
- add fixtures proving computed occupied capacity equals CKB field 6 for
  supported cells;
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

Required mitigation:

- deprecate `--gen-stdlib` for CKB syscall runtime output, or clearly label it
  as internal/debug-only;
- remove duplicated syscall wrapper generation when the main codegen path owns
  the emitted runtime helper;
- if the command must stay, generate it from the same ABI table used by
  `src/codegen/mod.rs`;
- add tests that compare generated stdlib helper behavior with main codegen
  helper behavior for the same builtin surface.

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

Required mitigation:

- add differential fixtures against `ckb-types::packed::WitnessArgs`;
- cover valid fields, `BytesOpt::None`, empty payloads, short tables,
  non-monotonic offsets, oversized witnesses, and trailing bytes;
- document exactly which `WitnessArgs` fields CellScript uses for entry
  payloads and lock signatures;
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

The future `cellscript-ckb-adapter` should absorb all chain-reality work:

- deployment transaction construction;
- live-cell selection;
- CellDep and HeaderDep resolution;
- occupied-capacity measurement;
- fee and change calculation;
- final `WitnessArgs` placement;
- signing hooks;
- `estimate_cycles`;
- `test_tx_pool_accept`;
- optional `send_transaction`;
- machine-readable acceptance reports.

Those are already available in `ckb-sdk-rust` through RPC clients, transaction
builders, input iterators, cell collectors, dep resolvers, signers, and packed
capacity APIs. CellScript should integrate with those APIs rather than grow
parallel infrastructure.

## Prioritized Cleanup

### P0

1. Add `ckb-std` parity tests for CKB syscall numbers, sources, field ids, and
   since constants.
2. Rework occupied-capacity helper to prefer `CellField::OccupiedCapacity`, or
   explicitly mark the current helper as fallback and prove equivalence.
3. Deprecate, remove, or unify `StdLib::generate_syscalls` with main codegen's
   ABI helper source of truth.

### P1

1. Add `WitnessArgs` differential fixtures against `ckb-types` readers.
2. Add TYPE_ID differential fixtures against `ckb-std::type_id`.
3. Add since/epoch parity fixtures against `ckb-std::since`.
4. Update `validate-tx` output/docs to say metadata/evidence validation, not
   node acceptance.

### P2

1. Add adapter examples that use `ckb-sdk-rust` for capacity, deps, signing,
   acceptance, and submission.
2. Add a `ckb-std` compatibility report command or test fixture summary.
3. Consider an optional generated Rust shim using
   `ckb_backend_runtime = "ckb-std"` for mixed Rust/CellScript projects.

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
