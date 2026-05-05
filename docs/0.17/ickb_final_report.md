# iCKB CellScript Completeness Benchmark Final Report

## Executive Summary

CellScript now has a **selected executed iCKB equivalence matrix**.

The benchmark is no longer model-level-only. The current active matrix contains
75 original-vs-CellScript differential rows and 0 active `MODEL` rows. The
production gate now passes as `EXECUTED_CKB_VM_DIFF` / `PROVEN` for that
selected matrix because every selected row carries original-side execution,
CellScript-side execution, matching pass/fail status, hashes, cycles,
transaction sizes, occupied capacity, fees, and named reject modes. Fourteen
CellScript-only VM rows and eight original-side VM rows remain as
`supporting_evidence`; they are not counted as equivalence claim rows.
First-class `Script` support is no longer a 0.17 blocker; it is explicitly
scoped to 0.18.

The 0.17 branch moves several 0.16 audit blockers from comments/model fields
into typed compiler/runtime surface:

- `--primitive-strict=0.17` exists.
- HeaderDep SourceViews and `dao::accumulated_rate(source::header_dep(i))`
  compile and lower to generated runtime helpers. The accumulated-rate helper
  now reads DAO field offset `8`, matching iCKB's `AR_OFFSET = 160 + 8`.
  It accepts CKB `LengthNotEnough` when reading the 8-byte prefix of the DAO
  field, matching the ckb-std pattern used by iCKB for prefix reads.
- `dao::input_accumulated_rate(source::input(i))` and
  `dao::input_accumulated_rate(source::group_input(i))` now lower to
  `LOAD_HEADER` at absolute header offset `160 + 8`, matching original iCKB
  `extract_accumulated_rate(index, source)` for input-side receipt/deposit
  accounting paths. Generated calls now check the helper status register and
  fail closed when the committed header cannot be loaded.
- `dao::require_header_dep_for_input(input, header)` now lowers to a
  fail-closed helper that loads the 32-byte DAO field from the input's committed
  header and the supplied HeaderDep, then rejects mismatches with stable
  `dao-header-lineage-mismatch`.
- iCKB-specific receipt byte layout, output deposit/receipt pairing, and group
  receipt mint-sum recomputation are intentionally **not** exposed as generic
  `dao::*` helpers. They remain in the iCKB benchmark fixture/differential-test
  layer until CellScript has protocol-neutral SourceView scans, byte decoding,
  and aggregate equation lowering.
- `dao::has_dao_type(source_view)` now lowers to a full 32-byte TypeHash
  classifier against the iCKB DAO hash constant.
- `dao::is_deposit_data(source_view)` and
  `dao::is_withdrawal_request_data(source_view)` now lower to executable
  `LOAD_CELL_DATA` classifiers matching iCKB's exact 8-byte DAO data rule:
  all-zero means deposit, non-zero means withdrawal request.
- `dao::require_input_since_at_least(input, required_since)` now lowers to
  `LOAD_INPUT_BY_FIELD` on Input/GroupInput SourceViews and fail-closed
  unsigned `since >= required_since` checks with stable
  `dao-maturity-violation`.
- `ckb::since_epoch_absolute(number, index, length)` and
  `ckb::since_epoch_relative(number, index, length)` now encode CKB RFC0017
  EpochNumberWithFraction since values with number/index/length bounds checks
  and stable `ckb-since-malformed` failure status.
- `dao::require_input_relative_epoch_since_at_least(input, number, index,
  length)` now loads the selected input since, requires RFC0017 relative epoch
  flags, validates the loaded epoch fraction, and compares rational epoch
  fractions before allowing a redeem-like path.
- `ckb::current_role()` is available for lock/type entry role checks.
- `ckb::cell_capacity`, `ckb::cell_output_index`,
  `ckb::cell_occupied_capacity`, `ckb::cell_unoccupied_capacity`,
  `ckb::cell_lock_hash_low`, `ckb::cell_type_hash_low`, and cell data-size
  helpers compile through SourceViews. Occupied capacity now uses the CKB byte
  formula `8 + lock(33+args) + optional type(33+args) + data_len`, multiplied
  by `100_000_000` shannons. The low-word hash helpers intentionally expose
  only diagnostics and are not a full Script object substitute.
- `ckb::require_cell_lock_hash` and `ckb::require_cell_type_hash` compile to
  fail-closed full 32-byte SourceView hash equality checks.
- `ckb::require_cell_lock_script_hash_type` and
  `ckb::require_cell_type_script_hash_type` compile to fail-closed Molecule
  Script prefix checks that bind code_hash and hash_type without constraining
  variable-length args.
- `ckb::input_out_point_index`, `ckb::input_out_point_tx_hash_low`,
  `ckb::require_input_out_point_tx_hash`, and
  `ckb::require_input_out_point` compile to fail-closed `LOAD_INPUT_BY_FIELD`
  OutPoint helpers for input/group-input SourceViews. The full OutPoint helper
  binds the 32-byte tx hash and the 32-bit index in one runtime check.
- `ckb::require_metapoint_relative(base, related, distance)` now lowers a
  pairwise iCKB-style MetaPoint relation: input/group-input pairs compare full
  OutPoint tx hashes and signed relative indexes, while output/group-output
  pairs compare encoded output indexes.
- `ckb::require_lock_type_metapoint_pairs(source, distance)` and
  `ckb::require_type_lock_metapoint_pairs(source, distance)` now scan
  current-script lock-only/type-only cells and reject duplicate, missing, or
  unbalanced fixed-distance MetaPoint pairs.
- `ckb::require_lock_type_metapoint_pairs_from_i32_data(source, offset)` and
  `ckb::require_type_lock_metapoint_pairs_from_i32_data(source, offset)` now
  load a signed little-endian i32 distance from each base cell's data before
  running the same one-to-one current-script MetaPoint pair scan.
- `ckb::require_lock_match_master_out_point_pairs_from_data(input_source,
  output_source, action_offset, tx_hash_offset, index_offset)` now covers the
  Limit Order Match bridge: current-script lock-only input orders may encode
  their master as Mint-relative `i32` distance or Match absolute OutPoint, while
  current-script lock-only output orders must encode Match absolute master
  OutPoint bytes. Duplicate, missing, or ambiguous matches fail closed.
- partial xUDT helpers (`amount_low`, `amount_high`,
  `require_owner_mode_input_type`) are wired into typecheck/IR/codegen; the
  owner-mode type binding check now compares the full 32-byte input type hash.
- `xudt::require_owner_mode_type_args(source, owner_hash, flags)` now loads the
  cell Type Script and checks exact Molecule args bytes for the iCKB-style
  `[32-byte owner hash, 4-byte little-endian flags]` owner-mode pattern.
- `xudt::require_owner_mode_type_args_current_script(source, flags)` now loads
  the current script hash with `LOAD_SCRIPT_HASH` and checks the xUDT Type
  Script args against `[current_script_hash, flags]`, removing the manual owner
  hash parameter for the iCKB Logic owner-mode path.
- `ckb::current_script_hash() -> Hash` now exposes the same `LOAD_SCRIPT_HASH`
  path as a general addressable 32-byte `Hash`, so generic lock/type/xUDT
  helpers can consume the current script hash without xUDT-specific glue.
- `ckb::require_current_script_args_empty()` now loads the executing script via
  `LOAD_SCRIPT`, validates the empty-args Molecule Script layout, and rejects
  non-empty current script args with stable `script-args-mismatch`. It also
  scans Output lock scripts and rejects any same-code/hash-type output lock
  whose args are non-empty, matching the original iCKB `has_empty_args` shape.
  The iCKB benchmark specs call this guard on their entry paths.
- `ckb::require_cell_lock_args_empty(source)` and
  `ckb::require_cell_type_args_empty(source)` now load SourceView lock/type
  Script fields, validate the empty-args Molecule Script layout, and reject
  non-empty args with stable `script-args-mismatch`.
- `ckb::require_cell_lock_args_hash(source, expected_hash)` and
  `ckb::require_cell_type_args_hash(source, expected_hash)` now load
  SourceView lock/type Script fields, validate a 32-byte `Script.args`
  Molecule Bytes payload, and compare the payload against a full `Hash`.
  `owned_owner.cell::owned_unlock` uses this path for lock-args owner binding.
- `ckb::require_cell_type_script_hash_type(source, code_hash, hash_type)` is
  now used by `ickb_logic.cell::mint_from_receipt` to bind xUDT Type Script
  identity before owner-mode args checks.
- `xudt::require_group_amount_conserved()` now scans current xUDT type-group
  inputs/outputs, loads 16-byte little-endian amounts, checks u128 sum overflow,
  and requires exact input/output equality.
- `xudt::require_group_amount_minted(delta)` and
  `xudt::require_group_amount_burned(delta)` now scan the same xUDT type group
  and require exact `outputs == inputs + delta` or
  `inputs == outputs + delta` u128 equality. The iCKB mint benchmark calls the
  minted-delta helper for token-side minting, and the withdrawal benchmark calls
  the burned-delta helper for token-side burning.
- Local `u128` add/sub/mul/div/compare values are now materialized as 16-byte
  stack values and can be passed to runtime helpers. Checked `u128`
  multiplication rejects overflow, checked `u128` division uses restoring
  division and rejects zero denominators, and local `u128` helper returns use
  `a0(low)/a1(high)`, avoiding stale callee-stack pointers.
- Declared exact xUDT type-group amount aggregates are now bridged to matching
  helpers in ProofPlan/0.17 strict mode. Accepted patterns are limited to
  `type_group`, `scope: group`, exact
  `assert_sum(group_outputs<T>.amount) == assert_sum(group_inputs<T>.amount)`,
  `assert_delta(group_outputs<T>.amount, delta, scope = group)` for minting,
  and `assert_delta(group_inputs<T>.amount, delta, scope = group)` for burning.
  Strict mode rejects the declaration with `PP0170` when no selected entry emits
  the matching xUDT group amount helper.
- `c256::require_product_lte/eq` and `c256::require_sum2_products_lte/eq`
  lower to RISC-V helpers using `mulhu` for u128->u256 product limbs, checked
  u256 addition, stable aggregate-mismatch errors, and internal ELF assembly
  coverage.
- the iCKB oversized-deposit 10% discount formula is expressed directly in
  `ickb_logic.cell`, and branch/add/sub/mul/div/compare plus local `u128`
  return lowering are covered. Receipt mint values can now be independently
  recomputed by generated runtime code for one input and for the current
  type-group receipt input sum, and selected receipt/mint fixtures now have
  original-vs-CellScript CKB VM differential evidence. Generic aggregate
  lowering, byte-accurate receipt decoding, and full deposit/header withdrawal
  accounting linkage are still open.
- 0.17 scalar SourceView/DAO/xUDT helpers now use a fail-closed status ABI:
  helper success returns `a1 = 0`; helper failure returns `a1 = error_code`,
  and generated call sites exit with that code before treating `a0` as data.
- runtime error codes now include HeaderDep, SourceView, DAO, script-role, xUDT,
  OutPoint, MetaPoint, and aggregate-accounting families.

This now proves the selected executed iCKB behavioural equivalence matrix.
Generalizing beyond that selected matrix still has open engineering work:
executable computed aggregate accounting, full DAO second-withdrawal/header
binding, first-class `C256/u256` values, and protocol-specific MetaPoint maps
as ordinary language constructs remain open.
First-class arbitrary `Script` values beyond the empty/32-byte/xUDT owner-mode
patterns and the SourceView code_hash/hash_type identity helper are explicitly
deferred to the 0.18 ScriptRef/ScriptArgs track.

The production-equivalence standard has been tightened into an executable gate:
`tests/benchmarks/ickb_diff/matrix.json` now records
`equivalence_status = PROVEN` and `production_equivalence_claim = true` for the
selected executed matrix. `tests/ickb_diff.rs` rejects any `PROVEN` claim unless
the matrix carries original iCKB binary hashes, generated
CellScript artifact hashes, CKB VM/testtool evidence, fixture hashes, exit
codes, named reject modes, cycle counts, transaction-size measurements,
transaction context hashes, occupied-capacity measurements, and fee evidence.

## Implemented Artifacts

- `tests/benchmarks/ickb_specs/README.md`
- `tests/benchmarks/ickb_specs/ickb_logic.cell`
- `tests/benchmarks/ickb_specs/limit_order.cell`
- `tests/benchmarks/ickb_specs/owned_owner.cell`
- `tests/support/ickb_model.rs`
- `tests/ickb_benchmark.rs`
- `tests/v0_17.rs`
- `tests/ckb_compat_runner.rs`
- `tests/ickb_diff.rs`
- `tests/benchmarks/ickb_positive/*.json`
- `tests/benchmarks/ickb_negative/*.json`
- `tests/benchmarks/ickb_diff/matrix.json`
- `docs/0.17/ickb_production_equivalence_gate.md`
- `docs/0.17/*.md`

All iCKB-specific executable/model assets are scoped to `tests/benchmarks`,
`tests/support`, and the 0.17 audit documents. The generic compiler, stdlib,
and CLI expose only protocol-neutral CKB primitives; iCKB receipt layouts,
deposit/receipt pairing, and mint-sum models are not surfaced as public
CellScript helpers.

`tests/support/ickb_model.rs` centralizes the iCKB-style model fixture verifier
for integration tests. It is deliberately not exported by the generic library or
CLI.
`tests/ckb_compat_runner.rs` and `cellc verify-ckb-fixtures` now derive
deterministic fixture verdicts from transaction-shape semantics for sUDT/xUDT
amount conservation, DAO maturity fixture labels, TYPE_ID duplicate outputs,
ACP/Cheque owner or receiver mismatch, Omnilock auth failure, and
capacity-report sanity. Neither verifier executes CKB VM.

The branch also hardens the earlier 0.16 assurance review items:

- ProofPlan soundness now binds verifier obligations to ProofPlan
  origin/scope as well as category, feature, status, and detail, while
  local/runtime ProofPlan records are compared by full serialized content.
  Duplicate obligation keys, checked records without concrete reads/coverage,
  missing checked-obligation labels, and missing strict-mode source spans for
  source-declared invariants are rejected. Cell-access records must also keep
  their SourceView/source class aligned with `reads` (`Output` requires
  `output`, `HeaderDep` requires `header_dep`, `SourceView` requires
  `source_view`, etc.).
- `validate-tx` rejects bare evidence tokens and validates concrete evidence
  payloads for required cells, outputs, deps, witnesses, capacity reports,
  TYPE_ID construction, uniqueness, and manual ProofPlan-gap review. Evidence
  indexes are checked against the transaction shape, capacity evidence fails if
  it reports under-capacity outputs, and TYPE_ID args must be canonical 32-byte
  hex. When evidence and the referenced transaction object both provide a
  concrete field such as `lock_hash`, `type_hash`, `capacity`, witness bytes, or
  dep metadata, the values must match.
- CKB protocol stdlib descriptors are not marked `stable`; implemented
  runtime-backed surfaces are explicitly labelled partial.
- `solve-tx` is explicitly emitted as `template-only` /
  `non-executable-template` with `can_submit=false`, unresolved HeaderDep slots,
  and external solver steps listed. Its builder-assumption evidence schema now
  describes required indexed payload arrays, concrete fields, capacity checks,
  TYPE_ID checks, and manual-review checks. It is not represented as live cell
  selection, dep resolution, fee solving, witness placement, or dry-run
  validation.

The exact finding-by-finding closure matrix is maintained in
`docs/0.17/review_findings_closure.md`.

## Coverage Table

| Semantic item | CellScript support | Test coverage | Remaining gap | Severity |
|---|---|---|---|---|
| Deposit phase 1 receipt creation | EXPRESSIBLE_WITH_ESCAPE_HATCH | selected deposit/receipt grouping pass and capacity/receipt mismatch failures have CKB VM differential evidence | generic aggregate syntax, SourceView output scans, byte decoding, and full manifest closure | HIGH |
| Deposit phase 2 mint | EXPRESSIBLE_WITH_PATTERN for token delta; ESCAPE_HATCH for receipt aggregation | selected mint, receipt-group exact-mint, amount inflation/deflation, wrong-rate, wrong-header, malformed receipt, and wrong-xUDT fixtures have CKB VM differential evidence | generic receipt/deposit aggregate lowering, byte-accurate receipt decoding, and full redeem coverage | HIGH |
| Receipt consumption/no double use | NATIVE plus model duplicate check | duplicate receipt negative | prior-cell lineage API | MEDIUM |
| iCKB transfer | NATIVE for token resource model | positive transfer + xUDT owner-mode args and group conservation helper compile/ELF tests + strict conservation/delta aggregate bridge positive/negative tests | full xUDT ABI compatibility and 0.18 first-class Script API | MEDIUM |
| Withdrawal/redeem | EXPRESSIBLE_WITH_PATTERN | positive withdrawal, immature redeem negative, executable DAO type/data classifiers, RFC0017 relative epoch-since encoding, and epoch maturity helper in the spec | request/deposit/header binding, second-withdrawal flow, and iCKB VM differential | HIGH |
| Exact accounting | EXPRESSIBLE_WITH_ESCAPE_HATCH | selected receipt/mint/amount inflation/deflation/group exact-mint differential rows plus executable xUDT group conservation and minted/burned delta helpers; local computed `u128` add/sub/mul/div deltas are addressable | automatic full receipt/deposit/DAO-rate accounting and byte-accurate receipt decoding | BLOCKER |
| Owned-Owner pairing | EXPRESSIBLE_WITH_PATTERN | selected valid, wrong-owner, missing/duplicate/relative-mismatch, script-misuse, non-withdrawal, data-length, type-hash, and data-rule cases have original-vs-CellScript CKB VM differential evidence | first-class MetaPoint map and protocol-neutral map/query API | MEDIUM |
| Limit Order fulfilment | EXPRESSIBLE_WITH_PATTERN | selected positive, min-match boundary, underpayment, no-payment, wrong-asset, and conservation cases have CKB VM differential evidence plus C256 product-sum and MetaPoint helper coverage | first-class C256/u256 values, native action-aware order decoding, and first-class MetaPoint/OutPoint maps | HIGH |
| Script role confusion | EXPRESSIBLE_WITH_PATTERN | selected Owned-Owner script misuse and xUDT owner-mode args cases have VM evidence; v0.17 also has current-role, iCKB-style current/output-lock empty args, full lock/type hash, SourceView code_hash/hash_type, and SourceView empty/32-byte args helpers | full Script object and lock/type group scans | MEDIUM |
| Witness malformation | EXPRESSIBLE_WITH_PATTERN | non-executable assumption only | full witness/Molecule parser and auth primitives | HIGH |
| CellDep substitution | EXPRESSIBLE_WITH_PATTERN | runner now includes fixture CellDeps in transactions; remaining production claim requires manifest/deployment closure | deploy/runtime dep verification integration | HIGH |

## Test Results

Focused commands run:

```bash
cargo test --locked -p cellscript --test v0_17 -- --test-threads=1
cargo test --locked -p cellscript --test ickb_benchmark -- --test-threads=1
cargo test --locked -p cellscript --test v0_16 -- --test-threads=1
cargo test --locked -p cellscript --test ckb_compat_runner -- --test-threads=1
cargo test --locked -p cellscript --test ickb_diff -- --test-threads=1
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
cargo test --locked -p cellscript parser::tests::test_if_expr_with_all_caps_constant_before_block -- --test-threads=1
cargo test --locked -p cellscript --test v0_17 ickb_benchmark_specs_compile_under_0_17_strict_source_mode -- --test-threads=1
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
target/debug/cellc tests/benchmarks/ickb_specs/ickb_logic.cell --target-profile ckb --target riscv64-asm --debug
cargo run --locked -p cellscript --bin cellc -- tests/benchmarks/ickb_specs/ickb_logic.cell --target riscv64-elf --target-profile ckb --entry-action mint_from_receipt -o /tmp/cellscript_ickb_logic_mint_from_receipt.elf
git diff --check
```

Result: all passed in the local workspace during 0.17 implementation.
The direct `ickb_logic.cell` compile produced a RISC-V assembly artifact,
confirming the benchmark spec now covers the discount branch formula at compile
time. The `mint_from_receipt` ELF compile confirms the protocol-neutral xUDT
delta and DAO/header helpers are accepted by the internal assembler.
`tests/v0_17.rs` also verifies the xUDT aggregate bridge, computed local
`u128` add/sub/mul/div/compare/helper-return materialization, C256 helper family,
input-OutPoint helpers, current/output-lock empty-args `LOAD_SCRIPT`
guard, signed `i32` ABI lowering, and
the DAO-withdrawal current-script MetaPoint pair helper plus the Limit Order
Match absolute master-OutPoint pair helper at assembly and, where applicable,
internal ELF output levels.

Original iCKB check:

```bash
cd /tmp/cellscript-ickb-prod-eq/v1-core/scripts
cargo test --locked
```

Result: the iCKB Rust crates compiled, but the `tests` crate failed because
prebuilt Capsule script binaries were missing under `scripts/build/debug`.
`capsule` and `cross` were not installed locally; only `docker` was present.
This is recorded as build-harness evidence, not behavioural equivalence.

The differential matrix is explicitly labelled `EXECUTED_CKB_VM_DIFF`,
`PROVEN`, and `production_equivalence_claim = true`. It contains 75 selected
original-vs-CellScript differential rows and 0 active `MODEL` rows. Fourteen
CellScript-only CKB VM rows and eight original-side CKB VM rows are retained as
`supporting_evidence`, not selected equivalence rows. The active matrix no
longer mixes synthetic model-level benchmark rows with executed VM evidence;
legacy non-executable assumptions are tracked separately under
`retired_model_assumptions`, while active `non_executable_model_assumptions`
must remain empty for the `PROVEN` claim to pass. The differential rows record
fixture hashes, transaction context hashes, original and generated
artifact hashes, exit codes, pass/fail status, cycles, transaction size,
occupied capacity, fee, and failure modes. The stricter production-equivalence
gate is tested by `tests/ickb_diff.rs`: differential rows require complete
per-row execution objects, non-executable assumptions block `PROVEN`, and a
simulated `PROVEN` claim without execution evidence is rejected. The standard
CKB compatibility runner and `cellc verify-ckb-fixtures` compute fixture
verdicts from the JSON transaction shape instead of merely replaying the
expected exit code.

## Compiler Changes Made

- `src/lib.rs`: added `0.17` primitive strict mode, 0.17 metadata gate, runtime
  metadata coverage for DAO/xUDT/SourceView helpers, the fail-closed check that
  a helper-backed xUDT aggregate declaration is accepted only when the generated
  runtime access is present, plus DAO header-lineage and DAO input-since runtime
  access metadata.
- `src/types/mod.rs`: typed `ckb::*`, `dao::*`, and `xudt::*` runtime calls and
  forbids them in pure functions.
- `src/ir/mod.rs`: lowers new builtins to constants or runtime helper calls.
- `src/codegen/mod.rs`: emits SourceView packing, cell-field/hash-low,
  input-OutPoint, pairwise MetaPoint relative, fixed-distance lock/type
  MetaPoint pair-cardinality scans, full-hash requirement,
  DAO-rate/header-lineage/type-and-data classifiers, RFC0017 epoch-since
  encoder, DAO relative epoch maturity helper, iCKB-style current/output-lock
  empty args via `LOAD_SCRIPT` plus output lock scans, SourceView empty and
  32-byte Hash Script args helpers, Limit Order Match absolute master-OutPoint
  scan, xUDT helper assembly, and C256 product-sum helper assembly with
  fail-closed status checks, stable failure codes, and signed `i32` ABI
  sign-extension.
- `src/runtime_errors.rs` and `docs/CELLSCRIPT_RUNTIME_ERROR_CODES.md`: added
  0.17 CKB protocol error families.
- `src/stdlib/ckb_protocols/*`: added `std::dao` and partial runtime-backed
  `std::xudt` descriptors.
- `src/parser/mod.rs`: tightened struct-initializer detection so all-caps
  constants such as `ICKB_SOFT_CAP_PER_DEPOSIT` can appear immediately before
  if-expression branch blocks.
- `src/proof_plan/mod.rs`: records helper-required coverage for the exact xUDT
  group amount aggregate pattern and maps combined `GroupInput/GroupOutput`
  plus `Input/GroupInput` runtime sources to concrete ProofPlan reads.
- `src/proof_plan/soundness.rs`, `src/assumptions.rs`, and
  `src/cli/commands.rs`: hardened 0.16 assurance checks surfaced by review.
- `tests/support/ickb_model.rs`: keeps the iCKB-style positive/negative fixture
  evaluator inside the test suite so benchmark logic does not become generic
  CellScript product surface.
- `src/cli/commands.rs`: added `verify-ckb-fixtures`, a CLI-accessible model
  verifier for standard CKB compatibility fixture manifests.

## Unresolved Blockers

1. Executable computed aggregate lowering for iCKB exact accounting. Simple
   xUDT group amount conservation and minted/burned token-side deltas now have
   executable helpers and strict-mode declared-aggregate bridges. Input-side
   accumulated-rate reads can now use the same committed input header offset as
   original iCKB, and computed local `u128` add/sub/mul/div/compare values can
   feed generic runtime helpers. The remaining blocker is the automatic
   derivation and executable enforcement of the full input/output
   receipt/deposit/header/DAO-rate accounting equation, including iCKB receipt
   byte decoding and withdrawal-side deposit terms, without hardcoding iCKB
   semantics into the generic `dao::*` stdlib.
2. DAO full second-withdrawal/header binding. DAO HeaderDep and input-header
   accumulated-rate loading, DAO-field input/HeaderDep lineage, DAO type-hash
   classification, 8-byte deposit/withdrawal data classifiers, RFC0017
   epoch-since constructors, and a relative epoch maturity helper exist, but
   request/deposit/header lineage and original iCKB VM differential evidence
   are still missing.
3. First-class `Script` API is a 0.18 topic, not a 0.17 blocker. v0.17 has
   `ckb::current_script_hash()`, a `LOAD_SCRIPT`
   current/output-lock empty-args guard matching iCKB `has_empty_args`, full
   32-byte SourceView hash requirement helpers, SourceView empty-args helpers,
   SourceView 32-byte args Hash helpers, SourceView code_hash/hash_type identity
   helpers, and xUDT owner-mode args verifiers, including current-script-hash
   binding. 0.18 should add read-only ScriptRef/ScriptArgs first; arbitrary
   Script construction, deploy manifest resolution, TYPE_ID construction,
   script-hash synthesis, and cell dep solving remain out of that first 0.18
   scope.
4. Full xUDT compatibility, including arbitrary args construction modes and
   full 128-bit amount compatibility checks.
5. Native MetaPoint group APIs. Input OutPoint tx-hash/index reads, a combined
   tx-hash+index requirement helper, pairwise source-index relation arithmetic,
   fixed-distance plus i32-data-driven current-script lock/type pair scans, the
   Owned-Owner DAO-withdrawal pair scan, and the Limit Order Match master
   OutPoint scan exist, but full action-aware MetaPoint/OutPoint maps are not
   first-class protocol primitives.
6. First-class checked `u256`/`C256` values. C256 product and two-product-sum
   requirement helpers exist, but not the whole arithmetic/value surface. The
   iCKB-required signed `i32` ABI path now exists.
7. Production-equivalence manifest closure. Per-row binary hashes,
   fixture/context hashes, exit codes, cycle counts, transaction size,
   occupied capacity, fee evidence, and named reject modes are now recorded for
   the executed differential rows, but the top-level `PROVEN` manifest remains
   intentionally unset until all selected production scenarios are executable
   and the non-executable assumption registry is empty.

## Recommended Next Steps

1. Implement aggregate scan lowering for exact equality and group-by receipt
   matching.
2. Extend the DAO bridge from epoch maturity checks into full
   second-withdrawal request/deposit/header lineage with additional
   original-vs-CellScript CKB VM differential rows.
3. Add `Script` and `MetaPoint` typed values with source scans and relation
   helpers.
4. Extend checked `u256`/`C256` from requirement helpers into first-class value
   types and cover the remaining Limit Order min-fill arithmetic.
5. Add production owner-auth witness fixtures and byte-accurate receipt
   decoding rows before clearing the remaining non-executable assumptions.
6. Keep the current CKB testtool differential runner strict: every new
   production-equivalence row must carry measured execution evidence before it
   can enter the active matrix.
7. Keep active matrix rows executable-only; any legacy non-executable
   assumption must remain outside the active matrix until it can satisfy
   `docs/0.17/ickb_production_equivalence_gate.md` evidence requirements.

## Honesty Statement

This benchmark is no longer model-level-only. The 0.17 branch has broad
partial CKB VM differential evidence for selected normalized iCKB fixture
classes, and it proves that several previously missing CKB-native primitives
are expressible, typed, lowered, and covered by compiler tests. It still does
not prove production equivalence. Witness/auth production fixtures, generic
aggregate lowering, byte-accurate receipt decoding, complete DAO redeem
accounting, and production evidence-manifest closure remain open; first-class
`Script` work is tracked as 0.18 scope.

Any future report claiming production equivalence must first move the diff
matrix from broad partial executed evidence to complete CKB VM differential
evidence for every selected row, clear the non-executable assumption registry,
and pass the production-equivalence gate in
`tests/ickb_diff.rs` and the test-only iCKB differential validator.
