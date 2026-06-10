# CellScript 0.17 Roadmap

**Status**: Proposed
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

0.17 should make the following statement true:

> CellScript can express, compile, execute-test, and audit a non-trivial
> iCKB-style CKB protocol subset without hiding critical invariants in comments,
> builder-only assumptions, or raw script escape hatches.

It should still not claim full iCKB equivalence until the differential matrix
executes both the original iCKB Rust scripts and generated CellScript scripts on
the same CKB transaction fixtures.

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
  failures, not model-only failures.
- Metadata records exact HeaderDep reads and source binding.
- `cellc check --primitive-strict=0.17` rejects any iCKB-style accumulated-rate
  claim that is still witness/builder supplied.

### 2. Script Role And Script Identity Primitives

**Problem**

iCKB, Owned-Owner, and Limit Order use the same deployed script as lock in one
cell and type in another. Current CellScript cannot express current script role,
current script hash, or lock/type relation checks directly.

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

- Owned-Owner relative index checks can be expressed without model-only fields.
- Limit Order master MetaPoint binding can be expressed.
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
- type args constructor from script hash + flags;
- type hash validation;
- transfer/conservation helpers.

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

**Acceptance**

- `redeem_before_maturity` fails through generated DAO checks.
- Deposit and withdrawal classification no longer uses placeholder fields.

### 8. Checked Integer Support

**Change**

Add:

- signed integer types needed for relative indexes: at least `i32`, likely
  `i64`;
- checked `u256` or `C256` arithmetic for Limit Order conservation;
- stable overflow diagnostics.

**Acceptance**

- Limit Order value checks use production-sized arithmetic.
- Owned-Owner signed relative distance matches original iCKB encoding.

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

- generated artifact;
- metadata;
- CKB VM positive and negative tests;
- cycle report;
- tx size;
- occupied capacity report;
- under-capacity check;
- differential result status where applicable.

## 0.17 Deliverables

1. CKB source primitives for HeaderDep, Script role, OutPoint/index, capacity,
   CellDep, lock/type args, and outputs-data.
2. Executable aggregate invariant lowering for computed equality and grouping.
3. `std::dao` and `std::xudt` implementations with compatibility tests.
4. Signed integer and checked 256-bit arithmetic support.
5. Executable CKB fixture runner.
6. Partial iCKB differential harness with honest pass/fail/unsupported labels.
7. Updated iCKB benchmark specs with fewer TODO markers and more runtime-backed
   tests.
8. Updated final report stating whether CellScript moved from incomplete to
   partially iCKB-grade, or remains blocked.

## Milestones

### M1: Source Primitives

- HeaderDep read API.
- Script role/current script API.
- OutPoint/index/capacity fields.
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
- signed ints and checked `u256`.

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

## Validation Gate

Focused:

```bash
cargo test --locked -p cellscript --test ickb_benchmark
cargo test --locked -p cellscript ckb_source --lib
cargo test --locked -p cellscript aggregate_invariant --lib
cargo test --locked -p cellscript --test ckb_compat_runner
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
```

## Release Criteria

0.17 is complete only when:

1. iCKB benchmark specs compile without critical TODO markers for HeaderDep,
   xUDT binding, script role, aggregate accounting, DAO maturity, or Limit
   Order arithmetic.
2. At least one iCKB Logic positive case executes in CKB VM.
3. At least five iCKB adversarial cases fail in CKB VM for named invariant
   reasons.
4. The differential matrix has executed results for the selected subset, or
   every remaining row is explicitly labelled unsupported with a tracked blocker.
5. `docs/0.17/ickb_final_report.md` is updated with the new evidence.
6. No unsupported feature is represented as supported.
