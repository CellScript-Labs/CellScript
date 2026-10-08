# Frozen native code-policy field bindings

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
snapshot choices. The finite artifact-only constructor does not enforce source package version
floors. Neither constructor authenticates Registry freshness or deployment
history. The expected root must later be
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

## Source receipt and version profile

`freeze_source_code_policy` consumes the same private actual source/catalog and
wire snapshot through an explicit separate constructor, returning the distinct
private `FrozenSourceCodePolicy`. Its `FrozenSourceCodePolicySelection` is also
separately typed. Artifact-only proofs cannot convert to either source proof. Its Header required
interface is the resolver-owned source identity, including the actual baseline
and selected defining package/dependency closure. Dependency alias spellings do
not replace that owner. Each Member receipt is the candidate's private
`CheckedSourceCodeReceipt` identity, so source context, defining module, package
coordinate/edition/full version and finite artifact receipt enter the wire root.
All prior API, codec, network, complete Script and creation input checks remain.
An artifact-only receipt or required module-API identity cannot substitute for
these source bindings, even under a freshly constructed valid Merkle root.

All members, including unselectable history, must match the baseline source
name, namespace and edition. A selectable member is Active, between the declared
minimum admission sequence and policy sequence, and (in exact mode) has the
exact selected receipt. Both modes require selectable source versions at or
above the actual baseline's SemVer **precedence**. Build metadata does not affect
ordering, but its full text remains committed by the source receipt. Compatible
mode additionally requires a stable baseline and candidate and the same major.
Exact mode can pin a prerelease or changed major at/above the baseline if all
independently checked required contracts still hold; it cannot bypass a version
downgrade. Intentionally selecting an older line requires explicitly pinning an
older baseline in the consumer's source closure.

Yanked, below-floor, or nonselected exact members may retain an
older or incompatible-major source version. They remain fully byte/API/source
checked and cannot be selected. Members above the policy sequence reject in the
existing wire constructor before native policy construction. Status and admission sequences remain declared
snapshot facts. Manifest versions remain source facts, with no publisher,
authenticated Registry release, freshness or historical status claim.

This source profile uses native schema/domain
`cellscript-frozen-code-policy-bindings-v2` /
`cellscript-frozen-code-policy-bindings-id-v2`. In addition to the v1 fields it
records the actual baseline source package/version, ordered candidate source
receipt IDs and rule `source-coordinate-exact-floor-compatible-stable-major-v1`.
The separately named artifact-only constructor retains its v1 record/domain
and omitted source-profile fields. Historical JSON cannot construct private
proofs. The wire layout, 656-byte selection, tree depth and runtime budgets are
unchanged. Both profiles return host evidence; immutable on-chain root
authorization, complete H1/H2 and independent review remain pending.

Five additional native tests cover actual source receipt fields and independent
hash oracles, all O0–O3 version cases, exact/compatible selection, 32 members and
oversized version text. A real manifest-only version edit produces identical
four-file bundle/finite receipt bytes but a different source receipt and wire
root. Rebuilt policies with stale source receipts or artifact-only bindings
reject. Later consumer-closure edits reject selection. Candidate snapshots
retain their actual historical source facts after physical source/manifest
edits; using a newly captured candidate requires its new receipt, even when
its artifact bytes are identical. Stable build-metadata
variants share precedence while retaining distinct committed source identities.
These tests supply no new CKB-VM or production claim.


The source/version selection can additionally consume itself into a private
[direct code dependency binding](DIRECT_CODE_DEPENDENCY.md) for canonical final
raw bytes and every supplied Cell snapshot. This finite data2/direct profile
rechecks actual source closure and rejects wrong/missing/duplicate/copied code,
malformed unselected Cells, dep groups and unproved Type history. It does not
certify supplied snapshots as consensus/VM resolution or freeze witnesses and
signatures. Source policy v2 record/domain and selection wire remain unchanged.

That private dependency proof can consume itself into the separate
[Type-group snapshot](DIRECT_TYPE_GROUP.md), binding complete transaction and
witness bytes, all supplied input Cells and derived group indices to its exact
candidate receipt. Consumer source rechecks remain required. It does not verify
signatures, authenticate supplied Cells or authorize a policy root; all existing
source policy and dependency records/domains remain unchanged.
