# CellScript Operational Semantics

Status: v0.16 mechanically precise assurance spec.

This document defines the executable subset that v0.16 checks against compiler
metadata. It is intentionally operational: every rule maps to AST/type-checker
behavior, IR effects, ProofPlan records, or builder assumptions.

## Judgments

```text
E |- e => v
S |- stmt => S'
S |- action(params) => S'
S |- lock(params) => bool
M |- proof_plan => sound | issue(code)
T |- tx against assumptions => ok | reject
```

`E` is the expression environment. `S` is the linear resource state and Cell
effect log. `M` is compile metadata. `T` is a transaction JSON shape consumed by
`cellc validate-tx`.

## Expression Evaluation

Pure expressions are deterministic big-step evaluations.

```text
E(x) = v
---------------- Identifier
E |- x => v

E |- a => va    E |- b => vb    va op vb = v
------------------------------------------- Binary
E |- a op b => v

E |- f(args) => v    f is pure
-------------------------------- CallPure
E |- f(args) => v
```

Runtime CKB calls such as `source::group_input`, `witness::lock`,
`env::sighash_all`, and `read_ref<T>()` are not pure expression rules. They emit
runtime access metadata and ProofPlan reads.

## Linear Resource State

Each linear binding is in one state:

```text
Live(name, ty)
Consumed(name, ty, op)
Returned(name, ty)
Created(name, ty, output_index, op)
```

Core transition rules:

```text
S has Live(x, T)
---------------------- Consume
S |- consume x => S + Consumed(x, T, consume)

fields valid for T
------------------------------ Create
S |- create T { fields } => S + Created(_, T, output, create)

S has Live(x, T)    fields valid for T
--------------------------------------- ReplaceUnique
S |- replace_unique<T>(identity = p) x { fields }
   => S + Consumed(x, T, replace_unique) + Created(_, T, output, replace_unique)
```

Branch merge is conservative. A linear name may merge only when both branches
leave the same linear state for that name. Loops may not hide linear state
changes that cannot be statically bounded.

## Cell Effects

The IR effect log is the source of metadata truth:

```text
consume_set    input Cells consumed by consume/transfer/claim/settle/replace_unique
create_set     output Cells created by create/transfer/claim/settle/create_unique/replace_unique
read_refs      dependency-backed state read without consuming
mutate_set     input/output replacement obligations from &mut state
```

Each effect must either lower to executable verifier code, produce a
checked-runtime verifier obligation, or fail closed.

## Triggers And Scopes

Supported invariant triggers:

```text
explicit_entry
lock_group
type_group
```

Supported scopes:

```text
selected_cells
group
transaction
```

Soundness rule:

```text
trigger = lock_group    scope = transaction
--------------------------------------------- LockGroupTxScope
builder_assumption(lock group only protects matching lock inputs)
```

This rule prevents lock-group transaction scans from being presented as
transaction-wide type conservation.

## ProofPlan Meaning

Every ProofPlan field has a checked meaning:

| Field | Meaning |
|---|---|
| `origin` | Source of the obligation: action, function, lock, or invariant |
| `trigger` | Script execution trigger that can observe the obligation |
| `scope` | Cell set protected by the obligation |
| `reads` | CKB views read by generated verifier code or declared invariant |
| `coverage` | Macro provenance and generated coverage notes |
| `input_output_relation_checks` | Runtime/static checks that compare inputs and outputs |
| `group_cardinality` | Script-group cardinality model used by the proof |
| `identity_lifecycle_policy` | Identity rule applied to create/replace/destroy |
| `builder_assumptions` | Off-chain obligations that builders/indexers must satisfy |
| `codegen_coverage_status` | `covered`, `gap:*`, `fail-closed`, or builder-required |
| `on_chain_checked` | True only when generated code covers the obligation |

v0.16 soundness rule:

```text
VerifierObligation(category, feature, status) in M
ProofPlan(category, feature, status) not in M
--------------------------------------------------- MissingPlan
M |- proof_plan => issue(PP0002)
```

```text
ProofPlan(status = runtime-required) and strict_0_16
---------------------------------------------------- StrictGap
M |- proof_plan => issue(PP0150)
```

```text
ProofPlan(on_chain_checked = true, codegen_coverage_status != covered)
--------------------------------------------------------------------- Overstatement
M |- proof_plan => issue(PP0102)
```

The implementation of these rules is `proof_plan::soundness::check_metadata`.
`--primitive-strict=0.16` makes the strict rules mandatory.

## Builder Assumptions

Builder assumptions are not prose in v0.16. They are schema records with:

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

`cellc explain-assumptions` emits this schema. `cellc validate-tx --against
metadata.json tx.json` checks a concrete transaction shape before signing.

## Conformance Fixtures

The conformance tests live in `tests/v0_16.rs` and
`tests/compat/ckb_standard/manifest.json`. They check:

- ProofPlan soundness metadata is emitted and passes for checked runtime cases;
- `--primitive-strict=0.16` rejects metadata-only ProofPlan gaps;
- builder assumption evidence is required by `validate-tx`;
- the standard CKB compatibility suite names accepted and rejected fixtures for
  sUDT, xUDT, ACP, Cheque, Omnilock, NervosDAO since/epoch, and Type ID.
