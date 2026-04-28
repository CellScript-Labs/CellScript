# iCKB Differential Results

The differential harness is partial and model-level only.

`tests/benchmarks/ickb_diff/matrix.json` records the selected comparison
matrix. The test `ickb_diff_matrix_is_partial_and_consistent_with_model_fixtures`
checks that every row is labelled as a model result rather than CKB VM
equivalence.

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

## Why This Is Not Behavioural Equivalence

- The original iCKB Rust scripts were not executed side-by-side with generated
  CellScript artifacts.
- iCKB `cargo test --locked` did not run to completion because prebuilt Capsule
  contract binaries were missing under `scripts/build/debug`.
- CellScript currently lacks first-class HeaderDep, Script role, CKB OutPoint,
  occupied capacity, xUDT args, and DAO maturity primitives required for a true
  CKB VM differential test.

## Next Differential Step

Build the iCKB scripts with Capsule, then adapt a small CKB testtool fixture set
so each transaction skeleton can run:

1. original iCKB binary,
2. generated CellScript binary,
3. identical cell deps, header deps, witnesses, inputs, outputs, and output
   data where semantics overlap.

Until that exists, equivalence must not be claimed.
