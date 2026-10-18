# Bounded runtime foundations: Cell sets, nested data and allocated values

**Status**: implementation in progress for #49, #48 and #47. This document
is not runtime admission, stable release acceptance, or issue closure.
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
is an internal prerequisite, **not reachable from DSL lowering** yet: source
admission, metadata/checker obligations and successor checks remain pending.

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

## Cell-set contract decisions to preserve

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

## Completion ledger

Each row is required for its corresponding issue, not an optional follow-up.

| Obligation | #49 | #48 | #47 |
|---|---|---|---|
| Source/type/ownership rules and unsupported-shape rejection | pending | pending | pending |
| Runtime lowering, including maximum and late-invalid execution | pending | pending | pending |
| Metadata and independent checker binding; rebound machine mutations | pending | pending | pending |
| Independent canonical/reference vectors | pending | in progress | pending |
| Generated builder/adapter and complete transaction bytes | pending | pending | pending |
| All required groups and hostile peer cases | pending | pending | applicable caller tests pending |
| Formatter/LSP/WASM/support matrix and examples | pending | pending | pending |
| Cycles, stack/heap or unavailable metrics, ELF, bytes and capacity | pending | pending | pending |
| Applicable dev/backend/ci gates and review disposition | pending | pending | pending |

No issue will close based solely on host decoding, a design record, a compile
success, a metadata field, or a happy-path VM example.
