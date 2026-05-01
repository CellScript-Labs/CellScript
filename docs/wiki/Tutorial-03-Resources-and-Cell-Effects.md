CellScript is built around explicit Cell movement. An effect is not just a
helper call. It is a statement about the transaction you expect to build: which
inputs are consumed, which outputs are created, which dependencies are read, and
which state transition is being proved.

If you come from account-style smart contracts, this is the chapter where the
mental model changes. In CellScript, persistent state does not quietly update in
place. A transaction spends Cells and creates new Cells.

## What You Will Learn

- how linear resources move through an action;
- why `create`, `consume`, `destroy`, `claim`, and `settle` are explicit;
- how `&mut` source syntax still maps to replacement-output style transitions;
- why unsupported CKB runtime behavior should fail closed.

## The Main Effects

| Effect | Read it as |
|---|---|
| `consume value` | Spend an input-backed linear value. |
| `create T { ... }` | Create a typed output Cell. |
| `read_ref T` | Read dependency-backed state without consuming it. |
| `transfer value to` | Move a value to a new lock or owner. |
| `destroy value` | Consume a value without replacement, if the type allows `destroy`. |
| `claim receipt` | Consume a receipt and materialize the claim path. |
| `settle receipt` | Finalize a receipt-backed process. |

The effects are deliberately visible. They make the source read like a
transaction plan instead of a hidden storage mutation.

## Linear Values

Resources are linear. In plain terms: if an action receives a resource, the
action must say where it goes.

```cellscript
action burn(token: Token) {
    assert_invariant(token.amount > 0, "cannot burn zero")
    destroy token
}
```

The `Token` cannot simply disappear. It must be consumed, returned, transferred,
claimed, settled, or destroyed. Silent loss is rejected because silent loss would
make the Cell lifecycle unclear.

## State Machines Use Explicit State Fields

State is ordinary schema data. Declare the state field yourself, usually as a
no-payload enum so SDKs, indexers, and explorers can decode the layout without
knowing compiler magic:

```cellscript
enum GrantState {
    Granted,
    Claimable,
    FullyClaimed,
}

receipt VestingGrant has store {
    state: GrantState,
    beneficiary: Address,
    total_amount: u64,
    claimed_amount: u64
}
```

Then declare the allowed transition graph separately:

```cellscript
state_machine GrantFlow for VestingGrant.state {
    Granted -> Claimable by unlock_grant;
    Claimable -> FullyClaimed by claim_all;
}
```

Bind each action to the transition it is allowed to prove. The semantic core is
an `action(input: T, output: T)` verifier form: `input` and `output` are
proposed transaction cells, and `moves` names both state fields explicitly.

```cellscript
action unlock_grant(input: VestingGrant, output: VestingGrant)
    moves input.state Granted -> output.state Claimable
{
    require input.beneficiary == output.beneficiary
    require input.total_amount == output.total_amount
    require input.claimed_amount == output.claimed_amount
}
```

`state Type.field { ... }` is the compact form when the state machine does not
need a separate name. The compiler keeps the state field explicit in Molecule
layout, lowers enum states to their ordinal values, verifies old/new state at
runtime, and rejects action moves that are not declared in the state graph.

Output binding is deterministic. Output parameters are bound to transaction
outputs in action parameter order among output parameters, starting at
`Output#0`. A `moves input.state A -> output.state B` target marks `output` as
an output binding; otherwise use `name: output T`. If a legacy
`moves input.state A -> B` action has output parameters, the compiler rejects it
instead of guessing which output is the replacement. Existing
`consume input` plus `create T { ... }` remains accepted as front-end sugar for
the same verifier shape.

## Creating Output Cells

`create` constructs typed output data and a corresponding Cell output. In the
verifier model this is sugar for selecting and checking a proposed transaction
output; the script still validates an existing transaction, it does not allocate
Cells inside CKB-VM.

```cellscript
create Token {
    amount,
    symbol: auth.token_symbol
} with_lock(to)
```

Persistent state is created only by explicit `create`. Local variables are just
local variables. They do not become on-chain storage unless they are placed into
a created Cell.

The `with_lock(to)` part matters. It says which lock will guard the newly
created Cell. If a later transaction wants to spend that Cell, the lock must
accept the spend.

## Consuming And Replacing State

A common CellScript pattern is:

1. read or consume an input Cell;
2. check the transition;
3. create a replacement output Cell.

For example, a transfer consumes one token and creates a replacement token under
a different lock:

```cellscript
action transfer_token(token: Token, to: Address) -> Token {
    consume token

    create Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
}
```

This is closer to CKB than an account-style assignment. The old Cell is spent;
the new Cell is created.

## Mutating Existing State

CellScript also supports mutable references for readable source code:

```cellscript
action mint(auth: &mut MintAuthority, to: Address, amount: u64) -> Token {
    assert_invariant(auth.minted + amount <= auth.max_supply, "exceeds max supply")
    auth.minted = auth.minted + amount

    create Token {
        amount,
        symbol: auth.token_symbol
    } with_lock(to)
}
```

The source says `auth.minted = ...`, but the CKB-facing model still needs an
input Cell and a replacement output Cell for `MintAuthority`. Metadata records
the runtime requirements and checked subconditions so reviewers can see that the
mutation is not pretending CKB has account storage.

When you read `&mut` in examples, translate it mentally as "this state must be
replaced consistently."

## Read-Only Dependencies

Some data is consulted but not spent: configuration, registry entries, reference
state, or dependency-backed protocol facts. Use read-only forms for that kind of
data.

On CKB, this usually maps to CellDep-style access in the target transaction
model. The compiler records read-only accesses so builders, schedulers, wallets,
and policy checks can decide which dependencies must be present.

## Receipts As Flow Control

Receipts are useful when a protocol needs a two-step or multi-step flow. One
action creates a right, and another action later consumes it.

For example:

- a vesting action creates a claimable grant;
- a later claim action consumes the grant;
- a settlement action consumes proof that a process completed.

This makes intermediate protocol state explicit instead of hiding it in a
generic event log.

## CKB Profile Notes

The CKB profile is intentionally strict. If the compiler rejects a shape that
depends on unsupported runtime behavior, that is usually the correct outcome.

For CKB code, prefer:

- fixed persistent schemas;
- explicit action parameters;
- explicit locks for authorization boundaries;
- explicit capacity, witness, and dependency review;
- metadata-backed explanations for every runtime obligation.

Avoid assuming that a helper, syscall, or collection shape is supported just
because it is convenient. If the profile cannot lower it safely, it should fail
closed.

## Next

After you know how values move, continue with
[Cookbook Recipes](https://github.com/tsukifune-kosei/CellScript/wiki/Cookbook-Recipes) for small copyable patterns, then move
on to [Packages and CLI Workflow](https://github.com/tsukifune-kosei/CellScript/wiki/Tutorial-04-Packages-and-CLI-Workflow).
