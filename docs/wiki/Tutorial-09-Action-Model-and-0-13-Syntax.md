# Tutorial 09: The 0.13 Action Model and Syntax

CellScript 0.13 makes one idea much more explicit:

```text
A transaction proposes a Cell transformation.
An action verifies whether that transformation is allowed.
```

This sounds small, but it changes how the language should be read. An action is
not a method call on a contract object. It is not an account storage update. It
is not a constructor that allocates new chain state at runtime.

An action is a typed verifier case. It names the input evidence a transaction
wants to spend or read, names the output evidence the transaction wants to
create, then proves the relationship between the two.

This tutorial explains the syntax introduced and tightened in the 0.13 surface:

- signature-direction actions: `action(old: T) -> new: T`;
- named output bindings: `-> (next: T, receipt: R)`;
- `where` proof blocks;
- singular state edges with colons: `transition old.state: A -> new.state: B`;
- prefix source qualifiers: `read`, `witness` (actions and locks), `protected` and `lock_args` (locks only);
- named output constraints: `create out = T { ... }`;
- explicit lifecycle verbs: `consume` and `destroy`.

The goal is not just prettier syntax. The goal is a source file that an auditor
can scan and understand as a CKB transaction shape.

## The Mental Model

On CKB, there is no global mutable contract object. A transaction consumes input
Cells and creates output Cells. Scripts verify whether that proposed change is
valid.

In earlier drafts, action outputs were sometimes expressed as object-update
language. 0.13 moves the transaction direction into the signature:

```cellscript
action fill_offer(before: Offer) -> after: Offer
where
    require after.price == before.price
```

Read it as:

```text
Spend one Offer input named before.
Validate one Offer output named after.
The proof obligations live under where.
```

The variable names are ordinary names. The direction comes from the action
signature.

## The Smallest Useful Action

Start with a token burn:

```cellscript
resource Token has store, consume, burn {
    amount: u64
    symbol: [u8; 8]
}

action burn(token: Token)
where
    require token.amount > 0
    destroy token
```

The left side parameter `token: Token` is Cell-backed, so in an action it is an
input Cell view by default. Since there is no output successor, the action must
say what happens to the input. Here it says `destroy token`.

This is intentional. No output does not silently mean "destroy". If the source
forgets to classify the consumed Cell, the verifier shape is unclear. 0.13
pushes authors toward explicit consumption intent.

## Signature Direction

The action signature has two sides:

```cellscript
action name(left_side_inputs...) -> right_side_outputs
where
    proof obligations
```

The left side contains input evidence and ordinary arguments. The right side
contains proposed output Cell bindings.

For example:

```cellscript
action mint(auth: MintAuthority, to: Address, amount: u64)
    -> (next_auth: MintAuthority, token: Token)
where
    require auth.minted + amount <= auth.max_supply

    require next_auth.token_symbol == auth.token_symbol
    require next_auth.max_supply == auth.max_supply
    require next_auth.minted == auth.minted + amount

    create token = Token {
        amount,
        symbol: auth.token_symbol
    } with_lock(to)
```

Read the signature first:

```text
auth: MintAuthority
    input Cell evidence

to: Address
amount: u64
    ordinary action arguments

next_auth: MintAuthority
token: Token
    proposed output Cell bindings
```

The signature does not by itself prove that `next_auth` is a valid successor of
`auth`. The `require` lines prove that. This is an important design choice:
continuity constraints should be visible line by line.

## Named Outputs

0.13 prefers named outputs:

```cellscript
action split(token: Token, to_a: Address, to_b: Address)
    -> (left: Token, right: Token)
where
    require token.amount > 1
    consume token

    create left = Token {
        amount: token.amount / 2,
        symbol: token.symbol
    } with_lock(to_a)

    create right = Token {
        amount: token.amount - token.amount / 2,
        symbol: token.symbol
    } with_lock(to_b)
```

Named outputs give the compiler and the reader a deterministic binding model.
The first named output corresponds to the first proposed output binding for this
action, the second named output to the second, and so on. The compiler does not
guess by variable names such as `new`, `after`, or `output`.

This is why the source says:

```cellscript
-> (left: Token, right: Token)
```

instead of returning an anonymous pair that the reader must decode by position.

## `create name = T { ... }`

The canonical 0.13 form for constraining an output is:

```cellscript
create token = Token {
    amount,
    symbol
} with_lock(owner)
```

This does not allocate a Cell at runtime. CKB-VM is not creating state out of
nothing. The transaction already proposes outputs. The verifier checks that the
named output binding has the right type, data, and lock.

So read:

```cellscript
create token = Token { ... } with_lock(owner)
```

as:

```text
The proposed output named token must be a Token with these fields and this lock.
```

Field shorthand is allowed when the field and local binding have the same name:

```cellscript
let amount = 100
let symbol = [67, 69, 76, 76, 0, 0, 0, 0]

create token = Token {
    amount,
    symbol
} with_lock(owner)
```

That is exactly the same as:

```cellscript
create token = Token {
    amount: amount,
    symbol: symbol
} with_lock(owner)
```

The shorthand never invents a field or renames a value.

## `where` Proof Blocks

Actions now use a structured `where` block:

```cellscript
action transfer_token(token: Token, to: Address) -> next_token: Token
where
    consume token

    create next_token = Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
```

The part above `where` describes the action boundary:

- what input evidence exists;
- what output evidence is proposed;
- what state edge is being declared, if any.

The part under `where` proves why the proposed transaction is valid:

- `require` constraints;
- `let` bindings;
- `if` / `else`;
- `match`;
- lifecycle effects;
- output construction constraints;
- helper calls.

This is different from ordinary function braces. Action proof logic is
deliberately scoped by `where` so the action reads like a verifier statement:

```text
This transformation is allowed where these constraints hold.
```

Ordinary helper functions still use braces:

```cellscript
fn min(a: u64, b: u64) -> u64 {
    if a < b {
        a
    } else {
        b
    }
}
```

`fn` is value-level helper code. It does not bind transaction inputs or outputs.

## State Is Data

0.13 does not hide lifecycle state in a compiler-injected field. If a protocol
has state, put the state field in the schema:

```cellscript
enum OfferState {
    Created,
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState
    seller: Address
    buyer: Address
    price: u64
}
```

That field is part of the Cell data layout. SDKs, indexers, explorers, tests,
and auditors can all see it.

The enum values name representation states. They do not define an order by
themselves. This declaration:

```cellscript
enum OfferState {
    Created,
    Live,
    Filled,
    Cancelled,
}
```

does not mean:

```text
Created -> Live -> Filled -> Cancelled
```

It only gives names to stored values. The transition graph is a separate
declaration.

## Flows Declare Allowed Edges

Use `flow` to declare which state edges exist:

```cellscript
flow OfferFlow for Offer.state {
    Created -> Live;
    Live -> Filled by fill_offer;
    Live -> Cancelled by cancel_offer;
}
```

There is also a compact form:

```cellscript
flow Offer.state {
    Created -> Live;
    Live -> Filled;
    Live -> Cancelled;
}
```

Rules to remember:

- one state field should have one flow declaration;
- keep all legal edges for that field in one place;
- `by action_name` optionally binds an edge to the action that is allowed to
  prove it;
- an action `transition` must match an edge declared by the flow;
- if a flow edge says `Live -> Filled by fill_offer`, then `fill_offer` must
  prove that exact edge, not just any edge on the same field.

This avoids a common audit problem: state edges scattered across unrelated
actions.

## `transition` Declares The State Edge

Use singular `transition`, with colons before state values:

```cellscript
action fill_offer(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Filled
where
    require output.price == input.price
    require output.seller == input.seller
```

Read the header as:

```text
input.state must be Live.
output.state must be Filled.
The edge Live -> Filled must be declared in Offer.state's flow.
```

The colon matters. It separates the field path from the required state value:

```cellscript
transition input.state: Live -> output.state: Filled
```

This is easier to scan than:

```cellscript
transition input.state Live -> output.state Filled
```

The latter is not the 0.13 syntax.

When an action declares more than one independent state edge, use a non-empty
block:

```cellscript
action settle(input: Offer, receipt: Receipt)
    -> (output: Offer, next_receipt: Receipt)
    transition {
        input.state: Live -> output.state: Filled
        receipt.state: Open -> next_receipt.state: Closed
    }
where
    require output.seller == input.seller
```

An empty `transition {}` block is rejected. It would claim a state-edge section
without naming any edge.

## `transition` Is Not Proof Logic

`transition` belongs between the action signature and `where`:

```cellscript
action settle(input: Position) -> output: Position
    transition input.phase: Open -> output.phase: Settled
where
    require output.owner == input.owner
```

Do not put it inside `where`:

```cellscript
action settle(input: Position) -> output: Position
where
    transition input.phase: Open -> output.phase: Settled
    require output.owner == input.owner
```

The reason is conceptual. A `transition` is an edge declaration. It tells the compiler
which state transition this action claims to prove. It should not hide inside
conditional proof logic.

If one input state can lead to two different output states, prefer separate
actions when the guards are different:

```cellscript
action fill_offer(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Filled
where
    require output.seller == input.seller
    require output.price == input.price

action cancel_offer(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Cancelled
where
    require output.seller == input.seller
    require output.price == input.price
```

This keeps the transition graph reviewable.

## Multi-Output Actions

Many real CKB transactions are not simple one-input, one-output updates. An AMM
swap may spend a pool Cell and an input token, then propose a new pool Cell and
an output token:

```cellscript
action swap_a_for_b(
    pool: Pool,
    token_in: Token,
    min_amount_out: u64,
    to: Address
) -> (next_pool: Pool, token_out: Token)
where
    require token_in.symbol == pool.token_a_symbol

    let fee = token_in.amount * pool.fee_rate_bps as u64 / 10000
    let net_in = token_in.amount - fee
    let amount_out = pool.reserve_b * net_in / (pool.reserve_a + net_in)

    require amount_out >= min_amount_out
    require amount_out < pool.reserve_b

    require next_pool.token_a_symbol == pool.token_a_symbol
    require next_pool.token_b_symbol == pool.token_b_symbol
    require next_pool.reserve_a == pool.reserve_a + token_in.amount
    require next_pool.reserve_b == pool.reserve_b - amount_out
    require next_pool.total_lp == pool.total_lp
    require next_pool.fee_rate_bps == pool.fee_rate_bps

    consume token_in

    create token_out = Token {
        amount: amount_out,
        symbol: pool.token_b_symbol
    } with_lock(to)
```

Notice what is not present: any implicit field preservation.
Continuity is expressed by explicit constraints:

```cellscript
require next_pool.reserve_a == pool.reserve_a + token_in.amount
require next_pool.reserve_b == pool.reserve_b - amount_out
```

This is more verbose, but it is also more auditable. The source says exactly
which fields are preserved and which fields change.

## Audit Visibility

Any verb that hides its own decomposition into consume + create + require
is not part of core. Core keeps only `consume` and `destroy` as input-fate
verbs. Every higher-level pattern must expand into these primitives with
visible constraints.

0.13 uses sharp primitives:

| Syntax | Responsibility |
|---|---|
| `action(input...) -> output...` | Transaction topology. |
| `transition input.state: A -> output.state: B` | State edge. |
| `require output.field == input.field` | Field preservation or accounting proof. |
| `create output = T { ... }` | Proposed output data and lock constraint. |
| `consume` / `destroy` | Consumption intent. |

If a continuity property matters, write it as a `require`.

## Source Qualifiers

Not every action parameter is a consumed input Cell. Some values come from
CellDeps, witnesses, protected lock context, or lock script args.

0.13 makes those sources prefix qualifiers:

```cellscript
action grant_vesting(
    read config: VestingConfig,
    tokens: Token,
    beneficiary: Address
) -> grant: VestingGrant
where
    require tokens.symbol == config.token_symbol
    require tokens.amount > 0

    consume tokens

    create grant = VestingGrant {
        beneficiary,
        total_amount: tokens.amount,
        claimed_amount: 0
    } with_lock(beneficiary)
```

The `read config: VestingConfig` parameter is read-only evidence. It is not
consumed. The `tokens: Token` parameter is a Cell-backed action input and must
be consumed or destroyed, or tied to an output successor through
proof constraints.

Common source qualifiers:

| Syntax | Meaning |
|---|---|
| `read config: T` | Read-only CellDep/reference-backed data. |
| `protected cell: T` | A lock-guarded Cell view (lock parameters only). |
| `witness sig: T` | Decoded witness data. |
| `lock_args args: T` | Typed bytes from the executing lock script's `Script.args` (lock parameters only). |

Expression-level `read_ref<T>()` still exists for lower-level reference reads.
At the action boundary, prefer the prefix form:

```cellscript
read config: VestingConfig
```

not:

```cellscript
config: read_ref VestingConfig
```

## Locks Keep Their Source Data Explicit

Locks still use braces because they are predicates, not action proof blocks:

```cellscript
lock vesting_admin(protected config: VestingConfig, witness claimed_admin: Address) -> bool {
    require claimed_admin == config.admin
}
```

Read this as:

```text
The lock receives a protected VestingConfig Cell view.
The claimed admin address comes from witness data.
The spend is valid only if the witness value matches the protected Cell data.
```

This is intentionally not inferred from parameter names.

## `fn` Has No Cell Source Semantics

Only action and lock boundaries talk about transaction sources. Ordinary helper
functions are value-level code:

```cellscript
fn is_vesting_admin(config: &VestingConfig, claimed_admin: Address) -> bool {
    claimed_admin == config.admin
}
```

The parameter `config: &VestingConfig` is a normal helper reference. It is not a
CellDep and it is not an input Cell. The action or lock that calls this helper
decides where the value came from:

```cellscript
lock vesting_admin(protected config: VestingConfig, witness claimed_admin: Address) -> bool {
    require is_vesting_admin(config, claimed_admin)
}
```

This separation matters. Borrow-like helper syntax is fine inside pure helper
code. It should not be used to describe action-boundary Cell replacement.

## What Replaced `&mut`

Do not model Cell updates as mutable references at the action boundary:

```cellscript
action mint(auth: &mut MintAuthority, amount: u64)
```

That looks like account storage mutation. CKB does not mutate an existing Cell
in place. The transaction spends one Cell and proposes another.

Use explicit input and output bindings:

```cellscript
action mint(auth_before: MintAuthority, amount: u64)
    -> auth_after: MintAuthority
where
    require auth_after.minted == auth_before.minted + amount
    require auth_after.max_supply == auth_before.max_supply
    require auth_after.token_symbol == auth_before.token_symbol
```

This is longer, but it has the right shape. The output is visible, and every
preserved field is explicit.

## Lifecycle Verbs

Cell-backed inputs must have a clear fate. 0.13 keeps core lifecycle verbs
visible:

| Verb | Use it when |
|---|---|
| `consume x` | The input is spent as ordinary protocol material. |
| `destroy x` | The object terminates, and the type has `destroy`. |

Example: ordinary consumption in a pool seed action:

```cellscript
action seed_pool(token_a: Token, token_b: Token, provider: Address)
    -> (pool: Pool, receipt: LPReceipt)
where
    require token_a.amount > 0
    require token_b.amount > 0

    consume token_a
    consume token_b

    create pool = Pool {
        token_a_symbol: token_a.symbol,
        token_b_symbol: token_b.symbol,
        reserve_a: token_a.amount,
        reserve_b: token_b.amount,
        total_lp: isqrt(token_a.amount * token_b.amount),
        fee_rate_bps: 30
    }

    create receipt = LPReceipt {
        pool_id: pool.type_hash(),
        lp_amount: pool.total_lp,
        provider
    } with_lock(provider)
```

Example: destruction:

```cellscript
resource Token has store, consume, burn {
    amount: u64
    symbol: [u8; 8]
}

action burn(token: Token)
where
    require token.amount > 0
    destroy token
```

Example: receipt consumption with explicit output constraints:

```cellscript
receipt VestingReceipt {
    amount: u64
    beneficiary: Address
    symbol: [u8; 8]
}

action redeem(receipt: VestingReceipt) -> token: Token
where
    consume receipt
    create token = Token {
        amount: receipt.amount,
        symbol: receipt.symbol
    } with_lock(receipt.beneficiary)
```

Receipts consumed with `consume` and explicit `create` output constraints give
full control over the output shape. The `claim` expression keyword has been
removed from core; receipt redemption now uses `consume` + `create` directly or
the explicit `std::receipt::claim` pattern when the receipt declares an output
type.

Higher-level transfer, claim, and settlement ergonomics live in stdlib patterns,
not in compiler-core expression verbs. Review their expansion as explicit
`consume` plus named output constraints.

## `require` Is The Atomic Proof Constraint

`transition` declares a state edge. It does not prove authorization, payment,
capacity, conservation, or field preservation.

Those belong in `require`:

```cellscript
action accept_offer(input: Offer, payment: Token)
    -> (output: Offer, seller_payment: Token)
    transition input.state: Live -> output.state: Filled
where
    require payment.amount == input.price
    require payment.symbol == input.payment_symbol

    require output.seller == input.seller
    require output.price == input.price
    require output.buyer == payment_owner(payment)

    consume payment

    create seller_payment = Token {
        amount: payment.amount,
        symbol: payment.symbol
    } with_lock(input.seller)
```

This is the intended style:

```text
Use transition for state.
Use require for proof.
Use lifecycle verbs for consumed inputs.
Use create for proposed outputs.
```

## Branches Inside `where`

Conditional proof logic is allowed, but it should be visually explicit:

```cellscript
action settle(input: Position) -> output: Position
    transition input.phase: Open -> output.phase: Settled
where
    let reward = dao_reward(input.deposit_header, current_header)

    if input.fast_exit {
        require output.claimable == input.claimable + reward - fast_fee
        require fast_fee <= reward / 10
    } else {
        require output.claimable == input.claimable + reward
    }

    require output.owner == input.owner
    require output.principal == input.principal
```

Branch boundaries matter in verifier code. If one branch constrains an output
field and the sibling branch does not, the compiler can reject that asymmetric
shape. This catches a common class of bugs: one proof path accidentally leaves a
proposed output underconstrained.

## A Full Walkthrough

Here is a small offer protocol using the 0.13 model.

First, define state as explicit data:

```cellscript
enum OfferState {
    Created,
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState
    seller: Address
    buyer: Address
    price: u64
    payment_symbol: [u8; 8]
}

resource Token has store, consume, burn {
    amount: u64
    symbol: [u8; 8]
}
```

Then declare the state graph:

```cellscript
flow Offer.state {
    Created -> Live;
    Live -> Filled;
    Live -> Cancelled;
}
```

Publish moves `Created` to `Live`:

```cellscript
action publish(input: Offer) -> output: Offer
    transition input.state: Created -> output.state: Live
where
    require output.seller == input.seller
    require output.buyer == input.buyer
    require output.price == input.price
    require output.payment_symbol == input.payment_symbol
```

Fill moves `Live` to `Filled`, consumes payment, and creates seller payment:

```cellscript
action fill(input: Offer, payment: Token, buyer: Address)
    -> (output: Offer, seller_payment: Token)
    transition input.state: Live -> output.state: Filled
where
    require payment.amount == input.price
    require payment.symbol == input.payment_symbol

    require output.seller == input.seller
    require output.buyer == buyer
    require output.price == input.price
    require output.payment_symbol == input.payment_symbol

    consume payment

    create seller_payment = Token {
        amount: payment.amount,
        symbol: payment.symbol
    } with_lock(input.seller)
```

Cancel moves `Live` to `Cancelled`:

```cellscript
action cancel(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Cancelled
where
    require output.seller == input.seller
    require output.buyer == input.buyer
    require output.price == input.price
    require output.payment_symbol == input.payment_symbol
```

The pattern is consistent:

1. The signature says which Cells are input evidence and which Cells are output
   evidence.
2. `transition` says which state edge is being proved.
3. `where` contains proof logic.
4. `require` states field preservation and accounting.
5. `consume` and `create` classify concrete Cell effects.

## Migration Guide From Older Drafts

If you have older CellScript examples, these are the important rewrites.

### Output Parameters Move To The Return Side

Old:

```cellscript
action fill(input: Offer, output: output Offer)
```

New:

```cellscript
action fill(input: Offer) -> output: Offer
```

For multiple outputs:

```cellscript
action mint(auth: MintAuthority, to: Address, amount: u64)
    -> (next_auth: MintAuthority, token: Token)
```

### Legacy `move` Became `transition`

Old:

```cellscript
moves input.state Live -> output.state Filled
move input.state: Live -> output.state: Filled
```

New:

```cellscript
transition input.state: Live -> output.state: Filled
```

The action-level `transition` form names the state edge directly. The colons
make field/value boundaries clear.

### Action Braces Became `where`

Old:

```cellscript
action fill(input: Offer) -> output: Offer
{
    require output.price == input.price
}
```

New:

```cellscript
action fill(input: Offer) -> output: Offer
where
    require output.price == input.price
```

Ordinary `fn` and `lock` declarations still use braces.

### `read_ref` Parameter Syntax Became `read`

Old action-boundary style:

```cellscript
action mint(config: read_ref Config, token: Token)
```

New:

```cellscript
action mint(read config: Config, token: Token)
```

Expression-level `read_ref<T>()` still exists when you need an explicit
expression.

### `&mut` Is Not The Action Update Model

Old mental model:

```cellscript
action mint(auth: &mut MintAuthority, amount: u64)
```

New Cell model:

```cellscript
action mint(auth_before: MintAuthority, amount: u64)
    -> auth_after: MintAuthority
where
    require auth_after.minted == auth_before.minted + amount
```

The new form makes the consumed Cell and proposed output Cell visible.

### Prefer `create name = ...`

Old local-binding style:

```cellscript
let token = create Token {
    amount,
    symbol
}
```

New named-output style:

```cellscript
action mint(...) -> token: Token
where
    create token = Token {
        amount,
        symbol
    }
```

The output binding comes from the action signature. The `create` statement
constrains that named proposed output.

## Style Checklist

When writing 0.13-style actions, use this checklist:

- Put Cell-backed inputs on the left side of the action signature.
- Put proposed output Cells on the right side as named outputs.
- Use `read` and `witness` as prefix source qualifiers in actions and locks.
- Use `protected` and `lock_args` as prefix source qualifiers in locks only.
- Keep ordinary scalar arguments as ordinary parameters.
- Put `transition` clauses before `where`.
- Use `transition field: From -> field: To`, with colons.
- Put proof logic under `where`.
- Use `require` for authorization, field preservation, accounting, and
  conservation checks.
- Use `create name = T { ... }` for named output constraints.
- Use `consume`, `destroy`, or explicit stdlib lifecycle patterns to classify
  input consumption.
- Do not use `&mut` for action-boundary Cell mutation.
- Do not rely on enum order as a transition graph; declare a `flow`.
- Do not hide state fields. State is data.

## The Short Version

The 0.13 action model can be summarized like this:

```text
Signature is transaction topology.
State is explicit data.
Flow declares legal state edges.
Transition binds an action to one edge.
Where scopes the proof.
Require states verifier constraints.
Create constrains proposed outputs.
Core lifecycle verbs and stdlib lifecycle patterns classify consumed inputs.
```

Or, even shorter:

```text
The action header says what changes.
The where block proves why it is allowed.
```
