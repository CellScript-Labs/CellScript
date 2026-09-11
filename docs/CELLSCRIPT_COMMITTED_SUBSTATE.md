# Fixed-width typed committed substate

Status: implemented candidate profile for the `0.30` development branch. This
document does not claim release acceptance or independent review.

## Contract

`Commitment<T>` is a nominal 32-byte CKB Blake2b-256 digest. `Opening<T>` is an
opaque, one-shot witness value containing the exact canonical packed bytes of
`T`. The accepted Phase 1 surface is:

```cellscript
let commitment: Commitment<State> = commitment::commit(state)
let state: State = commitment::open(commitment, opening)
```

The hash preimage is exactly:

```text
"CellScriptPackedHashV0\0"
|| UTF-8 canonical source type name
|| 0x00
|| packed_width_u32_le
|| exact packed value bytes
```

The hash algorithm is CKB Blake2b-256, including CKB's
`ckb-default-hash` personalization. The type name is part of the preimage, so
equal bytes of two different nominal types do not share commitment identity.

## Opening safety

An `Opening<T>`:

- must be a direct `witness` parameter of an action or lock;
- is linear and must be consumed exactly once;
- cannot be observed, compared, copied, aliased, returned, stored in a schema,
  nested in another value, or passed to an ordinary function;
- may appear only as the direct second argument of `commitment::open`;
- must match the first argument's exact `Commitment<T>` type.

The entry adapter first decodes one bounded byte segment and preserves its
pointer and exact length. The generated verifier constructs the domain-separated
preimage, hashes it, compares all 32 digest bytes, and only then materializes the
bytes as `T`. A mismatch exits with stable runtime error 73
(`commitment-opening-mismatch`).

## Accepted value profile

`T` must be an owned, non-Cell, non-reference, non-wrapper value with one
concrete packed width. Scalars, fixed arrays, fixed tuples, fixed structs, and
fixed payload enums are eligible when their complete layout is fixed. Unit,
`String`, `Vec`, bounded dynamic collections, Cell-backed resource/shared/receipt
values, recursive layouts, nested `Commitment`/`Opening`, and dynamic values are
rejected.

The complete preimage plus a 32-byte comparison digest must fit the backend's
512-byte scratch region. This is a compile-time admission check, not a runtime
best effort.

## ABI and builder encoding

For a callable parameter `expected: Commitment<T>`, the entry ABI consumes
exactly 32 raw bytes. For `opening: Opening<T>`, it consumes a little-endian
`u32` byte length followed by the exact packed bytes of `T`; the verifier then
requires that length to equal the compile-time width of `T`. The Rust metadata
encoder and `cellc entry-witness` accept these values as exact hex/byte
arguments. `Opening<T>` is deliberately length-delimited even though `T` is
fixed width so malformed or truncated witnesses fail before becoming typed.

When a successor Cell stores `Commitment<State>`, the ordinary output
correspondence rules bind its 32-byte field to the single value returned by
`commitment::commit(next_state)`. Builders must encode that evaluated digest,
not recompute an independently supplied successor. This reuses the existing
single-evaluation IR value and checked output-field materialization path.

## Evidence and remaining scope

The checked evidence consists of type-system misuse tests, formatter/LSP
round-trips, generated-assembly ordering tests, dedicated checked-runtime
ProofPlan records, host hash vectors, and real CKB-VM acceptance/rejection tests
in `tests/commitment_opening.rs`, with vectors in
`tests/fixtures/committed_substate_vectors.json`. The standalone artifact
checker rejects rebound typed-call records whose `Commitment<T>`, `Opening<T>`,
or result types no longer agree. It also binds the inline machine blocks to the
typed operation order, exact domain/type/width header, bounded scratch layout,
opening size guard, copy/hash/compare arguments and status paths,
compare-before-materialize boundary, mismatch error 73, and successor digest
destination. A Commitment read through a typed schema field is bound to the
exact root stack slot and accumulated fixed-layout field offset; an offset
mutation is rejected before the opening can authenticate. Independent mutations
cover removed or reordered markers and each security-relevant instruction
class. The exact maximum fixed-width shape uses
a one-byte type name and 451 packed value bytes, filling the 512-byte
preimage-plus-digest budget; its cycles, ELF, maximum stack frame, witness,
transaction, and dependency bytes are frozen in
`tests/fixtures/committed_substate_resource_budget.json`.
The separate `committed_substate_scenarios.json` fixture freezes the artifact,
lowering-record, source-map, verified-bundle, raw-transaction, and serialized-
transaction identities for the successor positive case and the stale,
malformed, wrong-root, wrong-index, and wrong-successor adversarial cases. With
the maximum-shape authenticated-opening fixture, this covers every named
committed-state business-inventory row.

Dynamic openings, selectable hash algorithms/domains, recursive or variable
layouts, zero-knowledge proof objects, and general-purpose witness codecs are
outside this Phase 1 profile. Independent cryptographic/compiler review and
release acceptance remain gates before the issue can close.
