# CellScript 0.16 Release Notes Draft

Status: devnet-accepted branch draft for `nightly-0.16`.

Updated: 2026-06-10.

CellScript 0.16 turns the v0.15 ProofPlan audit surface into a metadata
assurance toolchain. The release adds operational semantics, ProofPlan
soundness checks, stable builder assumption metadata, transaction-shape
validation, deployment and audit reports, standard CKB compatibility
descriptive fixtures, and CKB stdlib protocol module schema stubs.

The freeze gate also includes the compiler-hardening work from the Rust
comparative audit: IR poison rejection after recorded lowering errors,
instruction-level IR provenance, reserved-register contract verification,
checked syscall ABI baselines, and line-exact diagnostic regression tests.

General production-completeness items such as executable CKB VM fixture
execution for the standard compatibility suite, full transaction solving,
source-to-assembly maps, and protocol stdlib implementations are deliberately
deferred to 0.17. This does not negate proposal-local evidence such as
NovaSeal's local CKB VM, full transaction harness, live devnet, profile, Fiber,
and certification runs.

## Highlights

### Operational Semantics

The new spec is `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md`. It defines the
meaning of expression evaluation, linear resource states, branch merge rules,
Cell effects, triggers, scopes, ProofPlan fields, and builder assumptions.

Conformance is tied to `tests/v0_16.rs`.

**Note**: The semantics document is mechanically precise prose with rule
notation, not an executable/formally verified reference.

### ProofPlan Soundness

Metadata now includes:

```json
runtime.proof_plan_soundness
```

The checker rejects:

- verifier obligations with no matching ProofPlan record;
- on-chain checked records whose codegen coverage is not `covered`;
- runtime-required records marked as on-chain checked;
- `lock_args` provenance mixed into witness reads;
- local action/function/lock ProofPlan records that diverge from
  `runtime.proof_plan`;
- metadata-only/runtime-required gaps in `--primitive-strict=0.16` mode.

Nested packed receipt/resource guards now satisfy strict generated ProofPlan for
the NovaSeal proposal-local bundle. PP0201 is scoped to executing
`Script.args` fields only, so it no longer reports output lock args.

**Note**: This is a metadata consistency checker, not a formal proof.

### NovaSeal Devnet And Profile Certification

The `nightly-0.16` branch also carries the NovaSeal proposal-local acceptance
bundle. The current local acceptance boundary is:

```bash
./scripts/novaseal_devnet_stateful_acceptance.sh --pretty
target/debug/cellc certify --plugin novaseal-profile-v0 --repo-root . --json
```

The devnet wrapper writes
`target/novaseal-devnet-stateful-acceptance.json`. A current commit can only
claim local NovaSeal devnet acceptance when the freshly regenerated report
prints:

```text
status=passed
live_devnet_rpc_executed=true
local_blockers=0
acceptance_blockers=0
blockers=0
```

Historical reports from older commits are not current evidence after source or
certification-gate changes. The certification report must return
`status: "passed"` with
`certification_level: "public_ecosystem_profile_certification_local_ready"`.
It also keeps the production boundary honest:

```text
production_ready=false
v1_status=local_v1_ready_external_attestation_required
```

Production remains blocked by external BIP340 TCB review, public BTC SPV
evidence, public/shared CellDep attestation, and RWA legal/registry review.
Those blockers are intentional; a green devnet/local certification result is
not a mainnet production statement.

The BTC SPV evidence path now requires concrete transaction evidence. The public
BTC report must satisfy the current handoff bundle and the certification code
recomputes or checks:

- live CKB transaction and report hashes for each BTC-facing case;
- service-builder case, transaction-skeleton, and receipt-binding hashes;
- CKB-side BTC commitment hashes;
- raw Bitcoin transaction `txid`/`wtxid` consistency;
- profile-specific transaction bindings for BTC transaction output, sealed
  UTXO spend, or dual-seal closure;
- Bitcoin block-header hash, Merkle root, Merkle branch orientation, observed
  confirmation heights, and canonical SPV material hash.

Metadata-only or unrelated BTC evidence is rejected by the local certification
gate before any production claim can be made.

### Builder Assumption Contract

Metadata now includes:

```json
runtime.builder_assumptions
```

Each assumption has a stable schema:

```text
assumption_id
kind
origin
feature
required_inputs
required_outputs
required_cell_deps
required_witness_fields
capacity_policy
fee_policy
change_policy
signature_policy
failure_mode
```

`cellc explain-assumptions --json` prints the schema for a source package.

**Note**: `validate-tx` performs structural and schema-bound evidence
validation, not full CKB transaction semantic verification. Non-structural
assumption evidence must bind to the assumption id, kind, origin, feature, and
ProofPlan status and include a non-empty evidence payload, but CKB dry-run
remains the production acceptance layer.
For manifest-bound spawn targets, `validate-tx` now also checks the actual
`cell_deps[index]` object against the declared CellDep identity and requires
matching spawn-target evidence; an empty dep object plus a self-asserted payload
is rejected.

### Transaction Validation

New command:

```bash
cellc validate-tx --against metadata.json tx.json --json
```

The validator checks a transaction JSON shape against builder assumptions before
signing. Non-structural assumptions such as global uniqueness, TYPE_ID builder
plans, lock-group transaction-scope assumptions, and capacity evidence require
explicit `builder_assumption_evidence`.

### Transaction Template Emitter

```bash
cellc solve-tx
```

The transaction template emitter derives input/output/dep slot requirements
from ProofPlan records, surfaces CKB dependency metadata, reports fee/change
metadata from CKB constraints, and emits a signing manifest skeleton with
per-lock signature request requirements.

**Note**: This is a deterministic template emitter, not a runtime cell
selector or final solver. Builders must still perform concrete cell selection,
dep/header resolution, fee/change planning, occupied-capacity calculation,
witness placement, signing, and CKB dry-run.

### Metadata Tooling

0.16 adds metadata-driven commands:

```bash
cellc deploy-plan
cellc verify-deploy
cellc diff-deploy
cellc lock-deps
cellc proof-diff
cellc profile
cellc trace-tx
cellc audit-bundle
```

These commands produce deterministic JSON reports. The audit bundle now
includes `source_to_codegen` mapping that links ProofPlan records to source
spans, IR effect classes, and codegen coverage status, along with action/lock
traces that include per-entry source-to-IR-to-codegen mappings and runtime
access details.

`proof-diff` reports added, removed, and changed ProofPlan record keys and
includes `changed_records` field entries for changed trigger, scope, reads,
coverage, group cardinality, builder assumption, codegen coverage, and
on-chain-check status fields.

**Note**: Source-to-codegen mapping is at the metadata/IR level. Full
CellScript-to-RISC-V assembly source maps are not yet available.

### VS Code Extension

The VS Code extension package is aligned with CellScript 0.16.0. Its README,
changelog, package metadata, validation script, and packaged VSIX now describe
the current `cellc --lsp` tooling surface and 0.16 authoring surface.

Active-file editor commands cover the 0.16 report surface that does not need
extra input files: `explain-assumptions`, `solve-tx`, `deploy-plan`, `profile`,
and `audit-bundle`. Commands that compare or validate separate files remain
CLI-first: `validate-tx`, `trace-tx`, `proof-diff`, `verify-deploy`,
`diff-deploy`, and `lock-deps`.

### Standard Compatibility Suite

The compatibility manifest is:

```text
tests/compat/ckb_standard/manifest.json
```

It covers fixture expectations for sUDT, xUDT, ACP, Cheque,
Omnilock-compatible locks, NervosDAO since/epoch behavior, and Type ID.

Each suite has descriptive fixture files with transaction shapes,
ScriptGroup matrices, outputs/outputs_data binding matrices, expected behavior,
script args/witness/molecule data layouts, metadata expectations, cycle report
envelopes, and capacity reports.

**Note**: These are descriptive fixtures, not executable test runners.
No test harness validates accepted/rejected cases against CKB or the
compiler. CKB dry-run remains the acceptance mechanism.

### CKB Standard Library Protocol Module Stubs

0.16 adds schema stubs for CKB stdlib protocol modules:

- `std::sudt` — Simple UDT transfer and mint
- `std::xudt` — eXtensible UDT transfer
- `std::type_id` — TYPE_ID cell identity creation
- `std::htlc` — Hash Time-Locked Contract claim (preimage/timelock)
- `std::cheque` — Cheque claim and refund
- `std::acp` — Anyone-Can-Pay deposit and withdraw

Each module declares ProofPlan trigger/scope/reads, builder assumptions,
compatibility fixture references, and `schema-stub` status via
`CkbStdlibModule`/`ProtocolFunction` descriptors.

**Note**: These are schema stubs only — no CellScript source
implementations, no assembly generation, and no ProofPlan pipeline integration.
Descriptor coverage verifies module/function metadata and fixture linkage, but
there is no executable integration or production CKB evidence yet. A future
release must implement the modules before they can be used in production
contracts.

## Compatibility

`--primitive-strict=0.16` includes the v0.15 primitive strictness rules and adds
mandatory ProofPlan soundness enforcement. Existing v0.15 sources can still use
default compatibility mode while migration is in progress.

## Verification

Focused v0.16 gate:

```bash
cargo test --locked -p cellscript --test v0_16 -- --test-threads=1
cargo test --locked -p cellscript proof_plan --lib -- --test-threads=1
cargo check --locked -p cellscript --all-targets
git diff --check
```

Full scoped 0.16 gate:

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
```

NovaSeal proposal-local acceptance gate:

```bash
python3 scripts/novaseal_devnet_stateful_live.py --pretty
python3 scripts/novaseal_agreement_devnet_stateful_live.py --pretty
for profile in fungible-xudt rwa-receipt btc-transaction-commitment btc-utxo-seal dual-seal fiber-candidate; do
  python3 scripts/novaseal_planned_profiles_devnet_stateful_live.py --profile "$profile" --live --pretty
done
python3 scripts/novaseal_btc_spv_evidence_adapter.py --pretty
python3 scripts/novaseal_external_attestation_adapter.py --pretty
python3 scripts/novaseal_external_evidence_handoff_bundle.py --pretty
./scripts/novaseal_devnet_stateful_acceptance.sh --pretty
target/debug/cellc certify --plugin novaseal-profile-v0 --repo-root . --json
```

The NovaSeal gate is part of the `nightly-0.16` evidence bundle. It proves local
devnet/profile acceptance for the proposal packages only; it does not convert
the general 0.16 standard CKB compatibility fixtures into executable CKB VM
equivalence tests.

## Deferred To 0.17

The following items are outside the scoped 0.16 release and are tracked by
`docs/0.17/CELLSCRIPT_0_17_ROADMAP.md`:

The 0.16 freeze keeps only the P0 plus key P1 compiler-hardening items from
`RUST_COMPARATIVE_AUDIT.md`: IR poison, register/syscall gates, IR provenance,
and `expect-error-line:N:TEXT`. The remaining comparative-audit cleanup is
tracked by the 0.17 roadmap alongside CKB production-completeness work.

- executable CKB VM accepted/rejected fixture runner;
- full CKB transaction semantic validation and dry-run-backed fixture verdicts;
- real transaction solver with cell selection, dep/header resolution,
  occupied-capacity calculation, fee/change planning, witness placement,
  signing, and dry-run;
- on-chain deployment verification;
- full CellScript-to-RISC-V/assembly source maps;
- ABI-compatible `std::sudt`, `std::xudt`, `std::type_id`, `std::htlc`,
  `std::cheque`, `std::acp`, and DAO helpers;
- executable aggregate invariant lowering and iCKB differential tests;
- production formal-verification guarantees;
- deeper `Lowered<T>` poison representation, tuple/span hygiene,
  per-function backend validation, IR helper exhaustiveness tests, phase
  markers, diagnostic dedup/warnings, module splits, resolver/type cleanup, and
  release tidy gates.

The following 0.16 boundaries remain intentional:

- operational semantics are mechanically precise prose plus conformance tests,
  not a formal proof;
- ProofPlan soundness is a metadata consistency checker, not a formal proof of
  invariant soundness;
- standard CKB compatibility fixtures are descriptive, not executable
  equivalence tests;
- `validate-tx` is structural and schema-bound evidence validation, not full CKB
  transaction semantic validation;
- `solve-tx` is a deterministic template emitter, not a final solver;
- CKB stdlib protocol modules are `schema-stub`, not production-ready modules;
- NovaSeal devnet/profile certification is proposal-local evidence, not a
  blanket production claim for every CellScript package;
- CKB dry-run/commit evidence and external attestations remain the production
  acceptance layer.
