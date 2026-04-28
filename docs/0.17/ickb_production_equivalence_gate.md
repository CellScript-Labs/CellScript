# iCKB Production Equivalence Gate

This gate defines the minimum evidence required before CellScript may claim
production-equivalent behaviour for any selected iCKB scenario.

Current status: `NOT_PROVEN`.

The current benchmark remains `MODEL_LEVEL_ONLY`. Passing model fixtures,
compiling CellScript, emitting RISC-V assembly, or matching ProofPlan metadata is
not enough to claim behavioural equivalence with the audited iCKB Rust scripts.

## Claim Levels

| Level | Meaning | Allowed wording |
|---|---|---|
| `MODEL_LEVEL_ONLY` | CellScript examples and JSON fixtures model the intended invariant, but no original iCKB binary and generated CellScript binary were executed side by side. | "model-level", "iCKB-style", "partial" |
| `EXECUTED_CKB_VM_DIFF` | Original iCKB script and generated CellScript script were run on the same CKB VM/testtool transaction fixtures. | "executed differential subset" |
| `PROVEN` | Every scenario in the selected equivalence matrix has executed evidence and matching pass/fail behaviour with named reject reasons. | "production-equivalent for the selected executed subset" |

Full iCKB equivalence must not be claimed unless the matrix reaches
`PROVEN`. A partial executed subset must still identify every unsupported row as
`NOT_PROVEN` or `UNSUPPORTED`.

## Required Evidence

`tests/benchmarks/ickb_diff/matrix.json` is the executable claim manifest. The
integration test `tests/ickb_diff.rs` rejects any production equivalence claim
unless the manifest provides all of the following:

1. original iCKB repository commit;
2. original iCKB script binary SHA-256;
3. CellScript source commit;
4. generated CellScript artifact SHA-256;
5. CKB VM or CKB testtool version;
6. transaction fixture manifest SHA-256;
7. proof that both sides used identical inputs, outputs, cell deps, header deps,
   witnesses, and output data for the overlapping scenario;
8. original and generated script exit codes;
9. named failure mode for every reject case;
10. cycle and transaction-size measurements;
11. per-row execution objects;
12. pass/fail status match evidence;
13. transaction context hashes;
14. capacity and fee measurements.

Each row that claims executed equivalence must additionally contain an
`execution` object with:

- `fixture_sha256`;
- `transaction_context_sha256`;
- `original_ickb_binary_sha256`;
- `cellscript_artifact_sha256`;
- `ckb_vm_or_testtool_version`;
- `original_ickb_exit_code`;
- `cellscript_exit_code`;
- `original_ickb_status`;
- `cellscript_status`;
- `statuses_match = true`;
- `original_cycles`;
- `cellscript_cycles`;
- `tx_size_bytes`;
- `occupied_capacity_shannons`;
- `fee_shannons`;
- `failure_mode` for reject cases.

All SHA-256 values must be canonical `0x`-prefixed 32-byte hex. A passing row
must report exit code `0`; a failing row must report a non-zero exit code and a
named failure mode. `tests/ickb_diff.rs` rejects `PROVEN` if any row remains
`MODEL`, uses a `model-*` result, lacks CKB VM execution, lacks original iCKB
execution, or has mismatched status/exit-code evidence.

## Running The Gate

The iCKB-specific gate is executable through Rust integration tests. It is not
exposed from the generic `cellc` CLI:

```bash
cargo test --locked -p cellscript --test ickb_diff
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
```

`verify-ckb-fixtures` validates the standard CKB fixture manifest with the
deterministic model runner and emits a manifest hash. It still reports
`ckb_vm_execution = false`. `tests/ickb_benchmark.rs` validates the iCKB-style
positive and adversarial fixtures directly and also reports model-only coverage.
`tests/ickb_diff.rs` accepts the current `NOT_PROVEN` matrix as an honest
non-equivalence manifest. It fails if a matrix claims
`PROVEN` or `EXECUTED_CKB_VM_DIFF` without the required top-level and per-row
execution evidence.

## Current Enforcement

The current matrix intentionally sets:

- `mode = MODEL_LEVEL_ONLY`
- `equivalence_status = NOT_PROVEN`
- `production_equivalence_claim = false`
- `equivalence_evidence = null`

The tests assert that every current row remains model-level and lacks CKB VM
execution. A future change that flips the matrix to `PROVEN` without populated
execution evidence fails the test suite.

## What Still Blocks Equivalence

- No original iCKB Rust script binary is executed in this repository.
- No generated CellScript binary is executed in a CKB VM/testtool differential
  harness.
- DAO header dep lineage, accumulated-rate parsing, maturity, occupied
  capacity, xUDT args/data layout, witness parsing, and cell dep substitution
  are still partly modelled instead of byte-accurately executed.
- `solve-tx` emits a non-executable transaction template with `can_submit=false`
  and explicit unresolved external steps; it is not a cell-selection, fee,
  change, witness, dep, or dry-run solver.

Until these blockers are closed, the correct conclusion remains: CellScript is
partially iCKB-grade for modelling and compiler-surface audit work, but it does
not pass a complete production-equivalence iCKB benchmark.
