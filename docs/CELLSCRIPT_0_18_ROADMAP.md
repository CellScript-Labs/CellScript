# CellScript 0.18 Roadmap

## First-Class Script API

0.18 promotes Script handling from helper fragments into a first-class,
protocol-neutral CKB Script surface. The goal is no longer read-only
`ScriptRef` only: 0.18 must support arbitrary fixed-byte Script construction
inside the verifier and canonical off-chain Script construction for builder and
equivalence tests.

The implementation is deliberately split into three layers:

- `ScriptRef`: verifier-side reads of existing transaction scripts through
  `SourceView` and `LOAD_CELL_BY_FIELD`.
- `ScriptArgs`: constructed exact fixed-byte args values.
- `Script`: constructed `code_hash + hash_type + args` values that can be
  compared against an existing lock/type script.

## Delivered Surface

Read-only ScriptRef properties are available for SourceView script reads:

- `source_view.lock.code_hash`
- `source_view.lock.hash_type`
- `source_view.lock.args_empty`
- `source_view.lock.args_hash`
- `source_view.type.code_hash`
- `source_view.type.hash_type`
- `source_view.type.args_empty`
- `source_view.type.args_hash`
- exact / prefix / suffix args checks

The helper-call form remains available under `--primitive-strict=0.18`:

- `ckb::cell_lock_code_hash(source) -> Hash`
- `ckb::cell_type_code_hash(source) -> Hash`
- `ckb::cell_lock_hash_type(source) -> u64`
- `ckb::cell_type_hash_type(source) -> u64`
- `ckb::cell_lock_args_empty(source) -> bool`
- `ckb::cell_type_args_empty(source) -> bool`
- `ckb::cell_lock_args_hash(source) -> Hash`
- `ckb::cell_type_args_hash(source) -> Hash`
- `ckb::require_cell_lock_args_prefix_hash(source, expected) -> unit`
- `ckb::require_cell_type_args_prefix_hash(source, expected) -> unit`
- `ckb::require_cell_lock_args_suffix_hash(source, expected) -> unit`
- `ckb::require_cell_type_args_suffix_hash(source, expected) -> unit`

0.18 also adds constructed Script values:

```cell
let args = script::args(b"owner")
let expected = script::new(code_hash, hash_type, args)

script::require_cell_lock_matches(source::input(0), expected)
script::require_cell_type_matches(source::output(0), expected)
```

Constructed Script values expose:

- `expected.code_hash -> Hash`
- `expected.hash_type -> u64`
- `expected.args -> ScriptArgs`
- `expected.args.len -> u64`
- `expected.args.is_empty -> bool`

Hash type constructors are explicit:

- `script::hash_type_data() -> u64`
- `script::hash_type_type() -> u64`
- `script::hash_type_data1() -> u64`
- `script::hash_type_data2() -> u64`

Unsupported literal hash types fail in the type checker. `script::args(...)`
accepts fixed byte arrays and `Hash`; non-byte operands fail closed.

## Canonical CKB Encoding

Off-chain construction uses canonical CKB packed Script encoding:

```text
Script {
  code_hash: Byte32,
  hash_type: byte,
  args: Bytes,
}
```

`CkbScriptValue::packed_bytes()` is byte-for-byte checked against
`ckb_types::packed::Script::as_slice()`, and `CkbScriptValue::hash()` is checked
against `packed::Script::calc_script_hash()`.

Verifier lowering does not invent a second encoding. It reads the existing CKB
Molecule `Script` field through `LOAD_CELL_BY_FIELD`, validates the table
layout, checks `code_hash + hash_type`, and then verifies arbitrary exact
fixed-byte args through `__ckb_require_cell_*_args_exact`.

## VM Evidence

0.18 Script construction is covered by `tests/v0_18.rs`:

- compiler/lowering evidence for constructed `Script` requirements;
- typechecker rejection of unsupported hash_type and non-byte args;
- canonical packed Script byte/hash comparison against `ckb-types`;
- `ckb-testtool` VM fixture with a real type ScriptGroup continuation:
  - matching input lock args accepted;
  - mismatched input lock args rejected.

The VM fixture also caught and fixed three production-only lowering bugs:

- Script prefix reads for non-empty args must not require the loaded prefix size
  to equal the full table size.
- Runtime-loaded fixed-byte `Hash` aliases must propagate their buffer metadata
  through `Move` / aggregate field access.
- Exact args helper stack slots must not let the Script prefix buffer overwrite
  saved SourceView state.

## Still Out Of Scope

0.18 does not claim the full deployment/builder layer:

- deployment manifest resolution;
- CellDep solving;
- TYPE_ID script constructor policy;
- non-TYPE-ID global uniqueness proofs;
- arbitrary in-script Script hash synthesis as a DSL builtin;
- optional `source_view.type?` value model;
- Action Builder / CCC transaction generation.

Those remain separate builder/0.19+ topics. 0.18's claim is narrower and
stronger: arbitrary fixed-byte Script construction and exact lock/type Script
matching are now first-class verifier capabilities with CKB VM evidence.
