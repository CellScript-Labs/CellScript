# CellScript 0.20 Roadmap

**Status**: In progress
**Scope**: Generated Action Builder, live-chain deployment verification,
stateful transaction flows, and registry trust hardening
**Depends on**: 0.19 Phase 1 package / deployment identity registry closure

## Goal

CellScript 0.20 should turn the 0.19 registry/provenance layer into a
transaction-building layer that can prove which source package, build artifact,
metadata record, and on-chain script cell a transaction used.

0.19 closed the local and Git-backed identity loop:

```text
Cell.toml -> registry record -> checked-out source -> Cell.lock
           -> package verify -> registry verify
```

0.20 should consume that identity from generated builders and live-chain
verification:

```text
Cell.lock + metadata + deployment facts
  -> generated per-action builder
  -> CCC / ckb-sdk-rust materialization
  -> dry-run / tx-pool / submission evidence
  -> live-cell and lineage verification
```

## P0: Generated Action Builder

CellScript Action Builder turns one CellScript action into one valid CKB
transaction candidate.

Current first slice on the 0.20 branch: `cellc gen-builder --target typescript`
emits a compiling TypeScript package scaffold from compiler metadata, including
typed action parameters, action-plan functions, runtime adapter contracts,
builder manifest, embedded metadata, and explicit non-claims for live-chain
availability, signing, submission, and CKB VM acceptance.

Second slice: `cellc gen-builder --lockfile Cell.lock` now verifies source hash
and locked build identity before writing the package. Generated TypeScript also
exports `validateCellScriptLockfile` / `assertCellScriptLockfile`, and action
planning fails closed when callers provide a mismatched lockfile.

Third slice: `cellc registry verify --live --rpc-url ...` adds CKB RPC-backed
deployment checks for `get_blockchain_info` and `get_live_cell`, including
chain-id matching, live-cell status, data hash, code hash, and Type ID args
where declared.

Fourth slice: `cellc gen-builder --deployed Deployed.toml --lockfile Cell.lock`
binds generated packages to deployment identity. Generated TypeScript now
exports deployment/live-evidence validators and rejects action plans when
provided deployment records, lockfile refs, or live evidence disagree.

Fifth slice: generated TypeScript packages now ship `npm test` and
`test/builder.test.mjs` self-tests covering plan generation, runtime adapter
delegation, and fail-closed lockfile/deployment mismatch cases.

Target CLI:

```text
cellc gen-builder --target typescript --metadata target/.../metadata.json
```

The generated package should provide:

- typed action functions;
- typed live-cell inputs;
- typed literal and witness parameters;
- explicit dry-run and submit modes;
- returned tx plan, signed tx, submitted tx hash, and lineage records;
- structured error mapping from compiler/runtime codes to action fields.

Core responsibilities:

| Module | Responsibility |
|---|---|
| `metadata-loader` | Load compiler metadata, ABI, ProofPlan, and builder-facing action recipes. |
| `registry-client` | Resolve package and deployment records, then verify hashes against `Cell.lock`. |
| `cell-resolver` | Query live cells through CCC/indexer adapters and apply typed binding rules. |
| `recipe-engine` | Turn one action recipe into required inputs, outputs, witnesses, deps, and assumptions. |
| `output-builder` | Construct continuation and created outputs from transition, preserve, and create metadata. |
| `witness-builder` | Encode action selector, witness ABI, signer slots, and WitnessArgs fields. |
| `tx-planner` | Compute capacity floors, fee/change policy, HeaderDeps, CellDeps, and ordering. |
| `preflight` | Run metadata validation, local shape checks, and CKB dry-run before signing. |
| `ccc-adapter` | Delegate low-level transaction composition, signing, RPC, and indexer calls to CCC. |
| `ckb-sdk-adapter` | Keep Rust-side deploy/action materialization aligned with the generated builder contract. |
| `state-tracker` | Track committed outpoints and make follow-up action calls consume the new live outputs. |

Acceptance:

- generated TypeScript package compiles under the supported package manager;
- generated-builder tests cover valid build/dry-run flows and negative
  builder-shape rejection;
- generated builders refuse packages, metadata, or deployment records that do
  not match `Cell.lock`;
- CCC remains the low-level wallet, signing, RPC, and indexer boundary.

## P0: Live-Chain Deployment Verification

0.19 verifies off-chain deployment identity. 0.20 should verify chain-visible
deployment facts.

Required checks:

- network and chain id;
- script role: lock, type, dual-role, or helper dependency;
- tx hash, output index, and CellDep shape;
- code_hash and hash_type;
- script reference or dep-group metadata where applicable;
- Type ID and upgrade lineage where applicable;
- artifact hash, metadata hash, source hash, and build manifest hash;
- deployment status: local, testnet, mainnet candidate, deprecated, or revoked.

Acceptance:

- `cellc registry verify` can call CKB RPC / indexer APIs to confirm
  `get_live_cell` and data-hash facts;
- stale, wrong-network, wrong-code-hash, missing-CellDep, and deprecated
  deployment fixtures fail closed;
- generated builders refuse to construct transactions when live deployment
  verification disagrees with package/build identity;
- local devnet tests submit and confirm representative deployment cells.

## P1: Stateful Flow Runner

After single-action builders work, 0.20 should add a stateful flow runner for
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

Representative flows should include:

- Token: mint -> transfer -> invalid overspend rejected;
- Timelock: create -> early spend rejected -> valid spend accepted;
- NFT: mint -> list -> buy -> invalid payment rejected;
- AMM: create pool -> add liquidity -> swap -> remove liquidity;
- Multisig: propose -> threshold approve -> execute -> insufficient approvals
  rejected;
- Vesting: grant -> claim/revoke -> early claim and invalid revoke rejected;
- Registry: package resolve -> deployment resolve -> stale deployment rejected.

## P1: Registry Trust Hardening

0.20 should decide which trust features are part of the transaction-builder
release and which belong to a later distribution/security milestone.

Candidates:

- publisher signatures;
- trust anchors;
- signed mutable channels such as `latest` and `stable`;
- revocation and deprecation policy;
- audit-report and acceptance-evidence pointers;
- optional on-chain registry, index, or proxy design.

Acceptance should remain fail-closed: missing or unsupported trust metadata must
not silently downgrade into name-only package or deployment resolution.

## P2: CellFabric Exploration

CellFabric remains a later target:

```text
intent -> action DAG -> UTXO graph -> CKB transactions
```

0.20 may sketch interfaces for intent schemas, action-DAG planning,
multi-transaction batching, live-cell conflict detection, and planner evidence,
but it should not claim full CellFabric support until the per-action builder and
stateful flow runner are proven.

## Non-Goals

- Do not replace CCC.
- Do not make generated builders infer protocol semantics from names such as
  `claim`, `swap`, or `mint`.
- Do not treat package registry resolution as deployment verification.
- Do not mark a deployment mainnet-certified without external audit and chain
  evidence.
- Do not make builder success a substitute for CKB VM / tx-pool acceptance.
- Do not claim cross-protocol CellFabric intent composition in the per-action
  builder release.

## Acceptance Gate

The full 0.20 gate should include:

```text
cellc package verify
cellc registry verify --live
cellc gen-builder --target typescript
npm test for generated builders
local CKB dry-run for generated action transactions
local CKB submitted stateful flows for canonical examples
negative builder-shape rejection fixtures
deployment registry mismatch rejection fixtures
```

Required report fields:

- package namespace / name / version;
- source hash;
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
