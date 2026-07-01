---
name: cellscript-ckb-model
description: CKB Cell Model, lock/type roles, Source views, WitnessArgs, CellDeps, capacity, since, and replacement semantics for CellScript work.
references:
  - docs/wiki/CKB-Glossary.md
  - docs/wiki/Tutorial-05-CKB-Target-Profiles.md
  - docs/CELLSCRIPT_CKB_STD_COMPAT.md
  - docs/CELLSCRIPT_ENTRY_WITNESS_ABI.md
commands:
  - cellc ckb-std-compat
  - cellc constraints
---

# CellScript CKB Model

Use this skill for CKB-facing CellScript work. Keep the CKB Cell Model clear:
transactions consume live Cells and create new Cells; state changes happen by
Cell replacement. Lock Scripts control spending, Type Scripts validate state,
and Scripts execute in CKB-VM.

Do not guess CKB runtime, RPC, syscall, SDK, or deployment behaviour. For
version-sensitive claims, verify against official CKB documentation, source
repositories, RFCs, or release notes before coding.

Validation defaults:

- run `cellc constraints . --target-profile ckb --json` for compiler-visible
  CKB obligations;
- run `cellc ckb-std-compat --json` for the current compatibility boundary.
