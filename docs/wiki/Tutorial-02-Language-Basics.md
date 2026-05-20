CellScript source reads best when you treat it as a small Cell story. First you
name the module. Then you describe the state that can exist on chain. Finally
you write the actions and locks that say how that state may change or be spent.

This chapter is a map. It does not cover every syntax detail, but it gives you
the vocabulary you need before reading the bundled examples.

## A Source File At A Glance

A typical `.cell` file contains:

- one `module` declaration;
- persistent declarations such as `resource`, `shared`, and `receipt`;
- optional ordinary `struct`, `enum`, and `const` declarations;
- optional top-level `invariant` declarations;
- executable `action` entries;
- executable `lock` entries.

The first split to learn is simple:

- ordinary data helps you calculate;
- persistent declarations describe Cell-backed state;
- actions change state;
- locks guard spending.

## Current Syntax Checklist

The current public surface keeps transaction shape visible. These are the
syntax forms you will see in the examples:

| Syntax | Use it for |
|---|---|
| `module cellscript::name` | Stable module identity. |
| `use cellscript::path::{A, B}` | Grouped imports from another module. |
| `resource T has store, create, consume, replace, burn, relock` | Linear Cell-backed assets with explicit kernel-effect capabilities. |
| `shared T has store` | Shared Cell-backed state such as pools or registries. |
| `receipt T` | Settlement-style proof Cells. |
| `receipt T -> Output` | Claimable receipt Cells with a declared claim output type. |
| `with_default_hash_type(Data1)` | Default CKB hash type metadata for a persistent declaration. |
| `flow Name for T.state { A -> B by action; }` | Named state graph for one explicit state field. |
| `flow T.state { A -> B; }` | Compact state graph when a separate flow name is unnecessary. |
| `action(old: T) -> new: T` | Core input-to-output verifier signature. |
| `-> (left: T, right: Receipt)` | Multiple named proposed output Cell bindings. |
| `input x: T` | Explicit consumed input Cell qualifier when the default action side is not enough. |
| `read cfg: T` | Read-only CellDep-backed action input. |
| `protected cell: T` | Lock-guarded input Cell view (lock parameters only). |
| `witness arg: T` | Decoded witness data. |
| `lock_args args: T` | Typed bytes from the executing lock script's `Script.args` (lock parameters only). |
| `transition old.state: A -> new.state: B` | Explicit field-to-field state edge. |
| `transition { ... }` | Non-empty block form for multiple explicit state edges. |
| `create out = T { ... }` | Constraint on a named proposed output Cell. |
| `require condition, "message"` | Action or lock verifier guard with an optional message. |
| `assert(condition, "message")` | Internal checked assertion. |
| `let mut xs: Vec<Hash> = []` | Typed empty local `Vec<T>` literal. |

Names such as `old`, `new`, `input`, and `output` are ordinary bindings. The
semantics come from the action side, source qualifier, `transition`, `create`, and
`require` clauses. Do not use `&mut` on action-boundary Cell parameters; Cell
updates are expressed by naming the input and proposed output Cell.

## Module Declaration

Start with a stable module name:

```cellscript
module cellscript::demo
```

Bundled examples use the `cellscript::` namespace:

```cellscript
module cellscript::timelock
```

Module names are not decoration. They are part of source identity and appear in
metadata, so use names you are willing to keep stable.

## Scalar and Fixed Types

Common field and parameter types include:

```cellscript
u8
u16
u32
u64
u128
bool
Address
Hash
[u8; 8]
```

Use fixed-size byte arrays when a value must live in a predictable persistent
schema or CKB data layout.

### Expression-local Unsigned Widening

CellScript supports expression-local primitive unsigned integer widening for
arithmetic and numeric comparison:

```cellscript
let total: u64 = amount_u64 + fee_u16
let under_limit: bool = fee_u16 < amount_u64
```

The chain is `u8 -> u16 -> u32 -> u64 -> u128`, but the widening is local to
the expression being evaluated. It does not cross assignment, return, ABI,
witness, `create` layout, struct field initialization, or serialization
boundaries.

Integer literals may be context-typed by an expected primitive integer type:

```cellscript
let byte: u8 = 1
```

Non-literal numeric values must keep their actual width at boundaries:

```cellscript
let amount64: u64 = amount16        // rejected
let explicit: u64 = amount16 as u64 // accepted
```

Compound assignment is a write boundary. `target += rhs` is valid only when
`rhs` is the same width as, or narrower than, `target`. Generic `u128`
arithmetic and ordering remain unsupported except for explicitly implemented
`u128` delta or equality paths.

`Signature` is not a built-in scalar. If a contract needs to carry a signature,
model it explicitly:

```cellscript
struct Signature {
    signer: Address
    signature: [u8; 64]
}
```

That `signer` field is only data until a lock verifies it. Names do not create
authority.

For dynamic payloads that cross ABI or persistent schema boundaries, the
documented production surface includes targeted `Vec<u8>`, `Vec<Address>`,
`Vec<Hash>`, and concrete fixed-width struct-vector paths. Generic collection
ownership is intentionally narrower than "all collections are supported". Use
the collections support matrix before presenting a collection shape as
production-ready.

## Structs

Use `struct` for ordinary typed data that is not itself a persistent Cell:

```cellscript
struct Config {
    threshold: u64
}
```

A struct is a shape. It does not create on-chain storage by itself. A local
`Config` value is transaction-local unless you embed it in a `resource`,
`shared`, or `receipt`.

Struct literals and Cell `create` literals both support field shorthand when the
field name and local variable name match:

```cellscript
let config = Config { threshold }

create token = Token {
    amount,
    symbol
}
```

The shorthand is exactly `field: field`; it does not infer or rename fields.

## Typed Vec Literals

Use `[]` and `[x, y]` for local `Vec<T>` construction only where the expected
type is already known:

```cellscript
let mut keys: Vec<Hash> = []
let mut owners: Vec<Address> = [primary_owner, backup_owner]

create proposal = Proposal {
    data: [],
    signatures: []
}
```

These literals lower to the same bounded, stack-backed `Vec<T>` helpers as
`Vec::new()` plus pushes. Untyped `[]` remains rejected, and cell-backed
collection ownership remains outside the supported production surface.

## Resources

Use `resource` for linear Cell-backed assets. If your protocol should not be
able to duplicate or silently drop a value, it probably belongs in a resource.

```cellscript
resource Token has store, create, consume, replace, burn, relock {
    amount: u64
    symbol: [u8; 8]
}
```

Resources are linear values. When an action receives one, the action must say
where it goes: consume it, validate a proposed output, return it, destroy it,
or use an explicit stdlib lifecycle pattern such as
`std::lifecycle::transfer`, `std::receipt::claim`, or
`std::lifecycle::settle`.

Persistent declarations can also declare the default CKB script hash type used
for their type identity metadata:

```cellscript
#[type_id("cellscript::asset::Token:v1")]
resource Token has store
with_default_hash_type(Data1)
{
    amount: u64
    symbol: [u8; 8]
}
```

Supported spellings are `Data`, `Data1`, `Data2`, and `Type`. The lowercase CKB
forms are accepted too. Unknown hash types are compile errors, not deployment
warnings.

CellScript 0.15 resets `has ...` clauses from protocol verbs to kernel effects.
New strict-mode declarations should use capabilities such as `create`,
`consume`, `replace`, `burn`, `relock`, `retarget_type`, and `read_ref`.
The older `transfer` and `destroy` capability words are accepted only through
the `--primitive-compat=0.14` migration path; `--primitive-strict=0.15`
rejects them in type declarations.

## Identity Policies

A persistent declaration can name the identity policy that later lifecycle
forms must preserve:

```cellscript
resource NFT has store, create, replace {
    identity(field(token_id))
    token_id: [u8; 32]
    owner: Address
}

resource ScriptBoundToken has store, create, replace {
    identity(script_args)
    amount: u64
}

shared Config has store, replace {
    identity(singleton_type)
    value: u64
}
```

Supported policies are `ckb_type_id`, `field(name)`, `script_args`, and
`singleton_type`. Omitting the declaration is the default `identity none`.
Fields used for `identity(field(...))` must be fixed-width schema fields.

## Shared State

Use `shared` for contention-sensitive state such as pools, launch state, or
registries:

```cellscript
shared Pool has store {
    token_reserve: u64
    ckb_reserve: u64
}
```

Shared state tells tools and schedulers that multiple transactions may care
about the same Cell-backed value. Reads and writes remain visible in metadata.

## Receipts

Use `receipt` for single-use proof Cells. A receipt is useful when one action
creates a right and another action later consumes that right.

```cellscript
receipt VestingGrant has store {
    beneficiary: Address
    amount: u64
    unlock_epoch: u64
}
```

`has store` is optional for receipts; ephemeral records such as execution logs
or settlement proofs may omit it. Use a claim output arrow when a receipt has a
direct claim output type:

```cellscript
receipt ClaimTicket -> Token {
    amount: u64
    beneficiary: Address
}
```

Receipts are a good fit for deposits, vesting grants, voting records,
settlement proofs, and claim flows.

## Actions

Use `action` for type-script style transition logic. The semantic core is a
verifier over proposed transaction Cells: Cell-backed parameters on the left are
input Cell evidence, named outputs on the right are proposed output Cell
evidence, and `require` states the guard conditions that must pass.

For flow transitions, prefer the input-to-output signature form. Given
an `Offer.state` graph such as `Live -> Filled`, the action names both Cell
views:

```cellscript
action fill_offer(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Filled
where
    require input.price == output.price
    require input.seller == output.seller
```

The `transition` clause only proves the state edge. Authorization, preservation, and
conservation checks still belong in explicit `require` statements.

Consume/create-style actions remain valid as front-end sugar:

```cellscript
action transfer_token(token: Token, to: Address) -> next_token: Token
where
    assert(token.amount > 0, "empty token")
    consume token

    create next_token = Token {
        amount: token.amount,
        symbol: token.symbol
    } with_lock(to)
```

Read this as a Cell transition: spend one token input, then validate a proposed
token output under a new lock. The verifier checks a proposed
transaction; it does not allocate Cells inside CKB-VM.

## Scoped Invariants

CellScript 0.15 adds top-level invariant declarations. They are deliberately
explicit about the verifier trigger, the protected scope, and the CKB views
they read:

```cellscript
invariant token_conservation {
    trigger: type_group
    scope: group
    reads: group_inputs<Token>.amount, group_outputs<Token>.amount

    assert_sum(group_outputs<Token>.amount) <= assert_sum(group_inputs<Token>.amount)
}
```

Supported triggers are `explicit_entry`, `lock_group`, and `type_group`.
Supported scopes are `selected_cells`, `group`, and `transaction`.
Aggregate primitives such as `assert_sum`, `assert_conserved`,
`assert_delta`, `assert_distinct`, and `assert_singleton` are recorded in
ProofPlan metadata in 0.15; executable aggregate lowering is still a later
milestone.

## Locks

Use `lock` for CKB spend-boundary predicates. A lock should make its data
sources obvious:

- `protected` marks the typed input Cell guarded by this lock invocation;
- `witness` marks decoded transaction witness data;
- `require` marks a verifier guard that fails the current script validation.

```cellscript
shared Wallet has store {
    owner: Address
    nonce: u64
}

lock owner_only(protected wallet: Wallet, witness claimed_owner: Address) -> bool {
    require wallet.owner == claimed_owner
}
```

Locks return `bool`. `protected Wallet` means a typed view of one selected input
Cell in the current script group whose spend is guarded by this lock
invocation. It is not an output Cell, not a transaction-wide scan, and not all
same-type Cells unless the language explicitly adds such multiplicity syntax.

`witness Address` means decoded transaction witness data only. It is not a
signer or ownership proof.

## Lock Boundary Primitives

The lock-boundary keywords are meant to expose CKB's transaction model instead
of hiding it behind account-style authorization language.

| Primitive | Meaning in CellScript | CKB-facing interpretation |
|---|---|---|
| `protected T` | Typed view of the Cell state guarded by this lock invocation. | One selected input Cell in the current script group, not an output Cell and not a transaction-wide scan. |
| `witness T` | Typed value decoded from transaction witness data. | User-supplied witness bytes decoded by the entry ABI. It is not a signer proof. |
| `require expr` / `require expr, "message"` | Action or lock verifier guard. | If `expr` is false, the current script validation fails. The optional string message is kept for source readability and tooling. |
| `lock_args T` | Typed fixed-width value decoded from the executing script args. | CKB `Script.args` data for this lock invocation. It is not a signer proof. |

Use `require` for verifier guards inside actions and locks. Use
`assert` for ordinary internal sanity checks where the condition is not
part of the protocol boundary you want metadata and reviews to read as a guard.

This lock checks equality between protected Cell state and witness data:

```cellscript
lock owner_only(protected wallet: Wallet, witness claimed_owner: Address) -> bool {
    require wallet.owner == claimed_owner
}
```

That comparison may be useful, but it does not prove that `claimed_owner` signed
the transaction. A misleading parameter name does not make it safer:

```cellscript
// Unsafe as an authorization claim: `signer` is only a witness value here.
lock misleading(protected wallet: Wallet, witness signer: Address) -> bool {
    require wallet.owner == signer
}
```

Real CKB authorization needs explicit binding to script args, transaction digest
scope, witness layout, and signature verification. Script args can now be named
explicitly, but signature verification is still deliberately not implicit:

```cellscript
lock owner_boundary(
    wallet: protected Wallet,
    owner: lock_args Address,
    claimed_owner: witness Address
) -> bool {
    let input = source::group_input(0)
    let witness_lock = witness::lock(input)
    let digest = env::sighash_all(input)
    require wallet.owner == owner
    require claimed_owner == owner
    require witness_lock == digest
}
```

`lock_args Address` tells the reader where the owner value comes from. It still
does not prove a signature. `env::sighash_all(input)` makes the digest surface
visible, and `witness::lock(input)` makes the witness field visible, but the
example above is still a boundary-classification example. Until explicit
signature verification primitives are available, treat `Address`,
`lock_args Address`, and `witness Address` as data only.

`lock_args Address` is already bound to the executing lock script's typed
`Script.args` bytes. That makes it a stable script-argument value, but it still
does not verify a transaction signature unless the lock also calls an explicit
signature verification primitive.

## Assertions

Use `assert` for internal checked conditions:

```cellscript
assert(amount > 0, "amount must be positive")
```

Use `require` when the condition is a verifier guard on an action or lock
boundary. Use `assert` when you want an internal sanity assertion that still
fails closed but is not the boundary predicate readers should treat as
authorization.

## Comments

CellScript supports line comments and nested block comments:

```cellscript
// Explain Cell movement or security boundaries.

/*
   Block comments may contain nested /* inner */ comments.
*/
```

Use comments where they help the reader understand Cell movement, witness
scope, builder obligations, or a security boundary. Avoid comments that merely
repeat arithmetic.

## Next

With the source shape in mind, continue with
[Resources and Cell Effects](https://github.com/a19q3/CellScript/wiki/Tutorial-03-Resources-and-Cell-Effects). If a
CKB term is unclear, use the [CKB Glossary](https://github.com/a19q3/CellScript/wiki/CKB-Glossary).
