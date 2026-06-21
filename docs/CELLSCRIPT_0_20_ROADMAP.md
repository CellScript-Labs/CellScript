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

Sixth slice: generated builders now expose explicit `build`, `dry-run`, and
`submit` result modes. Dry-run and submit remain delegated to the runtime
adapter, submit forces a dry-run first, and generated self-tests cover missing
adapter and malformed runtime-shape rejection.

Seventh slice: generated builders now export the stable runtime error catalog
and helpers that map runtime codes, names, messages, or runtime error objects
back to action field context, witness/source metadata, and runtime requirements.

Eighth slice: the VS Code extension now exposes the action-builder workflow
through compiler-backed commands for entry-witness ABI, action build plans,
TypeScript builder generation, package verification, registry verification, and
live registry verification. The tooling release gate now runs
`check_action_builder_toolchain`, which generates a TypeScript builder package
from `examples/token`, installs its local dependencies, and runs generated
`npm test`.

Ninth slice: live deployment verification and generated-builder deployment
binding now treat deployment status as part of the fail-closed identity
boundary. `deprecated`, `revoked`, and other non-`active` deployment records
plus missing deployment statuses fail `cellc registry verify --live`, are
reported in live evidence, and are rejected by generated TypeScript deployment
validators.

Tenth slice: registry trust hardening now has an explicit metadata-presence
policy. `cellc registry verify --require-publisher-signature
--require-audit-report` fails closed when deployment records omit
`publisher_signature` or `audit_report_hash`, and generated TypeScript builders
expose the same opt-in trust policy without claiming cryptographic signature
verification.

Eleventh slice: the VS Code extension mirrors the trust metadata policy through
settings that add the registry trust flags to both offline and live registry
verification commands, and the tooling validator now guards those settings.

Twelfth slice: generated-builder self-tests now exercise trust-policy rejection
even when no deployment binding is embedded, so the release-gate generated
package catches accidental downgrades in the opt-in trust surface.

Thirteenth slice: the quick release gate now statically protects the stateful
scenario acceptance boundary by checking that the production gate still invokes
`--stateful-scenarios` and that the acceptance script still records live/dead
lineage, tx-size, occupied-capacity, and stateful coverage evidence.

Fourteenth slice: `cellc action build --fabric-intent` now emits a
`cellscript-cellfabric-intent-envelope-v0.20` JSON bridge for parent-project
CellFabric services. The envelope embeds the original action plan, hashes it,
maps CellScript metadata into a CellFabric intent template, and explicitly
states that it is not a signed intent, soft confirmation, live-cell proof,
tx-pool proof, or L1 finality claim. The detailed boundary is documented in
`docs/CELLSCRIPT_CELLFABRIC_BRIDGE.md`.

Target CLI:

```text
cellc gen-builder --target typescript --metadata target/.../metadata.json
cellc action build --fabric-intent --json
cellc package verify --json
cellc registry verify --live --rpc-url ... --json
```

Fifteenth slice: `scripts/cellscript_cellfabric_bridge_smoke.sh` now performs a
bounded cross-repo smoke check against sibling CellFabric. It generates a real
CellScript envelope, runs CellFabric's `cellscript_flow` example, and verifies
schema, import status, action, namespace, payload hash, strict gateway
submission, validated bundle selection, non-final soft confirmation, and the
external-settlement-builder boundary without making CellScript depend on the
CellFabric Rust crate.

Sixteenth slice: the CKB/devnet acceptance path now has an ELF entry ABI gate.
Every CKB RISC-V ELF compiled by `scripts/ckb_cellscript_acceptance.sh` is
audited before local-node dry-run, tx-pool acceptance, or submitted stateful
flows. The gate fails closed unless the executable `PT_LOAD` segment is
RX-only, has `filesz == memsz`, and the entry trampoline calls the real entry
without initialising `sp`. The critical 0.20 example path explicitly requires
passing ABI evidence for `launch.cell`, `token.cell`, and `amm_pool.cell`.

Seventeenth slice: production acceptance now emits exact-artifact
`CellScriptBuildReport` rows. Each row binds a CKB-deployable ELF to its CKB
blake2b deployable hash, host SHA-256 hash, `cellc verify-artifact` status, ELF
entry ABI gate status, ABI-trailer stripped state, and live code-cell data hash
when devnet deployment evidence is available. The production evidence validator
fails closed when a live deployment's code-cell data hash does not match the
compiled artifact hash.

Eighteenth slice: compile metadata now separates the CKB runtime VM ABI from
cell-data codec truth. Molecule-native contracts still declare
`cell_data_codec_manifest.abi = "molecule"`, while contracts that use raw
`LOAD_CELL_DATA` accesses declare `abi = "molecule+raw-bytes-v1"` and enumerate
the raw accesses. Generated TypeScript builders export this manifest in the
builder manifest and action plans, but still delegate raw cell-data
materialisation to runtime adapters.

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
- VS Code extension commands map directly to the same CLI boundaries and do not
  invent a parallel compiler or builder implementation;
- `scripts/cellscript_ckb_release_gate.sh` keeps the generated-builder
  scaffold, build, and self-tests in the local tooling gate through
  `check_action_builder_toolchain`;
- CCC remains the low-level wallet, signing, RPC, and indexer boundary.

## P0: Tooling And Documentation Sync

The 0.20 tooling surface must stay aligned across CLI, generated packages, VS
Code, release scripts, and docs.

Current status:

- `scripts/validate_cellscript_tooling_release.py` checks VS Code command
  contributions, activation events, generated-builder settings, extension
  runtime wiring, README/wiki references, and 0.20 roadmap tokens.
- `scripts/cellscript_ckb_release_gate.sh` validates the VS Code extension,
  packages a local VSIX, generates a TypeScript action-builder package, installs
  it, and runs its generated `npm test` suite.
- `docs/wiki/Tutorial-07-LSP-and-Tooling.md` documents the editor commands,
  package/registry verification commands, and generated-builder test loop.
- `README.md` lists `cellc action build`, `cellc gen-builder --target
  typescript`, `cellc package verify`, and `cellc registry verify --live` as
  first-class tooling surfaces.

Acceptance:

- release validators fail if extension commands/settings drift from the CLI
  builder workflow;
- generated-builder tests run in the tooling gate, not only in Rust unit tests;
- docs describe the exact evidence boundary: generated builders delegate
  live-cell resolution, signing, dry-run, submit, and final CKB acceptance to
  runtime adapters and node-backed gates.

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
For 0.20 this is a metadata-presence gate only: publisher signature
cryptographic verification and trust-anchor management remain a later security
milestone.

## P2: CellFabric Exploration (Frozen Except Bridge)

CellFabric is frozen for the 0.20 acceptance pass beyond the bounded
`cellc action build --fabric-intent` JSON bridge. Deeper integration remains a
later target:

```text
intent -> action DAG -> UTXO graph -> CKB transactions
```

Do not add generated code, additional CLI flags, direct Rust crate coupling,
fixture requirements, or release claims for signed CellFabric intents,
action-DAG planning, multi-transaction batching, live-cell conflict detection,
or planner evidence in 0.20. Revisit only after the per-action builder,
stateful flow runner, and bridge envelope have real service-side consumers.

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
- Do not treat frozen CellFabric exploration as a 0.20 acceptance blocker.

## Acceptance Gate

The full 0.20 gate should include:

```text
cellc package verify
cellc registry verify --live
cellc gen-builder --target typescript
npm test for generated builders
ELF entry ABI gate before devnet dry-run, tx-pool, and submit evidence
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
- ELF entry ABI gate result, including stack-pointer preservation and RX-only
  executable segment evidence;
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
