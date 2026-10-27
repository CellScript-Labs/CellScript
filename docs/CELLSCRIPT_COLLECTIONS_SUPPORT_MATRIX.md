# CellScript Collections Support Matrix

**Status**: production boundary document for the current CellScript CKB profile.

CellScript supports dynamic data in several different layers. These layers must
not be collapsed into one generic "collections are supported" claim.

## Support By Layer

The #47–#49 [authoring direction](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#authoring-direction-adopted-on-2026-10-19)
requires named roles/bounds, explicit allocation policy at construction or scope
entry, and checked read-only helper composition before stable API freeze.
`data_shape` now provides an experimental Edition 2027 declaration of reusable
named limits, elaborated to the existing `NestedValues<T, N, P, B>` contract.
Neither spelling is a stable public API freeze. `collections` declarations now
validate named allocation budgets. Local allocation-backed Vec and ordered
Map/Set operations have experimental runtime lowering with mandatory independent
proof blockers. Existing stack-backed Vec capacity semantics remain unchanged.

Allocated Vec elements and Map values also accept ordinary fixed records,
including concrete generic records, within their 64/32-byte limits. Fields must
recursively be admitted scalars, byte arrays or records. Reads copy values into
local storage; helper returns use caller-owned buffers. Hidden Cells, references,
recursive/dynamic layouts and record keys reject. This is experimental value
coverage, not completion of cross-package owners or independent allocation proof.

Ordered members additionally support experimental `load_sorted` from a fixed
array of at most 1024 entries into an empty member. It validates order, deduplicates
Set keys, keeps the last Map value and reserves once for distinct keys. Scalar
and fixed-byte values are covered; record-valued batches and arbitrary-order
sorting remain deferred. The independent allocation proof blocker remains.

These allocated members currently share a 32-byte descriptor and contiguous
backing storage. Vec stores elements in insertion order; OrderedSet stores sorted
unique keys; OrderedMap stores sorted unique key/value entries. This describes
local allocated values, not the wire layout of schema `Vec` fields. Element
lookup is direct for Vec and binary search for ordered members, but complete
operations also validate the backing allocation by walking arena records. Growth
retains charged old buffers, and middle insertion moves later entries. Known
sizes should use `reserve`; already sorted batches can use `load_sorted` within
its bounds. See the [measured comparison](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#experimental-named-collection-declarations).

Private frame-backed arena, allocated vector and ordered Map/Set machine
primitives have focused VM/reference-model coverage and experimental source
construction and read-only helper transport. Independent allocation/ownership
machine proof and complete business acceptance remain pending. See the
[experimental memory contract](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#experimental-allocator-and-vector-machine-primitives).

The independent checker now reconstructs typed owner availability across moves,
drops, branches and loop backedges, retains lexical loans at joins and checks
read-only helper arguments for stale references. This typed-CFG check does not
prove allocator machine state transitions or remove the admission blocker.
An outer lexical alias used only for a nested reborrow retains its typed identity.
Constructor initialization is also bound to actual zero/live-flag/pointer stores
and unavoidable typed-block control flow, including large arena offsets. This
does not establish allocation, growth or retirement state-machine admission.
Allocation primitives independently check budget predicates, monotonic cursor
and event updates, record writes, zeroing and failure exits. A typed-block site
inventory rejects erased or extra ranges. Arena resize/release now bind record
lookup, allocation children, bounded copying and retirement without budget
refund. Reserve/drop bind member-specific descriptor validation, bounded growth,
checked lifecycle children and pointer/capacity publication or full clearing.
Owned push/insert/set/remove also bind index admission, exact copy/shift/zero
loops, same-member reserve children and length publication. Owned Map/Set search
binds key width/order, binary-search bounds and found/index results; update
parents bind search to the checked insertion/replacement/removal child and
duplicate/absent behavior. Owned reads bind indexed copies to private result
buffers, read-before-remove ordering, pop's index and conditional/required Map
lookup. Sorted construction binds the typed input count, empty destination,
non-overlap, ordering, unique-key reservation and last-value copying. Readonly
helper entry checks bind pointer/extent spills to incoming ABI words and protect
both slots through the memory-effect CFG, including mixed nested/coordinate
parameters. Coordinate-bearing calls may follow guarded collection writes in the
same typed block: exact temporary spills, bounds and restoration are checked,
and coordinate arguments must be reloaded afterward. Readonly helper bodies
also bind caller-relative descriptor checks, allocation record walks,
length/capacity/empty results, ordered searches and indexed copies into disjoint
private scratch. Exact extent checks and each read site's incoming pointer
identity are required, with typed inventories covering erased markers.
Index arguments now bind to typed constants/local slots, and copied scalar
results bind to typed destinations with exact byte order, Map key/value offsets
and `i32` sign extension. Both compose with the same adjacent checked member
primitive; erased transport inventories and bypass branches are rejected.
Fixed-value result publication also binds full-width private buffers and typed
pointer slots for u128, byte arrays and records. Complete argument lists of
scalars and pointer-backed fixed values bind key/value/default bytes and indexed
writes to the adjacent checked primitive. Record widths come from typed layouts;
missing preparation, source/member substitution and bypass branches reject.
Short-array argument copies distinguish packed words/literals from pointed field,
index and collection results using typed producers and reference conversions.
Nonempty sorted inputs bind their evaluated typed slot and packed/pointer
representation to the adjacent checked sorted primitive; empty input reads no
source bytes. Upstream value/pointer provenance, machine retirement and full
source correspondence are not established merely by these argument checks.
The subsequent operation-path proof binds each top-level collection primitive
to its typed source operation, alongside scalar definitions and Cell operations.
Successful typed-block paths must execute the operations in order; owner drop
must retire every declared member. This covers whole-operation bypasses without
claiming arbitrary source-CFG or value equivalence. Final profile admission and
integration gates remain pending, and the admission blocker is retained.
Arena helper arguments now require the correct source owner pointer from a checked
constructor or incoming parameter on every reaching machine path. Copies preserve
that identity; overlapping writes and reconstruction invalidate prior facts.
Owned extents must equal the declared frame arena span, and forwarded extents must
come from the same incoming parameter as the pointer. Targets, call order and the
expanded register/stack ABI are bound separately from complete machine retirement.

The [transfer-only business example](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#full-transaction-collection-aggregation)
composes Map/Set operations with retained Cell reads and complete transaction
execution. Dynamic writes have checked private-frame bounds, preserving Cell
buffers; this isolation proof does not certify allocation state transitions or
remove the allocation admission blocker. Native/generated fixed Cell encoders
agree for the transfer's flat Note schema, and their bytes enter the actual
adapter/VM candidate. An equivalent fixed-array program matches all 44 O0/O3
transaction cases. A separate resource fixture freezes 24 O3 observations and
numeric ceilings; repeated and distinct assets produce different cycle tradeoffs.
This finite comparison does not complete adversarial resource acceptance.

The [locked-package composition fixtures](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#cross-package-readonly-business-composition)
exercise imported readonly settlement/migration helpers and imported generic
record copies after a local collection owner is dropped. They preserve helper
bounds/source checks, canonical transaction bytes and existing resource ceilings.
Catalog, shape and owner declarations remain module-local; these fixtures do not
admit owner export, escaping borrows or complete source equivalence.

| Feature | Schema/ABI | IR construction | Runtime verifier helper | Production status |
|---|---:|---:|---:|---|
| `Vec<u8>` | Yes | Targeted | Targeted create/update-output verification | Supported for documented witness and cell-data paths |
| `String` | Yes | Targeted | Byte-vector verification | Supported as UTF-8 bytes at the schema boundary |
| `Vec<Address>` | Yes | Targeted | Fixed-element vector verification | Supported where metadata marks a Molecule dynamic field |
| `Vec<Hash>` | Yes | Targeted | Fixed-element vector verification | Supported where metadata marks a Molecule dynamic field |
| Fixed byte arrays | Yes | Yes | Exact-size verification | Supported |
| Stack-backed local `Vec<T: FixedWidth>` | Local-only | Yes | Codegen stack-backed lowering | Supported for verifier-local scalar, fixed-byte, and fixed-width named values |
| `Vec<Vec<u8>>` | Boundary | Boundary | No generic helper | Must fail closed unless a concrete lowering is added |
| `NestedValues<T, N, P, B>` | Experimental explicit witness, direct/transaction Cell fields and immutable whole-value helper parameters | Immutable indexing, fields, length and iteration | Complete byte/record codec validation on entry, each transaction field read and helper call; checked pointer/length source binding and retained buffers | Experimental VM path; stable artifact admission and builder parity remain blocked |
| `data_shape` (Edition 2027) | Transparent named bounds for the same experimental codec | Immutable whole-value, `Shape::Item` and `Shape::Payload` read-only helpers | Complete list or element validation per helper call; no new wire or witness budget | Experimental; module-local names, complete source proof and stable admission pending |
| `collections` (Edition 2027) | Named shared arena budgets and fixed member types; no wire type | Local construction, owner moves, copied reads and ordered iteration; read-only helper reads, iteration and forwarding | Independent allocation, descriptor, element, helper transport, write-isolation and per-block operation-path checks | Strict admission remains blocked pending final profile acceptance, gates and review disposition; no whole-language semantic-equivalence claim |
| Generated allocation-backed collection helpers | No | No | Fail-closed entry symbols | Not a production allocator ABI |
| `HashMap<u64, u64>` | Limited | Limited | No production helper surface | Unsupported/fail-closed for production contracts |
| `HashMap<Hash, Token>` | No | No | No | Unsupported; must fail closed |
| `HashSet<T>` | Limited | Limited | No production helper surface | Unsupported/fail-closed for production contracts |
| Fixed-width `BoundedCellSet<Resource, N>` + `consume_each` | `input` source; `1 <= N <= 1024` | Checked loop, predicates, and numeric outer `+=` accumulators | `bounded-type-group-inputs-v1` scans exact `GroupInput` order, decodes exact data, and checks runtime count | Supported for fixed-width resource data of 1–512 bytes; other sources and dynamic layouts fail closed |
| Fixed-width `BoundedList<Plan, N>` + `create_each` | `witness` source; versioned plan bytes | Checked loop, predicates, numeric outer `+=` accumulators, and one complete create template | `bounded-output-plan-v1` decodes the plan and verifies the same relative `GroupOutput`, output lock, data, capacity floor, and final count | Supported only when `12 + N * plan_width <= 4084`, the output is fixed-width with no custom identity, and lock/capacity policy is explicit; other shapes fail closed |
| Generic Cell-backed resource collections | No executable ownership model | No | No | Unsupported |

An unadmitted dynamic-element vector's `len`/`is_empty` must return runtime
error 24, never reinterpret its encoded byte length as element cardinality.
The bounded nested codec work has independent host, primitive CKB-VM and
experimental source witness/Cell tests. Native/generated TypeScript value
encoders agree on bounded wire vectors. Stable artifact admission and complete
transaction builder parity remain pending; see the
[implementation and evidence ledger](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md).

## Stack-Backed Local Vec Rule

The current backend supports bounded local `Vec<T>` operations only when `T`
has a known fixed width and the vector is verifier-local. These operations are
compiler-recognized stack-backed codegen lowering, not calls into a production
allocator ABI. The supported helper surface is:

```text
new, with_capacity, capacity, push, extend_from_slice, len, is_empty,
indexing, first, last, contains, set, remove, pop, insert, reverse, truncate,
swap, clear
```

Type annotations such as `let mut bytes: Vec<u8> = Vec::with_capacity(52)` or
`Vec::new()` propagate the expected element type into the constructor. Arity
and capacity-type checks still apply. Constructor context accepts owned fixed
elements of 1–256 bytes; references, Cell resources and dynamic/nested vectors
remain rejected. A failed annotated initializer retains its valid owned local
binding only for subsequent diagnostics, so later uses do not become undefined
binding errors. Any initializer error still prevents compilation.

`Vec::capacity()` reports the fixed stack backing capacity
(`256 / element_width`), not the requested `Vec::with_capacity(n)` argument.
`cellc explain generics` exposes each checked instantiation, including element
type, element width, backing model, helper set, and constructor provenance.

Generated public collection symbols in `src/stdlib/collections.rs` are kept as
fail-closed stubs unless a concrete checked runtime ABI exists. Do not document
or use those symbols as production allocation-backed `Vec`, `HashMap`, or
`HashSet` helpers.

`examples/registry.cell`, `examples/language/collections/registry.cell`, and
`examples/language/collections/order_book.cell` are
compiler/tooling examples for this local helper surface. They are not part of
the bundled CKB production action acceptance matrix.

## Production Rule

Supported dynamic values must have deterministic Molecule metadata and verifier
evidence:

- `molecule_schema_manifest` entry
- dynamic field declaration where applicable
- generated create or update-output verifier marker
- constraints or production-gate evidence for the entrypoint that uses it

Unsupported generic collections must not silently compile into a weaker runtime
shape. They must produce one of:

- compile-time diagnostic
- structured blocker in metadata/constraints
- explicit fail-closed runtime path with a registered runtime error

Every selected consensus-relevant ProofPlan status beginning with `gap:` is a
production blocker, including `gap:runtime-helper-required`,
`gap:builder-evidence-required`, and `gap:metadata-only`. A transaction builder
can supply evidence for construction, but that evidence cannot authorize a
consensus operation the emitted Script does not check.

## Bounded Lifecycle Runtime Boundary

0.26 implements two deliberately narrow consensus contracts. A supported
`consume_each` scans the current Type Script's canonical `GroupInput` view. It
accepts only exact fixed-width data, checks that every selected Cell is acting
in the Type Script role, executes the body once per element, treats only
`CKB_INDEX_OUT_OF_BOUND` as the end of the group, and probes index `N` so an
`N + 1` member cannot be hidden. Zero elements are vacuous only when the Script
is invoked by an output-side group member.

A supported `create_each` reads a `bounded-output-plan-v1` payload from the
entry witness. The payload is the eight-byte `CSBPLv1\0` magic, a little-endian
Molecule FixVec count, and fixed-width plan elements. Plan element `i` must
match relative `GroupOutput[i]`; the generated verifier checks the complete
Cell data template, exact output lock, declared non-zero capacity floor, and
that no additional group output exists. The outer entry payload remains
`CSARGv1\0` plus the ordinary four-byte dynamic argument length.

Both bodies may use pure `require` predicates and mutable numeric variables
declared outside the body through `+=`. That restricted accumulator surface is
what makes count conservation, amount conservation, and many-to-one merge
checks expressible without admitting arbitrary loop side effects.

The positive contract does not cover dynamic or recursive element layouts,
transaction-wide scans, Lock Script groups, custom output identities, omitted
locks, implicit capacity policy, or arbitrary mutation inside a bounded body.
Those shapes keep the 0.25 fail-closed behavior: production stops with E2105
and permissive artifacts return runtime error 24.

The accepted 0.30 source-selection, ordering, identity, stable-error,
independent-checker, shared-fixture, and maximum-resource rules for the input
half are fixed in the
[bounded GroupInput contract](CELLSCRIPT_BOUNDED_GROUP_INPUT_CONTRACT.md).

## Authoring Guidance

Use `cellc explain collections [INPUT] [--json]` inside an Edition 2027 package to
compare per-member maximum reservation with cumulative incremental growth.
The report includes allocation records, alignment, retired buffers and shared
byte/event shortfalls, including unused declarations. Its two fresh-owner
scenarios are also shown on collection declaration/constructor hover using the
same calculation, including resolved imported generic widths in the native
editor. The optional browser language service supports single-source hover.
These scenarios are not an arbitrary program-path or worst-case proof. Budget UX and
the declarative-preallocation decision remain open; neither ordinary copied
reads nor growing checker coverage freezes the experimental authoring API. See
the [budget explanation and acceptance boundary](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#experimental-named-collection-declarations).

Use dynamic vectors for data that is still a single cell field, such as signer
lists, proposal payload bytes, NFT attributes, or launch distributions.

Use `BoundedCellSet`/`BoundedList` only for the checked shape above. The four
0.26 language examples cover variable-cardinality claims, 1–16 order
settlement, fragmented-Cell merging, and bridge/rollup batches. Generic
Cell-backed vectors and maps, dynamic element data, and non-group membership
proofs remain outside the production surface.
