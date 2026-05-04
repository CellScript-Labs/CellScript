# CellScript 0.19 Roadmap

**Status**: Planning
**Scope**: Package registry, deployment registry, and CellScript Action Builder
**Depends on**: v0.17 CKB protocol semantics and v0.18 first-class ScriptRef /
ScriptArgs work

## Goal

CellScript 0.19 should turn compiler artifacts into a reproducible package,
deployment, and transaction-building layer.

The compiler already emits metadata, ABI records, ProofPlan records, CKB target
profile data, and production evidence reports. 0.19 should make those artifacts
usable by wallets, dapps, indexers, deployment tools, and protocol SDKs without
forcing each protocol team to hand-write ad hoc transaction builders.

The target stack is:

```text
CellScript compiler
  -> action metadata / ABI / transaction recipe

CellScript Action Builder
  -> reads metadata
  -> selects live cells
  -> constructs expected outputs
  -> fills witness selector / args
  -> tracks old live output -> new live output
  -> asks CCC to build/sign/submit transaction

CCC
  -> low-level transaction composition
  -> wallet connector
  -> signing
  -> RPC / indexer interaction
```

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

## Relationship To CellFabric

CellScript Action Builder and CellFabric are on the same product spectrum, but
they are not the same release target.

```text
CellScript Action Builder
  = per-protocol / per-action transaction builder

CellFabric
  = cross-protocol intent composition + UTXO generation layer
```

Layering:

```text
User intent
   |
CellFabric
   | chooses actions and connects outputs to inputs
CellScript Action Builders
   | construct each action-shaped transaction or transaction node
CCC
   | build / sign / submit
CKB
```

0.19 should ship the Action Builder layer first. It is the smallest useful
kernel of CellFabric, not a full intent planner.

## P0: Package And Deployment Registry

### 1. Package Manifest And Lockfile

**Problem**

CellScript packages need stable identity across source, compiler version, build
profile, generated artifact, metadata, and deployment. Without a lockfile,
Action Builder output can depend on whatever package index, compiler build, or
deployment registry entry happens to be resolved at build time.

**Change**

Define a package manifest and lockfile contract that records:

- package name, namespace, version, and semver channel;
- package source hash and source archive digest;
- compiler version and primitive compatibility mode;
- target profile and build flags;
- dependency graph with locked package versions;
- generated artifact hashes;
- generated metadata and ABI hashes;
- action recipe schema version;
- audit bundle or ProofPlan report hash when available;
- deployment registry references per network;
- publisher signatures or trust anchors where supported.

**Acceptance**

- `cellc package verify` can validate package metadata against source and build
  artifacts.
- `Cell.lock` records enough information to reproduce builder input metadata.
- stale or mismatched artifact/metadata/deployment hashes fail closed.

### 2. Source Package Registry

**Problem**

Protocol SDKs need to discover CellScript packages and action ABIs without
depending on mutable repository branches or copied JSON snippets.

**Change**

Add a registry client and registry schema for off-chain source packages:

- immutable package-version records;
- content-addressed source archives;
- dependency metadata and compatibility constraints;
- action ABI and metadata index entries;
- release notes, license, and documentation pointers;
- optional audit report and acceptance evidence pointers;
- publisher identity and signature metadata.

**Acceptance**

- a local registry fixture can publish, resolve, and verify a package;
- the resolver rejects hash mismatches, missing ABI records, and incompatible
  metadata schema versions;
- README and docs distinguish package discovery from deployment discovery.

### 3. Deployment Registry

**Problem**

A source package does not tell a builder which on-chain script cell to use on a
specific network. Deployment truth must be indexed by chain-visible data, not by
package names alone.

**Change**

Define deployment registry records for:

- network and chain id;
- script role: lock, type, dual-role, or helper dependency;
- tx hash, output index, and CellDep shape;
- code_hash and hash_type;
- script reference or dep group metadata where applicable;
- Type ID and upgrade lineage where applicable;
- generated artifact hash and metadata hash;
- package source hash and build manifest hash;
- accepted/rejected fixture evidence pointers;
- deployment status: local, testnet, mainnet candidate, deprecated, revoked.

**Acceptance**

- `cellc deploy-plan`, `cellc verify-deploy`, and `cellc lock-deps` can emit or
  verify deployment registry records;
- Action Builder refuses to build a transaction when the deployment record does
  not match the package metadata it consumed;
- registry fixtures cover wrong network, wrong code hash, stale metadata hash,
  missing CellDep, and deprecated deployment rejection paths.

## P0: CellScript Action Builder Architecture

### 1. Scope

CellScript Action Builder turns one CellScript action into one valid CKB
transaction candidate.

Example target API:

```ts
await amm.swapAForB({
  pool: livePool,
  inputToken: userTokenA,
  minOutput,
  to,
});
```

Internally, the builder should:

- read action metadata, ABI, transition declarations, ProofPlan records, and
  builder assumptions;
- resolve package and deployment records;
- select live cells from CCC/indexer adapters;
- bind action parameters to live inputs, reference inputs, witnesses, and
  literal values;
- construct expected output cells for `transition`, `preserve`, and `create`;
- encode action selector, witness args, and typed parameters;
- assemble CellDeps, HeaderDeps, outputs, outputs_data, and witnesses;
- estimate occupied capacity, fees, and change outputs;
- dry-run the transaction and map failures back to action metadata;
- submit through CCC when the caller requests submission;
- record old live output -> new live output lineage.

### 2. Core Modules

The first implementation should be split by responsibility:

| Module | Responsibility |
|---|---|
| `metadata-loader` | Load and validate compiler metadata, ABI, ProofPlan, and action recipes. |
| `registry-client` | Resolve package and deployment records, then verify hashes against `Cell.lock`. |
| `cell-resolver` | Query live cells through CCC/indexer adapters and apply typed binding rules. |
| `recipe-engine` | Turn one action recipe into required inputs, outputs, witnesses, deps, and assumptions. |
| `output-builder` | Construct continuation and created outputs from transition, preserve, and create metadata. |
| `witness-builder` | Encode action selector, witness ABI, signer slots, and WitnessArgs fields. |
| `tx-planner` | Compute capacity floors, fee/change policy, HeaderDeps, CellDeps, and ordering. |
| `preflight` | Run metadata validation, local shape checks, and CKB dry-run before signing. |
| `ccc-adapter` | Delegate low-level transaction composition, signing, RPC, and indexer calls to CCC. |
| `state-tracker` | Track committed outpoints and make follow-up action calls consume the new live outputs. |

### 3. Builder Contract Types

The metadata schema should expose stable builder-facing records:

```text
ActionAbi
ActionRecipe
ActionSelector
CellBinding
ReadBinding
WitnessBinding
TransitionEdge
ConsumedInput
DestroyedInput
CreatedOutput
PreserveProof
CapacityPolicy
FeePolicy
ChangePolicy
DeploymentRef
BuilderAssumption
DryRunEvidence
SubmittedTxEvidence
LiveOutputLineage
```

The builder must not infer protocol semantics from names such as `claim`,
`swap`, or `mint`. It should consume compiler-emitted recipes and fail closed
when the recipe does not explain required cells, outputs, witness fields, or
deployment references.

### 4. Generated TypeScript Surface

0.19 should add a TypeScript-first generator:

```text
cellc gen-builder --target typescript --metadata target/.../metadata.json
```

The generated package should provide:

- typed action functions;
- typed live-cell inputs;
- typed literal/witness parameters;
- explicit dry-run and submit modes;
- returned tx plan, signed tx, submitted tx hash, and lineage records;
- structured error mapping from compiler/runtime codes to action fields.

The generated layer should remain thin. CCC stays responsible for low-level CKB
transaction composition, wallet integration, signing, RPC, and indexer access.

## P1: Stateful Flow Runner

After single-action builders work, 0.19 should add a stateful flow runner for
example and test workflows:

```text
tx1 output -> tx2 input -> tx3 input
```

Supported workflows:

- select the live output produced by a previous action;
- prove that the old output is dead and the new output is live;
- run canonical business examples as committed local CKB flows;
- preserve cycles, tx size, capacity, fee, witness, and outpoint-lineage
  evidence per step;
- reject malformed flows before signing when metadata already proves the shape
  impossible.

This is still not full CellFabric. It is action-builder evidence plus linear
state tracking for representative protocol flows.

## P2: CellFabric Core Exploration

CellFabric should remain an explicit later target:

```text
intent -> action DAG -> UTXO graph -> CKB transactions
```

P2 exploration may define:

- intent schemas;
- cross-protocol action selection;
- resource routing;
- DAG dependency resolution;
- multi-transaction batching and splitting;
- live-cell conflict detection;
- registry-backed protocol discovery;
- planner evidence and failure explanations.

0.19 should not claim this as shipped unless the builder can compose multiple
protocols without hidden assumptions.

## Integration With The Compiler

Compiler work required by 0.19:

- stable action recipe schema;
- stable ABI versioning for action selectors and witness args;
- metadata for `transition`, `consume`, `destroy`, `create`, and `preserve`;
- explicit builder assumptions for capacity, fees, change, witnesses, deps, and
  HeaderDeps;
- source spans for builder-facing diagnostics;
- canonical metadata hashes for registry and lockfile validation;
- compatibility checks between metadata schema version and generated builder
  version.

The compiler should emit enough metadata for a builder to construct the
transaction shape, but it should not become a wallet, indexer, or chain
submission layer.

## Non-Goals

- Do not replace CCC.
- Do not introduce hidden signer authority or hidden sighash defaults.
- Do not infer transaction semantics from protocol/action names.
- Do not claim full CellFabric intent composition in the Action Builder release.
- Keep Action Builder core headless; higher-level SDK, wallet UI, and dapp-framework packages may grow as ecosystem work with community or foundation developer help.
- Do not treat package registry resolution as deployment verification.
- Do not mark a deployment mainnet-certified without external audit and chain
  evidence.
- Do not make builder success a substitute for CKB VM acceptance.

## Acceptance Gate

0.19 should add a dedicated builder and registry gate:

```text
cellc package verify
cellc registry verify
cellc gen-builder --target typescript
npm test for generated builders
local CKB dry-run for generated action transactions
local CKB submitted stateful flows for canonical examples
negative builder-shape rejection fixtures
deployment registry mismatch rejection fixtures
```

Required report fields:

- package hash;
- metadata hash;
- artifact hash;
- deployment ref;
- action selector;
- input and output bindings;
- witness layout;
- CellDeps and HeaderDeps;
- cycles;
- serialized transaction size;
- occupied capacity;
- fee and change policy;
- dry-run exit code;
- submitted tx hash when run in submit mode;
- old output -> new output lineage;
- known limitations.

Representative flows should include at least:

- Token: mint -> transfer -> invalid overspend rejected;
- Timelock: create -> early spend rejected -> valid spend accepted;
- NFT: mint -> list -> buy -> invalid payment rejected;
- AMM: create pool -> add liquidity -> swap -> remove liquidity;
- Multisig: propose -> threshold approve -> execute -> insufficient approvals
  rejected;
- Vesting: grant -> claim/revoke -> early claim and invalid revoke rejected;
- Registry: package resolve -> deployment resolve -> stale deployment rejected.

## Open Questions

- Should the package registry be purely content-addressed, or should it also
  support signed mutable channels such as `latest` and `stable`?
- Should deployment registry records live only off-chain at first, or should
  CellScript define a canonical on-chain registry script later?
- How much output construction should be generated per action versus delegated
  to handwritten protocol SDK code?
- Which CCC APIs should be treated as stable enough for generated builders?
- Should `cellc gen-builder` generate one protocol package or one builder
  package per deployment network?
