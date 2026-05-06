# CellScript ckb-std Compatibility

**Status**: 0.19 scope compatibility contract with implementation landed for
ckb-std constant parity, SourceView decoding, WitnessArgs layout fixtures,
occupied-capacity field lowering, the formal headless CKB adapter crate, and
focused local-node adapter acceptance.

See also
[`CELLSCRIPT_CKB_ECOSYSTEM_REUSE_AUDIT.md`](CELLSCRIPT_CKB_ECOSYSTEM_REUSE_AUDIT.md)
for the audit of overlap with `ckb-std` and `ckb-sdk-rust`.

`ckb-std` is the canonical Rust-side contract standard library for CKB.
CellScript should treat it as the contract-side ABI and runtime oracle, not as a
transaction builder and not as a compiler-core dependency.

This document belongs to the 0.19 registry/deployment/adapter scope. It defines
the compatibility contract that the 0.19 Action Builder and
`cellscript-ckb-adapter` must respect; it is not counted as 0.18
protocol-equivalence evidence.

In practical terms:

```text
CellScript is the semantic compiler.
ckb-std is the contract-side ABI/runtime oracle.
ckb-sdk-rust is the transaction realiser.
```

The three components sit on different sides of the same production workflow:

| Layer | Responsibility |
|---|---|
| `cellc` compiler | Parse, type-check, lower, emit CKB verifier artifacts, metadata, ABI, constraints, deploy plans, action plans, and witness bytes. |
| `ckb-std` | Define the contract-side Rust vocabulary for CKB syscalls, sources, fields, entry points, witnesses, TYPE_ID, since, exec/spawn, debug, allocation, and native simulation. |
| `cellscript-ckb-adapter` | Consume compiler outputs and use `ckb-sdk-rust` to materialise deployments and action transactions. |
| `ckb-sdk-rust` (5.x) | Provide off-chain CKB data structures (`ckb-types` 1.0.0), sync and async RPC / indexer clients, `CellCollector` (Default / Offchain / LightClient), `CellDepResolver`, `HeaderDepResolver`, `Signer` and lock-specific `ScriptUnlocker` (SecpSighash, SecpMultisig Legacy/V2, ACP, Cheque, OmniLock), `CapacityBalancer` / `CapacityProvider`, protocol `tx_builder` modules, acceptance, and submission. |
| CKB node | Execute the script, measure cycles, and accept or reject the concrete transaction. |

## Boundary

The compiler core must not depend on `ckb-std`.

CellScript emits its own RISC-V assembly or ELF and must stay usable for offline
compile, metadata inspection, static checks, package workflows, and future
profile work. Pulling `ckb-std` into compiler core would make a Rust contract
runtime look like a language runtime dependency, which is the wrong boundary.

Allowed uses of `ckb-std`:

- compatibility tests and differential fixtures;
- documentation and cookbook mappings;
- optional example crates;
- optional generated Rust shim code for mixed Rust/CellScript projects;
- native-simulator or stub-syscall developer loops when they add useful local
  evidence.

Disallowed uses:

- making `ckb-std` a required dependency of `cellc` compiler core;
- using `ckb-std` as a substitute for CKB VM execution evidence;
- using `ckb-std` as a transaction builder;
- hiding CellScript's emitted syscall, witness, or source ABI behind an
  undocumented Rust wrapper.

## Backend Runtime Policy

`ckb-std` is not a CellScript codegen framework. It does not know CellScript's
AST, action ABI, resource schema, `CSARGv1` witness payload, transition
obligations, generated error model, metadata, or evidence format.

CellScript therefore owns verifier code generation. The CKB backend can still
choose how generated verifier code observes CKB runtime facts:

| Runtime policy | Output shape | Reuse point | Boundary |
|---|---|---|---|
| `ckb_backend_runtime = "ckb-std"` | Generated Rust verifier or Rust shim source | Imports `ckb_std` constants, `high_level` loaders, TYPE_ID, since, allocation, and helper APIs | Future Rust backend or mixed Rust/CellScript projects; no compiler-core dependency |
| `ckb_backend_runtime = "inline"` | Self-contained RISC-V/ELF verifier output | Emits low-level syscall wrappers and small runtime helpers directly | Current artifact strategy; implementation duplication only, protected by parity tests |

Today CellScript's CKB output is self-contained RISC-V/ELF, so it is an inline
runtime backend. If CellScript later grows a Rust verifier or Rust shim backend,
`ckb-std` mode should be the preferred default for ordinary CKB workflows.
Inline mode should remain available for bootstrap, size/cycle-sensitive output,
special profiles, or fully self-contained artifacts.

The governing rule is:

```text
Reuse ckb-std for observing CKB.
Generate CellScript code for enforcing CellScript semantics.
```

Concretely:

| Concern | Owner |
|---|---|
| CKB syscall constants, source encodings, field ids, `WitnessArgs` layout, TYPE_ID semantics, since semantics, occupied-capacity field access, exec/spawn helpers | `ckb-std` compatibility surface |
| Action dispatch wrapper, `CSARGv1` decoder, resource schema decoder, input/output matching, transition checks, generated error codes, metadata and evidence emission | CellScript compiler/backend |

Inlining runtime access code is an implementation strategy for self-contained
verifier output, not an independent semantic standard. Any inline helper that
overlaps with `ckb-std` must have parity or differential tests.

Current implementation evidence:

- `src/ckb_abi.rs` is the CellScript-owned inline ABI mirror.
- `tests/ckb_std_compat.rs` compares that mirror against `ckb-std` syscall,
  source, field, and since APIs.
- `StdLib::generate_syscalls` and main codegen consume the same ABI mirror.
- `ckb::cell_occupied_capacity` now lowers to
  `LOAD_CELL_BY_FIELD(CellField::OccupiedCapacity)`.
- `WitnessArgs` layout tests compare against
  `ckb-types::packed::WitnessArgs`, including malformed table cases.
- TYPE_ID lifecycle classification and args-hash generation are tested against
  the `ckb-std::type_id` API contract.
- since/epoch tests cover both valid encodings and malformed cases accepted or
  rejected by `ckb-std::since`.
- `cellc validate-tx --json` reports `validation_level =
  cellscript-metadata-evidence`, `ckb_vm_execution = false`, and
  `tx_pool_acceptance = false`.
- `cellc ckb-std-compat --json` emits the runtime policy, ABI mirror source,
  tested compatibility areas, script-construction evidence, and adapter
  boundary in a stable report.
- `crates/cellscript-ckb-adapter` is the formal headless off-chain bridge to
  `ckb-sdk-rust` while keeping compiler core free of SDK dependencies,
  including arbitrary `ckb_types::packed::Script` construction, script hash /
  args evidence, lock/type script readback from packed `CellOutput` values,
  explicit CellDep binding for script code hashes, and TYPE_ID output args
  checks.
- `examples/ckb-sdk-builder` is a cookbook wrapper around the formal adapter
  crate, not a second implementation.
- `cellc action build --json` and `cellc ckb-std-compat --json` expose the
  adapter-owned final `WitnessArgs` placement policy for CellScript entry
  payloads and lock signatures.
- `scripts/cellscript_ckb_ecosystem_reuse_gate.sh` runs the focused compatibility
  gate for this slice.
- `scripts/cellscript_ckb_adapter_acceptance.sh` records local CKB
  `estimate_cycles` and `test_tx_pool_accept` evidence for the adapter boundary.

## Compatibility Contract

CellScript's CKB backend should align with `ckb-std` at the ABI boundary:

| CKB contract concept | `ckb-std` reference | CellScript surface |
|---|---|---|
| Syscall numbers | `ckb_std::ckb_constants::SYS_*` | `RuntimeSyscallAbi` and emitted `ecall` numbers |
| Cell sources | `Source::{Input, Output, CellDep, HeaderDep, GroupInput, GroupOutput}` | `SourceView`, `source::input`, `source::output`, `source::group_input`, `source::group_output`, `source::header_dep` |
| Cell fields | `CellField::{Capacity, DataHash, Lock, LockHash, Type, TypeHash, OccupiedCapacity}` | `ckb::cell_capacity`, `ckb::cell_lock_hash`, `ckb::cell_type_hash`, `ckb::cell_occupied_capacity` |
| Input fields | `InputField::Since` | `dao::require_input_since_at_least`, `dao::require_input_relative_epoch_since_at_least` |
| Header fields | `HeaderField::{EpochNumber, EpochStartBlockNumber, EpochLength}` | DAO and time/epoch helper lowering |
| Witness args | `high_level::load_witness_args` | `cellc entry-witness` and the CellScript entry wrapper |
| TYPE_ID | `type_id::validate_type_id`, `type_id::check_type_id` | `ckb_type_id`, deploy plans, Type ID builder plans, metadata evidence |
| Since values | `since::Since`, `EpochNumberWithFraction` | `ckb::since_epoch_absolute`, `ckb::since_epoch_relative`, DAO maturity helpers |
| CellDep lookup | `look_for_dep_with_hash2`, `exec_cell`, `spawn_cell` | script refs, `spawn`, spawn/IPC metadata, builder-resolved CellDeps |
| Debug/entry/allocation | `debug!`, `entry!`, `default_alloc!` | CellScript's generated entry and runtime error ABI, not a direct macro dependency |

This table should become the compatibility matrix for CKB-facing backend
changes. If CellScript adds a new CKB builtin that overlaps with `ckb-std`, the
change should state which `ckb-std` API or constant it follows, or why it
intentionally differs.

## Runtime Semantics

`ckb-std` is especially useful where CKB behavior is easy to misremember:

- `GroupInput` and `GroupOutput` use the high-bit group-source encoding.
- `WitnessArgs` are Molecule values loaded through `load_witness`.
- TYPE_ID creation hashes the first input and output index, then compares the
  requested args prefix.
- relative epoch since uses the CKB packed epoch fraction model.
- `exec` and `spawn` execute code from `CellDep`, so builders must provide the
  matching dep cells.
- `OccupiedCapacity` is a concrete CKB cell field and should be treated as the
  reference value when the target profile exposes it.

In inline mode, CellScript can still use custom generated helpers, but those
helpers need evidence against the same semantics. For example, if a CellScript
helper computes occupied capacity from lock script, optional type script, and
data bytes, tests should prove it matches the CKB field value for supported
cells or the metadata must declare the profile-specific limitation.

## Evidence Strategy

Compatibility evidence should be layered:

```text
CellScript source
  -> cellc CKB artifact + metadata
  -> ckb-std reference contract or fixture where useful
  -> ckb-vm / ckb-testtool execution
  -> ckb-sdk-rust materialised transaction
  -> estimate_cycles / test_tx_pool_accept / optional send_transaction
```

Recommended test cuts:

- constant parity tests for syscall numbers, source values, and field ids;
- witness layout tests comparing CellScript entry witness bytes with
  `WitnessArgs` placement;
- TYPE_ID create, transfer, duplicate, and burn fixtures against
  `ckb-std` semantics;
- since/epoch fixtures for absolute, relative, malformed, immature, and mature
  cases;
- occupied-capacity fixtures that compare runtime helper lowering with
  `CellField::OccupiedCapacity`, and packed CKB capacity measurement;
- spawn/exec fixtures that prove required CellDeps are surfaced in metadata and
  resolved by the adapter.

Compile-only evidence is not enough for production claims. The useful claim is
that a CellScript artifact behaves like the corresponding CKB contract-side
ABI, and that a concrete transaction carrying that artifact is accepted or
rejected by the node for the expected reason.

The current 0.19 evidence now covers ABI compatibility, headless adapter
materialization, and focused local-node adapter acceptance. It is still not a
wallet UI, CellFabric, package registry production gate, external audit, or
exhaustive adversarial state-space proof.

## Cookbook Topics

The first ckb-std compatibility cookbook should be concrete:

```text
01_map_cellscript_sourceview_to_ckb_std_source.md
02_load_entry_witness_as_witness_args.md
03_validate_cellscript_type_id_against_ckb_std.md
04_compare_since_epoch_encoding.md
05_compare_occupied_capacity_measurement.md
06_resolve_spawn_and_exec_celldeps.md
07_run_ckb_std_reference_contract_side_by_side.md
08_emit_ckb_std_compat_report.md
```

These topics serve Rust CKB developers directly: they show how familiar
`ckb-std` concepts map to CellScript metadata, builtin calls, and generated
verifier behavior.

## Optional Rust Shim

A future optional shim can make mixed Rust/CellScript projects easier:

```text
cellc gen-rust-shim --runtime ckb-std
```

This is the natural place for the `ckb_backend_runtime = "ckb-std"` policy. A
Rust verifier/shim backend may call `ckb_std::high_level::*`,
`ckb_std::ckb_constants`, `ckb_std::type_id`, and `ckb_std::since` directly,
while CellScript still generates the action wrapper and semantic verifier logic.

Possible output:

- Rust constants for CellScript ABI names and metadata hashes;
- typed witness encoders/decoders compatible with `WitnessArgs`;
- helper functions for script args and TYPE_ID placement;
- fixture harnesses using `ckb-std` native simulator or stub syscalls.

This shim should be generated or example-level glue. It should not become the
CellScript compiler runtime.

## Relationship To The CKB Adapter

`ckb-std` answers:

```text
Inside the CKB VM, how should a contract read and interpret transaction data?
```

`ckb-sdk-rust` answers:

```text
Outside the CKB VM, how do we build, sign, preflight, and submit the transaction?
```

The adapter bridges CellScript compiler outputs to `ckb-sdk-rust`. It should
also preserve enough evidence to show that those outputs obey the `ckb-std`
contract-side ABI assumptions.

The mature positioning is:

```text
CellScript does not replace ckb-std.
CellScript generates contract artifacts that should be ckb-std-compatible at
the CKB ABI boundary.

CellScript does not replace ckb-sdk-rust.
CellScript emits transaction intent that ckb-sdk-rust can make real.
```
