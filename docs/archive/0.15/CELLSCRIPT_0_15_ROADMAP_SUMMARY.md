# CellScript 0.15 Roadmap

> Archived on 2026-05-26 after the `v0.15.0` release. The canonical planning
> tracker remains `roadmap/CELLSCRIPT_0_15_ROADMAP.md`; this file is a historical
> branch summary.

**Updated**: 2026-05-20

0.15 is the scoped-invariant, Covenant ProofPlan, and soundness hardening
preview track. It builds on the 0.14 CKB semantic surface by making verifier
trigger, scope, reads, coverage, builder assumptions, and enforcement gaps
explicit in source and metadata. It also records bounded invariant/action
coverage links so auditors can see which checked action obligations satisfy a
declared aggregate invariant and which declarations remain unmatched.

The release target is deliberately narrow: close known fail-open and semantic
boundary bugs, add negative regression coverage, and establish the boundary
scaffolding for 0.16. Full type-enforced status/value separation, complete
SyscallSpec migration, CFG lifecycle merging, hard schema API gates, and
source-to-runtime ProofPlan coverage linking are 0.16 work.

## Goals

1. Add a source-level scoped invariant model.
2. Add aggregate invariant primitives for common covenant-style relations.
3. Emit Covenant ProofPlan metadata for source invariants and compiler-recognized
   protocol flows.
4. Surface dangerous lock/type coverage assumptions as diagnostics.
5. Keep metadata-only invariant claims clearly separated from executable CKB
   verifier coverage.
6. Promote cell identity into a first-class primitive policy.
7. Reset resource capability vocabulary from protocol verbs to kernel effects.
8. Add explicit destruction policies while keeping bare `destroy` as the default compatibility form.
9. Provide a compat/strict migration path from v0.14 to v0.15.
10. Close known fail-open verifier paths and semantic-boundary leaks without
    broad late-cycle refactors.
11. Clearly distinguish enforced 0.15 behavior from scaffolded 0.16
    architecture.

## Implemented In This Branch

| Track | Status | Notes |
|---|---|---|
| Scoped invariant syntax | Implemented | Top-level `invariant` declarations require explicit `trigger`, `scope`, and `reads`. Supported triggers are `explicit_entry`, `lock_group`, and `type_group`; supported scopes are `selected_cells`, `group`, and `transaction`. |
| Invariant IR and metadata model | Implemented | Invariants are preserved through AST, type checking, IR, module metadata, formatting, LSP symbols, hover/completions, docs, and scoped CKB entry compilation. |
| Aggregate invariant primitives | Implemented with metadata-only aggregate lowering and action-coverage links | `assert_sum`, `assert_conserved`, `assert_delta`, `assert_distinct`, and `assert_singleton` are parsed, type-checked, formatted, lowered into IR metadata, and emitted into ProofPlan records. Aggregate fields must resolve to fixed-width integer or fixed-byte schema fields. Non-literal `assert_delta` arguments must be bound through `reads` to `witness.*` or `lock_args.*`. |
| Covenant ProofPlan metadata | Implemented | Runtime, action, function, and lock metadata expose ProofPlan records with trigger, scope, reads, coverage, relation checks, group cardinality, identity/lifecycle policy, builder assumptions, diagnostics, and codegen coverage status. |
| `cellc explain-proof` | Implemented | The CLI emits human-readable and JSON ProofPlan output for packages and single `.cell` files. |
| Invariant/action coverage cross-reference | Implemented | Declared aggregate invariants are matched against checked action obligations where the type, field, relation, and runtime coverage line up. Unmatched declarations are marked with `declared(no_checked_action_obligation_matches:...)` and warning diagnostics. |
| Runtime-obligation policy gate | Implemented | `cellc check --deny-runtime-obligations` rejects runtime-required ProofPlan gaps, including declared invariants whose coverage is still metadata-only or whose action coverage is unmatched. |
| Production checked-runtime evidence gate | Implemented | Production and strict checks reject ProofPlan records that claim checked runtime coverage without executable evidence. Static details such as `checked-static` remain metadata/static evidence and do not populate executable runtime evidence. |
| Semantic-boundary hardening | Implemented as 0.15 hardening | Known fail-open returns, helper/syscall status leaks, non-canonical lock truthiness, malformed Molecule field access, branch-local or duplicate lifecycle effects, package path escapes, and non-const const initializers are either fail-closed or rejected. |
| Boundary architecture scaffold | Implemented as scaffold | 0.15 introduces the initial SyscallSpec inventory, IR status-boundary checks, validated schema planning objects, ResourceEffectSummary scaffold, and ProofPlan executable-evidence metadata. These make the release safer but are not yet the final type-enforced architecture. |
| Lock-group transaction risk diagnostics | Implemented | ProofPlan records warn when a `lock_group` verifier scans transaction-wide views, because only inputs sharing that lock trigger the verifier. |
| Protocol macro provenance | Implemented | ProofPlan coverage records include macro provenance for selected compiler-recognized flows such as `transfer`, `create`, `claim`, `settle`, `consume`, `destroy`, and pool protocol metadata. |
| Cell identity and TYPE_ID lifecycle | Implemented with executable local verifier boundary | `IdentityPolicy` enum (`none`, `ckb_type_id`, `field(path)`, `script_args`, `singleton_type`) is a first-class type metadata primitive. `TypeMetadata.identity_policy` exposes the policy in compiled JSON metadata. `create_unique<T>(identity = ...)` and `replace_unique<T>(identity = ...)` lower through identity-aware IR/codegen records. `replace_unique` preserves field, script-args/lock-hash, and singleton/type-hash identity on chain. `create_unique` anchors the declared identity to the created output and reports global uniqueness, including TYPE_ID builder-plan completion, as runtime-required. |
| Explicit destruction policies | Implemented with policy-aware verifier boundary | `DestructionPolicy` enum (`Default`, `SingletonType`, `Unique`, `Instance`, `BurnAmount`) makes destruction intent explicit while retaining bare `destroy` as `Default`. Parser supports `destroy_singleton_type(cell)`, `destroy_unique(cell, identity = type_id)`, `destroy_instance(cell, identity_field = id)`, and `burn_amount(cell, field = amount)`. `IrInstruction::Destroy` carries `policy: IrDestructionPolicy` through IR/codegen. Singleton/type-id destruction emits the output absence scan; instance and amount policies are metadata-visible runtime-required gaps instead of being lowered as over-broad TypeHash absence. |
| Kernel/protocol primitive split | Implemented | AST `Capability` extended with `Create`, `Consume`, `Replace`, `Burn`, `Relock`, `RetargetType`, `ReadRef`. New capabilities are context-sensitive identifiers in `has ...` clauses. `create_unique`/`replace_unique` are identity-aware lifecycle forms distinct from bare `create`/`transfer`. |
| Capability vocabulary reset | Implemented | Strict mode (`--primitive-strict=0.15`) rejects `has transfer` and `has destroy` with diagnostic CS0150/CS0151. Compatibility mode (`--primitive-compat=0.14`) accepts legacy vocabulary. `Capability::is_protocol_verb()` and `Capability::kernel_effects()` classify capabilities for migration. |
| Internal `type_hash` renaming | Implemented | Metadata fields renamed: `type_hash-absence` → `ckb_type_script_hash-absence`, `type_hash-preservation` → `ckb_type_script_hash-preservation`, `lock_hash-preservation` → `ckb_lock_script_hash-preservation`. |
| Compatibility and migration infrastructure | Implemented | `--primitive-compat=0.14` and `--primitive-strict=0.15` CLI flags. CS0150–CS0160 migration diagnostic codes. `check_primitive_strict_015()` gate rejects protocol verbs in strict mode. |
| Documentation and tests | Implemented | README, docgen, CLI tests, parser tests, metadata tests, identity policy tests (5 new), and aggregate invariant tests cover the new surface. |
| Release gate evidence | Passed | `./scripts/cellscript_gate.sh release` passed on 2026-05-20. The recorded CKB production report is `target/ckb-cellscript-acceptance/20260520-215759-25169/ckb-cellscript-acceptance-report.json` with `status: "passed"` and `production_ready: true`. |

## Boundaries

- Declared invariants and aggregate primitives are currently ProofPlan metadata,
  not automatic aggregate verifier-loop lowering. They intentionally emit
  `codegen_coverage_status: "gap:metadata-only"` and `status:
  "runtime-required"` until a later lowering pass proves them on chain. When a
  matching checked action obligation already exists, ProofPlan records that
  bounded action coverage separately; unmatched invariants remain visible and
  gateable.
- 0.15 is a hardening release, not a full boundary-architecture rewrite. It
  blocks known dangerous paths and documents remaining gaps instead of
  broadening behavior late in the cycle.
- SyscallSpec is an inventory and generation foundation, but not every runtime,
  env, header, input, hash, Molecule, memory, or internal helper is fully
  spec-derived in 0.15.
- IR status checks prevent known status/helper values from becoming ordinary
  DSL values, but full first-class `Bool` / `DomainU64` / `ErrorCode` /
  `ExitStatus` / `SyscallStatus` / `HelperStatus` typing is deferred.
- Validated schema planning objects gate the known semantic access paths, but
  the complete hard Rust API boundary that makes raw span bypass impossible is
  0.16 scope.
- Lifecycle effects remain conservatively rejected in ambiguous branch-local or
  duplicate cases. Safe CFG-aware merging of mutually exclusive lifecycle paths
  is not part of 0.15.
- ProofPlan executable evidence is initial runtime/codegen evidence metadata,
  not a complete source obligation to runtime proof linker.
- `assert_delta` accepts literal deltas, or non-literal deltas that are bound
  through `reads` to `witness.*` or `lock_args.*`. Bare names and cell reads are
  rejected so the ProofPlan cannot contain an untraceable runtime value.
- `lock_group + transaction` means the verifier can inspect transaction-wide
  views, but the active CKB trigger is still the lock ScriptGroup. Builders and
  auditors must not read that as type-group conservation.
- Aggregate primitives only accept fixed-width fields so future executable
  lowering has a concrete ABI boundary. Dynamic tables, generic collections, and
  bool fields are rejected for aggregate relation targets.
- `assert_sum(...) <= assert_sum(...)` records a relation check in ProofPlan, but
  it does not yet generate an output-scan verifier.
- `replace_unique(identity = field(...))`,
  `replace_unique(identity = script_args)`, and
  `replace_unique(identity = singleton_type)` emit executable input/output
  identity-preservation checks. `create_unique(identity = field(...))` anchors
  the created output field to the verifier-covered output, while
  `create_unique(identity = script_args | singleton_type)` anchors the output
  lock/type hash to the active ScriptGroup input. These are local transaction
  verifier checks; the full create-time uniqueness proof is reported as
  runtime-required and must be provided by TYPE_ID builder planning or
  builder/indexer policy.
- Protocol macro provenance is audit metadata. It records how recognized source
  effects map to consume/create/write-intent shapes; it is not a replacement for
  builder-backed CKB transaction evidence.
- The 0.15.0 implementation does not claim covenant helper source functions
  (`lock_covenant(...)`, `type_invariant(...)`, `builder_assumption(...)`),
  an `Address` / `LockScript` / `LockHash` type split, explicit
  `#[entry(lock)]` / `#[entry(type)]` declarations, versioned data-layout
  preserve/migrate policies, `claim_proof(...)`, explicit split/merge/rebalance
  cardinality forms, full `cellc explain-macro` source maps, or moving `shared`
  entirely into a scheduler policy library.

## Future Direction: 0.16 Enforced Boundary Architecture

In 0.15, invariants are declared ProofPlan obligations rather than implicitly
executed verifier functions. This is intentional: an invariant is only sound
when its trigger, scope, reads, and CKB script boundary are explicit.

The next step is invariant satisfaction checking. A declared invariant should be
considered production-satisfied only if one of the following holds:

1. it has been lowered into executable verifier code;
2. it is matched by a checked action obligation with compatible trigger, scope,
   type, field, and relation coverage;
3. it is rejected by strict or production gates as runtime-required.

Aggregate primitives such as `assert_sum`, `assert_conserved`, `assert_delta`,
`assert_distinct`, and `assert_singleton` are the first candidates for
executable lowering, because their fixed-width field restrictions already
provide a bounded ABI and scanner shape.

0.16 should turn the 0.15 scaffolding into harder architectural boundaries:

1. derive all runtime, stdlib, and helper wrappers from a complete SyscallSpec;
2. make status-like values first-class internal IR classes that cannot flow into
   domain returns, locals, tuples, or arguments;
3. make semantic Molecule field access consume validated table and field objects
   instead of raw spans;
4. replace flat lifecycle collectors with CFG-aware ResourceEffectSummary
   merging;
5. link ProofPlan records from source obligation to IR obligation,
   codegen/runtime check, and evidence ID;
6. add more VM-level malformed Molecule and syscall-failure negative tests.

## Verification

Focused 0.15 checks:

```bash
cargo test --locked -p cellscript proof_plan --lib
cargo test --locked -p cellscript aggregate_invariant --lib
cargo test --locked -p cellscript explain_proof --test cli
cargo test --locked -p cellscript docgen --lib
cargo run --locked -p cellscript -- explain-proof examples/token.cell --target-profile ckb --json
```

Full gate before closing the branch:

```bash
./scripts/cellscript_gate.sh dev
./scripts/cellscript_gate.sh ci
./scripts/cellscript_gate.sh backend
./scripts/cellscript_gate.sh release
```

Latest recorded release evidence:

- CKB production acceptance report:
  `target/ckb-cellscript-acceptance/20260520-215759-25169/ckb-cellscript-acceptance-report.json`.
- Strict backend CI audit report:
  `target/cellscript-strict-backend-audit/strict-backend-audit-ci-20260520-215735.json`.
- Backend shape report:
  `target/cellscript-backend-shape/backend-shape-report-release.json`.
- Molecule schema manifest report:
  `target/cellscript-schema-manifest/schema-manifest-report-release.json`.
- Production result: 7 bundled examples, 43/43 scoped actions, 17/17 scoped
  locks, 17 valid-spend and 17 invalid-spend lock cases, 27 stateful local CKB
  scenario runs, and final production hardening gate `passed`.
