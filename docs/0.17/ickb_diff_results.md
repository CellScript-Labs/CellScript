# iCKB Differential Results

The differential harness is partial and model-level only.

`tests/benchmarks/ickb_diff/matrix.json` records the selected comparison
matrix. The manifest now uses schema `cellscript-ickb-diff-matrix-v1` and is
also the executable production-equivalence claim gate. The integration test
`tests/ickb_diff.rs` checks that every current row is labelled as a model result
rather than CKB VM equivalence, and rejects any future `PROVEN` claim that lacks
binary hashes, CKB VM/testtool evidence, fixture hashes, exit codes, cycle
counts, and named reject modes. The iCKB-specific gate is intentionally kept in
the test suite rather than the generic `cellc` CLI.

Current manifest status:

- `mode`: `MODEL_LEVEL_ONLY`
- `equivalence_status`: `NOT_PROVEN`
- `production_equivalence_claim`: `false`
- `equivalence_evidence`: `null`

| Scenario | Original iCKB expected | CellScript expected | Result |
|---|---:|---:|---|
| valid deposit phase 1 | pass | pass | model-pass |
| valid mint from receipt | pass | pass | model-pass |
| duplicate receipt | fail | fail | model-pass |
| amount inflation | fail | fail | model-pass |
| wrong owner | fail | fail | model-pass |
| wrong xUDT args | fail | fail | model-pass |
| immature redeem | fail | fail | model-pass |
| valid limit order | pass | pass | model-pass |
| limit order underpayment | fail | fail | model-pass |

## Production Equivalence Gate

The stricter gate is documented in
`docs/0.17/ickb_production_equivalence_gate.md`. In short, production
equivalence requires:

- original iCKB repo commit and script binary hashes;
- CellScript source commit and generated artifact hashes;
- CKB VM/testtool version;
- transaction fixture manifest hash;
- proof that inputs, outputs, cell deps, header deps, witnesses, and output data
  are identical across both executions;
- original and generated exit codes;
- named failure modes for rejects;
- cycle and transaction-size measurements;
- per-row execution objects with fixture/context hashes, both artifact hashes,
  status/exit-code match, cycles, transaction size, occupied capacity, and fee.

Without those fields, `MODEL_LEVEL_ONLY` rows cannot be upgraded to
behavioural-equivalence rows.

## Why This Is Not Behavioural Equivalence

- The original iCKB Rust scripts were not executed side-by-side with generated
  CellScript artifacts.
- iCKB `cargo test --locked` did not run to completion because prebuilt Capsule
  contract binaries were missing under `scripts/build/debug`.
- CellScript 0.17 has partial HeaderDep, SourceView, script-role, input
  OutPoint, pairwise MetaPoint relative, fixed-distance lock/type MetaPoint
  pair cardinality, base-cell-data signed i32 lock/type MetaPoint pair
  cardinality, DAO-rate/header-lineage/type-and-data-classification, and xUDT
  helper surfaces. Input-side accumulated-rate reads can now use `LOAD_HEADER`
  at the original iCKB `AR_OFFSET=160+8`. The iCKB owner-mode xUDT args pattern
  now has executable explicit-hash and current-script-hash Type Script args
  verifiers, SourceView lock/type empty-args and 32-byte args helpers exist,
  and simple xUDT group amount conservation plus token-side minted/burned
  deltas have executable helpers and strict helper-backed aggregate bridges.
  iCKB-specific output deposit/receipt map equality, receipt byte layout, and
  group receipt mint-sum recomputation are deliberately kept in benchmark
  fixtures instead of generic `dao::*` helpers. Local `u128`
  add/sub/mul/div/function-return deltas can now be materialized for generic
  xUDT delta helpers. CellScript also has
  RFC0017 epoch-since constructors and a generated relative epoch maturity
  bridge for DAO-like redeem paths. CellScript still lacks full arbitrary
  `Script`, full DAO second-withdrawal request/deposit/header binding,
  computed multi-cell iCKB mint-side receipt/deposit/DAO aggregate lowering,
  and native action-aware MetaPoint map semantics for a true CKB VM
  differential test.

## Next Differential Step

Build the iCKB scripts with Capsule, then adapt a small CKB testtool fixture set
so each transaction skeleton can run:

1. original iCKB binary,
2. generated CellScript binary,
3. identical cell deps, header deps, witnesses, inputs, outputs, and output
   data where semantics overlap.

Until that exists, equivalence must not be claimed.
