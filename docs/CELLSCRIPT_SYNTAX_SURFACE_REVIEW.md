# CellScript Syntax Surface Review And Convergence Proposal

## Status and decision scope

Review date: 2026-11-25. Baseline: branch `0.36`, HEAD `9f89d1af`, including
the existing uncommitted working-tree changes. Source semantics:
`cellscript-source-semantics-2027-0.33-authoring-v8`.

Status: baseline proposal for the broader redesign, with the initial bounded v8
contract fixes recorded below. The implementation now proceeds in the unpublished
authoring-v9 candidate tracked in the implementation record. Owners:
language/frontend maintainers, with the ABI, typed semantics, checker, tooling
and builder owners identified below.
The current [authoring grammar](CELLSCRIPT_AUTHORING_GRAMMAR.md) remains the
executable contract. Findings below describe the reviewed v8 baseline; the
implementation record identifies completed native changes and pending consumers.

The [authoring-v8 follow-up audit](reports/0.36/AUTHORING_V8_SYNTAX_AUDIT.md)
records direct reproductions, alternative designs and finance comparisons on
`e134d663`.
The [complete implementation record](reports/0.36/SYNTAX_CONVERGENCE_IMPLEMENTATION.md)
tracks the 2026-11-26 instruction to finish the remaining convergence and its
requirement-by-requirement evidence.

The initial review was static: grammar, parser dispatch, AST, entry contracts,
support matrices, representative examples and existing regression ownership.
The implementation follow-up below distinguishes the confirmed contract fixes
from the remaining design proposals. Validation runs in an isolated worktree;
the original checkout's root `target/` remains removed. The repair is based on
clean HEAD `9f89d1af`; unrelated Finance working-tree changes are excluded from
the commit and its validation evidence.

The recommended direction is to consolidate entry interfaces and lifecycle
composition while retaining explicit source, identity, authorization and
resource boundaries. Removing a keyword is useful only when authors no longer
need a second rule to express or understand the same behavior.

## Implemented contract fixes

The follow-up fixes confirmed v8 contract discrepancies without removing
currently documented signature forms or adopting a new source epoch:

- Historical RFC headers identify the current grammar authority. The authoring
  target now uses the current commit/integration gate distinction. The native
  documentation check requires the historical status markers instead of the
  obsolete active-governance claim.
- Flow edges require explicit semicolons. The previous implementation's
  accidental acceptance is covered by a failing-before/passing-after regression.
- Unsupported nested policies identify their entry-scope restriction, while
  ordinary calls with the same contextual names remain accepted.
- The grammar describes retained role spellings, reserved mutable-reference
  admission and policy placement explicitly. The bounded lifecycle fixture's
  comment identifies its missing output policy rather than rejecting the
  entire admitted family.
- Cache generation v214 prevents a pre-fix cached artifact from bypassing the
  corrected parser policy.

This section records the initial bounded v8 repairs. The ongoing v9 candidate
now includes shared branch-local lifecycle statements, explicit Lock artifacts
and delimited validity; the complete
[implementation record](reports/0.36/SYNTAX_CONVERGENCE_IMPLEMENTATION.md)
distinguishes native evidence from pending source migration and consumers.

## Surface inventory

| Area reviewed | Current surface | Decision |
| --- | --- | --- |
| Modules and interfaces | `module`, `use`, aliases, visibility, constants | Keep; make exported entry interfaces explicit in canonical examples. |
| Ordinary values | `struct`, `enum`, abilities, generics, `new`, tuples and arrays | Keep the shared value grammar; no second constructor grammar. |
| Cell schemas | `resource`, `shared`, `receipt`, capabilities and type policies | Preserve distinct kinds until their complete contracts are shown equivalent. |
| Callables | `fn`, `action`, `lock`, `lock_script` | Keep helper/entry responsibilities; converge the two Lock declaration paths. |
| Entry roles | Prefix/postfix sources, inferred sources, arrow/inline Cell outputs | Choose one canonical role signature; retain ordinary value returns. |
| Artifact selection | Context, bindings, coverage, layout, dispatch, tags | Keep resolved dimensions separate; reduce repetitive author input with tooling first. |
| Fixed lifecycle | `replace`, `pool`, `retire`, `fresh`, `audit` | Make executable dispositions compose in checked branches; keep audit claims separate. |
| Lower-level lifecycle | `consume`, `create`, destruction families, unique operations, claim/settle and stdlib helpers | Establish one recommended authoring route per complete semantic case. |
| State and preservation | `flow`, `transition`, `preserve`, successor field treatments | Distinguish state edges, partial equality and complete replacement. |
| Assertions | `require`, grouped `require`, validity, invariants and bounded aggregates | Keep scope and purity explicit; fix presentation inconsistencies. |
| Runtime values and codecs | Stack Vec, allocated members, schema Vec, `data_shape`, bounded plans | Expose backing and lifetime; avoid one misleading generic collection promise. |
| Cell selection | `cell_roles`, finite scans, typed views and raw selectors | Keep observation, binding and authorization distinct. |
| Control and borrowing | Branches, matches, loops, labels, borrow scopes, references | Retain bounded admission and ordinary control flow; do not imply general mutable references. |
| Consumers | Formatter, expansion, metadata, LSP/editor, browser and builders | Every convergence change must preserve one current contract across consumers. |

The inventory is a review of all declaration and major statement/type families,
not a claim of exhaustive behavior testing for every combination.

## Improvements already present in v8

These are resolved design directions, not new work proposed by this review:

- Semicolons, comma-separated lists, `new` and `::<...>` provide a shared
  syntax kernel. The `verification` and capability-attribute alternatives were
  removed.
- Type group business operations use `action`; artifact selection supplies
  execution context, physical bindings and coverage.
- Fixed group cardinality comes from resolved roles. Pool cardinality comes
  from its member lists. Neither needs an independently maintained count.
- Aggregate bare targets inherit invariant scope; explicit views must agree.
- `consume binding;` is a Unit statement, rather than an ordinal selector or
  an ordinary returned value.
- Effect facts are inferred. Reintroducing handwritten `effects` declarations
  would recreate a second authority for program behavior; see the
  [effect contract](CELLSCRIPT_EFFECT_CONTRACTS.md).

## Findings and design decisions

Priority P0 means an immediately misleading contract description. P1 means
grammar consistency or composition should be settled before the next source
convergence. P2 means an ergonomic improvement requiring separate evidence.
These priorities do not assert exploitable security defects.

### F01 — Conflicting grammar authorities (P0)

The historical [governance RFC](CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md) called
itself active while presenting mandatory `verification` and rejecting designs
that the adopted authoring model now supports. The historical
[elegance RFC](CELLSCRIPT_SURFACE_ELEGANCE_RFC.md) also described that old surface
as active. Their status headers are corrected with this review; their original
examples remain historical evidence.

Other entry points still need a scoped freshness pass. For example,
[`bounded_lifecycle.cell`](../examples/language/collections/bounded_lifecycle.cell)
has an opening blanket rejection comment, while the current
[collections matrix](CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md) describes admitted
bounded lifecycle shapes. That discrepancy does not prove that every shape in
that fixture executes. Explain each fixture's actual limitation instead of
teaching a family-wide rejection.

Decision: one current grammar and source contract, dated historical RFCs, and
explicitly pending proposals. Do not use a historical acceptance matrix as a
current syntax specification. Reconcile old gate instructions with the current
[gate policy](CELLSCRIPT_GATE_POLICY.md) during that pass.

### F02 — Several entry signature grammars for similar roles (P1)

The [callable parser](../src/parser/callables.rs) accepts both
`witness value: T` and `value: witness T` for ordinary callable declarations;
postfix `protected` and `lock_args` also remain. The prefix and postfix paths
populate the same source field. These alternatives add rules without a new
meaning. The separate `lock_script` port parser accepts only prefix roles.

Cell outputs additionally have named arrow forms and inline `output` ports.
The arrow also denotes an ordinary value return, so readers must distinguish
`-> next: Token`, `-> (next: Token, change: Token)` and `-> bool`. Inline
outputs select the Unit entry convention and reject an additional arrow return;
this is a real compatibility condition, not merely punctuation.

Decision for the next convergence: use prefix provenance and inline Cell
outputs as the canonical transaction-entry interface. Reserve `-> T` for an
ordinary returned value. Helper `fn` parameters continue to describe arguments,
without requiring transaction-source qualifiers. Current markerless entry
parameters retain their existing meaning until a deliberate source migration.

The following uses the current v8 spelling and is the target presentation:

```cellscript
action transfer(
    input token: Token,
    witness recipient: Address,
    output next: Token,
) {
    replace token -> next {
        data = same;
        lock = exact(recipient);
        type_script = same;
        capacity = same;
    }
}
```

This example includes complete successor policies. Rewriting a partial old
action into it can tighten behavior and is not automatically equivalent.
Removing postfix spellings or named arrow outputs is a future breaking parser
change, with targeted migration diagnostics, not a formatter normalization.
Preserve physical input/output positions and witness order: in v8, moving a
parameter can change its binding even when its type and name stay the same.

### F03 — Lock declarations have an asymmetric module model (P1)

At the reviewed baseline, the
[retired Lock parser](https://github.com/CellScript-Labs/CellScript/blob/e134d663de9e7c226af5c4691648e86bc3a283ce/src/frontend/next.rs)
required `lock_script` to be
the final top-level declaration and the module's only action/Lock entry.
[`src/parser/callables.rs`](../src/parser/callables.rs) separately parses
ordinary `lock`. Type group actions already support multiple declarations
selected by artifacts. Moving a helper below a Lock entry should not need a
new language concept.

Decision: converge on the ordinary `lock` declaration with explicit Lock-group
artifact selection, analogous to action selection. First extend the artifact
contract: its current [context enum](../src/artifact.rs) admits only Type groups.
Do not merely delete `lock_script` checks or reinterpret an ordinary Lock as a
group-bound artifact without binding and coverage evidence.

After that closure, retire the duplicate declaration path in a new source
epoch. Remove the final-declaration restriction; preserve protected-role
selection, boolean result handling, witness placement and the exact supported
Lock-group responsibility. Supporting multiple selectable Lock declarations
does not imply a new on-chain multi-entry dispatch ABI.

The distinction between transaction-absolute and group-relative coordinates
must remain visible. CKB group indices refer to a group-specific view, and
Type groups consider both inputs and outputs; see the official
[Script group execution documentation](https://docs.nervos.org/docs/tech-explanation/script-group-exe).
Module declaration order is a separate frontend concern.

### F04 — Flow edge termination contradicts the v8 list/statement rule (P1)

At the review baseline, [`parse_flow`](../src/parser/data_declarations.rs)
consumed a comma or semicolon if present after an edge, but required neither.
Initial and terminal state declarations required semicolons. The current
authoring grammar said flow edges end in `;`, and the normal frontend delegated
to that parser without a separate flow-termination check.

Implementation follow-up: regression tests reproduced the missing-semicolon
acceptance before the repair. Each edge now uses the shared statement/clause
termination check, with an insertion span at the end of the edge. Tests cover
missing and comma-only separators, final edges, same-line/multiline clauses,
comments, formatter round trips, metadata and language-service diagnostics.
Cache generation v214 invalidates results from the earlier parser policy.

Decision: each flow edge is a clause and requires `;`; comma-separated state
lists remain lists. Add missing/comma-only separator rejection, same-line and
multiline cases, comments, and recovery at the next edge. Settle the source
compatibility impact even though this tightens behavior to the documented rule.

### F05 — Lifecycle forms do not compose through the same statement grammar (P1)

[`parse_action_policy`](../src/parser/action_policies.rs) recognizes
`pool`, `retire` and `fresh` from the outer action-body loop.
[`parse_stmt_inner`](../src/parser/statements.rs) recognizes `replace` inside
ordinary blocks. Branches call the ordinary block parser. Consequently the
newer policy forms do not have the same branch composition route as replacement.
This complicates a natural operation such as “continue this note or retire it.”

Decision: represent executable lifecycle dispositions in the shared statement
grammar, with structured occurrence identity and source spans. Admit branches
only when typed and independent coverage checks can prove exactly-once role
accounting on every accepting path. Reject role aliasing, a missing branch,
double disposition and unsupported repeated disposal in loops.

This is a semantic/lowering/checker change as well as a parser change. A union
of all branches' obligations is insufficient. Keep `audit` as a declaration of
metadata-only evidence; parsing it in a branch must not imply that a condition
or external authorization was checked.

Implementation follow-up: unsupported policy placement now produces a direct
entry-level-policy diagnostic instead of an unrelated missing-semicolon error.
The outer parser and placement check share the same policy lookahead. Ordinary
calls named `pool`, `retire`, `fresh` or `audit` remain valid. The later native v9
implementation admits branch-local lifecycle policies with actual IR occurrences
and independent path/conservation checks; distributed consumer closure remains
tracked in the implementation record.

### F06 — Lifecycle vocabulary mixes complete policies and low-level disposal (P1)

The admitted surface includes `consume`, `destroy`, policy-specific destruction
calls, `create`, `create_unique`, `replace_unique`, structured dispositions,
claim/settle and stdlib lifecycle helpers. This is too much to teach as a set
of interchangeable ways to move a Cell.

The recommended authoring vocabulary is:

| Intent | Canonical authoring route | Essential distinction |
| --- | --- | --- |
| One predecessor and one checked successor | `replace` | Complete local data, Lock, Type Script and capacity treatments. |
| Many-to-many conserved quantity | `pool` | Named members, authenticated domain and checked aggregation. |
| No continuing object | `retire` | Exact absence/identity policy. |
| New output | `fresh` | Full initialization, creation identity and destination Lock. |
| Explicit low-level ownership disposal | `consume` | Does not establish retirement, conservation or a successor. |
| Quantity destruction | Existing checked burn policy | Not equivalent to absence of a particular business object. |
| Receipt claim or settlement | Existing typed operation | Keep its receipt/output/continuity contract until an exact replacement exists. |

`destroy` currently entails consume plus burn; the
[capability registry](../src/ast/mod.rs) also retains the older `destroy`
capability alias. Canonical capability declarations should teach the kernel
capabilities rather than a second equivalent permission list. Removing the old
source capability requires migration and wire-vocabulary review separately.

Decision: one recommended route per complete intent, with explicit advanced
documentation for the lower-level kernel. Do not remove supported operations
until every policy, output binding and failure behavior has an equivalent.
In particular, never mechanically translate bare `consume` into `retire` or
`destroy`: that invents a business guarantee or changes accepted transactions.
The DSL checks proposed transaction Cells; these verbs must not teach VM-side
allocation or in-place mutation.

### F07 — Identity appears at several sites with different responsibilities (P1)

A Cell type can declare `identity(...)`, while `create_unique` and
`replace_unique` name an operation identity again. `fresh` also has an identity
clause. This creates a visible repetition, but declaration policy and an
operation's requested guarantee must first be compared under resolved typing.
See the [identity example](../examples/language/ownership/identity_lifecycle.cell)
and [Cell operation parser](../src/parser/cell_operations.rs).

Decision: the resolved declaration is the authority for its identity policy.
An operation may state a required matching policy; a conflict rejects and
cannot silently override the type. A future concise operation may omit that
policy only when resolution yields one unambiguous enforced treatment, shown
in expansion. Preserve today's explicit form until that rule has full evidence.

Do not merge `#[type_id("...")]`, `identity(ckb_type_id)`, an identity field,
and complete Type Script hash equality. They answer different questions about
nominal schema identity, instance policy and Script identity. Similarly,
`Address`, `Hash` and `ScriptHash` must not become aliases merely because their
storage widths match. None is a verified signer value by itself.

### F08 — Policy completeness varies by relation without a uniform explanation (P1)

`replace` requires four treatments. `pool` and `fresh` derive some policies from
their context, while capacity planning is left to the builder subject to the
applicable declaration/profile checks. `retire` has no successor envelope.
This is reasonable only when the resolved result is inspectable and omission
has one meaning.

| Relation | Data | Lock | Type Script | Capacity |
| --- | --- | --- | --- | --- |
| `replace` | Explicit complete treatment | Explicit treatment | Explicit preservation | Explicit exact equality in the current admitted form. |
| `pool` | Conservation plus exact other fields | Explicit per output | Current Type-group membership when established by actual role bindings; transaction-absolute roles do not infer equality. | No predecessor-to-successor equality promised by the source form. |
| `fresh` | All fields initialized | Explicit destination | Actual output membership and declaration-owned identity obligations; the nominal schema alone supplies no group authority. | No predecessor equality exists; builder planning is separate. |
| `retire` | No successor | No successor | Absence policy, not a successor check | No successor capacity relation. |

Decision: use one resolved policy presentation for every disposition: explicit,
derived-and-enforced, unconstrained-by-this-relation, or not-applicable. Expansion
must show which category applies. “The builder handles it” is not executable
evidence of a capacity equality or bound.

Keep mandatory `replace` treatments. Do not shorten it by defaulting Lock or
capacity. A later named capacity policy needs a closed set of enforceable
meanings, separate old/new acceptance evidence and checker support; this review
does not introduce a generic policy-call DSL or claim such alternatives exist.

### F09 — Transition and preservation partially overlap replacement (P2)

A common older pattern is a lineage `transition`, a `preserve` whitelist and
several field requirements. A complete `replace` can put the successor policy
in one place. However, `preserve` only asserts selected equalities, and an
enum-state transition also binds a declared `flow` edge. They are not generally
equivalent to replacement.

Decision: recommend `replace` for complete one-to-one lifecycle authoring.
Retain independent `preserve` for local field comparisons and retain explicit
flow validation for state machines. Do not require authors to restate bare
lineage already established by the same complete replacement. A migration
must account for every old field, state edge and missing Lock/capacity policy;
it must stop when it cannot establish equivalence.

Whole-data `same`, exhaustive field treatments and `same except` are useful
intent levels. Keep all three, with existing schema acknowledgements. A
formatter must not convert an explicit whitelist into schema-wide implicit
preservation just to reduce line count.

### F10 — `Vec` hides substantially different backing models (P1)

The [collections support matrix](CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md)
describes schema values, stack-backed local Vec and arena-backed `collections`
members. A bare `Vec<T>` spelling does not communicate the same allocation,
capacity, ownership or wire contract in those contexts.

One especially surprising documented behavior is that stack
`Vec::with_capacity(n)` does not make `capacity()` equal to `n`; backing capacity
is `256 / element_width`. A developer accustomed to an allocating vector may
misunderstand the resource bound. This is a documentation-grounded observation,
not a new measurement from this review.

Decision: diagnostics, hover and explanation should expose backing model,
element width, actual capacity and applicable budget. Canonical bounded
business examples should declare their allocation policy instead of relying
on a constructor name to imply it. A future explicitly bounded stack type is
a separate type-model decision, not another alias to add beside today's Vec.

Keep `BoundedCellSet`, witness `BoundedList`, immutable nested views and owned
local collections distinct: they have different sources, ownership, codecs
and output-correspondence obligations. Do not flatten them into one generic
“list” feature or promise support for arbitrary nested dynamic elements.

### F11 — Named budgets are a good convergence; positional duplicates remain (P2)

`data_shape`, `cell_roles` and `collections` already share named limits and
comma-separated entries. Their distinct limits describe encoded bytes,
payload bytes, scan work, selected Cells, allocation events and local storage.
These quantities are not interchangeable.

The named `data_shape` form and low-level `NestedValues<T, N, P, B>` can describe
the same bounded codec. Prefer the named form in new authoring examples; retain
the underlying codec identity and explain its low-level representation in the
advanced reference. Do not add a third positional shorthand.

The parsers for arrays and named budgets currently require literal bounds.
Reusing a checked named unsigned constant could reduce repeated magic numbers,
but it needs one shared constant-resolution rule and bounded diagnostics.
Decision: consider only closed scalar constants first; arbitrary expressions,
user-defined compile-time execution and dependent generic arithmetic are out
of scope. Do not repurpose a witness limit as an allocation or scan limit.

### F12 — Aggregate shorthand and scope are useful, not arbitrary duplication (P2)

`assert_conserved(T.amount)` and an admitted input/output `assert_sum`
equality overlap in meaning for their matching bounded numeric domain.
Conservation deserves a readable intent form; generalized sum comparisons
still express other relations. Keep them linked to one typed elaboration and
admission boundary, rather than maintaining competing arithmetic semantics.
The existing [invariant parser](../src/parser/invariants.rs) retains both forms.

Keep `require` for executable conditions and `audit` for metadata-only claims.
Keep scoped invariants distinct from local action predicates. Grouped
`require { a, b, };` is a pure conjunction list, not a block with arbitrary
statements or per-item failure labels. Separate `require` statements remain
the clear choice for distinct messages and control flow.

Do not remove invariant `reads` merely because reads can also be inferred.
Its declaration/coverage role must be reconciled with the inferred facts
before making it derived. Likewise, `read name: T` identifies entry provenance,
while `&T` and a borrow scope describe a local view/lifetime; they are not
interchangeable syntax aliases.

### F13 — Validity sections and structured expressions have special punctuation (P2)

[`parse_fields_and_validity`](../src/parser/data_declarations.rs) uses
`validity` as an unbraced final section of a type body. The reader must learn
that it consumes all remaining predicates and that fields cannot follow it.
The term “validity block” in diagnostics does not expose that section rule.

Proposed next-epoch spelling, not current v8 syntax:

```text
resource PositiveAmount has store {
    amount: u64,
    validity {
        require amount > 0;
    }
}
```

Decision: give validity a real delimited region with the same predicate-purity
rule. No executable capability follows merely from adding braces. Coordinate
parser, formatter, diagnostics, highlighting and semantic identities.

The shared statement parser also treats braced create/unique operations as
structured expressions: a final expression can be a value, while a terminated
one discards it. This needs a complete reference table, not a blanket claim
that every braced form is a statement. Do not insert a semicolon after a
value-producing create and silently change callable return/linear behavior.

Keep the current grouping/singleton-tuple distinction, generic-call punctuation,
literal rules and bitwise-before-comparison precedence. They are coherent;
another precedence or constructor redesign has lower benefit than fixing
composition and the remaining punctuation exception in F04.

### F14 — Reserved syntax and public visibility need clearer presentation (P2)

The grammar includes `&mut T` in type syntax, while the
[executable surface matrix](generated/CELLSCRIPT_EXECUTABLE_SURFACE_MATRIX.md)
classifies general mutable references as reserved. A parser-recognized shape
should not look like an implemented feature in completion or an introductory
grammar. Show syntax recognition and executable admission separately.

Keep `has` for Cell capabilities and value abilities in their separate domains;
keep `type_id` and `scheduler_hint` attributes scoped to their actual consumers.
Do not reintroduce the removed capability attribute as a stylistic alternative.

Omitted visibility is currently `implicit-public`. Teach explicit visibility
for reusable interfaces, but do not silently change its default to private:
that changes package interfaces rather than merely cleaning source spelling.

## Specialized surfaces to retain under the shared kernel

The general convergence must also preserve specialized admitted type/call
families. Their names alone do not justify another grammar or imply identical
authority:

| Family | Design boundary |
| --- | --- |
| Typed transaction and witness byte views | Keep schema/source bindings, byte bounds and non-escaping lifetimes; an ordinary record with similar fields is not a substitute. See the [context view](../src/ast/context_view.rs) and [witness view](../src/ast/witness_view.rs) type descriptions. |
| Exact/open/deployment handles | Keep interface designation, deployed identity and checked selection distinct. Share ordinary generic type/call syntax rather than adding binding verbs; see the [handle design](CELLSCRIPT_OPEN_HANDLE_DESIGN.md). |
| ZK and opening values | Reuse ordinary witness/type/call syntax. Preserve each declared proof profile's entry placement, fatal verification and parameter restrictions. The [ZK profile](CELLSCRIPT_ZK_PROFILE.md) does not make proof verification a general-purpose boolean or grant Cell lifecycle authority. |
| Temporal values | Retain [typed temporal domains](CELLSCRIPT_0_30_TEMPORAL_DOMAINS.md); a common integer representation does not make absolute/relative or block/epoch/time values interchangeable. |
| External delegation and process calls | Keep the [trusted external contract](CELLSCRIPT_TRUSTED_EXTERNAL_VERIFIERS.md), exact manifest binding and evidence classification. Ordinary call syntax does not remove these obligations. |
| Reserved builder syntax | `launch` rejects in the [expression parser](../src/parser/expressions.rs). Keep it clearly reserved; do not teach it as an executable lifecycle synonym. |

F05's branch composition proposal concerns lifecycle dispositions. It is not a
proposal to permit every proof/delegation operation in arbitrary helpers,
branches or loops. Semantic placement restrictions can be necessary even when
the syntax kernel is shared.

## Artifact configuration decision

The current [artifact validator](../src/artifact.rs) admits two binding/coverage
combinations: group-relative/fixed-group with a layout, or
transaction-absolute/action-defined without one. Authoring several fields for
only two allowed combinations is repetitive, but those fields identify
different verified obligations and must remain separate in the resolved model.

First use existing scaffold/templates and immediate diagnostics to produce a
complete valid declaration. Do not add a new manifest shorthand in this pass.
If real editing evidence still justifies a compact profile later, define a
closed profile expansion, reject contradictory overrides and bind the fully
expanded context into interfaces, receipts, cache and builders. Do not select
a weaker context from the action name or presence of a relation.

## Proposed implementation order and ownership

| Step | Owner | Concrete result and acceptance |
| --- | --- | --- |
| 1. Establish one current reference | Language/docs and tooling | Historical status headers corrected here; follow with a fixture/support-status and snippet audit. Every active entry point links to the v8 grammar; pending proposals are labelled. |
| 2. Close grammar inconsistencies | Parser, formatter and language services | Flow clause termination tests; an explicit prefix-role/signature reference; accepted/rejected syntax and recovery agree. Removing admitted aliases needs the migration work in step 5. |
| 3. Make dispositions compose | AST, typing, lowering, backend and independent checker | Shared statement nodes and checked branch-local pool/retire/fresh; source/CFG occurrence bindings; negative branch/alias/coverage mutations. No new loop ownership promise. |
| 4. Unify Lock entry ownership | Artifact/ABI, frontend, checker and builders | Lock-group artifact context and selection; source order independence; exact protected binding, boolean status and witness behavior; then remove the duplicate declaration route. |
| 5. Migrate canonical authoring | Language, interfaces and upgrade tooling | Explicit Cell ports, prefix-only qualifiers and any braced validity change under one planned source epoch; source-span diagnostics and reviewable migrations; no permanent legacy grammar switch. |
| 6. Reduce resource-model surprises | Type/runtime, LSP and example owners | Hover/diagnostics show backing/bounds; canonical named codecs and budgets; documented and tested stack Vec capacity behavior. Keep codec and allocator contracts distinct. |

Each step is independently reviewable. Do not bundle new capacity semantics,
general heap ownership or a new witness ABI into punctuation cleanup.

## Acceptance corpus and migration contract

Use representative business operations, not just isolated token acceptance:

| Fixture | What the proposed design must make clear | Required negative/change cases |
| --- | --- | --- |
| Token transfer | Input/output roles, complete successor and Lock treatment | Extra group member, wrong destination, changed capacity, altered field. |
| Split and merge | Pool membership, authenticated asset and overflow behavior | Aliased members, foreign asset with identical data layout, overflow, extra output. |
| Continue or retire | One Cell disposition on each accepting branch | Missing branch, double disposal, false/rejecting exit, unsupported loop disposal. |
| Persistent multi-action policy | Artifact exports, explicit tags and shared deployed context | Unknown tag, changed witness order, accidental helper export, ABI mismatch. |
| Lock with reusable helpers | Explicit protected source and module order independence | Wrong protected binding, moving a helper, invalid proof, witness tampering. |
| Receipt settlement | Claim output and business continuity, not generic destruction | Wrong receipt kind, missing settlement output, changed identity policy. |
| Bounded aggregation and migration | Codec limits, scan limits and local allocation are distinct | N+1 elements, malformed codec, exhausted arena, escaped view and unaccounted output. |
| Schema update | `same` expansion and review of added/changed fields | Stale acknowledgement, an implicitly preserved new field, narrowing/widening changes. |

Existing ownership includes [authoring parity](../tests/authoring_parity.rs),
[replacement](../tests/authoring_replace.rs),
[frontend boundaries](../tests/frontend_boundaries.rs),
[entry witness ABI](../tests/entry_witness_abi.rs),
[policy lifecycle](../tests/policy_lifecycle.rs),
[checker successor cases](../tests/artifact_checker/authoring_successors.rs)
and the [syntax combination matrix](../tests/syntax_combo/matrix.toml).
These are the proposed redesign's regression owners; the follow-up checks
actually run for the smaller contract repair are recorded below.

Before removing a spelling, record its complete resolved roles, field policies,
authorization assumptions, accepted/rejected transactions and failure behavior.
Classify every change as spelling-only, semantic tightening, ABI/deployment
change or a newly supported behavior. Preserve wire/hash domains for genuinely
equivalent source changes; update source, cache and consumer identities where
required. A matching semantic hash alone is insufficient equivalence evidence.

A migration may rewrite only cases with a proven mapping. It must stop on
missing policies, source-coordinate changes, ambiguous identity or returned
Cell/value distinctions. Formatting remains lossless and semantics-preserving;
breaking migration is an explicit separate operation.

Run focused checks for changed behavior and the existing gate for the actual
scope: `commit` before ordinary commits/pushes, `backend` before backend
integration claims, and `ci` before merge-readiness claims. This documentation
review makes none of those runtime or readiness claims.

## Follow-up validation

The following checks passed in the isolated worktree on 2026-11-25:

| Check | Observed result |
| --- | --- |
| `cargo test --locked -p cellscript --lib` | 1,230 passed. |
| `cargo test --locked -p cellscript --test fmt_editions --test frontend_boundaries --test grammar_contract --test syntax_compatibility --test edition_contract -- --nocapture` | 50 passed; one explicit fixture-export test ignored. |
| `cargo test --locked -p cellscript-wasm --features wasm` | 34 passed under the native test runner with the WASM feature; not a rebuilt website bundle. |
| `cargo test --locked -p cellscript-tools --bin cellscript-tools repository_checks::tests::` | 18 passed. |
| `CELLC_BIN="$PWD/target/debug/cellc" ./scripts/cellscript_syntax_combo_audit.sh quick` | 98 cases: 60 accepted, 38 rejected, zero failures; seed 20260503. |

The full CI/backend/release gates and a fresh website bundle are not claimed by
these focused checks. Ordinary commit hygiene uses the existing `commit` gate.
