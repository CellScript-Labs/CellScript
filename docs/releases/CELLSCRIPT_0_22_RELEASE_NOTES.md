# CellScript 0.22 Release Notes

**Status**: Stable release notes for CellScript 0.22.0.

**Updated**: 2026-07-21.

CellScript 0.22 extends the typed language and evidence model while tightening
the CLI, diagnostics, LSP, and bounded Fiber interoperability path. The release
keeps CKB's Cell model and evidence boundaries explicit: compiler success is
not a production or live-chain claim.

## CLI And Diagnostics

`--json` is the single public machine-output switch. Success and failure emit
exactly one JSON document on stdout; the hidden `--message-format=json` spelling
remains only for compatibility. Public help and `cellc --list` show the
canonical nested command tree.

Backend diagnostics use stable `E2xxx` codes. JSON diagnostics include code,
name, description, and recovery hint; `cellc explain E2202 --json` exposes the
same registry. LSP diagnostics carry the standard `code` field and a
`codeDescription` link. VM and simulator `cellc run --json` results now share
one schema, with unavailable `cycles` or `steps` represented as `null`.

## Typed 0.22 Language Surface

The 0.22 line adds:

- checked casts, transitive callable effects, initial/terminal flow evidence,
  typed aggregate targets, and a six-tier ProofPlan evidence taxonomy;
- typed read-only transaction handles such as `InputView<T>`, `OutputView<T>`,
  `CellDepView`, `HeaderDepView`, and `WitnessArgsView`;
- finite `forall` and `count(...)` invariant quantification over closed source
  views;
- source-aware `BoundedCellSet<T, N>` and witness/static
  `BoundedList<T, N>` contracts with `consume_each` and `create_each`;
- a closed capability algebra with explicit entailment evidence;
- concrete fixed-width payload enums with exhaustive matching and bounded ABI
  support;
- canonical type `validity` blocks, including the explicit
  builder-evidence boundary for `env::block_number()`;
- compile-time-only `borrow root as view { ... }` regions;
- deterministic participant-role attribution in ProtocolGraph metadata.

Unsupported dynamic, recursive, generic, unbounded, or authority-ambiguous
forms remain rejected or explicitly recorded as deferred/runtime-required.

## Metadata Schema 55

Current artifacts emit compile metadata schema 55. It carries the 0.22
callable/flow evidence, transaction-view handles, bounded collections,
capability proofs, enum layouts, protocol-role candidates, validity predicates,
borrow regions, and the bounded Fiber compatibility contract. Source, artifact,
and constraints sub-schemas retain their own version fields.

Consumers must compare the emitted version rather than assuming that an older
schema number remains current. Historical release documents may still name the
schema in which a field first appeared.

## Bounded Fiber Interoperability

The separate `cellscript-fiber-adapter` crate derives a dedicated
`fungible-type-group-v1` artifact and native Fiber UDT configuration from typed
compiler, deployment, live-cell, and node evidence. It does not add a Fiber
target profile or depend on `fiber-lib`.

The path is deliberately narrow: exact 16-byte little-endian `u128` Cell data,
full Type Script group conservation, and closed issuance/destruction authority
formats. Local-devnet scenarios cover bounded multi-hop payment and watchtower
force-close flows. The clean pinned full lifecycle/negative matrix remains
pending, so 0.22 does not claim production-ready Fiber interoperability.

See `examples/fiber/README.md` for the operator workflow and examples.

## Editor And Agent Tooling

The VS Code extension is maintained as the
`editors/vscode-cellscript` submodule. The 0.22 extension release adds grammar
and snippet coverage for the new language forms while continuing to delegate
semantic diagnostics and reports to `cellc`.

`cellscript-mcp` adds bounded 0.22 language, Fiber interoperability, and roadmap
documentation topics. MCP remains read-only and does not become a second
compiler, builder, or deployment client.

## Deferred And External Evidence

The following remain outside a compiler-only 0.22 claim:

- production Fiber readiness without the pinned external lifecycle matrix;
- builder, capacity, tx-pool, commit, and live-chain evidence represented by
  non-compiler ProofPlan tiers;
- dynamic/generic payload ADTs and unbounded collection iteration;
- transaction-view reads inside type validity predicates;
- consensus-checked TemplateLayout commitments and canonical AST/IR receipt
  hashes.

## Validation Boundary

Routine local work:

```bash
./scripts/cellscript_gate.sh dev
```

Merge readiness:

```bash
./scripts/cellscript_gate.sh ci
```

Backend changes:

```bash
./scripts/cellscript_gate.sh backend
```

Production-facing release claims still require the release gate and its
external CKB/Fiber evidence dependencies.
