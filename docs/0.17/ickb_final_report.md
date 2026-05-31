# iCKB CellScript Completeness Benchmark Final Report

## Executive Summary

CellScript is **partially complete but not ready** for iCKB-grade CKB protocols.

It can express useful iCKB-style resource, receipt, linear-consumption,
owner-pairing, transfer, and limit-order arithmetic patterns. It cannot yet
express several production-critical iCKB invariants natively: HeaderDep
accumulated-rate binding, script lock/type role introspection, exact
transaction-wide computed accounting, xUDT args construction, DAO maturity, CKB
OutPoint/index relations, C256 arithmetic, and signed relative distances.

The benchmark is an iCKB-inspired semantic model. It is not a faithful port and
does not prove behavioural equivalence with the audited iCKB Rust scripts.

## Implemented Artifacts

- `examples/ickb_benchmark/README.md`
- `examples/ickb_benchmark/ickb_logic.cell`
- `examples/ickb_benchmark/limit_order.cell`
- `examples/ickb_benchmark/owned_owner.cell`
- `tests/ickb_benchmark.rs`
- `tests/benchmarks/ickb_positive/*.json`
- `tests/benchmarks/ickb_negative/*.json`
- `tests/benchmarks/ickb_diff/matrix.json`
- `docs/0.17/ickb_discovery.md`
- `docs/0.17/ickb_semantics.md`
- `docs/0.17/ickb_cellscript_gap_matrix.md`
- `docs/0.17/ickb_diff_results.md`
- `docs/0.17/ickb_final_report.md`

CI integration is through the existing Cargo test workflow: `tests/ickb_benchmark.rs`
is a normal integration test, so the existing `cargo test --locked ...` CI step
will run it.

## Coverage Table

| Semantic item | CellScript support | Test coverage | Remaining gap | Severity |
|---|---|---|---|---|
| Deposit phase 1 receipt creation | EXPRESSIBLE_WITH_PATTERN | positive + forged receipt/capacity negatives | executable output-group aggregate lowering | HIGH |
| Deposit phase 2 mint | EXPRESSIBLE_WITH_ESCAPE_HATCH | positive + inflation/deflation/header/xUDT negatives | HeaderDep and xUDT binding | BLOCKER |
| Receipt consumption/no double use | NATIVE plus model duplicate check | duplicate receipt negative | true prior-cell lineage API | MEDIUM |
| iCKB transfer | NATIVE for token resource model | positive transfer | xUDT ABI binding | HIGH |
| Withdrawal/redeem | EXPRESSIBLE_WITH_ESCAPE_HATCH | positive withdrawal, immature redeem negative | DAO maturity and since/header primitives | HIGH |
| Exact accounting | EXPRESSIBLE_WITH_ESCAPE_HATCH | inflation/deflation negatives | executable computed aggregate invariants | BLOCKER |
| Owned-Owner pairing | EXPRESSIBLE_WITH_PATTERN | positive + wrong owner negative | signed distance and source index APIs | HIGH |
| Limit Order fulfilment | EXPRESSIBLE_WITH_PATTERN | positive + underpayment/wrong asset negatives | C256 and MetaPoint support | HIGH |
| Script role confusion | NOT_EXPRESSIBLE | negative model fixture | current script role/source scans | BLOCKER |
| Witness malformation | EXPRESSIBLE_WITH_PATTERN | negative model fixture | full witness/Molecule parser and auth primitives | HIGH |
| CellDep substitution | EXPRESSIBLE_WITH_PATTERN | negative model fixture | deploy/runtime dep verification integration | HIGH |

## Test Results

Commands run before benchmark changes:

```bash
cargo test --locked -p cellscript
```

Result: passed, 529 Rust tests plus doc-tests.

iCKB test attempt:

```bash
cd /tmp/cellscript-ickb-audit/v1-core/scripts
cargo test --locked
```

Result: crates compiled, but the iCKB `tests` crate failed 2 tests because
Capsule-built binaries were missing under `scripts/build/debug`.

Benchmark command:

```bash
cargo test --locked -p cellscript --test ickb_benchmark
```

Result: passed, 4 tests.

The positive fixture set contains 6 model-level cases. The negative set contains
15 adversarial model-level cases. The differential matrix contains 9
model-level rows.

## Compiler Changes Made

No compiler implementation changes were made. The benchmark intentionally records
the current compiler gaps instead of weakening invariants to make a production
claim.

## Unresolved Blockers

1. HeaderDep accumulated-rate binding.
2. Lock/type script-role introspection and script args scanning.
3. Executable transaction-wide aggregate invariants with computed values.
4. Native xUDT owner-mode args and token amount layout support.
5. DAO deposit/withdrawal/maturity primitives.
6. CKB OutPoint, output index, occupied capacity, and CellDep surfaces.
7. Signed integer types and checked 256-bit arithmetic.
8. Stable per-invariant diagnostics and runtime error codes.

## Recommended Next Steps

1. Add typed CKB source primitives for headers, outpoints, script role, script
   args, occupied capacity, and cell deps.
2. Lower aggregate invariants into executable CKB runtime checks for exact
   equality and bounded inequality over CellScript-typed CKB Cell groups.
3. Add `std::dao` and `std::xudt` modules with deployed ABI-compatible helpers.
4. Add signed integer and checked `u256`/`C256` arithmetic support.
5. Build an actual CKB testtool differential harness once iCKB Capsule binaries
   and CellScript generated binaries can be run against the same fixtures.
6. Use a Rosen-style bridge settlement invariant benchmark after iCKB to test
   multi-party cross-chain accounting and replay prevention.

## Honesty Statement

This benchmark is a semantic model and partial port. It proves that selected
iCKB-style positive and adversarial scenarios can be represented and checked at
the model level while the CellScript specs compile. It does not prove CKB VM
behavioural equivalence, production readiness, cycle bounds, transaction size,
occupied capacity, or under-capacity safety.
