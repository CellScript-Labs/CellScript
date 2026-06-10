# iCKB CellScript Completeness Benchmark

This directory contains an iCKB-inspired CellScript benchmark, not a faithful
port of the audited iCKB Rust scripts.

The goal is to keep the scope explicit: each `.cell` file models the invariant
shape that CellScript can currently express, while `LIMITATION(ickb-benchmark:*)`
markers and `limitations.json` identify the CKB-native semantics that still
require compiler/runtime features or raw script code.

## Scope

- `ickb_logic.cell` models deposit phase 1 receipt creation, receipt
  consumption, iCKB accounting, xUDT binding as an explicit model field, withdrawal
  request creation, maturity checks, and linear no-double-consume behaviour.
- `limit_order.cell` models limit order creation, match value conservation,
  partial-fill minimums, cancellation through an owner/master cell, and malformed
  ratio rejection.
- `owned_owner.cell` models owned/owner cell pairing and wrong-owner rejection.

The benchmark does not prove behavioural equivalence with the original iCKB
scripts. The Rust integration tests under `tests/ickb_benchmark.rs` compile
these CellScript specs and run deterministic model-level positive, negative, and
differential fixtures. Fixtures that do not execute a generated CKB VM binary
are labelled `MODEL_LEVEL_ONLY`, and each unresolved protocol limitation is
tracked in `limitations.json`.

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

See `limitations.json` for the authoritative list. The current unresolved IDs
cover HeaderDep accumulated-rate binding, first-class script role/args
introspection, executable aggregate invariant lowering, iCKB discount and wide
integer arithmetic, Limit Order MetaPoint/OutPoint binding, and Owned-Owner
signed relative-index semantics.
