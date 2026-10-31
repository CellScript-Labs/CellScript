# Bounded runtime foundations: Cell sets, nested data and allocated values

**Status**: initial #49/#48/#47 profile available in 0.33.0-alpha.1 after compiler
integration tests. Full gates and stable API admission were deferred by the
maintainer for this alpha. The
[admission review](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md) defines the current
checked scope and supersedes blanket-blocker statements in the implementation
history below. This document is not release or deployment acceptance.
Implementation owner: Codex under the maintainer's 2026-10-18 instruction.
Review disposition: author review during implementation; independent human
review has not occurred and no waiver from earlier issues is inherited.

Baseline: `43e92004b3289a75ef97d48003462b5e21656a32`. Work remains on the
current pinned toolchain and package version; the issues' 0.33 planning label
is not permission to bump a release version.

## Required outcomes and dependency order

1. #49 fixed-layout phase: explicitly admitted heterogeneous exact roles,
   complete bounded transaction selection, preserved global/group coordinates,
   unambiguous predecessor/successor correspondence and foreign-observation
   versus local-lifecycle authority. Then a variable-payload batch consumes
   #48's codec. Both examples need actual all-group execution.
2. #48: canonical nested witness and single-Cell data views plus actual successor
   validation. Independent host byte validation is the first prerequisite,
   not evidence that emitted scripts enforce the codec.
3. #47: explicit allocation-backed value Vec and deterministic ordered Map/Set,
   with ownership, growth, exhaustion and resource evidence. Existing 256-byte
   stack-backed Vec capacity semantics must remain unchanged.

The existing `BoundedCellSet`/`consume_each` and `BoundedList`/`create_each`
contracts remain intact. #49 must not grant foreign Cell ownership by widening
an existing GroupInput scan, or silently reinterpret group indexes as global
indexes. ProtocolBundle and generated builders remain the transaction boundary;
a second standalone serializer is not an acceptable shortcut.

## Authoring direction adopted on 2026-10-19

The maintainer adopted an author-facing design pass across #47–#49 before
freezing their stable public APIs. Existing experimental spellings are useful
implementation and verification inputs; they are not the final authoring
contract. This decision preserves every runtime, checker, builder, resource
and gate obligation below. It does not reopen the accepted stack-backed Vec
or same-group lifecycle contracts, assign a release version, or waive review.
The broader [authoring target](CELLSCRIPT_AUTHORING_TARGET.md) applies here.

Authors should declare identities, authority, correspondence and numeric
budgets where those choices are made, then write ordinary business predicates
against named, typed values. Shorter spelling must not hide policy decisions,
infer authorization or move on-chain checks into a builder.

| Workstream | Current experimental surface or open decision | Required authoring direction |
|---|---|---|
| #49 selection | Experimental named catalogs and immutable `selected.orders.input(i)` reads; positional primitives retained | Named roles and named limits declared once; role access determines the schema without repeating a type and numeric role identifier at every read |
| #48 nested data | Experimental `data_shape` declarations resolve named limits into the existing nested codec; derived view types remain inferred | Reusable data shapes with named count, per-payload and total-encoding limits; ordinary indexing, fields and iteration at use sites |
| #47 allocation | Experimental `collections` declarations and local runtime operations; read-only helper transport with independent allocation proof pending | Familiar collection operations; explicit memory/element budgets at construction or scope declaration, with visible exhaustion and invalidation rules |

The initial declaration form is selected in the complete-program decision below.
Stable admission still requires the runtime, checker, parity and gate evidence.
Declarations elaborate directly into the existing structured IR/semantic
contracts; a wrapper that merely renames long argument lists would not satisfy
the authoring requirement.

### Initial authoring form: complete-program decision, 2026-10-26

Implementation decision for the initial bounded profile: retain one module-local
named declaration for each distinct contract, ordinary reads and collection
operations, and non-escaping read-only helper parameters. This is a source API
choice, not a stable-admission or release decision. The author reviewed the
complete programs below; no independent human review or review waiver is claimed.

| Contract | Chosen form and reviewed program | Reason and retained boundary |
|---|---|---|
| Cell roles | `cell_roles`, `ckb::select_cell_roles<Catalog>()`, named role access; [mixed settlement](../examples/language/batches/mixed_settlement.cell) | Two roles reuse `Balance` while retaining distinct Script identity, authority and correspondence. Helpers receive typed input/output views; foreign observation grants no lifecycle authority. |
| Nested data | `data_shape`, inferred indexing and fields, named `Item`/`Payload` helper types; [byte migration](../examples/language/batches/nested_migration.cell) and [record migration](../examples/language/batches/record_migration.cell) | One shape supplies witness and Cell bounds without repeating the codec limits at each read. Byte views retain their source and lifetime; the record case keeps revision identity separate from payload comparison. |
| Temporary collections | `collections`, `Owner::new()`, per-member `reserve(count)` and familiar Vec/OrderedMap/OrderedSet operations; [transfer](../examples/language/batches/multi_asset_transfer.cell) | Shared budgets and member limits are declared once. The four reservations express two checked transaction counts. Aggregation, duplicate detection and `conserved(&totals)` need no allocator arguments at use sites. |

For the initial collection API, keep explicit per-invocation reservations next
to construction. Do not silently reserve every declared maximum or introduce a
second constructor spelling solely to combine four calls. The complete transfer
comparison below shows why: full-maximum reservation raises the one-Note case
from 44,258 to 158,322 cycles and exceeds the existing observed-stack ceiling.
The CLI/editor budget report exposes alignment, allocation records and retained
growth history, so authors can inspect combined costs without calculating them
by hand. This makes explicit reservation a usable initial policy; it does not
make `items` a simultaneous-capacity guarantee or erase cumulative allocation
costs. A later declarative reservation proposal must preserve checked runtime
counts and demonstrate an improvement on these complete programs.

Keep explicit `borrow` for lexical read-only regions, copied ordinary reads,
and whole-owner read-only helpers. None of these four reviewed programs needs
member-level mutation during a live whole-owner borrow. Mutable/member-level
borrowing therefore remains outside this initial choice, rather than being
added to make these examples compile. Existing negative escape/stale-view
tests, module-local declaration boundaries, the stack-backed Vec contract and
all mandatory proof-admission blockers remain in force.

The 2026-10-22 implementation begins this declaration layer with Edition 2027
module-local `cell_roles` catalogs and `ckb::verify_cell_roles<Catalog>()`.
The parser retains named policies as structured AST, schema checking resolves
identity fields, and lowering emits the existing exact-role contract directly.
Named limits and inclusive count intervals are formatted without positional
rewrites. This is the [experimental verification slice](CELLSCRIPT_BOUNDED_CELL_SELECTION.md#experimental-named-verification-catalog),
extended by named selection, immutable role aliases, inferred view schemas and
role-local input/output/successor reads. Catalog names and role bindings remain
in the typed contract and independent pointer/read checks. Reusable nested
shape declarations are now implemented as described below; exported catalog
interfaces, complete helper/source proofs and the complete business-program
acceptance below remain open.

Role navigation and rename retain catalog identity across comments between the
receiver and role, including multiline comments, parenthesized receivers and
range endpoints such as `0..(selected) /* explanation */ .quote.input_count()`.
The complete settlement editor regression applies the rename, recompiles the program and preserves
same-named roles in another catalog, unrelated locals and comment text. Rename
requests on comment or string contents produce no edits. The optional browser
language service checks the same definition locations; this is local editor
coverage, not completion of the package/API admission requirements below.

### Experimental named data shapes

Edition 2027 authoring now accepts module-local reusable codec declarations:

```cellscript
data_shape Messages {
    element: Vec<u8>,
    limits {
        items: 32,
        payload_bytes: 256,
        encoded_bytes: 4096,
    }
}
```

Use `Messages` as a witness parameter or a direct Cell field type. A record
shape instead names an ordinary record containing supported fixed identity
fields followed by one `Vec<u8>` payload. The
[byte/record example](../tests/syntax_combo/seeds/named-data-shapes.cell) uses
both shapes, nested iteration and ordinary record/payload field reads.

The parser retains structured element/limit fields. Frontend type elaboration
resolves the declaration once into the existing concrete `NestedValues` type
contract; it does not generate and reparse source. These are transparent codec
shapes, not new nominal wire types or allocated collections. Renaming the shape
preserves its interface, builder contract and bytes; changing a numeric limit
changes the relevant bound identities. Names are currently private to their
module. Public/package exports, parameterized shape declarations and recursive
shape references are not supported. Missing/duplicate/unknown limits, invalid
ranges, conflicting names and unsupported elements reject, including in unused
declarations. Declaration elements resolve in their declaration scope rather
than capturing a use site's generic type parameters. Forward declarations and
reordering preserve the resolved interface and entry bounds; this does not
require identical ELF layout after arbitrary top-level item reordering.

The limits remain independent: at most 32 items, at most 256 payload bytes and
4–4096 complete encoded bytes. Zero item/payload limits preserve their existing
meaning. `encoded_bytes` includes the codec's count/offset headers; it does not
raise the separate 4096-byte complete entry-witness budget. Internal derived
view types are inferred at indexing/iteration sites. Whole shapes and their
derived item/payload views can be passed to immutable observation-only helper
parameters. Names do not lift escape restrictions or grant Cell lifecycle authority.

Focused evidence compares the named and expanded forms: identical interfaces,
resolved parameter contracts, schema manifests and generated/native builder
bytes; identical ELF and O0–O3 witness results/cycles; and actual single-Cell
successor checks for identity, payload, malformed encoding and capacity. Builder
vectors include empty/mixed/maximal byte lists, records and exceeded limits.
Formatter, declaration completion/navigation/rename, derived-view completion
and WASM metadata tests cover the source surface. Authoring completion templates
for named shapes and catalogs are compiled after expanding their defaults;
they are not offered as native-preview declarations. Independent checker mutations
reject altered resolved bounds and decoder instructions. Runtime nested values
also require `nested-values-artifact-proof` in the bound metadata: deleting that
marker and rebinding sidecars rejects. An unused schema by itself does not imply
runtime decoding. These checks do not establish complete source equivalence,
general helper composition or stable admission.

Reproduce with `cargo test --locked -p cellscript --lib named_data_shapes`,
`cargo test --locked -p cellscript --test bounded_nested named_data_shapes`,
`cargo test --locked -p cellscript --test nested_builder named_data_shapes`,
`cargo test --locked -p cellscript --test artifact_checker named_data_shapes`
and `cargo test --locked -p cellscript-wasm --features wasm --lib wasm_named_data_shapes`.

### Borrowing and helper composition

Retain `borrow root as view { ... }` for an explicit read-only region over a
linear resource. Its closing brace makes the end of the borrow visible before
a later lifecycle operation. Ordinary permitted field reads should remain
direct; authors should not need a new borrow block and alias for every read.
The existing observation-only helper and compatible `&T` rules remain intact.

Read-only is a common capability, not a reason to erase different ownership
or storage contracts:

| Value | Boundary that must remain visible | Helper contract to establish |
|---|---|---|
| Lexical resource borrow | Same linear root, block lifetime, no lifecycle operation while borrowed | Preserve existing compatible `&T` calls; no escape or ownership transfer |
| Transaction/Cell data view | Exact source/index and schema, observation without lifecycle authority | Checked read-only calls must retain the selected source and nominal role |
| Nested witness/Cell view | Validated codec, bounds, root and buffer lifetime | Checked read-only calls must retain root, bounds and backing bytes across calls |
| Allocated element view | Owner allocation, arena lifetime, mutation/relocation invalidation | Reject stale aliases across growth/removal/drop; no pointer serialization |

Classify each rejection as either a semantic prohibition or a temporary
implementation/proof limitation. Current invocation-local data and nested view
call restrictions remain enforced until their call transport and lifetime
proofs exist. Stable acceptance must demonstrate bounded, non-escaping helper
composition for the selected use cases; forcing all predicates into one large
action is not completion. This does not authorize returning borrowed views,
storing them, general mutable borrowing or passing foreign lifecycle authority.

The 2026-10-21 experimental slice permits immutable typed input/output views in
observation-only helper parameters, with local `.data` projections and transitive
forwarding. It preserves nominal source kinds and schema, and carries the
encoded coordinate across register/stack arguments. The
[predicate example](../examples/language/ckb/typed_cell_predicate.cell) and
`tests/cell_data_views.rs` exercise this boundary with global/group distractors,
wide-field mismatches and incorrect lengths. Helper metadata conservatively
retains `SourceView` rather than claiming a known caller coordinate domain.
Field-body checker mutations reject altered loads, but complete call-site
source/value proof, nested-view helpers and allocated-view invalidation remain
pending. Existing stable-admission blockers remain in force.

The 2026-10-22 checker slice independently binds typed helper call targets,
order and coverage, the coordinate's caller-local slot, register/outgoing-stack
placement, and the callee's incoming-parameter spill. Expanded ABI positions
come from the typed parameter declarations, including wide arguments and hidden
fixed-result pointers. Control-flow joins discard argument facts; an incoming
edge cannot skip setup or enter after a parameter spill. Caller slot writes
invalidate previously loaded argument facts. The recognizer has finite per-call
and aggregate instruction budgets and does not extend the accepted wire ABI.
Rebound mutations substitute source loads, incoming spills, outgoing offsets
and same-schema typed call operands.
The 2026-10-23 checker extension carries the fully checked parameter prefix into
the enclosing CFG memory-effect proof and protects each transaction-coordinate
parameter's eight-byte slot for the rest of the helper. The earlier transport
check alone accepted a rebound machine mutation redirecting an ordinary arithmetic
store into that slot. Direct, partial and overlapping writes now reject, including
helpers that only read transaction properties and never project `.data`.
Mixed nested signatures retain their independently checked decoder roots when
the prefix advances to the body. Outgoing stack extents are derived from the
checked callee's expanded, aligned ABI, including wide/nominal arguments and
hidden fixed results; a scalar parameter-count estimate cannot truncate them.

O0/O3 mutations cover both Input/Output parameters, register and stack positions,
all four store widths and final-byte overlap. Existing forwarding, branch and
wide-result programs retain acceptance. This parameter-lifetime proof does not
remove the stable-admission blocker.

The 2026-10-24 extension independently recognizes caller-local coordinate
constructors and aliases. It binds each typed index operand load, source-family
helper, 32-bit index guard, tag packing and destination store; aliases preserve
their nominal type and exact scalar slots. Typed CFG coverage requires every
producer, even when all its markers are erased. The existing data/selection
instruction proofs compose with these ranges, including relaxed branches.
The enclosing memory analysis requires live coordinates at aliases, data reads
and helper calls, invalidating them on full/partial writes and at incompatible
joins. Checked selected-role reads and helper parameter prefixes establish
their corresponding roots. Complete field-result/source equivalence and final
admission remain separate obligations; the mandatory admission blockers remain.

Typed Input/Output view scalar properties now compose with these coordinate
roots: `capacity`, `occupied_capacity`, `unoccupied_capacity`, `data_size` and
`output_index` bind the coordinate load, resolved runtime call, required error
handling and scalar destination. Their complete runtime bodies independently
bind source/index decoding, syscall buffers and returned values, occupied-size
validation, subtraction underflow and frame restoration. Every property call
participates in bounded-body operation ordering. The
[property mutations](../tests/artifact_checker/bounded_properties.rs) cover
O0/O3 and transaction/group sources; zeroed runtime results change an actual
VM rejection to success and must still reject after all identities are rebound.
Substituted call coordinates/results, ignored statuses and erased call evidence
also reject. The memory-effect pass requires a live coordinate at the actual
property call; an intervening helper result redirected into its slot invalidates
that coordinate, even when the property instructions themselves remain intact.
This finite property contract does not admit general raw runtime
calls or complete upstream aggregate computation.

The same property-call contract covers `data_hash`, `lock_hash` and `type_hash`
on typed Input/Output views. It composes the existing full hash-helper runtime
proof with the caller's live coordinate, private size/output buffers, status
guard and published result pointer. Distinct property results cannot share
storage, and unrelated stores, calls and syscalls must preserve those buffers.
The [hash-property mutations](../tests/artifact_checker/bounded_hash_properties.rs)
substitute sources, coherently alias result storage and redirect an intervening
helper's store; actual VM outcomes expose the substitutions. O0/O3 large-frame
cases retain bounded addressing. General hash computation and raw untyped APIs
are outside this property extension.

### Selection and data use sites

The following now has an experimental fixed/bounded-table implementation through
named catalogs. It remains a candidate grammar, not a stable API freeze:

```text
let orders = selected.orders
let before = orders.input(i)
let after = orders.successor(i)
require before.data.amount == after.data.amount
```

The surrounding role declaration must still bind the exact admitted Script
identity, schema, identity field, authority, correspondence and budgets.
`i` remains an ordinal within that named role; global/group coordinates remain
explicit when requested. Reordering role declarations must not silently retarget
an operation. Resolve field names and policy choices structurally so unknown
fields, incompatible schemas and invalid policies are diagnosed at declaration.
Retain `.data` to distinguish contract data from transaction fields such as
capacity and Lock identity. Reuse existing deployment/ProtocolBundle bindings
for authenticated policy inputs; a named declaration is not authentication.

For nested values, retain separate item-count, per-payload and complete encoded
byte limits, including the distinction between codec and entry-witness budgets.
Naming or reusing a bound must preserve its resolved value in interfaces,
generated builders, metadata and independent checking. A codec change requires
its own compatibility decision; authoring cleanup does not silently change wire
bytes. Inferred `NestedBytesView`/`NestedRecordView` names should not become
extra declarations authors must repeat to use ordinary indexing and fields.

For allocated collections, choose and document the storage policy at creation
or scope entry. Do not silently change old `Vec::capacity()` semantics or
require allocation controls on every lookup/insert. Deterministic ordered
Map/Set semantics must be visible in the chosen API, including iteration order,
duplicate handling, failure behavior and worst-case operation bounds.

### Authoring acceptance before stable API freeze

Use complete mixed-role settlement, variable-payload batch migration, asset
aggregation and identifier-deduplication programs to evaluate the public API.
For each, retain the experimental/current spelling and proposed authoring form,
showing declarations, business predicates and helper extraction together.
Reviewers must be able to locate source, identity, authority, pairing and budgets
without decoding positional argument lists or implementation-only view types.

Required evidence includes declaration reorder/rename cases, unknown role/field
and budget errors, helper source substitution and stale/escaping views. Compare
accepted/rejected transactions and canonical bytes through compiler, checker
and existing builders; preserve resource ceilings and explain any changed cost.
Record formatter, diagnostics, completion/rename and browser-summary behavior
for the selected syntax, plus the authoring decision and review disposition.
Readability review supplements all-group execution and proof evidence; neither
can substitute for the other. The examples and helper requirements belong to
these issues and cannot be moved to an unspecified future ergonomics issue.

The complete migration comparisons retain both declarations and helper bodies:

| Program | Explicit bounds at use sites | Named shape at use sites | Policy retained in both |
| --- | --- | --- | --- |
| [Byte-list migration](../examples/language/batches/nested_migration.cell) / [explicit counterpart](../tests/selection_bundle/fixtures/nested_migration_explicit.cell) | `NestedValues<Vec<u8>, 8, 256, 2120>` on the Cell field and witness; `NestedBytesView<256>` on helper parameters | `Messages` on the Cell field and witness; `Messages::Item` on helper parameters | Document identity and history preservation, one appended note, next Registry revision, capacity, source roles and peer Script obligations |
| [Record-list migration](../examples/language/batches/record_migration.cell) / [explicit counterpart](../tests/selection_bundle/fixtures/record_migration_explicit.cell) | `NestedValues<Message, 8, 256, 2280>`, `NestedRecordView<Message, 256>` and `NestedBytesView<256>` | `Messages`, `Messages::Item` and `Messages::Payload` | The same migration rules plus positive, ordered historical revision identities and the appended record's revision |

The named shape is the preferred authoring direction for these programs: count,
payload and encoding limits live together, and helpers refer back to the shape
instead of restating individual bounds. `cell_roles Migration` still owns source,
identity, authority and correspondence; the shape does not acquire those powers.
Both versions retain ordinary predicates and read-only helpers. This is a local
design assessment, not independent review or a stable API decision. It does not
resolve collection reservation syntax, member-level borrowing or the remaining
source/lifetime proof obligations.

## Initial nested codec contract (frozen before positive lowering)

Profile: `cellscript-bounded-nested-values-v1`. The two element shapes are:

- a Molecule `Bytes` value (`Vec<u8>`);
- a Molecule table with 1–8 fixed-width bitpattern fields followed by one
  `Bytes` payload. Each fixed field is 1–256 bytes. Source typing must further
  constrain which value types can occupy those fields; this width-only codec
  does not establish nominal identity, ownership, enum validity or permissions.

The outer collection is a Molecule DynVec. Absolute profile ceilings are 32
items, 256 payload bytes per item, and 4096 total encoded bytes including all
headers. Each concrete contract may choose smaller limits (including zero
items/zero payload); total bytes must permit the canonical empty value (4).
Record tables add one container level: byte-list depth is 2, record-list depth
is 3. There is no schema recursion or arbitrary nested object graph.
These are initial admitted shapes, not a restriction on future versioned work.

Rules follow the [Molecule encoding specification](https://github.com/nervosnetwork/molecule/blob/master/docs/encoding_spec.md):

- all size/count/offset words are little-endian u32;
- an empty DynVec is exactly `04 00 00 00`;
- a nonempty DynVec's first offset determines its exact field count;
- a record table has exactly the declared fields; compatibility-mode extra
  fields are not admitted by this profile;
- offsets are ordered and in range; each field consumes its complete slice;
- `Bytes` is an exact FixVec: its u32 count equals the remaining bytes;
- an empty payload has a four-byte zero count, not a zero-length field;
- unmodeled optional fields, arbitrary recursion and trailing bytes reject;
- all contract/total/count budgets are checked before traversing or allocating
  per-element views. Byte views borrow immutable input and cannot outlive it.

Initial host errors distinguish invalid contract, total budget, malformed
Molecule encoding (including truncated words, size mismatches and invalid
offsets), count bound, fixed-field width and payload bound. Runtime error
mapping and ELF obligations must be bound before the
compiler admits the corresponding executable source form. The internal emitter
uses a caller-provided terminal rejection label; its VM tests map every invalid
encoding to existing runtime error 16 (`dynamic-field-bounds-invalid`). The
source consumer must retain this terminal failure behavior rather than returning
an error number as an ordinary value.

### Implemented prerequisites and current evidence

The standalone checker's `bounded_nested` module validates borrowed byte and
record lists through the existing strict Molecule parser. Nine host tests cover
literal vectors, independent count/payload/total budgets, narrower and zero
budgets, truncation with rebound total sizes, field counts/widths, aliases,
overflow and unknown contract fields.

`src/codegen/nested.rs` emits a complete bounded decoder for both shapes. It
checks the entire value before returning a count and preserves its input
pointer and length. It uses no calls, allocation, stores or stack space. This
is now reached by the experimental `NestedValues<T, N, P, B>` witness path.
The experimental typed Cell path also validates actual inputs and successors.
Stable artifact admission, complete codec/source/machine obligations and
generated-builder parity remain pending. The explicit
`nested-values-artifact-proof` production blocker must remain until those
obligations are independently checked; successful experimental execution does
not discharge them.

Four CKB-VM tests in `src/codegen/nested/tests.rs` compare the emitted machine
code against the independent host parser, including literal encodings at all
eight byte alignments, truncation, rebound sizes, offset/length mutations and
invalid pointers whose arithmetic must reject before a load. The test fixture
encoder is not part of either validator. Measured primitive-only results on the
current working tree are:

| Shape and frozen joint maximum | Valid cycles | Late-invalid cycles | Test ELF bytes |
|---|---:|---:|---:|
| Bytes list, 32 elements and 4096 total bytes | 2808 | 2775 | 4968 |
| Record list, 32 elements, eight one-byte fixed fields, 4096 total bytes | 10008 | 9975 | 5464 |

The late-invalid case changes the final payload count while retaining exact
outer and record lengths. Decoder stack/heap use is zero; input storage, caller
frames and transaction loading are excluded. ELF sizes include the test entry
wrapper, ABI assertions and embedded data. These are not full-transaction,
witness-capacity or independently certified artifact measurements. Re-run with:

```bash
cargo test --locked -p cellscript-artifact-checker bounded_nested -- --test-threads=1
cargo test --locked -p cellscript --lib bounded_nested_machine -- --test-threads=1 --nocapture
```

The full-transaction regression `tests/bounded_nested.rs` reproduced a legacy
gap before the fix: an opaque nested vector containing one empty Bytes value
passed `items.len() == 12`, because its 12 encoded bytes became the reported
cardinality. Unadmitted vector operations now reject with error 24 before that
fallback. Three tests cover nested length, empty/record cases, and unchanged
fixed-element cardinality and malformed-size checks. This is a rejection-path
fix, not positive nested source admission.

### Experimental immutable witness views

An action or Lock can declare an immutable explicit witness parameter such as
`NestedValues<Vec<u8>, 3, 8, 128>` or `NestedValues<Record, 3, 8, 128>`.
The four arguments specify the element, item count, per-item payload bytes and
complete encoded value bytes. A record currently admits unsigned integers,
Hash, Address and fixed byte arrays of 1–256 bytes as its 1–8 fixed prefix
fields, followed by exactly one `Vec<u8>` payload. Boolean/enum validity,
recursive types and Cell-resource elements are not inferred from byte widths.

The entry validates the complete canonical value even when its parameter is
unused. Indexing derives invocation-local `NestedBytesView<P>` or
`NestedRecordView<Record, P>` values. Byte views support `len`, `is_empty`,
iteration and checked byte indexing; record views expose the fixed fields and
a byte view of the payload. Views may be aliased immutably. Construction,
mutable aliases, pointer equality and passing/returning views reject. Derived
views cannot be stored; direct `NestedValues` fields in Cell declarations encode
the complete canonical value, not an invocation-local pointer. Fixed identity
fields retain their declared value types.

The existing complete entry witness budget is 4096 bytes. It includes
WitnessArgs, CSARGv1, argument length prefixes and any other fields/parameters.
With one nested argument and absent Lock/output_type fields, its 32-byte
envelope leaves 4064 bytes for that argument. A codec bound of 4096 does not
increase the entry buffer or reserve space against other arguments. Codec
violations reject with error 16, indexing with error 21, and oversized entry
witnesses with error 25. Future builders must check both independent budgets.

`tests/bounded_nested.rs` covers source-to-transaction execution at O0–O3:
byte/record views, u128 high bits, Hash and 17-byte fields, immutable aliases,
nested loops, multiple parameters, zero/narrow limits, unused malformed values,
invalid indexes, the complete witness maximum and one byte beyond it. Its
formatter check preserves nested types; compiler diagnostics and syntax-combo
rejection seeds cover invalid bounds and mutable aliases. LSP completions mark
the types experimental. Rehashed typed-semantics mutations in
`tests/artifact_checker.rs` reject changed derived types and forbidden view
operations. These type checks do not yet prove the emitted validator itself.

For the 32-item byte-list fixture with 4064 encoded bytes (4096 witness bytes),
the existing scheduler observer measures the selected Type group independently
and requires its exit to agree with full transaction verification:

| Optimization | Successful group cycles | Last-item malformed count cycles |
|---|---:|---:|
| O0 | 71701 | 71402 |
| O1–O3 | 71658 | 71385 |

These include witness loading and entry validation. They exclude the separate
harness Lock group and are not primitive-only costs. Error-path counts come
from scheduler replay, never the legacy transaction-result zero placeholder.
Stack/heap observations and full maximum record/source evidence remain pending.
Reproduce with `cargo test --locked -p cellscript --test bounded_nested -- --test-threads=1 --nocapture`.

### Experimental single-Cell input and successor path

Resource/shared/receipt declarations may contain direct `NestedValues` fields.
Their enclosing Molecule table currently admits other bounded nested fields or
fixed unsigned/Hash/Address/byte-array fields. Other dynamic sibling fields need
explicit bounds and remain unsupported in this path. The compiler derives the
complete maximum Cell byte size from the table header and every declared field
bound. Each Cell uses a separate frame buffer sized to that bound (with the
legacy 512-byte minimum); existing types retain their prior allocation.
Standalone creates also receive dedicated buffers rather than shared scratch.

After a successful Cell-data load, the script checks the complete canonical
table and every nested value before application control flow. This includes
unused input fields. The exact first offset establishes field count, fixed field
widths are exact, and nested codecs check all payloads. Output checks compare
the canonical field bytes with the expected witness/input value. Cell ownership,
membership and lifecycle remain governed by the existing resolved bindings.

The transaction tests cover preserved and replaced values, byte and record
lists, named successors and standalone creates, multiple nested fields, changed
identity/payload, an extra table field, unread malformed input, malformed output
and an incorrect fixed-field width. The maximum byte-list case uses 4096 encoded
value bytes inside a 4116-byte Cell, beyond the old 512-byte buffer. Its input
and successor both execute at O0–O3. An explicit source predicate compares output
capacity with CKB's actual occupied capacity; the underfunded case rejects with
error 5. Codec checks alone do not assert financial ownership or capacity policy.

Both witness-only and Cell-only programs must still reject under
`ExecutableSurfacePolicy::DenyFailClosed` with `nested-values-artifact-proof`.
The current tests prove experimental execution, not independent machine
admission, complete public-interface closure or generated-builder parity.

### Independent witness decoder instruction checks

The standalone checker derives each root nested witness parameter's codec from
typed semantics, including record field widths. A source-local decoder marker
identifies its machine range. The checker verifies every decoder instruction,
canonical size/count/offset comparison, record field boundary, payload bound,
loop increment, success edge and fatal error-16 target. It rejects extra or
missing parameter decoders and external branches into their interiors. Branch
relaxation uses the existing logical-address normalization; block digests and
ELF control flow are still validated independently first.

An independent straight-line interpreter now follows the actual entry prologue,
parameter spills and decoder reloads. Incoming register words and caller-stack
words retain distinct identities through owned frame slots; pointer arithmetic,
large frame constants and stack addresses are interpreted rather than inferred
from variable IDs. Each decoder must receive its declared parameter's pointer
and length. The word expansion rules are shared with the existing fixed-result
ABI checker. Only a fully checked decoder can be summarized between parameter
reloads, preserving its immutable inputs and memory while invalidating scratch
register identities. Branches/calls cannot bypass this prefix or enter reloads.

`nested_witness_decoder_rejects_rehashed_instruction_and_marker_changes`
mutates every non-jump instruction for both byte-list and maximum fixed-field
record decoders at O0–O3, recomputing ELF, block and sidecar identities before
checking. Condition inversions, constants, byte loads and arithmetic changes
reject with V2420. Removing the decoder marker rejects too. A separate test
covers zero budgets and multiple parameters.

`nested_witness_inputs_bind_register_stack_and_mixed_width_arguments` covers
register arguments, a pointer/length pair straddling the register/stack boundary,
caller-stack arguments, and mixed Hash/u128/fixed-byte widths at O0–O3. Rebound
adjacent-slot reloads and swapped same-type parameter markers reject with V2420.
Redirecting the actual call past both decoders, with the declared CFG updated,
rejects at the preceding general ABI boundary with V2407.

This binds the decoder to incoming ABI words and proves mandatory validation
along the checked entry prefix. Later application uses still need to retain
the validated slice, and Cell loaders need independent complete-table proofs.
Those use provenance and Cell machine obligations, plus independent
source/interface binding, remain required. The production blocker stays active.
Reproduce with `cargo test --locked -p cellscript --test artifact_checker nested_witness_ -- --test-threads=1`.

### Independent single-entry nested witness transport

For a single entry containing nested witness arguments, the
checker now follows the complete `_cellscript_entry` wrapper. It reuses the
existing fixed-codec checker for actual GroupInput witness loading, the input
group presence probe before GroupOutput fallback, the 4096-byte load bound, and
exact three-field WitnessArgs validation and `input_type` extraction. The old
fixed-codec API and its admitted external handle profiles are unchanged.

The dynamic extension independently derives ordered argument representations
from typed parameters. It verifies CSARG framing, each length-prefixed segment's
remaining-byte guard, mixed fixed/scalar arguments, runtime-bound null slots,
and exact end-of-payload consumption. Registers and outgoing stack slots must
feed the exact action call; its owned frame, return path and terminal error 25
are checked too. No alternative call site may enter that action. This extends
the previous incoming-ABI proof through the actual witness transport for this
profile; it does not authenticate application data merely because it decodes.

`nested_witness_transport_rejects_rehashed_envelope_and_argument_mutations`
changes every wrapper conditional and byte load, plus nonconstant argument
register origins, at O0–O3 after rebinding all artifact/sidecar hashes.
`nested_witness_transport_rejects_wrong_field_source_and_framing` executes raw
payloads, wrong Lock/output_type placement, a wrong witness index, malformed
table/segment lengths, magic and trailing bytes in CKB-VM; each rejects at the
entry boundary with code 25. Fixed single-entry codec regressions also pass.

Mixed `lock_args`/witness Lock entries additionally bind their Script buffer to
the actual `LOAD_SCRIPT` syscall. The 1024-byte Script buffer, returned size,
args start/length and independent args cursor occupy checked frame locations
outside the 4096-byte witness buffer. Each fixed Script argument has a remaining
bytes guard and advances only the Script cursor; both transports must consume
exactly their own declared bytes before the call. Large stack-address helpers
may choose different temporary registers only if they preserve the Script total
length, both live cursors and previously decoded ABI arguments.

`nested_mixed_script_args_transport_binds_disjoint_buffers_and_live_cursors`
rejects rebound byte loads, guards, buffer/cursor constants and outgoing-stack
origins at O0–O3. It also consistently substitutes a complete address-helper
scratch register with the live Script-length register; this must reject even
though the helper's internal address arithmetic is still coherent.
`nested_mixed_script_args_transport_preserves_both_cursors_and_stack_arguments`
executes a real Lock group with a nested pointer/length pair crossing the
register/stack boundary, followed by Hash/u16/i32 Script arguments and signed
witness arguments. It verifies full-transaction/group replay agreement, negative
i32 ordering, changed Script argument values, truncation, trailing bytes,
oversized Scripts and attempted relocation of Script args into the witness.

### Independent policy-selected nested transport

The ordinary policy machine checker first binds the canonical outer witness,
Script identity, selected tag and record pointer/length to the ordered action
adapters. The nested transport checker then verifies each nested adapter's
entire private-copy loop, canonical positional payload decoder, bounds, ABI
register/stack arguments, exact action target and fatal/completion paths. It
rejects external jumps into the adapter interior and additional callers of the
action. This reuses the single-entry positional proof and the existing policy
selection proof; it introduces no second policy envelope or selector.

`nested_policy_transport_binds_each_selected_record_and_outgoing_argument`
checks two variants with different nested bounds, multiple nested parameters,
fixed scalars, a Hash and outgoing stack words at O0–O3. Rehashed mutations
change decoder branches, byte offsets, ABI register origins and outgoing stack
sources. `nested_policy_dispatch_executes_only_the_selected_bounded_arguments`
executes both variants with the selected Type group at transaction input 1,
compares full-transaction and per-group replay, and rejects switched payloads,
wrong bounds, scalar/Hash substitutions, malformed lengths, trailing/truncated
bytes, unknown tags, wrong Script identity and wrong WitnessArgs placement.
The input Locks are the test harness's explicit always-success fixture; this
is parameter-transport evidence, not application authorization acceptance.

### Independent typed nested read instructions

The checker derives length, index, record-field and pointer-move sites from the
optimized typed operations. It verifies the complete instructions between each
read's start/completion, the owning typed block, incoming flow, source/result
scalar slots, scalar index origin, little-endian reads, empty/last-element
branches and fatal error 21 on an out-of-range index. Record offsets come from
ordered typed field widths; wide values remain pointers. Entry-received root
lengths are bound to the independently checked ABI spills. Derived lengths must
use consistent frame slots outside scalar storage and frame bookkeeping; moves
preserve the same length slot, and independent views cannot share one.

Lowering blocks carry optional `nested_read_labels` because multiple boundaries
can name the same instruction. These are sorted, at most 16 labels per block,
and at most 128 bytes per label. Ordinary artifacts omit the field. A completion
label cannot hide a co-located typed CFG label. Labels only locate instructions;
missing, duplicated or unclaimed sites reject, and their contents are checked
against typed operations and decoded machine code. The existing lowering record
hash binds these optional annotations; no new public nested interface admission
is granted by their presence.

`nested_read_operations_reject_rebound_ranges_fields_and_aliases` mutates every
read-site conditional, byte/scalar load, arithmetic origin and result store,
and removes site annotations, at O0–O3. The large-offset/high-index regression
covers an eight-field 256-byte record prefix and indexes 305419896 and u64::MAX.
`nested_independent_views_cannot_share_a_coherently_rewritten_length_slot`
rewrites both a view's length producer and all consumers to another view's slot;
consistent local instructions still cannot merge independent view storage.
Existing VM cases retain loops, multiple parameters, wide fields, large Cell
buffers, mixed Script.args and policy dispatch.

### Witness lifetime and memory effects

The checker now seeds a separate CFG analysis from the independently checked
witness decoder prefix. It tracks must-initialized nested definitions across
branches and loops, so a rebound edge cannot skip an index/move producer and
enter a later, otherwise valid read. The read summaries above are used only
after their complete instruction contracts pass. Root pointer and size slots,
and derived nested pointer/size slots, stay protected from ordinary byte,
halfword, word and doubleword stores, including FP and computed stack aliases.
Distinct typed locals cannot share a nested pointer slot, even if both machine
accesses and typed source-slot identifiers are coherently rewritten. Parameter
size slots are checked against the scalar/frame boundaries even when unused.

Ordinary instructions propagate known constants and frame addresses; an
unproved write address rejects. Known CKB load syscalls require a bounded size
slot and destinations disjoint from protected storage. Zero-capacity size
queries write only their size slot. Memory knowledge is invalidated over the
actual write interval. The pass uses the already validated memory-free terminal
EXIT sink rather than trusting a helper name or treating arbitrary cross-entry
jumps as harmless.

Calls must enter the real callee prologue. Callees are checked transitively for
private-frame writes and actual SP/FP/return-address restoration; other
register facts are invalidated on return, and outgoing scratch is forgotten.
The fixed-copy helper is admitted through its existing independent instruction
proof, then its actual destination and constant width are checked at each call.
This is not a general exemption for runtime helpers or user functions.
Ordinary fixed-struct result calls reuse the independently checked result ABI:
the callee receives a symbolic output region only through the actual hidden
register or incoming stack argument. Saves, reloads and partial overwrites are
tracked across the CFG. Only the checked exact-width return copy may write that
region. Each caller independently binds the live hidden argument to its declared
private result buffer, rejects overlap with nested pointer/length slots and
invalidates prior memory facts for the written interval. Transitive helpers use
their own buffers. The owned prologue frame and temporary outgoing argument area
remain distinct, including a hidden argument crossing the eight-register limit.
Full Cell-field hash helpers have a separate complete instruction proof. Each
call must supply disjoint private 32-byte result and eight-byte length extents;
neither may overlap nested roots, lengths or live Cell buffers. Rebound helper
words and caller pointers redirected into nested storage are regression cases.
Additional caller-owned write protocols still require explicit verified
effect summaries; the experimental checker rejects them until proved.

The existing exact-byte cache now has a complete independent instruction
contract for its cold and hot variants and scalar wrappers. The checker derives
the four 312-byte entries, 256-byte windows, 24/72-byte header, source tags,
round-robin updates, retained hit pointers, syscall write bounds and fatal exits
from machine words. It does not treat a wrapper's tail jump as an ordinary
returning call or assume an arbitrary cache pointer is safe.

Lifetime analysis proves the counter, last-hit pointer and four valid flags
are initialized before use. Cache regions are disjoint from scalar slots,
saved frame bookkeeping and protected nested storage; ordinary writes and
other checked buffer effects cannot overwrite them. Successful checked reads
preserve the cache invariant while invalidating old scalar facts for its
mutable bytes. Hot-register coherence follows branches and merges and is
invalidated by unrelated register writes. Ordinary callees must prove actual
saved-register restoration. Helpers may borrow the invocation cache through
`s11` only when every caller supplies the proved owned or forwarded cache;
public entries cannot assume the VM supplied one. Header words saving caller
registers are excluded from the cache's mutable write summary.

The selected-view/nested-witness CKB regression covers cold and hot caches,
full hash reads, cached byte/word reads and a helper reading the paired output.
Independent mutations cover complete helper bodies, initialization stores,
caller/borrowed cache pointers and another helper redirected into cache storage.
This closes that combination's previously recorded cache-effect rejection;
the broader #47–#49 admission obligations remain unchanged.

Exact-role selection entries now use the same CFG memory analysis even when
they have no nested values. Checked constructors establish definite retained
tables and pointer facts; checked reads and aliases require those facts on all
incoming paths. Possible table regions stay protected across branch joins.
Handle slots cannot alias other typed locals, and ordinary stores or other
checked memory effects cannot overwrite live tables or handle slots. Callees
retain the existing private-frame and explicit caller-output contracts. This
proves retained-table lifetime within the admitted frame model; it does not
establish complete aggregate/source provenance, field borrows or final builder
binding. Reconstruction over a possibly live table remains fail-closed pending
an explicit reinitialization proof.

Co-located machine labels retain emission order for typed-block ownership,
while serialized nested/selection boundary sets retain canonical lexical
ordering. The nine-argument fixed-result regression covers multi-digit local
IDs at shared boundaries, and a rebound unsorted boundary set still rejects.

The pass is bounded to 64 active callees, 8192 instruction states per entry,
262144 instruction-state visits per entry, 2097152 visits overall and 262144
resident register/memory/definition atoms across active analyses. Unknown or
exhausted proofs reject with V2420; no nested stable-admission blocker is removed.

The lifetime mutation tests rebind ELF and sidecar hashes for ordinary full and
partial stores, FP aliases, helper writes, fixed-copy destinations/helper code,
and a branch that bypasses a definition. The branch case also recomputes CFG
reachability, ensuring rejection comes from initialization proof rather than
stale graph metadata. Existing maximum, loop, wide-field, mixed Script.args and
policy-dispatch VM cases remain required regressions.
Fixed-result regressions cover zero, one, seven, eight and nine ordinary scalar
arguments, direct and transitive helpers at O0–O3, and actual VM acceptance and
business-condition rejection while the original nested views remain live.
Rehashed mutations also reject arbitrary writes through a valid hidden output
pointer and a partial FP-relative overwrite of its saved value before the exact
return copy.

The pass also consumes the independently checked Cell load summaries described
below. Successor comparison proofs, complete
source/interface admission, additional write effects and final transaction
builder parity still prevent closing #48 or removing
`nested-values-artifact-proof`.

### Typed Cell load and complete-table machine checks

Each annotated typed Cell load now binds its declared role's actual source and
ordinal, zero data offset, LOAD_CELL_DATA syscall, capacity, size slot and private
buffer. Capacity is derived from the full declared table, with the existing
512-byte allocation floor. Buffers and size slots must remain outside scalar
locals and the frame trailer, and distinct Cell bindings cannot alias them.
Borrowed temporary aliases retain the original binding's buffer identity.

The dedicated table codec checks reported size against capacity and header,
the exact total and first offset, consecutive field spans and all fixed widths.
Each nested span executes the independently checked canonical nested decoder.
The machine proof covers successful return restoration and the existing fatal
16 (nested encoding/shape) and 2 (table extent) exits. The immediate caller must
reject a preserved nonzero load result through fatal 1 (syscall failure). Input
and read-only roles must then assign the checked buffer address to the declared
local pointer slot. Interior entry into this continuation is also rejected.
Dominance and later buffer/slot lifetime use the separate CFG proof below,
rather than being inferred from the instruction fragment alone.
Unused nested fields and group-relative policy roles require codec coverage too.

Optional bounded `nested_cell_labels` preserve co-located start, validation and
end boundaries. The checker derives the contract from typed bindings and fields,
checks the instructions rather than trusting names, rejects external entry into
the load/decoder interior, and requires disjoint machine ranges before analysis.
Range-limited decoding and indexed incoming edges bound the work by the checked
instruction/edge corpus. Constant programs must initialize their register; they
cannot assume an incoming value is zero. Rehashed mutation tests change load
arguments, table/element conditions and byte loads, and remove codec coverage.
A branch mutation also rebinds graph edges and reachability while entering after
the syscall; it must fail the Cell decoder's own control-flow boundary check.

Nested field projections have their own exact instruction contract. It binds the
source pointer slot, source length, total/header checks, declared field's two
offsets (or whole-table end for the final field), range checks, destination
pointer and destination length. Direct Cell roots must use the length slot of
their checked load. View lengths cannot overwrite scalar slots, their source
length or any loaded Cell buffer/size slot. Borrowed helper field projections
retain the same checked span instructions and bind their incoming buffer through
the call proof described below.

`TypedSemanticField.molecule_field_index` explicitly records declaration order
for nested Cell layouts and participates in their existing layout hash. Every
field requires a unique contiguous index; the extension is forbidden on other
layouts and omitted from their serialization. Legacy offset/name sorting remains
unchanged: multiple dynamic fields cannot safely infer wire order from offsets.
Both complete-table codecs and individual projections use the explicit indices.
Tests cover two same-type fields, fields with different bounds, rehashed missing,
duplicate and swapped indices, byte/slot/arithmetic mutations, failed-load status
checks and pointer initialization.

The CFG memory-effect pass starts Cell-owning entries at their real prologue,
or after their independently checked witness prefix when both sources are
present. A checked load summary requires the expected SP and marks an input or
read-only root initialized only after the complete load, codec and pointer
assignment. Field/index/move summaries propagate definite initialization;
joins intersect initialized definitions. External jumps past the entry
prologue and reads that bypass a source definition reject.

After a load, ordinary stores, syscall writes and checked-copy destinations
cannot overlap its Cell buffer, size slot or initialized root pointer. Cell
buffers are represented as intervals, not per-byte analysis state. Joins retain
the union of possibly loaded buffers for write protection, even when a root is
not definitely initialized on every path. Output loads protect their actual
data buffers but do not falsely initialize an expected-value pointer. Repeated
loads must retain their checked binding and buffer. Nested view-size slots,
including witness-derived views, must be disjoint from all Cell storage.

Tests redirect full and partial ordinary writes into these regions and rebind
branch targets/CFG reachability to bypass a Cell field definition. Mixed
witness/Cell reads, two dynamic fields with different bounds, large buffers and
existing borrowed-helper VM cases retain their positive behavior.

### Borrowed nested Cell arguments

Read-only helper Cell parameters carry separate symbolic pointer and length
identities from their actual incoming ABI registers or caller stack words.
The interpreter follows their spills, reloads and ordinary register moves.
Field projections require both values to retain the declared root identity;
a fabricated pointer or numeric length is not a checked view. A detached whole
`NestedValues` value may accompany a borrowed Cell under the separately checked
whole-value helper contract below. Derived byte/record views have their own
checked element codec on helper entry. Writes through a borrowed pointer remain
forbidden by the private-frame memory-effect contract.

Typed call operations are paired in order with actual calls within each typed
block. The checker derives argument word positions from the callee signature,
resolves immutable reference/move aliases to a declared Cell root, and requires
the actual pointer and length at the call to match that initialized root. It
checks direct and transitive calls, including split register/stack pairs and
fully stacked pairs. A different validated Cell with the same nominal layout
cannot replace the selected root by swapping both ABI words.

The private frame is derived from its decoded allocating prologue, not the
lowering record's aggregate negative-SP-adjustment bound. Checked fixed-result
frame declarations must agree with that prologue. Outgoing borrowed-call stack
space is derived from the complete expanded parameter ABI and optional hidden
result pointer. Tests cover zero, one, six, seven, eight and nine preceding
scalar arguments, optimized register forwarding, callee spills/stack reloads,
and coherent substitution of a second input Cell at O0–O3.

### Whole nested-value helper arguments

observation-only functions accept immutable ordinary parameters of a complete
`NestedValues<T, N, P, B>` type, including an Edition 2027 `data_shape` name.
The caller passes the existing encoded pointer/length pair; the callee validates
the complete codec before reading it. Every call repeats this validation, so
this is not a once-only decoding or frozen resource-cost claim. No pointer is
serialized, ownership transferred or new wire format introduced.

The checker follows immutable aliases to their declared roots and requires both
actual outgoing ABI words to retain that root. Incoming register/stack words,
spills and validation are checked independently. Substituting another valid
value with identical type and bounds, including changing pointer and length
together and rebinding hashes, rejects. A whole nested Cell field can also be
passed alongside its borrowed Cell; their distinct source contracts remain.

The named byte/record helper fixture iterates payloads and forwards values through
helpers. O0–O3 transaction tests cover empty and mixed inputs, wrong expected
results, malformed encodings, and zero/six/seven/eight/nine scalar arguments
before the nested pair. Checker tests cover rebound typed sources, incoming
spills and paired argument substitution. Source diagnostics reject mutable or
reference parameters, non-read-only callees, mismatched bounds, nested returns
and storage. Derived views use the additional contract below; these call proofs
alone do not establish complete source equivalence or stable admission.

Reproduce with `cargo test --locked -p cellscript --test bounded_nested nested_value_helpers`
and `cargo test --locked -p cellscript --test artifact_checker nested_value_helpers`.
`nested-values-artifact-proof` remains mandatory.

### Derived item and payload helpers

`Shape::Item` names the view returned by indexing/iterating a declared shape:
bytes for a byte-list shape, or the declared record for a record-list shape.
`Shape::Payload` names a byte view with that shape's per-payload bound. These
names carry no new count or wire limit. For example:

```cellscript
fn payload_total(payload: Entries::Payload) -> u64 {
    let mut total = 0
    for byte in payload { total = total + (byte as u64) }
    return total
}

fn record_total(entry: Entries::Item) -> u64 {
    return (entry.id as u64) + payload_total(entry.payload)
}
```

The [complete fixture](../tests/syntax_combo/seeds/named-derived-helpers.cell)
declares both shapes and calls these predicates from ordinary iteration.
Associated types resolve in the shape declaration's scope. Recursive shape
elements, including references through `Shape::Item` or `Shape::Payload`, reject
before elaboration. Named shapes remain module-local.

Derived helper parameters are ordinary immutable values with two internal ABI
words. The helper validates the exact byte or record encoding, the payload
bound and pointer arithmetic before application reads, including unused
parameters. This revalidates one element, not its enclosing list. The independent
checker recognizes the validation instructions and requires the caller's
pointer/length pair to match its initialized derived source. Local aliases and
transitive calls retain that identity; a valid same-type element from another
source cannot replace it. Whole-list codecs continue to validate every element.

Derived parameters require observation-only helpers. They cannot enter directly
through action/lock witnesses, become Cell fields, be wrapped in references or
owned containers, mutate their backing bytes or escape through returns. They
confer no Cell lifecycle authority. Source, editor, browser and machine proof
coverage is experimental; full source/interface and transaction-builder
admission remain open.

### Nested successor field comparisons

Nested create fields use an explicit bounded byte loop. The independent checker
binds each loop to its typed create block, selected output Cell, declared
Molecule field index and exact expected source local. It checks table spans,
length equality, both byte loads, mismatch failure, pointer advances and loop
termination. Expected lengths come from the already checked witness/Cell view
definitions; compatible types cannot substitute different source locals.

The CFG lifetime pass requires the output load and expected value definition
before comparison, intersects completed comparisons at joins, and rejects a
successful return or potentially successful exit that bypasses a required
comparison. Reloading an output invalidates its completed comparisons. Checked
output storage and expected pointer/length slots remain protected. Obligations
retain the typed create block and expected local: checking one expected value
cannot satisfy another constraint on the same field. This initial proof
conservatively requires all declared nested create constraints on every success
path; mutually exclusive branch-dependent creates remain outside this proof.
Rehashed instruction mutations and a rebound branch/CFG bypass exercise this boundary
at O0–O3.

This closes the fixed-role load/table, field-projection, direct-root lifetime,
borrowed-helper and nested successor comparison slices. Complete
source/interface and signed-builder admission remain required before
#48 can close. `nested-values-artifact-proof` remains.

### Fixed siblings in nested successor tables

The output instruction contract also covers every fixed sibling field. It binds
the span, exact width, expected local slot or typed literal bytes, comparison
and failure edges. Unsigned scalars use full register equality after a complete
little-endian field load. u128, Hash, Address and byte arrays compare all bytes.
Short byte arrays received as parameters use inline scalar ABI storage, including
move aliases; Cell field views use pointers. The checker derives that distinction
from typed parameters and definitions. Wide offsets use the checked stack helpers.
All fixed comparison obligations participate in the success-path intersection
and output reload invalidation.
Existing mismatch exits are retained: scalar mismatch is error 3 and fixed-byte
mismatch is error 32. The complete Cell codec rejects malformed fields first.

VM coverage at O0–O3 preserves and replaces u8/u16/u32/u64/u128, Hash, Address,
and 1/8/9/256-byte arrays from Cell fields or witness values, and rejects a change
to each field's last byte. Literal coverage includes u64::MAX, u128 zero, zero
Hash/Address and byte-string literals. Rehashed comparison mutations bind width,
expected storage, scalar high bits, literal bytes and loop steps. Short-array ABI
mutations reject interpreting inline bytes as a pointer.
Calculated-value VM cases cover a scalar helper result and a u128 addition
carrying into the high 64 bits; altered scalar/high-limb outputs reject.

This comparison proof does not yet establish arbitrary fixed-value producers or
their storage lifetime. Those scalar/fixed dataflow obligations remain required;
the existing nested-view initialization proof is not implicitly extended to
fixed locals. Constructed fixed-array expressions still encounter the existing
typed-IR placeholder rejection; this slice supports byte-string literals and
existing Cell/witness values. Stable nested artifact admission remains closed.

### Experimental native and generated value builders

`nested_builder::encode_nested_values` and generated TypeScript
`encodeNestedValues` encode one source-declared nested value. The generated
`nestedValueContracts` catalog derives count/payload/total limits and record
widths from the same artifact metadata; undeclared type keys reject. Inputs
contain exact fixed-field byte slices in declaration order and a raw payload.
Unsigned fixed fields use little-endian encoding. This is a byte-oriented
value encoder; nominal property-based record construction remains pending.

Both implementations validate counts, field widths and the complete encoded
size before allocating output. Tests compare independently written small
golden vectors, empty values, the 4096-byte maximum and over-budget/wrong-shape
inputs. TypeScript also rejects sparse arrays and non-byte payloads. The native
encoder's output is independently checked and is exercised inside actual Cell
input/successor transactions. Reproduce the host/generated SDK checks with
`cargo test --locked -p cellscript --test nested_builder`.

The fixture reader in `tests/support/nested_codec_oracle.rs` independently
reconstructs each fixed field and payload using literal schema widths and
numeric limits, without compiler metadata or codec helpers. Its wire rules
follow the [Molecule encoding specification](https://github.com/nervosnetwork/molecule/blob/master/docs/encoding_spec.md).
Every accepted native/generated vector round-trips its complete value, including
empty and mixed-size lists, 256-byte payloads, both byte/record lists at exactly
4096 bytes, and wide fixed fields. Both builders reject one-over record count,
payload and total limits. The reader and independent contract validator reject
truncation, trailing bytes even with a rebound outer size, and invalid first
offsets. These are bounded codec/reference checks; final source/machine
admission and gates remain separate.

Molecule schema generation now describes the actual vector of Bytes or record
tables instead of falling back to an opaque Bytes field. Count, payload and
total bounds enter the generated alias and schema identity. Tests confirm
changing any bound changes the schema and manifest hashes. This producer-side
binding still needs independent source/interface/machine evidence.

`encode_nested_cell_data` and generated `encodeNestedCellData` now construct
complete Cell data for the admitted table shape. The source-derived
`nestedCellContracts` catalog carries the schema hash, complete maximum encoded
size and ordered named fields. Callers provide fixed byte fields and nested
element inputs by name; missing, extra, wrong-width and wrong-representation
fields reject. Raw bytes cannot bypass nested value validation. Multiple nested
fields retain separate limits, so the enclosing Cell may exceed 4096 bytes.

Native/TypeScript tests compare a fixed record Cell golden vector and a
4124-byte Cell containing a maximum-sized value plus a second empty field.
The existing CKB adapter materializes these exact output bytes, computes
occupied capacity using actual Lock/Type scripts, accepts its exact minimum and
rejects one shannon less. This is output materialization evidence, not a funded
or signed transaction. Separate O0–O3 VM tests execute native-built byte-list
and record-list Cells through input/successor checks, including standalone
creation and malformed successor rejection.

Existing entry framing and transaction adapters still own witness placement,
combined witness limits, occupied capacity and signing. Typed record inputs and
final admitted/signed transaction builder parity remain required for #48.
`nested-values-artifact-proof` remains in force.

### Nested-table selection execution prerequisite

The selector's internal emitter now accepts a bounded complete Cell table with
fixed fields and the admitted nested list codecs. It reuses full table/element
validation before inspecting the identity field, including validation of unused
payloads. A fixed identity field of 1–32 bytes may follow a dynamic field: its
validated Molecule offset determines the key, rather than a fixed byte offset.
The key is copied into existing private scratch for uniqueness and correspondence
checks. Output payloads may change; application rules must separately constrain
the permitted change.

One reusable payload buffer follows the unchanged retained input/output tables.
Its capacity is the largest declared role bound, capped at 32768 bytes; the
existing fixed-layout frame remains 4992 bytes. The internal derived contract
checks the complete header/field bound and key layout before emission. The VM
compares returned Cell length to this capacity before reading and charges the
actual encoded length to the aggregate budget. It does not charge capacity as
if all Cells had maximum length. Source/group coordinates, custody checks,
cardinality, duplicate detection and correspondence still use the common selector.

`nested_role_machine` tests compare retained coordinates and identities to the
independent fixed-role oracle using the same logical keys. The codec vectors
are separately encoded tables; this is not a new general nested host oracle.
Tests include mixed layouts, changed payload lengths, a key after a variable
field, exact/one-under aggregate budgets, malformed unused payloads, overlong
loads, duplicates and substitutions. Real `ckb-testtool` transactions exercise
two/eight-role catalogs, unrelated fee Cells, output permutation and a rejecting
peer group. The maximum primitive fixture uses eight nested roles, 32 selected
Cells per side, 32320 actual data bytes and a 13232-byte private frame; its
late-invalid case corrupts the final selected Cell's nested payload length.
The primitive fixture observed 393659 valid cycles, 340431 late-invalid cycles
and 107656 bytes for the valid test ELF (including result-table assertions).
Reproduce with `cargo test --locked -p cellscript --lib nested_role_machine -- --nocapture`.
These are initial primitive observations, not frozen source-program budgets or
whole-transaction cost equivalence.

Named and positional source catalogs now select
`cellscript-exact-role-cell-selection-bounded-v2` automatically when a resolved
role contains a direct nested value field. Fixed-only catalogs retain
`cellscript-exact-role-cell-selection-fixed-v1`. The independent checker derives
the ordered table layout, complete capacity, identity field and private frame
extent from typed schemas. It checks the complete decoder, key normalization,
actual-byte accounting and terminal exits. Each retained selection owns its
whole frame; ephemeral verification reserves the largest required frame.
Nested failures use selection-local fatal sites, so different private SP values
never join an entry's shared error handler.

The `named-nested-selection.cell` syntax fixture combines named shapes, mixed
roles, differently sized retained catalogs and repeated verification. Source
transaction tests combine changed payloads, role-local/transaction coordinates,
live nested witnesses and all required peer groups. Selected coordinates also
expose bounded nested `.data` fields with independently checked, retained
per-result buffers. Complete source/interface proof, final builder admission,
frozen resource evidence and stable business builder admission remain
required. Experimental complete-program builder evidence is described below.
Selection and nested artifact-proof blockers remain mandatory, including a selection-only
entry with no nested witness or directly bound Cell local.

Focused source/checker reproduction:

```bash
cargo test --locked -p cellscript --lib nested_role_source_real_ckb -- --nocapture
cargo test --locked -p cellscript --test artifact_checker nested_role_selection_checks
cargo test --locked -p cellscript --lib named_nested_selection
cargo test --locked -p cellscript-wasm --features wasm --lib wasm_named_nested_selection
```

The source transaction fixture observed 99297 cycles for two roles and 1218359
for eight roles with 32 selected Cells per side, including a live nested witness,
repeated selections and selected nested `.data` reads. These are initial test observations,
not a frozen cost comparison with the primitive fixture or full business rules.

### Executable fixed mixed-role settlement

The experimental Edition 2027
[`mixed_settlement.cell`](../examples/language/batches/mixed_settlement.cell)
checks one through six full-fill orders against two exact asset Types. The
`orders` role has local Type authority; `base` and `quote` are observations of
foreign asset groups. Both asset roles reuse the fixed `Balance` schema while
retaining different complete Script identities and catalog ordinals. Order
terms, owners, quantities and settled-state changes are explicit DSL predicates,
not business rules inferred by the compiler from names.

Each order has a unique u64 identity, bounded so `id * 2 + 1` cannot overflow.
Each asset contributes exactly two balances per order: debit identity `id * 2`
and credit identity `id * 2 + 1`. Base goes from seller to buyer; quote goes
from buyer to seller. The complete role counts and distinct identities prevent
one balance or output from satisfying two orders. `full_fill` checks unchanged
terms, distinct parties, positive quantities, and the settled tag's `0 -> 1`
transition. `transfer_leg` checks owner, capacity, debit sufficiency and credit
overflow. Both are checked non-escaping read-only helpers over typed input/output
views; Hash owner arguments remain copied values.

The retained [positional comparison](../tests/selection_bundle/fixtures/mixed_settlement_positional.cell)
has the same complete business predicates and helpers. Only its policy/use-site
spelling differs. The named form declares schema, identity, authority and bounds
once, then uses `selected.base.input(j)` and `.successor(j)`; the positional form
repeats `Balance` and catalog ordinal `1` on each read. The complete bundle test
now runs both forms at O0/O3 through generated SDK bindings, native artifact and
ProtocolBundle checks, the packed adapter and all four Script groups. One, three
and six orders in both output orders produce identical transaction bytes between
forms. Both reject hostile peers, late amount changes, rebound catalog/output
substitutions and stale live-Cell/dependency observations. SDK plans retain the
same admission blockers.

The O3 named form uses 117,533 / 356,202 / 843,183 whole-transaction cycles for
one / three / six orders; the positional form uses 117,447 / 355,924 / 842,377.
The settlement ELF is 26,752 versus 26,648 bytes, and observed stack is 8,800
versus 8,704 bytes. Both touch 45,056 guest-page bytes for that Script; separate
heap accounting remains unavailable. These differences are measured compiler
costs, not intrinsic costs of named syntax. The named baseline still passes its
unchanged frozen observations and ceilings; positional observations do not
replace or relax them. This is an authoring comparison for the chosen direction;
final API freeze and review remain dependent on the complete #47–#49 acceptance
program.

The all-group `ckb-testtool` fixture executes the settlement Type, two separate
asset Type groups and the spending Lock group. Each asset Type independently
sums its own input/output amounts as u128 and rejects imbalance; a test replaces
the settlement code with a no-op and confirms that the asset peer still rejects
inflation. Explicit rejecting peers and a rejecting Lock also fail the full
transaction, consistent with [CKB Script execution](https://docs.nervos.org/docs/script/intro-to-script).
Successful fixtures use a permissive test Lock; this evidence does not establish
an application signature scheme or deployment authentication.

VM cases cover 1/3/6 orders, equivalent reversed outputs, seven-order rejection,
late debit/credit/owner/term substitution, reused/zero/overflowing order state,
insufficient balances, overflowing credit, duplicate/missing/unmatched balances,
cross-asset output swaps, custody changes, insufficient capacity and wrong fixed
layouts. Untyped fee/change Cells deliberately carry balance-like data and remain
outside protected role membership. The source limits are 32 scanned Cells per
side, 30 selected per side and 8192 protected data bytes. The six-order case uses
30 protected Cells per side (plus an unrelated fee/change Cell), totaling 3372
protected data bytes across both sides.

The example exposed a checker attribution bug: a shared helper return tail
inherited the last typed block's identity and looked like a branch bypassing
that block's field checks. The checker now recognizes the entire RA/FP/SP restore
and RET before exempting that exact tail entry from the prefix-entry rule. The
originating typed block still must execute its required checks. Mutations alter
restore instructions, jump into a tail's middle or replace a field check with an
early return; they reject after outer identities are rebound. No arbitrary
label, epilogue-like name or unverified instruction sequence gains an exemption.

At O3, the named source uses 26,704 ELF bytes; the positional source uses
26,600. Both use the same 4,024-byte asset peer. Exact fixture observations:

| Orders | Named cycles | Positional cycles | Transaction bytes | Occupied output capacity (shannons) |
|---:|---:|---:|---:|---:|
| 1 | 117,560 | 117,474 | 1,655 | 75,100,000,000 |
| 3 | 356,301 | 356,023 | 4,097 | 207,300,000,000 |
| 6 | 843,390 | 842,584 | 7,760 | 405,600,000,000 |

The entry-witness payload occupies 20 bytes in each transaction. Named source
frames are 7,712 bytes for `settle`, 1,808 for `full_fill` and 1,584 for
`transfer_leg`; positional `settle` uses 7,520. These are declared individual
frames, not observed peak call-stack use. The named form currently costs 104 ELF
bytes and 192 caller-frame bytes more, with 806 additional cycles at six orders;
that overhead is reported rather than hidden by the readability comparison.

Runtime observations are reproducible with the command below. They are not yet
frozen per-case ceilings or a matched Rust/array baseline. `ckb-testtool` returns
aggregate cycles; observed stack/heap fields remain explicitly unavailable.
Compiler-declared frame sizes, ELF lengths, full transaction/witness byte lengths
and output occupied capacity are recorded separately, not substituted for observed
peak memory. Complete source equivalence, stable builder admission, resource
freeze and final gates/review remain pending. Both
`exact-role-selection-artifact-proof` and `cell-data-view-artifact-proof` remain.

The [settlement bundle integration](../tests/selection_bundle/settlement.rs)
now runs this complete named program through generated TypeScript bindings,
native ProtocolBundle checking and the existing transaction adapter. It uses
the `ckb-type-hash` profile with distinct built-in TYPE_ID code/admission Cell
identities for all four participants. Exact handles and initial deployment-line
receipts bind each checked ELF and interface; admission Cells commit the full
line handle. Missing admission evidence rejects. These are deterministic local
fixtures, not published deployments or proof of chain liveness.

For 1/3/6 orders and forward/reversed outputs, adapter bytes equal an independent
construction using CKB's canonical Molecule builders. All four Script groups
execute those exact bytes. Generated SDKs bind each participant and the three-role
catalog, reject reordered role contracts and retain both admission blockers.
Resolved input checks reject omissions, swapped Cells and changed data; final
transaction binding rejects substituted witness bytes. Dependency checks use
actual local fixture Cells and reject missing or substituted code/admission
Cells at all eight dependency positions. Rebound reports reject changed protected
identities, custody, pairing evidence and committed witness bytes.

The role checker intentionally does not prove the package's transfer formula:
a late amount change can pass membership/correspondence reconstruction but fails
the all-group VM execution. Hostile asset or Lock code also fails execution. The
fixture uses the shared [asset conservation program](../tests/fixtures/mixed_settlement_asset.cell)
and a permissive test Lock, so it does not establish signature authorization.

This bundle fixture includes four code deps, four admission deps and a complete
WitnessArgs slot list. Its O3 observations are 117,533 / 356,202 / 843,183 cycles
and 1,923 / 4,605 / 8,628 transaction bytes for 1/3/6 orders, equal for reversed
outputs. They are a separate fixture from the authoring comparison above, not
an optimization comparison. The resource replay below freezes this fixture's
observations and ceilings separately. Resolver API results
from local Cells are not published as node or consensus evidence. The nested
migration has its own complete-program builder fixture below.

```bash
cargo test --locked -p cellscript --lib mixed_settlement -- --nocapture
cargo test --locked -p cellscript --test artifact_checker mixed_settlement
cargo test --locked -p cellscript-artifact-checker --lib shared_return_tail
cargo test --locked -p cellscript-wasm --features wasm --lib wasm_mixed_settlement
cargo test --locked -p cellscript --test selection_bundle mixed_settlement -- --nocapture
```

### Executable nested batch migration

The experimental Edition 2027
[`nested_migration.cell`](../examples/language/batches/nested_migration.cell)
now supplies the variable-payload business program. Its named catalog selects
one through eight local documents and exactly one observed Registry, with
one-to-one identity correspondence. It preserves every document's history,
appends the single witness release note and advances each revision to the
Registry's next revision. `same_message` and `migrate_document` are read-only
helpers; transaction coordinates and a derived nested message share the latter's
parameter list without erasing their distinct provenance.

This example permits neither creation, burn nor merge. Protected documents
cannot disappear or change custody, unrelated fee/change Cells are excluded,
and reversed output order retains identity pairing. The current Type checks
its own transition; the Registry Type independently checks its revision change,
and every spending Lock executes. The test Locks are permissive fixtures, not
an application signature scheme. The complete test transaction is run by
`ckb-testtool`; it is all-group CKB-VM evidence, not a stateful-node deployment.

The source declares eight messages per document, 256 bytes per message and
2120 encoded bytes per message list. The maximum successful migration has seven
full messages before and eight after, in each of eight documents. Both sides
plus the Registry contain 32288 protected data bytes under the declared 32768
aggregate limit. The illustrative positive test Cells carry sufficient occupied
capacity; a deliberately insufficient successor rejects.

`nested_migration_business_transaction_preserves_history_and_checks_every_group`
executes O0–O3 and uses the existing source-bound nested Cell/value encoders and
entry-witness encoder. It checks minimum, representative and maximum batches;
mutations cover history, appended note, revision, capacity, missing/duplicate
successors, custody, malformed tables and a rejecting Registry peer. Separate
checker mutations replace either transaction-coordinate parameter spill while
retaining the valid nested decoder and rebinding outer identities.

Initial O3 observations are 41910 cycles for one document, 79778 for three and
1726794 for eight with maximum payloads. These are fixture observations, not
frozen resource ceilings or a cost-equivalent Rust comparison. The fixed
mixed-role program is covered separately above. All three artifact-proof
blockers remain.

The [migration bundle fixture](../tests/selection_bundle/migration.rs) now binds
the complete named program, independently executing Registry and test Lock to
generated SDK manifests and exact Type-hash deployment-line receipts. It covers
one document with an empty history, three documents with mixed message lengths,
and eight documents with maximum histories/payloads. Each case runs with forward
and reversed outputs. Both programs reuse the same deployment and canonical
transaction fixtures; the Registry program is shared with the original VM suite.

Native and generated TypeScript encoders agree on every input/successor Document
and release-note value. An independent Molecule table/DynVec constructor checks
the native vectors, including empty histories. Generated bytes are put back into
the actual transaction skeleton; the existing native entry-witness encoder wraps
the generated note value, and all skeleton fields remain byte-identical. This
does not claim a new TypeScript entry-envelope encoder. The existing adapter's
packed transaction then equals an independent use of CKB's Molecule builders,
and all three Script groups execute those exact transaction bytes.

SDK bindings retain the source-derived roles and nested schema identities and
reject a changed payload bound. SDK encoders reject excessive payloads/counts;
plans retain every admission blocker. Native checking and rebound adapter reports
reject malformed output tables, and materialization rejects insufficient output
capacity. A maximum-batch late note-byte change retains canonical encoding and
membership but fails the package predicate in CKB-VM. Rejecting Registry/Lock
peers fail the full transaction. The local resolver also rejects substitution of
the last input history. Fixture resolver results are not live-node evidence.

O3 observations for this separate bundle fixture are:

| Documents / history | Cycles | Transaction bytes | Raw WitnessArgs bytes | Occupied output capacity (shannons) |
|---|---:|---:|---:|---:|
| 1 / empty | 40,803 | 968 | 82 | 25,600,000,000 |
| 3 / mixed | 79,790 | 1,568 | 114 | 58,400,000,000 |
| 8 / maximum | 1,726,786 | 19,844 | 448 | 1,792,600,000,000 |

Forward and reversed outputs have equal observations. Complete ELF lengths are
20,560 bytes for migration, 2,240 for Registry and 656 for the test Lock. Declared
individual frames are 8,816 bytes for `migrate`, 14,576 for `migrate_document`
and 1,312 for `same_message`. Individual frames do not measure peak call-stack
use; the separate resource replay below now observes the combined stack and
late-invalid group cycles. Heap-only accounting remains unavailable.
The maximum selected data total is 32,288 bytes. Complete worst-case coverage,
source/interface admission and final gates/review remain open; this evidence
does not close #48 or #49.

The [explicit-bound byte-list counterpart](../tests/selection_bundle/fixtures/nested_migration_explicit.cell)
retains the entire program, replacing `Messages` with
`NestedValues<Vec<u8>, 8, 256, 2120>` and `Messages::Item` with
`NestedBytesView<256>`. The bundle test runs both forms at O0/O3 with one, three
and eight documents in both output orders. All 24 positive executions agree on
canonical transaction bytes between forms, with matching cycle and ELF-size
observations at each optimization level. Each form also rejects the same
late-note, malformed-data, rebound-adapter and hostile-peer cases. The existing
named O3 resource observations and ceilings remain unchanged. The record-list
authoring comparison follows below.

Reproduce with:

```bash
cargo test --locked -p cellscript --lib nested_migration_business -- --nocapture
cargo test --locked -p cellscript --test artifact_checker nested_migration_mixed
cargo test --locked -p cellscript --test selection_bundle nested_migration -- --nocapture
```

### Record-list migration and authoring comparison

The [record migration program](../examples/language/batches/record_migration.cell)
stores a revision identity and bounded byte payload in each message. It retains
the byte-list program's document/Registry roles and preservation rules, and
requires positive, strictly increasing historical revisions no greater than the
current Registry revision. The appended record must carry the next revision.
`same_message` compares identities and delegates payload comparison to
`same_payload`; `migrate_document` checks every historical record and successor.

The [explicit-bound counterpart](../tests/selection_bundle/fixtures/record_migration_explicit.cell)
contains the same complete predicates using `NestedValues<Message, 8, 256, 2280>`,
`NestedRecordView<Message, 256>` and `NestedBytesView<256>`. The named form uses
`Messages`, `Messages::Item` and `Messages::Payload`. The bundle fixture compiles
both at O0 and O3, generates SDKs, compares native/TypeScript encodings against
independent Molecule tables, materializes canonical transactions and executes all
three Script groups. Named and explicit forms produce identical transaction
bytes and matching cycle observations for each case and output order.

The maximum histories contain seven records before migration and eight after it.
Seven documents fit with 256-byte payloads. Eight use 240-byte payloads and
exactly exhaust the unchanged 32,768-byte aggregate selection budget. Eight
documents with 256-byte payloads require 34,688 protected bytes: individual values
are valid, but both native bundle validation and VM execution reject the total.
The fixture also rejects preserved duplicate historical revisions, a wrong new
revision, a late payload-byte change, malformed tables and hostile peers.

O3 observations for both authoring forms, with either output order, are:

| Documents / history / payload | Cycles | Transaction bytes | Raw WitnessArgs bytes | Occupied output capacity (shannons) |
|---|---:|---:|---:|---:|
| 1 / empty | 43,172 | 1,008 | 102 | 27,600,000,000 |
| 3 / mixed | 92,221 | 1,768 | 134 | 76,400,000,000 |
| 7 / maximum / 256 bytes | 1,596,249 | 18,624 | 452 | 1,682,200,000,000 |
| 8 / maximum / 240 bytes | 1,724,915 | 20,104 | 452 | 1,818,200,000,000 |

Complete O3 ELF lengths are 24,824 bytes for migration, 2,240 for Registry and
656 for the test Lock. Declared individual frames are 9,040 bytes for `migrate`,
15,680 for `migrate_document`, 1,280 for `same_message` and 1,312 for
`same_payload`. The resource replay below measures combined stack use and
late-invalid group cycles and freezes these particular fixture ceilings.
Heap-only accounting and a complete worst-case corpus remain open.
Different payload sizes also prevent treating this as a matched byte-list
performance comparison.

Editor coverage checks record-field completion, shape navigation and rename
across derived helper types. WASM metadata retains exact bounds and proof
blockers and rejects oversized payload bounds or mutating payload helpers.
Independent checker mutations replace paired payload arguments in typed evidence
or the machine pointer/length loads after rebinding hashes; both reject at O0/O3.
All existing proof-admission blockers remain required.

Reproduce with:

```bash
cargo test --locked -p cellscript --test selection_bundle record_migration -- --nocapture
cargo test --locked -p cellscript --test artifact_checker record_migration
cargo test --locked -p cellscript --lib record_migration
cargo test --locked -p cellscript-wasm --features wasm --lib wasm_record_migration
```

### Cross-package readonly business composition

The `cross_package` cases in [the complete-program suite](../tests/selection_bundle.rs)
extract public schemas and readonly helpers into a real local dependency package.
They create an explicit `Cell.lock`, compile through the normal package loader,
require dependency source-unit evidence and check that compilation leaves the
lock bytes unchanged. They do not concatenate the library into one source file.

The mixed settlement case moves `Order`, `Balance`, `full_fill` and `transfer_leg`
into the dependency. At O0/O3, 1/3/6 orders and normal/reversed outputs preserve
the original canonical transaction bytes and execute all four required groups.
Either asset Type or the spending Lock can still reject the transaction. An
unsettled order and a wrong transfer quantity reject through the imported
helpers; the latter keeps the asset total conserved, so foreign conservation
alone cannot satisfy the order's business rule.

Both byte-list and record-list migration extract their schemas and all readonly
comparison helpers. Local named shapes elaborate independently in each package;
the entry witness's named bounds and the imported helper's canonical parameter
bounds must agree. The complete existing pipeline checks actual generated SDK
bytes, native codecs, packed adapter transactions, standalone artifact evidence,
all required VM groups and the existing late-invalid/source-substitution cases.
Across O0/O3 the packaged programs preserve the single-module transaction bytes.
Six compile-time cases reject changed payload bounds, Registry-for-Document
substitution and output-for-input substitution. This does not export `data_shape`
or `cell_roles` names: those declarations remain module-local.

The [package editor regression](../tests/selection_bundle/editor.rs) uses these
same migrations to check imported Cell-data and nested-record fields. Completion
resolves the actual import and its package visibility, including a schema alias;
an unrelated same-named type cannot supply fields. Open dependency buffers
override their older disk contents, and making a dependency type private removes
its field suggestions. Imported record payloads expose the readonly view methods.
Completion at an unfinished trailing dot retains those open dependency schemas
and visibility rules while leaving the document's actual diagnostics intact.
These are editor hints backed by the resolver, not runtime or authorization
evidence; the compiler still performs the actual source and bound checks.

For #47, a local collection stores imported `Pair<T>` records and passes copies
to an imported generic helper after dropping the owner. Twelve O0/O3 VM runs
cover `u64`, high-word `u128` and 17-byte arrays, including wrong expected values.
The budget report derives the same 16/32/34-byte record widths through the locked
dependency, and the standalone checker verifies the generated bundles without
claiming semantic equivalence. This covers imported fixed-value generics and
copied results, not cross-package owner export or escaping arena borrows.

O3 package variants replay valid and late-invalid groups against the existing
per-profile cycle, stack, guest-page, ELF and transaction-byte ceilings. Package
source/artifact identities may differ; the original exact resource observations
remain unchanged, and no ceiling is enlarged by library extraction. These finite
package fixtures do not replace the advertised source/bounds/lifetime/authority
proofs, final API review, browser/editor package parity or the remaining
integration gates. They do not create a whole-language equivalence obligation.

```bash
cargo test --locked -p cellscript --test selection_bundle cross_package -- --nocapture
```

### Frozen complete-program resource fixtures

The [resource replay](../tests/selection_bundle/resources.rs) runs each required
Script group through the existing real-scheduler instruction tracer and requires
its exit and cycle total to match a separate standard-verifier run. Required
groups are derived from actual resolved inputs and packed outputs, then matched
against participants; no group may be silently omitted. Successful group cycles
must sum to the ordinary all-group verifier total.

The [versioned resource fixture](../tests/fixtures/selection_business_resources.json)
binds source hashes, complete ELF hashes, full transaction hashes including
witnesses, exact observations and separate numeric ceilings. It covers O3 named
settlement at 1/3/6 orders, byte-list migration at 1/3/8 documents and record-list
migration at 1/3/7/8 documents, with both output orders and maximum-batch late
amount/payload failures. These are finite-case ceilings, not a proof of global
worst-case cost for every transaction admitted by the declarations. Existing
primitive, authoring-comparison and runtime-view budgets remain unchanged.

The 35-case inventory also fills both global scan domains to 32 Cells using
unrelated untyped fee/change Cells, while retaining identical selected membership
and correspondence. Each complete program accepts that candidate through the
adapter and all groups. Separate 33rd-input and 33rd-output candidates fail both
host reconstruction and the main Script's bound probe with exit 21; peers still
accept when replayed independently. The new scan profiles have separate ceilings
and do not relax the original 23 fixture limits. Three additional candidates
combine the full scan with the maximum-batch late amount/payload failure. They
preserve input resolution, witnesses, selected membership and correspondence;
native reconstruction and canonical adapter serialization still succeed, while
the main Script rejects the final business check. Their observations fit the
existing scan-profile ceilings without raising them.

| Full 32-input / 32-output scan | Successful transaction cycles | Transaction bytes | Main group cycles: 33rd input / 33rd output |
|---|---:|---:|---:|
| Settlement / 6 orders | 846,435 | 8,834 | 27,301 / 45,203 |
| Byte-list migration / 8 documents | 1,798,330 | 23,320 | 31,261 / 49,163 |
| Record-list migration / 8 documents | 1,796,459 | 23,580 | 32,463 / 50,365 |

| Full 32-input / 32-output scan with late business failure | Main group cycles | Main exit | Observed stack bytes |
|---|---:|---:|---:|
| Settlement / 6 orders | 768,904 | 5 | 8,800 |
| Byte-list migration / 8 documents | 1,791,891 | 5 | 29,840 |
| Record-list migration / 8 documents | 1,790,020 | 5 | 32,416 |

The settlement quote peer also rejects, while its base and Lock peers accept;
both migration peers accept. Rejected transaction cycle totals remain null.
Combining these two limits does not cover every permitted input ordering,
business-value distribution or branch combination.

| Main Script / maximum ordinary fixture | Successful group cycles | Late-invalid group cycles | Observed stack bytes | Touched guest-page bytes |
|---|---:|---:|---:|---:|
| Settlement / 6 orders | 765,737 | 765,652 | 8,800 | 45,056 |
| Byte-list migration / 8 documents | 1,720,411 | 1,720,347 | 29,840 | 57,344 |
| Record-list migration / 8 documents | 1,718,540 | 1,718,476 | 32,416 | 61,440 |

The late amount fixture also makes the quote conservation peer reject with
38,401 cycles, while the base peer accepts with 38,410. Migration's Registry
and Lock peers retain successful exits. Every failing main Script exits with
code 5. The ordinary verifier does not report a rejected transaction's cycle
total, so that field stays null: independently replayed rejected groups must
not be summed into an invented executed transaction total.

Stack observations measure entry SP minus minimum SP within each VM generation,
including nested calls. Guest-page bytes count distinct pages touched, including
ELF/argv initialization and syscall buffers; they are neither host RSS nor a
heap peak. Separate heap accounting remains explicitly null. Reports also retain
declared frames, VM version/ISA and instruction/syscall/loading cycle attribution.
The fixture requires one VM generation per participant; adding EXEC/SPAWN cannot
silently retain this measurement scope.

Run the complete replay and frozen assertions with:

```bash
cargo test --locked -p cellscript --test selection_bundle
```

This closes the missing stack and late-invalid observations for these business
fixtures. It does not close stable source/machine admission, the remaining
adversarial resource corpus, final gates or review for #48/#49.

## Cell-set contract decisions to preserve

The fixed-layout phase now has a versioned
[exact-role semantic contract](CELLSCRIPT_BOUNDED_CELL_SELECTION.md) and an
independent host reference model with hostile membership, correspondence,
authority, boundary and permutation cases. This freezes protection before
Lock/layout checking, complete global scans, catalog-wide budgets and explicit
preserve/create/burn/merge shapes. An experimental source verification operation
now derives fixed-role layouts and executes through the VM with a mandatory
independent-machine-proof admission blocker. The reference model alone is not
that execution evidence; the variable-payload transition and integration
obligations remain open.

Selected transaction views now expose immutable fixed/bounded-table `.data` fields;
the [field-read contract](CELLSCRIPT_BOUNDED_CELL_SELECTION.md#fixed-schema-fields-through-transaction-views)
records exact bounds, full-width copying, complete codecs, retained buffers, source domains, checker write effects,
editor behavior and remaining admission obligations. This observation path is
available in the maximum selected/nested coexistence fixture, but does not yet
complete the mixed-role settlement or final transaction builder binding.

Independent field checking now rejects coherent result-buffer aliasing across
Cell data producers, including copied fixed fields from nested tables. The
regression covers exact/partial result overlap and overlap with another table
load, and executes a forged alias that changes hash equality in CKB-VM. This
closes that local storage-composition gap. CFG lifetime analysis also preserves
copied buffers across ordinary stores and summarized memory effects, allowing
initialization only by their recognized producer. Mutable binding snapshots
retain independent values after reassignment, with scalar/wide and fixed/nested
VM regressions. Collection record snapshots cover 8–64-byte values through
local reassignment, collection mutation and owner drop at O0/O3, with inferred
and explicitly typed bindings. Short byte arrays (1–8 bytes) use packed values
for literal initialization and local moves, while collection reads retain their
private copied buffers. Indexing resolves the actual byte storage in either
case; copying a field does not preserve an alias to that field's parent.
VM regressions cover every short-array width with literals, witness parameters,
record fields and collection results, helper returns, owner drop and unsigned
index bounds. Complete source/value provenance remains an admission obligation.

Copied fixed-value locals do not inherit their source's pointer/length aliases
or field-decoding cache. Small named records copy bytes into their own storage,
just like wider records. A field saved before parent reassignment therefore
continues to carry the evaluated value into Vec/Map operations and readonly
helper defaults; O0/O3 VM regressions cover 1/8/16/32-byte fields. Pointer-alias
propagation is bounded by the body size and rejects incompatible source/length
bindings instead of failing to converge. This is a compiler correctness boundary,
not a claim of complete independent lifetime proof.

The corrected emission-order ownership of co-located CFG labels changes the
lowering/source-map/bundle identities in two existing typed runtime-view
resource profiles. Their reproduced ELF and transaction identities, cycles,
stack frames and byte measurements remain unchanged. The fixture refresh keeps
all existing numeric ceilings and exact measurement assertions.

The selected schema and complete Script identity are separate axes. Equal-size
layouts do not make roles interchangeable. Selection must be reconstructed from
actual transaction Cells, with a probe proving the scan boundary; a witness
index list is only a proposal. All selected inputs and outputs retain global
indexes, group-relative indexes when applicable, exact role and schema identity.

Pairing must explicitly fix ordering and duplicate behavior, distinguish
one-to-one from create/burn/merge, exclude unrelated fee/change, and reject
reuse of an output to discharge independent obligations. Application amount
conservation remains explicit source predicates. Variable records use the
nested codec above; allocation-backed value collections are not a prerequisite.

The implementation must retain a failing-program matrix before extending the
source surface, then bind each admitted operation through typed IR, metadata,
independent machine checking, generated builder and complete transaction tests.
Exact-role examples precede optional runtime-selected role composition.

## Allocation decisions to preserve

The allocator is per invocation, never persistent Cell/account storage.
The implementation must freeze an explicit arena or allocator, total byte
budget, element bounds, growth and deterministic exhaustion before admitting
positive lowering. A new constructor/storage policy must distinguish allocated
Vec from the old stack-backed constructor, including observable capacity.

Map/Set iteration must be deterministic; an ordered implementation must not
masquerade as a HashMap API. Fixed-width non-Cell values are the initial target.
Borrowed element views cannot survive relocation or mutation. Resource values,
recursive ownership, pointer serialization and unsupported nested owned values
must be rejected by source analysis, not by hoping callers avoid them.

### Experimental named collection declarations

Edition 2027 now parses, formats and validates this structured declaration:

```cellscript
collections Scratch {
    limits { bytes: 8192, allocations: 32 }
    totals: OrderedMap<Hash, u64> { items: 64 }
    seen: OrderedSet<Hash> { items: 64 }
    pending: Vec<u64> { items: 64 }
}
```

This chooses one shared arena and names its collections where the budgets are
declared. It does not change ordinary stack-backed `Vec::new()` or introduce a
wire type. `items` is a maximum element count, not an initial reservation.
Each member's maximum payload plus its allocation record must fit individually;
the members' simultaneous use and retired growth buffers still compete for the
shared byte/event budget. Independent maxima do not promise that all members can
reach their maxima together. The [declaration fixture](../tests/syntax_combo/seeds/named-collections.cell)
executes no allocation. `Scratch::new()` initializes a disjoint bounded arena
in the invocation frame; local methods and iteration now lower to the primitives
below. Read-only `&Scratch` helpers receive the caller's arena pointer and exact
extent. Each borrowed operation checks that extent, the live descriptor and its
backing allocation; copied inputs/results use separate callee frame scratch.
Forwarding a borrow preserves its extent. The mandatory
`allocated-values-artifact-proof` runtime blocker prevents this experimental
source execution from being mistaken for independent allocation admission.

For a package declaring `edition = "2027"`, inspect the budgets with
`cellc explain collections path/to/package` (or add `--json`). This native
declaration analysis includes unused owners and resolves imported record/generic
widths through the same type checker. It reports payload, 16-byte rounding,
32-byte allocation records, retained growth buffers, and shared byte/event
shortfalls. An insufficient shared budget is reported as `fits: false`, not a
source error: the program may intentionally use only part of the declared maxima.

Hovering a named `collections` declaration (or its constructor's owner name)
shows the same member costs, growth scenarios and shared shortfalls in the editor.
The CLI and hover share one calculation. Imported generic record layouts resolve
through the package graph, including valid unsaved dependency buffers; unresolved
or inaccessible types yield an unavailable estimate rather than guessed widths.
The optional WASM `language-service` exposes the same single-source hover; the
default browser metadata response does not gain a budget report. These hints do
not reserve capacity or replace source diagnostics.

The report compares two explicit scenarios starting from an empty owner: reserve
each member to its maximum before insertion, or insert one element/distinct key
at a time with no explicit reserve calls. These are not a proof of arbitrary
program paths or a worst-case estimate. Intermediate reserve requests, owner
recreation and other growth histories need separate analysis. The owner region
includes arena state, member descriptors and copy scratch outside the byte
budget; it excludes preceding alignment, scalar slots, other locals and call
frames, so it must not be read as total stack or peak VM memory.

For the [complete transfer example](../examples/language/batches/multi_asset_transfer.cell),
the 4096-byte/4-event `Totals` owner produces:

| Members at their 32-item limits | Reserve each once | Incremental growth |
| --- | --- | --- |
| Each `OrderedMap<Hash, u128>` | 1568 bytes, 1 event | 3216 bytes, 6 events |
| Each `OrderedSet<u64>` | 288 bytes, 1 event | 704 bytes, 6 events |
| Both Maps and both Sets together | 3712 bytes, 4 events: fits | 7840 bytes, 24 events: does not fit |

The business example reserves against checked input/output counts. The initial
authoring decision above selects these explicit reservations together with
CLI/editor budget explanations; it does not rule out a later declarative form
supported by complete-program evidence.

The [budget execution test](../tests/collection_budgets.rs) compares the report
with a separate logical arena model and actual interleaved Vec/Map/Set operations.
Its six profiles cover 1, 3, 8, 32, 128 and 1024 items with element widths of
1, 17 or 64 bytes. Across 68 O0/O3 executions, exact reported budgets succeed;
one 16-byte alignment unit or one allocation event less rejects with runtime
error 21. Incremental growth of the 1024-item profile exceeds the admitted
65536-byte arena and rejects at that limit. Large-profile execution uses an
explicit 100-million-cycle measurement limit after exceeding the ordinary test
runner's 10-million-cycle limit. This verifies budget thresholds for those
programs; it does not claim that 1024-item collections satisfy a business cycle
budget or that every growth history is covered.

The [complete transfer policy experiment](../tests/selection_bundle/collections.rs)
changes only the four reserve arguments from checked input/output counts to 32.
It runs the same 44 valid/invalid cases under both policies at O0/O3, retaining
native/generated-SDK byte agreement, canonical transaction bytes, required
Type/Lock groups and hostile-Lock rejection. The generated
`target/collection-preallocation-comparison.json` binds both source digests and
records 24 O3 resource observations. Representative whole-transaction cycles are:

| Input and output Notes / distinct assets | Actual-count reservations | Reserve all maxima |
| --- | --- | --- |
| 1 / 1 | 44,258 | 158,322 |
| 8 / 1 | 241,389 | 330,109 |
| 32 / 32 | 2,040,832 | 2,040,848 |

The current allocator clears each allocated buffer, so unused reserved capacity
still consumes initialization cycles. The full-maximum variant also increases
the generated transfer ELF from 40,536 to 40,568 bytes and the observed valid-path
stack from 7,904 to 7,936 bytes; these are measurements of this compiler and
program, not intrinsic costs of declarative syntax. Touched guest pages are
53,248 bytes in both variants for these valid cases. The original numeric
ceilings are unchanged. After the wide-copy optimization, both ELF sizes and
cycle totals fit those ceilings; the full-maximum candidate still exceeds the
stack ceiling. The comparison records that exceedance instead of granting admission.
Rejected transactions retain a null whole-transaction cycle field rather than
substituting a sum of independently replayed groups.

Consequently, automatic full-maximum reservation is not selected as the default.
Any future declaration/constructor design must support checked per-invocation
member counts, explain their combined budget before use, and make full-maximum
reservation an explicit choice. It should reduce repeated reservation syntax
without forcing small transactions to initialize every declared maximum. The
syntax and failure/initialization boundary remain design work; this experiment
changes neither source syntax nor the allocator and is not a global worst-case
proof.

The initial budget-experience decision combines named workspaces, copied reads,
non-escaping readonly helpers, explicit count-based reservations and shared
CLI/editor budget explanations. Keep the monotonic arena while evaluating
future declarative preallocation against complete business programs. Developers
should not need to derive alignment, record overhead or historical growth by
hand. Whole-owner borrowing remains the current
rule; use real examples of reading one member while updating another to decide
whether member-level borrowing is needed. Arena lifetime (#47), decoded byte/view
lifetime (#48), and Cell source/selection/state rules (#49) remain distinct
contracts. A shared helper style does not make them one allocator abstraction.
Machine-proof progress alone does not settle these API decisions: keep allocator,
read/write, borrow and call contracts explicit, with independent semantic models
and mutation evidence, so emitter-specific recognizers can evolve behind those
contracts without silently changing the language.

Declarations are module-local and contain 1–32 uniquely named members. The
arena limits match the private primitive. Value declarations admit fixed scalars,
1–64 byte arrays and ordinary fixed records, with a 64-byte Vec element ceiling
and a 32-byte Map value ceiling. Record fields may recursively contain these
scalars, byte arrays and records; concrete applications such as `Pair<u64>` use
the existing generic specialization and ordinary record layout. Empty records,
recursive layouts, references, hidden Cells and nested owned values reject even
in unused declarations. Other aggregate field shapes remain unsupported in this
experimental allocation path. Keys remain unsigned u8/u16/u32/u64, Hash,
Address, or 1–32 bytes; records do not introduce an implicit ordering.

Parsing retains named record references without inventing widths. After name
resolution and specialization, the type checker checks every field and the
resolved member budget. Codegen and the independent checker separately derive
record widths; the latter also checks field offsets and declared widths. Layouts
used only by an unused member of a constructed owner remain in scoped artifact
evidence. This structural check does not discharge the allocation proof blocker.
Record reads copy into local storage and reuse the ordinary caller-owned helper
result ABI. See the [record fixture](../tests/syntax_combo/seeds/collections-records.cell)
and [runtime cases](../tests/allocated_runtime.rs). Imported generic value copies
are exercised by the complete transfer suite; final parity, admission and
business acceptance remain part of #47. Exporting owner declarations is outside
the initial module-local interface.

The initial-form decision above selects this declaration after complete
aggregation/deduplication programs, helper extraction and measured reservation
comparisons. Stable admission still requires final parity, independent-checker
and integration evidence. The experimental type checker enforces these local rules:

- Construction types a fresh invocation-local owner. Named member methods use
  the declared types without repeating budgets; each constructor reserves aligned
  arena state, budgeted allocation bytes, member descriptors and copy scratch.
- Moving an owner transfers all its collections; copying owners or extracting
  an independently owned member must not duplicate allocation authority.
- Ordinary reads return fixed value copies. Explicit read-only borrows and
  local observation-only helpers retain the owner lifetime and prohibit
  mutation/drop through aliases. Pass `&owner` for a single call, or forward an
  existing borrowed binding directly. The borrow lasts through subsequent
  argument evaluation; those arguments cannot mutate, move or drop that owner.
- An unused owner may leave scope without a Cell-consumption obligation;
  its frame region is reclaimed on return or reinitialized on scope reentry.
  Explicit `drop` retires member allocations; it does not refund allocation
  events or arena bytes within that owner lifetime. Owners and references cannot
  escape into Cell data, witness encoding, return values, wrappers or another
  owner. Cell `consume`, `destroy`, `claim`, `settle` and correspondence syntax
  do not accept these allocation owners.
- Mutating collection operations remain explicit statements; for example,
  deduplication first records the boolean insertion result and then `require`s
  it, preserving the purity requirement for `require` conditions.

Member methods include length/capacity/emptiness, reservation, Vec
push/insert/set/get/remove/pop, map insert/get/get_or/contains_key/remove and
set insert/contains/remove. Map insertion returns whether a key was new;
updating an existing key preserves order. Vec reads and map `get` are checked
operations with deterministic failure for an invalid index/missing key;
`get_or` supplies the explicit fallback. Iteration copies elements (map entries
are `(key, value)`) and borrows the whole owner for the loop body. No element
pointer is exposed. Source CKB-VM cases exercise local operations, owner moves,
loop reentry, sorted traversal, exhaustion and copied fixed-width values at
multiple optimization levels. Helpers can read, traverse and forward a borrow,
return fixed value copies, and use their own local owner. Early return ends a
parameter borrow without transferring the caller's owner. Runtime tests include
an executed early-return branch that drops a helper's local owner; the continuing
path retains its own owner state. This branch merge exemption applies only to
affine collection owners and does not waive Cell lifecycle obligations. Tests use
two distinct owners, repeated references, outgoing stack arguments and the
maximum arena size. These cases do not establish independent allocation machine
proof or the complete business acceptance corpus.

Experimental ordered members also expose `load_sorted(entries)` for an empty
live destination. The input is a fixed array of at most 1024 keys (Set) or
`(key, value)` pairs (Map), ordered by the same unsigned/lexicographic key order
as `insert`. Set duplicates collapse; Map duplicates keep the last input value.
The emitter checks the complete input order and counts distinct keys before
reserving once and copying. The distinct count, rather than the input length,
must fit `items`. Existing reservation on an empty member can be reused. Empty
input does not dereference an input buffer, but still checks the live empty owner.
Nonempty destinations, descending keys, exhausted shared budgets and overlapping
input/destination storage reject with runtime error 21. No input is mutated.

This slice admits scalar, Hash/Address and fixed-byte values. Record-valued
batch input is explicitly deferred until aggregate storage/ABI coverage is
complete; those Maps retain `reserve` plus `insert`. Arbitrary input sorting is
also pending. This method neither sorts silently nor removes the allocation
proof blocker. See the [source fixture](../tests/syntax_combo/seeds/collections-sorted.cell).
Array construction records exact element types/counts in typed evidence; the
checker rejects altered array shapes and borrowed mutation after hash rebinding.

The private VM comparison uses identical input/frame initialization for both
paths and compares sorted loading with reservation plus repeated insertion.
At 1024 u64 keys it measures 2,683,174 → 1,233,695 cycles for Set and
3,109,158 → 1,561,375 for Map<u64, u64>. Charged arena bytes stay at 8,224 and
16,416 respectively (one allocation each). The suite also covers 8/32/128 items,
duplicates, bad order and memory guards. These totals include harness setup;
they are comparative primitive evidence, not frozen application budgets.

The [aggregation](../tests/syntax_combo/seeds/collections-aggregation.cell) and
[deduplication](../tests/syntax_combo/seeds/collections-deduplication.cell)
programs verify witness values and return zero on success. Aggregation compares
the computed sum with an explicit expected total; deduplication rejects repeated
identifiers. Source-to-VM tests compare identical, distinct, descending,
interleaved and late-duplicate inputs against independent `BTreeMap`/`BTreeSet`
oracles at O0 and O3, including zero amounts, the u64 maximum, total overflow
and a wrong claimed total. Both retain the allocation proof blocker. This is
algorithm and witness-execution evidence, not asset custody or full-transaction
acceptance.

### Full-transaction collection aggregation

The experimental [multi-asset transfer](../examples/language/batches/multi_asset_transfer.cell)
now reads actual input/output `Note` Cells in its Type group. Two
`OrderedMap<Hash, u128>` members aggregate amounts per asset label; two
`OrderedSet<u64>` members reject duplicate serials separately on each side.
Each side admits 1–32 positive `u64` amounts. Accumulating in `u128` preserves
the exact total even when all 32 amounts equal `u64::MAX`. A read-only
`conserved(&Totals)` helper checks both key membership and per-asset equality.
The four collections reserve once within a shared 4096-byte, four-allocation
arena. This does not change monotonic charging or reclaim retired buffers.

The [complete transaction fixture](../tests/selection_bundle/collections.rs)
generates builder manifests, compares native and generated SDK Cell encoders,
checks the native ProtocolBundle, compares packed adapter bytes with an
independent Molecule transaction construction and executes
the Type and spending Lock groups. An unrelated fee input places the first
Type-group input at transaction index 1. An independent Rust `BTreeMap`/`BTreeSet`
model checks O0/O3 cases with 1/8/32 notes, repeated/distinct asset labels,
reversed output order, split/merge, maximum amounts, late amount/asset changes,
duplicate input/output serials, zero amounts and 33-note rejection. Replacing
the spending Lock with a rejecting fixture makes otherwise valid transfers fail.
The normal test Lock is permissive; this is not a wallet authentication example,
issuance protocol, xUDT implementation or deployment approval.

The [fixed-array reference](../tests/selection_bundle/fixtures/multi_asset_transfer_stack.cell)
copies up to 32 Notes per side into fixed stack arrays and uses linear scans
for per-asset totals and serial uniqueness. It implements the same transfer
rule, including output-only assets, positive amounts and occupied capacity.
The legacy stack Vec has a fixed 256-byte backing buffer and cannot hold 32
48-byte Notes, so it is not used as a full-size reference. This array program
is a comparison fixture, not the proposed collection authoring API.

Both implementations execute all 44 cases at O0 and O3 against the independent
host model. Packed transaction bytes must match exactly for each paired case.
Each implementation runs in a separate local resolver with its own ELF behind
the same fixture Type-ID/dependency coordinates; byte equality does not mean
the two deployment states or code artifacts are identical. The original
allocation example retains both allocation and Cell-data admission blockers.
The reference retains the Cell-data blocker in its program metadata; its reads
are in helpers rather than directly in the action. Generated action plans are
checked against their own action metadata, and remain non-submittable.

The separate [transfer resource fixture](../tests/fixtures/collection_transfer_resources.json)
freezes 24 O3 observations: 12 cases for each implementation, with source,
ELF and complete transaction hashes and separate numeric ceilings. It includes
1/8/32-item successes, repeated/distinct assets, split/merge, maximum-size
duplicate serials, a last-key amount failure, zero amounts and 33-item rejection.
The existing 35 settlement/migration resource observations are unchanged.
Typed wide-argument copies now retain one source pointer across all byte stores.
The refreshed Map/Set observations remain within every original numeric ceiling;
array observations and canonical transaction identities are unchanged.

| Notes per side / asset labels | Map/Set transaction cycles | Array transaction cycles | Transaction bytes |
|---|---:|---:|---:|
| 1 / 1 | 44,258 | 228,108 | 638 |
| 8 / 1 | 241,389 | 449,574 | 2,458 |
| 8 / 8 | 314,876 | 324,806 | 2,458 |
| 32 / 1 | 1,045,213 | 3,258,606 | 8,698 |
| 32 / 32 | 2,040,832 | 1,048,430 | 8,698 |

These successful totals include both Type and Lock groups. Map/Set uses 7,904
observed stack bytes and a 40,536-byte main ELF; the fixed-array program uses
17,472 observed stack bytes and a 42,824-byte main ELF. Stack measurements are
entry-SP-to-minimum-SP observations, not a separate heap metric. The opposite
cycle results for repeated and distinct assets prevent a blanket speed claim.
The array fixture performs repeated linear scans and initializes all fixed
slots; it is a concrete equivalent implementation, not a proof of the cheapest
possible stack implementation. Both programs read identical Cell data; the
collection program reserves once, so this comparison does not measure repeated
arena growth or reclaim retired buffers.

Rejected transactions retain a null total cycle count; independently replayed
groups are recorded separately. For the last-key amount failure the Type group
uses 2,040,168 cycles with Map/Set and 849,416 with arrays, and both reject with
error 5. These are finite corpus ceilings, not global worst-case bounds.

The [native fixed Cell encoder](../src/fixed_cell_builder.rs) and generated
`encodeFixedCellData(type, fields)` accept named fields with exact bytes. Source
declaration order determines concatenation; JavaScript property order does not.
The supported flat profile includes unsigned integers, `i32`, Hash/Address,
1–256-byte arrays and canonical booleans (one byte, 0 or 1). Ordinary structs,
aggregate fields, enums and dynamic fields are absent from `fixedCellContracts`;
they cannot fall back to an unchecked raw encoder. Type/field identity, contiguous
offsets and encoded sizes are derived from source metadata. This is a fixed
Molecule struct with no table header. Encoding does not authenticate metadata,
check application invariants, sign a transaction or remove admission blockers.

The transfer test compares every SDK-produced Note against native encoding and
an independent field concatenation, then places those actual SDK bytes into the
candidate used by the adapter, resolver and VM. Its generated `planTransfer`
retains `canSubmit = false` and both allocation/Cell-data proof blockers.
The separate `fixed_cell_builder` suite covers every supported field shape,
missing/extra fields, width errors, noncanonical booleans, unsupported types,
inherited fields, prototype-like type names and inconsistent metadata layouts.
The generated package also passes its strict TypeScript build.

This combination exposed a missing memory-effect boundary: the checker could
not prove a dynamic arena store disjoint from retained Cell data. Source-level
collection operations now guard each dynamic write with independently checked
unsigned lower/upper address bounds. Owned operations may write their arena,
descriptors and local copy scratch; borrowed read-only operations may write only
their callee-local copy scratch. Three live registers are saved/restored through
a temporary aligned 32-byte subframe. The checker validates actual instructions,
private-frame bounds, all protected regions, terminal error-21 edges and absence
of branches into the checked sequence. It summarizes the complete sequence to
avoid retaining redundant CFG states; proof budgets are unchanged. Rebound
mutations cover missing/changed bounds, address displacement, writes below the
temporary subframe, entry past guards and incorrect SP restoration at O0/O3.

Owned arena storage is also protected between recognized operations. The
independent constructor derives the region containing state, allocation
records, elements and descriptors. Complete checked primitive ranges authorize
writes only to their own region; an ordinary result store, unrelated copy,
call or syscall cannot change it. The write-effect proof runs even for a
local-only owner with no Cell/nested view or borrowed helper. Private scratch
remains separate from the arena's persistent state.

The [arena-isolation mutations](../tests/artifact_checker/allocated_isolation.rs)
redirect an ordinary helper's result store into the cursor, allocation live
flag, element bytes and descriptor length/live flag, without changing any
collection operation or its evidence. They also redirect a tuple copy into
live element storage and change its count/stride. O0/O3 VM outcomes distinguish
the corruption from the valid program. Finite private byte-copy recognition
checks the complete loop, actual incoming destination/count and absence of
partial entry; it does not prove the copied source's semantic value. This
preserves existing tuple and stack Vec composition while closing unrelated
writes into the arena, not every source/value or ownership obligation.

An independent typed-CFG pass additionally tracks initialized owners, affine
moves, explicit drops and read-only references through branches and loop
backedges. Available handles intersect at joins; possible active lexical loans
are combined, so a loan on only one incoming path still prohibits mutation.
Growth/mutation invalidates temporary references, while moves and drops retire
all handles to the old owner. Calls accept only live read-only references to
local helpers, including reference temporaries whose canonical ABI type is
spelled `Work` rather than `&Work`. Borrowed parameters cannot become owners;
owners/references cannot be returned, embedded in values or sent externally.
Rebound O0/O3 cases cover stale constructor results, reads after move/drop,
borrowed drops, mutation during argument evaluation, branch joins, loop
backedges, nested loans and a loan retained on only one predecessor.

The pass checks the declared typed graph, not correspondence between that graph
and every allocator machine instruction. Its work/state bounds fail closed;
the allocation admission blocker remains mandatory. Lexical alias identity is
retained in the IR and typed locals even when an outer alias is used only for
a nested reborrow. This fixes an otherwise valid nested-borrow rejection without
allocating a view object or changing its runtime ABI.

Typed owner constructors now have a separate independent machine check. Every
constructor must have a unique begin/end range whose instructions write exactly
four zero arena-state words, three zero descriptor words and one live flag for
each declared member, then publish the arena address to the constructor's local
slot. The checker derives constants and SP-relative addresses from instructions;
labels only locate the candidate range. Loads, calls, branches, SP changes,
unknown values and non-scratch register writes inside it reject. The region
must be aligned and fit the actual prologue frame, outside scalar slots, other
owner regions and saved frame bookkeeping, with room for private copy scratch.
This does not yet prove disjointness from every other fixed-value buffer.

The shared typed-block CFG check makes the initializer unavoidable on successful
paths and rejects entry into its interior. Rebound O0/O3 mutations cover every
initialization store, zeroed live flags/owner pointers, erased boundary labels
and a bypass with both machine CFG edges and outer identities recomputed. The
fixture uses two owners, three different member types and both 512-byte and
65,536-byte arenas, exercising large address materialization. The optional
`allocated_init_labels` lowering field is confined to experimental constructors;
older deny-unknown-fields checkers reject records carrying this extension.

Allocation primitives now have a separate bounded instruction recognizer. It
checks the null request path, byte/alignment/event limits before mutation,
cursor and event-count increments, all four allocation-record fields, returned
payload address and the complete byte-zeroing loop. Every dynamic store must
preserve its spill registers and enforce the checked owner's private bounds;
all failure paths terminate with error 21. Both small and large constants and
stack offsets are derived from actual RV64 instructions.

The experimental `allocated_allocation_labels` field locates candidate ranges.
The checker independently requires two static allocation paths per reserve,
push, insert or nonempty sorted batch, grouped by typed block and constructor
storage through owner moves. Missing, extra, duplicate or overlapping ranges
and external branches entering a partial allocation reject. Empty sorted input
requires no allocation site. Rebound O0/O3 tests mutate budget predicates,
cursor/record writes, result addresses, zeroing and marker inventories for
64-byte and 65,536-byte arenas, plus skipped admission and external loop entry
with the machine CFG and all identities rebound.

The `allocated_lifecycle_labels` extension binds one arena resize per allocating
operation and one arena release per member of an explicitly dropped owner.
Each resize must contain its two already checked allocation children, with the
same owner, contract and private region; detached or reused children reject.
The checker recognizes the complete record walk inside resize/release, including
cursor/count bounds, aligned extents, requested size, allocation ordinal and
live flags. It accepts only an exact live payload address from that walk.

The non-null resize path saves the old pointer/size in the arena's private
scratch, allocates a zeroed replacement, copies exactly `min(old, new)` bytes
and invalidates the old record. Zero-size resize and explicit release only
retire the live record; they never refund cursor bytes or allocation events.
Null release is a no-op. Parent ranges cannot overlap or be entered midway,
including an external branch to a child allocation's start. Rebound O0/O3
mutations alter lookup predicates/loads, scratch transport, copy bounds/strides,
retirement stores and range inventories, including large offsets for a second
owner. This establishes these arena primitives, not every use of record lookup
in collection operations.

Reserve/drop descriptor ranges now bind the enclosing member transition. The
typed member ordinal determines its exact descriptor address, fixed element
width and item limit, including record values. Before changing it, the checker
recognizes the live flag, `length <= capacity <= limit`, null empty storage and
exact live allocation lookup with `allocated_bytes == capacity * element_width`.
Reserve validates the requested capacity, the no-growth branch, doubled/minimum
growth clipped to the member limit, its checked resize child and publication of
the resulting pointer/capacity. Drop validates its release child and clears all
four descriptor words. Each lifecycle child belongs to exactly one descriptor
parent; missing ranges, member substitutions and partial entry reject.
Rebound tests cover multiple owners, heterogeneous members, fixed records and
large frame offsets. Early returns use the actual private prologue frame when
checking restore tails; temporary spill budgets are not the amount restored.
Owned push/insert/set/remove now have complete instruction recognizers as well.
Their typed member fixes the descriptor, element width, bound and private input
scratch. Push/insert compose exactly one checked reserve for the same member;
set/remove require an in-range index. Push copies one full element, insert moves
the suffix backwards before copying, and remove moves the suffix forwards then
zeros the vacated trailing element. Exact pointer/count steps and final length
stores are checked; replacement preserves length. The source inventory includes
both replacement and insertion bodies for Map insertion, insertion only for
Set, and removal for Vec pop and ordered removal. Child entry and erased or
extra ranges reject. Rebound tests mutate bounds, byte loads/stores, displacement
arithmetic, loop direction/counts and length stores for wide records and large
second-owner offsets.

Owned indexed reads copy the complete fixed-width element into the owner's
private result scratch after descriptor and index validation. Vec remove/pop
parents require that read before the checked removal of the same member/index;
pop derives its index from `length - 1`, with the read bounds check rejecting an
empty vector. Ordered entry projections reuse the full-entry copy. Map lookup
composes checked search and indexed read: an absent key leaves the preexisting
result buffer unchanged, a found key copies the whole entry, and required `get`
terminates with error 21 when absent. These checks do not yet prove default-value
staging or final conversion/projection from scratch into the source destination.

Owned ordered search now binds its key profile, full member stride, midpoint,
shrinking half-open search interval and exact found/index results. Unsigned
integers compare high bytes first; Hash, Address and fixed byte-array keys use
lexicographic order. Map/Set update parents compose that checked search with
checked element children for the same member: Map replacement preserves length,
duplicate Set insertion does nothing, missing removal does nothing, and returned
booleans match the selected branch. Typed site inventories also retain owned
contains/get/get-or searches. Rebound tests cover byte/count/stride changes,
comparison and bound mutations, erased ranges and entry into a partial child;
accepted profiles cover every integer key width, Hash/Address and representative
byte-array widths at O0/O3, with nested local aliases.
This does not yet establish the sorted invariant across an entire program:
input/value provenance and complete operation ordering remain unproved.

Sorted construction now binds its 0–1024 input count to the typed array operand.
The machine recognizer requires an empty destination, nonwrapping input span and
disjointness from the destination arena/descriptor. Its first scan rejects
descending keys and counts unique entries before invoking the same-member
checked reserve. The second scan copies each fixed-width entry, overwriting the
last output on equal keys; maps retain the last value and sets retain one key.
Empty batches only check the descriptor and empty length, without reading an
input pointer or reserving. Singleton batches omit the initial adjacent-key
scan. Rebound tests mutate span checks, comparison direction, counts/strides,
length publication and copied bytes, erase boundaries, change marker counts and
enter child bodies; numeric/byte keys and large second-owner offsets are covered.
Input-pointer provenance remains separate from recognition of these span checks.

Readonly helper entry now independently binds the arena pointer's typed scalar
slot and one distinct extent slot to their incoming ABI words. Expanded preceding
arguments determine those word indices, including register/stack splits. Mixed
nested/Cell-view signatures resume only from independently checked decoder
prefixes; no external jump may bypass the helper prologue. The memory-effect CFG
protects both slots against full and partial stores, including writes through
checked dynamic-copy summaries. They do not become transaction coordinates or
gain Cell lifecycle authority. Rebound tests cover two same-type owner parameters,
forwarding signatures, mixed nested/coordinate parameters and all store widths.
This prefix check proves incoming spills and preservation; borrowed read bodies
are checked separately below. Complete machine owner lifetime remains pending.
The mixed-signature fixture also writes a collection before its coordinate-bearing
helper call in the same typed block. Transport and memory-effect checks share an
exact recognizer for the 32-byte temporary spill, bounded address setup, two
unsigned guards, store and complete register/SP restoration. Both evaluate the
private-frame bounds; the memory proof additionally protects live retained slots.
Transport discards prior slot/outgoing facts after the dynamic write, so the
actual call must reload its typed coordinate. Arbitrary branching in outgoing
argument frames remains rejected. Rebound tests change saves/restores, stack
adjustments, guards and their bound registers, store offsets, interior entry and
the final coordinate load. Shared source-to-VM fixtures cover register/stack
arguments, forwarding and 512/65536-byte arenas at O0/O3. Arena argument provenance
is checked separately below.

Arena helper calls now bind both actual ABI words to the typed source owner on
every reaching machine path. Pointer facts are created only by independently
checked constructor summaries or checked incoming readonly parameters. Actual
loads, stores and zero-offset copies propagate those facts; overlapping full or
partial writes erase them, and a constructor executed again invalidates previous
facts for that source. CFG joins retain only matching facts. Source moves and
lexical reborrows resolve to the same owner identity used by allocation sites.
At each call, an owned argument needs that owner's pointer and its exact declared
arena extent, excluding copy scratch. Forwarded arguments need pointer and extent
from the same incoming parameter. The checker also binds target/order and the
expanded stack extent, including hidden fixed-result arguments. Rebound tests
substitute another same-type owner, replace owned/forwarded lengths and overwrite
the retained binding on either predecessor of a branch join before argument
evaluation, across register/stack boundaries and small/large frames. The shared
CFG proof retains historical states at branch targets and checked summary exits,
and consumes straight-line states after propagation. Loop targets still merge
facts to convergence; rebound tests also clobber pointers after a valid call and
require rejection at the next iteration. State, fact and step limits remain
unchanged. Large record
and ordered-collection regressions cover the resulting memory use. This
establishes argument identity and extent transport;
it does not complete machine retirement, final source result values or whole-operation
source correspondence.

Readonly helper bodies now reuse independent descriptor, allocation-record walk,
ordered search and indexed-copy recognizers with caller-relative addresses.
Each read reloads its typed pointer slot; the CFG proof requires that slot's
incoming arena identity, and each extent check needs the matching incoming
length slot. Descriptors bind the declared member, width and item limit, live
flag, length/capacity relation, exact record payload and allocation ordinal.
Map lookups compose the search and copied-element children, preserving absent
defaults or the required missing-key error. Length/capacity/empty reads bind
their selected descriptor result. Copies write only to one disjoint 128-byte
private scratch region, never into the borrowed arena. Typed-block inventories
reject missing/extra ranges even when every borrowed-read marker is erased.
No edge may enter a partial checked read. Rebound tests alter extent,
addressing, bounds, copies, result selection, marker coverage and interior entry;
shared VM cases cover records, numeric/byte keys, missing keys and indexes at
register/stack boundaries with small/large arenas. These body checks still do
not by themselves prove index/key staging, final source result publication, whole-operation
reachability or full source-to-machine/lifetime correspondence.

The collection transport check now binds every explicit index (including reserve
counts and iteration entry indexes) to its typed constant or local scalar slot.
It recognizes the exact argument register load or immediate store-to-load
forwarding, then requires the checked primitive for the same owner/member at the
next address. Copied `bool`, `u8`, `u16`, `u32`, `i32` and `u64` results bind the
producer's scratch buffer to the typed destination slot, checking little-endian
assembly, signed extension and Map key/value offsets. Typed-block inventories
remain mandatory even if all transport markers are erased. External branches
cannot skip the input stage or the result producer; label-only evidence preserves
existing optimized instruction bytes. Rebound cases substitute argument loads,
copy arithmetic and destination stores, remove markers and redirect source
branches around transport. Small/large arenas and O0/O3 cover owned operations
and readonly helpers. This establishes local argument/result transport, not
upstream scalar computation,
whole-operation reachability or complete source equivalence.

Key/value preparation now composes with that transport. For complete argument
lists of scalars and pointer-backed fixed values, the checker binds each input
to its typed local slot or literal. It recognizes every scalar byte store/shift
and the typed pointer load plus every copied byte into the exact member scratch region.
The latter covers u128, byte arrays, Hash/Address and admitted fixed records;
record widths are independently derived from their typed fields. Existing source
and length checks precede the transfer boundary. Fixed-value copies then use the
already evaluated typed local instead of re-decoding its original field. Each
copy retains that pointer in an explicitly preserved register across small or
large-offset stores, avoiding a repeated pointer reload for every byte. This covers Vec push/insert/set,
ordered insert/remove/contains/get and Map get_or defaults, including readonly
helpers. Vec index preparation is included in the checked range. The consumer
must be the adjacent independently checked primitive for the same owner, member
and typed block; external branches cannot skip any part of preparation. Complete
typed inventories reject omitted markers rather than trusting label presence.
The [argument regressions](../tests/artifact_checker/allocated_arguments.rs)
cover scalar and fixed-value inputs, small/large arenas and O0/O3, wrong input slots,
byte widths/offsets/shifts, marker erasure, forged owner/member coordinates and
bypass branches. A key substitution accepted with the new check disabled changes
the full program's VM outcome; the complete check rejects it after hash rebinding.
For 1–8-byte array arguments, the checker derives the representation from typed
producers: parameters, literals and local copies carry packed words; fields,
indexed elements and collection reads carry pointers, and reference conversions
preserve the source representation. Every literal byte is checked independently.
VM regressions also cover packed fixed-aggregate parameter and local-copy indexing,
constant and dynamic element selection and unsigned bounds. Short elements retain their
addressed storage when subsequently copied into collections.

Nonempty `load_sorted` input transport binds the evaluated typed local to the
sorted primitive's exact input-pointer scratch slot. Its array element type,
count and byte extent come from the member layout and typed operation. Packed
arrays use the address of their scalar slot; pointer-backed arrays load the
evaluated pointer. The same complete inventory, adjacent owner/member binding
and branch-entry checks apply. Empty arrays introduce no input-read evidence.
The [sorted-input mutations](../tests/artifact_checker/allocated_sorted_inputs.rs)
cover counts 0/1/4/1024, short/wide keys and values, small/large arenas and O0/O3.
They substitute machine and typed input identities, erase preparation and markers,
forge coordinates and bypass preparation. VM regressions additionally check
mutable record field snapshots after reassignment and loops, and 1024-item
`OrderedMap<u16, u8>` construction against a host `BTreeMap`, including last-item
disorder/duplicates and prior shared-budget consumption. These maximum-size VM
inputs fit the existing entry-witness limit; larger checker-only shapes are not
runtime acceptance evidence. No arena or entry-witness limit is increased.

Mutable local record, tuple and fixed-array reads refresh projections after
assignment. Projected writes rebuild each affected parent into separate storage
before replacing the root, preserving earlier reads and whole-value snapshots.
Array writes still require a compile-time constant, in-range index; literals use
the destination type. Iteration, tuple destructuring and match projections read
current storage across loop backedges. Fixed primitive tuple/array fields in records are
serialized with their complete width. Short aggregate fields use addressed
copies. Fixed primitive tuple helper results retain the eight-register field
ABI, with a complete caller-owned copy made before subsequent calls. Return
values are read from current storage, including parameters and updated locals,
instead of replaying constructor operands. Wider fields are copied without a
helper call while the callee bytes remain available. More than eight fields
fail closed; this does not introduce a larger return ABI. These rules do not
widen the admitted collection element-type inventory.
The [runtime fixtures](../tests/allocated_runtime.rs) and
[projected-write cases](../tests/allocated_runtime/projections.rs) check mixed
nested paths, by-value helpers, imported generic records, branches and loops,
unchanged siblings and 1/8/9/32/1024-byte arrays. Rebuilding copies all components
of each affected parent and uses frame storage; it does not reclaim arena bytes
or establish new resource ceilings. Cell resource transitions retain their
dedicated lowering. General aggregate/source proof obligations remain open.

The [tuple-return cases](../tests/allocated_runtime/returns.rs) additionally
check whole-value forwarding and snapshots, conditional writes, nested small
tuples, empty fields, negative i32 values, 1/7/8/9/32/1024-byte fields and all
eight registers with offsets above 2047. They execute at O0/O3 and require
register overflow to fail. This is runtime evidence for fixed primitive tuples;
it does not certify the generic source/lifetime proof or ordinary-record,
dynamic and resource tuple combinations.

This establishes local value transport, not complete upstream value computation or the
validity/lifetime of every source pointer. Those obligations still apply to sorted
arrays; an operation with an unproved argument does not claim partial coverage.

Scalar definitions within bodies that contain allocated-value operations, Cell
coordinates, exact selections or nested views have mandatory source-coordinate
boundaries, including named-shape helpers without an arena. The independent checker derives
their complete inventory from typed operations and checks constants, copies,
negation, boolean operations and integer arithmetic/comparisons, including
signed division/remainder, truncation/sign extension and fatal divisor/shift
guards. It binds every operand/result slot, forbids overlapping or reordered
ranges and outside entry into a partial definition, and handles large stack
offsets. Every successful exit from a covered typed block must execute these
definitions in order; branches cannot skip an entire checked range. Fatal
arithmetic guards retain their exact independently checked exit behavior.
Comparison values retain the exact boolean result; fused branches bind
the predicate's signedness, operand order, polarity and both typed successors.
The checker permits fusion only for an adjacent branch condition that is the
result's sole typed use. Empty unconditional target blocks may resolve to their
successor, with cycle/work limits; definitions and conditions cannot be skipped.
The path traversal follows both fused successors after the checked operand
preparation, rather than treating the branch as an atomic fallthrough.
Checked Cell reads and selections compose in the same typed operation order;
their complete independently recognized bodies account for inline returning
syscalls without trusting an unverified syscall annotation.
Complete nested load/read/output and coordinate bodies also compose after their
instruction and lifetime checks pass. A summary cannot hide a required scalar
definition or partially overlap another checked body. O0/O3 Cell and named-view
fixtures reject changed arithmetic, whole-definition bypasses and erased markers;
complete settlement and byte/record migration examples require the same inventory.
The markers do not add executable instructions. Entry/helper parameter decoding, aggregate producers and
preservation across arbitrary intervening writes remain separate obligations;
these definition checks do not discharge the complete source/lifetime proof.
See [machine/typed mutations](../tests/artifact_checker/allocated_scalars.rs)
and [Cell/nested-view mutations](../tests/artifact_checker/bounded_scalars.rs)
and [integer-model VM cases](../tests/allocated_runtime/scalars.rs).
Contextual nonnegative i32 literals retain a typed local even though their
constant encoding is U32; array/vector index reads restore the signed value.

Helpers using Cell coordinates, nested views or allocated-value operations, and
helpers receiving readonly collection handles, bind ordinary `bool`, `u8`,
`u16`, `u32`, `i32` and `u64` parameter slots to their exact incoming ABI words.
The independent prefix interpreter counts expanded pointer/extent pairs, follows
register and caller-stack spills and rejects source substitution, including
constant-zero stores. The [parameter mutations](../tests/artifact_checker/allocated_parameters.rs)
exercise mixed owner/Hash/scalar signatures and a corruption that changes VM
behavior. Scalar entry binding does not make those slots immutable and does not
prove caller-side scalar computation or preservation across subsequent writes.
The [bounded-helper parameter cases](../tests/artifact_checker/bounded_parameters.rs)
extend the same checks to coordinates, complete nested values and derived views,
including helpers with scalar-only signatures that create bounded values locally.

For calls to those helpers, scalar argument preparation is independently bound
to the evaluated typed local or literal and the expanded ABI word, including
outgoing stack stores. Mandatory preparation/invocation markers are located in
the same typed block; their decoded instructions and complete inventory are
checked. The existing path/memory-effect analysis carries each prepared argument
to its consuming call, invalidates overwritten facts, and consumes the facts
after invocation. Missing preparation cannot be replaced with an unchecked
register value. [Call mutations](../tests/artifact_checker/allocated_calls.rs)
cover source substitution that changes VM execution, erased evidence, register
and stack arguments, forwarding and skipped preparation. This establishes
evaluated scalar transport; it does not certify earlier scalar/aggregate
computation or full source-to-machine control-flow equivalence.
[Cell/nested call mutations](../tests/artifact_checker/bounded_calls.rs) apply the
same obligations to arena-free helpers, including forwarding a value returned by
another helper and overwriting a prepared argument before invocation.
The enclosing write proof derives its outgoing stack allowance from all typed
helper calls in that body, including ordinary array/scalar helpers beside bounded
ones; writes below that finite ABI-derived allowance still reject.

Scalar results from these helpers are bound to the caller's typed destination,
including helpers with no scalar parameters. The checker starts immediately
after the resolved call and recognizes outgoing-stack restoration followed by
the complete `a0` store. The destination must have the callee's scalar type and
fit inside the caller's private frame. Explicit incoming edges cannot enter
partway through reception. [Result mutations](../tests/artifact_checker/bounded_results.rs)
cover bool/u8/u16/u32/i32/u64, calls with register and stack arguments, zeroed
results with changed VM outcomes, destination substitution and incorrect stack
restoration at O0/O3. This is call-result reception evidence; callee computation
and preservation across arbitrary later writes remain separate obligations.
The callee-side scalar return check now binds bool/u8/u16/u32/i32/u64 return
operands to `a0`, requires the return on each successful source-block path and
preserves its value through the shared frame restore. Lock predicates retain
their explicit failure semantics. The same result-mutation suite covers zeroed
and substituted return sources, erased return markers and epilogue clobbers;
zeroing a collection helper's result changes the actual VM outcome and rejects.
This closes scalar return transport, not arbitrary upstream computation or
aggregate-return provenance.
Direct collection results produced and returned in the same typed block now
also bind `u128` and `[u8; 1..8]` transport. The checker derives the copied
pointer slot from the typed producer, checks every little-endian byte load and
register combination, and preserves `a0`/`a1` through the shared epilogue.
This additional boundary does not cover general aggregate producers, alias
chains or tuple returns.
Scalar-only wrappers that directly call these helpers also retain distinct
source-local slots, because argument/result checks bind that layout. Repeated
reads through a wrapper must compile and execute at O0/O3, while redirected or
zeroed result stores still reject. Slot reuse remains available to other
scalar-only functions; admitting it at checked calls requires a separately
verified location map. This restriction can change wrapper frame/ELF layouts.

Owned `len`, `capacity` and `is_empty` queries now use the same complete
descriptor-check grammar as borrowed queries. Their scalar results, and ordered
Map `contains_key` and Set `contains` results, must be stored from `a0` into the exact typed destination
immediately after the independently checked producer. Parent-owned searches
inside insert/get/remove are not separate source membership queries.
[Query mutations](../tests/artifact_checker/allocated_queries.rs) cover owned
and borrowed Vec/Map/Set, small/large arena offsets and O0/O3, including changed
descriptor results, omitted query markers and substituted/zeroed result stores.
This binds local query production and publication, not whole-operation
successful-path coverage or preservation across arbitrary later writes.

The subsequent operation-path check binds complete top-level collection
primitives to their typed operation kind, owner/member and contract, then
requires every successful typed-block exit to execute them in source order.
It includes constructors, explicit member operations, all member retirements
within an owner drop and the extent check before each borrowed operation.
Scalar definitions and Cell operations share the ordering check; complete
independently checked nested/Cell bodies provide returning-syscall summaries.
[Path mutations](../tests/artifact_checker/allocated_paths.rs) rebind jumps and
all outer identities to skip an entire push, one/all member retirements in
2/32-member owners, or a borrowed query plus result. Push/query mutations also
change actual VM outcomes. This closes per-block operation bypasses; arbitrary
source-CFG/value equivalence and preservation across unrelated writes are
separate from that guarantee.

Non-fused source branches and jumps in collection, Cell and nested-view bodies
also bind their evaluated scalar conditions and typed successors. The complete
terminator inventory covers both optimized and unoptimized code; only an
independently recognized fused comparison replaces a separate branch site.
Empty fallthrough aliases retain evidence without instructions. Branch
exits from mapped blocks must still reach their typed successor, preventing
relocated empty-span labels from disguising a changed loop backedge. Branch
preparation participates in source operation order while its decoded successors
remain visible to path analysis. The path mutations above additionally cover
predicate inversion, condition substitution, redirected targets and erased
branch evidence, including changed VM outcomes for conditional collection
execution. This does not prove arbitrary upstream value provenance.

Fixed-value result publication extends this same contract to `u128`, fixed byte
arrays (including short arrays) and ordinary records. The checker derives the
width from typed layouts, recognizes every copied byte and binds the published
pointer to the result's typed slot. Destination storage must remain inside the
private frame, outside scalar slots and collection arena/scratch regions, and
disjoint from other copied collection results. Borrowed extent checks have no
copy scratch and do not introduce a fictitious protected buffer. The
[wide-result mutations](../tests/artifact_checker/allocated_wide_transport.rs)
cover 1/8/16/17/32/64-byte results, small/large arenas and O0/O3, including first
and last bytes, pointer stores, erased evidence, forged widths and destination
ranges. Destination substitutions rebind both emitted stores and label evidence;
the complete transfer also checks the distinct Map key/value result slices.
The completed publication exports its exact byte-buffer and pointer-slot regions
to the enclosing memory-effect analysis. Only that independently recognized
publication may write them; subsequent ordinary stores, calls and syscalls must
preserve both. This protects the immutable collection-read temporary, while
mutable source bindings use their own copied storage. The
[copy-lifetime regressions](../tests/artifact_checker/allocated_copy_lifetime.rs)
redirect an unrelated helper result into the copy's first/last bytes or pointer
slot after rebinding all artifact identities. They cover owned and borrowed
reads, 1/8/9/16/32/64-byte values, small/large arenas and O0/O3, including actual
VM outcome changes. Mutable-record updates, snapshots, helper returns and owner
drop remain positive cases. These checks do not prove arbitrary earlier
computation, all unrelated frame values or whole-language source equivalence.

These checks establish write isolation, typed owner availability and machine
initialization/allocation/resize/release, descriptor transitions, owned element
copies/writes, ordered search/update and fixed-count sorted construction.
The subsequent transport and path checks above also bind readonly helper
arguments/results and require every declared member retirement on successful
typed-block paths. These are specific allocation/lifecycle claims; they do not
prove arbitrary upstream computation or whole-language source equivalence.
Final profile admission, the remaining resource-scope audit and integration
gates remain pending. The array/stack comparison and finite application resource ceilings
above cover this transfer fixture. Fixed Cell encoder parity is
established for this transfer and the flat profile; aggregate Cell encoding is
outside that profile.
The accepted note count does not itself prove a worst-case bound for the runtime
group-count helper. Allocation and Cell-data proof-admission blockers remain.

Focused checks include `cargo test --locked -p cellscript --test fixed_cell_builder`,
the existing `nested_builder` regression suite, `cargo test --locked -p cellscript --test selection_bundle
allocated_multi_asset`, `cargo test --locked -p cellscript --test artifact_checker
allocated_writes`, the complete `allocated_runtime` suite and the
`multi_asset_transfer` LSP/WASM tests. The full checker suite is required when
changing the shared memory-effect proof.

Read-only helper results copy their values: byte arrays of 1–8 bytes retain the
single-word ABI, including arguments read from collection storage or schema
fields; Hash, Address and arrays of 9–64 bytes use caller-owned result buffers.
The fixed-result checker derives buffered widths and rejects missing copies,
incorrect destination placement and changed copy instructions after outer
hashes are rebound. Scalar byte packing is exercised at register and outgoing
stack boundaries. Allocation machine/ownership proof remains a separate gate.

Formatter, symbol navigation/rename/hover, declaration snippets, generated docs
and WASM diagnostics describe this experimental typing boundary. No executable
allocation metadata or public interface entry is inferred from an unused
declaration. Executed operations emit a structured `cellscript-allocated-values-v1`
contract containing owner/member identity, types and named budgets. In addition
to shape/declaration consistency and the mandatory blocker, the checker now
validates the allocation, descriptor, element, helper, write-isolation and
operation-path machine contracts described above, with rebound-hash mutations.
The locked-package tests cover imported fixed-value generics and readonly helper
composition; the complete transfer binds native/generated flat Cell encoding.
Exported owner declarations and aggregate Cell encoding are separate extensions.
Stable admission still requires the final scope audit, gates and recorded review
disposition. The source runtime and checker suites are
`cargo test --locked -p cellscript --test allocated_runtime` and
`cargo test --locked -p cellscript --test artifact_checker allocated_values`.
Run `cargo test --locked -p cellscript --test owned_collections`,
`cargo test --locked -p cellscript --lib named_collections`,
`cargo test --locked -p cellscript --lib named_runtime_snippets`, and
`cargo test --locked -p cellscript-wasm --features wasm --lib wasm_named_collections`.

### Experimental allocator and vector machine primitives

The private [arena emitter](../src/codegen/value_arena.rs),
[vector emitter](../src/codegen/value_arena/vector.rs) and
[ordered collection emitter](../src/codegen/value_arena/vector/ordered.rs) begin #47's executable
implementation and now serve experimental local source operations through typed
IR. Independent allocation machine admission and the complete Map/Set business
acceptance contract remain pending. The existing stack-backed Vec
and fail-closed allocation helper surface are unchanged. These internal layouts
do not freeze the final public authoring API.

One invocation reserves an explicit frame-backed arena shared by its allocated
collections. This adds no allocation syscall or writable ELF segment. The
current primitive accepts 64–65536 charged bytes in multiples of 16 and 1–256
allocation events. Owner state uses 32 additional bytes; the entire region must
fit within a frame of at most 1 MiB. A positive allocation consumes a 32-byte
record plus its requested payload rounded up to 16 bytes. The entire rounded
payload is zeroed. Zero-byte allocation returns null without consuming an event.
These are primitive admission limits; they are not measured final resource
ceilings for complete applications.

The arena is monotonic within its lifetime. Resize allocates a fresh span,
copies the smaller requested length, leaves additional capacity zeroed, and
retires the old span. Resize to zero retires without allocating. Release also
retires; neither operation refunds bytes or events. Exhaustion and invalid live
allocation pointers terminate with `CollectionBoundsInvalid` (21). Failure to
allocate a replacement preserves the old live allocation and payload; the
owner's private resize scratch may change. Pointer lookup walks allocation
records rather than accepting an aligned interior address. Retired pointers
cannot be resized or released again. Source analysis must still prevent stale
element reads, pointer escape, owner duplication and arena reinitialization.
Primitive pointer checks alone do not prove those properties.

The vector descriptor occupies 32 disjoint frame bytes: backing pointer,
initialized length, element capacity and live owner flag. Internal fixed byte
widths are 1–64 and element limits 1–1024, with the maximum requested payload
required to fit the arena. Source declarations accept the fixed subset above;
runtime admission remains pending, and an internal width does not permit
resource, reference or nested owned elements.
The primitive implements reserve, push, copied reads, replacement, stable-order
insertion/removal and drop. Read/write copy buffers must be disjoint from its
descriptor and arena. No element pointer is returned. Empty construction
allocates nothing; reserve grows to the greater of the requested minimum and
twice the old capacity, clipped to the declared element limit. Removal retains
capacity and zeroes the vacated trailing element. Drop retires backing storage;
subsequent operations on the descriptor fail.

Direct Vec indexing and ordered binary search describe only element access.
Backing-allocation validation walks arena records, including retired growth
records on the path to the live buffer, so whole-operation cost also depends on
allocation history. Preallocation reduces that history as well as copying and
charged bytes. Reusing validation across operations would need a separate proof
that owner, extent and lifetime facts remain valid across intervening calls or
mutation; the current cost observations do not justify removing those checks.

Growth history is part of the declared byte budget. For example, gradually
growing 1024 elements of 32 bytes requires more than a 65536-byte arena because
old buffers and record headers remain charged. Reserving 1024 elements before
insertion fits. Tests require both the gradual-growth failure and pre-reserved
success; a byte budget cannot be described as payload capacity alone.

The independent [logical reference model](../tests/support/value_arena_reference.rs)
is compared with real CKB-VM memory, including live and retired records, payload
bytes, initialized padding, descriptors and surrounding guards. Focused cases
cover mixed shared collections, large frame offsets, generated operation
sequences, boundary widths, maximum counts, invalid/stale pointers, poisoned
state/header fields, overflow, exhaustion, stable order and dropped owners.
Run `cargo test --locked -p cellscript --lib value_arena -- --nocapture`.
Cycle/ELF observations include the test harness and its initial dirty-memory
fill; they are not frozen application budgets or source/checker acceptance.

Optimization work keeps the contiguous representation as its baseline. Compare
8, 32, 128 and 1024 entries with and without one initial reservation, ascending
and descending distinct keys, and a second pass of duplicate keys. Measure full
operation cycles, ELF bytes, charged arena bytes and allocation events; binary
search complexity alone excludes descriptor/allocation validation and shifting.
The primitive cost matrix checks the resulting memory against the independent
model, including that duplicate updates allocate nothing. Reproduce it with
`cargo test --locked -p cellscript --lib ordered_reservation_cost_matrix -- --nocapture`.

The experimental `load_sorted` comparison above covers already sorted fixed-array
input, retaining the same key order, overflow/exhaustion behavior and copied-value
semantics. Arbitrary-order batch construction remains pending. Its Map duplicate
policy must explicitly define whether values replace, reject or aggregate;
sorting cannot silently choose a business rule.
Reusable validation requires proof that no intervening mutation invalidates its
facts. Neither a new data structure nor removal of checks is justified solely by
asymptotic notation. Application-level budgets, maximum-success/latest-failure
cases and independent checking remain separate acceptance requirements.

Ordered Map/Set use the same vector backing with unique sorted keys. The
primitive accepts unsigned little-endian keys of 1/2/4/8 bytes, compared by
numeric value, or 1–32 fixed bytes compared lexicographically. A map adds
1–32 fixed value bytes; zero value bytes select set behavior. Binary search
returns a matching entry or insertion ordinal. Map insertion replaces the
value for an existing key, including at full capacity; duplicate set insertion
is a no-op. Lookup copies a complete entry when present, otherwise leaves its
output buffer unchanged. Missing-key removal is a no-op. Ordinal reads expose
sorted iteration, not insertion order. Key equality has no hash/collision path.
Sorted uniqueness depends on using these primitives exclusively. Experimental
source operations expose copied reads and reject raw vector writes or key mutation;
complete independent source/runtime proof and canonical serialization admission
remain pending.

Search compares at most the bounded binary-search path times the key width.
Insertion/removal can shift the entire suffix, so building in reverse key order
has quadratic total movement. The CKB-VM fixtures compare contents and sorted
iteration with Rust `BTreeMap`, including byte-order boundaries, common-prefix
keys, duplicate replacement, full-capacity failures and 1024-entry batches.
Observed costs on 2026-10-23 (pre-reserved capacity, two terminal ordinal reads,
65536-byte arena, no transaction/syscall workload) are:

| 1024-entry insertion fixture | Cycles | ELF bytes |
|---|---:|---:|
| 8-byte unsigned set, ascending | 2,420,339 | 4,112 |
| 8-byte unsigned set, descending | 52,890,090 | 4,112 |
| 8-byte unsigned key + 8-byte value, ascending | 2,584,395 | 4,472 |
| 8-byte unsigned key + 8-byte value, descending | 103,336,642 | 4,472 |

These expose the movement cost; they are primitive observations, not final
acceptance ceilings. The transfer fixture above adds complete aggregation and
deduplication, an array comparison and finite application ceilings. Complete
typed ownership, source/checker binding and adversarial resource acceptance
remain required before admission.

### Maximum sorted-construction resource evidence

`tests/fixtures/sorted_collection_resources.json` freezes eight O3 observations
for `OrderedMap<u16, u8>::load_sorted` with 1024 input pairs, an 8192-byte arena
and two allocation events. The original O0/O3 outcome test compares cardinality
and the aggregate value sum against `BTreeMap`, including last-value replacement
for duplicate keys; it does not compare every emitted key/value pair.
The resource replay uses the same completed transaction, with an independently
stepped Type Script group checked against the ordinary group verifier's exit
and cycle count. It also exercises preallocation of a separate 636-element
`Vec<u64>` before construction, without changing the shared budget.

| Input pattern | Without preallocation: exit / Type-group cycles | With preallocation: exit / Type-group cycles |
|---|---:|---:|
| 1024 ascending unique keys | 0 / 1,028,773 | 21 / 265,174 |
| 1024 copies of one key | 0 / 271,285 | 0 / 440,214 |
| Final key less than the previous key | 21 / 96,373 | 21 / 265,074 |
| Final key equal to the previous key | 0 / 1,028,131 | 21 / 265,180 |

Frozen corpus ceilings are 1,028,773 Type-group cycles, 15,168 observed stack
bytes, 32,768 touched guest-page bytes, 8704 ELF bytes, 3456 transaction bytes
and 3116 serialized witness bytes. Stack is measured from entry SP after argv
initialization; guest pages include ELF, argv and syscall buffers. Separate heap
accounting is unavailable, and the declared arena size is not reported as an
observed heap peak. Rejected transactions have no aggregate cycle total in this
harness; the table reports the independently observed Type-group cost instead.

The test checks all eight case keys, exact source/ELF/transaction identities,
observations and finite ceilings. Reproduce with
`cargo test --locked -p cellscript --test allocated_runtime allocated_sorted_maximum_and_late_invalid_inputs_match_btree -- --exact`.
This adds maximum-size sorted-construction evidence without relaxing existing
transfer/array budgets or claiming a global worst-case bound for every operation.

## Completion ledger

Each row is required for its corresponding issue, not an optional follow-up.
Scalar definition binding, per-block successful-path coverage and scalar
helper entry/call transport now apply across all three issues, including
arena-free Cell and nested-view helpers.
Scalar call-result reception also binds the typed destination and outgoing
stack restoration across these helpers.
This shared progress does not discharge the complete source/value obligations
listed below.

Read this ledger against the issues' explicit acceptance criteria. Module-local
declarations are the implemented interface boundary; exporting `cell_roles`,
`data_shape` or `collections` declarations is a separate extension, not an
implicit prerequisite for testing imported ordinary values and checked helpers.
Likewise, source/value proof obligations below concern the operations, sources,
bounds, lifetimes and authority advertised by the admitted profile. They do not
require proving arbitrary whole-language source equivalence. This distinction
does not waive any advertised machine claim, permit removal of an unproved
admission blocker, or replace final gates and a recorded review disposition.

The current scope audit is recorded in the [admission review](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md).
The rows below are candidate coverage, subject to the final gates on one source
revision; earlier progress paragraphs are historical evidence increments.

| Obligation | #49 | #48 | #47 |
|---|---|---|---|
| Authoring API and complete programs | Named catalogs; fixed settlement and byte/record migration | Named shapes and derived read-only helpers; byte/record migration | Shared collections, explicit actual-count reservation and read-only helpers; transfer/array comparison |
| Source/type/ownership restrictions | Immutable catalogs and source coordinates; foreign observation grants no lifecycle authority | Bounded immutable witness/Cell views; no recursive, escaping or owned dynamic values | Affine owners, copied reads, lexical loans, shared cumulative budgets and stale/escape rejection |
| Runtime and hostile boundaries | Complete scans, correspondence and all-group/hostile-peer transactions | Canonical witness/Cell decoding, actual successors, late-invalid and maximum values | Arena/vector/map/set operations, shared exhaustion, sorted batches and hostile spending Locks |
| Independent checker | Nominal source/authority, selector/read/property instructions, helper transport, retained buffers and CFG lifetime | Decoder/read/output instructions, source/length transport, mandatory comparison and buffer lifetime | Allocation/descriptor/element/sorted instructions, owner CFG, argument/result transport, operation order, arena isolation and retained copies |
| Reference and builder parity | Independent host selection model; selected-entry v2 reconstruction; unbound conditional sites reject | Literal codec vectors, independent fixture reader, native/SDK encoding and complete migration bytes | Arena/BTreeMap model, transfer host oracle, native/SDK flat Note encoding and packed transactions |
| Editor/browser/package parity | Named navigation/rename, syntax seeds, browser summary and locked-package helpers | Named limits, helper views, formatter/diagnostics and locked-package helpers | Budget reports, formatter/diagnostics, browser summary and imported fixed-value generic copies |
| Resources | 35 combined selection/migration observations, unchanged finite ceilings | Same selection/migration corpus; unavailable metrics remain explicit | 24 transfer/array observations and eight maximum sorted-construction observations, unchanged finite ceilings |
| Strict executable-surface candidate | O0/O3 complete programs and erased-proof rejection | O0/O3 complete programs and erased-proof rejection | O0/O3 complete program and erased-proof rejection |
| Applicable dev/backend/ci gates | deferred for alpha by maintainer; not passed | same explicit exception | same explicit exception |
| Review disposition | implementation-author scope audit; no independent human review claimed | same shared disposition | same shared disposition |

No issue will close based solely on host decoding, a design record, a compile
success, a metadata field, or a happy-path VM example.
