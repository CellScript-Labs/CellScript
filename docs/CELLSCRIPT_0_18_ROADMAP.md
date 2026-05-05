# CellScript 0.18 Roadmap Stub

## First-Class Script API

First-class Script API work is deferred from 0.17 to 0.18.

0.17 keeps helper-level Script support for iCKB equivalence evidence:

- `ckb::current_script_hash()`
- `ckb::require_current_script_args_empty()`
- `ckb::require_cell_lock_script_hash_type(...)`
- `ckb::require_cell_type_script_hash_type(...)`
- `ckb::require_cell_lock_args_empty(...)`
- `ckb::require_cell_type_args_empty(...)`
- `ckb::require_cell_lock_args_hash(...)`
- `ckb::require_cell_type_args_hash(...)`

0.18 starts with read-only ScriptRef / ScriptArgs. The canonical long-term
surface remains property-like:

- `cell.lock.code_hash`
- `cell.lock.hash_type`
- `cell.lock.args_empty`
- `cell.lock.args_hash`
- `cell.type?.code_hash`
- `cell.type?.hash_type`
- `cell.type?.args_empty`
- `cell.type?.args_hash`
- exact / prefix / suffix args checks

The first implementation pass exposes the same verifier facts through explicit
CKB SourceView helper calls while the parser-level property surface is still
being designed. It is available under `--primitive-strict=0.18`:

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

These are read-only extraction primitives over an existing CKB transaction
source. They lower to `LOAD_CELL_BY_FIELD`, parse the Molecule `Script` shape,
and fail closed on malformed source data. `*_args_hash` is intentionally
restricted to exactly 32-byte Script args in this pass, matching the existing
hash-shaped comparison helpers. The prefix/suffix helpers bind the first or
last 32 bytes of a Script args payload and require the args payload to be at
least 32 bytes. Optional type-script reads still fail closed when the source has
no type script; a true `cell.type?` optional surface remains future work.

Initial 0.18 scope excludes:

- constructing arbitrary `Script` values
- constructing TYPE_ID scripts
- script hash synthesis from arbitrary fields
- deployment manifest resolution
- cell dep solving
- arbitrary-length prefix / suffix args matching beyond 32-byte hash-shaped
  bindings
- a general optional Script value model

The goal is to collapse helper fragmentation into a typed read/compare surface,
not to introduce a script-construction layer in the first 0.18 pass.
