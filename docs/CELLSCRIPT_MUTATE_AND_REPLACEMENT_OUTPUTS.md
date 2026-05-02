# CellScript Replacement Outputs

**Status**: production semantics for the current CellScript CKB profile.

CellScript models persistent state as Cell transformations, not in-place object
mutation. The canonical one-to-one replacement form is:

```cellscript
action update(before: State) -> after: State
    replace before -> after
{
    require after.owner == before.owner
    require after.counter == before.counter + 1
}
```

Read this as:

```text
Input#N before  ->  Output#M after
```

`before` is consumed transaction evidence. `after` is a proposed output Cell.
`replace before -> after` declares the deterministic lineage relationship between
the two. Field preservation, arithmetic transitions, authorization, capacity,
and asset-conservation rules remain explicit `require` or verifier checks.

## Required Checks

For each explicit replacement, generated metadata records:

- input cell data binding
- output cell data binding
- scheduler-visible input/output access for shared state
- field reads needed by `require` and `move`
- declared state transition edges from `flow`/`move`

The compiler does not inject a hidden state field, does not mutate Molecule
layout, and does not infer which output should replace which input. If a state
move crosses two variables, it must have a matching replacement clause:

```cellscript
move before.state Live -> after.state Filled
```

requires:

```cellscript
replace before -> after
```

## Transition Shapes

Current production transition checks are ordinary source requirements:

| Shape | Source form |
|---|---|
| Preserve | `require after.owner == before.owner` |
| Set | `require after.owner == new_owner` |
| Add | `require after.balance == before.balance + delta` |
| Sub | `require after.balance == before.balance - delta` |
| State edge | `move before.state A -> after.state B` |

Unsupported runtime shapes must remain fail-closed and must use a registered
runtime error code.

## AMM Pool Example

`examples/amm_pool.cell` is the canonical advanced replacement example:

- `swap_a_for_b` replaces pool reserves through explicit add/sub requirements
- `add_liquidity` replaces reserves and LP supply through proportional updates
- `remove_liquidity` replaces reserves and LP supply through subtraction

The generated metadata exposes the replacement input/output bindings, runtime
requirements, CKB runtime accesses, and scheduler shared-state domains.

## Builder Contract

The transaction builder must place consumed cells and proposed replacement
outputs at the indexes declared by metadata. Production reports must retain:

- action name
- input and output indexes
- occupied-capacity measurement for the replacement output
- serialized transaction size
- dry-run or VM execution evidence

If the builder cannot prove this mapping, the artifact is not production-ready
even if it compiles.
