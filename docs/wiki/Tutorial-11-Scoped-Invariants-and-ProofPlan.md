# Tutorial 11: Scoped Invariants and ProofPlan

CellScript 0.15 adds scoped invariants and Covenant ProofPlan metadata. This
chapter explains what they are for, what the compiler records today, and how to
read the evidence without mistaking metadata for executable verifier code.

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

CellScript 0.15 supports three invariant triggers:

| Trigger | Use it when |
|---|---|
| `explicit_entry` | The invariant is attached to a specific action/entry-style path or selected-cell flow. |
| `lock_group` | The invariant belongs to a CKB lock-group spend boundary. |
| `type_group` | The invariant belongs to a CKB type-script group path. |

A trigger is not a scheduler hint. It is the verifier boundary the invariant is
claiming to describe.

## Scopes

CellScript 0.15 supports three scopes:

| Scope | Meaning |
|---|---|
| `selected_cells` | The invariant covers cells selected by explicit effects such as `consume`, `create`, `read_ref`, or mutation summaries. |
| `group` | The invariant covers the current script group. |
| `transaction` | The invariant talks about a transaction-wide view such as all outputs of a type. |

Transaction-wide scopes are powerful but risky. ProofPlan will surface warnings
when a verifier boundary cannot by itself guarantee that a transaction-wide view
has been fully checked.

## Aggregate Primitives

The v0.15 aggregate primitives are:

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
    metadata_only_gaps: 10
    diagnostic_warnings: 12
```

The exact counts may change as the compiler grows, but the categories matter:

- `records`: total ProofPlan entries emitted;
- `on_chain_checked`: obligations represented by executable checks today;
- `runtime_required`: obligations that still need runtime/builder/verifier
  evidence;
- `metadata_only_gaps`: declared claims that are not yet executable verifier
  lowering;
- `diagnostic_warnings`: review warnings that deserve human attention.

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
  relation_checks:
    - assert_sum:group_outputs<Token>.amount==group_inputs<Token>.amount=metadata-only
  on_chain_checked: no
  codegen_coverage_status: gap:metadata-only
```

Interpretation:

- `origin` tells you which source construct emitted the record;
- `trigger` and `scope` are the intended CKB boundary;
- `reads` is the audit read set;
- `relation_checks` lists the invariant primitive and relation;
- `on_chain_checked: no` means this record is not executable verifier code yet;
- `gap:metadata-only` means the compiler preserved the claim for audit, but the
  production system still needs a closing mechanism.

## Metadata-Only Is Not Failure

In 0.15, many declared aggregate invariants intentionally emit
`gap:metadata-only`. That is useful, not useless:

- reviews can see the intended invariant;
- CI can reject unexpected runtime-required gaps with policy flags;
- builders can inspect what transaction views must be supplied;
- future executable lowering has a stable metadata target to close.

But it is not the same as an on-chain proof. Do not claim a metadata-only
invariant is enforced by CKB-VM.

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

Before treating a 0.15 invariant as production evidence, check:

1. Does every invariant have the intended `trigger`?
2. Is the `scope` narrow enough for the actual verifier boundary?
3. Are all transaction views listed in `reads`?
4. Does `cellc explain-proof` report `gap:metadata-only`?
5. If there is a gap, who closes it: action checks, lock/type verifier code,
   builder policy, or future executable invariant lowering?
6. Are warnings about transaction-wide or lock-group coverage understood?
7. Does the package pass the appropriate production gate?

For package-level strict gates, run the check from a directory that contains
`Cell.toml`:

```bash
cd path/to/your-cellscript-package
cellc check --all-targets --target-profile ckb --production --primitive-strict 0.15
```

If this fails with runtime-required ProofPlan gaps, the compiler is telling you
that metadata has exposed a real review obligation. The top-level CellScript
repository is not itself a package root for this command unless you create a
`Cell.toml` there.

## Where To Go Next

- Use `Tutorial-06-Metadata-Verification-and-Production-Gates` for artifact and
  metadata verification.
- Use `Tutorial-08-Bundled-Example-Contracts` to see production-oriented example
  contracts.
- Read `docs/CELLSCRIPT_0_15_RELEASE_NOTES_DRAFT.md` for the release boundary.
- Read `roadmap/CELLSCRIPT_0_16_ROADMAP.md` for future ProofPlan soundness work.
