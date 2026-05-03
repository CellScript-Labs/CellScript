# CellScript Syntax Governance: Core, Sugar, and Stdlib

> **0.13 core should be like double-entry bookkeeping's base entries,
> not a business process dictionary.**

---

## 1. Purpose

This document defines the formal syntax-governance policy for CellScript.
It classifies every language feature into one of four layers, specifies
what may enter each layer, and declares which features should be removed
from core or avoided entirely.

The goal is to keep the verifier calculus small, stable, and audit-visible,
while allowing ergonomics to grow through local sugar and standard-library
patterns that remain mechanically expandable.

---

## 2. Design Principle

> **Reduce ceremony, not safety visibility.**

A syntax addition is acceptable only if:

1. its desugared form is obvious;
2. all security-relevant fields or expressions remain visible at the action site;
3. it does not introduce remote policy lookup;
4. it does not hide what the transaction does;
5. audit tooling can expand it back into canonical core CellScript.

This keeps the language aligned with the 0.13 philosophy:

```text
The action header says what changes.
The where block proves why it is allowed.
```

---

## 3. The Four-Layer Model

```text
┌──────────────────────────────────────────────────────────┐
│ Layer 4: Avoided                                            │
│   policy primitives, implicit magic                        │
├──────────────────────────────────────────────────────────┤
│ Layer 3: Standard-Library Patterns                       │
│   claim, settle, transfer, conserve, cell metadata       │
├──────────────────────────────────────────────────────────┤
│ Layer 2: Local Explicit Sugar                            │
│   preserve, anonymous require block                      │
├──────────────────────────────────────────────────────────┤
│ Layer 1: Core Verifier Syntax                            │
│   action, flow, move, where, require, create,            │
│   consume, destroy, read/protected/witness/lock_args     │
└──────────────────────────────────────────────────────────┘
```

---

## 4. Layer 1: Core Verifier Syntax

Core syntax is small, stable, and directly tied to the Cell verifier model.

### 4.1 Canonical Core Vocabulary

```text
action          — declare a typed verifier case
flow            — declare legal state transitions
move            — declare a state edge
where           — scope proof obligations
require expr    — state a verifier constraint
create          — constrain a proposed output
consume         — declare input is used by the protocol
destroy         — declare input is intentionally terminated
read_ref        — declare a Cell dependency read
protected       — source qualifier: entry lock script
witness         — source qualifier: transaction witness
lock_args       — source qualifier: lock script arguments
```

### 4.2 Why Only `consume` and `destroy` for Input Fate

The core language should only classify input fate at the lowest level:

```text
consume  — "this input has a successor; its consequences are stated explicitly"
destroy  — "this input has no successor and is intentionally terminated"
```

These two are the irreducible input-fate verbs. Every higher-level
lifecycle concept (claim, settle, transfer) expands into `consume` +
`create` + `require` at the core level.

### 4.3 What Is NOT Core

The following are **not** core verifier syntax and should be removed
or deprecated from the core language:

| Keyword | Current Status | Governance Decision | Rationale |
|---------|---------------|---------------------|-----------|
| `claim` | Core AST node | **Remove from core** | Semantics depend on protocol context (vesting claim ≠ DAO claim ≠ bridge withdrawal). A universal verb that means different things in different contexts violates the verifier model. |
| `settle` | Core AST node | **Remove from core** | Too abstract. "settle order", "settle receipt", "settle channel" are all different operations. In core it becomes a beautiful but ambiguous word. |
| `transfer` | Core AST node + keyword | **Remove from core** | Hides audit-critical questions: preserve data? preserve type? preserve capacity? change lock? output binding name? A lock-change verb that hides its own decomposition is not core. |

---

## 5. Deprecation Analysis

### 5.1 `claim`

**Former usage in codebase**: `ClaimExpr` used to exist in AST/IR/types/codegen.
It has been removed from the executable core expression surface.

**Problem**: `claim` sounds like a universal lifecycle verb, but actual claim
semantics are highly protocol-dependent:

```text
vesting claim        → consume receipt + create token
DAO claim            → consume proposal + execute effect
receipt redemption   → consume receipt + create output
bridge withdrawal    → consume proof + create asset
order settlement     → consume order + create payout
LP withdrawal        → consume LP token + create assets
```

These are not the same underlying action.

**Canonical core equivalent**:

```cellscript
consume receipt

create token = Token {
    amount: receipt.amount,
    symbol: receipt.symbol
} with_lock(receipt.beneficiary)
```

**Stdlib home**: `std::receipt::claim`:

```cellscript
receipt VestingReceipt -> Token {
    amount: u64
    symbol: Symbol
    beneficiary: Address
}

std::receipt::claim(receipt, token, receipt.beneficiary) {
    amount
    symbol
}
```

**Deprecation path**:

```text
DONE (0.13.1): Removed claim keyword from compiler immediately.
                   AST, lexer, parser, IR, types, codegen, flow, optimize,
                   simulate, formatter — all claim expression paths removed.
DONE (0.13.1): Introduced std::receipt::claim as an explicit stdlib pattern
                   for receipt consumption plus canonical named output
                   construction from a declared receipt output type.
```

---

### 5.2 `settle`

**Former usage in codebase**: `SettleExpr` used to exist in AST/IR/types/codegen.
It has been removed from the executable core expression surface.

**Problem**: `settle` is even more abstract than `claim`.
"Settle order", "settle receipt", "settle channel", "settle DAO position"
are semantically different operations that share only a colloquial label.
In a core verifier language, an abstract but ambiguous word is worse
than an explicit decomposition.

**Canonical core equivalent**:

```cellscript
consume order

create receipt = Receipt { order_id: order.id, ... }
create payout = Token { amount: order.amount, ... }
require payout.amount == order.amount
```

**Stdlib home**: `std::lifecycle::settle` as an explicit lifecycle helper:

```cellscript
std::lifecycle::settle(order, payout, order.seller) {
    amount
    symbol
}
```

**Deprecation path**:

```text
DONE (0.13.1): Removed settle keyword from compiler immediately.
                   AST, lexer, parser, IR, types, codegen, flow, optimize,
                   simulate, formatter — all settle expression paths removed.
DONE (0.13.1): Introduced std::lifecycle::settle as an explicit stdlib pattern
                   for input consumption plus canonical named output
                   construction from explicit output and lock arguments.
```

---

### 5.3 `transfer`

**Former usage in codebase**: `TransferExpr` used to exist in AST/IR/types/codegen.
Examples now use explicit `consume + create with_lock` or compiler-recognized
stdlib lifecycle patterns.

**Problem**: `transfer x to y` sounds simple, but hides audit-critical
decisions:

```text
Which data fields are preserved?
Is type_hash preserved?
Is lock_hash preserved?
Is capacity preserved?
What is the output binding name?
Is a new Cell created?
Is the old Cell consumed?
```

Any verb that hides its own decomposition into consume + create + require
is not core.

**Canonical core equivalent** (already used in token.cell):

```cellscript
consume token

create next_token = Token {
    amount: token.amount,
    symbol: token.symbol
} with_lock(to)
```

Or with 0.13.1 preserve sugar:

```cellscript
consume token

create next_token = Token {
    amount: token.amount,
    symbol: token.symbol
} with_lock(to)

preserve next_token from token {
    amount
    symbol
}
```

**Stdlib home**: `std::lifecycle::transfer` as a protocol-pattern helper
with explicit preservation whitelist.

**Deprecation path**:

```text
DONE (0.13.1): Removed transfer keyword from compiler immediately.
                   AST, lexer, parser, IR, types, codegen, flow, optimize,
                   simulate, formatter — all transfer expression paths removed.
                   Capability::Transfer retained for resource declarations.
DONE (0.13.1): Introduced std::lifecycle::transfer as stdlib pattern
                   with named input/output bindings, destination lock, and
                   explicit preserve list parameter.
```

**Impact on Capability system**: `Capability::Transfer` on resource declarations
should also be reviewed. A resource that allows lock-change can be expressed
without a dedicated `Transfer` capability — the lock script itself enforces
transfer semantics. This is a separate decision; the `transfer` keyword
deprecation does not require immediate `Capability::Transfer` removal.

---

## 6. Layer 2: Local Explicit Sugar

Local sugar may enter the language if it keeps all safety facts visible
and is mechanically expandable into core syntax.

### 6.1 0.13.1 Additions

```cellscript
preserve output from input {
    field1
    field2
}
```

Expands to:

```cellscript
require output.field1 == input.field1
require output.field2 == input.field2
```

And:

```cellscript
require {
    expr1
    expr2
}
```

Expands to:

```cellscript
require expr1
require expr2
```

### 6.2 Acceptance Criteria for Local Sugar

| Criterion | preserve | require block |
|-----------|----------|---------------|
| Desugared form is obvious | ✅ | ✅ |
| Safety facts remain visible | ✅ | ✅ |
| No remote policy lookup | ✅ | ✅ |
| Audit-visible decomposition | ✅ | ✅ |
| Mechanically expandable | ✅ | ✅ |

### 6.3 Future Local Sugar Candidates

These are **candidates only** — not accepted. Each must pass the same
criteria before inclusion:

```text
single-field create shorthand    (requires design)
single-expression if guard       (requires design)
match-require pattern             (requires design)
```

---

## 7. Layer 3: Standard-Library Patterns

Advanced protocol concepts should not enter core syntax. They should
first live in standard-library categories where they remain explicitly
expandable into core CellScript.

### 7.1 Proposed Stdlib Namespaces

```text
std::cell           — Cell metadata helpers
std::accounting     — Capacity / value conservation
std::receipt         — Receipt claim / redemption
std::lifecycle       — Transfer / settle / higher-order patterns
std::ckb            — CKB-specific protocol operations
```

### 7.2 Candidate Stdlib Primitives

| Primitive | Namespace | Core Expansion | Current Status |
|-----------|-----------|----------------|----------------|
| `same_type` | `std::cell` | `require output.type_hash == input.type_hash` | Implemented |
| `conserved` | `std::accounting` | `require output.amount == input.amount` | Implemented |
| `transfer` | `std::lifecycle` | `consume input + create output with_lock(to) + preserve full output field set` | Implemented for named input/output bindings |
| `claim` | `std::receipt` | `consume receipt + create declared output with explicit lock + preserve full output field set` | Implemented for receipts declaring `-> OutputType` |
| `settle` | `std::lifecycle` | `consume input + create explicit locked output + preserve full output field set` | Implemented for named input/output bindings |
| `same_lock` | `std::cell` | `require output.lock_hash == input.lock_hash` | Implemented via canonical cell metadata verifier check |
| `preserve_lock` | `std::cell` | `require output.lock_hash == input.lock_hash` | Implemented via canonical cell metadata verifier check |
| `preserve_type` | `std::cell` | `require output.type_hash == input.type_hash` | Implemented |
| `preserve_capacity` | `std::cell` | `require output.capacity == input.capacity` | Implemented via canonical cell metadata verifier check |

### 7.3 Rule

Every stdlib primitive **must** have a canonical expansion into core CellScript.
If a primitive cannot be expanded, it does not belong in the stdlib — it
needs a protocol-specific implementation, not a language feature.

---

## 8. Layer 4: Avoided / Prohibited Features

The following should remain outside CellScript entirely:

```text
general policy primitive       — hides verifier obligations
named reusable require blocks  — hides audit decomposition
preserve all                   — blacklist preservation is unsafe
preserve except                — blacklist preservation is unsafe
implicit transfer magic        — hides lock/capacity/data decisions
general settle magic           — hides consume/create/require decomposition
capacity conservation sugar    — hides accounting invariants
cross-module proof policy      — breaks locality guarantee
```

### 8.1 Why These Are Prohibited

These features share one or more of these properties:

1. **Hidden obligations**: They make audit paths longer by hiding
   what is actually being verified.
2. **Ambiguous expansion**: They cannot be expanded into a single
   canonical core form — the expansion depends on context.
3. **Policy confusion**: They blur the line between verifier logic
   and business logic.
4. **Hidden decomposition**: They present a single verb for what
   is actually a consume + create + require decomposition, making
   the action body hide what the transaction does.

Every avoided feature above violates the principle that **the action
body must not hide what the transaction does**.

---

## 9. Summary: What Changes

### Core Verifier Syntax (Layer 1)

```text
KEEP:          action, flow, move, where, require, create,
               consume, destroy, read_ref, protected, witness, lock_args

REMOVE:        claim, settle, transfer
```

### Local Sugar (Layer 2)

```text
ADDED (0.13.1):  preserve, anonymous require block

NOT ADDED:       preserve *, preserve except, named require blocks,
                 bare preserve, policy primitives
```

### Standard Library (Layer 3)

```text
COMPILER-RECOGNIZED STDLIB PATTERNS:
    std::cell::same_type
    std::cell::preserve_type
    std::cell::same_lock
    std::cell::preserve_lock
    std::cell::preserve_capacity
    std::accounting::conserved
    std::receipt::claim
    std::lifecycle::settle
    std::lifecycle::transfer
```

### Prohibited (Layer 4)

```text
NEVER:    policy primitives, preserve all, preserve except,
          implicit transfer magic, general settle magic,
          cross-module proof policy imports
```

---

## 10. Implementation Status

| Version | Action | Status |
|---------|--------|--------|
| 0.13.1 | Remove `claim`, `settle`, `transfer` keywords from compiler. No deprecation warnings, no backward compatibility. | **DONE** |
| 0.13.1 | Introduce local `preserve` and anonymous `require` block sugar with type-checked, pure, canonical expansion. | **DONE** |
| 0.13.1 | Introduce compiler-recognized stdlib patterns for `std::lifecycle::transfer`, `std::receipt::claim`, `std::lifecycle::settle`, `std::cell::preserve_type`, and `std::accounting::conserved`. | **DONE**: lifecycle patterns expand to consume + canonical named output/constraint checks. |
| 0.13.1 | Implement `std::cell::same_lock`, `std::cell::preserve_lock`, and `std::cell::preserve_capacity` with canonical lock/capacity metadata verifier checks. | **DONE** |

### Migration Example: `transfer`

The `transfer x to y` expression keyword has been removed.
All transfer semantics must now use explicit `consume` + `create`:

After (core only):

```cellscript
action transfer_nft(nft: NFT, to: Address) -> nft_after: NFT
where
    consume nft
    create nft_after = NFT {
        token_id: nft.token_id,
        owner: to,
        metadata_hash: nft.metadata_hash,
        royalty_recipient: nft.royalty_recipient,
        royalty_bps: nft.royalty_bps
    }
```

After (with preserve sugar + stdlib):

```cellscript
action transfer_nft(nft: NFT, to: Address) -> nft_after: NFT
where
    consume nft
    create nft_after = NFT {
        token_id: nft.token_id,
        owner: to,
        metadata_hash: nft.metadata_hash,
        royalty_recipient: nft.royalty_recipient,
        royalty_bps: nft.royalty_bps
    }

    preserve nft_after from nft {
        token_id
        metadata_hash
        royalty_recipient
        royalty_bps
    }
```

Or with the stdlib lifecycle pattern:

```cellscript
action transfer_nft(nft: NFT, to: Address) -> nft_after: NFT
where
    std::lifecycle::transfer(nft, nft_after, to) {
        token_id, metadata_hash, royalty_recipient, royalty_bps
    }
```

---

## 11. Final Design Principle

```text
Core stays explicit.
Sugar stays local.
Advanced patterns go to stdlib.
Audit mode can always expand everything.
```

The core language is the verifier calculus. It names the inputs,
names the outputs, states the constraints, and declares the fate
of every Cell. Nothing more, nothing less.

Higher-level convenience belongs in layers above core, where it
can evolve independently without destabilizing the audit contract.
