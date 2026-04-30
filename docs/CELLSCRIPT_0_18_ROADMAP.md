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

0.18 should start with read-only ScriptRef / ScriptArgs:

- `cell.lock.code_hash`
- `cell.lock.hash_type`
- `cell.lock.args_empty`
- `cell.lock.args_hash`
- `cell.type?.code_hash`
- `cell.type?.hash_type`
- `cell.type?.args_empty`
- `cell.type?.args_hash`
- exact / prefix / suffix args checks

Initial 0.18 scope excludes:

- constructing arbitrary `Script` values
- constructing TYPE_ID scripts
- script hash synthesis from arbitrary fields
- deployment manifest resolution
- cell dep solving

The goal is to collapse helper fragmentation into a typed read/compare surface,
not to introduce a script-construction layer in the first 0.18 pass.
