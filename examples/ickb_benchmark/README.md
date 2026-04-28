# iCKB CellScript Completeness Benchmark

This directory contains an iCKB-inspired CellScript benchmark, not a faithful
port of the audited iCKB Rust scripts.

The goal is to keep the scope explicit: each `.cell` file models the invariant
shape that CellScript can currently express, while TODO markers identify the
CKB-native semantics that still require compiler/runtime features or raw script
code.

## Scope

- `ickb_logic.cell` models deposit phase 1 receipt creation, receipt
  consumption, iCKB accounting, xUDT binding as a hash placeholder, withdrawal
  request creation, maturity checks, and linear no-double-consume behaviour.
- `limit_order.cell` models limit order creation, match value conservation,
  partial-fill minimums, cancellation through an owner/master cell, and malformed
  ratio rejection.
- `owned_owner.cell` models owned/owner cell pairing and wrong-owner rejection.

The benchmark does not prove behavioural equivalence with the original iCKB
scripts. The Rust integration tests under `tests/ickb_benchmark.rs` compile
these CellScript specs and run deterministic model-level positive, negative, and
differential fixtures. Fixtures that do not execute a generated CKB VM binary
are labelled `MODEL_LEVEL_ONLY`.

## Original Semantics Mapped

- iCKB Logic: proposal deposit/withdrawal sections and
  `scripts/contracts/ickb_logic/src/entry.rs`.
- Limit Order: proposal Limit Order section and
  `scripts/contracts/limit_order/src/entry.rs`.
- Owned-Owner: proposal Owned Owner section and
  `scripts/contracts/owned_owner/src/entry.rs`.

## Running

```bash
cargo test --locked -p cellscript --test ickb_benchmark
```

For the broader repository gate:

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
git diff --check
```

## Known Limitations

- HeaderDep access and accumulated-rate binding are modelled as explicit fields.
- xUDT args construction is represented by an expected hash field.
- lock/type script-role dual use is not first-class in CellScript.
- transaction-wide group scans and exact iCKB accounting are tested by the
  benchmark model, not by executable aggregate invariant lowering.
- Owned-Owner signed relative indexes are approximated with unsigned indexes.
- Limit Order C256 checked arithmetic and MetaPoint/OutPoint binding are not
  native CellScript concepts.
