# CellScript v0.16 Roadmap

**Status**: Implemented on `nightly-0.16` for the scoped 0.16 release;
compiler-freeze hardening and proposal-local NovaSeal devnet acceptance are
complete for the P0 plus key P1 freeze scope.
**Scope**: Metadata Semantics, Descriptive Standard Compatibility, Production
Tooling Skeleton, and Rust-comparative compiler hardening.
**Dependencies**: v0.13, v0.14, and v0.15 complete

---

## Implementation Status

The `nightly-0.16` branch implements the scoped metadata-assurance release.
It does not claim full production transaction solving, standard-suite CKB VM
compatibility execution, stable protocol stdlib implementations, or formal
verification.

After the Rust comparative audit (`RUST_COMPARATIVE_AUDIT.md`), 0.16 also owns
the freeze-critical compiler hardening needed to make the release credible as a
compiler release rather than only a metadata/tooling release. CKB production
completeness and the non-critical comparative-audit cleanup remain 0.17 work.

| Area | 0.16 Status | Artifact | Remaining / next scope |
|---|---|---|---|
| Operational semantics | Implemented as mechanically precise prose plus conformance tests | `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md` and `tests/v0_16.rs` | Machine-checked/formal proof backend |
| ProofPlan soundness checks | Implemented as fail-closed metadata consistency checker | `proof_plan::soundness`, `runtime.proof_plan_soundness`, and `--primitive-strict=0.16` enforcement | Formal invariant proof and source-to-assembly coverage |
| Standard CKB compatibility suite | Implemented as descriptive fixture suite | `tests/compat/ckb_standard/manifest.json` and descriptive fixture files | Executable CKB VM accepted/rejected runner |
| Builder assumption contract | Implemented | `runtime.builder_assumptions`, `cellc explain-assumptions`, schema-bound `validate-tx` evidence | Full CKB transaction semantic validation |
| Transaction validation | Implemented as pre-signing structural/schema-bound evidence check | `cellc validate-tx --against metadata.json tx.json` | CKB dry-run-backed fixture validation |
| Transaction solving | Implemented as deterministic template emitter | `cellc solve-tx` emits requirements, not a final transaction | Real solver with cell selection, deps, fees, witnesses, dry-run |
| Deployment governance | Implemented as local manifest/schema/integrity tooling | `cellc deploy-plan`, `verify-deploy`, `diff-deploy`, and `lock-deps` | On-chain deployment verification |
| Audit/debug UX | Implemented as metadata/IR reports | `cellc proof-diff`, `profile`, `trace-tx`, and `audit-bundle` | Full CellScript-to-RISC-V source maps |
| Stdlib release track | Implemented as schema stubs only | `src/stdlib/ckb_protocols/*` descriptors marked `schema-stub` | ABI-compatible protocol stdlib implementations and executable coverage |
| Compiler-freeze hardening | Implemented for P0 plus key P1 freeze scope | `RUST_COMPARATIVE_AUDIT.md`, IR poison lowering, register contract gate, syscall ABI contract baseline, IR provenance, error-line tests | Remaining comparative-audit cleanup is 0.17 scope |
| NovaSeal proposal-local acceptance | Implemented as live devnet plus certification evidence | `scripts/novaseal_devnet_stateful_acceptance.sh`, `cellc certify --plugin novaseal-profile-v0`, live profile reports, Fiber report, adapter and handoff reports | External production attestations: BIP340 TCB, public BTC SPV, public/shared CellDep, RWA legal/registry |

Boundary: v0.16 does not claim full formal verification or production CKB
protocol equivalence. The branch implements metadata consistency checking,
schema-bound builder evidence, descriptive fixtures, local deployment integrity
checks, transaction template generation, compiler-freeze hardening, and
proposal-local NovaSeal devnet/profile certification. Standard compatibility
CKB VM execution, real transaction solving, and protocol stdlib implementation
are 0.17 scope; non-critical Rust-comparative compiler cleanup is also 0.17
scope. NovaSeal production readiness remains blocked on external attestations.

## Goal

v0.16 turns the v0.15 semantic audit layer into a scoped metadata-assurance
tooling release.

v0.15 makes CKB invariants visible:

- trigger
- scope
- reads
- coverage
- on-chain checked obligations
- builder assumptions

v0.16 answers the next questions within compiler metadata and local tooling:

- Can the compiler reject ProofPlan records that overstate their own metadata?
- Can CellScript describe standard CKB contract ABI/layout expectations as
  deterministic fixtures?
- Can wallets/builders/indexers receive stable, schema-bound builder assumptions?
- Can developers inspect deploy plans, proof diffs, profiles, tx traces, and
  audit bundles before moving to CKB VM evidence?
- Can a proposal package such as NovaSeal carry live devnet, profile, adapter,
  handoff, and certification evidence without overclaiming production?

---

## Out of Scope

Do not re-plan v0.13:

- bounded value generics
- zero-cost abstraction passes
- CLI baseline ergonomics

Do not re-plan v0.14:

- Spawn/IPC DSL
- WitnessArgs and Source views
- ScriptGroup / outputs_data / TYPE_ID metadata MVP
- capacity/time/since syntax
- script reference and HashType strictness

Do not re-plan v0.15:

- scoped invariants
- Covenant ProofPlan
- trigger/scope/reads/coverage modeling
- protocol macro lowering
- identity lifecycle primitives
- explicit destroy policies

The v0.15 hardening backlog is still tracked here, but the 0.16 assurance
branch treats it as a scoped metadata/tooling release rather than a full
production-equivalence release. Concretely:

| Deferred v0.15 hardening track | 0.16 assurance status | Full production target |
|---|---|---|
| Executable verifier lowering for aggregate invariants | ProofPlan soundness can fail closed on metadata-only aggregate obligations | 0.17 executable aggregate lowering |
| Full ProofPlan soundness checker | Metadata consistency checker implemented for strict mode | formal source-to-code proof coverage |
| Full macro-only lowering with no protocol-name recognizers | macro provenance and descriptor boundaries documented | removal of all stable protocol recognizers from core/codegen |
| Covenant helper stdlib | protocol descriptors exist as `schema-stub` only | stable audited protocol stdlib modules |
| `Address` / `LockScript` / `LockHash` split | builder/schema evidence records distinguish lock-facing fields | strict type-system split |
| Explicit `#[entry(lock)]` / `#[entry(type)]` roles | entry kind is exposed through metadata and validation reports | mandatory role annotations for ambiguous entries |
| Versioned data-layout preserve/migrate policies | deployment diff and layout evidence are surfaced | strict replacement/migration policy syntax |
| Full `cellc explain-macro` source maps | macro provenance appears in ProofPlan/audit bundle records | source-to-expansion-to-code mapping |
| Non-TYPE-ID global uniqueness proof | local anchors and builder/indexer assumptions are explicit | global uniqueness certification where possible |
| Standard CKB compatibility fixtures | descriptive accepted/rejected fixture suite | executable CKB VM fixture runner |
| NovaSeal BTC-facing profile evidence | live CKB devnet stateful reports plus public BTC SPV adapter/handoff contract | externally attested public BTC SPV reports |

---

## P0: 0.16 Release Scope

### 1. Formal Operational Semantics

**Problem**

CellScript has a rich invariant model after v0.15, but the language needs a
precise compiler-facing semantics for resource states, cell effects, script
triggers, scopes, and ProofPlan obligations.

**Change**

Publish mechanically precise semantics for:

- expression evaluation
- linear resource state transitions
- branch merge rules
- cell input/output/ref effects
- lock/type trigger execution
- group and transaction scopes
- ProofPlan obligation coverage
- builder assumption boundaries

**Artifacts**

- `docs/spec/CELLSCRIPT_OPERATIONAL_SEMANTICS.md`
- small-step/big-step style rules in prose notation
- conformance fixtures linked to compiler tests

**Acceptance**

- every v0.15 ProofPlan field has a documented meaning
- resource state rules match type checker behavior
- trigger/scope/coverage examples have expected conformance outcomes
- compiler tests include spec conformance fixtures

---

### 2. ProofPlan Soundness Checks

**Problem**

`ProofPlan` is auditable metadata, not a proof. v0.16 must verify that
ProofPlan obligations do not overstate their own metadata coverage or diverge
between local action/function/lock metadata and `runtime.proof_plan`.

**Change**

Add soundness checks:

```text
source/runtime obligation
  -> ProofPlan obligation
  -> metadata coverage record
  -> builder assumption boundary
```

Add an internal checker that rejects:

- metadata-only obligations in strict mode
- checked records whose codegen coverage metadata is not `covered`
- mismatched local/runtime trigger, scope, reads, coverage, assumptions, detail,
  or codegen coverage records
- incorrect group cardinality coverage
- unchecked builder assumptions marked as on-chain
- stale ProofPlan records after optimization
- verifier obligations without a matching category, feature, status, and detail

**Code Areas**

- `src/proof_plan/`
- IR validation
- codegen coverage emitter
- metadata validation
- optimization passes

**Acceptance**

- strict mode fails for metadata-only/runtime-required ProofPlan gaps
- local/runtime ProofPlan mutations are rejected
- checked records cannot carry coverage gaps or unchecked builder assumptions

---

### 3. Descriptive Standard CKB Contract Compatibility Suite

**Problem**

CellScript needs a deterministic description of standard CKB script layouts
and expected accepted/rejected transaction shapes before 0.17 adds executable
CKB VM compatibility.

**Change**

Create compatibility suites for:

- sUDT
- xUDT
- ACP
- Cheque
- Omnilock-compatible lock patterns
- NervosDAO-style epoch/since fixtures
- Type ID

Each descriptive suite must cover:

- script args layout
- witness layout
- Molecule data layout
- ScriptGroup and `outputs` / `outputs_data` positive and negative transaction
  shape matrices
- error behavior
- accepted/rejected transaction fixtures
- cycle envelope
- script reference metadata

**Artifacts**

- `tests/compat/ckb_standard/`
- fixture transactions
- metadata expectation snapshots
- cycle and capacity report envelopes

**Acceptance**

- manifest names accepted and rejected fixtures for each suite
- fixture files parse and expose transaction shape, expected behavior,
  script args, witness, Molecule data, ScriptGroup, `outputs_data`, metadata,
  cycle, and capacity fields
- no behavioural equivalence is claimed until 0.17 executable runner coverage

---

### 4. Builder Assumption Contract

**Problem**

v0.15 marks builder assumptions, but wallets, SDKs, relayers, and transaction builders need a stable contract to honor them.

**Change**

Define a builder assumption schema:

```text
assumption_id
kind
origin
feature
proof_plan_status
required_inputs
required_outputs
required_cell_deps
required_witness_fields
capacity_policy
fee_policy
change_policy
signature_policy
failure_mode
```

Add validation APIs:

- `cellc explain-assumptions`
- `cellc validate-tx --against metadata.json tx.json`

For manifest-bound spawn targets, `required_cell_deps` carries the required
CellDep slot and manifest identity. `validate-tx` checks both the transaction
`cell_deps[index]` object and the matching `builder_assumption_evidence`.

**Acceptance**

- every builder assumption has a stable schema record
- generated transaction templates include evidence requirements
- validation rejects missing, bare, or malformed schema-bound evidence before
  signing

---

## P0: Compiler Freeze Hardening From Rust Comparative Audit

This track incorporates the Rust comparative audit into 0.16. It is split into
`0.16 freeze blockers` and `key P1 hardening`. Everything outside those two
sets moves to the 0.17 roadmap.

The principle is:

> 0.16 blocks only on correctness, backend contracts, CKB ABI evidence,
> instruction-level provenance, and line-aware diagnostic tests. Broader
> maintainability refactors do not block this freeze.

### 4A. 0.16.0 Freeze Blockers

| Priority | Item | 0.16 disposition | Acceptance |
|---|---|---|---|
| P0 | IR poison/error lowering | Error recovery must not produce a normal-looking IR value. Lowering errors return explicit poison, not live `IrConst::U64(0)` sentinels. | Invalid source cannot feed semantic `Binary`, `Call`, `Cast`, `Index`, `FieldAccess`, `If`, `Match`, array, or Vec lowering as if it had a valid value; verifier and codegen reject poisoned IR. A deeper `Lowered<T>` representation is tracked for 0.17. |
| P0 | Register contract constants and gate | `s10`, `s11`, and `t6` conventions are named compiler contracts. | Entry-wrapper writes to reserved registers are whitelisted; non-entry clobbers fail codegen; far branch relaxation reports the named scratch register. |
| P0 | Syscall ABI baseline | CKB syscall numbers, status registers, return registers, size-check policy, and fail behaviour are checked against a versioned baseline. | `tests/syscall_abi_baseline.json` is compared to `syscalls.rs` and runtime ABI fields in the fast test gate. |

### 4B. Key P1 Hardening Kept In 0.16

| Priority | Item | 0.16 disposition | Acceptance |
|---|---|---|---|
| P1 | IR provenance wrapper | Add `SpannedIrInstruction { kind, span }` and `SpannedIrTerminator { kind, span }` views without changing semantic helper matches. | Lowered user-originated IR records instruction/terminator provenance; verifier gates provenance shape; synthetic test IR can remain unspanned. |
| P1 | Error-line regression directives | Extend test directives from substring-only `expect-error:TEXT` to `expect-error-line:N:TEXT`. | Diagnostics assert line and text together; span regressions fail tests. |

### 4C. Comparative-Audit Work Moved To 0.17

The remaining audit recommendations are valuable, but they are no longer 0.16
freeze items. They are tracked in `docs/0.17/CELLSCRIPT_0_17_ROADMAP.md`:

- deeper `Lowered<T>` / `LoweredOperand::{Value, Poisoned}` representation;
- tuple formatter and `Span::Display` hygiene;
- per-function backend validation beyond the 0.16 register/syscall gates;
- exhaustive IR helper semantic tests;
- phase boundary markers;
- warning/dedup diagnostic model hardening;
- behaviour-preserving splits of `lib.rs`, `types/mod.rs`, and CLI commands;
- structured generic/type resolver cleanup;
- release tidy gate.

### 4D. Rustc Discipline To Adopt Without Rustc Bureaucracy

Adopt these compiler disciplines, with only the freeze-critical subset blocking
0.16:

- every IR artifact has provenance;
- user-facing errors flow through diagnostics, not panic paths;
- error recovery can continue collecting diagnostics but must not create trusted
  semantic artifacts;
- backend conventions are named, tested contracts;
- snapshot and error-line tests are stronger than substring-only tests;
- phase boundaries are explicit and locally verifiable;
- release gates validate local invariants, not only final ELF assembly.

Do not copy these rustc-scale systems for 0.16:

- query system / `TyCtxt`;
- macro hygiene / `SyntaxContext`;
- full MIR dataflow framework;
- global `SourceMap` and interning architecture;
- LLVM/Cranelift abstraction;
- multi-crate compiler cathedral.

CellScript remains a CKB-first DSL. The target is rustc-level discipline, not
rustc-level bureaucracy.

---

## P1: 0.16 Tooling Skeleton

### 5. Transaction Template Emitter

**Problem**

0.16 needs a deterministic handoff artifact for builders. A real transaction
solver with cell selection, dep resolution, fee/change planning, witness
placement, and dry-run validation is 0.17 scope.

**Change**

Add a transaction template emitter that consumes:

- action metadata
- ProofPlan
- builder assumptions
- target profile

Template responsibilities:

- list required input/output/dep/header/witness slots from metadata
- surface schema-bound builder assumption evidence requirements
- emit fee/change metadata that a builder must satisfy
- emit a deterministic signing manifest skeleton
- state limitations explicitly in JSON output

**Acceptance**

- `cellc solve-tx` emits a deterministic template, not a final transaction
- output uses `status: "template-only"` and names concrete limitations
- evidence output is requirements-only and cannot be mistaken for satisfied
  builder evidence

---

### 6. Local Deployment and Upgrade Governance

**Problem**

CKB deployment is a governance problem: code cells, dep groups, hash types,
Type ID, audit labels, and version locks need a stable local manifest workflow.
On-chain deployment verification is 0.17 scope.

**Change**

Add deployment governance artifacts:

- code cell manifest
- dep group manifest
- version lock file
- audit hash record
- local upgrade diff
- script reference metadata

Add commands:

```bash
cellc deploy-plan
cellc verify-deploy
cellc diff-deploy
cellc lock-deps
```

**Acceptance**

- deploy plans include artifact hash/size, target profile, dep group metadata,
  script references, ProofPlan soundness, and builder assumptions
- `verify-deploy` rejects malformed local plan schema, noncanonical artifact
  hashes, zero artifact sizes, failed ProofPlan soundness, and missing builder
  assumption sections
- upgrade diffs identify artifact, target profile, ProofPlan soundness, and
  metadata schema changes

---

### 7. Audit and Debug UX

**Problem**

`explain-proof` is necessary but not enough for production audits. 0.16 adds
metadata-level traceability before 0.17 adds full source-to-RISC-V and CKB
execution views.

**Change**

Add audit tooling:

- metadata/IR-level source-to-codegen mapping
- proof diff between versions
- cycle profiler per invariant/check
- tx assumption trace viewer
- coverage report for invariants and assumptions
- HTML audit bundle

**Commands**

```bash
cellc explain-proof
cellc proof-diff old.json new.json
cellc profile --entry transfer
cellc trace-tx tx.json
cellc audit-bundle
```

**Acceptance**

- audit bundle links source spans, ProofPlan obligations, metadata, IR effect
  classes, and codegen coverage status
- proof diff highlights changed trigger/scope/coverage semantics
- cycle profiler emits deterministic metadata-level estimates

---

### 8. Standard Library Schema Track

**Problem**

v0.16 should not make the standard library the main language milestone. It
ships descriptor stubs for standard CKB protocols so the compatibility suite
and future 0.17 implementations have stable names and metadata shapes.

**Change**

Ship schema descriptors for:

- `std::sudt`
- `std::xudt`
- `std::type_id`
- `std::htlc`
- `std::cheque`
- `std::acp`

Rules:

- modules must be marked `schema-stub`, not `stable`
- descriptors must expose ProofPlan trigger/scope/reads and builder
  assumptions
- compatibility fixture references must be explicit

**Acceptance**

- descriptor modules exist for the listed protocols
- tests reject marking the descriptors stable before implementation coverage
- no descriptor is represented as production-ready

---

## P2: 0.16 Long-Horizon Research Tracks

### 9. Advanced Linear Collections

**Problem**

v0.13 intentionally avoids cell-backed generic collections, and v0.15 does not solve them. Some protocols need collections of linear or cell-backed resources.

**Change**

Design, but do not rush, bounded forms:

```text
Vec<CellRef<T>>
Map<Key, CellRef<T>>
IndexedSet<T>
```

Constraints:

- no hidden ownership transfer
- no implicit consume inside collection operations
- explicit iteration bounds
- ProofPlan records collection coverage

**Acceptance**

- design doc published
- unsafe collection forms remain fail-closed
- prototype examples show explicit ownership and coverage

---

### 10. Formal Verification Backend Exploration

**Problem**

Operational semantics and soundness checks are not full formal verification.

**Change**

Explore one or more backends:

- SMT encoding for bounded invariants
- K-framework semantics
- Lean/Coq model for core resource calculus
- model checker for transaction-shape fixtures

**Acceptance**

- one prototype proves a non-trivial invariant
- limitations are documented
- no production guarantee is claimed without proof coverage

---

### 11. 0.17 Production-Completeness Tracks

The following are CKB production-completeness tracks, not compiler-freeze
hardening. They remain in `docs/0.17/CELLSCRIPT_0_17_ROADMAP.md` and should not
be confused with the freeze-critical Rust-comparative hardening items kept in
0.16:

- executable CKB VM compatibility fixtures;
- iCKB-style differential testing;
- full transaction solver with live cell selection and dry-run;
- ABI-compatible CKB protocol stdlib implementations;
- source-to-RISC-V/assembly source maps;
- on-chain deployment verification;
- executable aggregate invariant lowering;
- full macro-only protocol lowering;
- strict `Address` / `LockScript` / `LockHash` type separation;
- explicit entry-role syntax gates;
- versioned data-layout preserve/migrate policies;
- non-TYPE-ID global uniqueness certification.

---

## Release Gates

v0.16 can ship when the scoped metadata/tooling release satisfies:

- operational semantics document covers resource state, cell effects, triggers, scopes, and ProofPlan
- ProofPlan soundness checker is mandatory in strict mode and rejects
  local/runtime ProofPlan drift
- standard CKB compatibility suites provide descriptive accepted/rejected
  fixture shapes, including ScriptGroup and `outputs_data` matrices
- builder assumption schema is stable and `validate-tx` rejects missing,
  bare, or malformed schema-bound evidence
- `solve-tx` emits a deterministic template with explicit limitations and
  evidence requirements
- deployment manifests are reproducible and `verify-deploy` rejects malformed
  local integrity fields
- audit bundle links source spans, ProofPlan, metadata, IR effect classes, and
  codegen coverage status
- CKB protocol stdlib descriptors are explicitly marked `schema-stub`, not
  `stable`
- IR lowering has explicit poison/error semantics; verifier and codegen reject
  poisoned artifacts rather than treating error recovery as a valid value
- backend register contracts for entry-wrapper registers and branch-relaxation
  scratch registers are named and gate-tested
- CKB syscall and VM2 spawn/IPC ABI contract fields match a checked 0.16
  baseline
- key P1 audit hardening is present: instruction/terminator provenance wrappers
  and `expect-error-line:N:TEXT` diagnostics tests
- NovaSeal proposal-local acceptance reports `status=passed`,
  `live_devnet_rpc_executed=true`, and `blockers=0` while preserving external
  production blockers in `cellc certify --plugin novaseal-profile-v0`
- public BTC SPV evidence for NovaSeal BTC-facing profiles is bound to live CKB
  report hashes, service-builder hashes, CKB-side BTC commitment hashes, raw
  BTC transaction material, block-header/Merkle proof checks, confirmation
  heights, and canonical SPV material hashes
- all non-critical Rust-comparative audit cleanup is tracked in the 0.17 roadmap
  rather than blocking 0.16 freeze
- `cargo fmt --all`, `cargo check --locked -p cellscript --all-targets`,
  `cargo test --locked -p cellscript`, `cargo clippy --locked -p cellscript
  --all-targets -- -D warnings`, and `git diff --check` pass
