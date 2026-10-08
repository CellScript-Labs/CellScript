# Frozen finite code-policy field bindings

Native `package::frozen_interface::freeze_code_policy` consumes an actual private
`ResolvedSourceCatalog` and a separately declared low-level `AuthorizationSet`.
The resulting private `FrozenCodePolicy` owns both, binds every catalog candidate
to exactly one wire member and cannot be reconstructed from exported JSON.
This is finite host field-binding evidence, not full H1 admission or authority
for the resulting root.

The supported boundary is Script/Type with the independently checked data2
`ckb` profile and [finite Type-policy receipts](FIXED_POLICY_RECEIPT.md).
Type-hash code receipts remain constructible by the separate byte/target/receipt
APIs, but this policy-binding constructor rejects them: their authenticated
replacement history is not established by a structurally valid wire member.
Lock and spawned-verifier policy bindings need separate complete profiles.

The Header must match the actual required module projection identity, the
native pinned network genesis and hashes of the independently checked runtime:

| Header field | Actual bound value |
| --- | --- |
| required interface | Required module projection identity |
| network genesis | Required native context's pinned genesis |
| target profile | Canonical hash of checked target-profile string under `cellscript-code-policy-target-id-v1` |
| runtime ABI | Canonical hash of complete checked `InterfaceRuntimeContract` under `cellscript-code-policy-runtime-id-v1` |

Every member must match its private receipt identity, candidate module projection
identity, artifact/data code hash, complete selected Script hash, RawTransaction
hash and code output index. Hash type must be data2; deployment sequence, line
and history fields must be zero under this data-hash profile. All members are
checked, including inactive, below-floor, unselected and final members. Count
must equal the checked catalog count. The existing wire constructor separately
rejects duplicates, conflicts and malformed tags/fields before a policy exists.
Directional required/candidate compatibility was checked for the entire private
catalog; an added candidate API remains recorded rather than erased.

The native binding schema/domain are
`cellscript-frozen-code-policy-bindings-v1` /
`cellscript-frozen-code-policy-bindings-id-v1`. Its record contains the actual
source-catalog identity, declared policy root and canonical wire-index-to-catalog
candidate mapping. Source capture/catalog receipt bounds remain unchanged;
this operation parses no new artifact or raw transaction. Source rechecks retain
their separate 16 MiB ceiling. Membership consumes exactly the existing 656-byte
selection and fixed depth-five tree, without expanding runtime budgets.

`check_selection` verifies under this privately retained snapshot's root, applies
its exact/compatible, active/yanked and admission-sequence selection rules, and
rechecks actual source ownership. Its private result borrows the real candidate
and retains the checked membership. `check_unchanged_inputs` then rechecks the
selected receipt's original four-file bundle, RawTransaction, output index and
complete Script, plus the actual source closure. These are the original code
creation inputs; they are not a final transaction or resolved CellDep proof.

Status, policy/admission sequences and floors are application-declared immutable
snapshot choices. This layer does not authenticate Registry freshness, package
version floors/downgrades or deployment history. The expected root must later be
bound to committed current-Script code/args or controlled state; a witness cannot
authorize its own root. This result is not a nominal source handle or stable
admission token. Active chain/VM context, Type history, complete general receipts,
source `I`, H2, ProtocolBundle/generated-builder parity and independent review
remain required. No consensus, liveness, behavioral equivalence, production
admission or final signing/materialization claim is inferred.

`tests/frozen_interface.rs` covers exact/compatible actual implementations at
O0–O3, all 32 members, an independent binding hash oracle and canonical mapping.
Valid reconstructed Merkle trees with 12 changed header/member axes reject,
including an unselected member. Selection tests cover declared yank/floor/root
changes; later exact input and real source edits reject. An actual independently
checked Type-hash native catalog fails closed instead of accepting supplied line
and history hashes. These are host tests, with no new CKB-VM claim.
