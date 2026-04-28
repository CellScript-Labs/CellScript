# CellScript 0.16 Release Notes Draft

Status: implementation branch draft for `cellscript-0.16`.

Updated: 2026-04-28.

CellScript 0.16 turns the v0.15 ProofPlan audit surface into an assurance
toolchain. The release adds operational semantics, ProofPlan soundness checks,
stable builder assumption metadata, transaction-shape validation, deployment
and audit reports, and a standard CKB compatibility fixture suite.

## Highlights

### Operational Semantics

The new spec is `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md`. It defines the
meaning of expression evaluation, linear resource states, branch merge rules,
Cell effects, triggers, scopes, ProofPlan fields, and builder assumptions.

Conformance is tied to `tests/v0_16.rs`.

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

### Transaction Validation

New command:

```bash
cellc validate-tx --against metadata.json tx.json --json
```

The validator checks a transaction JSON shape against builder assumptions before
signing. Non-structural assumptions such as global uniqueness, TYPE_ID builder
plans, lock-group transaction-scope assumptions, and capacity evidence require
explicit `builder_assumption_evidence`.

### Production Tooling

0.16 adds metadata-driven commands:

```bash
cellc solve-tx
cellc deploy-plan
cellc verify-deploy
cellc diff-deploy
cellc lock-deps
cellc proof-diff
cellc profile
cellc trace-tx
cellc audit-bundle
```

These commands produce deterministic JSON reports. They do not replace local CKB
dry-run/commit evidence; they make assumptions and diffs explicit before that
stage.

### Standard Compatibility Suite

The compatibility manifest is:

```text
tests/compat/ckb_standard/manifest.json
```

It covers fixture expectations for sUDT, xUDT, ACP, Cheque,
Omnilock-compatible locks, NervosDAO since/epoch behavior, and Type ID.

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

Full gate remains:

```bash
cargo fmt --all
cargo test --locked -p cellscript
bash scripts/cellscript_ckb_release_gate.sh production
```
