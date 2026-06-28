# CellScript Roadmap

**Updated**: 2026-06-26

This roadmap is the high-level planning map for CellScript. It links the
release-specific trackers and the deeper design notes so the project does not
split into unrelated TODO files.

The current project direction is simple:

1. keep the CKB Cell model visible in the language;
2. keep release claims tied to compiler evidence and builder-backed CKB
   evidence;
3. make the language surface easier to teach without hiding authorization,
   capacity, witness, or lock-group boundaries;
4. keep syntax sugar audit-visible by requiring parser, formatter, type,
   lowering, metadata, codegen, docs, and automated syntax-combination gates to
   agree before release.

## Current State

| Area | Current status | Detailed document |
|---|---|---|
| 0.13 release scope | Implementation scope is closed for the `v0.13.2` stable release; the full gate includes stateful business-flow/action coverage. | [0.13 release scope](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md), [0.13 release tracker](CELLSCRIPT_0_13_TODOLIST.md), [0.13.2 release notes](../docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md) |
| 0.14 release scope | CKB semantic-completeness scope is complete for the current stable line. | [0.14 roadmap](CELLSCRIPT_0_14_ROADMAP.md), [0.14 release notes](../docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES.md) |
| 0.15 release scope | `v0.15.0` is released from `nightly-0.15` with scoped invariants, aggregate invariant primitives, invariant/action coverage links, Covenant ProofPlan output, risk diagnostics, macro provenance, identity-aware lifecycle forms, and final release-gate evidence. | [0.15 roadmap](CELLSCRIPT_0_15_ROADMAP.md), [0.15 roadmap summary](../docs/archive/0.15/CELLSCRIPT_0_15_ROADMAP_SUMMARY.md), [0.15 release notes](../docs/releases/CELLSCRIPT_0_15_RELEASE_NOTES.md) |
| 0.16 release scope | `v0.16.1` is released for the scoped metadata-assurance line, including operational semantics, ProofPlan soundness, builder assumptions, transaction validation/solver templates, deployment governance, audit tooling, standard CKB compatibility fixtures, compiler hardening, proposal-local NovaSeal devnet/profile certification, and bundled example bootstrap cleanup. | [0.16 roadmap](CELLSCRIPT_0_16_ROADMAP.md), [0.16.1 release notes](../docs/releases/CELLSCRIPT_0_16_1_RELEASE_NOTES.md) |
| 0.17/0.18 iCKB equivalence state | The standalone 0.17 line introduced the protocol-semantics surface and partial CKB VM differential evidence; the carried-forward 0.18 work closes the manifest-declared executable iCKB claim set as `EXECUTED_CKB_VM_DIFF` / `PROVEN`. | [0.17 roadmap](../docs/archive/0.17/CELLSCRIPT_0_17_ROADMAP.md), [0.17 iCKB final report](../docs/archive/0.17/CELLSCRIPT_0_17_ICKB_FINAL_REPORT.md) |
| 0.18 planning scope | First-class read-only `ScriptRef` / `ScriptArgs` surface and the remaining iCKB equivalence-closure prerequisites. | [0.18 roadmap](../docs/CELLSCRIPT_0_18_ROADMAP.md) |
| 0.19 scope | Scope complete for CKB ecosystem reuse, `ckb-std` compatibility, grammar governance, and Phase 1 package/deployment identity registry closure. Generated builders and live-chain registry proof moved to 0.20. | [0.19 roadmap](../docs/CELLSCRIPT_0_19_ROADMAP.md), [0.16-0.20 release notes](../docs/releases/CELLSCRIPT_0_16_TO_0_20_RELEASE_NOTES.md), [CKB ecosystem reuse audit](../docs/CELLSCRIPT_CKB_ECOSYSTEM_REUSE_AUDIT.md), [ckb-std compatibility](../docs/CELLSCRIPT_CKB_STD_COMPAT.md), [Registry Phase 1](../docs/CELLSCRIPT_REGISTRY_PHASE1.md) |
| 0.20 planned scope | Generated Action Builder, live-chain deployment verification, stateful transaction flows, and registry trust hardening. | [0.20 roadmap](../docs/CELLSCRIPT_0_20_ROADMAP.md) |
| 0.21 planned scope | Semantic closure, authenticated compiler evidence, CLI UX reorganisation, dedicated MCP server and CellScript programming skills, derived cyclic graph views, type-level TemplateLayout metadata, and deferred optional template Merkleisation. | [0.21 roadmap](../docs/CELLSCRIPT_0_21_ROADMAP.md), [0.21 CLI UX plan](CELLSCRIPT_0_21_CLI_UX_PLAN.md) |
| CKB language fit | CKB-first design is confirmed; remaining gaps are signer binding, continuity policy, capacity policy, and declarative time policy. | [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md) |
| Surface syntax | Low-risk syntax pass and 0.13.2 syntax-governance hardening are implemented; authority-sensitive syntax remains staged. | [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md), [Syntax-combination audit](../docs/CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md) |
| Collections | Stack-backed fixed-width `Vec<T>` helper surface is implemented; cell-backed and generic map ownership remain fail-closed. | [Collections support matrix](../docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md), [0.13 release scope](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md) |
| CKB production evidence | Bundled actions and locks have builder-backed local CKB evidence; full release claims also require stateful coverage for every production acceptance action. | [Metadata and production gates wiki](../docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md) |
| Documentation and wiki | Wiki is version-neutral, cookbook-oriented, includes a standard-library chapter, and is published separately to GitHub Wiki. | [GitHub Wiki](https://github.com/a19q3/CellScript/wiki) |

## Release Tracks

### 0.13: Closed Implementation Scope

0.13 is a closed stable release line. Its implementation scope covers:

- executable stack-backed `Vec<T>` helper support for fixed-width values;
- low-risk surface syntax improvements and cleaner example organization;
- CKB lock-boundary classification with `protected`, `witness`, and `require`;
- 0.13.2 stdlib lifecycle/cell metadata patterns that lower to explicit
  verifier effects instead of core protocol-name magic;
- automated syntax-combination audit coverage for parser, formatter, type,
  lowering, metadata, codegen, and release-gate contracts;
- full release-gate stateful evidence: seven end-to-end business scenarios plus
  action-branch coverage for all production acceptance actions.

0.13 deliberately does not introduce hidden signer authority, hidden sighash
defaults, full generic maps, or cell-backed collection ownership.

Detailed status:

- [0.13 release scope](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md)
- [0.13 release tracker](CELLSCRIPT_0_13_TODOLIST.md)
- [0.13.2 release notes](../docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md)
- [Syntax-combination audit methodology](../docs/CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md)

### 0.14: CKB Semantic Completeness

0.14 exposes more of CKB's concrete execution surface without hiding lock/type
boundaries:

- Spawn/IPC builtins for bounded verifier reuse;
- explicit Source views, typed fixed-width lock args, and structured
  WitnessArgs field access;
- target profile metadata for witness ABI, lock args ABI, Source encoding,
  Spawn/IPC ABI, since semantics, CellDep ABI, script reference ABI,
  outputs/outputs_data ABI, capacity floor ABI, TYPE_ID ABI, and tx version;
- declarative since/time and capacity surfaces;
- fixed-Hash dynamic BLAKE2b via `hash_blake2b(input: Hash) -> Hash` with a
  real CKB-profile RISC-V helper and metadata-visible `CKB_BLAKE2B` access.

Detailed status:

- [0.14 roadmap](CELLSCRIPT_0_14_ROADMAP.md)
- [0.14 release notes](../docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES.md)

### 0.15: Scoped Invariants And Covenant ProofPlan

0.15 makes invariant scope and enforcement status visible without pretending that
metadata-only declarations are already executable CKB verifier code:

- top-level scoped `invariant` declarations with explicit `trigger`, `scope`,
  and `reads`;
- aggregate primitives for sum, conservation, delta, distinct field, and
  singleton identity relations;
- bounded invariant/action coverage links that show whether a declared
  aggregate invariant matches a checked action obligation;
- Covenant ProofPlan records for declared invariants, aggregate primitives,
  selected protocol flows, and pool protocol metadata;
- diagnostics for risky coverage assumptions such as `lock_group` verifiers that
  inspect transaction-wide views;
- macro expansion provenance for compiler-recognized protocol flows.

Detailed status:

- [0.15 roadmap](CELLSCRIPT_0_15_ROADMAP.md)

### 0.16: Formal Semantics And Production Tooling

The 0.16 line turns v0.15 audit metadata into an
assurance surface:

- operational semantics in `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md`;
- `runtime.proof_plan_soundness` and strict `--primitive-strict=0.16`
  enforcement;
- `runtime.builder_assumptions`, `cellc explain-assumptions`, and
  `cellc validate-tx`;
- template-only transaction plans, deployment plans, dependency locks, proof
  diffs, profiles, transaction traces, and audit bundles;
- standard CKB compatibility fixture manifest for sUDT, xUDT, ACP, Cheque,
  Omnilock, NervosDAO since/epoch, and Type ID.
- proposal-local NovaSeal devnet/profile certification that passes local live
  CKB RPC acceptance and preserves external production blockers.

The 0.17 branch records closure of the 0.16 review findings in
`docs/archive/0.17/CELLSCRIPT_0_17_REVIEW_FINDINGS_CLOSURE.md`: ProofPlan matching is no longer keyed
only by coarse category/feature/status, `validate-tx` rejects bare evidence
tokens and cross-checks indexed payload fields, protocol stdlib descriptor
stubs are not stable, and `solve-tx` is explicitly `can_submit=false`.

Detailed status:

- [0.16 roadmap](CELLSCRIPT_0_16_ROADMAP.md)
- [0.16.1 release notes](../docs/releases/CELLSCRIPT_0_16_1_RELEASE_NOTES.md)

### 0.17: iCKB-Grade Protocol Semantics

0.17 moves the protocol-equivalence track from design/model evidence into
executable CKB-facing semantics:

- `--primitive-strict=0.17`;
- HeaderDep SourceViews and DAO accumulated-rate/maturity checks;
- xUDT group amount conserved/minted/burned helpers;
- current script hash, script args/hash guards, OutPoint and MetaPoint bridge
  helpers;
- C256 helper lowering and executable local `u128` materialization;
- iCKB benchmark specs and the first partial CKB VM differential evidence;
- fail-closed production-equivalence gate semantics that remain `NOT_PROVEN`
  until every selected row has dual-side VM evidence.

The standalone 0.17 milestone does not claim full iCKB production equivalence.
It closes the major semantic gaps and records the remaining proof closure work
for 0.18. On the carried-forward 0.20 branch, that 0.18 work has moved the
manifest-declared executable iCKB claim set to `EXECUTED_CKB_VM_DIFF` /
`PROVEN`; broader state-space, external-audit, and mainnet-certification claims
remain out of scope.

Detailed status:

- [0.17 roadmap](../docs/archive/0.17/CELLSCRIPT_0_17_ROADMAP.md)
- [iCKB final report](../docs/archive/0.17/CELLSCRIPT_0_17_ICKB_FINAL_REPORT.md)

### 0.18: First-Class Script API And Equivalence Closure

0.18 should start by replacing helper fragmentation with typed read-only
ScriptRef / ScriptArgs access:

- `cell.lock.code_hash`, `cell.lock.hash_type`, and args checks;
- optional type script code/hash/args checks;
- exact, prefix, suffix, and hash-based script args comparisons;
- remaining iCKB equivalence prerequisites such as byte-accurate receipt
  decoding, owner-auth witness fixtures, generic aggregate lowering, and
  production evidence-manifest closure.

The goal is to make iCKB-style equivalence verification possible without adding
script construction or deployment solving to the compiler.

Detailed status:

- [0.18 roadmap](../docs/CELLSCRIPT_0_18_ROADMAP.md)

### 0.19: Package Registry Phase 1 And Adapter Boundary

0.19 scope is complete. It turns the CKB ecosystem reuse boundary and Phase 1
package/deployment identity registry into executable evidence:

- centralized inline CKB ABI constants in `src/ckb_abi.rs`;
- parity tests against `ckb-std` / `ckb-types` for constants, SourceView,
  WitnessArgs layout, TYPE_ID, since/epoch, and occupied-capacity field use;
- `cellc action build --json` adapter contracts and packed-materialization
  requirements;
- `cellc ckb-std-compat --json` compatibility reports;
- an offline `examples/ckb-sdk-builder` adapter-shape crate using
  `ckb-sdk-rust` packed types and adapter-owned evidence boundaries;
- namespace-aware package manifests and `cellc init --namespace`;
- Git-backed source registry records with tag-pinned source hash verification;
- path, git, and registry dependency resolution in the compile pipeline;
- `Cell.lock` build identity for compiler version, target profile, artifact,
  metadata, schema, ABI, and constraints hashes;
- `cellc package verify` and `cellc registry verify` fail-closed text and JSON
  verification.

Generated TypeScript builders, live-chain deployment proof, stateful flow
runner evidence, publisher signatures, and on-chain registry/index/proxy design
are moved to 0.20.

Detailed status:

- [0.19 roadmap](../docs/CELLSCRIPT_0_19_ROADMAP.md)
- [0.16-0.20 release notes](../docs/releases/CELLSCRIPT_0_16_TO_0_20_RELEASE_NOTES.md)
- [Registry Phase 1](../docs/CELLSCRIPT_REGISTRY_PHASE1.md)

### 0.20: Generated Builder And Live Registry Proof

0.20 should consume the 0.19 package/build/deployment identity from generated
builders and live-chain verification:

- `cellc gen-builder --target typescript` with typed action APIs and CCC
  integration;
- generated-builder package tests, dry-run/submit modes, and negative
  builder-shape rejection;
- `cellc registry verify --live` / equivalent live-cell verification for
  network-specific deployment facts;
- VS Code and tooling-gate coverage for generated builder creation, package
  verification, registry verification, and generated `npm test`;
- stale/wrong-network/wrong-code-hash/missing-CellDep/deprecated deployment
  rejection fixtures;
- stateful flow runner evidence for canonical examples;
- multi-file package support as a compiler/tooling boundary. NovaSeal
  fungible-xUDT has a shared-schema refactor with live local devnet stateful
  evidence for issue, transfer, settle, and required negative cases; iCKB and
  DobEvo remain unrefactored unless a real shared-schema boundary and matching
  evidence exist;
- browser-local playground file-tree and import/export support without
  server-side source storage or compile load;
- registry trust hardening for publisher signatures, trust anchors, mutable
  channels, revocation, and possible on-chain registry/index/proxy design.

Detailed status:

- [0.20 roadmap](../docs/CELLSCRIPT_0_20_ROADMAP.md)

### 0.21: Semantic Closure And Authenticated Evidence

0.21 keeps the action-centred CellScript model intact and closes the highest
value gaps around executable protocol law and evidence integrity:

- executable lowering for supported aggregate invariant shapes;
- static validation that action `transition` edges are declared by their
  `flow` rules;
- completed live-cell action resolution and action-aware CKB Script scans where
  metadata already proves the required source views;
- an authenticated compile receipt envelope over the existing
  `CompileMetadata`, ProofPlan, audit bundle, metadata hash, and artifact hash
  stream;
- a canonical grouped `cellc` command tree with compatibility aliases,
  complete help text, and an explicit diagnostic transport contract;
- a dedicated CellScript MCP server and programming skill pack so agents can
  query compiler evidence, docs, examples, command discovery, and gate policy
  without inventing workflows;
- a derived cyclic ProtocolGraph view for audit and builder guidance, not a
  core compiler graph IR;
- type-level TemplateLayout metadata that separates semantic validity from
  physical commitment layout.

Actor syntax, general template Merkleisation, and `observes` / `covid`
composition syntax stay deferred until a concrete CKB protocol proves that the
existing action surface and metadata cannot express the requirement clearly.

Detailed status:

- [0.21 roadmap](../docs/CELLSCRIPT_0_21_ROADMAP.md)
- [0.21 CLI UX reorganisation plan](CELLSCRIPT_0_21_CLI_UX_PLAN.md)

### Next Authorization Hardening Track

The next security-sensitive track should make CKB authorization literal before
it becomes ergonomic.

Fixed-width `lock_args` binding to the executing script args landed in the
0.13 line. Remaining planned order:

1. explicit sighash verification primitive with digest mode, script group scope,
   witness layout, and replay assumptions;
2. stable metadata and report fields for signature verification obligations;
3. first-class verified signer values only after explicit primitives are proven;
4. optional `protects T { self ... }` sugar only after protected-input
   selection and lock-group aggregation semantics are exact.

Non-goals:

- no implicit signer derivation from `Address`;
- no hidden sighash defaults;
- no parameter-name-based authority.

Source documents:

- [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)
- [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)

### CKB Evidence Hardening Track

The CKB acceptance surface should continue moving from broad acceptance evidence
to predicate-specific evidence.

Priorities:

- keep action acceptance builder-backed and report-validated;
- keep lock valid-spend and invalid-spend matrices mandatory for bundled locks;
- require invalid-spend cases to match stable script failure paths, not generic
  transaction rejection;
- keep cycles, serialized transaction size, occupied capacity, and malformed
  rejection evidence in reports;
- keep stateful business-flow/action coverage mandatory for full releases;
- extend the matrix when new bundled locks enter production scope.

Source documents:

- [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)
- [Capacity and builder contract](../docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [Metadata and production gates wiki](../docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md)

### Collections And Ownership Track

The collections roadmap stays conservative because CKB Cell ownership is not a
generic heap model.

Completed:

- stack-backed fixed-width `Vec<T>` helper support;
- typed/contextual `Vec<T>` literals for local stack vectors;
- metadata and `cellc explain-generics` visibility for checked instantiations.

Deferred:

- full generic `HashMap<K, V>` and `HashSet<T>`;
- `Vec<Cell<T>>` and other cell-backed linear ownership collections;
- source-level `Option<T>` lowering;
- explicit `Vec<T, N>[...]` bounded-vector literal syntax.

Source documents:

- [0.13 release scope](../docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md)
- [Collections support matrix](../docs/CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md)
- [Linear ownership](../docs/CELLSCRIPT_LINEAR_OWNERSHIP.md)

### Declarative CKB Policy Track

Some CKB facts are currently visible in metadata and builder evidence rather than
first-class source policy.

Future work:

- declarative capacity requirements where the compiler can check them;
- declarative since/header/timepoint assumptions for timelock-like protocols;
- explicit continuity policy for signature-directed input/output Cell updates, including type id,
  lock, data schema, and capacity continuity;
- clearer builder obligations in action builder plans.

Source documents:

- [Capacity and builder contract](../docs/CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md)
- [Output bindings](../docs/CELLSCRIPT_OUTPUT_BINDINGS.md)
- [CKB language audit](../docs/CELLSCRIPT_CKB_LANGUAGE_AUDIT.md)

### Documentation And Developer Experience Track

The docs should stay useful to new readers and strict enough for reviewers.

Completed:

- GitHub Wiki is version-neutral and cookbook-oriented;
- `_Sidebar.md` gives a book-like navigation structure;
- cookbook recipes and CKB glossary exist;
- LSP and VS Code grammar/snippets cover the new lock-boundary syntax.

Future work:

- keep wiki links rendered through GitHub Wiki URLs;
- add recipes when new stable language patterns land;
- keep release notes in `docs/releases/` and roadmap files in `roadmap/`,
  separate from tutorial pages;
- keep top-level `examples/*.cell` as the single checked-in bundled business
  source, with `examples/language/*.cell` for compiler/tooling coverage and
  `tests/benchmarks/ickb_specs/*.cell` for the iCKB-inspired benchmark surface.

Source documents:

- [GitHub Wiki](https://github.com/a19q3/CellScript/wiki)
- [Surface elegance RFC](../docs/CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)

## Roadmap Discipline

Roadmap entries should follow these rules:

- completed work must point to tests, release notes, or evidence reports;
- deferred work must say why it is deferred;
- security-sensitive syntax must distinguish data source from authority;
- CKB production claims must distinguish compiler evidence from chain evidence;
- wiki pages should teach the current stable surface, not act as release notes.
