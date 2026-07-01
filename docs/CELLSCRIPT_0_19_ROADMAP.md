# CellScript 0.19 Roadmap

**Status**: Scope complete
**Scope**: CKB ecosystem reuse, ckb-std compatibility, grammar / syntax
governance, and Phase 1 package / deployment identity registry closure
**Depends on**: v0.17 CKB protocol semantics and v0.18 first-class ScriptRef /
ScriptArgs work

**Version carry-forward**: 0.19 inherits the 0.18 `EXECUTED_CKB_VM_DIFF` /
`PROVEN` iCKB evidence state. It may keep the iCKB fixtures compiling while
syntax and adapter boundaries evolve, but its release claim is registry,
adapter, `ckb-std`, and grammar-governance closure rather than a new iCKB
equivalence milestone.

## Goal

CellScript 0.19 turns compiler artifacts into a reproducible package and
deployment-identity layer.

The compiler already emits metadata, ABI records, ProofPlan records, CKB target
profile data, and production evidence reports. 0.19 closes the first registry
layer that lets source packages, build artifacts, metadata, and deployment facts
be resolved and verified fail-closed before later transaction builders consume
them.

The target stack is:

```text
CellScript compiler
  -> source package metadata / ABI / target profile / build identity

Git-backed source registry
  -> immutable package-version record / tag / source hash

Cell.lock
  -> resolved package graph / artifact hashes / metadata hashes / deployment facts

cellc package verify / cellc registry verify
  -> fail-closed source, artifact, metadata, and off-chain deployment checks
```

Rust-side production workflows use the same compiler outputs through the CKB
adapter:

```text
CellScript compiler
  -> artifact / metadata / ABI / deploy plan / action build plan / witness bytes

cellscript-ckb-adapter
  -> reads compiler outputs
  -> verifies deployment and schema hashes
  -> materialises CKB transactions through ckb-sdk-rust
  -> records capacity, CellDep, cycle, tx-pool, and submission evidence

ckb-sdk-rust (5.x)
  -> CKB data structures (ckb-types 1.0.0)
  -> sync + async RPC / indexer clients
  -> CellCollector (Default / Offchain / LightClient)
  -> CellDepResolver, HeaderDepResolver
  -> Signer + lock-specific ScriptUnlocker (SecpSighash, SecpMultisig Legacy/V2, ACP, Cheque, OmniLock)
  -> CapacityBalancer / CapacityProvider, protocol tx_builder modules
  -> transaction construction, signing, tx-pool acceptance, submission
```

The adapter boundary is documented in
[`CELLSCRIPT_CKB_ADAPTER.md`](CELLSCRIPT_CKB_ADAPTER.md). It is intentionally
outside compiler core: CellScript emits verified transaction intent,
`ckb-sdk-rust` realises that intent, and CKB node acceptance is the production
evidence.

0.19 also owns the CKB ecosystem reuse and ckb-std compatibility work:

- [`CELLSCRIPT_CKB_STD_COMPAT.md`](CELLSCRIPT_CKB_STD_COMPAT.md) defines the
  contract-side compatibility boundary for generated verifier code, parity
  tests, and future Rust-shim/native-simulation workflows.
- [`CELLSCRIPT_CKB_ADAPTER.md`](CELLSCRIPT_CKB_ADAPTER.md) defines the
  transaction-realisation boundary between compiler outputs, `ckb-sdk-rust`,
  and local-node acceptance evidence.

These two documents are part of 0.19 scope because they govern how the
registry/deployment/adapter layer reuses existing CKB ecosystem libraries.
They are not 0.18 protocol-equivalence evidence.

0.19 also owns the active grammar and syntax-governance pass:

- [`CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md`](CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md)
  defines the public semantic split between action shape, Cell movement,
  verification constraints, and global protocol law.
- [`CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md`](CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md)
  defines the reusable syntax-combination audit method that must guard parser,
  typechecker, lowering, metadata, codegen, formatter, LSP, and docs changes.
- [`CELLSCRIPT_SURFACE_ELEGANCE_RFC.md`](CELLSCRIPT_SURFACE_ELEGANCE_RFC.md)
  remains a candidate syntax backlog. Items in that RFC are not shipped unless
  they have parser, typechecker, lowering, metadata, formatter, LSP, examples,
  and regression coverage.

This governance work is in scope because registry-bound compiler metadata must
stay stable and source-level audit shape must remain visible. It must not reopen
the v0.13 action model casually, and it must not promote sugar that hides
`consume`, `create`, `destroy`, `transition`, `preserve`, witness placement,
CellDeps, or signer authority.

Completed 0.19 implementation slices:

- `scripts/install.sh` is a one-line curl installer for CellScript toolchain
  distribution. It supports platform detection (macOS/Linux, x86_64/ARM64),
  automatic latest-version resolution, multi-mirror binary download with
  SHA256 verification, PATH configuration for bash/zsh/fish, and dry-run
  mode. Mirror sources include GitHub direct, ghgo.xyz, gh-proxy.com, and
  ghfast.top for China mainland users who cannot reach
  `raw.githubusercontent.com` or `api.github.com`.
- `.github/workflows/release.yml` is the cross-platform release CI. It
  builds `cellc` for four targets (macOS Intel/ARM, Linux x86_64/ARM64),
  packages tarballs, computes SHA256SUMS, and creates GitHub Releases with
  install instructions covering both global and China mainland users.
  `install.sh` is also uploaded as a Release asset for direct download
  from `github.com` domain.

- `src/ckb_abi.rs` centralizes the inline CKB ABI constants used by CellScript.
- `tests/ckb_std_compat.rs` adds parity tests against `ckb-std` and
  `ckb-types`.
- inline occupied-capacity lowering now uses CKB
  `CellField::OccupiedCapacity`;
- generated stdlib syscall wrappers consume the same ABI table as main
  codegen;
- `WitnessArgs` layout fixtures cover valid and malformed Molecule tables.
- TYPE_ID lifecycle and args-hash helpers are pinned to the
  `ckb-std::type_id` contract.
- since/epoch fixtures cover valid and malformed `ckb-std::since` cases.
- `cellc tx validate --json` explicitly reports metadata/evidence-only
  validation and no CKB VM or tx-pool acceptance.
- `cellc action build --json` emits a v0.19 adapter contract and
  packed-materialization requirements for headless `ckb-sdk-rust` / CCC
  adapters, including required resolved-transaction fields and an acceptance
  report template, while keeping the compiler-produced draft non-submittable.
- `cellc ckb-std-compat --json` emits a machine-readable compatibility report
  for ABI parity and adapter-boundary release evidence.
- `crates/cellscript-ckb-adapter` is the formal headless adapter crate using
  local `ckb-sdk-rust` for packed transaction materialization, signer/RPC
  boundary types, script construction, TYPE_ID args checks, capacity checks,
  outputs/outputs_data pairing, preview data, and acceptance reports.
- `examples/ckb-sdk-builder` is now a cookbook wrapper around the formal
  adapter crate.
- `scripts/cellscript_ckb_adapter_acceptance.sh` starts a local CKB devnet and
  records focused `estimate_cycles` plus `test_tx_pool_accept` evidence for the
  adapter boundary.
- `scripts/cellscript_syntax_combo_audit.py` emits a machine-readable
  grammar-governance release matrix, oracle summary, and required bug-class
  coverage for high-risk syntax regressions.
- `tests/syntax_combo/matrix.toml` records the mode floors, required origins,
  and required grammar-governance bug classes used by the reusable audit gate.
- `cellc init --namespace` writes namespace-aware package manifests.
- `cellc publish` creates registry records with source roots, explicit entry
  parent handling, and a source hash that includes `Cell.toml`.
- package resolution supports path, git, and registry dependencies; registry
  resolution is pinned by tag and verified against the recorded source hash.
- `CELLSCRIPT_REGISTRY_URL` configures a Git-backed registry, while the default
  registry path remains fail-closed and only falls back to discovery when the
  configured default cannot be cloned.
- `cellc build` records lockfile-bound identity for compiler version, target
  profile, artifact hash, metadata hash, schema hash, ABI hash, and constraints
  hash.
- `cellc package verify` and `cellc registry verify` validate package and
  off-chain deployment identity fail-closed, including JSON mode.
- `tests/registry.rs` and `tests/cli.rs` cover local registry publish/resolve,
  source-hash mismatch rejection, source-root hashing, namespace initialization,
  build lockfile identity, and JSON fail-closed verification.

Current 0.19 status:

| Track | Status | Evidence / boundary |
|---|---|---|
| CKB ecosystem reuse audit | Done for this slice | The audit records the compiler / `ckb-std` / `ckb-sdk-rust` split, ABI-parity cleanup, formal adapter crate, and focused node evidence. |
| `ckb-std` compatibility | Done for this slice | `src/ckb_abi.rs`, `tests/ckb_std_compat.rs`, occupied-capacity field lowering, WitnessArgs fixtures, TYPE_ID checks, since/epoch parity, and `cellc ckb-std-compat --json`. |
| Compiler-to-adapter JSON boundary | Done for this slice | `cellc action build --json` emits the v0.19 adapter contract, packed-materialization requirements, witness policy, required resolved-transaction fields, and acceptance report template. |
| Rust adapter crate | Done for this slice | `crates/cellscript-ckb-adapter` materializes packed CKB transaction shape and adapter evidence with `ckb-sdk-rust`; `examples/ckb-sdk-builder` is a cookbook wrapper. Headless `build_deploy_transaction()` constructs TYPE_ID code-cell deploy transactions; `build_deployment_manifest_from_evidence()` produces manifests from deploy evidence. Full lifecycle bridge: `ManifestCellDepResolver` (CellDepResolver trait), `TransactionSubmitter` (submit + confirm), `SigningAdapter` (signing state tracking), `CapacityBridge` (CapacityBalancer), `TransactionLifecycleEvidence` (end-to-end evidence). `CellScriptAdapter` facade provides `connect()`, `deploy_artifact()`, `build_deploy()`, `submit_transaction()`, `wait_for_commitment()`, `get_transaction_status()`, `estimate_cycles()`, `test_tx_pool_accept()`, and `get_tip_block_number()`. `cellscript-deploy` CLI provides `deploy`, `build-deploy`, `action`, `status`, and `info` subcommands. |
| Focused local-node adapter acceptance | Done for this slice | `scripts/cellscript_ckb_adapter_acceptance.sh` records local CKB `estimate_cycles` and `test_tx_pool_accept` evidence for both action transactions and deploy probe (TYPE_ID code-cell deployment with always_success on devnet). Phase 3 submits the deploy transaction, generates blocks until committed, and verifies the code cell is live with a type script. |
| Focused ecosystem reuse gate | Done for this slice | `./scripts/cellscript_ckb_ecosystem_reuse_gate.sh quick` / `full` cover compatibility, adapter crate, cookbook, and focused node acceptance. |
| Grammar and syntax governance | Done for this slice | Governance docs now include a 0.19 release status matrix; syntax-combo quick/ci/deep emit machine-readable governance and known-bug-class coverage; VS Code grammar/snippets are aligned with `verification`. `assert` keyword removed from action/lock context; only `assert_invariant` retained for invariant declarations. |
| Package manifest and lockfile | Done for Phase 1 | `Cell.toml` source identity, namespace-aware init, registry dependency resolution, and `Cell.lock` build identity are implemented. `cellc package verify` fails closed on mismatched source/build identity. |
| Source package registry | Done for Phase 1 | Git-backed registry records resolve by namespace/name/version/tag/source hash. Local registry fixtures cover publish/resolve, source-root hashing, and mismatch rejection. |
| Deployment identity registry | Done for Phase 1 | `cellc registry verify` validates off-chain deployment facts against build/package identity and fails closed in text and JSON modes. Live-chain `get_live_cell` proof and generated-builder consumption move to 0.20. |
| Generated TypeScript Action Builder | Moved to 0.20 | `cellc gen-builder --target typescript`, CCC integration, and generated-builder tests are now the 0.20 transaction-builder milestone. |
| Stateful flow runner | Moved to 0.20 | Committed local CKB multi-step flows and live-output lineage evidence are now the 0.20 builder evidence milestone. |
| Toolchain one-line installer | Done for this slice | `scripts/install.sh` with multi-mirror fallback (GitHub, ghgo.xyz, gh-proxy.com, ghfast.top), SHA256 verification, PATH setup, and dry-run. `.github/workflows/release.yml` builds 4-platform tarballs and publishes GitHub Releases with install instructions. |
| CellFabric core | 0.20+ exploration | Cross-protocol intent-DAG planning remains outside the 0.19 registry closure and should only become release scope after the per-action builder is proven. |

## P0: Grammar And Syntax Governance

0.19 turns grammar governance from a loose RFC into an executable release
discipline.

The goal is not to add another syntax-cleanup release. The goal is to make sure
future grammar changes preserve CellScript's audit identity:

```text
action shape is visible
Cell movement is visible
verification constraints are visible
stdlib/helper sugar lowers to explicit obligations
metadata exposes the canonical expansion
```

### 1. Governance Contract

Promote the active governance documents into a checked release boundary:

- classify every language surface as core verifier syntax, local explicit
  sugar, stdlib/helper pattern, or deferred/non-core surface;
- keep `action`, `transition`, `verification`, `consume`, `create`,
  `destroy`, `preserve`, `require`, `lock`, and witness/source syntax in one
  documented semantic table;
- make every accepted sugar point to a canonical core expansion;
- make every deferred sugar fail closed with precise diagnostics, not partial
  parser acceptance;
- keep public examples on the canonical style only.

### 2. Syntax Combination Gate

The syntax-combination audit should become reusable release infrastructure:

- weighted grammar generation for accepted and rejected syntax forms;
- pairwise and targeted high-risk combinations across lifecycle operations,
  `require` blocks, stdlib calls, ScriptRef/Script construction, SourceView,
  WitnessArgs, capacity/time helpers, and metadata features;
- oracle checks for parser success/failure, type equivalence, lowering shape,
  metadata obligations, codegen helpers, formatter stability, and LSP grammar;
- mutation tests for known governance bugs such as hidden lifecycle operations
  in pure constraint positions, name-based protocol semantics, duplicate
  lineage consumption, and preserve/type mismatches.

### 3. Execution Plan

| Phase | Work | Evidence |
|---|---|---|
| G0 inventory | Map existing parser, formatter, LSP, examples, docs, and tests to the governance layers. | `docs/CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md` updated with a status matrix. |
| G1 canonical surface | Audit examples and wiki for canonical action / transition / verification wording without changing semantics. | diff-limited docs/examples update plus compile gate. |
| G2 parser/type boundary | Add focused accepted/rejected tests for governance-only syntax boundaries and deferred sugar. | parser/typechecker tests and diagnostics snapshots. |
| G3 lowering/metadata equivalence | Prove sugar lowers to the same obligations as the canonical core form. | IR/metadata/codegen assertions, not only compile success. |
| G4 combo/fuzzy gate | Add a reusable `syntax-combo` gate that runs deterministic seeds and high-risk reject variants. | CI script with seed recording and no active unknown failures. |
| G5 editor/docs sync | Update VS Code grammar, snippets, LSP completions, README/wiki, and release wording from evidence only. | VSIX dry-run or extension grammar test plus docs link check. |

### 4. Non-Goals

- Do not introduce non-canonical action-body sugar in 0.19.
- Do not introduce reusable proof-block syntax.
- Do not hide transaction semantics behind protocol names such as `claim`,
  `transfer`, `swap`, or `settle`.
- Do not claim 0.20 generated-builder support for syntax that is not
  represented in metadata and acceptance evidence.
- Do not treat formatter support as parser/typechecker/lowering support.

0.19 also deepens the package and deployment registry design discussed in the
Nervos forum design thread:

- <https://talk.nervos.org/t/cellscript-package-and-deployment-registry-early-design-discussion/10210/4>

The important registry split is:

```text
source/package registry
  = package identity, source hash, build recipe, metadata, ABI, audit artifacts

deployment registry
  = concrete on-chain script cells, CellDeps, code_hash/hash_type, network,
    artifact hash, metadata hash, and package provenance
```

`Cell.lock` should bind resolved package versions, build artifacts, and
deployment references so generated builders do not silently drift from the
contract artifacts that were audited.

## P0: Package And Deployment Registry Phase 1

### 1. Package Manifest And Lockfile

**Problem**

CellScript packages need stable identity across source, compiler version, build
profile, generated artifact, metadata, and deployment facts. Without a lockfile,
tools can silently consume whatever package index, compiler build, or deployment
fact happens to be resolved at build time.

**Completed Change**

0.19 defines and implements the Phase 1 manifest / lockfile contract:

- namespace, package name, version, explicit entry, and source roots in
  `Cell.toml`;
- source hash computed from `Cell.toml`, configured source roots, and the
  explicit entry parent;
- path, git, and registry dependency resolution through `PackageManager`;
- Git-backed registry records pinned by namespace, name, version, tag, and
  source hash;
- `Cell.lock` build identity for compiler version, target profile, artifact
  hash, metadata hash, schema hash, ABI hash, and constraints hash;
- off-chain deployment facts checked against package/build identity by
  `cellc registry verify`.

**Acceptance**

- `cellc package verify` validates package source/build identity.
- `Cell.lock` records enough information to reproduce the Phase 1 package and
  artifact identity consumed by downstream tools.
- stale or mismatched source, artifact, metadata, or deployment facts fail
  closed in text and JSON modes.

### 2. Source Package Registry

**Problem**

Protocol SDKs need to discover CellScript packages without depending on mutable
repository branches or copied JSON snippets.

**Completed Change**

0.19 adds a minimal off-chain Git registry:

- immutable package-version records under namespace/name/version;
- tag-pinned git source resolution;
- source hash verification after checkout;
- `CELLSCRIPT_REGISTRY_URL` override for local or private registries;
- local fixture coverage for publish, resolve, hash mismatch, and source-root
  hashing.

**Acceptance**

- a local registry fixture can publish, resolve, and verify a package;
- the resolver rejects schema/name/namespace/version/tag/source-hash mismatches;
- source package discovery is documented separately from deployment discovery.

### 3. Deployment Identity Registry

**Problem**

A source package does not by itself prove which build output or deployment facts
downstream tooling should trust.

**Completed Change**

0.19 closes the off-chain deployment-identity layer:

- `cellc build` writes package/build identity into `Cell.lock`;
- `cellc registry verify` compares deployment facts with lockfile identity;
- verification rejects missing lockfile/build facts, hash mismatches, and
  malformed records;
- JSON mode reports success and failure without downgrading errors.

**Acceptance**

- package identity, artifact identity, metadata identity, and deployment facts
  are checked by CLI verification commands;
- deployment identity mismatch fixtures fail closed;
- live-chain proof remains explicitly outside 0.19 Phase 1.

## Deferred To 0.20

The following work was intentionally moved out of 0.19 to keep the release
closed around the Phase 1 registry/provenance contract:

- generated TypeScript Action Builder: `cellc gen-builder --target typescript`,
  CCC integration, generated package tests, and typed action APIs;
- live-chain deployment verification: `get_live_cell`, network-specific
  CellDep solving, stale/deprecated deployment rejection, and builder refusal
  when chain-visible facts disagree with `Cell.lock`;
- stateful flow runner: committed local CKB multi-step flows, old-output to
  new-output lineage, and canonical example evidence;
- registry trust hardening: publisher signatures, trust anchors, mutable
  channels, revocation policy, and optional on-chain registry/index/proxy
  design;
- CellFabric exploration: cross-protocol intent DAG planning only after the
  per-action builder is proven.

## Integration With The Compiler

Compiler work completed by 0.19:

- package source hashing that includes `Cell.toml`, configured source roots,
  and the explicit entry parent;
- package resolution integrated into compile-time dependency loading;
- lockfile-bound compiler version, target profile, artifact, metadata, schema,
  ABI, and constraints hashes;
- fail-closed registry dependency verification before dependency source is
  loaded;
- fail-closed package and deployment identity verification in CLI text and JSON
  modes;
- stable compiler-to-adapter JSON boundary for the Rust-side transaction
  materialization crate.

0.20 owns generator-specific action recipe contracts, TypeScript builder
compatibility checks, and live-chain deployment proof. The compiler should emit
enough metadata for builders to construct transaction shape, but it should not
become a wallet, indexer, or chain submission layer.

## Non-Goals

- Do not replace CCC.
- Do not introduce hidden signer authority or hidden sighash defaults.
- Do not infer transaction semantics from protocol/action names.
- Do not claim generated TypeScript Action Builder support in 0.19.
- Do not claim live-chain deployment verification from off-chain registry
  verification alone.
- Do not claim full CellFabric intent composition in 0.19.
- Do not treat package registry resolution as deployment verification.
- Do not mark a deployment mainnet-certified without external audit and chain
  evidence.
- Do not make local package/build verification a substitute for CKB VM
  acceptance.

## Acceptance Gate

0.19 is accepted by the Phase 1 registry/provenance gate:

```text
cellc init --namespace
cellc publish
cellc install
cellc build
cellc package verify
cellc registry verify
```

Required validation for this release line:

```text
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
./scripts/cellscript_ckb_ecosystem_reuse_gate.sh quick
./scripts/cellscript_ckb_ecosystem_reuse_gate.sh full
```

The registry/provenance tests cover namespace initialization, local Git registry
publish/resolve, registry source-hash mismatch rejection, configured
source-root hashing, compile-time registry dependency loading, build lockfile
identity, package verification, registry verification, and JSON fail-closed
behavior.

The CKB ecosystem reuse gate covers `ckb-std` ABI parity, machine-readable
adapter contracts, the formal `crates/cellscript-ckb-adapter` crate, the
`examples/ckb-sdk-builder` cookbook, witness placement policy, TYPE_ID args
checks, focused local-node adapter acceptance, and formatter/diff hygiene.

0.19 does not claim generated TypeScript builders, wallet UI, CellFabric intent
DAG, external audit, live-chain registry certification, or exhaustive
adversarial state-space verification.

Required report fields:

- package namespace / name / version;
- source hash;
- registry tag;
- compiler version;
- target profile;
- metadata hash;
- artifact hash;
- schema hash;
- ABI hash;
- constraints hash;
- off-chain deployment ref / facts when present;
- verification status and structured error details;
- known limitations.

Representative Phase 1 flows:

- init namespace -> publish to local registry -> install by registry dep ->
  build with locked package identity;
- mutate registry source hash -> resolver rejects before dependency loading;
- mutate source root contents -> package verification fails closed;
- mutate artifact/metadata/deployment facts -> registry verification fails
  closed;
- request JSON verification -> errors remain failing process outcomes and are
  machine-readable.

## Open Questions

Open questions about generated builders, live-chain registry proof, signed
mutable channels, and on-chain registry/index/proxy design move to
[`CELLSCRIPT_0_20_ROADMAP.md`](CELLSCRIPT_0_20_ROADMAP.md).
