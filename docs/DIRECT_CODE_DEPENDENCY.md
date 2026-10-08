# Direct code dependency snapshot binding

The optional standalone `code_origin::check_direct_code_dependency` consumes an
actual private `CheckedTargetCodeCellOrigin`, canonical final RawTransaction
bytes and untrusted `SuppliedDependencyCell` snapshots (OutPoint, CellOutput,
data). Its private `CheckedDirectCodeDependency` cannot be reconstructed from
JSON or a caller compatibility flag. It proves consistency of these supplied
bytes with the checked creation/artifact/complete Script; it does not prove
consensus resolution, dependency liveness, VM execution or policy authority.

## Initial profile and checks

The initial `data2-direct-host-supplied-cells-v1` profile requires the checked
`ckb` data2 target. Every raw CellDep must be direct code; dep groups reject
rather than imply authenticated expansion. Type-hash targets reject without
separate history evidence. Existing canonical version-zero RawTransaction
validation covers all inputs, outputs, Scripts, data and header/raw deps.

There must be exactly one supplied snapshot for every distinct raw OutPoint and
no extra snapshot. All supplied CellOutputs are parsed, including unselected and
final members. The selected ELF data hash must occur exactly once across the
complete supplied set. Its OutPoint must equal the checked code creation's
transaction hash and output index; copying identical ELF bytes to a different
OutPoint cannot substitute. Its capacity, complete Lock hash and optional Type
hash must equal the independently checked creation output. The data2 origin
already binds the complete selected Script/code hash/args to the actual artifact.

Raw dependency positions and supplied-list positions are determined separately
by exact OutPoint correspondence. Supplied order may differ from raw order. The
record's `supplied_cell_index` denotes only a position in that host input list;
it is **not** a certified CKB resolved dependency/syscall index. VM resolution,
active chain/VM, final selected Script group, witness placement and peer execution
are not checked by this byte-consistency profile.

CKB's pinned implementation expands dep groups into resolved Cells and uses the
resolved list for CellDep syscalls. Its data-hash lookup does not universally
reject duplicate code bytes. This profile deliberately requires direct deps and
unique selected data to keep its limited host claims explicit. See pinned
[dependency expansion](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/types/src/core/cell.rs#L754),
[CellDep syscall indexing](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/syscalls/load_cell.rs#L45)
and [Script lookup](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/types.rs#L828).
These were checked against the pinned source, not assumed from general docs.

## Limits and deterministic evidence

Before parsing or hashing, supplied count must be 1–64. Every raw/output/data
input is at most 4 MiB and respects its caller record/artifact ceiling. The raw
transaction plus **all** OutPoint/output/data inputs together are at most 16 MiB.
A malformed earlier input cannot hide a later size overflow. Existing raw deps
also retain their 64-member ceiling and Scripts their 4,096-byte limit. These
limits are separate from native source capture and four-file receipt checking;
no global VM, witness or metadata budget changes.

Schema/domain are `cellscript-direct-code-dependency-v1` /
`cellscript-direct-code-dependency-id-v1`. The record commits actual target
origin, complete raw hash/length, both selected positions and every ordered
supplied OutPoint/output/data hash/length. Consensus, VM and authorization claims
are explicitly false. `check_unchanged_inputs` repeats the bounds and compares
all original bytes/identities, including unselected Cells and supplied order.
Changed raw fields or snapshots require a new proof. RawTransaction excludes
witnesses/signatures; this is not a signed-transaction freeze token.

## Native source/version composition

`freeze_source_code_policy` now returns a distinct `FrozenSourceCodePolicy`, and
its membership check returns `FrozenSourceCodePolicySelection`. Artifact-only
`FrozenCodePolicy` / selection values cannot convert to those types. The private
source policy retains the same v2 canonical record/identity and version rules;
this change adds no wire reinterpretation or new nominal source syntax.

A source selection's `check_direct_dependency` consumes it and performs the
independent final-raw/snapshot check against that exact candidate's private target
origin. It then rechecks the consumer's actual pinned source closure. Its private
`FrozenSourceCodeDependency` owns both proofs and binds source-policy identity,
selected source receipt and checked dependency identity under schema/domain
`cellscript-frozen-source-code-dependency-v1` /
`cellscript-frozen-source-code-dependency-id-v1`. Later unchanged-input checks
repeat final raw/snapshot bounds and consumer source checks. Candidate sources
remain immutable historical snapshots, as in the source receipt boundary.

This remains host evidence. Immutable root authorization, actual authenticated
resolution/liveness, Type history, complete H1/H2, `I` / source handles, general
codec, ProtocolBundle/generated-builder signing parity, #29 and #44–#46 remain
pending. Independent security review is still required for stable admission.

Five independent tests cover O0–O3 SDK-built fixtures, separate list positions,
raw hash/canonical identity oracles, 12 mutation axes, Type targets, preparse
limits and all 64 Cells with a malformed final snapshot. Two native tests cover
source/version membership composition at O0–O3 and both selection modes,
changed final raw/data and changed consumer source. Two compile-fail doctests
reject artifact-only proof/selection substitution. These tests provide no new
on-chain execution, consensus, behavioral-equivalence or production claim.
