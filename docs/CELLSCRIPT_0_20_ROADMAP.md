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

Nineteenth slice: the release gate policy is now explicit. The supported entry
point is `./scripts/cellscript_gate.sh <dev|ci|backend|release|release-quick>`,
with the old `scripts/cellscript_ckb_release_gate.sh` wrapper delegating to it.
`release` means CI, tooling/docs validation, VS Code validation,
builder-backed CKB production acceptance, and stateful scenario/action coverage.
`release-quick` is compile-only preflight evidence and must not be reported as
external live devnet acceptance.

Twentieth slice: the Molecule / IFRN design-space audit has been collapsed into
an English improvement report. The current local source-package conclusion is
that raw cell-data access is expressible and must be declared honestly through
`cell_data_codec_manifest`, while public raw-layout production claims still
need external codec adapters, roundtrip vectors, builder/indexer support,
multi-ABI registry support, and parity matrices.

Twenty-first slice: the registry source-package plan is now closed against the
implementation boundary. `cellc install namespace/pkg@version`, package builds,
and `cellc update` resolve registry CellScript source packages through the
two-tier Git path, verify `registry.json`, skip yanked versions, check
`source_hash`, and write `Cell.lock` identity. `cellc publish` updates local
`registry.json`; `cellc registry add` writes into the actual cloned discovery
worktree and prints correct Git next steps. Remaining registry work is explicitly
outside the source-package resolver: proxy caching, non-CellScript artifact
profiles, cryptographic publisher-signature verification, and auditor key
management.

Twenty-second slice: the language abstraction-cost audit is now wired into
compiler work. Numeric equality no longer treats all numeric widths as equal,
declared integer literal widths propagate through constants, returns,
assignments, calls, arrays, and braced `if` branches, IR lowering preserves those
widths, backend terminator emission removes fall-through jumps, `opt-report`
serialises backend shape and estimated-cycle deltas, and the incremental
parallel compiler uses a real dependency topological sort.

Twenty-third slice: the parent-synthesis follow-up is now reflected in compiler
contracts rather than only prose. Compile diagnostics carry typed severity
(`error` or `warning`), with `ErrorReporter::has_errors()` only treating error
severity as release-blocking. Metadata schema versioning is split into
source/package, artifact, and constraints component versions while preserving
the top-level envelope version; validation, incremental-cache partitioning, CLI
JSON reports, generated-builder ABI hashes, deploy plans, dependency locks, and
audit bundles all surface the split contract. Enum `match` exhaustiveness was
already enforced by type checking; the remaining pattern item is a larger AST
cleanup to replace raw `MatchArm.pattern: String` with structured pattern nodes.

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
- `docs/CELLSCRIPT_GATE_POLICY.md`, `CHANGELOG.md`, and
  `docs/releases/CELLSCRIPT_0_20_RELEASE_NOTES.md` define the current release
  evidence boundary, including compile-only versus local-devnet evidence,
  exact-artifact build reports, ELF entry ABI checks, and codec-manifest
  identity.
- The public Astro website is part of the release communication surface. Recent
  hardening moved the narrative order to explain the core model before install
  steps, collapsed noisy metadata JSON by default, added copy-to-clipboard for
  commands and metadata, fixed keyboard focus/anchor behaviour, made the
  nine-tab model row scroll on mobile, and raised code/link contrast for WCAG
  AA readability.
- Phase 1 registry tutorial entry points are now visible from the wiki sidebar,
  while stale dependency-cache, codegen-refactor, iCKB investigation, and
  error-flow audit documents have been removed or archived so old audit notes do
  not look like current 0.20 status.

Acceptance:

- release validators fail if extension commands/settings drift from the CLI
  builder workflow;
- generated-builder tests run in the tooling gate, not only in Rust unit tests;
- docs describe the exact evidence boundary: generated builders delegate
  live-cell resolution, signing, dry-run, submit, and final CKB acceptance to
  runtime adapters and node-backed gates.
- website changes carry their own build, interaction, accessibility, or
  Playwright smoke evidence; website polish is not protocol evidence.
- the documentation map and wiki navigation point to current 0.20 status
  documents instead of stale audit files.

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

## P1: Proposal-Local Profiles And Design-Space Evidence

Recent 0.20 work also closed several proposal and design-space gates. These are
important release-adjacent evidence, but they must remain labelled as
proposal-local or design-space evidence unless a later milestone promotes them
into general CellScript guarantees.

### NovaSeal

NovaSeal profile certification is now tracked as a compiler-hosted proposal
gate through `cellc certify --plugin novaseal-profile-v0`. The current surface
includes the core v0 skeleton, Agreement, fungible xUDT, RWA receipt,
BTC transaction commitment, BTC UTXO seal, dual seal, and Fiber candidate
profiles.

The local/devnet evidence chain covers:

- live local devnet stateful reports for core and Agreement flows;
- planned-profile live devnet reports for the BTC, dual-seal, Fiber,
  fungible-xUDT, and RWA surfaces;
- BIP340 verifier IPC/vector/shell evidence and CKB VM parent/child harnesses;
- wallet signing vectors and wallet-lock alignment;
- profile-operator, service-builder, BTC SPV adapter, external-attestation
  adapter, and external-evidence handoff reports;
- public/shared CellDep, external TCB, public BTC SPV, and RWA legal/registry
  templates for later public evidence.

The current claim is local source-package and local/devnet readiness for the
proposal gates that pass. Public testnet/mainnet or ecosystem production claims
still require independent public CellDep, external BIP340 TCB, public BTC SPV,
and RWA legal/registry attestations where those profiles rely on them.

### Evolving DOB

The `proposals/evolving-dob/evolving-dob-profile-v1` package is now part of the
0.20 proposal evidence surface. It carries `Cell.toml`, `Cell.lock`,
`Deployed.toml`, registry metadata, schemas, fixtures, ProofPlan/invariant
records, a devnet workflow script, and a registry-pressure script.

The DOB-EVO audit records the profile as structurally coherent for its
state-transition specification after the genesis owner-lock check and tooling
fixes, while keeping unresolved fixture-depth and public-promotion items visible:
missing negative coverage for many guards, invariant matrix references,
`released_at` regeneration, action-salt hardening, and minimum CKB version
policy for `data1`.

### Molecule, IFRN, Raw Layout, And iCKB

The 0.20 design-space closure keeps CellScript's first-class public path as
Molecule-native typed cells, metadata, audit evidence, provenance, and builder
identity. Raw cell-data layouts and IFRN-style contracts are possible through
`ckb::cell_data_*`, `ckb::input_out_point_*`, packed-hash helpers, and
`lock_args`, but production use requires explicit codec manifests and external
adapter evidence.

iCKB remains a benchmark and differential-evidence surface under
`tests/benchmarks`, not a public bundled example or production-equivalence
claim. The refreshed committed matrix is evidence for the benchmark surface; it
does not promote iCKB compatibility to the 0.20 release boundary.

Acceptance:

- proposal-local gates must declare their scope, report schema, profile id, and
  live/devnet/public evidence boundary;
- NovaSeal public-production status remains blocked until required external
  attestations are real and current;
- DOB-EVO public-promotion status remains blocked until fixture depth and
  deployment-policy follow-ups are closed;
- raw-layout users must carry codec manifest identity and roundtrip/adapter
  evidence before builders or registries claim byte-for-byte materialisation;
- iCKB reports must remain labelled as benchmark/differential evidence unless a
  later roadmap explicitly promotes production-equivalence closure.

## P1: Registry Trust Hardening

0.20 should decide which trust features are part of the transaction-builder
release and which belong to a later distribution/security milestone.

Candidates:

- publisher signatures;
- trust anchors;
- signed mutable channels such as `latest` and `stable`;
- revocation and deprecation policy;
- audit-report and acceptance-evidence pointers;
- optional on-chain registry, index, or proxy design;
- profile compatibility rules for future non-CellScript artifacts, including
  reproducible CKB binaries, verifier artifacts, deployment records, and
  scaffold-only templates.

Acceptance should remain fail-closed: missing or unsupported trust metadata must
not silently downgrade into name-only package or deployment resolution.
For 0.20 this is a metadata-presence gate only: publisher signature
cryptographic verification and trust-anchor management remain a later security
milestone.

Profile compatibility is a documentation and schema-reservation concern for
0.20. Current `cellc` package resolution remains scoped to CellScript source
packages. Future generic artifact profiles may share the registry service and
`namespace/name` naming style, but they must carry explicit profile identity in
lockfiles and must not be installable through the CellScript dependency resolver
until profile-specific fail-closed checks exist.

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
- Do not treat future generic artifact discovery as current CellScript package
  dependency support.
- Do not treat NovaSeal or DOB proposal-local devnet/profile evidence as a
  generic CellScript mainnet certification claim.
- Do not treat raw-layout expressibility as builder/indexer codec support
  without adapter identity, roundtrip vectors, and manifest hashes.
- Do not treat public website polish or documentation navigation as protocol,
  compiler, or CKB VM evidence.
- Do not mark a deployment mainnet-certified without external audit and chain
  evidence.
- Do not make builder success a substitute for CKB VM / tx-pool acceptance.
- Do not claim cross-protocol CellFabric intent composition in the per-action
  builder release.
- Do not treat frozen CellFabric exploration as a 0.20 acceptance blocker.

## Acceptance Gate

The full 0.20 gate should include:

```text
./scripts/cellscript_gate.sh release
cellc package verify
cellc registry verify --live
cellc gen-builder --target typescript
npm test for generated builders
ELF entry ABI gate before devnet dry-run, tx-pool, and submit evidence
local CKB dry-run for generated action transactions
local CKB submitted stateful flows for canonical examples
negative builder-shape rejection fixtures
deployment registry mismatch rejection fixtures
cellc certify --plugin novaseal-profile-v0 when NovaSeal proposal evidence is claimed
Evolving DOB devnet workflow and registry-pressure checks when DOB proposal evidence is claimed
website build and interaction/a11y smoke checks when website files change
```

Required report fields:

- gate mode and report schema;
- package namespace / name / version;
- source hash;
- metadata hash;
- artifact hash;
- cell_data_codec_manifest_hash when cell-data codec identity is present;
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
- proposal profile id and proposal-local evidence boundary when proposal
  reports are included;
- website build/smoke evidence when public website changes are included;
- known limitations.
