# CellScript v0.16 Roadmap

**Status**: Draft
**Scope**: Formal Semantics, Standard Compatibility, and Production Tooling
**Dependencies**: v0.13, v0.14, and v0.15 complete

---

## Goal

v0.16 turns the v0.15 semantic audit layer into production-grade assurance.

v0.15 makes CKB invariants visible:

- trigger
- scope
- reads
- coverage
- matched or unmatched action coverage for aggregate invariants
- on-chain checked obligations
- builder assumptions

v0.16 answers the next questions:

- Can we prove the invariant model is sound?
- Can we prove that declared invariants are satisfied by emitted aggregate
  checks or by checked action obligations?
- Can CellScript match standard CKB contract behavior, ABI/layout, error behavior, and cycle envelopes where needed?
- Can wallets/builders/indexers honor the assumptions emitted by the compiler?
- Can developers debug, audit, deploy, and upgrade contracts without relying on ad hoc tooling?

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
- identity policy primitives
- explicit destroy policies

v0.16 does promote the following deferred v0.15 hardening tracks. These are not
new syntax promises; they turn the 0.15 audit-visible semantics into executable,
soundness-checked, compatibility-tested production behavior:

- executable verifier lowering for aggregate invariants
- full ProofPlan soundness checking
- formal invariant satisfaction checking beyond the bounded v0.15
  action-coverage cross-reference
- macro-only protocol lowering, with no protocol-name recognizers in core/codegen
- covenant helper stdlib
- `Address` / `LockScript` / `LockHash` type separation
- explicit `#[entry(lock)]` / `#[entry(type)]` role declarations
- versioned data-layout preserve/migrate policies
- full `cellc explain-macro` source maps
- non-TYPE-ID global uniqueness proof boundaries
- standard CKB compatibility suites with accepted/rejected transaction fixtures

---

## P0

### 1. Formal Operational Semantics

**Problem**

CellScript will have a rich invariant model after v0.15, but the language still lacks a formal semantics for resource states, cell effects, script triggers, scopes, and ProofPlan obligations.

**Change**

Publish a machine-checkable or mechanically precise semantics for:

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
- formal small-step or big-step rules
- executable reference checker for selected rules
- conformance fixtures linked to compiler tests

**Acceptance**

- every v0.15 ProofPlan field has a formal meaning
- resource state rules match type checker behavior
- trigger/scope/coverage examples have expected formal outcomes
- compiler tests include spec conformance fixtures

---

### 2. ProofPlan Soundness Checks

**Problem**

`ProofPlan` is auditable metadata, not a proof. v0.16 must verify that ProofPlan obligations match emitted code and cannot overstate enforcement.

This is a soundness check, not a completeness check: it proves that metadata does
not lie about generated checks. The separate invariant satisfaction gate below
proves, or fails closed when it cannot prove, that declared invariants are
covered by executable aggregate lowering or by checked action obligations.

**Change**

Add soundness checks:

```text
source invariant
  -> ProofPlan obligation
  -> IR operation
  -> codegen check
  -> metadata coverage record
```

Add an internal checker that rejects:

- metadata-only obligations in strict mode
- missing emitted checks
- mismatched source views
- incorrect group cardinality coverage
- unchecked builder assumptions marked as on-chain
- stale ProofPlan records after optimization

**Code Areas**

- `src/proof_plan/`
- IR validation
- codegen coverage emitter
- metadata validation
- optimization passes

**Acceptance**

- strict mode fails if ProofPlan and emitted code diverge
- optimization cannot remove checks without updating ProofPlan
- negative tests mutate metadata/codegen coverage and are rejected

---

### 3. Invariant Satisfaction Gate

**Problem**

v0.15 now cross-references declared aggregate invariants with checked action
obligations, but that is a bounded coverage heuristic. It catches useful gaps,
yet it is not a full proof that every action preserves every declared invariant.

**Change**

Add a strict invariant satisfaction gate:

```text
declared invariant
  -> aggregate obligation
  -> executable aggregate lowering OR matched checked action obligation
  -> satisfaction status
```

The checker must reject or explicitly mark:

- declared invariants with no executable aggregate check and no matching checked action obligation
- action effects that can obviously violate a declared invariant
- aggregate obligations whose `reads` or `scope` cannot be related to an action obligation
- `assert_delta` obligations whose delta source is not bound to witness or lock-args data
- wildcard matches that ignore field, type, trigger, or scope mismatches

**Acceptance**

- strict mode fails when a declared invariant has no satisfaction evidence
- satisfaction status appears in `cellc explain-proof`, JSON ProofPlan output,
  docgen audit output, and audit bundles
- negative fixtures cover missing action obligations, mismatched fields,
  mismatched scopes, and unbound delta sources
- bounded action-coverage matches from v0.15 remain visible, but cannot be
  reported as formal satisfaction without this gate

---

### 4. Executable Aggregate Invariant Lowering

**Problem**

v0.15 aggregate primitives are intentionally metadata-only. They are useful for
audits, but they do not yet emit verifier loops for output/input sums,
distinctness, singleton checks, or scoped deltas.

**Change**

Lower aggregate ProofPlan obligations into bounded CKB verifier code for:

- `assert_sum`
- `assert_conserved`
- `assert_delta`
- `assert_distinct`
- `assert_singleton`

Rules:

- every aggregate lowering is tied to an explicit trigger and scope
- loop bounds are profile-visible and fail closed
- fixed-width ABI assumptions are checked before arithmetic or comparison
- overflow and malformed `outputs_data` fail with stable runtime errors
- metadata-only status is removed only when emitted code covers the obligation

**Acceptance**

- strict mode can require executable aggregate coverage
- accepted/rejected fixtures prove conserved, delta, distinct, and singleton cases
- tampered metadata claiming aggregate coverage without emitted code is rejected
- cycle reports include aggregate-loop costs

---

### 5. Macro-Only Protocol Lowering

**Problem**

v0.15 records protocol macro provenance but still allows selected lifecycle and
protocol flows to pass through compiler-recognized names. For production
soundness, protocol behavior should lower through explicit stdlib macros,
ProofPlan obligations, and kernel effects instead of name-based recognizers.

**Change**

Move protocol-name behavior out of core/codegen:

- `transfer`
- `claim`
- `settle`
- pool lifecycle helpers
- receipt redemption helpers

Each stable macro must publish:

- canonical core expansion
- required kernel effects
- trigger/scope/coverage facts
- builder assumptions
- source-map spans for macro expansion

**Acceptance**

- core/codegen has no protocol-name special cases for stable protocol flows
- `cellc explain-macro` shows source-to-expansion-to-ProofPlan mapping
- strict tests reject hidden protocol recognizers
- stdlib macro output matches accepted/rejected transaction fixtures

---

### 6. Standard CKB Contract Compatibility Suite

**Problem**

CellScript can express CKB-native semantics, but it still needs fixture-level compatibility with standard CKB scripts and ecosystem conventions.

**Change**

Create compatibility suites for:

- sUDT
- xUDT
- ACP
- Cheque
- Omnilock-compatible lock patterns
- NervosDAO-style epoch/since fixtures
- Type ID

Each suite must cover:

- script args layout
- witness layout
- Molecule data layout
- ScriptGroup and `outputs` / `outputs_data` positive and negative transaction fixture matrices
- error behavior
- accepted/rejected transaction fixtures
- cycle envelope
- script reference metadata

**Artifacts**

- `tests/compat/ckb_standard/`
- fixture transactions
- expected metadata snapshots
- cycle reports

**Acceptance**

- CellScript fixtures match standard script behavior for accepted/rejected cases
- metadata exposes exact script args/witness/data assumptions
- incompatibilities are documented as intentional and profile-gated

---

### 7. Script Role and Lock Identity Precision

**Problem**

v0.15 exposes lock/type trigger semantics in metadata, but source code can still
lean on generic `Address`-like values and implicit entry-role inference in some
places. That is too loose for production compatibility and audit tooling.

**Change**

Add strict source distinctions:

- `Address`
- `LockScript`
- `LockHash`
- `TypeScript`
- `TypeHash`

Require explicit entry-role declarations where ambiguity affects trigger scope:

```cellscript
#[entry(lock)]
lock owner_guard(...)

#[entry(type)]
action transfer(...)
```

**Acceptance**

- strict mode rejects implicit signer/lock-hash coercions
- `#[entry(lock)]` and `#[entry(type)]` determine active ScriptGroup metadata
- error messages explain whether a proof is lock-triggered or type-triggered
- compatibility fixtures cover lock/type ScriptGroup positive and negative cases

---

### 8. Builder Assumption Contract

**Problem**

v0.15 marks builder assumptions, but wallets, SDKs, relayers, and transaction builders need a stable contract to honor them.

**Change**

Define a builder assumption schema:

```text
assumption_id
kind
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
- SDK assumption validator

**Acceptance**

- every builder assumption has a stable schema record
- generated transaction templates include assumption IDs
- validation rejects transactions that violate assumptions before signing

---

## P1

### 9. Versioned Data-Layout Policies

**Problem**

0.15 can preserve identity and expose layout assumptions, but protocol upgrades
need explicit rules for preserving, migrating, or rejecting data layout changes.

**Change**

Add versioned layout policies:

```text
preserve_layout<T>(version = ...)
migrate_layout<T>(from = ..., to = ...)
reject_layout<T>(version = ...)
```

Policies must connect source fields, output fields, schema hashes, and
ProofPlan obligations.

**Acceptance**

- replacements without required layout policy fail in strict mode
- migration fixtures prove accepted and rejected layout transitions
- audit bundles show layout version diffs and field-level obligations

---

### 10. Non-TYPE-ID Global Uniqueness Proof Boundaries

**Problem**

v0.15 emits local runtime anchors for field-, script-args-, and singleton-type
identity creation, but global uniqueness outside TYPE_ID still needs
builder/indexer/deployment evidence.

**Change**

Define proof boundaries for non-TYPE-ID uniqueness:

- on-chain absence checks where ScriptGroup scope can prove them
- builder/indexer assumption records where CKB-VM cannot see global state
- registry/deployment manifests for singleton and script-args identities

**Acceptance**

- `create_unique(field(...))`, `create_unique(script_args)`, and
  `create_unique(singleton_type)` cannot overstate global on-chain proof
- transaction fixtures cover valid creation, duplicate rejection, and
  builder-assumption-only cases
- ProofPlan distinguishes local anchors from global uniqueness certification

---

### 11. Transaction Solver

**Problem**

Builder templates are not enough. Real applications need a solver for cell selection, capacity, fees, change outputs, witness placement, dep resolution, and multi-party signing flows.

**Change**

Add a transaction solver that consumes:

- action metadata
- ProofPlan
- builder assumptions
- available cells
- signing policy
- target profile

Solver responsibilities:

- cell selection
- dep resolution
- output planning
- occupied capacity calculation
- fee/change planning
- witness placement
- signature request manifest
- dry-run validation

**Acceptance**

- solver can build transactions for all bundled examples
- solver emits a deterministic signing manifest
- solver validates builder assumptions before finalization
- failure messages point to missing cells, deps, witnesses, or capacity

---

### 12. Deployment and Upgrade Governance

**Problem**

CKB deployment is a governance problem: code cells, dep groups, hash types, Type ID, audit labels, and version locks need a stable workflow.

**Change**

Add deployment governance artifacts:

- code cell manifest
- dep group manifest
- version lock file
- audit hash record
- upgrade policy
- rollback policy
- script reference registry entry

Add commands:

```bash
cellc deploy-plan
cellc verify-deploy
cellc diff-deploy
cellc lock-deps
```

**Acceptance**

- deployments are reproducible from manifests
- upgrade diffs identify script hash, args, data layout, and ProofPlan changes
- registry entries include audit status and compatibility range

---

### 13. Audit and Debug UX

**Problem**

`explain-proof` is necessary but not enough for production audits. Developers need traceable source-to-code, proof diff, cycle, and transaction execution views.

**Change**

Add audit tooling:

- source maps from CellScript to RISC-V assembly
- proof diff between versions
- cycle profiler per invariant/check
- tx trace viewer
- coverage report for invariants and assumptions
- HTML audit bundle

**Commands**

```bash
cellc explain-proof
cellc explain-macro
cellc proof-diff old.json new.json
cellc profile --entry transfer
cellc trace-tx tx.json
cellc audit-bundle
```

**Acceptance**

- audit bundle links source spans, ProofPlan obligations, emitted code, and metadata
- `cellc explain-macro` links source macro calls, canonical expansion, ProofPlan
  obligations, and emitted checks
- proof diff highlights changed trigger/scope/coverage semantics
- cycle profiler identifies the most expensive generated checks

---

### 14. Standard Library Release Track

**Problem**

v0.16 should not make the standard library the main language milestone, but the compatibility suite needs curated library modules for common patterns.

**Change**

Ship audited stdlib modules as wrappers over v0.15 scoped invariants:

- `std::sudt`
- `std::xudt`
- `std::type_id`
- `std::htlc`
- `std::cheque`
- `std::acp`

Rules:

- stdlib modules must expose ProofPlan
- no hidden builder assumptions
- compatibility fixtures required before marking stable

**Acceptance**

- each stable stdlib module has compatibility fixtures
- module docs include trigger/scope/coverage explanation
- audit bundle generated for each module

---

## P2

### 15. Advanced Linear Collections

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

### 16. Formal Verification Backend Exploration

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

## Release Gates

v0.16 cannot ship until:

- operational semantics document covers resource state, cell effects, triggers, scopes, and ProofPlan
- ProofPlan soundness checker is mandatory in strict mode
- invariant satisfaction gate is mandatory in strict mode and rejects declared
  invariants with no executable aggregate coverage or checked action coverage
- aggregate invariants have executable verifier lowering or fail strict executable coverage gates
- stable protocol flows lower through stdlib macros, not protocol-name codegen recognizers
- standard CKB compatibility suites cover accepted and rejected fixtures
- ScriptGroup and `outputs` / `outputs_data` positive and negative fixture matrices are included
- explicit lock/type entry roles and lock identity types are enforced in strict mode
- versioned data-layout preserve/migrate fixtures cover accepted and rejected replacements
- non-TYPE-ID global uniqueness boundaries cannot be reported as fully on-chain unless proven
- builder assumption schema is stable
- `cellc validate-tx` checks builder assumptions against a transaction
- transaction solver builds all bundled examples
- deployment manifests are reproducible
- audit bundle links source, ProofPlan, emitted code, metadata, and cycles
- `cellc explain-macro` links macro source, canonical expansion, ProofPlan, and emitted checks
- stdlib stable modules have compatibility fixtures and audit bundles
