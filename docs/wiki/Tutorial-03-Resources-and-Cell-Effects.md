CellScript is built around explicit Cell movement. An effect is not just a
helper call. It is a statement about the transaction you expect to validate:
which inputs are consumed, which outputs are proposed, which dependencies are
read, and which state transition is being proved.

If you come from account-style smart contracts, this is the chapter where the
mental model changes. In CellScript, persistent state does not quietly update in
place. A transaction spends Cells and creates new Cells.

## What You Will Learn

- how linear resources move through an action;
- why `create`, `consume`, `destroy`, and stdlib lifecycle patterns are explicit;
- how `action(before: T) -> after: T` expresses the verifier core for
  input-to-output transitions;
- how `create_unique` and `replace_unique` preserve declared identity;
- why v0.15 uses explicit destruction policy forms;
- why unsupported CKB runtime behavior should fail closed.

## The Main Effects

| Effect | Read it as |
|---|---|
| `input param: T` | Explicit consumed input Cell parameter. Equivalent to `param: T` for Cell-backed action parameters. |
| `-> output: T` | Named proposed output Cell binding. |
| `consume value` | Spend an input-backed linear value. |
| `create output = T { ... }` | Sugar for validating a typed proposed output Cell. |
| `read param: T` | Read dependency-backed state without consuming it. |
| `read_ref<T>()` | Read dependency-backed state from an expression. |
| `destroy value` | Consume a value without a successor output, if the type allows `destroy`. |
| `create_unique<T>(identity = policy) { ... }` | Create a typed output, anchor its declared identity, and report full uniqueness as runtime-required. |
| `replace_unique<T>(identity = policy) input { ... }` | Consume one input-backed value and create a replacement that preserves identity. |
| `destroy_singleton_type(value)` | Consume a singleton and prove no same-TypeHash output continues it. |
| `destroy_unique(value, identity = type_id)` | Consume a TYPE_ID-backed unique value without replacement. |
| `destroy_instance(value, identity_field = id)` | Consume one field-identified instance; executable same-field output exclusion is runtime-required. |
| `burn_amount(value, field = amount)` | Declare a quantity burn rather than output absence; executable delta proof is runtime-required. |
| `std::lifecycle::transfer(input, output, to) { ... }` | Expand to consume plus a locked output and explicit preservation checks. |
| `std::receipt::claim(receipt, output, to) { ... }` | Consume a receipt and materialize the claim output. |
| `std::lifecycle::settle(receipt, output, to) { ... }` | Finalize a receipt-backed process with an explicit output. |

The effects are deliberately visible. They make the source read like a
transaction plan instead of a hidden storage mutation. The core verifier form
can also name proposed Cells directly as action parameters; `consume` and
`create` remain convenient source syntax over that transaction evidence.

## Linear Values

Resources are linear. In plain terms: if an action receives a resource, the
action must say where it goes.

```cellscript
action burn(token: Token)
where
    assert(token.amount > 0, "cannot burn zero")
    burn_amount(token, field = amount)
```

The `Token` cannot simply disappear. It must be consumed, returned, destroyed,
validated as a named successor output, or handled by an explicit stdlib
lifecycle pattern. Silent loss is rejected because silent loss would make Cell
movement unclear.

## Flows Use Explicit State Fields

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
flow GrantFlow for VestingGrant.state {
    Granted -> Claimable by unlock_grant;
    Claimable -> FullyClaimed by claim_all;
}
```

Bind each action to the transition it is allowed to prove. The semantic core is
an input-to-output verifier signature: the left side names consumed input Cell
views, the right side names proposed output Cell bindings, and `transition`
names both state fields explicitly.

```cellscript
action unlock_grant(input: VestingGrant) -> output: VestingGrant
    transition input.state: Granted -> output.state: Claimable
where
    require input.beneficiary == output.beneficiary
    require input.total_amount == output.total_amount
    require input.claimed_amount == output.claimed_amount
```

`flow Type.field { ... }` is the compact form when the flow does not
need a separate name. The compiler keeps the state field explicit in Molecule
layout, lowers enum states to their ordinal values, verifies old/new state at
runtime, and rejects action `transition` clauses that are not declared in the state graph. A
state field may have only one flow declaration, so keep all legal edges for
that field in one named or compact flow block.

Output binding is deterministic. Named action outputs are bound to transaction
outputs in signature order, starting at `Output#0`. A field-to-field transition such as
`transition input.state: A -> output.state: B` names both the input and proposed output
directly. Existing `consume input` plus `create output = T { ... }` remains
accepted as front-end sugar for the same verifier shape.

Action proof logic is scoped by `where`. Put `transition` clauses before `where` and
keep proof obligations below it:

```cellscript
action fill_offer(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Filled
where
    require output.price == input.price
    require output.seller == input.seller
```

Inside `where`, conditional proof branches must constrain output fields
symmetrically. If one branch requires `output.claimable`, sibling branches must
also constrain `output.claimable` unless it was already constrained in the
surrounding proof scope.

Bare `destroy token` remains available. In `--primitive-strict=0.16` mode, it
keeps the 0.15 rule: the operation must be authorized by the `consume + burn`
kernel effects instead of legacy `has destroy`. Choose a policy-specific
destruction form when reviewers need to see whether the contract proves
singleton absence, TYPE_ID consumption, field-identified instance consumption,
or amount burn.

## Creating Output Cells

`create` describes typed output data and a corresponding Cell output. In the
verifier model this is sugar for selecting and checking a proposed transaction
output; the script still validates an existing transaction, it does not allocate
Cells inside CKB-VM.

```cellscript
create token = Token {
    amount,
    symbol: auth.token_symbol
} with_lock(to)
```

Persistent state enters the transaction output set only through explicit output
evidence: either a named action output or a `create output = T { ... }` sugar
expression. Local variables are just local variables. They do not become
on-chain storage unless they are tied to a proposed output Cell.

The `with_lock(to)` part matters. It says which lock will guard the newly
created Cell. If a later transaction wants to spend that Cell, the lock must
accept the spend.

## Consuming And Updating State

A common CellScript sugar pattern is:

1. read or consume an input Cell;
2. check the transition;
3. validate a proposed output Cell.

For example, a transfer consumes one token and validates a proposed token
under a different lock:

```cellscript
action transfer_token(token: Token, to: Address) -> next_token: Token
where
    consume token

    create next_token = Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
```

This is closer to CKB than an account-style assignment. The old Cell is spent;
the new Cell is a proposed output that the verifier checks.

## Identity-Aware Creation And Replacement

When a type declares an identity policy, use the identity-aware lifecycle forms
for creation and replacement:

```cellscript
resource NFT has store, create, replace
    identity(field(token_id))
{
    token_id: [u8; 32]
    owner: Address
}

action mint_nft(token_id: [u8; 32], owner: Address) -> NFT
where
    create_unique<NFT>(identity = field(token_id)) {
        token_id,
        owner
    } with_lock(owner)

action move_nft(nft_before: NFT, new_owner: Address) -> NFT
where
    replace_unique<NFT>(identity = field(token_id)) nft_before {
        token_id: nft_before.token_id,
        owner: new_owner
    }
```

`replace_unique<T>(identity = policy) input { ... }` always names the consumed
input before the replacement fields. The verifier then compares the relevant
identity evidence across input and output: fixed-width field bytes for
`field(...)`, LockHash for `script_args`, and TypeHash for `ckb_type_id` or
`singleton_type`.

For `create_unique`, 0.15 emits local runtime anchors for the created output.
The full global uniqueness proof is recorded as runtime-required and still
needs TYPE_ID builder-plan evidence or builder/indexer evidence; do not treat
compiler metadata alone as a chain-wide uniqueness proof.

## Explicit Destruction Policies

Use the destruction form that matches the proof you need:

```cellscript
destroy_singleton_type(config)
destroy_unique(asset, identity = type_id)
destroy_instance(nft, identity_field = token_id)
burn_amount(token, field = amount)
```

These forms are intentionally different. Destroying a singleton is an output
absence proof. Destroying a TYPE_ID value uses the same executable absence scan
for the identity continuation. Destroying an instance by field and burning an
amount are explicit runtime-required proof gaps; they are not lowered as
over-broad same-TypeHash absence claims.

## Updating Existing State

For one-to-one state updates, make both cells visible:

```cellscript
action mint_with_authority(auth_before: MintAuthority, to: Address, amount: u64) -> (auth_after: MintAuthority, token: Token)
where
    assert(auth_before.minted + amount <= auth_before.max_supply, "exceeds max supply")

    require auth_after.token_symbol == auth_before.token_symbol
    require auth_after.max_supply == auth_before.max_supply
    require auth_after.minted == auth_before.minted + amount

    create token = Token {
        amount,
        symbol: auth_before.token_symbol
    } with_lock(to)
```

This is intentionally explicit: `auth_before` is the existing state Cell,
`auth_after` is the proposed output, and the `require` guards prove
which fields may change. There is no hidden account-style mutation.

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
- a later claim action consumes the grant and explicitly creates its output;
- a settlement action consumes proof that a process completed and explicitly
  creates its output.

This makes intermediate protocol state explicit instead of hiding it in a
generic event log.

## CKB Profile Notes

The CKB profile is intentionally strict. If the compiler rejects a shape that
depends on unsupported runtime behavior, that is usually the correct outcome.

For CKB code, prefer:

- fixed persistent schemas;
- explicit action parameters;
- explicit locks for authorization boundaries;
- `--primitive-strict=0.16` syntax for new code;
- explicit capacity, witness, and dependency review;
- metadata-backed explanations for every runtime obligation.

Avoid assuming that a helper, syscall, or collection shape is supported just
because it is convenient. If the profile cannot lower it safely, it should fail
closed.

## Next

After you know how values move, continue with
[Action Model and 0.13 Syntax](https://github.com/a19q3/CellScript/wiki/Tutorial-09-Action-Model-and-0-13-Syntax)
for a deeper walkthrough of signature-direction actions, then use
[Cookbook Recipes](https://github.com/a19q3/CellScript/wiki/Cookbook-Recipes)
for small copyable patterns.
