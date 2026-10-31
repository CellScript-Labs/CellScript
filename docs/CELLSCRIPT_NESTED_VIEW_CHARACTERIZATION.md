# Nested-view representation: current boundaries

Status: structured executable IR implemented; final-source backend/integration acceptance is pending.
Date: 2026-10-30. Scope: alpha.2 item A2-04.

## Consumer inventory

| Boundary | Current responsibility | Text that remains legitimate |
| --- | --- | --- |
| [Source descriptions](../src/nested_values.rs), [data-shape parsing](../src/parser/data_shape.rs) | Parse bounds and admit supported record fields | Source syntax and diagnostics |
| [Nested checking](../src/types/nested.rs), main type checker and Cell selection checking | Resolve records, check bounds, borrowing and argument compatibility | Source types before resolution |
| [Generics](../src/generics.rs) | Expand transparent shape aliases and concrete substitutions | Source declarations and generic arguments before instantiation |
| [IR](../src/ir/mod.rs) and [nested descriptions](../src/ir/nested.rs) | Carry resolved element identity, layout and bounds; derive structured index/field results | Source-to-IR admission and final canonical rendering only |
| [Metadata](../src/lib.rs), [typed semantics](../src/typed_semantics.rs) | Render parameter/layout contracts and bind decoded values | Canonical external text, kept separate from internal references |
| [Nested emission](../src/codegen/nested.rs), frame and expression emission | Consume admitted layouts and explicit pointer/length ABI classifications | User-facing diagnostics and source fixtures |
| [Nested builder](../src/nested_builder.rs) | Encode values against serialized metadata | Versioned external schema/type text |
| [LSP views](../src/lsp/cell_views.rs), [view index](../src/lsp/view_index.rs) | Resolve source views, fields, definitions and completions | Source spelling and user-facing rendering, scoped to the document/dependency revision |
| [Independent checker](../crates/cellscript-artifact-checker/src/bounded_nested.rs) and nested machines | Validate contracts, decoding, bounds, source identity and lifetimes | Independently validated wire contracts; no compiler reference is proof |

The source checker now retains compilation-scoped parsed/resolved descriptions,
including record owner, concrete specialization, ordered admitted layout and
separate import spelling. Repeated field queries reuse admission; a fresh
resolver/document compilation receives a fresh cache. The same `NestedType<E>`
representation describes source elements and resolved `Element` values.

Codegen consumes the same admitted ordered fields through `IrType::Nested`.
Index and payload-field results retain structured descriptors; they do not format
another name for later reparsing. The two checked-width expectations and the
separate string-keyed nested-record layout reconstruction have been removed.
The independent checker retains its own codec, source, transport and lifetime
validation; a compiler reference supplies none of that proof.

The source-description checkpoint passes all 32 native nested tests, four
description tests, the resolver-revision reuse test, eight resolver tests and
all-target clippy. Direct/generic shape-alias ELF, interfaces, typed semantics
and complete native receipts are byte-identical before/after at the same source
locations. The local immutable source/check receipt is in
`target/alpha2-work/effect-facts-migration/nested-description-checkpoint/` and the
comparison is `nested-generic-native/description-foundation-parity.json`.
This is a working-tree checkpoint; no separate Git commit is claimed.

## Structured IR checkpoint

Cache v144 (`0.33-structured-nested-views`) separates the new internal
representation. The executable source/ABI admission boundary resolves recognized
nested names or reports an error. Unresolved named nested IR rejects with E2105;
there is no alternative executable string representation. Source generic
declaration projections and external metadata readers remain textual boundaries.

Three IR tests cover structured index/field results, equal aliases of one
declaration, unequal same-name owners, and rejection of unresolved/malformed
types. The 32 existing native nested regressions pass. A permanent generic-record
shape-alias regression adds native/checker/VM positive and wrong-key cases in both
optimization configurations. The 30 host WASM tests pass with `--features wasm`,
including canonical type strings in serialized browser-profile metadata.
Twenty-one effect/binding/interface regressions and all-target compiler clippy
also pass. The new native source-policy guard rejects nested reparsing/name
reconstruction in migrated IR/codegen files; source/diagnostic fixtures remain
allowed at their declared boundaries.

The same-location direct/generic alias probe before/after v144 preserves complete
ELF, public interface, typed semantics and complete compile-receipt bytes.
`nested-generic-native/structured-ir-parity.json` records both compiler hashes and
all comparisons. This is focused validation, not a full backend-gate result.

## Confirmed baseline behavior

[The named-shape regression](../tests/bounded_nested.rs) compares named
`data_shape` declarations with their expanded forms under normal compilation and
the diagnostic configuration without optimizations. They have identical public
interfaces, interface hashes, external parameter type text and executable ELF
bytes after removing the metadata trailer. Existing positive and malformed-wire
cases also retain identical VM outcomes and cycle observations. The separate
[interface test](../src/interface.rs) covers renaming shape aliases.

Thus a shape alias such as `Messages` is transparent at these boundaries: its
parameter is rendered as `NestedValues<Vec<u8>, 3, 8, 128>`. This does not imply
that every other source alias is erased. Imported record aliases remain visible
in parameter text; the metadata-only characterization renders
`NestedValues<Left, 2, 8, 128>` and `NestedValues<Right, 2, 8, 128>`.

The new cross-owner regression defines `left::Entry` with an eight-byte identity
field and `right::Entry` with a one-byte identity field, importing them as `Left`
and `Right`. Both optimization configurations compile under `DenyFailClosed`,
pass independent bundle and interface inspection, preserve each metadata field
type, and execute the corresponding witness encodings in CKB-VM. Exchanging the
encodings rejects with runtime error 16. These cases passed on the baseline named
IR representation and remain permanent structured-IR regressions. Bare map keys
alone therefore do not establish a collision;
valid same-name records from different owners must remain admitted.

A separate metadata-only probe found a narrower limitation: passing
`NestedValues<Left, ...>` to an imported helper declared with its owner's
`NestedValues<Entry, ...>` rejects as a string type mismatch. This probe does not
establish a native machine exploit or complete interface/receipt behavior. The
structured-reference migration must test this path through the resolver,
external IR merge, metadata and independent checker before changing admission.

The generic baseline uses `Entry<T: fixed_value> { key: T, payload: Vec<u8> }`
instantiated with `u64`. Native compilation of the expanded type and a transparent
`data_shape Messages` alias produces the same external type text,
`NestedValues<Entry__mono__753634, 2, 8, 128>`, the same public interface,
interface hash, typed semantic hash and complete ELF bytes. Both emitted bundles
pass independent `verify-artifact` checking, source verification and compile
receipt binding. The native probe uses the explicit Cargo `target/debug/cellc`
binary built for the v143 candidate and records its SHA-256.

The v4 receipts preserve the same artifact, profile, ProofPlan and protocol-graph
hashes. Their source hashes and complete metadata hashes differ, as expected for
different source declarations and source-unit locations. These are unsigned
receipt envelopes: successful payload binding does not establish signer
authentication. This probe emits and checks artifacts but does not execute them
in a VM; VM evidence above belongs to the separate witness/owner regressions.

The current distributed browser compiler agrees on the generic parameter text
and bounded public summary. Direct syntax works in both editions; `data_shape`
declarations remain Edition 2027 syntax. Browser summaries omit public interface
records and native receipts, so their equality is not inferred from a browser
result. The representation migration must preserve that profile boundary.

Local comparison artifacts are under
`target/alpha2-work/effect-facts-migration/nested-generic-native/` and
`nested-generic-characterization.json` in its parent. They record observations
of a working-tree candidate, not published release evidence. Permanent native
regressions in `tests/bounded_nested.rs` now include the generic probe as a
native/checker/VM regression; its implementation evidence is recorded above.

## Migration constraints established by the baseline

Semantic record identity must retain the defining owner, concrete substitution
and admitted ordered layout. Source spelling belongs to rendering/diagnostic
information and must not accidentally determine semantic equality or hashing.
Caches must belong to a compilation or document/dependency scope; a spelling-only
global cache cannot establish owner identity or survive dependency edits.

Keep the existing transparent-shape ELF/wire/interface parity tests throughout
the migration. Record intentional differences for previously rejected cases
separately. Internal references, cache keys and implementation IDs must not enter
serialized browser/native metadata, interface hashes, receipts or witnesses.
These constraints do not relax value provenance, decoder validation, bounds or
borrow lifetimes.
