# Edition 2027 Lock Script preview

This package exercises the experimental `0.26b` native `lock_script` surface.
It keeps the current entry-witness and CKB target ABIs while making the
protected Cell, current Lock Script arguments, and witness provenance explicit.

```bash
cellc expand examples/lock-script-2027 --json
(cd examples/lock-script-2027 && cellc check --target-profile ckb)
```

This is a provenance and equality fixture, not an ownership-authentication
example. `claimed_owner == owner` compares public values; anyone can copy the
owner into the witness. The fixture contains no signature verification and
does not prove control of the owner's credentials. A real ownership Lock must
enforce authorization bound to the transaction.

The generated `AuthorizationOnly` disposition records the Lock's scope. The
classification does not strengthen these predicates or prove that another
particular policy enforces successor, retirement, data, identity, Type Script,
or capacity constraints. Reference-policy requirements are recorded in the
[authoring target](../../docs/CELLSCRIPT_AUTHORING_TARGET.md).

The grammar is a bounded preview rather than the frozen CellScript 1.0 source
contract. The complete implemented boundary, rejected forms, issue conflicts,
and deferred syntax are documented in
[`docs/CELLSCRIPT_AUTHORING_GRAMMAR.md`](../../docs/CELLSCRIPT_AUTHORING_GRAMMAR.md).
