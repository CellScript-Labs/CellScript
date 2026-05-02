# CellScript Collections Support Matrix

**Status**: production boundary document for CellScript 0.13.

CellScript supports dynamic data in several different layers. These layers must
not be collapsed into one generic "collections are supported" claim.

## Support By Layer

| Feature | Schema/ABI | IR construction | Runtime verifier helper | Production status |
|---|---:|---:|---:|---|
| `Vec<u8>` | Yes | Targeted | Targeted create/update-output verification | Supported for documented witness and cell-data paths |
| `String` | Yes | Targeted | Byte-vector verification | Supported as UTF-8 bytes at the schema boundary |
| `Vec<Address>` | Yes | Targeted | Fixed-element vector verification | Supported where metadata marks a Molecule dynamic field |
| `Vec<Hash>` | Yes | Targeted | Fixed-element vector verification | Supported where metadata marks a Molecule dynamic field |
| Fixed byte arrays | Yes | Yes | Exact-size verification | Supported |
| Stack-backed local `Vec<T: FixedWidth>` | Local-only | Yes | Bounded helper lowering | Supported for verifier-local scalar, fixed-byte, and fixed-width named values |
| `Vec<Vec<u8>>` | Boundary | Boundary | No generic helper | Must fail closed unless a concrete lowering is added |
| `HashMap<u64, u64>` | Limited | Limited | No production helper surface | Experimental/internal; not a production contract |
| `HashMap<Hash, Token>` | No | No | No | Unsupported; must fail closed |
| Cell-backed resource collections | No executable ownership model | No | No | Unsupported until a linear collection ownership primitive exists |

## Stack-Backed Local Vec Rule

0.13 supports bounded local `Vec<T>` helpers only when `T` has a known fixed
width and the vector is verifier-local. The supported helper surface is:

```text
new, with_capacity, capacity, push, extend_from_slice, len, is_empty,
indexing, first, last, contains, set, remove, pop, insert, reverse, truncate,
swap, clear
```

`Vec::capacity()` reports the fixed stack backing capacity
(`256 / element_width`), not the requested `Vec::with_capacity(n)` argument.
`cellc explain-generics` exposes each checked instantiation, including element
type, element width, backing model, helper set, and constructor provenance.

`examples/language/registry.cell`, its top-level compatibility mirror
`examples/registry.cell`, and `examples/language/order_book.cell` are
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

## Authoring Guidance

Use dynamic vectors for data that is still a single cell field, such as signer
lists, proposal payload bytes, NFT attributes, or launch distributions.

Do not model ownership of multiple independent linear cells as a generic vector
or map. Use explicit action parameters, named output bindings, and explicit
`consume`, `transfer`, or `destroy` operations until the language gains a verifier-backed
collection ownership primitive.

The missing verifier pieces are explicit cell consumption, typed collection
destructuring, and membership proofs tied to Molecule schema manifests. Until
those pieces exist, generic cell-backed collections stay outside the production
surface.
