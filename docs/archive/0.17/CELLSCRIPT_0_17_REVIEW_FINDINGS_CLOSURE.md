# 0.16 Review Findings Closure

Updated: 2026-04-28.

This note records how the 0.17 branch handles the four 0.16 review findings.
It is intentionally precise about what is fixed and what remains outside the
0.16/0.17 evidence boundary.

## Closure Table

| Finding | Status in this branch | Enforcement | Remaining boundary |
|---|---|---|---|
| P1 ProofPlan soundness key too coarse | Fixed for metadata soundness | Obligation matching includes origin/scope plus category, feature, status, and detail; duplicate obligation keys are rejected; local/runtime ProofPlan records are compared by full serialized content; checked records require concrete reads, coverage, checked labels, valid source spans, and cell-access source/read consistency. | This is still a metadata consistency gate, not a formal proof that every invariant is semantically correct. |
| P1 `validate-tx` accepts unvalidated evidence tokens | Fixed for schema-bound and transaction-bound evidence | Bare tokens are rejected. Evidence must bind to the assumption id/kind/origin/feature/status and include typed payloads. Required input/output/cell_dep/witness payload items must carry indexes and are range-checked against the transaction shape. Concrete fields such as outpoint, lock/type hash, capacity, dep metadata, witness bytes, and TYPE_ID args are cross-checked when present. | The validator does not execute CKB VM, run CKB consensus validation, select cells, calculate fees, or prove live-chain availability. |
| P2 stdlib stubs are marked stable | Fixed | Protocol descriptors that are schema/metadata stubs are not marked `stable`. Runtime-backed protocol surfaces are labelled partial, and tests reject premature `stable` labels. | Full ABI-compatible sUDT/xUDT/TYPE_ID/DAO/ACP/Cheque/Omnilock implementations still require executable coverage before any stable production claim. |
| P2 `solve-tx` is a template emitter, not a solver | Fixed by explicit demotion and schema output | `solve-tx` emits `status = template-only`, `solver_capability = template-emitter-only`, `execution_mode = non-executable-template`, `can_submit = false`, unresolved header dep slots, external solver steps, and a machine-readable evidence schema for every builder assumption. | It still does not perform live cell selection, dep/header resolution, fee/change solving, occupied-capacity calculation, final witness/signature placement, or dry-run validation. |

## Tests

Focused coverage lives in `tests/v0_16.rs`:

- `proof_plan_soundness_rejects_duplicate_and_incomplete_semantic_records`
- `proof_plan_soundness_rejects_obligation_scope_mismatches`
- `proof_plan_soundness_rejects_local_runtime_mismatches`
- `proof_plan_soundness_requires_source_spans_for_source_invariants_in_strict_mode`
- `proof_plan_soundness_rejects_cell_access_read_mismatches`
- `validate_tx_checks_builder_assumption_evidence`
- `ckb_stdlib_protocol_modules_exist_and_cover_required_suites`
- `ckb_stdlib_protocol_functions_cover_core_operations`
- `cli_solve_tx_is_explicitly_template_only`

Run:

```bash
cargo test --locked -p cellscript --test v0_16
```

Full branch validation also includes:

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
```

## Production Claim Boundary

These fixes are necessary for iCKB-grade auditability but are not sufficient for
production equivalence. A production iCKB claim still requires the evidence gate
in `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_PRODUCTION_EQUIVALENCE_GATE.md`: original iCKB binary hashes,
generated CellScript artifact hashes, identical CKB transaction fixtures,
original and generated script exit codes, named reject modes, CKB VM/testtool
version, cycle counts, transaction size, and fixture manifest hashes.
