# CellScript 0.15 Roadmap

**Updated**: 2026-04-28

0.15 is the scoped-invariant and Covenant ProofPlan track. It builds on the
0.14 CKB semantic surface by making verifier trigger, scope, reads, coverage,
builder assumptions, and enforcement gaps explicit in source and metadata.

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

## Implemented In This Branch

| Track | Status | Notes |
|---|---|---|
| Scoped invariant syntax | Implemented | Top-level `invariant` declarations require explicit `trigger`, `scope`, and `reads`. Supported triggers are `explicit_entry`, `lock_group`, and `type_group`; supported scopes are `selected_cells`, `group`, and `transaction`. |
| Invariant IR and metadata model | Implemented | Invariants are preserved through AST, type checking, IR, module metadata, formatting, LSP symbols, hover/completions, docs, and scoped CKB entry compilation. |
| Aggregate invariant primitives | Implemented as metadata-only | `assert_sum`, `assert_conserved`, `assert_delta`, `assert_distinct`, and `assert_singleton` are parsed, type-checked, formatted, lowered into IR metadata, and emitted into ProofPlan records. Aggregate fields must resolve to fixed-width integer or fixed-byte schema fields. |
| Covenant ProofPlan metadata | Implemented | Runtime, action, function, and lock metadata expose ProofPlan records with trigger, scope, reads, coverage, relation checks, group cardinality, identity/lifecycle policy, builder assumptions, diagnostics, and codegen coverage status. |
| `cellc explain-proof` | Implemented | The CLI emits human-readable and JSON ProofPlan output for packages and single `.cell` files. |
| Runtime-obligation policy gate | Implemented | `cellc check --deny-runtime-obligations` rejects runtime-required ProofPlan gaps, including declared invariants whose coverage is still metadata-only. |
| Lock-group transaction risk diagnostics | Implemented | ProofPlan records warn when a `lock_group` verifier scans transaction-wide views, because only inputs sharing that lock trigger the verifier. |
| Protocol macro provenance | Implemented | ProofPlan coverage records include macro provenance for selected compiler-recognized flows such as `transfer`, `create`, `claim`, `settle`, `consume`, `destroy`, and pool protocol metadata. |
| Cell identity and TYPE_ID lifecycle | Implemented with executable local verifier boundary | `IdentityPolicy` enum (`none`, `ckb_type_id`, `field(path)`, `script_args`, `singleton_type`) is a first-class type metadata primitive. `TypeMetadata.identity_policy` exposes the policy in compiled JSON metadata. `create_unique<T>(identity = ...)` and `replace_unique<T>(identity = ...)` lower through identity-aware IR/codegen records. `replace_unique` preserves field, script-args/lock-hash, and singleton/type-hash identity on chain. `create_unique` anchors the declared identity to the created output and reports global uniqueness, including TYPE_ID builder-plan completion, as runtime-required. |
| Explicit destruction policies | Implemented with policy-aware verifier boundary | `DestructionPolicy` enum (`Default`, `SingletonType`, `Unique`, `Instance`, `BurnAmount`) makes destruction intent explicit while retaining bare `destroy` as `Default`. Parser supports `destroy_singleton_type(cell)`, `destroy_unique(cell, identity = type_id)`, `destroy_instance(cell, identity_field = id)`, and `burn_amount(cell, field = amount)`. `IrInstruction::Destroy` carries `policy: IrDestructionPolicy` through IR/codegen. Singleton/type-id destruction emits the output absence scan; instance and amount policies are metadata-visible runtime-required gaps instead of being lowered as over-broad TypeHash absence. |
| Kernel/protocol primitive split | Implemented | AST `Capability` extended with `Create`, `Consume`, `Replace`, `Burn`, `Relock`, `RetargetType`, `ReadRef`. New capabilities are context-sensitive identifiers in `has ...` clauses. `create_unique`/`replace_unique` are identity-aware lifecycle forms distinct from bare `create`/`transfer`. |
| Capability vocabulary reset | Implemented | Strict mode (`--primitive-strict=0.15`) rejects `has transfer` and `has destroy` with diagnostic CS0150/CS0151. Compatibility mode (`--primitive-compat=0.14`) accepts legacy vocabulary. `Capability::is_protocol_verb()` and `Capability::kernel_effects()` classify capabilities for migration. |
| Internal `type_hash` renaming | Implemented | Metadata fields renamed: `type_hash-absence` → `ckb_type_script_hash-absence`, `type_hash-preservation` → `ckb_type_script_hash-preservation`, `lock_hash-preservation` → `ckb_lock_script_hash-preservation`. |
| Compatibility and migration infrastructure | Implemented | `--primitive-compat=0.14` and `--primitive-strict=0.15` CLI flags. CS0150–CS0160 migration diagnostic codes. `check_primitive_strict_015()` gate rejects protocol verbs in strict mode. |
| Documentation and tests | Implemented | README, docgen, CLI tests, parser tests, metadata tests, identity policy tests (5 new), and aggregate invariant tests cover the new surface. |

## Boundaries

- Declared invariants and aggregate primitives are currently ProofPlan metadata,
  not executable verifier lowering. They intentionally emit
  `codegen_coverage_status: "gap:metadata-only"` and `status:
  "runtime-required"` until a later lowering pass proves them on chain.
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

## Verification

Focused 0.15 checks:

```bash
cargo test --locked -p cellscript proof_plan --lib
cargo test --locked -p cellscript aggregate_invariant --lib
cargo test --locked -p cellscript explain_proof --test cli
cargo run --locked -p cellscript -- explain-proof examples/token.cell --target-profile ckb --json
```

Full gate before closing the branch:

```bash
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
```
