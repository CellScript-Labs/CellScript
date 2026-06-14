# CellScript 0.17 Roadmap

**Status**: Partially implemented on `cellscript-0.17`
**Scope**: iCKB-Grade CKB Protocol Completeness
**Depends on**: v0.15 scoped invariants, v0.16 metadata assurance/tooling

## Goal

CellScript 0.17 closes the gap between auditable CKB metadata and executable
CKB-native protocol semantics.

The iCKB benchmark showed that CellScript can model protocol intent, but cannot
yet honestly claim support for production-grade CKB protocols whose security
depends on HeaderDeps, DAO accumulated rates, lock/type role dual-use, xUDT
script binding, transaction-wide computed accounting, OutPoint relations, and
CKB VM differential tests.

0.17 is being implemented toward the following statement:

> CellScript can express, compile, execute-test, and audit a non-trivial
> iCKB-style CKB protocol subset without hiding critical invariants in comments,
> builder-only assumptions, or raw script escape hatches.

It should still not claim full iCKB equivalence until the differential matrix
executes both the original iCKB Rust scripts and generated CellScript scripts on
the same CKB transaction fixtures.

## Implementation Status In This Branch

Implemented:

- `--primitive-strict=0.17`.
- HeaderDep SourceView and `dao::accumulated_rate(source::header_dep(i))`
  compile/typecheck/IR/codegen surface.
- `dao::input_accumulated_rate(source::input(i) |
  source::group_input(i))` compile/typecheck/IR/codegen surface, reading the
  original iCKB accumulated-rate offset `160 + 8` through `LOAD_HEADER` and
  propagating helper status fail-closed.
- `dao::require_header_dep_for_input(input, header)` compile/typecheck/IR/codegen
  surface, comparing the input header DAO field to the supplied HeaderDep DAO
  field with fail-closed mismatch diagnostics.
- iCKB-specific receipt data binding, receipt mint-value recomputation, group
  receipt mint-sum scanning, and output deposit/receipt pairing must not become
  generic `dao::*` helpers. Solve these through protocol-neutral SourceView
  scans, byte decoding primitives, and aggregate equation lowering.
- `dao::is_deposit_data(source_view)` and
  `dao::is_withdrawal_request_data(source_view)` compile/typecheck/IR/codegen
  surface, matching iCKB's exact 8-byte DAO data classifiers.
- `dao::has_dao_type(source_view)` compile/typecheck/IR/codegen surface,
  matching iCKB's hardcoded DAO type-hash classifier.
- `ckb::current_role()` for lock/type entry role checks.
- `ckb::cell_capacity`, `ckb::cell_occupied_capacity`,
  `ckb::cell_unoccupied_capacity`, `ckb::cell_output_index`,
  `ckb::cell_lock_hash_low`, `ckb::cell_type_hash_low`, and cell data-size
  helpers. Occupied capacity uses the CKB byte formula for capacity field,
  lock/type scripts, and cell data bytes, then converts bytes to shannons.
- `ckb::require_cell_lock_hash` and `ckb::require_cell_type_hash`, lowering to
  full 32-byte SourceView hash equality checks.
- `ckb::require_cell_lock_args_empty`,
  `ckb::require_cell_type_args_empty`,
  `ckb::require_cell_lock_args_hash`, and
  `ckb::require_cell_type_args_hash`, lowering to Molecule `Script` field
  checks for empty args or exact 32-byte owner/type args.
- `ckb::require_cell_lock_script_hash_type` and
  `ckb::require_cell_type_script_hash_type`, lowering to Molecule `Script`
  prefix checks for code_hash/hash_type identity without a protocol-specific
  helper.
- `ckb::require_current_script_args_empty`, lowering to `LOAD_SCRIPT` for the
  executing script and scanning Output locks so same-code/hash-type output
  locks also have empty `Script.args`, matching iCKB `has_empty_args`.
- `ckb::input_out_point_index`, `ckb::input_out_point_tx_hash_low`,
  `ckb::require_input_out_point_tx_hash`, and
  `ckb::require_input_out_point`, lowering to `LOAD_INPUT_BY_FIELD` OutPoint
  helpers for input/group-input SourceViews. The combined helper binds both the
  32-byte tx hash and 32-bit index.
- partial xUDT layout/binding helpers, including full 32-byte owner-mode input
  type-hash binding, exact owner-mode type args verification for the iCKB-style
  `[owner_hash, flags_u32_le]` pattern, and a current-script variant that binds
  owner args to `LOAD_SCRIPT_HASH(current script)`.
- a general `ckb::current_script_hash() -> Hash` primitive that lowers to
  `LOAD_SCRIPT_HASH` and produces an addressable 32-byte hash for generic
  lock/type/xUDT helpers.
- executable xUDT type-group amount conservation helper for simple transfer
  invariants (`xudt::require_group_amount_conserved()`).
- executable xUDT type-group minted/burned delta helpers
  (`xudt::require_group_amount_minted(delta)` and
  `xudt::require_group_amount_burned(delta)`), covering token-side exact
  delta checks when the delta is available as an addressable `u128`.
- executable local `u128` add/sub/mul/div/compare materialization for helper operands,
  including local `u128` function returns through `a0(low)/a1(high)`. This
  closes the stale callee-stack-pointer class for computed `u128` helper
  inputs. Division uses checked restoring division and rejects zero
  denominators; `u128` modulo remains fail-closed because iCKB does not need it.
- ProofPlan/0.17 strict bridge for exact xUDT `type_group`/`group` amount
  aggregate patterns. Strict mode accepts conservation only when a selected
  entry emits `xudt::require_group_amount_conserved()`, mint deltas only when it
  emits `xudt::require_group_amount_minted(delta)`, and burn deltas only when it
  emits `xudt::require_group_amount_burned(delta)`; missing helper coverage is
  rejected with `PP0170`.
- executable C256 requirement helpers:
  `c256::require_product_lte/eq` and
  `c256::require_sum2_products_lte/eq`, backed by RISC-V `mulhu`, u128->u256
  product limbs, checked u256 addition, and internal ELF assembler coverage.
- signed `i32` primitive support for the iCKB Owned-Owner relative-distance ABI:
  parser/type/IR support, fixed 4-byte metadata, entry witness little-endian
  encoding, generated RISC-V sign extension, and internal ELF assembler support
  for the emitted arithmetic shift.
- fail-closed scalar runtime helper ABI for 0.17 SourceView/DAO/xUDT reads
  (`a1 = 0` on success, `a1 = runtime_error_code` on failure).
- deterministic model execution for standard CKB compatibility fixtures instead
  of replaying JSON expected exit codes, now available as
  `cellc verify-ckb-fixtures`.
- deterministic model execution for iCKB-style positive and adversarial
  fixtures, now centralized in `tests/support/ickb_model.rs` and kept inside
  the integration test suite.
- parser disambiguation for all-caps constants before branch blocks, allowing
  the iCKB oversized-deposit 10% discount formula to compile directly in
  `tests/benchmarks/ickb_specs/ickb_logic.cell`.
- executable production-equivalence claim gate in
  `tests/benchmarks/ickb_diff/matrix.json` and `tests/ickb_diff.rs`; current
  status is `PARTIAL_CKB_VM_EXECUTION` / `NOT_PROVEN`, with three
  original-vs-CellScript differential rows and remaining iCKB scenarios still
  model-level.
- hardened 0.16 assurance paths: ProofPlan obligation origin/scope matching,
  duplicate/semantically incomplete ProofPlan record rejection, cell-access
  source/read consistency checks, concrete builder evidence payload validation,
  transaction-shape and transaction-content cross-checks, non-stable protocol
  descriptor labels, and non-executable template-only `solve-tx` output with
  machine-readable evidence schema requirements. The closure matrix is tracked
  in `docs/0.17/review_findings_closure.md`.
- stable 0.17 runtime error families.
- `tests/v0_17.rs`, `tests/ckb_compat_runner.rs`, and `tests/ickb_diff.rs`.

Still blocked:

- executable computed aggregate lowering beyond the explicit xUDT transfer,
  token-delta, output deposit/receipt, and receipt mint-value bridges;
- full DAO second-withdrawal binding, broader xUDT compatibility, and original
  iCKB VM evidence beyond the current fail-closed helper bridges;
- full `Script`, first-class action-aware `MetaPoint` maps, additional signed
  integer widths if needed, and first-class C256/u256 value support;
- CKB VM fixture execution and original iCKB differential execution.

## Relationship To 0.15 And 0.16

0.15 made invariants visible through scoped invariant syntax and Covenant
ProofPlan metadata.

0.16 added metadata assurance, builder assumptions, descriptive compatibility
fixtures, transaction-shape validation, deployment/audit tooling, and
Rust-comparative compiler hardening for the freeze-critical subset: IR poison
semantics, backend register contracts, syscall ABI baselines, IR provenance,
and line-aware diagnostic tests.

0.17 must convert the important remaining metadata/model claims into executable
CKB checks and CKB test evidence, and it owns the non-critical comparative-audit
cleanup deliberately kept out of the 0.16 freeze.

| Track | 0.15 | 0.16 | 0.17 |
|---|---|---|---|
| Invariant expression | Source + ProofPlan metadata | Soundness consistency checks | Executable aggregate lowering |
| CKB compatibility | Metadata surface | Descriptive fixtures | Executed accepted/rejected CKB fixtures |
| Builder assumptions | Recorded | Structurally validated | Reduced by native CKB source primitives |
| Protocol stdlib | Macro provenance | Schema stubs | ABI-compatible DAO/xUDT/script helpers |
| Evidence | Compiler tests | Metadata/tooling tests | CKB VM and differential tests |

## Production Completeness Deferred From 0.16

0.16 owns only the P0 plus key P1 compiler-freeze hardening tracked in
`roadmap/CELLSCRIPT_0_16_ROADMAP.md`. 0.17 owns the CKB
production-completeness work that would make iCKB-style protocol claims
meaningful:

- executable CKB VM accepted/rejected fixture runner;
- iCKB-style differential tests against original Rust scripts and generated
  CellScript artifacts;
- full transaction solver with live cell selection, concrete CellDeps/HeaderDeps,
  occupied-capacity calculation, fee/change planning, witness placement,
  signing, and dry-run;
- ABI-compatible protocol stdlib implementations for xUDT, sUDT, TYPE_ID,
  ACP, Cheque, HTLC, DAO, and iCKB-needed script helpers;
- source-to-RISC-V/assembly source maps;
- on-chain deployment verification;
- executable aggregate invariant lowering with exact equality, grouping,
  computed per-cell terms, and overflow-safe accounting.
  xUDT group amount conservation and mint/burn delta helpers now exist as
  interim runtime primitives, and exact declared xUDT aggregates can be
  discharged by explicitly calling the matching helper. iCKB-specific
  output deposit/receipt pairing, receipt data/current-type binding, and receipt
  mint-sum recomputation remain benchmark-only model logic until generic
  aggregate lowering can express them without protocol-specific compiler APIs.

## Comparative Audit Cleanup Deferred From 0.16

The following Rust-comparative audit items remain important, but they do not
block the 0.16 freeze after IR poison, register/syscall gates, IR provenance,
and error-line tests are in place:

- replace the bridge `IrConst::Poisoned` representation with a deeper
  `Lowered<T>` / `LoweredOperand::{Value, Poisoned}` lowering result;
- fix tuple formatter round-trip and `Span::Display` line/column hygiene;
- extend backend validation to per-function stack balance, call targets,
  register clobbers, unsupported pseudo-ops, and ABI drift;
- add exhaustive semantic tests for `instruction_dest`,
  `instruction_operands`, and related IR helper coverage;
- introduce lightweight `IrPhase` / `CodegenPhase` legality markers;
- harden the diagnostic model with warning-level diagnostics and deduplication;
- split `lib.rs`, `types/mod.rs`, and CLI command ownership after the freeze
  without changing behaviour;
- replace ad hoc generic/type-name parsing with structured resolver/type
  boundary data;
- add release tidy checks for debug leftovers, runtime error-code coverage,
  migration diagnostic tests, and lint posture.

## Non-Goals

- Do not make a production-readiness claim without CKB VM evidence.
- Do not weaken iCKB invariants to make examples compile.
- Do not vendor iCKB repositories into CellScript source control.
- Do not treat descriptive JSON fixtures as behavioural equivalence.
- Do not hide HeaderDep, xUDT, DAO, or script-role checks inside comments.
- Do not mark the iCKB diff matrix `PROVEN` unless every executed row includes
  binary hashes, fixture hashes, CKB VM/testtool version, exit codes, cycle
  counts, transaction size, occupied capacity, fee evidence, transaction context
  hashes, and named failure modes.

## P0: CKB Source Semantics Required By iCKB

### 1. HeaderDep And DAO Accumulated Rate Access

**Problem**

iCKB deposit phase 2 and withdrawal require the accumulated rate from the block
header corresponding to the receipt/deposit cell. The current benchmark models
this as an explicit field, which is not production-safe.

**Change**

Add typed CKB header access:

```cellscript
let header = ckb::header_for_input(receipt)
let ar = dao::accumulated_rate(header)
require_header_dep(receipt)
```

Current bridge surface:

```cellscript
let input = source::group_input(0)
let header = source::header_dep(0)
dao::require_header_dep_for_input(input, header)
let ar = dao::accumulated_rate(header)
```

The API must fail closed when:

- the header dep is missing;
- the header cannot be bound to the referenced input;
- the DAO field is malformed;
- the accumulated rate width/layout is wrong.

**Code Areas**

- AST/parser for CKB source expressions
- type checker source binding
- IR runtime source reads
- CKB codegen syscall lowering
- ProofPlan read coverage
- runtime error registry

**Acceptance**

- `wrong_accumulated_rate` and `missing_header_dep` become generated-runtime
  failures, not model-only failures. The current branch has a generated
  DAO-field input/HeaderDep equality helper, but fixture verdicts remain
  model-level until CKB VM/testtool is integrated.
- Metadata records exact HeaderDep reads and source binding.
- `cellc check --primitive-strict=0.17` rejects any iCKB-style accumulated-rate
  claim that is still witness/builder supplied.

### 2. Script Role And Script Identity Primitives

**Problem**

iCKB, Owned-Owner, and Limit Order use the same deployed script as lock in one
cell and type in another. v0.17 now exposes current-role,
`ckb::current_script_hash() -> Hash`, a `LOAD_SCRIPT` current/output-lock
empty-args guard matching iCKB `has_empty_args`, full 32-byte SourceView
lock/type hash requirement helpers, SourceView empty-args and 32-byte args
helpers, generic SourceView Script code_hash/hash_type identity helpers, and an
xUDT owner-mode type-args verifier, including current-script-hash binding, as
auditable bridges. CellScript still cannot express arbitrary script args or
lock/type relation scans as a production-equivalent first-class Script identity
API.

**Change**

Add first-class script identity and role expressions:

```cellscript
let self = ckb::current_script()
require ckb::current_role() == ckb::Role::Type
require cell.lock.script_hash == self.hash
require cell.type.script_hash == self.hash
require_empty_args(self, outputs = true)
```

**Acceptance**

- Script role confusion lowers to a generated runtime check.
- Empty lock/type Script args can be rejected through SourceView helper calls.
- 32-byte owner/type Script args can be bound to a `Hash` operand through
  SourceView helper calls.
- Output lock args and output type args can be scanned.
- ProofPlan distinguishes lock-group, input-type-group, and output-type-group
  coverage without overstating enforcement.

### 3. CKB Cell Source Fields

**Problem**

Limit Order and Owned-Owner depend on OutPoint, output index, occupied capacity,
lock hash, type hash, type args, and outputs-data alignment.

**Change**

Expose typed source fields:

```cellscript
cell.out_point.tx_hash
cell.out_point.index
cell.output_index
cell.capacity
cell.occupied_capacity
cell.unoccupied_capacity
cell.lock.hash
cell.type.hash
cell.data
```

**Acceptance**

- Owned-Owner can bind input OutPoint tx hash and pairwise relative
  index/MetaPoint checks in generated code.
- Limit Order master MetaPoint pair binding can be expressed through
  `ckb::require_metapoint_relative`.
- Fixed-distance current-script lock-only/type-only MetaPoint pair
  cardinality can be checked with `ckb::require_lock_type_metapoint_pairs`
  and `ckb::require_type_lock_metapoint_pairs`.
- Base-cell-data signed i32 current-script lock/type pair cardinality can be
  checked with `ckb::require_lock_type_metapoint_pairs_from_i32_data` and
  `ckb::require_type_lock_metapoint_pairs_from_i32_data`.
- Owned-Owner's exact DAO-withdrawal related-cell filter can now be expressed
  with protocol-neutral filtered MetaPoint scans that bind related TypeHash and
  exact 8-byte zero/nonzero data rules.
- Limit Order Match paths can scan current-script lock-only input/output order
  cells and require outputs to preserve the same absolute master OutPoint with
  `ckb::require_lock_match_master_out_point_pairs_from_data`.
- Capacity violation checks use real occupied/unoccupied capacity.

## P0: Executable Aggregate Invariant Lowering

### 4. Computed Transaction-Wide Accounting

**Problem**

iCKB's core invariant is computed accounting:

```text
input_udt + input_receipts == output_udt + input_deposits
```

Receipt and deposit values are functions of unoccupied capacity and accumulated
rate. 0.15 aggregate primitives are metadata-only and cannot lower this today.

**Change**

Add executable aggregate lowering for:

- transaction/group input scans;
- transaction/group output scans;
- schema-backed CKB Cell classification;
- computed per-cell terms;
- local `u128` add/sub/mul/div/compare terms are implemented for DAO-rate
  formulas;
- iCKB output deposit/receipt pairing, receipt data/current-type binding, and
  current type-group receipt mint-sum recomputation remain benchmark-only model
  logic until they can be represented by typed aggregate lowering;
- exact equality, `<=`, `>=`;
- fail-closed overflow;
- bounded scan limits.

Desired surface:

```cellscript
invariant ickb_exact_accounting {
    trigger: type_group
    scope: transaction
    reads: inputs<IckbToken>.amount,
           inputs<IckbReceipt>,
           outputs<IckbToken>.amount,
           inputs<DaoDeposit>

    assert_sum(inputs<IckbToken>.amount)
      + assert_sum(inputs<IckbReceipt>.quantity * receipt_ickb_value(self))
      == assert_sum(outputs<IckbToken>.amount)
       + assert_sum(inputs<DaoDeposit>.deposit_ickb_value(self))
}
```

**Acceptance**

- `amount_inflation` and `amount_deflation_exact_equality` fail in generated
  CKB verifier code.
- Strict mode rejects metadata-only aggregate invariants.
- Strict mode accepts only the narrow xUDT group amount aggregate bridge when
  `xudt::require_group_amount_conserved()` is actually present in generated
  runtime accesses.
- Overflow and malformed cell data have stable error codes.

### 5. Executable Output Grouping

**Problem**

Deposit phase 1 requires output DAO deposits grouped by unoccupied capacity to
match receipt quantities. The current benchmark checks this in Rust fixtures.

**Change**

Add aggregate grouping primitives:

```cellscript
assert_group_count(outputs<DaoDeposit>.unoccupied_capacity)
  == assert_sum_by(outputs<IckbReceipt>.deposit_amount,
                   outputs<IckbReceipt>.deposit_quantity)
```

**Acceptance**

- `forged_receipt` fails in generated verifier code.
- Multiple receipts for the same deposit size are supported.
- The 64-output DAO bound is expressible and checked.

## P1: Protocol Stdlib Needed For iCKB-Style Contracts

### 6. `std::xudt`

**Change**

Implement ABI-compatible xUDT helpers:

- xUDT amount layout: first 16 bytes, little-endian `u128`;
- owner-mode input-type flags;
- owner-mode Type Script args verifier for script hash + flags is implemented
  for the iCKB `[hash, flags_u32_le]` pattern, including a current-script-hash
  variant; a general type args constructor remains open, while generic 32-byte
  lock/type args binding is covered by `ckb::require_cell_*_args_hash`;
- type hash validation;
- transfer/conservation helpers. `xudt::require_group_amount_conserved()` now
  scans current type-group input/output amount data and checks exact u128 sum
  equality. `xudt::require_group_amount_minted(delta)` and
  `xudt::require_group_amount_burned(delta)` check exact token-side delta
  equality for mint/burn paths, and deltas may now be local `u128`
  add/sub/mul/div/function-return values. ProofPlan can tie exact declared
  group amount conservation and mint/burn delta aggregates to the matching
  helper in 0.17 strict mode; remaining xUDT compatibility work covers
  extension scripts, arbitrary args modes, and automatic lowering of computed
  mint/redeem terms.

**Acceptance**

- `wrong_xudt_binding` fails in generated code.
- Existing descriptive xUDT compatibility fixtures become executable.

### 7. `std::dao`

**Change**

Implement DAO helpers:

- DAO type hash recognition;
- deposit data recognition;
- withdrawal request data recognition;
- occupied/unoccupied capacity;
- accumulated-rate extraction;
- maturity/since checks for withdrawal phase 2.

Current partial surface:

- `dao::accumulated_rate(source::header_dep(i))`;
- `dao::require_header_dep_for_input(source::group_input(i), source::header_dep(j))`,
  which compares full 32-byte DAO fields via `LOAD_HEADER` absolute header offsets.
- `dao::is_deposit_data(source::group_input(i))` and
  `dao::is_withdrawal_request_data(source::group_input(i))`, which classify DAO
  deposit/withdrawal-request data with the same exact 8-byte zero/non-zero rule
  used by iCKB Rust.
- `dao::has_dao_type(source::group_input(i))`, which compares the SourceView
  TypeHash to iCKB's DAO hash constant.
- `dao::require_input_since_at_least(source::group_input(i), required_since)`,
  which loads the selected input's since field and fail-closed checks
  `since >= required_since`.
- `ckb::since_epoch_relative(number, index, length)` and
  `ckb::since_epoch_absolute(number, index, length)`, which encode RFC0017
  EpochNumberWithFraction since values with bounds checks.
- `dao::require_input_relative_epoch_since_at_least(input, number, index,
  length)`, which loads the selected input since, validates relative epoch
  flags, and compares epoch fractions.

**Acceptance**

- DAO data classification no longer uses placeholder fields in the CellScript
  spec.
- `redeem_before_maturity` now has generated raw and RFC0017 relative
  epoch-since bridges, but request/deposit/header lineage and original iCKB
  second-withdrawal fixture execution are still open.

### 8. Checked Integer Support

**Change**

Add:

- signed integer types needed for relative indexes: `i32` for iCKB
  Owned-Owner is implemented; `i64` remains deferred until a benchmark requires
  it;
- first-class checked `u256` or `C256` arithmetic for Limit Order conservation;
- stable overflow diagnostics.

**Implemented partial surface**

```cellscript
c256::require_product_lte(a, b, c, d)
c256::require_product_eq(a, b, c, d)
c256::require_sum2_products_lte(a, b, c, d, e, f, g, h)
c256::require_sum2_products_eq(a, b, c, d, e, f, g, h)
```

These helpers lower to executable RISC-V product limbs and checked u256
addition, but they do not yet provide a first-class `C256` value type or
general operators. Signed `i32` relative-distance values now have executable
ABI/sign-extension support, input OutPoint tx-hash/index reads now lower to
`LOAD_INPUT_BY_FIELD`, a combined OutPoint tx-hash+index requirement helper
exists, and `ckb::require_metapoint_relative(base, related, distance)` lowers
pairwise source-index MetaPoint binding for input/group-input and
output/group-output pairs. Fixed-distance and base-cell-data signed i32
current-script lock/type pair cardinality also lower to executable scans.
Filtered variants now require every related-role cell to match a caller-supplied
TypeHash and generic data rule (`0` no data check, `1` exact 8-byte zero u64,
`2` exact 8-byte nonzero u64), which covers the Owned-Owner DAO-withdrawal
related-cell shape without an iCKB-specific combined helper. The Limit Order
Match absolute master-OutPoint bridge now lowers to an executable current-script
lock-only input/output scan. Full protocol-specific maps with native
action/data validation still need first-class aggregate scan lowering.

**Acceptance**

- Limit Order core value-conservation checks use production-sized arithmetic.
- Owned-Owner signed relative distance matches the original iCKB `i32` byte
  encoding path and can bind a 32-byte owner lock args hash plus a full input
  OutPoint tx-hash/index pair, pairwise MetaPoint relative relation, and
  fixed-distance or owner-cell-data type-lock pair cardinality, including a
  protocol-neutral filtered related-cell TypeHash/data-rule check.
- Limit Order Match can require absolute master-OutPoint preservation across
  lock-only order inputs/outputs; native action-aware MetaPoint/OutPoint maps
  and original iCKB VM differential evidence remain open.

## P1: Executable Compatibility And Differential Harness

### 9. CKB Fixture Runner

**Problem**

0.16 compatibility fixtures are descriptive. iCKB requires accepted/rejected
transaction execution evidence.

**Change**

Add a CKB test runner that can:

- load generated CellScript ELF/assembly artifacts;
- construct CKB transactions from fixtures;
- attach CellDeps/HeaderDeps/WitnessArgs;
- run CKB VM verification;
- assert expected error code and failure class;
- report cycles, tx size, occupied capacity, and under-capacity checks.

**Acceptance**

- At least the iCKB benchmark positive and negative fixtures execute against
  generated CellScript artifacts.
- Fixture failures are tied to named invariants, not accidental VM failure.

### 10. iCKB Differential Harness

**Change**

For a selected subset, run the same logical transaction shape against:

1. original iCKB Rust script binary;
2. generated CellScript script binary.

The first target subset:

- valid deposit phase 1;
- valid mint from receipt;
- duplicate receipt;
- amount inflation;
- wrong owner;
- wrong xUDT args;
- immature redeem;
- valid limit order;
- limit order underpayment.

**Acceptance**

- `docs/0.17/ickb_diff_results.md` contains executed results, not only
  `MODEL_LEVEL_ONLY` rows.
- Any non-equivalence is recorded as either a CellScript bug, unsupported
  semantic, or intentional scope difference.

## P2: Tooling, Diagnostics, And Production Gates

### 11. Error Code Contract

Each lowered invariant family must have a stable diagnostic/runtime code:

- missing HeaderDep;
- wrong accumulated rate;
- xUDT binding mismatch;
- script role misuse;
- amount mismatch;
- receipt mismatch;
- capacity violation;
- maturity violation;
- witness malformation;
- cell dep substitution;
- arithmetic overflow.

### 12. Production Evidence Gate

Extend the production gate so iCKB-grade claims require:

- original iCKB repository commit and script binary SHA-256;
- CellScript source commit and generated artifact SHA-256;
- metadata and ProofPlan soundness report;
- CKB VM/testtool version;
- transaction fixture manifest SHA-256;
- proof that inputs, outputs, output data, cell deps, header deps, and witnesses
  are identical across original and generated executions;
- CKB VM positive and negative tests;
- original and generated exit codes;
- named failure mode for every reject case;
- cycle report;
- tx size;
- occupied capacity report;
- under-capacity check;
- differential result status for every selected row.
- row-level execution objects with fixture/context hashes, both artifact hashes,
  pass/fail status match, exit-code/status consistency, cycles, transaction
  size, occupied capacity, and fee.

`docs/0.17/ickb_production_equivalence_gate.md` defines the current evidence
schema. `tests/ickb_diff.rs` must reject any `PROVEN` claim that lacks these
fields.

## 0.17 Deliverables

1. CKB source primitives for HeaderDep, Script role, input OutPoint/index,
   capacity, CellDep, lock/type args, and outputs-data.
2. Executable aggregate invariant lowering for computed equality and grouping,
   including automatic receipt/deposit/DAO-rate aggregate equations.
3. `std::dao` and `std::xudt` implementations with compatibility tests.
   The iCKB owner-mode xUDT args pattern has executable explicit-hash and
   current-script-hash helpers; arbitrary xUDT args construction remains open.
4. Signed `i32` and checked 256-bit arithmetic support. `i32` and C256
   requirement helpers exist; first-class `u256`/`C256` values remain open.
5. Executable CKB fixture runner.
6. Partial iCKB differential harness with honest pass/fail/unsupported labels.
7. Updated iCKB benchmark specs with fewer unresolved limitation manifest
   entries and more runtime-backed tests.
8. Updated final report stating whether CellScript moved from incomplete to
   partially iCKB-grade, or remains blocked.
9. Production-equivalence gate manifest that prevents `MODEL_LEVEL_ONLY` rows
   from being reported as behavioural equivalence.

## Milestones

### M1: Source Primitives

- HeaderDep read API.
- Script role/current script API.
- Input OutPoint/index/capacity fields.
- Unit and integration tests for each primitive.

Exit criteria: iCKB missing-header, script-role, and capacity negatives can fail
through generated code.

### M2: Aggregate Lowering

- Computed aggregate equality.
- Group-by/count/sum primitives.
- Overflow fail-closed lowering.
- ProofPlan soundness updated to reject stale metadata-only claims.

Exit criteria: amount inflation, amount deflation, and forged receipt negatives
fail through generated code.

### M3: Protocol Stdlib

- `std::xudt`.
- `std::dao`.
- first-class checked `u256`.

Exit criteria: wrong xUDT binding, wrong accumulated rate, immature redeem, and
limit order underpayment run through stdlib-backed checks.

### M4: CKB Execution Evidence

- CKB fixture runner.
- iCKB generated artifact fixtures.
- cycle/size/capacity reports.

Exit criteria: positive and negative benchmark fixtures execute in CKB VM.

### M5: Differential Evidence

- Build or load original iCKB binaries.
- Run selected matrix against original and generated artifacts.
- Update differential report from model-level to executed partial.

Exit criteria: no equivalence claim is made without executed evidence.
The diff matrix may only move from `MODEL_LEVEL_ONLY` to partial executed modes
when every executed row carries the evidence required by
`docs/0.17/ickb_production_equivalence_gate.md`; it may only move to production
mode `EXECUTED_CKB_VM_DIFF` / `PROVEN` when every selected row satisfies that
gate.

## Validation Gate

Focused:

```bash
cargo test --locked -p cellscript --test ickb_benchmark
cargo test --locked -p cellscript ckb_source --lib
cargo test --locked -p cellscript aggregate_invariant --lib
cargo test --locked -p cellscript --test ckb_compat_runner
cargo test --locked -p cellscript --test ickb_diff
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
cargo run --locked -p cellscript --bin cellc -- tests/benchmarks/ickb_specs/ickb_logic.cell --target riscv64-elf --target-profile ckb --entry-action mint_from_receipt -o /tmp/cellscript_ickb_logic_mint_from_receipt.elf
```

Full:

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
```

Production evidence:

```bash
bash scripts/cellscript_ckb_release_gate.sh production
cargo test --locked -p cellscript --test ickb_diff
cargo run --locked -p cellscript --bin cellc -- verify-ckb-fixtures tests/compat/ckb_standard/manifest.json --json
```

## Release Criteria

0.17 is complete only when:

1. iCKB benchmark specs compile without unresolved limitation manifest entries
   for HeaderDep, xUDT binding, script role, aggregate accounting, DAO maturity,
   or Limit Order arithmetic.
2. At least one iCKB Logic positive case executes in CKB VM.
3. At least five iCKB adversarial cases fail in CKB VM for named invariant
   reasons.
4. The differential matrix has executed results for the selected subset, or
   every remaining row is explicitly labelled unsupported with a tracked blocker.
5. `docs/0.17/ickb_final_report.md` is updated with the new evidence.
6. No unsupported feature is represented as supported.
7. `tests/ickb_diff.rs` accepts the matrix as executed evidence rather than
   model-only evidence; otherwise 0.17 remains partial/not-proven.
