# Semantic Foundation

The Edition 2027 frontend lowers both ordinary authoring and bounded native
Script containers into one checked AST/IR and the current typed semantic record.
The concrete source grammar remains bounded by the language implementation;
this contract does not promise additional runtime features.

## Records and identities

Metadata 75 binds typed semantics v12 and semantic foundation v4. The foundation
contains a bounded value-provenance DAG, explicit entry contract, transaction
roles, input dispositions/output origins, enforcement-classified claims,
operation nodes and layered identities. Ordinary consume/create operations
record their actual policy boundary; they do not acquire unstated absence,
pooled conservation or authorization guarantees.

`SourceDigest` identifies source bytes. `CoreSemanticId` binds normalized typed
meaning and obligations. `EntryContractId` binds the selected trigger, roles,
physical sources and dispatch ABI. Artifact identity binds exact emitted bytes.
Paths and formatting spans remain source-map information. Equal semantic IDs
alone do not prove machine equivalence or transaction acceptance.

## Checked elaboration

`require` and native `enforce` create executable claims with condition provenance,
ordered success/failure paths and a specific terminal error. A structured
`replace` relation generates its own complete field, lock, capacity and identity
checks. Every accepting path must discharge a role exactly once when the entry
uses a consuming disposition. Pooled accounting, explicit retirement and fresh
output origin retain their distinct checked policies.

Absolute transaction positions, current Script-group positions, CellDep reads
and witnesses remain distinct sources. Provenance identifies where a value came
from and does not establish credential control or authenticate a deployed policy.

## Independent boundary

The artifact checker reconstructs and validates the closed wire vocabulary
without loading the compiler frontend or backend. It checks canonical hashes,
budgets, typed/role projections, mapped machine structures and the bounded
runtime contracts it admits. CKB-VM positives and negatives provide separate
runtime evidence. Neither structural admission nor a source claim certifies
arbitrary syscall semantics, chain acceptance or business authorization.

See the [source contract](CELLSCRIPT_SOURCE_CONTRACT.md),
[artifact boundary](CELLSCRIPT_VERIFIED_ARTIFACT_BOUNDARY.md) and
[native grammar](CELLSCRIPT_2027_PREVIEW_GRAMMAR.md).
