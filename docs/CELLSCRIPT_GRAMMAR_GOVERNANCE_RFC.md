# RFC: CellScript Grammar Governance For Cell Transition Legibility

## Status

Active 0.19 grammar-governance contract.

This document defines the public syntax boundary that parser, formatter, LSP,
examples, docs, lowering metadata, and release gates must keep aligned.

## Thesis

CellScript should not chase the cleanest general-purpose grammar. It should
chase the clearest audit grammar for CKB Cell transformations.

The governing rule is:

> Action shape, Cell lifecycle declarations, and verification obligations must
> stay visible at the action site.

Short form:

> `action` states the transaction shape; `transition` states which state
> continues; `verification` states why the Cell transformation is acceptable.

## Canonical Surface

The 0.19 public action form is:

```cellscript
action NAME(params...) -> outputs {
    transition old -> new
    transition old.state: A -> new.state: B

    verification
        require ...
        preserve old -> new {
            field_a
            field_b
        }
        consume ...
        destroy ...
        create ...
}
```

The 0.19 public lock form is:

```cellscript
lock NAME(protected cell: T, witness arg: U) -> bool {
    verification
        require ...
}
```

`transition` is optional. Resource-accounting actions such as token split and
merge can have no state continuation.

## Semantic Layers

| Layer | Surface | Governance rule |
|---|---|---|
| Core verifier syntax | `action`, `lock`, `verification`, `transition`, `consume`, `create`, `destroy`, `read`, `protected`, `witness`, `lock_args` | Must expose transaction shape and CKB source boundary directly. |
| Local explicit sugar | `preserve old -> new { fields }`, anonymous `require { ... }` | Must expand to canonical `require` obligations and remain visible in metadata. |
| Stdlib helper patterns | `std::cell::*`, `std::lifecycle::*`, `std::receipt::*`, `std::accounting::*` | Must validate arguments, fail closed, and lower to explicit obligations. |
| Global protocol law | `invariant`, aggregate primitives, ProofPlan records | Must state trigger, scope, reads, and executable/metadata coverage. |
| Deferred / rejected syntax | Non-canonical action-body sugar or protocol-name semantics | Must not be accepted as partial syntax. |

`preserve ... except` is intentionally outside the accepted local-sugar
surface. Preservation is an audit boundary, so the source must name every field
whose equality is required. A blacklist form can silently change meaning when a
schema gains a field, either preserving a new field without review or leaving it
outside the visible audit trail. The parser therefore rejects `except` and `*`
inside `preserve` blocks and asks authors to write an explicit whitelist.

## 0.19 Release Status Matrix

| Track | Status | Implementation / evidence | Release gate |
|---|---|---|---|
| Canonical action / lock surface | Done for this slice | Parser, formatter, examples, wiki, VS Code grammar/snippets, and syntax-combo accepted cases use `verification` and action-level `transition`. | `scripts/cellscript_syntax_combo_audit.sh quick/ci/deep`; VS Code validate and dry-run in the release gate. |
| `verification` section boundary | Done for this slice | Public examples and editor snippets use `verification`; lifecycle statements remain proof obligations, not runtime execution. | Syntax-combo parser, formatter, type/effect, metadata, and codegen oracles. |
| Local sugar expansion | Done for this slice | `preserve` and anonymous `require { ... }` are checked against canonical field equality and pure-boolean grouping rules. | Required bug classes `SCA-BUG-PRESERVE-TYPE-EQUIVALENCE` and `SCA-BUG-REQUIRE-BLOCK-PURITY`. |
| Stdlib lifecycle helper boundary | Done for this slice | `std::lifecycle::*` and `std::receipt::*` helpers must validate arity/cell kind and lower to consume/create/locked-output obligations. | Required bug classes `SCA-BUG-STD-LIFECYCLE-LOCKED-OUTPUT`, `SCA-BUG-STDLIB-ARGUMENT-VALIDATION`, and `SCA-BUG-RECEIPT-LIFECYCLE-OUTPUT`. |
| Source qualifier linearity | Done for this slice | `read`, `protected`, `witness`, and `lock_args` bindings cannot be consumed, destroyed, or hidden behind stdlib lifecycle calls. | Required bug classes `SCA-BUG-SOURCE-QUALIFIER-LINEARITY` and `SCA-BUG-DEEP-READ-STDLIB-LIFECYCLE`. |
| Deferred / rejected surfaces | Done for this slice | Unknown `std::*`, named protocol magic, action-body sugar, reusable proof blocks, and transition blocks are not public 0.19 syntax. | Required bug classes `SCA-BUG-STDLIB-NAMESPACE-FAIL-CLOSED`, `SCA-BUG-DEEP-HIDDEN-LIFECYCLE`, and `SCA-BUG-DEEP-UNKNOWN-STDLIB`. |
| Machine-readable governance evidence | Done for this slice | The syntax-combo report emits `governance_release_matrix`, `governance_oracles`, and `known_bug_classes`. | The runner fails closed when the mode's required origins or bug classes are missing. |

This matrix is compiler-governance evidence. It is not a replacement for
builder-backed CKB transaction acceptance, external audit, or exhaustive
adversarial state-space verification.

## `verification`

`verification` is a section header, not an execution body. It can contain local
calculation, `require`, `preserve`, lifecycle operations, and output checks, but
those statements are proof obligations over a proposed CKB transaction.

Rules:

- `require` guards action/lock validity and can carry a static message or
  error label.
- anonymous `require { ... }` blocks contain only pure boolean expressions.
- lifecycle operations are forbidden inside `require { ... }`.
- `consume`, `create`, and `destroy` validate transaction shape; they are not
  VM-side allocation or storage mutation.
- public action/lock conditions use `require`.

## `transition`

`transition old -> new` declares a same-type Cell continuation. It does not
prove the field delta. The delta must be expressed with `require` and
`preserve`.

`transition old.state: A -> new.state: B` binds a declared `flow` edge for an
explicit state field.

Multiple transitions are written as repeated action-level lines:

```cellscript
transition wallet_before -> wallet_after
transition proposal_before -> proposal_after
```

Multiple continuations stay as repeated `transition` declarations so each edge
remains visible at the action site.

## Rejected Surfaces

The current parser and docs must present only the canonical forms above.
Protocol-name magic is especially forbidden: action names must not trigger
compiler behavior based on words such as claim, transfer, swap, or settle.

If a future version reconsiders a non-canonical surface, it needs parser,
typechecker, lowering, metadata, formatter, LSP, example, and regression
coverage in the same change.

## Acceptance Rules

Every grammar change must be checked across:

- parser accepted/rejected forms;
- typechecker semantic boundary;
- IR lowering and metadata expansion;
- codegen or explicit non-codegen blocker;
- formatter round trip;
- LSP completion/highlighting/snippets;
- examples and wiki docs;
- syntax-combination audit seeds.

Compile-only success is not enough for a CKB-facing grammar claim.

## Conclusion

CellScript's language moat is not generic elegance. It is a stable, readable
audit transcript for Cell transformations:

```text
action = verifier case
transition = lifecycle continuation declaration
verification = proof obligations
```
