# CellScript Output Bindings

**Status**: production semantics for the current CellScript CKB profile.

CellScript models persistent state as Cell transformations, not in-place object
mutation. The canonical one-to-one input/output form is:

```cellscript
action update(before: State) -> after: State
where
    require after.owner == before.owner
    require after.counter == before.counter + 1
```

Read this as:

```text
Input#N before  ->  Output#M after
```

`before` is consumed transaction evidence. `after` is a proposed output Cell.
The action signature gives the transaction topology; field preservation,
arithmetic transitions, authorization, capacity, and asset-conservation rules
remain explicit `require` or verifier checks.

## Required Checks

For each named input/output transition, generated metadata records:

- input cell data binding
- output cell data binding
- scheduler-visible input/output access for shared state
- field reads needed by `require` and `move`
- declared state transition edges from `flow`/`move`

The compiler does not inject a hidden state field, does not alter Molecule
layout behind the source schema, and does not use a narrative lineage keyword.
If a state transition
crosses two variables, the full field-to-field `move` names both sides:

```cellscript
move before.state: Live -> after.state: Filled
```

For non-state continuity, write the required preservation or arithmetic
constraints explicitly with `require`.

## Transition Shapes

Current production transition checks are ordinary source requirements:

| Shape | Source form |
|---|---|
| Preserve | `require after.owner == before.owner` |
| Set | `require after.owner == new_owner` |
| Add | `require after.balance == before.balance + delta` |
| Sub | `require after.balance == before.balance - delta` |
| State edge | `move before.state: A -> after.state: B` |

Unsupported runtime shapes must remain fail-closed and must use a registered
runtime error code.

## AMM Pool Example

`examples/amm_pool.cell` is the canonical advanced shared-output example:

- `swap_a_for_b` updates pool reserves through explicit add/sub requirements
- `add_liquidity` updates reserves and LP supply through proportional updates
- `remove_liquidity` updates reserves and LP supply through subtraction

The generated metadata exposes input/output bindings, runtime requirements, CKB
runtime accesses, and scheduler shared-state domains.

## Builder Contract

The transaction builder must place consumed cells and proposed outputs at the
indexes declared by metadata. Production reports must retain:

- action name
- input and output indexes
- occupied-capacity measurement for proposed outputs
- serialized transaction size
- dry-run or VM execution evidence

If the builder cannot prove this mapping, the artifact is not production-ready
even if it compiles.
