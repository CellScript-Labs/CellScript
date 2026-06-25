# Tutorial 11: Scoped Invariants and ProofPlan

CellScript's current invariant surface started in 0.15 and remains part of the
0.20 authoring model. This chapter explains what it is for, what the compiler
records today, and how to read the evidence without mistaking metadata for
executable verifier code.

## What You Will Learn

- how to declare an invariant with an explicit trigger, scope, and read set;
- how the aggregate invariant primitives map to ProofPlan records;
- how to inspect those records with `cellc explain-proof`;
- which ProofPlan records are checked on chain today and which are
  `gap:metadata-only`;
- how to use ProofPlan output in reviews and production gates.

## The Core Rule

A scoped invariant is an auditable protocol claim. It must say when it is meant
to run, which cells it covers, and which CKB views it reads.

```cellscript
invariant token_amount_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount

    assert_sum(group_outputs<Token>.amount) == assert_sum(group_inputs<Token>.amount)
}
```

Read this as:

- `trigger: type_group`: the claim belongs to the type-script group path;
- `scope: group`: it talks about cells in the current script group;
- `reads`: review tools and builders must know which transaction views the claim
  depends on;
- `assert_sum(...) == assert_sum(...)`: the conservation relation the protocol
  wants to preserve.

The compiler does not let the claim stay implicit. It emits Covenant ProofPlan
records so reviewers can see the intended trigger, scope, reads, relation checks,
coverage status, warnings, and builder assumptions.

## Triggers

The current invariant surface supports three invariant triggers:

| Trigger | Use it when |
|---|---|
| `explicit_entry` | The invariant is attached to a specific action/entry-style path or selected-cell flow. |
| `lock_group` | The invariant belongs to a CKB lock-group spend boundary. |
| `type_group` | The invariant belongs to a CKB type-script group path. |

A trigger is not a scheduler hint. It is the verifier boundary the invariant is
claiming to describe.

## Scopes

The current invariant surface supports three scopes:

| Scope | Meaning |
|---|---|
| `selected_cells` | The invariant covers cells selected by explicit effects such as `consume`, `create`, `read_ref`, or mutation summaries. |
| `group` | The invariant covers the current script group. |
| `transaction` | The invariant talks about a transaction-wide view such as all outputs of a type. |

Transaction-wide scopes are powerful but risky. ProofPlan will surface warnings
when a verifier boundary cannot by itself guarantee that a transaction-wide view
has been fully checked.

## Aggregate Primitives

The aggregate primitives are:

| Primitive | Typical use |
|---|---|
| `assert_sum(view.field)` | Compare sums over input/output views. |
| `assert_conserved(Type.field, scope = ...)` | Declare field conservation across a scope. |
| `assert_delta(Type.field, witness_or_value, scope = ...)` | Declare an allowed numeric delta. |
| `assert_distinct(view.field, scope = ...)` | Declare uniqueness over a view. |
| `assert_singleton(Type.field, scope = ...)` | Declare singleton-style membership. |

Example from `examples/language/v0_15_scoped_invariant.cell`:

```cellscript
invariant nft_no_duplicates {
    trigger: type_group
    scope: transaction
    reads: outputs<NFT>.token_id

    assert_distinct(outputs<NFT>.token_id, scope = transaction)
}
```

This does not hide the hard part. A transaction-wide uniqueness claim needs the
builder and verifier boundary to agree on what was read. ProofPlan records that
assumption instead of pretending it is automatically solved.

## Simple Invariant Assertions

For boolean checks that do not need aggregate primitives, use `assert_invariant`
inside the invariant body:

```cellscript
invariant token_positive {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount

    assert_invariant(true, "placeholder for future executable check")
}
```

`assert_invariant` is accepted alongside aggregate primitives. It is recorded in
ProofPlan metadata and counts toward `declared_invariant_assertions` coverage.
Like aggregate primitives, it is currently metadata-only unless later action
evidence or stricter gates close it.

## Inspect ProofPlan Output

Run:

```bash
cargo run --locked --bin cellc -- explain-proof \
  examples/language/v0_15_scoped_invariant.cell \
  --target riscv64-elf \
  --target-profile ckb
```

The first lines summarize the audit surface:

```text
Covenant ProofPlan for module `cellscript::language::v0_15_scoped_invariant`
  Summary:
    records: 16
    on_chain_checked: 6
    runtime_required: 10
    checked_partial: 0
    metadata_only_gaps: 10
    fail_closed: 0
    diagnostic_errors: 0
    diagnostic_warnings: 12
    macro_provenance_records: 2
    invariant_action_matches: 0
    invariant_unmatched_action_coverage: 2
```

The exact counts may change as the compiler grows, but the categories matter:

- `records`: total ProofPlan entries emitted;
- `on_chain_checked`: obligations represented by executable checks today;
- `runtime_required`: obligations that still need runtime/builder/verifier
  evidence;
- `checked_partial`: obligations where only a subset of checks are executable;
- `metadata_only_gaps`: declared claims that are not yet executable verifier
  lowering;
- `fail_closed`: obligations that fail closed at runtime because lowering is
  not yet available;
- `diagnostic_errors` / `diagnostic_warnings`: review issues that deserve human
  attention;
- `macro_provenance_records`: macro-generated obligation records;
- `invariant_action_matches`: invariant claims with matching action evidence;
- `invariant_unmatched_action_coverage`: related actions that still lack
  invariant evidence.

## Read One Record

A declared invariant record looks like this in text form:

```text
constraint: token_amount_conservation
  origin: invariant:token_amount_conservation
  trigger: type_group
  scope: group
  reads:
    - group_inputs<Token>.amount
    - group_outputs<Token>.amount
    - Source::GroupOutput
    - Source::GroupInput
  coverage:
    - declared_invariant_assertions:0
    - aggregate_assertion:group_outputs<Token>.amount==group_inputs<Token>.amount scope=group
    - type ScriptGroup coverage: cells sharing this type script
    - invariant_coverage:aggregate_action_evidence_matches=0/1
  relation_checks:
    - assert_sum:group_outputs<Token>.amount==group_inputs<Token>.amount=metadata-only
  on_chain_checked: no
  codegen_coverage_status: gap:metadata-only
  builder_assumption:
    - declared(metadata-only invariant not yet lowered to executable verifier code)
    - declared(assert_invariant_count:0)
    - declared(aggregate_invariant_count:1)
    - declared(no_aggregate_action_evidence_matches)
  warning: declared invariant is metadata-only until executable lowering covers it
```

Interpretation:

- `origin` tells you which source construct emitted the record;
- `trigger` and `scope` are the intended CKB boundary;
- `reads` is the audit read set (the compiler may append inferred sources);
- `coverage` describes how the invariant maps to action evidence and script-group
  semantics;
- `relation_checks` lists the invariant primitive and relation;
- `on_chain_checked: no` means this record is not executable verifier code yet;
- `gap:metadata-only` means the compiler preserved the claim for audit, but the
  production system still needs a closing mechanism;
- `builder_assumption` lists metadata obligations that builders or reviewers must
  close;
- `warning` surfaces review notes that deserve human attention.

## Metadata-Only Is Not Failure

In the default development mode, many declared aggregate invariants intentionally emit
`gap:metadata-only`. That is useful, not useless:

- reviews can see the intended invariant;
- CI can reject unexpected runtime-required gaps with policy flags;
- builders can inspect what transaction views must be supplied;
- future executable lowering has a stable metadata target to close.

But it is not the same as an on-chain proof. Do not claim a metadata-only
invariant is enforced by CKB-VM.

## Why Invariants Have No Generated Code

Declared invariants do not produce RISC-V instructions. The compiler records
their trigger, scope, reads, and aggregate relations into ProofPlan metadata,
but the code-generation stage treats invariants as a no-op. Every
action, function, and lock has an IR body that the code generator walks to emit
assembly; an invariant has no body, no verifier obligations, and no ABI slot,
so there is nothing for the code generator to lower.

This is not a temporary shortcut — it reflects a deliberate split between two
audit layers:

1. **Action-level checks** are executable. When an action calls `consume`,
   `create`, `require`, or performs a mutation, the compiler emits concrete
   on-chain verification instructions (field equality, type-hash presence,
   identity preservation, and so on). These become `on_chain_checked: true`
   ProofPlan records backed by `executable_evidence`.

2. **Invariant-level declarations** are auditable claims about what the
   protocol *should* guarantee. They live in metadata so that reviewers, CI
   pipelines, and future tooling can verify the claim is satisfied — by the
   action-level checks above, by builder policy, or by a future executable
   invariant lowering pass.

When an invariant's aggregate matches a checked action obligation, ProofPlan
records the link under `invariant_coverage:matched-action-obligation:*`. When
no action provides evidence, the invariant stays `runtime-required` with the
builder assumption `declared(no_checked_action_obligation_matches:...)`.

## How Soundness Audits Invariants

ProofPlan soundness runs two independent checks:

**Completeness — every verifier obligation has a ProofPlan record.** The
compiler collects verifier obligations from actions, functions, and locks, then
verifies that each one appears in the global ProofPlan set. This catches
missing audit trails.

**Consistency — local and runtime ProofPlan records agree.** For every action,
function, and lock, the compiler builds ProofPlan records from the local IR
body and also stores the same records at the global `runtime.proof_plan` level.
Soundness checks that both copies are identical (same trigger, scope, reads,
coverage, assumptions, detail). A mismatch signals a compiler bug.

Invariants are **exempt** from the consistency check. The reason is structural:
an invariant does not belong to any callable body, so there is no "local" copy
to compare against. Its ProofPlan records are generated directly from the
declaration and exist only at the runtime level. Applying the local-to-runtime
reconciliation to invariants would always report a false "missing from local"
error.

Instead, invariant soundness is guaranteed by a separate mechanism: the action
coverage link described above, plus the strict-mode gate described next.

## Strict Mode and Gradual Enforcement

CellScript uses a **progressive guarantee** model for invariant enforcement:

| Stage | What happens to invariants |
|---|---|
| Development (default) | Invariants emit `gap:metadata-only` and `runtime-required`. Compilation succeeds. Warnings and builder assumptions are recorded for review. |
| Pre-production (`--primitive-strict 0.16`) | Strict soundness rejects any ProofPlan record that is still `metadata-only` or `runtime-required` (PP0150). Every declared invariant must have matching action evidence or compilation fails. |
| CI gate (`--deny-runtime-obligations`) | Additionally rejects unmatched invariant action coverage, runtime-required transaction invariants, and partial ProofPlan gaps. |

Under strict mode, the compiler enforces the following invariant-specific
rules:

- **PP0150**: a `metadata-only` or `runtime-required` ProofPlan record is a
  compile error. The invariant must be closed by action evidence, lock/type
  verifier code, or executable lowering.
- **PP0101**: a ProofPlan record cannot simultaneously claim `on_chain_checked`
  and `runtime-required`.
- **PP0104**: a `gap:*` coverage status is incompatible with `on_chain_checked`.
- **PP0301**: an `on_chain_checked` record must not carry `runtime-required` or
  `metadata-only` builder assumptions.

These rules mean that in strict mode, an invariant is not just a declaration —
it is a contract that the rest of the module must fulfill with executable
evidence.

## Action Coverage Records

ProofPlan also compares invariant claims with action evidence when possible. If
an action has explicit `require`, `consume`, `create`, lifecycle, or cell-access
summaries that match an aggregate claim, ProofPlan can report evidence links.
These links are existential evidence, not a proof that every action touching the
same type is covered.

When there is no match, you may see assumptions such as:

```text
declared(no_aggregate_action_evidence_matches)
```

That means the invariant is still a runtime-required obligation until executable
invariant lowering, stronger action checks, or builder-side evidence closes the
gap.

When some related action origins still lack matching evidence, ProofPlan reports
`declared(unmatched_related_action_obligation_count:...)` so reviewers do not
mistake one matching action for exhaustive action coverage.

## Numeric Boundary Evidence

ProofPlan and release review should treat integer width changes at verifier
boundaries as explicit evidence. The language rule is expression-local unsigned
widening: `u8`, `u16`, `u32`, `u64`, and `u128` values may widen inside
arithmetic and numeric comparison, but not across assignment, return, ABI,
witness, layout, struct field initialization, or serialization boundaries.

For boundary values, the source should show intent:

```text
receipt.amount = value as u64
```

Do not read acceptance of mixed-width arithmetic as acceptance of implicit
numeric coercions throughout the language. Non-literal boundary conversions
must stay explicit so reviewers can see which transaction fields, witness
values, and serialized layouts changed width.

## JSON Output

For tooling, use:

```bash
cargo run --locked --bin cellc -- explain-proof \
  examples/language/v0_15_scoped_invariant.cell \
  --target riscv64-elf \
  --target-profile ckb \
  --json > /tmp/proof-plan.json
```

The JSON form is the right input for CI dashboards, release evidence, and custom
review tools.

## Production Review Checklist

Before treating an invariant as production evidence, check:

1. Does every invariant have the intended `trigger`?
2. Is the `scope` narrow enough for the actual verifier boundary?
3. Are all transaction views listed in `reads`?
4. Does `cellc explain-proof` report `gap:metadata-only`?
5. If there is a gap, who closes it: action checks, lock/type verifier code,
   builder policy, or future executable invariant lowering?
6. Are warnings about transaction-wide or lock-group coverage understood?
7. Does every aggregate invariant have at least one matching action obligation
   (see *Action Coverage Records* above)?
8. Does the package pass the appropriate production gate?

For package-level strict gates, run the check from a directory that contains
`Cell.toml`:

```bash
cd path/to/your-cellscript-package
cellc check --all-targets --target-profile ckb --production --primitive-strict 0.16
```

Under `--primitive-strict 0.16`, strict soundness rejects invariants that
remain `metadata-only` or `runtime-required` (PP0150). This means every
declared invariant must have corresponding action evidence or the build fails.
See *Strict Mode and Gradual Enforcement* for the full rule set.

For CI pipelines that must reject any outstanding runtime obligation:

```bash
cellc check --all-targets --target-profile ckb --deny-runtime-obligations
```

This additionally flags unmatched invariant action coverage, runtime-required
transaction invariants, and partial ProofPlan gaps.

The top-level CellScript repository is not itself a package root for these
commands unless you create a `Cell.toml` there.

## Where To Go Next

- Use `Tutorial-06-Metadata-Verification-and-Production-Gates` for artifact and
  metadata verification.
- Use `Tutorial-08-Bundled-Example-Contracts` to see production-oriented example
  contracts.
- Read `docs/releases/CELLSCRIPT_0_20_RELEASE_NOTES.md` for the current source,
  package, and evidence boundary.
- Read `docs/releases/CELLSCRIPT_0_15_RELEASE_NOTES.md`,
  `roadmap/CELLSCRIPT_0_16_ROADMAP.md`, and
  `docs/releases/CELLSCRIPT_0_16_2_RELEASE_NOTES.md` for the historical
  ProofPlan soundness and metadata-assurance boundary.
