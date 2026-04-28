# CellScript 0.16 Release Notes Draft

Status: implementation branch draft for `cellscript-0.16`.

Updated: 2026-04-28.

CellScript 0.16 turns the v0.15 ProofPlan audit surface into a metadata
assurance toolchain. The release adds operational semantics, ProofPlan
soundness checks, stable builder assumption metadata, transaction-shape
validation, deployment and audit reports, standard CKB compatibility
descriptive fixtures, and CKB stdlib protocol module schema stubs.

Production-completeness items such as executable CKB VM fixture execution,
full transaction solving, source-to-assembly maps, and protocol stdlib
implementations are deliberately deferred to 0.17.

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
- verifier obligations whose matching ProofPlan record differs in origin,
  scope, category, feature, status, or detail;
- duplicate ProofPlan obligation keys;
- on-chain checked records whose codegen coverage is not `covered`;
- runtime-required records marked as on-chain checked;
- `lock_args` provenance mixed into witness reads;
- local action/function/lock ProofPlan records that diverge from
  `runtime.proof_plan`, including changed trigger, scope, reads, coverage,
  assumptions, detail, or codegen coverage;
- checked records without concrete reads, coverage, checked-obligation labels,
  or valid source spans when source-declared;
- cell-access records whose source class and reads disagree;
- metadata-only/runtime-required gaps in `--primitive-strict=0.16` mode.

**Note**: This is a metadata consistency checker, not a formal proof.

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

**Note**: `validate-tx` performs structural, schema-bound, and
transaction-bound evidence validation, not full CKB transaction semantic
verification. Non-structural assumption evidence must bind to the assumption
id, kind, origin, feature, and ProofPlan status and include a typed evidence
payload. Bare evidence tokens are rejected. Required cell/output/dep/witness
payload items must include indexes that are range-checked against the
transaction shape. When evidence and the indexed transaction object both expose
a concrete field such as outpoint, lock hash, type hash, capacity, dep metadata,
witness bytes, or TYPE_ID args, the values must match. CKB dry-run remains the
production acceptance layer.

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
per-lock signature request requirements. It also emits a machine-readable
`evidence_schema` for each builder assumption so downstream builders know the
required indexed payload arrays, concrete fields, TYPE_ID fields, capacity
checks, and manual-review evidence.

**Note**: This is a deterministic template emitter, not a runtime cell
selector or final solver. Output is explicitly labelled `template-only`,
`template-emitter-only`, `non-executable-template`, and `can_submit=false`.
Builders must still perform concrete cell selection, dep/header resolution,
fee/change planning, occupied-capacity calculation, witness placement, signing,
and CKB dry-run.

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

**Note**: Source-to-codegen mapping is at the metadata/IR level. Full
CellScript-to-RISC-V assembly source maps are not yet available.

### Standard Compatibility Suite

The compatibility manifest is:

```text
tests/compat/ckb_standard/manifest.json
```

It covers fixture expectations for sUDT, xUDT, ACP, Cheque,
Omnilock-compatible locks, NervosDAO since/epoch behavior, and Type ID.

Each suite has descriptive fixture files with transaction shapes,
expected behavior, script args/witness/molecule data layouts, metadata
expectations, cycle report envelopes, and capacity reports.

**Note**: These are descriptive fixtures in the scoped 0.16 release. The 0.17
branch adds a deterministic model verifier for the same fixture family, but it
still does not execute CKB VM. CKB dry-run remains the acceptance mechanism.

### CKB Standard Library Protocol Module Descriptors

0.16 adds schema stubs for CKB stdlib protocol modules:

- `std::sudt` — Simple UDT transfer and mint
- `std::xudt` — eXtensible UDT transfer
- `std::type_id` — TYPE_ID cell identity creation
- `std::htlc` — Hash Time-Locked Contract claim (preimage/timelock)
- `std::cheque` — Cheque claim and refund
- `std::acp` — Anyone-Can-Pay deposit and withdraw

Each module declares ProofPlan trigger/scope/reads, builder assumptions,
compatibility fixture references, and non-stable `schema-stub` status via
`CkbStdlibModule`/`ProtocolFunction` descriptors.

**Note**: These are schema stubs only — no CellScript source
implementations, no assembly generation, no ProofPlan pipeline integration,
and no test coverage. A future release must implement the modules before
they can be used in production contracts.

### Review Finding Closure

The 0.17 branch hardens the scoped 0.16 implementation against the review
findings for ProofPlan key coarseness, weak `validate-tx` evidence, premature
stdlib stability labels, and `solve-tx` solver overclaiming. See
`docs/0.17/review_findings_closure.md` for the exact closure matrix, tests, and
remaining boundaries.

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

## Deferred To 0.17

The following items are outside the scoped 0.16 release and are tracked by
`docs/0.17/CELLSCRIPT_0_17_ROADMAP.md`:

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
- formal verification backend exploration.

The following 0.16 boundaries remain intentional:

- operational semantics are mechanically precise prose plus conformance tests,
  not a formal proof;
- ProofPlan soundness is a metadata consistency checker, not a formal proof of
  invariant soundness;
- standard CKB compatibility fixtures are descriptive, not executable
  equivalence tests;
- `validate-tx` is structural, schema-bound, and transaction-bound evidence
  validation, not full CKB transaction semantic validation;
- `solve-tx` is a deterministic template emitter, not a final solver;
- CKB stdlib protocol modules are non-stable `schema-stub` descriptors, not
  production-ready modules;
- CKB dry-run/commit evidence remains the production acceptance layer
