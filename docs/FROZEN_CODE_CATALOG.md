# Frozen native code catalog

Native `package::frozen_interface::freeze_code_catalog` consumes an owned
required `FrozenPackageModule` and 1–32 `CodeCandidateInput` values. Each input
contains an actual independently compiled frozen package module, canonical
RawTransaction bytes, the code output index and complete selected Script bytes.
It accepts no caller receipt, compatibility boolean, source owner label or
exported context JSON. The returned `FrozenCodeCatalog` privately owns every
source snapshot, complete bundle, checked external codec, code byte/target
origin, finite policy receipt, private source receipt and the original
deployment/Script bytes. Its getters are immutable; serialized
records cannot reconstruct it.

Every member must satisfy the required module's directional contract, the
current finite public-policy external-codec profile, the exact code-output byte
binding, the checked target deployment hash type, a private finite Type-policy
receipt and the required pinned chain ID/genesis. All required/candidate
four-file bundles and all raw transaction/Script inputs share **16 MiB** and
respect the **4 MiB/file** and caller artifact/record/source-map ceilings before
any new catalog, codec or transaction parser. The earlier native source
compilation remains a separate bounded operation. Empty and >32 candidate sets
reject before inspecting members. A failure in any member returns no catalog.

The record schema is `cellscript-frozen-code-catalog-v4`; identity uses canonical
JSON with domain `cellscript-frozen-code-catalog-id-v4`. It binds the checked
module-catalog identity, required source/codec identities and every ordered
candidate source, byte-origin, target-origin, finite and source receipt identity.
The experimental v4 record adds source receipts; historical v1–v3 records are
not reinterpreted as source-receipt-checked catalogs. Each origin retains
its exact RawTransaction hash, output index, ELF hash and complete selected
Script hash. Duplicate
(transaction hash, output index, complete Script hash) tuples reject. Two
different args at the same code OutPoint remain distinct; that does not authorize
either selection or introduce an args wildcard policy.

Every member exposes its immutable [finite receipt](FIXED_POLICY_RECEIPT.md).
Public constants reject because their values lack independent evidence in this
profile; generic declarations remain explicitly non-executable. Its private
`CheckedSourceCodeReceipt` also binds the actual captured source context,
defining module, manifest name/namespace/version/edition and finite artifact
receipt. It has no public constructor, mutable fields or deserialization path.
Coordinates are source facts, not authenticated publisher releases. Names and
namespace/module identifiers are bounded to 512 bytes, version text to 128 bytes,
before SemVer parsing, cloning or source-receipt hashing. The receipt schema/
domain are `cellscript-frozen-source-code-receipt-v1` /
`cellscript-frozen-source-code-receipt-id-v1`. See the separate
[source policy profile](FROZEN_CODE_POLICY.md) for selectable version checks.

This closes a **host-only source/API/byte-origin prerequisite** for #28. It does
not prove source-to-machine or predicate equivalence. It does not authenticate
transaction commitment, consensus validity, live code Cells, Lock authorization,
Type replacement history, status/yank/downgrade policy, consumer authorization or
immutable on-chain roots. Type-hash candidates are byte/profile-bound, not admitted
replacement histories. Full H1/H2, nominal `I`, source handles, ProtocolBundle /
generated-builder integration, #29 and #44–#46 remain unfinished. Independent
security review remains required before stable admission.

The native [resolver-bound source owner](RESOLVED_CODE_CATALOG.md) additionally
links the defining baseline to its actual selected origin inside a consumer's
locked source closure. It preserves this catalog's limited host evidence and
does not authorize a policy root or admit a source-level handle.

`tests/frozen_interface.rs` exercises all optimizer levels with independently
compiled same-ABI/different-predicate implementations, owned bytes after source
edits, malformed unselected member inputs, incompatible field widths and wrong
networks and otherwise byte-valid wrong VM hash types. It also checks the 32-member limit, repeated concrete deployments,
distinct complete args, per-input/caller/shared budget rejection before malformed
transaction parsing. Canonical transaction/Script construction uses the pinned
SDK types already used by the code-origin tests. These are host tests; no new
on-chain claim or full-CI evidence is supplied by this catalog constructor.
