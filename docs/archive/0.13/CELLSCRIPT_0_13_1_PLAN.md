
> **0.13 freezes the explicit action model.
> 0.13.1 adds local, audit-preserving ergonomics.**

---

**Status**: archived 0.13.1 patch plan. The implemented release contract is
recorded in `../../releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md`.

---

# CellScript 0.13.1 Patch Plan

## Local Ergonomics and Syntax Governance

---

## 1. Background

CellScript 0.13.0 establishes the new action model:

```text
A transaction proposes a Cell transformation.
An action verifies whether that transformation is allowed.
```

In this model, an `action` is not a method call, not an account-storage mutation, and not a runtime constructor. It is a typed verifier case: it names the input evidence, names the proposed output evidence, and proves the relationship between them. The 0.13 tutorial already frames this explicitly: signature direction describes transaction topology, `where` scopes proof obligations, `move` declares a state edge, `require` states verifier constraints, and `create` constrains proposed outputs.

0.13.1 does **not** change this model.

Instead, 0.13.1 introduces two local syntax improvements:

```text
1. preserve sugar
2. anonymous require block
```

Both are intentionally small.
Both are local.
Both are mechanically expandable into existing 0.13 core syntax.

The goal is to reduce repetitive notation without hiding verifier obligations.

### Version Positioning

0.13 has been released; see `../../../roadmap/CELLSCRIPT_0_13_RELEASE_SCOPE.md`
for the accepted scope and
`../../releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md` for the release notes.
0.13.1 is a **forward patch** on the current development branch: it
extends 0.13 surface syntax without reopening the 0.13 release boundary.

The 0.14 roadmap (`roadmap/CELLSCRIPT_0_14_ROADMAP.md` §12) lists surface
ergonomics backlog items including `transfer` sugar and batch-create sugar.
`preserve` sugar is scoped separately because it is a **pure local desugaring**
that does not touch verifier semantics, builder integration, or transaction
shape — making it safe for a patch release without waiting for the full 0.14
ergonomics pass.

---

## 2. Design Principle

0.13.1 follows one rule:

> **Reduce ceremony, not safety visibility.**


A syntax addition is acceptable only if:

1. its desugared form is obvious;
2. all security-relevant fields or expressions remain visible at the action site;
3. it does not introduce remote policy lookup;
4. it does not hide what the transaction does;
5. audit tooling can expand it back into canonical 0.13 `require` statements.

This keeps 0.13.1 aligned with the 0.13 philosophy:

```text
The action header says what changes.
The where block proves why it is allowed.
```

---

## 3. Feature 1: `preserve` Sugar

## 3.1 Motivation

A common 0.13 pattern is explicit field preservation:

```cellscript
require output.seller == input.seller
require output.price == input.price
require output.payment_symbol == input.payment_symbol
```

This is clear, but repetitive.

0.13.1 introduces local field-preservation sugar:

```cellscript
preserve output from input {
    seller
    price
    payment_symbol
}
```

This desugars exactly to:

```cellscript
require output.seller == input.seller
require output.price == input.price
require output.payment_symbol == input.payment_symbol
```

## 3.2 Canonical Example

Before:

```cellscript
action fill(input: Offer, payment: Token, buyer: Address)
    -> (output: Offer, seller_payment: Token)
    move input.state: Live -> output.state: Filled
where
    require output.seller == input.seller
    require output.price == input.price
    require output.payment_symbol == input.payment_symbol

    require payment.amount == input.price
    require payment.symbol == input.payment_symbol
    require output.buyer == buyer

    consume payment

    create seller_payment = Token {
        amount: payment.amount,
        symbol: payment.symbol
    } with_lock(input.seller)
```

After:

```cellscript
action fill(input: Offer, payment: Token, buyer: Address)
    -> (output: Offer, seller_payment: Token)
    move input.state: Live -> output.state: Filled
where
    preserve output from input {
        seller
        price
        payment_symbol
    }

    require {
        payment.amount == input.price
        payment.symbol == input.payment_symbol
        output.buyer == buyer
    }

    consume payment

    create seller_payment = Token {
        amount: payment.amount,
        symbol: payment.symbol
    } with_lock(input.seller)
```

The second version is shorter, but still audit-visible.

The auditor still sees exactly which fields are preserved:

```text
seller
price
payment_symbol
```

Nothing is hidden behind a reusable policy or opaque replacement primitive.

---

## 3.3 Rules

`preserve` is a **local field-preservation shorthand**.

Allowed:

```cellscript
preserve output from input {
    seller
    price
    payment_symbol
}
```

Not allowed:

```cellscript
preserve output from input
```

Not allowed:

```cellscript
preserve output from input {
    *
}
```

Not allowed:

```cellscript
preserve output from input except {
    state
}
```

### Reason

CellScript should use whitelist preservation, not blacklist preservation.

Whitelist:

```cellscript
preserve output from input {
    seller
    price
}
```

is safe because every preserved field is visible.

Blacklist:

```cellscript
preserve output from input except {
    state
}
```

is dangerous because newly added fields may be accidentally preserved or accidentally left unaudited.


---

## 3.4 Scope and Existing Mechanism

In 0.13.1, `preserve` applies only to resource data fields.

Cell metadata such as:

```text
lock
type
capacity
```

is handled through explicit constraints or compiler-recognized stdlib patterns:

```cellscript
require output.lock_hash == input.lock_hash
require output.type_hash == input.type_hash
require output.capacity == input.capacity

std::cell::same_lock(output, input)
std::cell::same_type(output, input)
std::cell::preserve_lock(output, input)
std::cell::preserve_type(output, input)
std::cell::preserve_capacity(output, input)
```

These helpers are Layer 3 stdlib patterns, not 0.13.1 core syntax.

### Relationship to Existing IR `preserved_fields`

The compiler already has a `preserved_fields: Vec<String>` vector in the IR
and metadata layer (`src/ir/mod.rs`, `src/lib.rs`), plus separate
`preserve_type_hash` and `preserve_lock_hash` boolean flags.
`preserve` sugar feeds into the **same** `preserved_fields` vector:

```text
preserve output from input { seller, price }
    ↓ lowering
preserved_fields = ["seller", "price"]   // appended to existing vector
preserve_type_hash = <unchanged>          // set by separate IR logic
preserve_lock_hash = <unchanged>          // set by separate IR logic
```

Cell metadata preservation (type_hash, lock_hash) remains on the separate
flag path. The `preserve` sugar only generates data-field equality
constraints and does not touch the metadata flags.

---

## 4. Feature 2: Anonymous `require` Block

## 4.1 Motivation

Long runs of `require` statements are common:

```cellscript
require payment.amount == input.price
require payment.symbol == input.payment_symbol
require output.buyer == buyer
```

0.13.1 allows grouping them:

```cellscript
require {
    payment.amount == input.price
    payment.symbol == input.payment_symbol
    output.buyer == buyer
}
```

This desugars into independent atomic `require` statements:

```cellscript
require payment.amount == input.price
require payment.symbol == input.payment_symbol
require output.buyer == buyer
```

## 4.2 Rules

A `require` block may contain only pure boolean expressions.

Allowed:

```cellscript
require {
    payment.amount == input.price
    payment.symbol == input.payment_symbol
}
```

Not allowed:

```cellscript
require {
    let x = payment.amount
    x == input.price
}
```

Not allowed:

```cellscript
require {
    consume payment
}
```

Not allowed:

```cellscript
require {
    if payment.amount == input.price {
        output.buyer == buyer
    }
}
```

### Reason

A `require` block is not a nested proof language.
It is only grouping sugar.

### Edge Cases

- **Empty `require {}`**: compile error — a require block must contain at
  least one boolean expression.
- **Single-expression `require { expr }`**: syntactically valid; the formatter
  may suggest the single-line `require expr` form.
- **Separator rules**: expressions are separated by newlines (no commas or
  semicolons required), matching the existing `where` block style.


---

## 4.3 No Named Blocks in 0.13.1

0.13.1 should not introduce named `require` blocks.

Do not add:

```cellscript
require payment_valid {
    payment.amount == input.price
    payment.symbol == input.payment_symbol
}
```

Reason:

1. names can mislead;
2. names may imply semantic status;
3. names invite future reusable policy calls;
4. reusable proof labels risk hiding the audit decomposition.

For 0.13.1, keep it anonymous:

```cellscript
require {
    payment.amount == input.price
    payment.symbol == input.payment_symbol
}
```

If future tooling needs diagnostics labels, consider an annotation form later:

```cellscript
require @payment_valid {
    payment.amount == input.price
    payment.symbol == input.payment_symbol
}
```

But that should remain metadata, not semantics.

---

## 5. Syntax Governance

0.13.1 introduces a formal syntax-governance policy.
The final 0.13.2 release contract is summarized in
`docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md`, with automated audit
methodology in `docs/CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md`.

The governance model classifies language features into four layers:

```text
Layer 1: Core Verifier Syntax   — action, flow, move, where, require, create, consume, destroy
Layer 2: Local Explicit Sugar    — preserve, anonymous require block
Layer 3: Standard-Library Patterns — claim, settle, transfer, conserve, cell metadata helpers
Layer 4: Avoided                — policy primitives, preserve all/except
```

Key governance decisions for 0.13.1:

- `claim`, `settle`, `transfer` are **removed from core** and represented as
  compiler-recognized stdlib patterns.
- Core input-fate verbs are reduced to `consume` and `destroy` only.
- `preserve` and `require` block are accepted as Layer 2 local sugar.
- All higher-level patterns live in stdlib namespaces
  (`std::cell`, `std::accounting`, `std::receipt`, `std::lifecycle`, `std::ckb`).

---

## 6. Desugaring Requirements

0.13.1 should define canonical expansion rules.

## 6.1 `preserve`

Source:

```cellscript
preserve output from input {
    seller
    price
    payment_symbol
}
```

Canonical expansion:

```cellscript
require output.seller == input.seller
require output.price == input.price
require output.payment_symbol == input.payment_symbol
```

## 6.2 `require` Block

Source:

```cellscript
require {
    payment.amount == input.price
    payment.symbol == input.payment_symbol
}
```

Canonical expansion:

```cellscript
require payment.amount == input.price
require payment.symbol == input.payment_symbol
```

## 6.3 Audit Expansion

The compiler should expose a desugaring view:

```bash
cellc expand <file> --action <name>
```

or alternatively as a build flag:

```bash
cellc build <file> --expand
```

This is deferred to 0.13.2 tooling (see §7.2). For 0.13.1, the desugaring
contract is defined by this document; the CLI command is not blocking.

Expected output format: plain CellScript source showing the expanded `require`
statements in place of `preserve` and anonymous `require` blocks, preserving
source spans for diagnostics.

Example:

```cellscript
require output.seller == input.seller
require output.price == input.price
require output.payment_symbol == input.payment_symbol
require payment.amount == input.price
require payment.symbol == input.payment_symbol
require output.buyer == buyer
```

This makes sugar acceptable to auditors.

---

## 7. Compiler and Tooling Notes

0.13.1 itself may focus on parser, AST, and lowering support.

Full toolchain improvements may be deferred to 0.13.2.

## 7.1 0.13.1 Implementation Scope

Required:

```text
parse preserve blocks
parse anonymous require blocks
lower preserve to require equality constraints (into existing preserved_fields IR vector)
lower require block to atomic require statements
type-check preserve fields exist on both source and target
update examples
update tutorial
add syntax governance document
add parser tests
add lowering tests
update formatter for new syntax forms
update LSP completions for preserve keyword and fields
update VS Code extension (grammar, snippets)
bump metadata schema version if AST/lowering additions require it
```

### File-Level Change List

| File | Change |
|------|--------|
| `src/lexer/token.rs` | Add `Preserve` keyword token |
| `src/lexer/mod.rs` | Register `preserve` keyword |
| `src/parser/mod.rs` | Add `preserve` block and anonymous `require` block parsing |
| `src/ast/mod.rs` | Add `PreserveBlock` and `RequireBlock` AST nodes |
| `src/types/mod.rs` | Validate `preserve` fields exist on both source and target types |
| `src/resolve/mod.rs` | Resolve `preserve` field names to concrete type fields |
| `src/ir/mod.rs` | Lower `preserve` to `preserved_fields` entries; lower `require` block to atomic `require` statements |
| `src/codegen/mod.rs` | No new codegen paths — desugared forms use existing `require` lowering |
| `src/fmt/mod.rs` | Format `preserve` blocks and `require` blocks |
| `src/lsp/server.rs` | Add `preserve` keyword and field-name completions |
| `src/lsp/mod.rs` | Register new completion triggers |
| `src/error/mod.rs` | Add compile-time error codes for invalid `preserve`/`require` block usage |
| `src/runtime_errors.rs` | No new runtime errors — `preserve` is compile-time sugar |
| `src/docgen/mod.rs` | Document new syntax in generated docs |
| `src/lib.rs` | Register new AST/lowering paths; bump metadata schema version if needed |
| `editors/vscode-cellscript/syntaxes/cellscript.tmLanguage.json` | Add `preserve` keyword and block grammar |
| `editors/vscode-cellscript/snippets/cellscript.json` | Add `preserve` and `require` block snippets |
| `editors/vscode-cellscript/extension.js` | Sync extension capabilities |

### Compile-Time Error Codes

New compile-time error codes for 0.13.1:

```text
E1001  preserve block is empty — at least one field name required
E1002  preserve field '{name}' does not exist on output type '{type}'
E1003  preserve field '{name}' does not exist on input type '{type}'
E1004  require block contains non-boolean expression
E1005  require block contains statement (let/consume/create/destroy)
E1006  require block contains control flow (if/match)
E1007  preserve wildcard '*' is not allowed
E1008  preserve except is not allowed — use explicit whitelist
E1009  bare 'preserve output from input' requires a field block
```

These are compile-time diagnostics, not runtime error codes. They do not
enter the runtime error registry (`src/runtime_errors.rs`).

### Test Coverage Requirements

**Parser tests**:
- positive: `preserve` block with 1 field, N fields; `require` block with 1 expression, N expressions
- negative: empty `preserve {}`, `preserve *`, `preserve ... except`, bare `preserve` without block, `require` block with `let`/`consume`/`if` inside
- edge: `preserve` in nested action, `require` block inside `where` after other statements

**Lowering tests**:
- `preserve output from input { a, b }` lowers to `require output.a == input.a; require output.b == input.b`
- `require { x, y }` lowers to `require x; require y`
- `preserved_fields` vector is populated correctly
- `preserve_type_hash` / `preserve_lock_hash` flags are not affected

**Formatter tests**:
- round-trip: formatted `preserve` and `require` blocks parse identically
- single-expression `require` block formats to single-line form

**Integration tests**:
- existing examples compile without regression
- `cellc expand` (when implemented) produces canonical desugared output

Optional:

```text
basic expanded output in debug mode
basic diagnostics showing desugared require source span
```

## 7.2 0.13.2 Deferred Tooling

Recommended for 0.13.2:

```text
audit expansion view
better require failure diagnostics
underconstraint analysis
create field coverage checks
input fate completeness checks
flow graph report
test fixture skeleton generation
LSP autocomplete for preserve fields
auto-fix repeated require into preserve
```

---

## 8. Example: Updated 0.13.1 Style

See §3.2 for the before/after comparison. The canonical 0.13.1 `fill` action
using both features is:

```cellscript
action fill(input: Offer, payment: Token, buyer: Address)
    -> (output: Offer, seller_payment: Token)
    move input.state: Live -> output.state: Filled
where
    preserve output from input {
        seller
        price
        payment_symbol
    }

    require {
        payment.amount == input.price
        payment.symbol == input.payment_symbol
        output.buyer == buyer
    }

    consume payment

    create seller_payment = Token {
        amount: payment.amount,
        symbol: payment.symbol
    } with_lock(input.seller)
```

The action still clearly shows:

```text
input evidence
output proposal
state edge
field continuity     ← preserve sugar, still audit-visible
payment validation   ← require block, still audit-visible
input fate
output construction
```

---

## 9. Release Positioning

## 0.13.0

```text
Action model freeze.
```

0.13.0 establishes the explicit Cell transformation model.

## 0.13.1

```text
Local ergonomics patch.
```

0.13.1 adds:

```text
preserve sugar
anonymous require block
syntax governance
canonical desugaring rules
```

## 0.13.2

```text
Stable tooling release.
```

0.13.2 extends that tooling work with the completed syntax-governance hardening:
stdlib lifecycle patterns expand to explicit consume/create/output constraints,
cell metadata helpers lower to canonical verifier checks, and editor tooling
surfaces those helpers directly.

---

## 10. Final Summary and Acceptance Criteria

CellScript 0.13.1 should be intentionally small.

It should add only:

```text
1. preserve output from input { fields }
2. require { exprs }
3. syntax governance for core vs sugar vs stdlib patterns
```

It should not add:

```text
policy
named reusable require blocks
preserve all
preserve except
general conserve syntax
capacity magic
```

Final design principle:

```text
Core stays explicit.
Sugar stays local.
Advanced patterns go to stdlib.
Audit mode can always expand everything.
```

### Acceptance Criteria

0.13.1 is complete when all of the following hold:

```text
✅ 'preserve' and anonymous 'require' blocks parse without error
✅ 'preserve' lowers into the existing preserved_fields IR vector
✅ 'require' block lowers into independent atomic require statements (IR conditional branches)
✅ Invalid forms (empty preserve, wildcard, except, non-boolean require)
   produce compile-time errors with the codes E1001–E1009 defined in §7.1
✅ Type checker validates preserve fields exist on both output and input types (E1002/E1003)
✅ RequireBlock/Preserve are restricted to actions and locks (verifier-boundary syntax)
✅ Existing 0.13 examples compile without regression
✅ Formatter round-trips both new syntax forms
✅ LSP completes 'preserve' keyword and target-type fields
✅ VS Code extension grammar and snippets updated
✅ Parser + lowering + formatter test coverage meets §7.1 requirements
   (10 parser tests, 2 IR lowering tests, 3 formatter tests)
✅ cargo fmt/check/clippy/test pass cleanly
✅ CompileError supports structured error codes via .with_code()
✅ Syntax governance contract recorded in the 0.13.2 release notes
✅ Examples updated to demonstrate 0.13.1 syntax and 0.13.2 stdlib lifecycle/cell metadata patterns
```
