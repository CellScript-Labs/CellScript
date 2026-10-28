# CellScript Collections Support Matrix

**Status**: production boundary document for the current CellScript CKB profile.

CellScript supports dynamic data in several different layers. These layers must
not be collapsed into one generic "collections are supported" claim.

## Support By Layer

The #47–#49 bounded profile is **available in 0.33.0-alpha.1; stable admission
and full gates are deferred under the maintainer's alpha publication decision**.
The [scope review](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md) records its checked
contracts, initial authoring decision and remaining validation. Strict compilation
accepts supported shapes without blanket experimental proof flags; native bundle
checking still requires the machine, source, bounds and lifetime evidence.
This candidate is not a release or deployment receipt.

Edition 2027 module-local `data_shape`, `cell_roles` and `collections`
declarations centralize named limits and policies. Use ordinary reads and
non-escaping read-only helpers. Existing stack-backed Vec capacity semantics
remain unchanged. Exporting these declarations, mutable/member-level loans,
recursive values and owned dynamic elements remain outside this profile.

Allocated Vec elements and Map values accept ordinary fixed records, including
concrete generic records, within their 64/32-byte limits. Fields must recursively
be admitted scalars, byte arrays or records. Reads copy values into local storage;
helper returns use caller-owned buffers. Hidden Cells, references, recursive or
dynamic layouts and record keys reject. Runtime pointers never become serialized
identities.

Ordered members support `load_sorted` from a fixed array of at most 1024 entries
into an empty member. It validates order, deduplicates Set keys, keeps the last
Map value and reserves once for distinct keys. Scalar and fixed-byte values are
covered; record-valued batches and arbitrary-order sorting remain deferred.

Allocated members share a 32-byte descriptor and contiguous backing storage.
Vec retains insertion order; OrderedSet stores sorted unique keys; OrderedMap
stores sorted unique key/value entries. This is invocation-local memory, not
schema wire layout. Vec indexes directly and ordered members use binary search,
but complete operations also validate backing allocations by walking arena
records. Growth retains charged old buffers; middle insertion moves later items.
Use actual-count `reserve` when possible and `load_sorted` for already sorted
batches. The [measured comparison](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#experimental-named-collection-declarations)
explains the costs and rejects automatic maximum reservation that exceeds the
existing small-case ceilings.

The independent checker composes typed owner/loan CFG analysis with exact
constructor, allocation, resize/release, descriptor, element and ordered-operation
instructions. Checked argument/result transport, successful-path operation order,
arena isolation and copied-result lifetimes apply across ordinary and read-only
helper calls. Intervening writes must preserve copied bytes and pointer slots;
mutable record bindings use separate storage. The
[admission review](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md) maps each claim to its
rebound mutation coverage; no arbitrary whole-language equivalence is claimed.

The [complete transfer](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#full-transaction-collection-aggregation)
composes aggregation and deduplication with retained Cell reads and every required
Script group. Native/generated flat Note encoders feed the canonical adapter and
actual VM transactions. Its equivalent array program agrees on all 44 O0/O3
transaction cases. A separate fixture freezes 24 O3 resource observations; eight
additional 1024-input sorted-construction cases cover unique/duplicate keys, late
disorder and shared exhaustion. Existing finite ceilings remain unchanged;
unavailable heap or rejected-transaction totals remain explicit, and these cases
are not a global worst-case theorem.

[Locked-package fixtures](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#cross-package-readonly-business-composition)
exercise imported settlement/migration helpers and imported fixed-value generic
record copies after local owner drop. They retain source/bounds checks, canonical
transaction bytes and resource ceilings. Module-local declarations and non-escaping
borrows remain distinct from exported owner types or general heap ownership.

| Feature | Schema/ABI | IR construction | Runtime verifier helper | Production status |
|---|---:|---:|---:|---|
| `Vec<u8>` | Yes | Targeted | Targeted create/update-output verification | Supported for documented witness and cell-data paths |
| `String` | Yes | Targeted | Byte-vector verification | Supported as UTF-8 bytes at the schema boundary |
| `Vec<Address>` | Yes | Targeted | Fixed-element vector verification | Supported where metadata marks a Molecule dynamic field |
| `Vec<Hash>` | Yes | Targeted | Fixed-element vector verification | Supported where metadata marks a Molecule dynamic field |
| Fixed byte arrays | Yes | Yes | Exact-size verification | Supported |
| Stack-backed local `Vec<T: FixedWidth>` | Local-only | Yes | Codegen stack-backed lowering | Supported for verifier-local scalar, fixed-byte, and fixed-width named values |
| `Vec<Vec<u8>>` | Boundary | Boundary | No generic helper | Must fail closed unless a concrete lowering is added |
| `NestedValues<T, N, P, B>` | Bounded explicit witness, direct/transaction Cell fields and immutable whole-value helper parameters | Immutable indexing, fields, length and iteration | Complete byte/record codec validation on entry, each transaction field read and helper call; checked pointer/length source binding and retained buffers | Bounded strict-admission candidate; final gates pending |
| `data_shape` (Edition 2027) | Transparent named bounds for the same bounded codec | Immutable whole-value, `Shape::Item` and `Shape::Payload` read-only helpers | Complete list or element validation per helper call; no new wire or witness budget | Module-local bounded strict-admission candidate; final gates pending |
| `collections` (Edition 2027) | Named shared arena budgets and fixed member types; no wire type | Local construction, owner moves, copied reads and ordered iteration; read-only helper reads, iteration and forwarding | Independent allocation, descriptor, element, helper transport, write-isolation and per-block operation-path checks | Bounded strict-admission candidate; final gates pending; no whole-language semantic-equivalence claim |
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
source witness/Cell tests. Native/generated TypeScript encoders agree on bounded
wire vectors and complete transaction bytes. Final candidate gates remain pending;
see the
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
These scenarios are not an arbitrary program-path or worst-case proof. The initial
authoring decision keeps explicit actual-count reservation; declarative reservation
and allocator reuse require a separate measured improvement. See
the [budget explanation and acceptance boundary](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md#experimental-named-collection-declarations).

Use dynamic vectors for data that is still a single cell field, such as signer
lists, proposal payload bytes, NFT attributes, or launch distributions.

Use `BoundedCellSet`/`BoundedList` only for the checked shape above. The four
0.26 language examples cover variable-cardinality claims, 1–16 order
settlement, fragmented-Cell merging, and bridge/rollup batches. Generic
Cell-backed vectors and maps, dynamic element data, and non-group membership
proofs remain outside the production surface.
