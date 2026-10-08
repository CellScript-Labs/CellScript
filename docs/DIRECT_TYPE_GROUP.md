# Direct Type-group transaction snapshot

Status: experimental host byte-consistency prerequisite for #28. This is an
optional standalone checker operation, not source handle syntax or on-chain
authorization. `code_origin::check_direct_type_group` consumes an actual private
`CheckedDirectCodeDependency` and a private `CheckedFixedPolicyReceipt` with the
same checked target identity. An unrelated target, exported JSON or a boolean
cannot create `CheckedDirectTypeGroup`. The dependency record directly stores
its checked target identity. This operation separately commits the supplied
private fixed-receipt identity; the native composition
always uses the source selection's exact candidate receipt.

## Complete inputs and group derivation

`SuppliedTypeGroupTransaction` contains canonical full Transaction bytes, every
dependency snapshot and every input Cell snapshot. `SuppliedInputCell` is
explicitly untrusted OutPoint/CellOutput/data input. Raw transaction bytes must
match the checked dependency proof, including all dependencies and snapshots.
Every referenced input OutPoint must have exactly one supplied Cell, with no
extra or duplicate input. All supplied CellOutputs are parsed, including the
unselected final member. All raw outputs/data and every witness Bytes item are
canonically checked. Foreign and extra witness contents remain opaque bytes.

The selected group is derived from complete **Type Script** hashes across raw
input order and output order. It is not a caller group index, a supplied-list
position, a code hash or a matching Lock occurrence. Changed args create a
different full Script. The first matching global input index selects the group
witness; only when no matching input exists does the first matching global
output index supply that position. No group or no witness rejects.

This indexing was checked against the pinned official
[CKB witness syscall](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/syscalls/load_witness.rs#L32)
and [Molecule schema](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/gen-types/schemas/blockchain.mol#L66).
The initial finite receipt supports unit-result actions without output encoding.
Output-only appearance cannot satisfy an input-consuming action's cardinality;
deriving the fallback index does not admit an unsupported output action.

## Selected witness and fixed codec

The selected WitnessArgs has the checked Type-policy placement and at most
4,096 bytes. Every optional field uses canonical Bytes. `input_type` must carry
the canonical CSPOLv1 bundle, bounded to 4,076 bytes and 1–8 records. Every
record's role, full Script hash, tag and args fields are parsed; keys must be
strictly sorted and unique. Nonempty args require CSARGv1 magic. Invalid
unselected records also reject.

Exactly the Type/current-complete-Script key selects the request. Its tag and
input/output group cardinalities must match the private receipt's checked
dispatch. Its args must match that actual independently checked positional
machine decoder's fixed width and magic. A payload-free action requires empty
args, not an eight-byte empty header. This finite external profile transports
unsigned scalar/i32 bitpatterns; it does not check business predicates. Input
Cell data is fingerprinted, not admitted as valid application state by this
operation. Other policy records receive no peer execution or admission claim.

## Bounds and unchanged-byte evidence

Before parsing or hashing: at most 256 input snapshots, 1–64 dependency
snapshots, 4 MiB per input under the caller's record/artifact ceiling, and
16 MiB across the full transaction plus every OutPoint/output/data snapshot.
There are at most 256 witness items. Raw/header dependency and Script limits
remain inherited from the direct dependency profile. An earlier malformed
transaction cannot conceal later input overflow. These are separate operation
budgets, without changing runtime or default metadata budgets.

Schema/domain are `cellscript-direct-type-group-v1` /
`cellscript-direct-type-group-id-v1`. The record commits the actual fixed receipt
and dependency identities, complete transaction hash/length, complete Script
hash, all ordered supplied input fingerprints, derived group indices, witness
source/index, selected tag and policy bundle hash/length. Consensus, VM,
authorization and signature-verification claims are explicitly false.

`check_unchanged_inputs` repeats bounds and checks the full transaction,
including Lock placeholders/signature bytes and foreign/extra witnesses, plus
every input and dependency snapshot. A witness-only edit preserves CKB's raw
transaction hash but invalidates this proof. Signing edits require a new checked
complete snapshot. Rechecking opaque signature bytes does not verify signatures.
This is not yet a signing pipeline that freezes business fields while allowing
an authenticated signer to fill its designated fields.

The native consuming `FrozenSourceCodeDependency::check_type_group` constructs
private `FrozenSourceCodeTypeGroup`, then rechecks the actual pinned consumer
source closure. Its identity binds the actual source policy, selected source
receipt and independent Type-group proof. Later rechecks repeat all byte checks
and consumer source checks. Artifact-only selections cannot enter this path.

Seven independent tests cover O0–O3 SDK transactions, raw/supplied group positions,
complete witness hashes, signature/foreign/extra substitutions, role/args/count
mutations, malformed envelopes and records, all 256 inputs/eight records,
preparse limits, exact scalar/empty args and rejected actual receipt-bundle byte
substitution. One native test covers both source
selection modes at O0–O3 and changed full witnesses/consumer sources; a
compile-fail doctest rejects artifact-only substitution.

Authenticated resolution/liveness, immutable root authority, Type history,
nominal `I`/handles, full H1/H2, ProtocolBundle/generated-builder signing parity,
#29 and #44–#46 remain required. Independent security review remains pending;
full CI is deferred until the complete implementation queue.
