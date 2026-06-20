# iCKB CellScript Completeness Benchmark

This test-suite directory contains an iCKB-inspired CellScript benchmark, not a
faithful port of the audited iCKB Rust scripts. It intentionally lives under
`tests/benchmarks` instead of the public examples tree so iCKB-specific
protocol assumptions do not become part of CellScript's generic API surface.

The goal is to keep the scope explicit: each `.cell` file models the invariant
shape needed by the manifest-declared executable iCKB claim set. The 0.17/0.18
work adds runtime-backed SourceView, HeaderDep, DAO-rate/header-lineage,
script-role, first-class Script construction/matching, full input OutPoint
tx-hash/index reads, pairwise MetaPoint relative checks, fixed-distance and
i32-data-driven lock/type MetaPoint pair cardinality scans, and xUDT helper
calls, including executable xUDT group amount conservation. The differential
tests use protocol-neutral `ckb::cell_data_u32_le` /
`ckb::cell_data_u64_le` helpers to enforce the 12-byte receipt shape, decode
receipt quantity/amount bytes, and recompute mint sums at runtime, including a
`quantity = 2` single receipt and a mixed receipt group with different
quantity/deposit amount bytes.
The DAO withdrawal differential coverage also includes two-input same-rate
exact/plus-one rows, mixed-deposit-rate exact/plus-one rows, and
mixed-withdraw-rate exact/plus-one aggregate capacity rows executed against the
original DAO ELF and generated CellScript ELF. It also includes three-input
same-rate exact/plus-one aggregate capacity rows, plus malformed
second-witness `input_type` missing/empty/short/long and
withdraw-header/out-of-bounds index reject rows.
iCKB-specific output deposit/receipt pairing and broader receipt group scans
remain in the benchmark fixture layer rather than generic compiler helpers, so
the files remain an iCKB-style protocol-equivalence benchmark rather than an
audited port or a generic language template.

## Scope

- `ickb_logic.cell` models deposit phase 1 receipt creation, receipt
  consumption, iCKB accounting, xUDT binding through full hash / Script checks,
  withdrawal
  request creation, maturity checks, the iCKB 10% oversized-deposit discount
  arithmetic, model-level output-side deposit/receipt pairing, and linear
  no-double-consume behaviour.
- `limit_order.cell` models limit order creation, match value conservation,
  partial-fill minimums, absolute master-OutPoint preservation for Match
  outputs, cancellation through an owner/master cell, and malformed ratio
  rejection.
- `owned_owner.cell` models owned/owner cell pairing and wrong-owner rejection.

The `.cell` benchmark specs alone do not prove behavioural equivalence with the
original iCKB scripts. The Rust integration tests under
`tests/ickb_benchmark.rs` compile these CellScript specs and run deterministic
model-level positive and negative fixtures, while `tests/ickb_diff.rs` is the
executable original-vs-CellScript CKB VM differential replay gate and matrix
consistency check. `tests/v0_17.rs` and `tests/v0_18.rs` additionally check
that CKB source primitives, C256
requirement helpers, signed `i32` ABI lowering, first-class Script
construction, OutPoint reads, and cell-data decoders compile and execute where
required. Fixtures that do not execute a generated CKB VM binary are not
counted as equivalence rows. The stricter production-equivalence claim gate is
documented in `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_PRODUCTION_EQUIVALENCE_GATE.md`.

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
cargo test --locked -p cellscript --test v0_17
cargo test --locked -p cellscript --test ckb_compat_runner
cargo test --locked -p cellscript --test ickb_diff
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/benchmarks/ickb_diff/claim_manifest.json --json
```

`ckb_compat_runner` is still a model runner, but it derives verdicts from the
fixture transaction shape for amount conservation, duplicate TYPE_ID outputs,
DAO maturity labels, owner/receiver mismatch, auth failure, and capacity report
sanity instead of merely reading the expected exit code.
The iCKB-style positive and negative JSON fixtures are verified only by the
test suite. No iCKB-specific fixture verifier is exposed from the generic
`cellc` CLI.
`cellc verify-ckb-fixtures tests/benchmarks/ickb_diff/claim_manifest.json` is
an iCKB claim-manifest checker: it validates the committed differential matrix
shape, production evidence, hardening thresholds, and branch coverage, but it
does not execute CKB VM itself. Run `cargo test --locked -p cellscript --test
ickb_diff` when fresh executable replay evidence is required.

For the broader repository gate:

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
git diff --check
```

## Known Limitations

- HeaderDep access exists through `dao::accumulated_rate(source::header_dep(i))`
  and input committed-header access exists through
  `dao::input_accumulated_rate(source::input(i))` /
  `dao::input_accumulated_rate(source::group_input(i))`, which reads the
  original iCKB `AR_OFFSET=160+8` via `LOAD_HEADER`. Helper failures propagate
  through generated fail-closed checks. DAO type and
  deposit/withdrawal data classification now use
  `dao::has_dao_type(source_view)`, `dao::is_deposit_data(source_view)`, and
  `dao::is_withdrawal_request_data(source_view)`, matching iCKB's DAO hash and
  exact 8-byte zero/non-zero data rules.
  `ckb::since_epoch_relative(number, index, length)` and
  `dao::require_input_relative_epoch_since_at_least(input, number, index,
  length)` now add generated RFC0017 relative epoch-since checks for
  redeem-like paths. Receipt quantity/amount byte decoding is covered in the VM
  differential layer through protocol-neutral `ckb::cell_data_u32_le` /
  `ckb::cell_data_u64_le`. iCKB-specific output deposit/receipt pairing and
  broader group receipt scans are intentionally not exposed as generic
  `dao::*` helpers; they remain in the benchmark fixture and differential-test
  layer. `mint_from_receipt` still requires the receipt accumulated rate to
  match both the supplied HeaderDep rate and the input committed-header rate.
  The declared executable withdrawal claim set is covered by the differential
  matrix; additional adversarial header/witness permutations are hardening
  work.
- The discount arithmetic is expressed directly in CellScript. Plain xUDT
  transfer conservation can declare the exact group amount aggregate and call
  `xudt::require_group_amount_conserved()`; 0.17 strict mode rejects that
  declaration if the helper is not emitted. xUDT token-side mint/burn deltas now
  have declared `assert_delta` invariants paired with
  `xudt::require_group_amount_minted(delta)` and
  `xudt::require_group_amount_burned(delta)`; this benchmark uses the
  minted-delta helper in `mint_from_receipt` and the burned-delta helper in
  `request_withdrawal`. Computed local `u128` add/sub/mul/div deltas are now
  addressable by those helpers. Output-side deposit/receipt pairing and
  broader current type-group receipt scans stay in the iCKB fixture suite
  because hardcoding those exact iCKB layouts in the generic compiler would
  weaken CellScript's protocol neutrality. The stricter differential suite
  executes receipt quantity/amount data binding, mint-value recomputation, and
  DAO redeem aggregate boundaries in CKB VM through generic helpers and
  fixture-scoped protocol rows.
- xUDT owner-mode args can be checked with
  `xudt::require_owner_mode_type_args(source, owner_hash, flags)` for the
  iCKB-style `[logic_hash, 0x80000000]` pattern, and
  `xudt::require_owner_mode_type_args_current_script(source, flags)` binds that
  owner hash to `LOAD_SCRIPT_HASH(current script)`. `mint_from_receipt` also
  calls `ckb::require_cell_type_script_hash_type(source, code_hash, hash_type)`
  so xUDT Type Script identity is bound without an iCKB-specific helper. The
  benchmark still keeps an `xudt_args_hash` field for the fixture ABI, while
  0.18 also exposes first-class fixed-byte `Script` construction for rows that
  need exact expected Script values.
- `ckb::current_script_hash()` exposes the current script hash as a reusable
  32-byte `Hash`; `ickb_logic.cell::mint_from_receipt` passes it into the
  generic xUDT owner-mode args verifier before the receipt hash check.
- lock/type script-role dual use has `ckb::current_role()` plus
  `ckb::require_current_script_args_empty()` for the executing script's
  Molecule `Script.args` and same-code/hash-type Output lock args, full 32-byte
  `ckb::require_cell_lock_hash` / `ckb::require_cell_type_hash` SourceView
  requirement helpers, generic SourceView Script code_hash/hash_type helpers,
  and
  `ckb::require_cell_lock_args_empty` / `ckb::require_cell_type_args_empty` plus
  `ckb::require_cell_lock_args_hash` /
  `ckb::require_cell_type_args_hash` SourceView helpers, plus 0.18 exact
  `Script` construction/matching for fixture-bound expected scripts. Generic
  lock/type group scans remain outside this benchmark spec.
- cross-cell iCKB mint/redeem accounting across receipts, deposits, DAO rates,
  and token amounts is tested by the benchmark model, not by complete
  executable aggregate invariant lowering.
- Owned-Owner signed relative distance is represented as fixed-width
  little-endian `i32` and sign-extended in generated RISC-V code.
  `ckb::require_metapoint_relative(base, related, distance)` now lowers the
  iCKB-style pairwise relation `extract_metapoint(base).index + distance ==
  extract_metapoint(related).index`: input/group-input pairs compare full
  OutPoint tx hashes and indexes, while output/group-output pairs compare
  encoded output indexes. Fixed-distance current-script pair scans lower
  through `ckb::require_lock_type_metapoint_pairs` and
  `ckb::require_type_lock_metapoint_pairs`, and i32-data-driven scans lower
  through `ckb::require_*_metapoint_pairs_from_i32_data(source, offset)`.
  Filtered variants such as
  `ckb::require_type_lock_metapoint_pairs_from_i32_data_filtered(source,
  offset, expected_type_hash, related_data_rule)` additionally require every
  related-role cell to match a caller-supplied TypeHash and generic data rule
  (`0` no data check, `1` exact 8-byte zero u64, `2` exact 8-byte nonzero u64).
  These helpers reject duplicate, missing, unbalanced, wrong-type, or
  wrong-data lock-only/type-only MetaPoint pairs.
  `ckb::require_lock_match_master_out_point_pairs_from_data(input_source,
  output_source, 16, 20, 52)` covers the Limit Order Match bridge where input
  order cells may still encode Mint-relative master distance while output order
  cells must encode Action::Match with absolute master OutPoint bytes.
  `ckb::input_out_point_tx_hash(source)` additionally exposes the full input
  OutPoint transaction hash as an addressable `Hash`, so generated verifiers no
  longer need low-word probes for OutPoint identity.
- Limit Order core product-sum value conservation now uses executable
  `c256::require_sum2_products_lte`; first-class `C256/u256` values,
  additional checked arithmetic operators, and protocol-specific MetaPoint
  maps are still not native CellScript concepts.
