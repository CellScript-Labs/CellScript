# RFC: CellScript Grammar Governance For Cell Transition Legibility

## Status

Draft governance note for the 0.13 / 0.14 language-surface cleanup track.

This document records a grammar-governance position rather than an immediate
implementation plan. It should guide future parser, formatter, example, and
reference-documentation work when CellScript decides how strictly to separate
transition shape, validity constraints, and global protocol laws.

## Thesis

The recent grammar feedback is useful because it does not overpraise the
language surface and does not misread CellScript's direction. It identifies the
main governance issue that appears once the language grows beyond early 0.13 /
0.14 syntax work:

> CellScript should not chase the cleanest general-purpose language grammar. It
> should chase the cleanest possible grammar for describing CKB Cell
> transitions.

In short:

> Not the most elegant general-purpose language, but the clearest audit format
> for Cell transitions.

CellScript's strongest identity is not that it resembles Rust, Move, Solidity,
or Python. Its strongest identity is that it can become:

> A verifier-oriented double-entry accounting DSL over the CKB Cell model.

That framing suggests one governing rule:

> State movement must be visible. Validity constraints must be visible. The
> grammar should not hide one inside the other.

## Design Principles

- Keep Cell movement first-class and auditable.
- Keep `resource`, `shared`, `receipt`, `action`, `lock`, `consume`, `create`,
  `destroy`, and `invariant` as domain words. They are the soul of the
  language surface.
- Separate transition shape from validity constraints.
- Separate action-local validity from resource/global law.
- Prefer formatter-friendly regularity over permissive optional syntax.
- Allow familiar expression syntax where it improves engineering legibility, but
  do not mix semantic roles.
- Treat accounting sugar such as `conserve` as explicit sugar that lowers to
  ordinary input/output constraints.

## 1. `where` Should Not Become A Hidden Function Body

The criticism that `where` has become more like a function body than a pure
constraint block is accurate.

A block like this mixes semantic roles:

```cell
where
    assert(...)
    let x = ...
    consume token
    create output = ...
    return ...
```

Once `where` contains local computation, Cell consumption, output creation, and
returns, it no longer reads as a validity-refinement block. It becomes a hidden
transition body. That makes the language model less honest.

The clearer direction is:

```cell
action transfer(token: Token, to: Address) -> Token
    consume token
    create next = Token { amount: token.amount } with_lock(to)
where
    require token.amount > 0
    require to != ZERO_ADDRESS
```

The transition shape is declared in the action body. The `where` block refines
validity. The `where` block does not secretly perform state transition work.

The mental model should be:

```text
signature = transition interface
body      = Cell movement and output shape
where     = validity constraints
```

Or, more compactly:

```text
signature: what action this is
body: which Cells are consumed, created, or preserved
where: why this action is valid
```

The compact rule is:

> Put state movement in the open and constraints in `where`. `where` should not
> do hidden work.

## 2. Govern `assert`, `require`, And `invariant`

`assert`, `require`, and `assert_invariant` are currently too close from the
external user's point of view. If all three can mean "this condition must hold",
then users will not know which one belongs in normal protocol code:

```cell
assert(x)
require(x)
assert_invariant(x)
```

A cleaner public semantic split is:

| Syntax | Meaning | Failure meaning |
|---|---|---|
| `require` | Action-local precondition or postcondition | This proposed transition is invalid. |
| `invariant` | Resource/global protocol law | The contract/spec is internally inconsistent if violated. |
| `assert` | Low-level/internal compiler primitive | Mostly generated, internal, or test-oriented. |

The main user-facing style should be:

```cell
where
    require token.amount > 0
    require output.amount == token.amount
```

Global or resource-level law should remain top-level:

```cell
invariant conservation:
    sum(inputs<Token>.amount) == sum(outputs<Token>.amount)
```

`assert` should be demoted in public guidance. Possible governance options:

- keep `assert` as a compiler/internal lowering primitive;
- expose only `debug_assert(...)` to users;
- allow `assert` only in test-only or explicitly low-level contexts;
- keep current syntax for compatibility, but make `require` the canonical style
  in docs, examples, snippets, and formatter output.

The key is not the exact spelling. The key is that public documentation should
not encourage three near-equivalent ways to express mandatory validity.

## 3. Mixed Surface Style Is Acceptable; Mixed Semantic Roles Are Not

Applied DSLs do not need perfect aesthetic purity. They need domain legibility.
CellScript benefits from familiar engineering syntax where it keeps examples
short and obvious:

```cell
let x = ...
Token { amount: x }
Vec<Token>
group_inputs<Token>
```

That kind of mixture is acceptable because it borrows expression forms that many
engineers already understand.

The dangerous mixture is semantic, not visual:

```cell
where
    assert(...)
    consume ...
    create ...
    return ...
```

Here the problem is not that the language looks partly Rust-like or partly
DSL-like. The problem is that the same block performs transition effects,
returns local values, and declares validity constraints.

Governance rule:

> Surface syntax may be mixed when it improves familiarity. Semantic roles must
> not be mixed.

Short form:

> Syntax may look familiar and mixed; semantic boundaries must not be mixed.

## 4. Prefer Formatter-Friendly Separator Rules

CellScript should not be too permissive with optional commas and semicolons. A
DSL that must support auditing, generated examples, LSP tooling, and stable
reference snippets benefits from regular syntax.

Recommended separator policy:

### Statement Blocks

Use newline-separated statements and avoid mandatory semicolons:

```cell
action transfer(token: Token, to: Address) -> Token
    consume token
    create next = Token { amount: token.amount } with_lock(to)
where
    require token.amount > 0
    require to != ZERO_ADDRESS
```

### Struct Literals, Lists, And Tuples

Require commas inside expression forms:

```cell
Token {
    amount: token.amount,
    owner: to,
}
```

### Function Calls

Require commas between arguments:

```cell
require hash(lock_args, nonce) == expected
```

The compact rule is:

> Blocks use newlines. Expressions use commas. Do not make both optional.

Short form:

> Blocks use newlines; expressions use commas. Do not make both optional.

This policy helps the parser, formatter, syntax highlighter, snippets, and code
review diffs. It also makes generated examples more stable.

## 5. Proposed Action Shape

A future grammar-cleanup pass can divide actions into three layers:

```cell
action transfer(token: Token, to: Address) -> Token
    consume token
    create next = Token {
        amount: token.amount,
    } with_lock(to)
where
    require token.amount > 0
    require next.amount == token.amount
```

Layer meaning:

| Layer | Role |
|---|---|
| Signature | Names the action and declares the transition interface. |
| Body | Declares Cell movement, output construction, preservation, and accounting shape. |
| `where` | Declares why the proposed transition is valid. |

This keeps Cell movement in the open. It also makes the `where` block easier to
read as proof obligations rather than an imperative body.

## 6. Integrating `preserve`, `conserve`, And Accounting Sugar

CellScript can make higher-level Cell accounting readable without hiding what it
lowers to.

Example:

```cell
action transfer(token: Token, to: Address) -> Token
    consume token
    create next = Token {
        amount: token.amount,
    } with_lock(to)
    preserve_capacity token -> next
where
    require to != ZERO_ADDRESS
    require next.amount == token.amount
```

A richer split example:

```cell
action split(token: Token, amounts: Vec<u64>) -> Vec<Token>
    consume token
    create outputs = amounts.map(|amount| Token { amount })
    preserve_lock token -> outputs
    conserve Token.amount
where
    require sum(amounts) == token.amount
    require all(amounts, |x| x > 0)
```

`conserve Token.amount` is powerful accounting sugar, but it must be documented
as sugar. It should lower to explicit input/output relations, for example:

```cell
require sum(inputs<Token>.amount) == sum(outputs<Token>.amount)
```

The governance principle is:

> Accounting sugar is welcome when it makes the audit shape clearer, but it must
> lower to explicit CellScript obligations that metadata and proof tooling can
> expose.

## 7. Preserve The Double-Entry Accounting Feel

The most valuable aesthetic direction for CellScript is not "small Rust" or
"Solidity for CKB". It is a verifier-oriented accounting language over proposed
Cell transitions.

The core verbs should stabilize around:

```cell
consume
create
preserve
require
```

And the higher-level governance words should be:

```cell
invariant
conserve
```

This vocabulary makes CellScript feel like it writes state-transition receipts,
not merely contract functions.

Short summary:

> This is not merely contract logic; it writes state-transition receipts.

## Compatibility And Migration Guidance

This RFC does not require immediate breaking changes. A staged path is safer:

1. Document the intended semantic split first.
2. Update examples and tutorials to prefer `require` for action-local validity.
3. Keep existing `where` forms working while warning on transition effects inside
   `where` only when a future compatibility mode enables that lint.
4. Teach the formatter to canonicalize commas in expression forms before the
   parser rejects old examples.
5. Reserve `assert` for internal, generated, debug, or legacy-compatible use.
6. Introduce `preserve` / `conserve` only when their lowering is explicit in
   metadata and proof-plan output.

The migration goal is not to punish existing syntax. The goal is to make the
canonical style honest enough that users can audit Cell transitions directly
from the source.

## Conclusion

The feedback is correct on four main points:

1. The domain words are strong. `resource`, `receipt`, `action`, `lock`,
   `consume`, and `create` are CellScript's identity.
2. `where` is drifting into a mixed transition/function body and should be
   governed back toward constraints.
3. `assert`, `require`, and `invariant` need public semantic separation.
4. Overly free punctuation is less valuable than formatter-friendly regularity.

However, CellScript should not respond by chasing the most elegant generic
language grammar. It should respond by becoming the clearest audit format for
CKB Cell transitions.

That is the language moat.
