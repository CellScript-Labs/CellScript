# CellScript 0.21 Roadmap

**Status**: Implementation checkpoint; P0/P1 slices are in tree, with release
claims still gated by CI/release evidence.
**Scope**: semantic closure, authenticated compiler evidence, CLI UX
reorganisation, agent-facing developer tooling, derived protocol views, and
audit-visible template layout
**Depends on**: 0.20 generated builder, live deployment verification, stateful
flow evidence, and registry trust hardening

## Goal

CellScript 0.21 should harden the language model without changing its centre.
The compiler remains an action-centred cell-transition DSL:

```text
action + transition + verification + typed cell effects
```

0.21 should not turn CellScript into an actor language. It should make the
existing action model more executable, more auditable, and more precise across
trust boundaries.

0.21 should also clean up the `cellc` command surface without weakening the
tooling it exposes. The CLI work is a developer-experience and maintenance
track: group related commands, keep compatibility aliases, and make diagnostic
output easier for tools to consume.

0.21 should add an agent-facing tooling layer as well: a dedicated CellScript
MCP server and a CellScript programming skill pack. These should make AI agents
and editors use the same compiler evidence, metadata, examples, and gate policy
that human developers use, rather than relying on prompt memory or ad-hoc shell
recipes.

The target stack is:

```text
CellScript source
  -> checked action / flow / invariant semantics
  -> executable aggregate obligations
  -> CompileMetadata + ProofPlan + authenticated receipt envelope
  -> derived cyclic ProtocolGraph view
  -> optional type-level TemplateLayout metadata
  -> later backend commitments when a concrete protocol needs them
```

## Non-Goals

- Do not replace `action` with `actor`, `entry`, `emit`, or `become`.
- Do not add an actor frontend unless a concrete CKB protocol proves that the
  existing action surface cannot express it clearly.
- Do not introduce a core `ProtocolGraph` IR. The graph is a derived view over
  existing IR and metadata.
- Do not call protocol transition graphs DAGs. Pool, channel, game, and many
  router-style protocols can cycle.
- Do not Merkleise every protocol. Simple tokens, vesting flows, fixed-shape
  AMMs, and small receipt flows should stay flat.
- Do not treat signed metadata as consensus truth. Consensus truth remains the
  generated on-chain script and CKB transaction validation.
- Do not remove existing flat CLI commands without a documented compatibility
  window and release-note migration path.
- Do not let the MCP server sign keys, submit transactions, mutate registries,
  or change shell/editor configuration by default.
- Do not let programming skills describe syntax, CKB behaviour, or production
  claims that are not backed by current docs, examples, tests, and gates.

## P0: Executable Aggregate Invariant Lowering

0.15 introduced aggregate invariant declarations and ProofPlan records, but
many aggregate invariants are still metadata-only or runtime-helper-required.
0.21 starts promoting the common group-scan cases into executable verifier
lowering. The first compiler-owned slice recognises xUDT group amount
conservation equality and emits the runtime group scanner only in matching
action preludes where the action locally proves one consumed amount is preserved
into one created amount. Delta and generic fixed-field scans remain
explicit-helper or fail-closed until their bindings are lowered safely.

Target declarations:

```text
assert_sum(group_outputs<T>.field) == assert_sum(group_inputs<T>.field)
assert_delta(group_outputs<T>.field, minted, scope = group)
assert_delta(group_inputs<T>.field, burned, scope = group)
```

Required compiler work:

- lower `group_inputs<T>.field` and `group_outputs<T>.field` aggregate reads
  into explicit CKB source scans;
- emit checked accumulation for fixed-width numeric fields;
- connect lowered checks to the existing ProofPlan rows;
- change the relevant ProofPlan coverage from metadata-only/runtime-required to
  executable when the backend really emits the verifier check;
- keep unsupported aggregate shapes fail-closed with precise diagnostics.

Acceptance:

- iCKB benchmark specs should be able to remove manual xUDT conservation-helper
  shadowing for the action-local aggregate equality case that the compiler now
  lowers;
- existing iCKB differential rows must keep passing for the promoted cases;
- strict gates must reject unsupported aggregate declarations instead of
  silently recording them as executable;
- strict gates must reject stale helper-required aggregate ProofPlan records
  even when a raw matching runtime access is present elsewhere in module
  metadata;
- ProofPlan output must distinguish executable aggregate lowering from
  metadata-only or helper-required aggregate declarations.

## P0: Flow Edge Membership Validation

`flow Type.state { A -> B; }` should be enforced against action-level
`transition` edges. Today the flow checker validates states and flow-aware
creates, but the declared `(from, to)` rule set should become part of the
static contract.

Required compiler work:

- store declared flow edges in the flow checker, not only the state set;
- reject an action transition whose `(from, to)` pair is absent from the
  declared flow for that type and state field;
- keep repeated action-level `transition` declarations as the public syntax;
- report the rejected action, type, state field, and missing edge.

Acceptance:

- actions that follow declared flow rules continue to compile;
- an undeclared `A -> C` action edge fails even when both `A` and `C` are known
  states;
- tests include at least one cyclic flow edge, proving cycles are valid when
  declared;
- no codegen changes are required for this track.

## P0: Builder Resolution And Action-Aware CKB Scans

0.20 moves builders and live deployment verification forward, but full action
resolution still has practical gaps. 0.21 should close the highest-value
builder-facing holes before adding new language metaphors.

Required work:

- complete live-cell action resolution in the CKB adapter so a typed action
  plan can become a resolved transaction candidate without manual
  `ResolvedActionTx` construction;
- support action-aware Script scans where metadata already proves which source
  views, roles, and script fields are required;
- improve variable-length ScriptArgs construction and checking where the
  current fixed-width surfaces are too narrow;
- keep all builder results explicit about what is compiler-proven, builder
  assumed, node-dry-run checked, tx-pool accepted, or submitted.

Current implementation note:

- the adapter now resolves builder/runtime-filled materialised action drafts
  into `ResolvedActionTx`, including packed inputs, outputs, outputs_data,
  witnesses, CellDeps, header deps, lineage, and manifest-assisted CellDep
  completion for matching output scripts;
- `cellc action build --json` now emits
  `action_scan_selectors.schema =
  cellscript-action-scan-selectors-v0.21`. The selectors are derived directly
  from `transaction_runtime_input_requirements` and expose action, source,
  role, component, field, ABI, blocker, and adapter-action hints for builder
  runtimes;
- `cellc gen-builder --target typescript` now embeds the same selector envelope
  in the generated builder manifest, exports it through `actionSpecs`, includes
  it on every `GeneratedActionPlan`, and passes it through the runtime
  `resolveLiveCells` request;
- generated TypeScript builders now require `resolveLiveCells` to return
  `scanSelectorEvidence` for declared selectors. Missing evidence or selector
  mismatches such as a wrong role/source/binding fail before
  `buildTransaction` is called;
- the Rust adapter now applies the same fail-closed check to materialised
  `ActionPlan` JSON: when `action_scan_selectors` is declared,
  `transaction_draft.scan_selector_evidence` must resolve every selector and
  must match source, role, binding, feature, component, and script field before
  `ResolvedActionTx` is constructed;
- materialised output lock/type scripts now support byte-fragment
  `args_parts` construction (`hex`, `utf8`, `u8`, `u32_le`, `u64_le`) for
  variable-length ScriptArgs while rejecting ambiguous drafts that combine
  non-empty `args` with `args_parts`;
- plain compiler `ActionPlan` templates still do not include live-cell
  candidates or node-backed evidence. Selectors are compile-only guidance:
  runtime-required selectors remain `requires-runtime-resolution`, and full
  automatic live-cell discovery remains open until a builder or adapter binds
  them to concrete cells and CKB node evidence.

Acceptance:

- generated builders and the Rust adapter share the same resolved-action
  contract;
- missing live cells, wrong script role, wrong ScriptArgs, wrong CellDep, and
  stale deployment evidence fail before signing when metadata can prove the
  mismatch;
- CKB node-backed evidence remains separate from compile-only evidence.

## P0: Authenticated Metadata Envelope

The existing `CompileMetadata`, ProofPlan, and audit bundle are already the
compiler evidence stream. 0.21 should authenticate that stream instead of
creating a parallel receipt format.

Add an authenticated compile receipt envelope over canonical metadata:

```text
CompileReceipt {
  schema = "cellscript-compile-receipt-v1"
  compiler_version
  rust_toolchain
  target
  target_profile
  source_hash
  ast_normalised_hash
  ir_normalised_hash
  proof_plan_hash
  protocol_graph_hash?
  template_layout_hash?
  artifact_hash
  metadata_hash
  signatures
}
```

Signature roles:

- `compiler` or release identity, using Ed25519 for public trust;
- `publisher` or deployer identity, also Ed25519 by default;
- private/internal workflows may use a keyed BLAKE2b MAC only when both producer
  and consumer share the trust boundary.

Required CLI surface:

```text
cellc receipt <input> --output receipt.json
cellc sign-receipt receipt.json --role publisher --key <key>
cellc verify-receipt receipt.json --metadata artifact.meta.json --artifact artifact.elf
```

Acceptance:

- canonical receipt hashing is deterministic across machines;
- verification fails if source hash, metadata hash, artifact hash, or signature
  payload changes;
- `verify-artifact` can consume the receipt without weakening the existing
  artifact hash and target-profile checks;
- unsigned sidecars remain usable for local development but are labelled
  advisory across trust boundaries.

Current implementation note:

- `cellc receipt` writes a `cellscript-compile-receipt-v1` envelope from
  compiler metadata, including artifact, metadata, ProofPlan, ProtocolGraph,
  and TemplateLayout hashes;
- `cellc sign-receipt` signs the canonical receipt payload hash with Ed25519
  PKCS#8 keys and records the signature role, public key, algorithm, payload
  hash, and signature bytes;
- `cellc verify-receipt` rebinds the receipt to the supplied metadata and
  artifact, verifies signatures when present, and reports unsigned receipts as
  advisory;
- `cellc verify-artifact --receipt` consumes the same receipt verification path
  while preserving the existing artifact hash, metadata, and policy checks.

## P1: CLI UX Reorganisation And Diagnostic Contract

The current CLI exposes the right categories of functionality, but too many
related workflows occupy top-level slots or exist under both flat and nested
spellings. 0.21 should make the nested command tree canonical while preserving
legacy compatibility for scripts.

Detailed plan:

- [0.21 CLI UX reorganisation plan](../roadmap/CELLSCRIPT_0_21_CLI_UX_PLAN.md)

Required work:

- keep the curated `cellc --help` entry point, but derive its common-command
  list from the canonical command registry instead of detached strings;
- make nested forms canonical for explain, transaction, deployment, registry,
  package, and auth workflows;
- hide duplicate flat aliases from ordinary discovery while keeping them
  executable during a deprecation window;
- fill missing help text for visible arguments;
- migrate grouped parser surfaces toward `#[derive(Args)]` and
  `#[derive(Subcommand)]` so command definitions and argument extraction cannot
  drift independently;
- add `--message-format=json` for diagnostics while keeping `--json` as the
  successful-output payload flag;
- add explicit colour control and respect `NO_COLOR`.

Canonical grouped surface:

```text
cellc explain profile|proof|assumptions|generics|graph ...
cellc tx validate|solve|trace ...
cellc deploy plan|verify|diff|lock-deps ...
cellc registry verify|add|edit ...
cellc package verify
cellc auth capability create|submit|revoke
```

Acceptance:

- `cellc --list` promotes the canonical command tree, not duplicate legacy
  aliases;
- legacy flat commands still route to the same handlers during the 0.21
  compatibility window and warn on stderr when appropriate;
- documentation examples use canonical nested commands;
- all visible flags and positionals have non-empty help text;
- machine-readable diagnostics can be requested without scraping coloured text;
- parser changes do not alter backend, metadata, registry, deployment, or CKB
  semantics.

Current implementation note:

- `cellc --help` and `cellc --list --json` expose the canonical 0.21 command
  groups while preserving executable legacy aliases for the compatibility
  window;
- canonical nested forms are wired for explain, transaction, deployment,
  registry, package, and auth workflows, with legacy aliases routed to the same
  handlers;
- `--message-format=json` is available for diagnostics without changing
  successful payload `--json` output;
- `--color=auto|always|never` is available and `NO_COLOR` disables ANSI colour
  output in automatic mode.

## P1: Dedicated MCP Server And Programming Skills

CellScript's CLI and metadata are already useful for human auditors. 0.21
should expose the same evidence through a dedicated MCP server and a curated
CellScript programming skill pack so AI agents can work from project facts
instead of reconstructing workflows from memory.

The MCP server is an agent-facing adapter over existing project contracts. It
must not become a second compiler, an unaudited deployment client, or a hidden
authority layer.

Required MCP server capabilities:

- inspect the current package, workspace members, source units, dependency
  lock state, and build identity;
- run or prepare safe read-only compiler queries such as check, metadata,
  constraints, ABI, ProofPlan, assumptions, graph, and TemplateLayout reports;
- return structured diagnostics with source spans and rendered text;
- expose command discovery for the canonical 0.21 command tree and legacy alias
  migration hints;
- surface gate policy and recommended validation commands for the current
  change type;
- read relevant docs, wiki tutorials, examples, and release-roadmap entries by
  topic;
- report whether evidence is compile-only, builder-backed, node dry-run,
  tx-pool accepted, submitted, or externally attested;
- keep write, signing, publish, deployment, and registry mutation tools
  disabled unless a future authenticated workflow explicitly opts in.

Required programming skill pack:

- `cellscript-language-basics`: action, transition, resource/shared/receipt,
  capabilities, flows, and fail-closed boundaries;
- `cellscript-ckb-model`: Cell Model, lock/type script roles, Source views,
  WitnessArgs, CellDeps, capacity, since/time, and transaction replacement;
- `cellscript-package-cli`: package layout, `Cell.toml`, build/check/fmt/test,
  canonical 0.21 command groups, registry/package verification, and migration
  from legacy flat aliases;
- `cellscript-metadata-audit`: CompileMetadata, ProofPlan, builder
  assumptions, constraints, ABI, audit bundles, receipts, and artifact
  verification;
- `cellscript-builder-deployment`: generated builders, action-aware scans,
  deployment plans, live registry verification, and evidence boundaries;
- `cellscript-diagnostics`: common parser/type/lowering/runtime diagnostics,
  migration hints, and safe next actions.

Acceptance:

- agents can discover CellScript workflows through the MCP server without
  scraping `--help` output;
- MCP responses preserve stdout/stderr and human/machine-readable diagnostic
  boundaries from the CLI plan;
- MCP tools use existing compiler APIs or `cellc` command contracts rather than
  duplicating semantic logic;
- programming skills cite current repository docs and examples, not stale
  release plans;
- skills distinguish implemented, reserved, deferred, metadata-only, and
  fail-closed surfaces;
- no MCP or skill workflow can claim CKB production readiness without pointing
  to the required compiler, builder, and chain evidence;
- the skill pack is covered by a lightweight freshness check that fails when
  referenced docs or command names disappear.

Current implementation note:

- `cellscript-mcp` is an in-repository stdio MCP server that exposes read-only
  compiler and documentation tools while delegating compiler facts to `cellc`;
- the first tool set covers command discovery, check, constraints, metadata,
  TemplateLayout extraction, ProtocolGraph, diagnostics, gate policy, docs by
  topic, and evidence-level reporting;
- write, signing, publish, deployment submission, registry mutation, and
  shell/editor configuration tools are intentionally absent by default;
- the CellScript skill pack lives under `docs/skills/cellscript-*` and
  `scripts/check_cellscript_skill_pack.py` verifies that referenced docs,
  examples, and command names still exist.

## P1: Derived Cyclic ProtocolGraph View

CellScript needs a whole-protocol audit view, not a new core graph IR.
0.21 should derive a cyclic `ProtocolGraph` from existing IR and metadata.

Primary vertex:

```text
vertex = cell type family
       = resource/shared/receipt type
       + optional flow state
```

Module/package membership is grouping and namespacing, not the primary semantic
cut. This matches the current type-shaped flow, action, scheduler, and runtime
metadata better than a source-module vertex.

Each edge should include:

```text
action_name
source_vertex
target_vertex
consume_set
read_refs
create_set
mutate_set
proof_plan_ids
ckb_runtime_accesses
builder_assumptions
touches_shared
source_span
```

Required CLI surface:

```text
cellc explain-graph <input> --json
cellc explain-graph <input> --format mermaid
```

Acceptance:

- AMM-style `Pool -> Pool` produces a visible self-loop;
- acyclic factory/launch flows are labelled acyclic only after graph analysis,
  not by assumption;
- graph output is embedded into audit bundles as a derived view;
- WASM/browser metadata does not bloat by default. Large graph payloads should
  be opt-in or omitted from the default playground JSON.

Current implementation note:

- `cellc explain graph` is the canonical grouped command and
  `cellc explain-graph` remains a compatibility alias;
- JSON and Mermaid outputs are derived from existing `CompileMetadata`, including
  flow-state vertices, action edges, runtime accesses, builder assumptions, and
  cycle detection;
- audit bundles embed the derived `protocol_graph` view without making it core
  IR or consensus state.

## P1: Type-Level TemplateLayout Metadata

Template layout describes physical commitment shape, not semantic validity.
ProofPlan explains why a transition is valid; TemplateLayout explains how a
set of authorised templates is committed in metadata or, later, on chain.

0.21 should add metadata only:

```text
TemplateLayout {
  scope = Type | TypeFamily
  type_name
  state_field?
  state_machine_acyclic
  cycle_policy = RootRequired | PathOnlyAllowed
  layout = Flat | MerkleCandidate
  root_hash?
  leaf_schema
  consensus_checked = false
}
```

Rules:

- the primary cut is type-level;
- package/module roots may group type-level subtrees, but they are not the
  primary proof unit;
- cyclic protocols can still use Merkle authentication, but they must retain a
  cycle-capable root commitment;
- acyclic factory, launch, and router flows may later use more aggressive path
  pruning.

Acceptance:

- TemplateLayout appears in metadata and authenticated receipt hashes;
- no generated verifier code changes are required for metadata-only layouts;
- audit output clearly says whether the layout is flat, Merkle-candidate, or
  consensus-checked;
- unsupported layout claims fail validation instead of being ignored.

Current implementation note:

- `CompileMetadata.template_layouts` now carries
  `cellscript-template-layout-v0.21` records for resource, shared, and receipt
  cell types;
- each record includes deterministic `template_layout_hash` material that is
  validated with the rest of compile metadata and included in compile receipt
  hashing;
- current layouts remain metadata-only with `consensus_checked = false`; any
  unsupported consensus-checked claim is rejected by metadata validation.

## P2: Optional Template Merkleisation Backend

Backend Merkleisation is deliberately later than TemplateLayout metadata.

Promote only when a concrete protocol has a large authorised-output menu, such
as:

- game multiplexers;
- channel factories;
- launch/factory protocols;
- large routers;
- nested protocol menus.

Not useful for:

- simple token actions;
- small vesting flows;
- fixed-shape AMM actions;
- one-shot receipts.

Acceptance before promotion:

- TemplateLayout metadata is already stable;
- the generated script verifies Merkle membership when `consensus_checked =
  true`;
- builders can produce the required path material;
- flat layout remains the default and remains backwards compatible.

## P2: `observes` / `covid` Protocol-Composition Syntax

`observes` and `covid` remain candidate additive syntax. They should be added
only after a real CKB composition case proves that manual action metadata and
builder assumptions are too weak or too noisy.

Candidate surface:

```text
observes asset by self.covid_id {
  inputs { proxy: MinterProxy }
  outputs { proxy: MinterProxy; recipient: KCC20 }
}
```

Lowering requirements:

- lower into existing action metadata, not an actor model;
- add an `IrObservedCovenant`-style structured record;
- emit ProofPlan rows with category `covenant_observation`;
- expose runtime accesses and builder assumptions;
- require generated builders to provide the observed input/output lane
  material.

Non-goals:

- no `actor` keyword;
- no `entry` keyword;
- no runtime cross-script call semantics;
- no hidden covenant authority.

## Evidence Boundary

0.21 claims should stay conservative:

- executable aggregate invariant lowering is a compiler/backend claim only for
  the aggregate shapes that have tests and generated verifier evidence on the
  corresponding ProofPlan record;
- authenticated receipts prove metadata/artifact integrity, not transaction
  validity;
- CLI command grouping is a discovery and compatibility improvement, not a
  change to generated artifacts or CKB semantics;
- MCP and programming-skill support are agent-facing developer tooling, not
  independent evidence of compiler correctness or production readiness;
- ProtocolGraph is an audit derived view, not consensus state;
- TemplateLayout is metadata until a backend explicitly verifies the layout on
  chain;
- Merkleisation and `observes`/`covid` remain out of release scope unless this
  roadmap is updated with concrete protocol evidence.

## Validation

Routine 0.21 development should still use:

```bash
./scripts/cellscript_gate.sh dev
```

Backend or verifier-lowering work must use:

```bash
./scripts/cellscript_gate.sh backend
```

Merge-readiness remains:

```bash
./scripts/cellscript_gate.sh ci
```

Release-facing claims require:

```bash
./scripts/cellscript_gate.sh release
```

0.21 work that changes aggregate lowering, graph metadata, receipt
authentication, TemplateLayout hashing, CLI parsing, diagnostic transport, MCP
tooling, programming skills, or builder resolution must add focused regression
tests before relying on a broad gate.
