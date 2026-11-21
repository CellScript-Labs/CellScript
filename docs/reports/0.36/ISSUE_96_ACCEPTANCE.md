# Issue #96: structured executable type identity

Status: implementation and validation in progress on `0.36`, 2026-11-21.
This record concerns [issue #96](https://github.com/CellScript-Labs/CellScript/issues/96)
and the [structured identity contract](../../CELLSCRIPT_STRUCTURED_TYPE_IDENTITY_RFC.md).
It does not publish a release or establish production equivalence.

## Scope and representation inventory

The issue's original examples predate the current implementation. Numeric
arguments already use `TypeArgument::UnsignedConstant(u128)`, and generated
specialization labels have no semantic decoder. Edition 2027 authoring-v3 is
now the sole source contract; the original request to characterize two editions
does not reintroduce retired syntax.

| Boundary | Retained representation and owner |
| --- | --- |
| Source applications | `ast::TypeApplication` and `TypeArgument` retain type nodes and unsigned constants; the parser owns syntax |
| Declaration resolution | `resolve::ResolvedTypeIdentity` retains defining module/name, template and ordered resolved arguments |
| Specialization | `ast::SpecializationOrigin` transports template ownership and arguments through imports and instantiation |
| Nominal IR | `IrModule::nominal_identities` supplies semantic lookup; labels and field layouts supply no substitute identity |
| Nested records | `nested_values::RecordRef` retains resolved nominal identity and admitted layout, separately from source spelling |
| Context, bounded and selection families | Structured AST arguments lower to admitted IR family variants; family-specific authority and bounds remain separate |
| Commitment/opening and handles | Distinct IR nodes and closed handle arguments retain their existing ABI and admission rules |
| Native interfaces and bundles | Required nominal catalogs supply independently reconstructed alias and specialization evidence |
| Metadata, editor and browser readers | Text is admitted at explicit source/wire boundaries; browser output remains a bounded metadata summary |
| Standalone checker | Its own catalog resolution, nested compatibility, typed operations and machine checks admit complete bundles |

`cellscript-tools::repository_checks` enforces the existing specialization,
context, bounded, commitment and runtime-view boundaries. Its mutation tests
reject encoded-label inference, family text reconstruction and generic parsing
in executable IR/codegen. Metadata/editor readers retain their explicit text
contracts. `ExactCellSelection` and unsupported open nested projections remain
reserved or fail closed; this work adds no executable family or ABI.

## Concrete correction

Previously `Envelope<First>` and `Envelope<Alias>` could denote the same
template with the same phantom declaration argument, yet a nested record view
compared the generated specialization names and rejected the helper call.
Record descriptions now compare the retained template/argument identity and
admitted layout. An ambiguous retained binding cannot fall back to a different
resolver result. Descriptions remain scoped to one immutable compilation or
document revision.

A second regression used `Envelope<Value>` in a helper and
`Envelope<nested_qualified_arguments::Value>` in its caller. After correcting
source equality, default ELF export lacked the declaration evidence needed by
the independent checker (`V2419`). Default evidence selection now detects
duplicate resolved specialization identities among emitted declaration
bindings. Ordinary qualified lookup aliases alone do not force a catalog.
The checker was not relaxed and does not consume a producer admission flag.

## Compatibility decision

Cache generation advances from v203 to
`project-source-set-v204-nested-nominal-identity`. Previously rejected valid
source now compiles; default products for equivalent specialization spellings
may include the existing nominal catalog. These are intentional semantic and
evidence corrections, not a claim of identical metadata for every source.

Cargo remains `0.33.0-alpha.2`, Rust remains 1.97.1, and Edition 2027
authoring-v3, versioned wire shapes, metadata/hash renderers, symbol encoding,
resource ceilings and established V0 commitment domains remain unchanged.
The patch changes no code generator or assembler emission. Existing alias
interface/ELF controls and packed-domain vectors remain acceptance checks;
newly admitted programs have positive execution evidence rather than a
previous successful ELF to compare.

## Validation

The new source regressions exercise imported aliases and qualified phantom
arguments through metadata and ELF compilation, with optimizations on/off and
optional source contracts on/off. Equal-layout different-owner source rejects
before artifact construction. Positive nested payloads execute in CKB-VM.

The new complete-bundle mutation regression substitutes an index result from a
different argument owner, a different template owner, or a different payload
bound, and separately removes required catalog evidence. It recomputes the
layered outer hashes and requires `V2419` for each mutation in both optimization
modes. Editor revision and browser diagnostic tests share the qualified source
fixture and reject a same-layout distinct phantom argument.

Canonical browser evidence:

- `CELLSCRIPT_WASM_DOCKER_NETWORK=host website/scripts/build-wasm.sh`: passed
  with the pinned Docker/toolchain build. The initial default-network attempt
  could not reach the configured loopback download proxy; host networking
  resolved that environment failure without changing the build contract.
- WASM: 1,962,942 raw bytes, 743,376 gzip bytes, below the 1,536,000-byte budget;
  SHA-256 `df177d74ec5b860e92d116ae876368743fcb318cc7a21c233584a76df157829b`.
- `npm --prefix website run test:playground-compiler`: 53 passed, including the
  shared fixture through single-source, diagnostic and source-set entrypoints.
- Signed website consumer commit:
  [d53cd56030e22781904fdfc7c2d61592bfa4af24](https://github.com/CellScript-Labs/cellscript-website/commit/d53cd56030e22781904fdfc7c2d61592bfa4af24).

Focused native evidence:

- `cargo test --locked -p cellscript --test structured_nominal_types --test
  generic_identities --test structured_nested_types --test
  structured_bounded_types --test structured_context_views --test
  structured_committed_types --test structured_witness_views --
  --test-threads=1`: 47 passed, one existing generator test ignored. Existing
  ELF/interface/typed identity characterizations and V0 domain controls pass.
- `cargo test --locked -p cellscript --test artifact_checker
  nested_specializations -- --nocapture`: the new complete-bundle mutation and
  VM regression passed in both optimization modes.
- `./scripts/cellscript_gate.sh commit`: passed, including affected native and
  WASM host checks, source policy, doc status and local links.

Final integration gate results will be recorded after execution.
