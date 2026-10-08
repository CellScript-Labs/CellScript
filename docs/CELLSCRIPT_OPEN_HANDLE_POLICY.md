# Open-handle authorization-set wire contract

Status: experimental 0.32 host codec and membership verification for #28.
This fixes the bounded commitment representation used by the implementation
work. It does not admit a compatible artifact, source-level `ScriptHandle<I>`,
CKB runtime helper, open participant, or production deployment. Independent
security review and the complete H1/H2 contract remain outstanding.

## Authority and receipt prerequisites

A compatibility report describes an interface relationship. An authorization
root approves particular implementations. These are distinct inputs. The
complete independent interface receipt, exact artifact bundle and deployment
history must be checked before the application approves a member. A producer's
metadata hash or `compatible: true` cannot substitute for that admission.

`AuthorizationSet::new` is a low-level canonical commitment builder. Its fields
are supplied records, not independently admitted receipts. `verify_selection`
returns `PolicyMembership`, which proves selection under its **separately
supplied expected root** and retains that root in the immutable result, so a
downstream consumer can bind it to its own authorized policy. It deliberately
cannot return a verified compatible
artifact or nominal source handle. Both APIs are useful for wire tests before
complete artifact admission is implemented; neither is an admission shortcut.

H2 must bind the expected root to a code literal, to committed current-Script
args whose location/meaning is checked, or to authenticated state controlled by
an explicit transition contract. A root obtained from the same untrusted
witness as the membership proof is forbidden as an authorization input.
Changing a builder configuration or Registry record cannot change that root.
The expected nominal interface must likewise come from the resolved consuming
package contract; arbitrary user types and names cannot manufacture it.

The pre-signing admission layer must consume actual checker inputs, reconstruct
all required compatibility dimensions and match every selected receipt field.
Header interface and runtime-ABI hashes identify the required baseline contract.
A member retains its actual interface and complete receipt identity; compatible
extensions must be compared directionally by admission, rather than requiring
the whole new interface hash to equal the baseline or erasing its extra fields.
The target profile must remain the admitted target profile.
All-member duplicate/conflict checks run when constructing/admitting the set;
a single member path is not proof that all other members were independently
admitted. ProtocolBundle must retain this distinction when integrating the
codec. That integration and the nominal parameter resolver are not implemented
by this module.

## Fixed Molecule records and numeric bounds

The authoritative fixed-field order is
[`open_handle_policy.mol`](../crates/cellscript-artifact-checker/src/open_handle_policy.mol).
The Rust codec does not add a second transaction serializer. Struct fields are
concatenated as prescribed by Molecule; integer arrays are little-endian.
Unknown tags, wrong byte lengths and nonzero reserved bytes reject.

| Record | Bytes | Fields in order |
| --- | ---: | --- |
| Header | 188 | Magic `CSOHPv1\0`; class, role, mode, member count (one byte each); policy sequence and minimum admission sequence (u64 each); required interface, exact receipt, network genesis, target profile, runtime ABI (32 bytes each) |
| Member | 292 | Magic `CSOHMv1\0`; status and hash type (one byte each); six zero bytes; admission and deployment sequences (u64 each); receipt, interface, artifact, complete Script, code hash, code transaction hash, deployment line, history tip (32 bytes each); code output index (u32) |
| Selection | 656 | Magic `CSOHWv1\0`; Header; Member; canonical member index (one byte); seven zero bytes; five sibling hashes in leaf-to-root order |

The set has **1–32 members** and a fixed tree depth of **5**. Count is checked
before cloning/traversing members. Empty and over-limit sets reject. A selector
must be below the header's member count; arbitrary depth, paths, padding fields
and trailing bytes are not accepted. No proof-controlled allocation is needed
for decoding a selection. This host implementation is not yet a no-allocator
CKB helper, and no heap/stack or cycle measurement is claimed.

The 32-member catalog limit is not permission to place 32 proofs in one entry
witness. H2 and generated builders must account for each 656-byte selection and
all other arguments within the existing bounded witness ABI. Exceeding that
aggregate limit must reject instead of silently expanding it.

Tags are frozen for this experimental schema:

- class: Script = 0, verifier = 1;
- role: Lock = 0, Type = 1, spawned verifier = 2;
- mode: exact = 0, compatible = 1;
- member status: active = 0, yanked = 1;
- code hash type: data = 0, type = 1, data1 = 2, data2 = 4.

Only Script/Lock, Script/Type and verifier/spawned-verifier combinations are
valid. Header interface/network/target/ABI and required member identities must
be nonzero. Exact mode requires a nonzero receipt present in the set and selects
only that receipt. Compatible mode requires a zero exact-receipt field and may
select an active admitted member; the membership operation does not infer or
validate compatibility itself.

## Canonical commitment

Let `H` be the repository's CKB-personalized Blake2b-256. All domain strings
below are UTF-8 with the displayed terminating zero byte. Concatenations have
fixed field widths and no implicit JSON or text normalization.

1. Sort members lexicographically by the raw 32-byte receipt hash. Duplicate
   receipt hashes reject, including duplicates whose other fields differ.
2. For occupied slot `i`, compute
   `H("cellscript-open-handle-member-v1\0" || u8(i) || MemberBytes)`.
3. Pad slots through 31 with
   `H("cellscript-open-handle-empty-v1\0" || u8(i))`. Empty leaves are distinct
   from occupied members and position-bound.
4. Each parent is
   `H("cellscript-open-handle-node-v1\0" || leftHash || rightHash)`.
5. The policy root is
   `H("cellscript-open-handle-policy-v1\0" || HeaderBytes || treeRoot)`.

There are 32 leaf hashes, 31 internal hashes and one final policy hash during
construction. Verification computes the selected leaf, five path hashes and
one final policy hash. These are bounded algorithm counts, not measured CKB
cycles. Input member order cannot change a canonical root or witness.

Changing interface/network/ABI identities, mode, status, floors, a concrete
Script, code Cell, history tip, member position or a sibling changes the
commitment. The root covers all those distinctions rather than a producer's
summary bit. The fixed-array tree is not an on-chain Registry.

## Version, yank and stale-snapshot semantics

Policy `sequence` and `minimum_admission_sequence` are u64 admission revisions,
not SemVer versions, block heights or proof of wall-clock freshness. The floor
cannot exceed the policy sequence. Construction rejects a member admitted in a
future revision. Selection rejects an inactive member or one outside the
closed floor/sequence interval. Below-floor and yanked historical members may
remain committed for auditing but cannot be selected.

An immutable root is a snapshot. A later Registry yank does not modify it and
cannot invalidate a proof checked against that unchanged root. If an
application needs live revocation, H2 must authenticate a unique current policy
state and its transition authority; validating a fresh off-chain snapshot alone
is insufficient. When the expected authenticated root changes, old selection
witnesses fail root binding. No live revocation contract is supplied here.

Package version, directional compatibility and permitted downgrade policies
must be derived and checked by complete receipt admission. An admission
sequence comparison alone cannot establish those relationships. Deployment
sequence is separately recorded for Type-hash history and must not be confused
with policy admission sequence.

## Concrete deployment binding

The member binds the **complete Script hash**, so code hash, hash type and the
concrete args all belong to its selected identity. This format has no wildcard
or schema-only args policy. Different concrete deployments require distinct
exact receipts. A handle remains an observation/selection value and conveys no
Cell consume/create authority.

For data/data1/data2, code hash must equal the exact artifact hash; deployment
line, history tip and deployment sequence are zero. For Type-hash deployment,
line and history tip must be nonzero, and the exact artifact hash and code
OutPoint remain mandatory. Two receipts cannot claim different bytes at the
same OutPoint, or competing tips at one line/sequence. A later Type-hash member
may retain the same Script hash while binding its new code OutPoint, bytes and
history tip. Encoding those identities is not validation of the history or
proof that a code Cell is live.

H2 must check the actual selected Script and code bytes. The initial dependency
binding will require the selected code to occur as a raw **direct code** CellDep,
then independently find one matching resolved code dependency by its data and,
for Type-hash mode, Type identity. Unrelated dep groups can remain present within
explicit scan bounds. Reject ambiguous duplicate matching code/Type identities
rather than assuming raw and expanded indices coincide. Full history and exact
raw-OutPoint checks remain necessary; a `CellDepView` index alone supplies
neither. This is an implementation requirement, not current runtime support.

Network genesis is an admission/build context commitment. This codec does not
observe the active network, live Cells or consensus execution. Pre-signing
network observations and all-group execution remain mandatory and separately
attributed. A local membership test must never be reported as either.

## Evidence and remaining integration

Host tests cover every catalog size and leaf, reordered input, duplicate and
conflicting identities, role/class separation, exact/compatible selection,
yank/floor/new-root behavior, Type-hash history fields, integer extremes,
truncation/extension, reserved/tag errors and every byte of a selection.
Independent CCC 1.23.0 Molecule/CKB-hash vectors cover 1, 3 and 32 members and all
36 member paths; all identities in those vectors are synthetic/non-authorizing.
The generator is retained with the vectors and consumes the existing pinned
CCC installation. Rust tests read the frozen vectors without requiring Node.
CI invokes the generator in `--check` mode after installing that existing pin;
it rejects drift without overwriting the vectors. The vectors also bind the
Molecule schema bytes. This test-tool dependency does not make the policy or
an application depend on ZK.

Still required for #28: complete independent receipt projection and directional
compatibility; nominal interface resolution; checked admission of all members;
source/IR/runtime/helper and independent machine evidence; actual dependency,
history, network and final-transaction bindings; builder/ProtocolBundle/editor
parity; measured worst-case resources and independent review. #29 additionally
requires its participant ownership/claim matrix and complete peer execution.
Neither issue can close on this codec alone.
