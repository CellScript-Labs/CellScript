# Resolver-bound defining source owner

Native `resolve_code_catalog_source` consumes an actual privately compiled
consumer `FrozenPackageModule` and an actual checked `FrozenCodeCatalog`.
Its private `ResolvedCodeCatalog` construction path identifies the defining
baseline module in the consumer's pinned package/source closure. It requires
the separately root-compiled baseline's package name, namespace, version,
edition, manifest digest, source snapshot and compiler requirement to match
that actual defining owner. Every baseline source module must agree on owner,
relative path, source hash and size. All selected transitive package identities
must remain identical, including their Git/Registry/Local origin records.
Chain ID and genesis pins must agree. No owner label or exported context JSON
can construct this evidence.

Compiling a dependency directory as a root produces a RootSnapshot source
label. That label is not the dependency's actual selected origin in a consumer.
This binding retains the consumer's actual defining package identity; it never
replaces a locked Git URL/revision, Registry coordinate/revision or local source
snapshot with an asserted origin. The independently compiled baseline supplies
the checked API/codec/code evidence, and its exact source closure must match
the selected consumer closure. The stored code catalog remains immutable.

The defining owner record has schema
`cellscript-resolver-interface-source-owner-v1` and hash domain
`cellscript-resolver-interface-source-owner-id-v1`. It binds the actual defining
package identity, actual module, required checked module contract and all
selected defining-closure package identities. Consumer
context and candidate catalog identities enter a separate
`cellscript-resolver-code-catalog-v1` binding with domain
`cellscript-resolver-code-catalog-id-v1`. Renaming dependency aliases therefore
changes consumer context while preserving the defining owner. Changing an
actual owner or its defining snapshot changes the owner identity even when the
module name and checked API projection have identical shapes. Physical local
paths do not supply publisher identity; identical relocated local snapshots
remain the same snapshot according to the existing frozen-source model.

The consumer plus all catalog bundles, raw transactions and complete Scripts
share 16 MiB, with 4 MiB/file, before ownership traversal. Earlier source
compilation/catalog checking remain separately bounded operations. Getters
only return immutable owned snapshots and checked tokens. Serialization is
audit output, not a constructor.

This is a **host source-owner prerequisite** for nominal interface resolution.
It does not introduce source-level `I`, `ScriptHandle<I>` / `VerifierHandle<I>`,
complete H1 receipts, package-family/publisher inference, Type replacement
history, immutable authorization, on-chain enforcement, ProtocolBundle signing
admission or behavioral equivalence. Whole H1/H2, #29 and #44–#46 remain
unfinished. Independent security review still precedes stable admission.

The native integration tests in `tests/frozen_interface.rs` cover O0–O3 alias
invariance, same-module/same-API definitions from distinct actual packages and
rejected cross-owner binding. Changed source/version, absent imports and wrong
chain/genesis reject. A local Git fixture changes only commit history while
preserving source and manifest bytes; the transitive pin difference still
rejects. An already checked synthetic catalog just below 16 MiB exceeds the
combined ceiling after adding the consumer, and rejects before looking up its
deliberately absent defining module. Synthetic creation transactions remain
host byte oracles, not consensus or authenticated deployment evidence.
