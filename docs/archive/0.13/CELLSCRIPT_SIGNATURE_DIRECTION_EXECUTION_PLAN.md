# CellScript Signature-Direction Execution Plan

**Status**: archived 0.13 execution plan. The canonical action/input/output
surface it describes has shipped; use this document as historical design and
migration context, not as the current release contract.

This document consolidates the signature-direction design into an actionable
compiler, docs, and example-migration plan. It intentionally keeps the CKB
transaction model visible:

```text
transaction proposes cell transformations;
CellScript verifier actions prove whether those transformations are valid.
```

It is not an object-mutation model, a workflow engine, or a runtime allocation
model.

## Design Decision

Adopt the signature-direction surface as the canonical action model:

```cellscript
action fill_offer(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Filled
where
    require output.price == input.price
    require output.seller == input.seller
```

Meaning:

| Surface | Semantics |
|---|---|
| `input: Offer` | consumed input cell view |
| `-> output: Offer` | proposed output cell binding |
| `transition input.state: Live -> output.state: Filled` | explicit state edge constraint |
| `where` | proof block |
| `require` | verifier constraint |

The signature defines the input/output topology. `transition` defines state edges.
Lifecycle verbs define how consumed inputs are used. Source qualifiers define
non-default data sources.

## Canonical Syntax

### Action

`action` is the transaction-shaped verifier entrypoint.

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
        symbol: auth.token_symbol,
    } with_lock(to)
```

Rules:

- resource-like action parameters are input cell bindings by default;
- named action returns are output cell bindings by default;
- scalar parameters remain ordinary entry arguments;
- proof logic lives under `where`;
- action body braces are not the canonical action scope form.

### Source Qualifiers

Use prefix source qualifiers at action and lock boundaries:

```cellscript
read config: VestingConfig
witness sig: Signature
protected wallet: MultisigWallet
lock_args owner: OwnerArgs
```

Accepted direction:

| Source | Meaning |
|---|---|
| default resource parameter | consumed input cell |
| `read x: T` | read-only referenced cell |
| `protected x: T` | lock-protected current/group input view |
| `witness x: T` | decoded witness value |
| `lock_args x: T` | decoded current script args |

Do not use action-boundary `&T` or `&mut T` as the public cell-source model.
`fn` helpers may keep ordinary reference syntax because `fn` does not define
transaction sources.

### Function Boundary

Only `action` and `lock` signatures carry transaction source semantics.

```cellscript
fn fee_from(token: Token) -> u64 {
    token.amount / 100
}
```

Here `token` is an ordinary value parameter. It is not an input cell and is not
consumed by the transaction.

### Lock

Lock parameters should make their source explicit:

```cellscript
lock owner_only(lock_args owner: OwnerArgs, witness sig: Signature) -> bool {
    require verify_sig(owner.pubkey_hash, sig)
}
```

`protected` remains valid for guarded input views:

```cellscript
lock vesting_admin(protected config: VestingConfig, witness claimed_admin: Address) -> bool {
    require claimed_admin == config.admin
}
```

The canonical lock form keeps `-> bool` for now.

### State Flow

State is explicit schema data. The compiler must not inject hidden state fields
or mutate Molecule layout.

```cellscript
enum OfferState {
    Created,
    Live,
    Filled,
    Cancelled,
}

resource Offer has store {
    state: OfferState,
    amount: u64,
}

flow OfferFlow for Offer.state {
    Created -> Live;
    Live -> Filled by fill_offer;
    Live -> Cancelled;
}
```

Rules:

- one state field has one flow declaration;
- compact `flow Type.field { ... }` is an anonymous flow;
- `by action` must match the exact action edge;
- enum order is ABI-sensitive until explicit discriminants are supported.

### State Transition

Use `transition`, not legacy `move` or `moves`.

```cellscript
transition input.state: Live -> output.state: Filled
```

Multiple explicit edges use a block:

```cellscript
transition {
    input.state: Live -> output.state: Filled
    receipt.state: Open -> next_receipt.state: Closed
}
```

The block form must contain at least one edge.

Rules:

- the source and target state values must be preceded by `:`;
- `transition` appears after the action signature and before `where`;
- `transition` is not proof logic and is not allowed inside `where`, `if`, or `match`;
- the full field-to-field form is canonical.

Do not add short `transition input.state: Live -> Filled` until successor-output
resolution is formally specified.

### Named Output Creation

`create name = T { ... }` constrains a named proposed output binding. It does
not allocate a runtime object.

```cellscript
action grant_vesting(read config: VestingConfig, tokens: Token, beneficiary: Address)
    -> grant: VestingGrant
where
    require tokens.symbol == config.token_symbol
    require tokens.amount > 0

    consume tokens

    create grant = VestingGrant {
        state: Granted,
        beneficiary,
        total_amount: tokens.amount,
        claimed_amount: 0,
        token_symbol: config.token_symbol,
    } with_lock(beneficiary)
```

Strict mode should reject `create extra = ...` when `extra` is not declared in
the action return list.

### Lifecycle Verbs

Consumed inputs must reach an explicit lifecycle or output-binding role.

| Verb | Meaning |
|---|---|
| `consume x` | ordinary protocol consumption or value transformation |
| `destroy x` | terminal resource destruction |
| `transition x.state: A -> y.state: B` | state successor relation |

Do not infer destruction from silence:

```cellscript
action burn(token: Token)
where
    require token.amount > 0
    destroy token
```

No output does not mean destroy. If an input resource has no successor output,
the lifecycle verb must say why it is consumed.

## Adopted Now

These decisions are canonical and should be reflected across parser, typecheck,
lowering, formatter, LSP, examples, docs, and tests:

- signature direction defines action input/output topology;
- named returns define proposed output cells;
- `where` is the action proof block;
- `transition` with colon syntax defines state edges;
- `flow` is the public state topology primitive;
- source qualifiers are prefix forms: `read`, `witness`, `protected`,
  `lock_args`;
- `create name = T { ... }` is the canonical named-output constraint;
- `consume` and `destroy` remain explicit lifecycle verbs;
  `claim`, `settle`, and `transfer` expression keywords have been removed from
  core (see syntax governance document);
- ordinary `fn` parameters have no cell source semantics;
- action-boundary `&mut` is not part of the public cell transformation model;
- continuity is expressed by `require` constraints and state `transition`, not by a
  separate lineage keyword.

## Rejected

These forms should not be kept as compatibility surface:

| Rejected form | Reason |
|---|---|
| `move old.state: Live -> new.state: Filled` | legacy spelling is rejected; use `transition` |
| plural `moves` | legacy spelling; current surface uses `transition` |
| `transition old.state Live -> new.state Filled` | state values need `:` for readability and parser clarity |
| action body braces as canonical action proof scope | `where` better separates transition declarations from proof obligations |
| `&mut T` at action boundary | implies in-place mutation instead of input/output cell transformation |
| `x: read_ref T` | source qualifier belongs before the binding: `read x: T` |
| silent destroy | terminal consumption must be explicit |
| anonymous extra outputs in strict mode | output topology should be visible in the signature |

## Deferred

These are reasonable, but should be separate implementation tracks rather than
0.13 correctness blockers:

| Item | Rationale |
|---|---|
| `transfer token { ... } with_lock(to)` | removed; use `consume` + `create` with explicit field mapping instead |
| `create_each` | useful batch-create sugar; needs static expansion and output obligation checks |
| short `transition input.state: A -> B` | requires deterministic successor inference in multi-output actions |
| `Option<T>` | useful for lookup/membership, but affects typecheck, ABI, pattern matching, and lowering |
| `Result<T, E>` | verifier failure is normally transaction rejection; recoverable errors need a stricter design |
| explicit enum discriminants | important for ABI stability; needs parser/typecheck/metadata/formatter support |
| richer `match` patterns | useful for payload enums; defer nested patterns, guards, and or-patterns |
| `require cond else ERR` | valuable for failure evidence; needs runtime error/metadata integration |
| doc comments in generated docs and metadata | valuable for audit output; can be implemented independently |
| CKB metadata methods such as `capacity()`, `lock_hash()`, `type_hash()` | important CKB surface; should be standardized as method-style helpers |

## Strict Mode Rules

Strict mode should reject:

- input resource with no lifecycle verb and no output successor;
- `transition` edge not declared in the relevant `flow`;
- `flow ... by action` that does not exactly match the action's explicit
  `transition` edge;
- `claim` on a non-claimable value;
- `destroy` without a destroy capability;
- lifecycle verbs applied to `read`, `witness`, `protected`, or `lock_args`
  values;
- lifecycle verbs inside ordinary `fn`;
- `create name = ...` where `name` is not a declared action output;
- output field constraints that are present in one branch but not all sibling
  branches unless dominated by an earlier constraint.

## Example Migration Targets

### Token Mint

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
        symbol: auth.token_symbol,
    } with_lock(to)
```

### Token Merge

```cellscript
action merge(a: Token, b: Token, to: Address) -> merged: Token
where
    require a.symbol == b.symbol

    consume a
    consume b

    create merged = Token {
        amount: a.amount + b.amount,
        symbol: a.symbol,
    } with_lock(to)
```

### AMM Swap

```cellscript
action swap_a_for_b(pool: Pool, token_in: Token, min_amount_out: u64, to: Address)
    -> (next_pool: Pool, token_out: Token)
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
        symbol: pool.token_b_symbol,
    } with_lock(to)
```

### Vesting Claim

Use multiple explicit `transition` clauses only when the action really permits
multiple declared state edges. Otherwise split the action into separate
transition-specific actions.

```cellscript
action claim_vested(grant: VestingGrant)
    -> (tokens: Token, next_grant: VestingGrant)
    transition grant.state: Claimable -> next_grant.state: FullyClaimed
where
    consume grant

    create tokens = Token {
        amount: next_grant.claimed_amount - grant.claimed_amount,
        symbol: grant.token_symbol,
    } with_lock(grant.beneficiary)

    require next_grant.beneficiary == grant.beneficiary
    require next_grant.total_amount == grant.total_amount
    require next_grant.token_symbol == grant.token_symbol
```

If partial and final claims share most proof logic, prefer helper functions for
shared arithmetic and separate actions for distinct state edges.

## Implementation Tracks

### Parser And Formatter

- enforce `where` action proof blocks;
- enforce `transition`;
- require `:` before both state values;
- parse named returns as output bindings;
- parse prefix source qualifiers;
- parse `create name = T { ... }`;
- keep formatter output semicolon-light and canonical.

### Typecheck

- classify action resource parameters as inputs;
- classify named action returns as outputs;
- keep scalar parameters ordinary;
- ensure `fn` has no cell source semantics;
- reject lifecycle effects on read-only sources;
- validate `transition` edges against `flow`;
- validate `by action` against exact action edge;
- enforce lifecycle classification for consumed inputs;
- enforce named output creation.

### Lowering And Metadata

- lower action parameters to input bindings;
- lower named returns to output bindings;
- lower `create name = ...` to output-field constraints;
- record lifecycle effects explicitly;
- record state edge constraints with field names and source/target bindings;
- expose read/protected/witness/lock_args source reads in metadata;
- keep proof obligations branch-aware.

### Examples And Tutorials

- update all examples to named output bindings;
- avoid local variable name `output` when it means an amount;
- remove action-boundary `&T` and `&mut T`;
- use `read x: T` for CellDep/reference inputs;
- document `fn` as value-only helper boundary;
- show `flow`, `transition`, lifecycle verbs, and `where` together in one canonical
  example;
- keep deferred ergonomics in roadmap, not tutorial mainline.

## Final Surface Summary

```text
State is data.
Signature is topology.
Transition is a state edge.
Lifecycle verbs classify consumption.
Where is proof.
Require is the atomic verifier constraint.
```
