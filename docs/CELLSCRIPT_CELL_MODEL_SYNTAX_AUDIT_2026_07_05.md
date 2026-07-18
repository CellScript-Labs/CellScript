# Cell Model Syntax Audit Integration

**Date**: 2026-07-05
**Scope**: CellScript 0.21.0-rc.1 cell-model syntax fit, with 0.21/0.22
roadmap alignment.
**Input**: Desktop swarm report
`/Users/arthur/Desktop/cellscript-cell-model-syntax-audit-2026-07-05.md`.
**Status**: Integrated design review. This document is not release evidence and
does not claim that any 0.22 feature has shipped.

## Executive Verdict

The audit's main thesis is fair:

```text
CellScript 0.21 is strong as a single-cell resource and transition DSL.
It is not yet a full multi-cell transaction DSL.
```

The current language makes the core cell lifecycle visible through `resource`,
`shared`, `receipt`, `action`, `transition`, `verification`, `consume`,
`create`, `destroy`, `preserve`, `lock`, `protected`, `witness`, and
`lock_args`. That surface matches CellScript's action-centered design.

The gap is the next layer of CKB transaction structure:

- transaction structure handles such as CellDep, HeaderDep, OutPoint, and
  WitnessArgs are mostly low-level source-view/helper surfaces rather than
  first-class source values;
- cell-backed collections do not have a production ownership model;
- most aggregate invariant forms remain metadata-only or runtime-helper-required
  until concrete lowering exists;
- production examples under-teach identity lifecycle primitives;
- some helper and composition patterns are represented indirectly through
  actions or metadata rather than direct syntax.

This is not a contradiction in the shipped 0.21 model. It is a roadmap pressure:
0.22 should make these boundaries more explicit without turning CellScript into
an account-style state language or a theorem prover.

## CKB Baseline Used By This Review

Official CKB documentation models a transaction as a structure containing
`cell_deps`, `header_deps`, `inputs`, `witnesses`, `outputs`, and
`outputs_data`. It also treats `WitnessArgs` as a common Molecule convention
with `lock`, `input_type`, and `output_type` fields, and CellDep as an
OutPoint plus `dep_type`.

That baseline supports the audit's framing: a language that claims CKB-native
transaction expressiveness should let authors see which transaction surfaces
are compiler-checked, builder-supplied, runtime-read, metadata-only, or
chain-evidence-dependent.

## Calibration Against The Live Repo

The Desktop audit is useful, but several findings need correction before they
become repository guidance.

### Corrected Finding: Witness Parameters Already Exist

The audit says action signatures cannot declare `witness` parameters. That is
not current repo truth.

`parse_param_source_prefix` accepts both `witness` and `lock_args`, and the type
checker allows `witness` parameters on actions and locks. `lock_args` remains
lock-only, fixed-width script-args syntax.

Therefore the real gap is not "action has no witness parameters." The real gap
is:

- no first-class `CellDep` or `HeaderDep` parameter source;
- no source-level `OutPoint` value type;
- no source-level `WitnessArgs` envelope value;
- no explicit action-level transaction-footprint declaration tying these
  surfaces together.

### Corrected Finding: WitnessArgs And OutPoint Are Not Absent

The audit says `WitnessArgs` and `OutPoint` are completely absent. That is too
strong.

The backend and stdlib surfaces include low-level WitnessArgs and OutPoint
helpers, source views, and metadata/runtime access records. The source language
does not yet expose them as first-class typed values.

Use this wording instead:

```text
CellScript has low-level CKB SourceView, WitnessArgs, and OutPoint helper
coverage, but lacks first-class source-level value types and action-footprint
syntax for these transaction surfaces.
```

### Corrected Finding: Preserve Except Was Intentionally Rejected

The audit treats missing `preserve except` mainly as a usability gap. The repo
governance rationale is stronger: blacklist-style preservation can hide schema
drift when a type gains a field. CellScript currently prefers explicit
preserved-field whitelists because they are audit-visible.

The issue is not "restore `preserve except`." The better question is whether a
new output-construction form can reduce boilerplate while still expanding into
explicit preserved-field metadata.

Acceptable direction:

```text
create next: T = from previous with_overrides {
    changed_field: expr
}
```

or an equivalent form, only if the formatter, metadata, ProofPlan, and docs
materialize the resulting full preserved-field set.

### Corrected Finding: Count Production Examples Precisely

The Desktop report says "9/9 production examples" in multiple places. The root
example set currently includes more than nine files depending on whether
`registry.cell` and mirrored package examples are counted. The underlying
finding remains true for the canonical top-level production examples checked
in this review: identity lifecycle primitives and scoped invariants are taught
mainly through `examples/language/*`, not the production tutorial examples.

Use exact file lists in future reports instead of a bare `9/9` count.

## Core Findings To Preserve

### 1. CellScript Is Still Mostly Single-Cell Native

Cell lifecycle syntax is real and useful. The compiler tracks linear ownership,
state transitions, consumes, creates, destroys, and named outputs. That is the
language's strongest part.

The missing layer is multi-cell transaction ergonomics:

- no production `Vec<Resource>` ownership model;
- no `consume_each` or `create_each`;
- no source-level bounded cell set type;
- no typed transaction view that authors can use without raw `u64` source
  indexes or helper calls.

This matches `docs/CELLSCRIPT_LINEAR_OWNERSHIP.md`: generic ownership of
collections of linear cells is not a production feature and would require
`consume_each`, typed destructuring, membership proofs, and schema-level
ownership witnesses.

### 2. Aggregate Invariants Need Typed Targets Before More Syntax

0.21 promotes one narrow aggregate family: xUDT-style group amount conservation
when the source and action shape match the helper contract. Other aggregate
primitives still report metadata-only or runtime-helper-required states.

The deeper issue is not just missing helpers. Current aggregate targets are
still string-like enough that multiple compiler phases must parse or interpret
the same conceptual target. 0.22 correctly identifies typed aggregate targets
as a syntax-integrity prerequisite.

The minimum internal model should be a closed representation:

```text
SourceView(Input | Output | GroupInput | GroupOutput | CellDep)
AggregateTarget {
    source_view,
    type_name?,
    field_path?
}
```

That model should be shared by aggregate invariants, bounded quantifiers,
ProofPlan reads, cost reporting, and helper selection.

### 3. Evidence Tiers Are The Right Bridge Between 0.21 And 0.22

The repo already distinguishes compiler evidence, artifact evidence, builder
evidence, and chain evidence. 0.22 should deepen that distinction instead of
pretending every declaration becomes on-chain enforcement.

The durable evidence tiers are:

- `checked-static`;
- `checked-runtime`;
- `runtime-helper-required`;
- `builder-evidence-required`;
- `metadata-only`;
- `chain-evidence-required`.

The rule should be strict:

```text
Metadata-only is acceptable only when the feature contract says it is
metadata-only. Production-facing gates must reject metadata-only obligations
when the source or docs claim executable enforcement.
```

### 4. The Curriculum Lags The Language

Identity lifecycle primitives exist: `identity(ckb_type_id)`, `field(...)`,
`script_args`, `singleton_type`, `create_unique`, `replace_unique`, and explicit
destruction policies. They are mostly taught in language/reference examples,
not production examples.

This creates a false impression that authors should hand-roll identity with
`type_hash()` checks or custom pool identifiers.

The fix is mostly documentation and example work:

- teach `create_unique` in a production-style token or NFT lifecycle;
- show `identity(ckb_type_id)` for a canonical unique cell;
- show `identity(script_args)` where script args are actually the identity
  source;
- explain which identity obligations are checked locally, runtime-required,
  builder-required, or chain-evidence-dependent.

### 5. Action Composition Must Be A Deliberate Choice

The current action model is intentionally transaction-shaped. It is safer to
keep action composition conservative than to pretend that one action's cell
operations automatically become another action's sub-transaction.

There are two coherent choices:

1. Reject action-to-action calls when the callee has cell effects, with a
   diagnostic telling authors to inline the lifecycle shape or call a pure/read
   helper.
2. Implement true composition by merging consumed/created/destroyed outputs,
   named output bindings, verifier obligations, metadata, and scheduler effects.

The first choice is much smaller and aligns with 0.22's callable-effect
signatures. The second choice is a larger transaction-composition feature and
should not slip in as a cleanup.

## 0.21 And 0.22 Planning Alignment

There is no fatal contradiction between the 0.21 and 0.22 plans if the draft
status is respected.

0.21 is an implementation checkpoint. Its own roadmap says release claims still
depend on gate evidence, and its aggregate work starts by promoting a narrow
xUDT group amount shape.

0.22 is a draft, pending Nervos Talk adoption. Its acceptance criteria are
future-tense requirements. It should not be cited as evidence that parser,
type checker, lowering, metadata, LSP, tests, or gates already implement a
feature.

### Consistent Parts

| Topic | 0.21 position | 0.22 position | Verdict |
|---|---|---|---|
| Language center | Action plus transition plus verification | Same center, sharpened by effects and evidence tiers | Consistent |
| Aggregate invariants | First helper-backed slice only | Typed targets, bounded quantifiers, evidence tiers | Consistent if staged |
| ProtocolGraph | Derived view, not consensus truth | Metadata/UX role view, not authorization evidence | Consistent |
| TemplateLayout | Metadata-only in 0.21 RC | Evidence-tier clarity before claims | Consistent |
| Chain evidence | Compiler evidence is not CKB acceptance | Same boundary explicitly retained | Consistent |

### Planning Tensions To Resolve

#### Tension 1: "Block Release" vs 0.21 RC Scope

The Desktop audit labels several large items as block-release:

- first-class CellDep/HeaderDep/WitnessArgs/OutPoint/Tx values;
- `consume_each` / `create_each` / `Vec<Resource>`;
- generalized aggregate lowering.

Those are valid roadmap gaps, but they are not reasonable blockers for the
0.21 RC as described in the current 0.21 roadmap. They should be reclassified:

- 0.21: document the boundary and keep unsupported claims honest;
- 0.22 P0: typed source-view/aggregate target foundations and evidence tiers;
- post-0.22 or explicit design slice: cell-backed collection ownership.

#### Tension 2: Metadata-Only Rejection vs Known Metadata-Only Surfaces

0.22 says production-facing gates should reject metadata-only obligations when
the feature contract claims executable enforcement. This does not mean every
metadata-only record is forbidden.

The contradiction appears only if the docs say an invariant is enforced while
ProofPlan reports `gap:metadata-only`. The fix is wording and gates:

- metadata-only observations remain allowed when honestly labelled;
- executable claims require generated checks or helper coverage;
- strict production modes reject unsupported executable claims.

#### Tension 3: Bounded Quantifiers Depend On Typed Targets

0.22 proposes bounded source-view quantifiers as P1. They are only safe after
the P0 typed aggregate target model exists. Otherwise quantifiers repeat the
same stringly target problem that the audit criticizes.

Implementation order:

1. typed source views and aggregate targets;
2. shared ProofPlan/cost/evidence model;
3. bounded `forall` and `count`;
4. nested quantifiers and joins only after precise cost modeling.

#### Tension 4: Audit Wants Source Qualifiers, 0.22 Marks Them As Discussion

The Desktop audit suggests broad action signature additions. The 0.22 roadmap
explicitly says action source qualifiers and related sugar are discussion
candidates, not baseline commitments.

This is not a contradiction. It is a governance decision:

- do not silently fold these into 0.22;
- propose them separately;
- start with typed handles and metadata footprints rather than broad new
  grammar.

#### Tension 5: `env::tx()` Is Too Large For The First Fix

The audit proposes `let tx = env::tx()` returning a whole transaction object.
That is coherent as a long-term model, but it risks large syntax, wasm size,
and runtime surface expansion.

Prefer a smaller first cut:

```text
source::input(i) -> SourceView<Input>
source::output(i) -> SourceView<Output>
source::cell_dep(i) -> SourceView<CellDep>
source::header_dep(i) -> SourceView<HeaderDep>
ckb::input_out_point(view) -> OutPoint
witness::args(view) -> WitnessArgs
```

The exact spellings can differ, but the important point is typed handles before
a large transaction object.

## Recommended Modification Plan

### Tightened Syntax Adoption Rule

Do not treat the audit gaps as a request for a complete transaction-builder
DSL. The safer 0.22 direction is:

```text
typed transaction-view handles
+ typed aggregate targets
+ bounded multi-cell resource sets
+ honest evidence tiers
```

Every proposed syntax form must answer three questions before it becomes part
of the language surface:

1. Which CKB transaction source does it read?
2. Which linear cell obligation does it create, consume, preserve, destroy, or
   explicitly avoid?
3. Which evidence tier discharges the obligation?

If a form cannot answer those questions, it stays a discussion candidate rather
than a roadmap commitment.

### P0: Correct The Audit And Lock The Boundary

1. Replace overstated claims with calibrated language:
   - low-level helper exists;
   - first-class source value is missing;
   - evidence tier is metadata-only, runtime-helper-required, or checked.
2. Remove the false claim that action `witness` parameters are absent.
3. Stop using the bare `9/9` production-example count.
4. Reclassify "block-release" findings as roadmap priorities unless they block
   a specific release claim.

### P0: Typed CKB Source And Aggregate Foundations

Implement a typed internal source-view model shared by:

- aggregate invariant reads;
- future bounded quantifiers;
- runtime access metadata;
- ProofPlan reads;
- cost and cycle reporting;
- helper selection.

Do not begin with a broad `Tx` object. Begin with typed handles that map to the
existing CKB source views and helper paths.

The first surface should keep transaction observation separate from linear
resource ownership:

```cellscript
let input = ckb::input<Token>(0);      // read-only transaction view
let token = consume Token from input;  // linear resource operation
```

Candidate handle types include:

```text
InputView<T>
OutputView<T>
CellDepView
HeaderDepView
WitnessArgsView
OutPoint
ScriptView
ScriptHash
```

Those handles are views, not resources. They should not imply consumption,
creation, or transaction-builder authority.

### P0: Evidence-Tier Enforcement

Make every new or promoted feature choose one evidence tier:

- if checked by compiler, emit `checked-static`;
- if backed by generated verifier code, emit `checked-runtime`;
- if helper-known but not emitted for this entry, emit
  `runtime-helper-required`;
- if builder material is required, emit `builder-evidence-required`;
- if retained only for audit, emit `metadata-only`;
- if dry-run/commit/cycle/capacity evidence is required, emit
  `chain-evidence-required`.

Production-facing gates should reject mismatches between claimed enforcement
and actual evidence tier.

### P1: Identity Curriculum

Update production-style examples before adding new identity syntax:

- introduce one `create_unique` production example;
- show `identity(ckb_type_id)` and `identity(script_args)` in realistic flows;
- explain `type_hash()` as a low-level identity value, not the default
  curriculum path.

This is high leverage because it changes how authors learn the language without
destabilizing compiler surfaces.

### P1: Callable Effects And Helper Cleanup

Adopt 0.22's declared effect direction for ordinary `fn`:

```cellscript
#[effect(Pure)]
fn isqrt(n: u64) -> u64 {
    ...
}
```

Then reject pure helpers that call mutating, creating, or destroying callees,
including across package boundaries when summaries are available. This is a
better first step than adding a new `proof fn` keyword immediately.

### P1/P2: Cell-Backed Collections

Do not expose generic `Vec<Resource>` first. Start with a bounded ownership
surface:

```text
BoundedCellSet<T, N>
consume_each item in set { ... }
create_each item in plan { ... }
```

The first implementation should require:

- finite cardinality;
- explicit source view;
- typed membership proof or builder evidence;
- exact consumption or creation obligation;
- ProofPlan cardinality and vacuous-satisfaction metadata.

Only after that should generic `Vec<Resource>` be considered.

The first public syntax should prefer explicit bounded types:

```text
BoundedCellSet<T, N>
BoundedList<T, N>
```

`BoundedCellSet<T, N>` is for transaction-backed cell sources. `BoundedList<T,
N>` is for bounded witness/static/pure-computation plans. `consume_each` should
only accept bounded cell sets of resources. `create_each` can consume a bounded
plan only when the produced output cardinality and builder evidence are visible
in ProofPlan.

### P2: Boilerplate Reduction Without Hidden Preservation

If output-construction sugar is added, it must preserve audit visibility.

Acceptable properties:

- no blacklist-only `preserve except` semantics;
- metadata expands the full preserved-field set;
- formatter makes generated preservation visible or inspectable;
- schema drift changes the emitted preservation plan and test snapshots.

## Revised Priority Table

| Priority | Work item | Why |
|---|---|---|
| P0 | Calibrate the audit report and docs | Prevent false release blockers and stale claims |
| P0 | Typed transaction-view handles | Exposes CellDep/HeaderDep/OutPoint/WitnessArgs without a whole `Tx` object |
| P0 | Typed source-view and aggregate target model | Foundation for quantifiers, helpers, ProofPlan, and cost |
| P0 | Evidence-tier enforcement | Prevent metadata-only claims from looking enforced |
| P1 | Identity lifecycle in production examples | Fix curriculum lag without compiler churn |
| P1 | Callable effect signatures for `fn` | Cleans helper/action confusion and supports borrow/validity predicates |
| P1 | Bounded quantifiers over typed source views | Adds finite multi-cell predicates after target typing exists |
| P1/P2 | Bounded cell-backed collection ownership | Large feature; needs explicit linear membership model |
| P2 | Output construction sugar | Useful only if preservation remains explicit in metadata |

## Non-Goals For This Integration

- Do not block 0.21 RC on broad multi-cell DSL features.
- Do not claim that 0.22 is accepted or implemented.
- Do not turn metadata-only observations into executable claims.
- Do not hide CKB transaction structure behind account-style mutation.
- Do not add a whole `env::tx()` value before typed source-view handles exist.
- Do not add generic `Vec<Resource>` without verifier-backed ownership.
- Do not let `assert_*` or `enforce_*` names describe metadata-only records.
- Do not allow effectful action-to-action calls as ordinary helper calls.

## Final Position

The audit is directionally correct and should influence the roadmap. Its most
valuable contribution is the distinction between:

```text
single-cell lifecycle syntax that is first-class today
vs.
multi-cell transaction structure that remains helper/metadata/builder heavy
```

The modifications above turn the audit from a broad complaint into an
implementation sequence:

1. correct overstatements;
2. strengthen typed source-view foundations;
3. make evidence tiers enforceable;
4. teach existing identity primitives;
5. add bounded transaction-set syntax only after the type/evidence model is
   ready.
