# CellScript 0.33.0-alpha.2: accepted audit follow-up

Status: implementation in progress; full integration and release acceptance are pending.
Date: 2026-10-29.
Review baseline: `9fa44d86b86a3ec9b0415719956715890a73cefe`.

This backlog records the accepted technical-debt findings and the nested-view
design review. Every unchecked item remains open. It does not announce a release,
authorize publication, change a version, or claim that a gate has passed. The
publication exception for alpha.1 does not apply to alpha.2. General Registry
operations are outside this backlog except the explicitly authorized post-alpha.1
audit corrections recorded below; its current interface reader and replacement
comparison must agree with the A2-08 contract. Do not trigger TAC or Daybreak.

Delivery policy: affected changes make a clean break. There are no existing-user
migration constraints. Remove superseded implementations, old readers, aliases,
compatibility flags, and transition adapters in the same delivery. Do not leave
migration cleanup as future work or retain competing semantic authorities.

Priorities describe implementation order, not vulnerability severity. Large
files, panic-related search counts, and missing TODO comments are investigation
signals; they are not evidence of a correctness defect by themselves.

## P1: correctness and interactive behavior

### A2-01: diagnose incomplete pool assignments before lowering

Evidence: [native frontend](../../src/frontend/next.rs) calls `lower_pool_body`
before `validate_surface`; lowering expects every output assignment to exist.

- [x] Validate complete, ordered output assignments before consuming them, or
  return a source diagnostic from lowering instead of using the unchecked
  `expect`. Preserve the remaining pool validation rules.
- [x] Cover missing assignments with one and multiple outputs, duplicate and
  unknown output names, and valid complete assignments.
- [x] Confirm ordinary parsing, diagnostic parsing, and editor diagnostics reject
  malformed input without unwinding. Preserve useful field/output spans.

### A2-02: reject unterminated ordinary strings and block comments

Evidence: [lexer](../../src/lexer/mod.rs) currently reaches EOF without reporting
an error for an ordinary string, an escape at EOF, or an open block comment.

- [x] Return explicit diagnostics for all three forms, including nested block
  comments with an unmatched opening delimiter.
- [x] Preserve valid escaped strings, closed nested comments, and line comments
  ending at EOF. Check both supported ordinary-string quote forms.
- [x] Test lexer results and frontend diagnostics, including an otherwise valid
  module followed by an unterminated block comment.
- [x] Locate errors at the relevant opening delimiter or incomplete escape;
  retain correct byte offsets and line/column positions for Unicode input.

Focused validation: `cargo test --locked -p cellscript --test frontend_boundaries`
passes all three tests, including ordinary/recovery parsing, metadata and LSP
diagnostics, both quote forms, nested comments, Unicode spans and valid controls.
Pool lowering now returns `Result`; the two input-reachable `expect` calls are
removed. Stable source diagnostic identities are documented in
[compiler error codes](../CELLSCRIPT_COMPILER_ERROR_CODES.md). Full acceptance
for the complete alpha.2 change remains tracked under A2-14.

This checkpoint does not close byte-string EOF handling. A later independent
review reproduced both an unclosed byte string and an escape at byte-string EOF
being accepted by real `cellc metadata` commands. Those forms remain a known
unfixed frontend issue; they must not be counted as covered by the ordinary-string
tests above.

### A2-03: make LSP analysis responsive and revision-aware

Evidence: [document synchronization](../../src/lsp/server.rs) performs synchronous
metadata compilation while holding the shared server lock. Separately,
[role references](../../src/lsp/cell_views.rs) repeatedly resolve bindings and
may reload workspace information inside one request. Reference queries also hold
the shared server lock. This is the highest-priority interactive usability issue.

The audit measured the following single-query times with the actual Cargo release
profile, imported record fields, and 16 additional modules:

| Source bytes | Role uses | Returned references | Query time |
| --- | --- | --- | --- |
| 3,556 | 32 | 33 | 0.835 seconds |
| 6,468 | 64 | 65 | 3.883 seconds |
| 12,404 | 128 | 129 | 20.043 seconds |

These are observations on one host, not universal latency guarantees. The scaling
agrees with repeated prefix analysis and workspace reconstruction; the baseline
is not an acceptable responsiveness target.

- [x] Separate document mutation, snapshot creation, background analysis, and
  diagnostic publication. Keep compilation and workspace I/O outside the global
  lock and off Tokio executor workers.
- [x] Bind results to document and dependency revisions. Discard stale results;
  preserve unsaved dependency overlays and edition-specific parsing.
- [x] Build reusable semantic/binding indexes for references, field resolution,
  hover, and completion instead of repeatedly scanning statement prefixes and
  reconstructing workspace state.
- [x] Add cancellation or bounded coalescing for rapid edits. Preserve current
  error recovery and navigation on incomplete source.
- [x] Record latency and repeated-work measurements for local and imported
  schemas, growing reference counts, and growing workspaces; verify unrelated
  requests remain responsive while analysis runs.
- [x] Retain a reproducible regression fixture for the 12,404-byte, 128-role-use,
  16-additional-module case and compare before/after release-profile results.
  Check returned locations as well as time; prevent repeated prefix/workspace
  work from restoring the observed growth pattern.

The existing [artifact cache](../../src/lib.rs) reuses results when source inputs
are unchanged. It is not an incremental semantic engine for changed documents.
Moving work to `spawn_blocking` alone does not remove repeated analysis or stale
result races. The nested-type work below is a separate change.

Implementation checkpoint: [session ownership](../../src/lsp/session.rs) keeps
snapshot compilation and workspace I/O outside the server lock, coalesces edits
through one analysis worker and rejects stale publications/results. The shared
[view index](../../src/lsp/view_index.rs) visits statements once and retains
bindings, tokens and one resolved workspace per revision. Tests verify exact
reference ranges, index reuse, same-size disk edits, invalid/unsaved dependency
overlays and responsiveness while another query is blocked.

On 2026-10-30, the two ordinary [LSP behavior regressions](../../tests/lsp_responsiveness.rs)
passed, including 64 rapid edits and stale-version rejection. The explicit Cargo
release-profile run passed two measurement tests covering 19 cases: local and
imported records, 32/64/128 uses, and 1/16/64 extra modules. For the exact
12,404-byte/128-use/16-module fixture, the first query took 18.528 ms and its
repeat 13.910 ms, returning all 129 locations. The scaling run measured
4.676/8.843/18.118 ms for 32/64/128 imported uses with 16 extra modules. Its
smaller fixtures are 4,148 and 6,900 bytes, so only the 12,404-byte row is an
exact-size comparison with the audit. Timings measure reference queries after
document analysis, not total edit-to-diagnostic latency, and are host observations
rather than deadlines. Final-source integration remains under A2-14.

The v148 release-profile repeat (`lsp-final-v148-release.log`) passes the same
two measurement tests and all 19 cases. The exact 12,404-byte/128-use/16-module
fixture returns 129 locations in 7.114 ms initially and 5.802 ms on repeat. This
is a fresh host observation of reference-query time after analysis, without a
fixed timing assertion or an edit-to-diagnostic latency claim.

## P1: explicit compiler contracts

### A2-04: retain structured nested-view types through IR

Evidence: [nested type descriptions](../../src/nested_values.rs) already provide
`NestedType`, but derived views are formatted into `IrType::Named` and parsed
again by consumers. [Nested codegen](../../src/codegen/nested.rs) reconstructs
record shapes from string-keyed layout tables.

Start with `NestedValues`, `NestedBytesView`, and `NestedRecordView`. Reuse the
existing description and checker contract types rather than introducing a
parallel hierarchy or a compiler-wide `TypeId` system.

The [boundary characterization](../CELLSCRIPT_NESTED_VIEW_CHARACTERIZATION.md)
records shape-alias parity and a native/checker/VM case with same-name,
different-owner records and different fixed widths. These cases pass through
the structured IR; final integration acceptance remains under A2-14.

#### Checkpoint 0: characterize identities and current output

- [x] Inventory source parsing, type checking, generics, lowering, metadata,
  codegen, builders, and LSP consumers. Record which boundaries still legitimately
  accept source text.
- [x] Capture current canonical renderings and metadata/interface/receipt
  projections for direct types, named `data_shape` aliases, imported aliases,
  and generic substitutions before changing representation.
- [x] Preserve the existing [interface test](../../src/interface.rs) requiring
  renamed shape aliases and their expanded forms to have the same interface
  identity. Identify any external fields that intentionally retain spelling.
- [x] Construct minimal cross-owner same-name record cases through the resolver,
  external IR merge, metadata, codegen, and independent checker. Establish
  whether a collision is reachable before recording a confirmed bug.

String-keyed layout maps alone do not prove an exploitable or accepted collision:
import aliases, origin records, and downstream rejection must also be checked.
Valid references to distinct owners should remain distinguishable; incorrect
owner substitution must reject. Do not reject every same-name declaration.

#### Checkpoint 1: centralize descriptions without changing IR

- [x] Centralize nested parsing, bound validation, shape admission, and canonical
  rendering around the existing `NestedType` representation.
- [x] Distinguish byte elements from resolved record elements explicitly, for
  example `Element::Bytes` and `Element::Record(RecordRef)`.
- [x] Resolve record references to the defining declaration and admitted ordered
  layout. Reuse descriptions within the correct compilation scope; include
  owner, generic substitution, and dependency revision where applicable.
- [x] Keep the existing IR representation for this checkpoint. Demonstrate
  unchanged behavior and output projections in an independently reviewable
  source checkpoint before introducing a structured IR variant.
- [x] Measure repeated parsing/resolution to verify reuse. Avoid a global cache
  keyed only by spelling or a cache that can outlive dependency changes.

Checkpoint 1 implementation, 2026-10-30: the same generic `NestedType` description
serves parsed source and resolved elements. Compilation-scoped descriptions cache
parsing and admission; `Element::Bytes` and `Element::Record(RecordRef)` retain
owner, concrete specialization and ordered fixed/payload fields. Import spelling
does not participate in record equality/hashing. Four description tests and one
type-checker test cover 128 repeated queries, alias equality, distinct owners,
changed dependency layouts, malformed shapes and generated canonical round trips.
All 32 native nested regressions, eight resolver tests and all-target clippy pass.
Two generic direct/shape-alias programs retain identical ELF, interface, typed
semantics and complete receipts at the same source locations. The immutable local
checkpoint and parity receipt are under `target/alpha2-work/effect-facts-migration/`;
no separate intermediate Git commit is claimed. An immutable source/check receipt
records this implementation checkpoint.
The zero-behavior-change checkpoint preceded the structured IR cutover below;
the final combined source must still pass all A2-14 gates.

#### Checkpoint 2: migrate the three nested IR types together

- [x] Add structured nested-view representation to `IrType`, retaining element
  identity, admitted layout, and all declared bounds.
- [x] Convert migrated IR, codegen, and metadata consumers together. Derived
  index and field results must remain structured instead of formatting and
  reparsing a type name.
- [x] Keep any temporary conversion at a documented boundary with an explicit
  removal condition. Do not leave a permanent string and structured type system
  competing for authority.
- [x] Replace redundant compiler-side record reconstruction with the admitted
  layout and remove the associated cross-function `expect` where it becomes
  unnecessary. Keep independent checker validation intact.
- [x] Advance `ARTIFACT_CACHE_VERSION` for the semantic migration. Use the next
  unused generation and record it in the implementation receipt. The native
  disposition cutover allocated v139 and resolved-call binding fixes allocated
  v140, the initial fact-only protocol cutover allocated v141, and its source-evidence
  closure allocated v142, and resolved callable identities allocate v143; do not
  reuse them. The nested representation migration allocated v144,
  `0.33-structured-nested-views`.
- [x] Audit `Eq` and `Hash` consumers and expected types across lowering,
  generics, metadata, and editor tests.

#### Required invariants and tests

- [x] Preserve transparent `data_shape` alias equivalence. Preserve nominal
  record-owner identity and the existing distinction between hash domains.
- [x] Keep source spelling in separate diagnostic/source information when
  needed. It must not accidentally participate in semantic `Eq`/`Hash` or change
  canonical interface identities.
- [x] Test the new renderer against the legacy canonical renderer over generated
  valid descriptions, aliases, whitespace variants, and boundary values. Test
  semantic parse/render round trips separately from exact canonical byte parity.
  Finite property tests are evidence, not proof for every possible program.
- [x] Recognized malformed nested types must produce errors, not fall through
  as unrelated named types.
- [x] Compare ELF and relevant metadata/interface/receipt projections for the
  same source and options. Explain each intentional change; do not refresh
  fixtures merely to make differences disappear.
- [x] Preserve negative cases for wrong owners, changed bounds, malformed
  records, out-of-range access, borrowed-view escape, and changed source identity.
- [x] Keep buffer provenance, runtime indices, decoding evidence, and borrow
  lifetime checks on values/operations. A resolved type is not runtime proof.
- [x] Keep internal references out of serialized `CompileMetadata`. Add a
  serialization regression covering native and browser metadata. Internal IDs
  must never enter interface hashes, receipts, or witness protocols.
- [x] Add a scoped source-policy check for migrated IR/codegen paths: new string
  construction and reparsing must be confined to the designated boundary.
  Parser, renderer, diagnostics, and tests may legitimately mention these names.

Structured-IR checkpoint: `IrType::Nested` carries the same resolved descriptions
through variables, fields, ABI projections and derived views. Codegen consumes
admitted record fields and explicit pointer/length classifications; redundant
record reconstruction and its two width expectations are removed. Unresolved
named nested IR rejects with E2105. Three IR tests, all 32 existing native nested
regressions, the new generic/shape-alias VM regression, 30 browser-profile host
tests, the source-policy guard and 21 binding/effect/interface regressions pass.
The same-location v143-to-v144 probe preserves complete ELF, interfaces, typed
semantics and complete receipts. See the characterization document and
`nested-generic-native/structured-ir-parity.json`. Current-only wire readers and
independent proof obligations remain separate. Final backend/integration gates
remain open under A2-14; this checkpoint is not a release or merge-readiness claim.

Subsequent Cell views and selection types should reuse the proven pattern, but
are separate scope decisions. A selected role must retain its defining catalog,
complete role ordering, schema binding, and checked ordinal. Keep the independent
checker independent. Do not merge stack vectors, arena-owned collections, and
borrowed views merely because they all carry lengths.

### A2-05: make runtime helper availability and dispatch exhaustive

Evidence: [runtime ABI inventory](../../src/codegen/runtime_contract.rs),
[helper emission](../../src/codegen/runtime.rs), call recognition, and helper
gathering require coordinated manual updates. Previous success stubs demonstrate
the failure mode. H1's specific omissions have been fixed, and the baseline
inventory found no additional unclassified helper. A generic `li a0, 0; ret`
fallback still exists, so a future missing dispatch arm can repeat the failure.

- [x] Introduce a typed helper identity and explicit availability/return contract
  consumed by gathering, call emission, and runtime emission.
- [x] Make emitter dispatch exhaustive, or mechanically require every inventory
  entry to have an implementation or a specific deferred failure.
- [x] Replace generic success fallbacks for unsupported helpers with an explicit
  compile rejection or specific runtime failure appropriate to policy.
- [x] Derive the all-helper test inventory from structured declarations instead
  of mining Rust source string literals.
- [x] Retain machine-level assembly/ABI checks, production-policy rejection,
  ignored-result failure tests, and the existing IPC boundary regressions.
- [x] Add a regression showing that an inventory entry without an implementation
  or an explicit deferred classification cannot silently acquire a success body.
  Adding a helper must require an explicit availability decision.

The checker-owned [typed registry](../../crates/cellscript-artifact-checker/src/runtime_contract.rs)
requires an explicit availability and return ABI for each helper. Exhaustive
surface/IPC emitter matches and the independent fact catalog have no missing-arm
success default. The [machine inventory](../../src/codegen/runtime/stack_audit.rs)
derives names from `RuntimeHelper::ALL`, checks every emitted implementation and
return contract, and inspects decoded instructions. Unknown-helper rejection
covers both artifact formats; existing deferred/ignored-result and IPC ABI
regressions remain. The current 1,120-test compiler library run and focused
runtime/bundle checks pass. The v142 backend checkpoint completed the 1,120
library tests and 42 allocated-runtime tests; it was interrupted when the v143
resolved-owner fix changed the candidate. Final acceptance remains under A2-14.

The earlier v146 availability review found bare named `spawn` still labeled implemented
while returning ZK error 76. Its status stub is removed: the typed registry,
compiler and checker then agreed on deferred feature `ckb-bare-spawn-deferred`,
non-returning error 88 and production rejection. Actual CellDep transports keep
their existing contracts. Seven semantic tests, including direct/discarded/helper
VM failures under both optimization configurations, 24 fact-contract tests and
four decoded runtime-inventory tests pass. Same-spelled user functions retain
their source binding. Cache v146 supersedes v145; the interrupted v145 backend
log is retained and is not a passed gate.

#### Bare-spawn source boundary follow-up — 2026-10-31

The earlier v146 abort-only path is superseded. Bare named `spawn` is rejected
with E1021 during semantic checking, including metadata-only analysis, either
edition, discarded results, unreachable branches and uncalled functions. The
public IR API rejects it as well. Source definitions, imports and aliases are
resolved first; a user function named `spawn` remains callable. Remove the
runtime helper, deferred feature and error-88 emitter rather than adding a
runtime `Rejected` availability. CellDep spawn/exec retain their separate
transport and checker contracts. Cache v150 excludes the earlier artifacts.

Owner: compiler frontend/IR, runtime/checker and editor maintainers (A2-05/06/12).
Acceptance requires native ASM/ELF and both-policy rejection, metadata/WASM/LSP
diagnostics, resolved-source positive controls, unknown retired-helper rejection,
current generated surface and the commit gate. Follow-up validation is recorded
separately from the completed ccc97035 backend/CI checkpoint; those earlier
receipts do not certify this changed source.

Validation passed for this follow-up: compiler library 1,137; standalone checker
library 85; WASM crate 31; source/binding/IPC 21; checker effect mutations 5; LSP
source-boundary regression 1; distributed browser bundle 82. The commit gate
also passes. The canonical browser artifact is 1,794,715 raw / 678,389 gzip bytes,
SHA-256 `76387cd258a7466d63bb31e24628b844fa8a3489e1d4a36cbd47c37b33e506bb`.
Host tests use opt-level 1 with debug assertions and overflow checks enabled,
incremental disabled and two build jobs. Full backend/CI were not rerun for this
follow-up. Logs and the incremental review patch are retained under
`target/bare-spawn-retirement/`.

The remaining three deferred families retain errors 66/86/87 and production
rejection. Before implementing or retiring any one of them, the owning
frontend/runtime/checker maintainers must identify the legacy API's distinct
purpose, exact binding/input domain, difference from the existing typed Since,
actual occupied-capacity or zero-lock signing path, and positive/hostile VM plus
independent-checker acceptance criteria. A deferred label is not a delivery
promise. See the [current inventory and distinctions](../CELLSCRIPT_EFFECT_CONTRACTS.md).

#### Current-foundation audit follow-up — 2026-10-31

This follow-up implements the confirmed v150 audit findings and bounded cleanup:
byte-string EOF diagnostics E1022/E1023, unknown scheduler hint E1024, nominal
selection aliases with catalog-bound executable views, an explicit checked
bundle comparison path, native protocol-builder constant generation, shared
WitnessBytesView syntax/vectors with independent admission, edition-routed
formatter idempotence, checker ISA/source-map module extraction, and arithmetic
ownership for both mixed and general u128 paths. Cache v151 excludes the earlier
source-boundary results. New regression suites own their behavioral contracts.

Declaration interface digests deliberately retain the existing source-spelling
contract. `interface-diff --checked` verifies complete source-contract bundles
and compares qualified nominal projections; it accepts equivalent aliases and
rejects a distinct same-width owner. The report does not certify behavior or
deployment. No old receipt reader or digest translation is introduced.

Shared syntax and closed wire constants are trusted descriptions. Source scope
resolution, inference/checker reconstruction and machine admission remain
independent; conformance vectors and source policy prevent private copies of
WitnessBytesView parsing. Renderers with display/source/wire responsibilities
retain explicit format contracts. The full Type/monomorph representation and
native proposal packaging remain separately designed migrations; line-count
reduction is not evidence that either migration is complete.

The first complete v151 backend attempt stopped at the recorded anchor
ProtocolBundle hash. Restoring only the earlier 86/87 diagnostic text and removed
error-88 catalog entry exactly reconstructs the old hash. The refreshed hash binds
the current metadata and generated-builder catalogs; all four ELF identities,
transaction bytes, fixed resource measurements and rejection assertions remain
unchanged. The failed run and field-level reconstruction are retained alongside
the final acceptance receipts.

The next full run passed the seven business-corpus cases and 226 checker
integration tests, then exposed an obsolete bare-spawn example. Replacing it
with endpoint cleanup and executing the new VM regression revealed a separate
pipe call ABI defect: tuple reception consumed a0/a1 instead of checking a0 and
receiving the descriptors from a1/a2. The repair retains the helper ABI, closes
the source-call boundary and advances caches to v152. The example and playground
now describe only pipe creation and checked closure, with no invented child or
single-process data roundtrip. A clippy-only redundant Copy clone in the earlier
regression suite is also removed.

Validation receipts for this follow-up live under `target/base-debt-fixes/`.
The earlier ccc97035 full backend/CI receipts and v150 focused receipt retain
their own source identities. Final backend/CI acceptance must bind one unchanged
clean v152 candidate, with its website bundle and all submodule revisions.

The subsequent review identified an external-deletion ordering defect in the
earlier Registry retention work: deleting object bytes before committing the
logical tombstone could leave a visible row after a failed commit. Cleanup now
commits an irreversible tombstone first and retains the rejecting identity for
idempotent retry after deletion or acknowledgement failure. Memory and real
PostgreSQL regressions exercise both commit and object-storage failure points.
The codegen stack-access source guard now discovers extracted emitter modules,
and a closed helper-inventory assertion pairs compiler/checker deferred payloads.
These additions require their own final acceptance receipts; earlier passes do
not certify later edits. A source-only callable still carries no effective
execution contract or checked effect set; its binding concern remains an audit
candidate rather than a demonstrated bypass. Full Type/monomorph and NovaSeal
packaging migrations remain separate work.

### A2-06: strengthen phase invariants and diagnostic identities

Evidence: [typed semantics](../../src/typed_semantics.rs),
[nested codegen](../../src/codegen/nested.rs), and
[diagnostics](../../src/error/mod.rs) expose invariants whose guarantees are
distributed across phases.

- [x] Classify production `expect`/`unwrap` sites by input reachability and
  owning invariant. Exclude parser methods named `expect` that already return
  `Result`; do not treat search totals as panic counts.
- [x] Replace input-reachable failures with source diagnostics. Use validated
  representations for guarantees that must survive a phase boundary; retain
  justified internal assertions with clear contracts.
- [x] Introduce stable typed diagnostic identities incrementally for the changed
  boundaries. Keep presentation text separate from machine-readable codes.
- [x] Move affected consumers/tests away from message substring classification
  while preserving user-facing spans and actionable explanations.

The [invariant review](../CELLSCRIPT_ALPHA_2_BOUNDARY_REVIEW.md#phase-invariant-classification)
separates guarded local operations, admitted cross-phase records, closed
serialization and private concurrency invariants. Its source-hashed inventory
contains 343 assertions after excluding fallible parser methods; counts are not
a general panic proof. Real oversized-array inputs exposed unchecked ABI extent
and alignment arithmetic. Fixed layouts now validate checked products/sums
against the typed artifact's `u32` width contract, with E1020 and source spans;
parameter-width capture is fallible and alignment clamps before rounding.
The CLI lockfile race returns CLI0003; upgrade environment analysis retains the
validated manifest snapshot. LSP URI/edit conversion and runtime startup return
errors instead of unwinding or returning partial edits. Seven frontend tests,
three LSP conversion tests, the parameter-extent test, 19 command tests and four
upgrade tests pass. Offline native metadata/ELF probes reproduce diagnostic
rejection with a successful normal-array control. Cache v145 invalidates the
earlier artifacts. This closes the scoped invariant improvements; final-source
backend/dev/CI gates remain under A2-14.

### A2-07: preserve explicit emission failure and scratch contracts

Evidence: [frame helpers](../../src/codegen/frame.rs) record scratch exhaustion
through [the generator](../../src/codegen/mod.rs); its public generation path
checks the sticky error before returning an artifact.

- [x] Document and mechanically test error propagation for every artifact-returning
  entry point, including scratch exhaustion and aliased registers.
- [x] Assess a typed failed-emitter state or local `Result` propagation where it
  improves enforcement. Do not describe the current checked path as a confirmed
  truncated-artifact escape without a reproducer.
- [x] Make helper live-register contracts consistent and audit call sites when
  changing scratch selection. Keep safe destination reuse in textual immediate
  normalization; prohibit hidden scratch clobbers.

The [boundary review](../CELLSCRIPT_ALPHA_2_BOUNDARY_REVIEW.md) records all four
artifact-returning paths, first-error preservation and the decision to retain
the single checked completion path. The 47 assembler/emitter tests pass,
including exhaustion and unstated-scratch failures for both assembly/ELF and
both consuming entry points. Final-source acceptance remains under A2-14.

### A2-08: infer structured effect facts with no category model

Tracked by [GitHub issue #85](https://github.com/CellScript-Labs/CellScript/issues/85).
The accepted [effect-facts contract](../CELLSCRIPT_EFFECT_CONTRACTS.md) is the
sole design. Decision date: 2026-10-30. The working-tree implementation is present;
final-source acceptance and issue closure remain open.

Implementation checkpoint: bounded [fact descriptions](../../crates/cellscript-artifact-checker/src/effect_facts.rs)
and exhaustive [IPC process facts](../../crates/cellscript-artifact-checker/src/effect_facts/ipc.rs)
are present. The descriptions enforce mandatory wire fields, closed identities,
canonical name ordering, set bounds, compound lifecycle constituents and resolved
source substitution. The old category enum and runtime category column have been removed. Compiler
and independent-checker reconstruction now use their own IR/typed-record
control-flow analyses. The wire cutover requires mandatory structured facts,
removes duplicate call/block labels, and allocates cache generations v141/v142. These
changes are still under behavioral validation and do not close A2-08.
The compiler's [resolved fact graph](../../src/effects/resolved.rs) now computes
bounded transitive facts with explicit definition owners and argument sources;
its six focused regressions cover source substitution, ordinary values,
recursive components, incomplete bindings, determinism and analysis budgets.
Binding regressions also exposed source functions being mistaken for same-spelled
intrinsics. Type checking and IR now preserve local/imported source bindings
before constructor and builtin paths; the five focused cases and nine owned
collection regressions pass. This changes generated calls and allocates artifact
cache generation v140. A subsequent regression reproduced `a::b` and `a__b`
collapsing into one physical callee. The v143 follow-up encodes the complete
defining module, removes physical label selection by import alias and rewrites
dependency calls through their recorded source bindings. Seven binding tests,
including independent interface inspection and real VM execution of colliding
owners plus a transitive alias, pass. Broader affected-policy/interface checks and
final acceptance remain open.

Reader closure also removes Registry v2/v3 admission and its category comparison
field: source interfaces require v5, mandatory canonical facts or explicit null
template evidence, and the same product-subset relation. Registry type checking,
134 tests and Node bundles pass; 10 pre-existing environment-dependent tests are
skipped, not counted as passes. Shared Rust/Registry/browser wire vectors expose
and cover rejection of extraneous non-parameter source indices. Five actual
compiler-produced interface fixtures and eight comparison cases check Rust and
Registry agreement. SDK receipt schemas and numeric version guards now derive
from native protocol constants; three previously failing open/instance/verifier
transaction cases pass after deleting the numeric v2 guard. A new negative
generated test rejects a rewritten current-schema receipt with an old number.
The later backend checkpoint passed 1,121 library and 42 allocated-runtime
tests, then was interrupted for these confirmed reader fixes. It is not final
acceptance; failed/interrupted logs remain available.

- [x] Make `context_reads`, `lifecycle_ops` and `process_ops` the only effect
  dimensions. Infer bounded, closed, canonical facts from resolved operations,
  source/binding evidence and instantiated transitive calls.
- [x] Remove the exclusive category enum, category fields, aggregate joins,
  rankings, numeric encodings and replacement table. Retain no category display,
  translator, alias, fallback, optional mode or alternate semantic authority.
- [x] Require an explicit fact mapping for every admitted typed operation and
  helper. Preserve read source/access and call substitution; passing an ordinary
  value or view alone must not invent a transaction read. Unknown operations,
  unresolved bindings, analysis budgets and missing evidence fail explicitly.
- [x] Keep creation, consumption, destruction and explicit replacement distinct
  wherever their semantics differ. Record combined operations as a union;
  keep lifecycle authority, correspondence and obligations separately proved.
- [x] Give process/IPC operations their own facts. Do not hide communication,
  execution or descriptor access inside transaction observations.
- [x] Remove manual effect declarations and every declaration/default/presence/
  upper-bound API across AST, IR, metadata and consumers. Introduce no source,
  interface, manifest or CLI declaration override.
- [x] Remove the native `effects { ... }` wrapper. Dispositions appear directly;
  reject the wrapper and retired attributes without stripping or conversion.
  Preserve native ordering, disposition coverage and authorization checks.
- [x] Use the same resolved facts for actions, helpers, locks, native entries,
  imports, aliases and generic/recursive calls. Preserve source validation across
  optimization and bind admitted artifacts to the generated source contract.
- [x] Reconstruct facts independently from typed/lowered operation and callee
  evidence. Reject omitted, added, broadened, rebound or inconsistent facts.
  Missing executable evidence must remain unavailable, never an empty contract.
- [x] Replace category-based context, borrow, lock and scheduler checks with the
  applicable fact predicates plus existing ownership, aliasing, lifetime, source
  and lifecycle obligations. Observation-only borrowing excludes process I/O.
- [x] Replace #28's effect comparator explicitly with component-wise candidate
  subset checks against required facts. Apply the same relation in CLI diffs,
  upgrades, checked projections, receipts, handles, policies and builders.
  Preserve all independent signature, ABI, lifecycle and deployment obligations;
  compilation must not silently rewrite the required baseline.
- [x] Test set laws, source substitution and widening in each dimension. Include
  input-read-plus-creation, consume-plus-create versus bound replacement, each
  admitted IPC operation, local mutation, failed validation, missing evidence,
  transitive calls and source/optimization boundaries in real checked bundles.
- [x] Advance every affected source/profile/schema/domain/cache identity and
  update all active producers and consumers together. Require every fact field;
  reject unknown members, noncanonical sets, retired categories and mixed
  generations. Delete old readers, defaulting decoders and compatibility shims.
- [x] Show concrete facts and evidence status in hover, CLI and generated docs.
  Complete formatter, diagnostics, editor/browser/native metadata, examples,
  scheduler/adapters and normative-documentation parity without category labels.
- [ ] Close #85 only after the whole contract is implemented, obsolete machinery
  is removed and the applicable focused checks and gates pass on identified
  source. The category model and browser presentation have been replaced in the
  working tree; whole-flow acceptance is still open. No completion or
  release-readiness claim follows from this design decision.

## P2: maintenance boundaries and deterministic tooling

### A2-09: consolidate compiler-side canonical JSON behavior

- [x] Inventory actual implementations and delegating wrappers. Reuse one
  canonicalization implementation inside the compiler crate where contracts match.
  `cli/artifact.rs` already delegates; it is not a fourth independent algorithm.
- [x] Preserve existing key ordering, arrays, numbers, escaping, domain separation,
  and exact hashed bytes. Reject invalid inputs explicitly.
- [x] Add shared conformance vectors for independently maintained consumers
  instead of forcing independent verifiers to depend on compiler implementation.
- [x] Explain any identity change as a protocol change rather than an incidental
  deduplication. No current digest mismatch is asserted by this backlog.

The compiler's [canonical JSON implementation](../../src/canonical_json.rs)
owns recursive key sorting. Interface hashing, package/Registry helpers and CLI
helpers delegate to it. Parsed scalar spellings, array order and caller-owned
hash domains are preserved; this is not a change to a different JSON
canonicalization standard. The independent artifact checker retains its own
implementation and publishes [conformance vectors](../../crates/cellscript-artifact-checker/src/canonical_json_vectors.json)
as data; both implementations test exact bytes and invalid inputs against them.
The deduplication introduces no hash-domain change. Identity changes in this
delivery belong to the separately documented effect-fact protocol cutover.

### A2-10: select test binaries deterministically

Evidence: [test command helper](../../tests/common/mod.rs) prefers the newest
hashed executable by modification time over Cargo's declared binary path.

- [x] Use an explicit Cargo-built binary identity and remove mtime selection.
  Cover coexistence of stale binaries and differing build feature sets.
- [x] Audit shared filesystem, cache, environment, and subprocess resources
  separately before attempting parallel execution of the large suites.
- [x] Retain required `--test-threads=1` flags until isolation has been proved and
  the project contract is intentionally updated. Do not promise a speedup based
  solely on changing binary selection.

The [resource review](../CELLSCRIPT_TEST_RESOURCE_ISOLATION.md) separates Cargo
binary identity from temporary package roots, child environments, ephemeral HTTP
ports, local CKB contexts, shared fixture reads and fixed evidence paths. The
large-suite serialization policy is retained; no parallel isolation claim is
made.

### A2-11: extract cohesive modules and isolate CLI integration

- [x] Split compile orchestration, metadata construction, and cache handling out
  of `lib.rs` along existing responsibilities. Separate large test groups where
  that improves ownership and focused execution.
- [x] Extract coherent type-checking, IR builtin, runtime syscall, hashing, and
  handle emission modules. Preserve phase ownership and avoid another generic
  utility layer or a file-size-only rewrite.
- [x] Reduce duplicated command definitions/parsing/dispatch coordination using
  the existing CLI contract; preserve help, arguments, errors, and exit behavior.
- [x] Isolate proposal-specific NovaSeal certification behind a clear native
  integration boundary. First inventory CLI use, package inclusion, evidence
  paths, and release requirements; preserve the advertised command until an
  explicit migration is defined.
- [x] Measure compiler build time and shipped binary/browser size independently
  of source-file bytes. Audit every `sha2` consumer before changing dependency
  optionality; removing the certifier alone does not remove other uses.

Emitter extraction must use the original implementation verbatim and follow the
[refactor procedure](../../AGENTS.md). Keep the independent artifact checker and
its admission responsibilities separate from compiler convenience APIs.

The library extraction now has private owners for compile orchestration,
native/browser metadata construction and persistent caches. Diagnostic tests
have a dedicated file and retain their existing test names and source spans;
the all-target check and seven diagnostic tests pass. The direct/generic shape-alias characterization retains identical
ELF, interface, typed-semantic and complete compile-receipt bytes at the same
source locations before/after extraction. Generated ProtocolBundle
and checked-handle APIs live in `cli/protocol_builder.rs`, with their receipt
identities sourced from native constants. `types/calls.rs` and `ir/builtins.rs`
own cohesive call-checking and builtin lowering. Runtime dispatch, hash routines
and handle routines have dedicated modules; all 27 extracted emitter bodies
and literals match their source snapshot. That extraction checkpoint predates
the intentional bare-spawn correction above.
The complete post-runtime compiler
library run passes 1,129 tests. The certification adapter owns one Clap argument
declaration, engine integration and output policy; its seven real CLI cases
retain identical status/stdout/stderr, and 19 command tests pass. See the
[native integration inventory](../CELLSCRIPT_NATIVE_CERTIFICATION_BOUNDARY.md).
Two direct/shape-alias programs retain complete ELF, interface, typed-semantics
and receipt bytes across the cohesive extractions with caches disabled.

The v144 warmed-dependency native release build took 83.45 seconds and measured
4,223,920 KiB peak compiler-process RSS; `cellc` is 54,789,784 bytes. This is not
a cold workspace build or aggregate concurrent-process memory measurement.
Browser raw/gzip measurements are recorded separately under A2-12. `sha2` still
has package/integrity/inspection and test consumers, so no dependency optionality
or version was changed. Source movement is not claimed as a binary-size saving.
Current-source behavioral acceptance remains under A2-14.

The v147 warmed-dependency release build takes 80.52 seconds,
measures 4,314,712 KiB peak compiler-process RSS and produces a 54,716,008-byte
native binary. Its source and recipe are recorded with final acceptance; this
does not attribute the size difference to one extraction or claim a cold build.

The final v148 warmed-dependency native release build takes 96.39 seconds,
peaks at 4,298,192 KiB compiler-process RSS, and produces a 54,717,896-byte
`cellc` (SHA-256 `ecd174d2be0e95f275a0d9ef6473ce2500452884de0d83cda96ca997fb6829a9`).
Website example data is regenerated from that binary. These are build-process
measurements on this host, not compiler runtime-memory ceilings.

### A2-12: maintain browser and native profile boundaries

- [x] Keep reusable semantic descriptions free of native I/O and usable in the
  metadata-only WASM build.
- [x] Verify browser serialization and bundle size when introducing shared types
  or dependencies. Report measured retained code and gzip size, not assumed
  savings from removing a dependency declaration.
- [x] Keep tracked browser artifacts tied to their source/build recipe. Treat
  ignored `dist` directories as local outputs, not additional committed releases.
- [x] Preserve explicit differences between metadata-only and native admission;
  do not claim that browser metadata provides native execution evidence.

Fact-protocol checkpoint, 2026-10-30: the canonical Linux/amd64 Docker recipe
produced 1,770,668 raw bytes and 668,541 gzip bytes (652 KiB, within 1500 KiB).
Replaying the recipe produced the identical WASM SHA-256
`e353e0fe84b554abddd6db70dbc744a704c69db5232036f7c9a0b1ef11175847`.
The build generates the website's `compiler-artifact.json`; page/worker cache
keys and distribution verification share that content identity instead of
separate release strings. The browser reads three mandatory fact sets and rejects
retired/mixed action fields, without synthesizing an empty contract. Native WASM
crate tests pass 30 cases; the initial distributed WASM run passed 75 cases;
`npm run build:ci` passes, including both static environment builds, Astro and
distribution checks. These checks perform no deployment and provide no native
execution evidence. Later compiler changes require regenerating the bundle and
revalidating this boundary on final source under A2-14.

The subsequent v143 owner-label/function-IR checkpoint rebuild measures
1,770,025 raw bytes and 668,152 gzip bytes. Its build-generated SHA-256 is
`138bffde6f5432111bca86adfcfc659f65f0746b214afc210400c317a1ddb30d`.
The distributed regression inventory now has 77 cases, including distinct
same-spelled callable owners. Earlier replay equality belongs to the v142
checkpoint; it is not asserted for this different binary.

The v144 structured-nested checkpoint rebuild measures 1,794,603 raw bytes and
677,798 gzip bytes (661 KiB). Its generated SHA-256 is
`876549bfd7330bd2a9bcb031097d24035c1a8c741fff488c8a42c7fbdba23fc6`.
All 77 distributed-compiler cases and the complete static `build:ci` pass.
The 9,646-byte gzip increase from the preceding v143 measurement remains within
the 1500 KiB budget. This is a new binary, not a replay-equality claim. No
deployment or publication command ran.

The v145 checked-layout checkpoint measures 1,795,843 raw bytes and 678,376 gzip
bytes (662 KiB), with generated SHA-256
`0407d6b080b8efe3f503dc0dffdf68bbbb5a90d83289eb78873d9a5ba562ee31`.
All 79 distributed-compiler tests pass, including fixed-layout diagnostics in
both editions. Browser action and Cell-flow readers explicitly reject all former
declaration/inference fields mixed into current facts; the new presentation
regressions pass. Complete static builds passed before that guard tightening;
the final repeated static `build:ci` also passes. Source-identified compiler
gates remain under A2-14.

The v148 checkpoint cache generation, `0.33-allocated-result-effect-flow`, excludes
intermediate native artifacts. The canonical browser rebuild measures 1,796,256
raw bytes and 678,483 gzip bytes, SHA-256
`10f7a8633044a01fdb57fb538feb87676e05d89d79cf78da2798535354197bd1`.
All 82 distributed-compiler cases and the complete static website build pass,
including the allocated-result continuation regression and explicit bare-spawn
unavailability. Complete compiler-gate acceptance remains under A2-14.

### A2-13: repair documentation and classify retained compatibility paths

- [x] Reconcile assembler instruction/alias documentation with implemented and
  emitted forms and the mechanical closure tests. `sgt` is already documented;
  document real gaps without inventing an encoding defect.
- [x] Remove guidance referencing deleted versioned suites and update module
  ownership guidance. Prefer structural descriptions over quickly stale byte counts.
- [x] Document why Edition 2026/2027 frontends, legacy entry selection, and the
  distinct storage models remain. Rename misleading internal terminology only
  with an explicit compatibility and serialization audit.
- [x] Distinguish intentional compatibility boundaries, deferred semantics, and
  genuinely unfinished migrations. Do not merge them merely to remove `legacy`.
- [x] Preserve the documented reasons for dependency pins and independent checker
  validation. Review duplication by responsibility rather than name similarity.

The [boundary review](../CELLSCRIPT_ALPHA_2_BOUNDARY_REVIEW.md) records the
serialization audit for renamed function/entry IR, the explicit edition routes,
current visibility policy and distinct storage models. Documentation now has
one assembler inventory and behavior-owned test guidance. This closes the
documentation review, without claiming the separate A2-04/A2-06 migrations.

## Acceptance and release evidence

### A2-14: close the validation gap on identified final source

- [x] Record source revision, toolchain, command, outcome, and remaining exclusions
  for each acceptance run. Historical alpha.1 results and focused regressions do
  not establish alpha.2 acceptance.
- [x] Use `./scripts/cellscript_gate.sh commit` plus focused behavioral checks for
  ordinary commits. Documentation-only edits require no VM execution or full
  compiler test run.
- [x] Run `./scripts/cellscript_gate.sh backend` before claiming integration
  readiness for the nested IR/codegen or runtime changes.
- [x] Run `./scripts/cellscript_gate.sh dev` at integration checkpoints and
  `./scripts/cellscript_gate.sh ci` before claiming merge readiness.
- [ ] Satisfy the documented release requirements before publication, with the
  pinned external prerequisites available. Do not start publication, deployments,
  TAC, or Daybreak from this planning task.
- [x] Retain failed/interrupted receipts and describe their scope. A deferred or
  skipped check is not a pass, and the alpha.1 waiver is not reusable authority.

The v148 acceptance checkpoint used review HEAD `9fa44d86b86a3ec9b0415719956715890a73cefe`
plus the source manifest
`f528453aba58eef6b847bf18b621c08725b6b3e205c357190a486bc8f6ed85c5`.
The manifest records 949 source/configuration/fixture hashes and Rust
`1.97.1 (8bab26f4f 2026-07-14)`. This identifies a working-tree candidate, not a
new Git commit or release. The manifest is `v148-acceptance-source2.json`; its
identity is SHA-256 of canonical JSON excluding `manifest_sha256`. All receipts
below are retained under
`target/alpha2-work/effect-facts-migration/`.

| Check | Current outcome | Receipt |
| --- | --- | --- |
| Commit gate | Passed at v148 | `commit-v148-current.log` |
| Complete backend gate | Interrupted after 1,135 library and 42 allocated-runtime passes; clean committed source and pinned CKB are required for the remaining production acceptance | `backend-v147-final2.log`, `backend-v147-interruption.json` |
| Dev / CI | Dev exposed a stale example template and raw-bytes-to-Hash call; both pass their focused suite. The later CI attempt was interrupted after 198 passing test lines, with no complete gate exit result. | `dev-v148-working-tree.log`, `bounded-byte-context-v148-current2.log`, `ci-v148-working-tree2.log`, `ci-v148-interruption2.json` |
| Canonical browser rebuild | Passed; gzip 678,483 bytes | `canonical-wasm-v148.log` |
| Distributed compiler / static website | 82 tests and the full static build passed | `website-v148-compiler2.log`, `website-v148-build.log` |

The interrupted v145/v147 backend runs and failed v147 stale-inventory preflight are
retained separately. The inventory was regenerated after the reviewed example
metadata-budget changes; no VM resource ceilings changed. Full gate completion,
source stability and the #85 closure condition remain unchecked until their
commands finish successfully.

See the [gate policy](../CELLSCRIPT_GATE_POLICY.md),
[verification tutorial](../wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md),
and [alpha.1 release scope](CELLSCRIPT_0_33_ALPHA_1_RELEASE_NOTES.md).

### Post-alpha.1 self-audit corrections

The maintainer's follow-up authorizes these four audit fixes, cache cleanup and
continued alpha.2 acceptance. It does not authorize publication or deployment.

- [x] Apply non-trapping prefix admission to source-to-memory and zero-scan
  fusion as well as source-to-source loops. Both reproduced division-by-zero
  cases now retain error 20 in real CKB-VM under standard and diagnostic
  compilation, with independent bundle validation. Unit tests cover division,
  remainder, shifts and checked wide arithmetic in both additional matchers.
- [x] Advance the persistent artifact cache to v149,
  `0.33-byte-loop-trap-preservation`, excluding the earlier miscompiled results.
- [x] Keep best-effort audit quota failures inside the failure handler, and
  make memory quota admission atomic with session rollback. Shared memory/SQL
  regressions admit exactly one of 32 simultaneous requests for a one-slot quota.
- [x] Replace the ineffective dead-letter orphan query with locked permanent
  rejection cleanup. Preserve foreign-key/audit records as hidden tombstones;
  forbid retry and admission of purged source, retain infrastructure failures,
  and protect other release references, including expired retained releases.
  Memory/PostgreSQL tests cover concurrent retry, stale attempts and failed I/O.
- [x] Repair the two entry-selection assertions: standard inlining need not
  retain an unused helper body, and imported generic labels use their defining
  owner. Keep the VM entry-selection and independent bundle checks intact.
- [x] Remove stale generated native build directories and package incremental
  caches while retaining prior receipts and the pinned CKB worktree. About
  149 GiB was removed. The roughly 555 MiB root-owned historical WASM cache
  could not be deleted with the available host permissions.

Focused checks pass: ten byte-loop unit tests, ten crypto/VM integration tests,
thirteen entry-selection tests, and Registry type checking plus 158 passing tests with
PostgreSQL 17 and both Rust verifier subprocesses enabled. One optional upstream
LS-IDL Rust-client check is skipped because its external checkout is absent. Logs and cleanup inventory are under
`target/post-alpha1-audit/`. These checks do not replace A2-14. The fresh canonical
browser build passes at 1,796,256 raw bytes and 678,483 gzip bytes, retaining the
v148 WASM digest. The pinned recipe ran through Docker's API on rootless Podman;
the initial BuildKit attempt failed on its loopback proxy, and the host-network
legacy-builder run passed. The complete identified-source gates are being rerun
after these corrections; their results must be recorded before changing
acceptance checkboxes. PostgreSQL test extensions are installed once in `public`
under a database lock so parallel disposable-schema suites cannot race creation
or drop another suite's `pgcrypto` extension.

The first complete-test backend attempt at candidate `355e586d` passed 1,136
library and 225 artifact-checker integration tests, then failed two canonical
business-corpus identity assertions. The accepted effect-facts migration had
changed lowering/source-map/bundle records while all four ELF hashes, transaction
hashes and six VM/resource measurements remained identical. Only the four
metadata identities, derived ProtocolBundle identity, matching policy metadata identities
and corpus inventory digest were refreshed. Resource ceilings and negative
scenario assertions are unchanged. The failed gate and exact observation remain
in `fix-backend-snapshot3.log` and `anchor-identity-refresh.json`; the focused seven-test business-corpus rerun passes (`fix-business-corpus2.log`).
A complete gate rerun is still required for integration acceptance.

Candidate `1acf446f` next passed the refreshed business corpus, then the full
CLI suite stopped at one obsolete ReadOnly-category assertion (254 passed).
Replacing that fixture with two invalid dynamic exact-handle commitments exposed
a real diagnostic regression: effect inference discarded all but the first IR
lowering error. Diagnostic consumers now retain the complete error list; ordinary
single-error compilation retains its first-error contract. API and JSON CLI
regressions preserve both errors and their distinct source ranges. This changes
invalid-source diagnostics, without changing valid artifacts or cache generation.
The fresh canonical WASM bundle is 1,796,414 raw / 678,604 gzip bytes, SHA-256
`1628fd339ca6f76c9e0f93a24a59c9ec34d0772bde0d8a400a54e64834f44d08`.
The six diagnostic API tests, all 255 serial CLI tests, 82 distributed-compiler
tests and complete website build pass. The 33-test scenario-identity preflight
also passes across the seven owning suites. The fungible/NFT issuer fixtures now
explicitly construct Hash from the new raw-byte return type. Frozen artifact,
lowering, source-map and transaction identities were reviewed against the same
VM exit codes/outcomes and rejection stages; the complete diff is retained in
`scenario-identity-refresh.json`. Some ELF identities also changed under the
current standard compiler, so this is execution evidence rather than a claim of
byte equivalence. The maximum committed-state profile now measures 77,588 cycles
and 9,800 ELF bytes (31 cycles / 40 bytes lower); all resource ceilings and other
measurements remain unchanged.

The default-host dev attempt stopped at strict audit after an interim new-test
compile error (`Span` was incorrectly treated as optional). That test is fixed,
and the diagnostic/CLI regressions pass. The failed strict report identifies
`assembler-emitted-surface` and is retained with `fix-dev.log`; this run is not a
complete dev pass. A new stable source freeze and complete gates remain required.

The complete-test preflight next exposed stale prerelease Registry fixtures,
retired raw-selector source evidence, static-versus-dynamic Script hash-type
inputs, and paired consume/create facts. Positive Registry fixtures now explicitly
admit the current compiler prerelease and matching lock requirements. Runtime
negative cases still execute real missing typed coordinates and invalid witness
hash types; unresolved raw selectors and invalid constant hash types reject
before emission. Shift-trap cases use typed input views and retain all three
compiler configurations, nonzero/zero iteration checks, and exact VM exits.
The multisig lifecycle fixture explicitly constructs Hash from raw args. Its
measured ELF changes from 8,128 to 8,160 bytes under unchanged resource ceilings;
cycles remain signature-dependent, with a fresh observation of 6,282,178.

This preflight also found a production adapter defect: the deployment-line schema
already required v3 while its numeric-version check still required 2. The adapter
now requires schema and numeric v3 together; rehashed reports with versions 1, 2
or 4 remain rejected. The focused adapter regression and end-to-end selection
materialization tests pass. No older receipt reader or compatibility fallback
was added. These checks and the failed preflight logs are retained under
`target/post-alpha1-audit/`; full gates on the corrected candidate remain pending.

The later default-host dev run on unchanged candidate `510690ea` passed strict
quick, then stopped at two edition instances of the same syntax-combo fixture:
the expected diagnostic still named the retired Mutating category. The fixture
now requires the specific current observation-only borrow rejection; the
associated case description uses effect facts. Its DSL program and rejection
requirement are unchanged. The repeated quick matrix passes all 164 cases
(92 accepted, 72 rejected, zero failures). The failed dev receipt and successful
focused repeat are `dev3-result.json` and `fix-syntax-combo2.log`. Backend attempt
6 on `b9b734b8` was intentionally interrupted before its known failing syntax
step so a corrected, clean candidate can be frozen; its completed tests remain
recorded in `backend6-interruption.json`. Neither attempt is a complete gate pass.

A separate temporary positive control exposed a retained fail-closed boundary:
a non-trapping loop over typed input-view parameters can fuse to
`__ckb_source_bytes_equal`, then reject with V2420 because nested lifetime
validation cannot prove its variable syscall destination size. No invalid
artifact was accepted, and the checker was not relaxed. Complete fast-path
admission for this form requires its own bounded machine proof and mutation/VM
coverage; it is not established by the passing shift-trap regression. The source
and failure are retained in `typed-source-loop-observation.json` and
`fix-unified-compilation.log`.

The first complete CI attempt on `2ea5e32c` passed the full compiler suite
(including 1,137 library, 225 artifact-checker and 255 serial CLI tests), the
standalone checker, Fiber/CKB adapters, WASM crate and SDK example tests. It then
failed `native_tools`: skill-pack freshness accidentally parsed component
`SOURCE_METADATA_SCHEMA_VERSION = 2` after the envelope became a checker-owned
alias. The document correctly says envelope schema 74. Skill-pack and document
validation now share the anchored authoritative-envelope reader; component
versions cannot seed that check. The next native-tools run exposed an old
semicolon-only LSP call marker. The boundary now requires the fallible entry
point and native CLI error routing together. All 73 native-tools tests pass
(`fix-tools-complete3.log`); the failed complete CI remains `ci1-result.json` /
`fix-ci-snapshot1.log`. Remaining complete gates still need the corrected source.

After the default-host runs and the current main-worktree commit gate finished,
their idle debug dependency/build/fingerprint/incremental directories were
removed while retaining native binaries, receipts and the active acceptance
worktree. Their apparent size was about 64 GiB; filesystem free space increased
by about 14.6 GiB during the operation. These are separate measurements, not an
additional claim of 64 GiB physically freed (`cache-cleanup2.json`). The root-owned
historical WASM cache remains outside available host deletion permissions.

The corrected-source dev run on `7cffb5ec` passed strict quick and all 164 syntax
cases, then failed the scenario package: the Edition 2027 LSP benchmark source
had been added to the shared Edition 2026 fixture module directory. The exact
12,404-byte file now lives under `tests/lsp_responsiveness/fixtures/`; both Rust
include sites use that behavior-owned location. Its bytes and source hash are
unchanged. The repeated package command with `--backend all` passes: six files
compiled and four scenario/backend runs executed. The failed dev and byte-identical
move are recorded in `dev4-result.json` and `lsp-fixture-isolation.json`.

The complete dev gate subsequently passed on frozen candidate `f4bb5b55`
(`dev5-result.json`, `fix-dev-snapshot5.log`). The second CI attempt passed the
compiler and workspace suites, clippy, 208 syntax combinations, package archive
verification and Registry API tests/builds. It failed a positive fixture in the
independent Registry verifier: its temporary manifest omitted `cellscript_version`, whose
default `*` excludes the active prerelease. The fixture now explicitly requires
`>=0.33.0-alpha.1`; compiler admission policy is unchanged. All seven verifier tests now pass
(`fix-independent-registry-verifier2.log`). That failed gate is
retained in `ci2-result.json` / `fix-ci-snapshot2.log`; it is not CI acceptance.
These acceptance runs use host-test optimization level 1 with debug assertions
and overflow checks enabled; DSL optimization selection and serial CLI/iCKB
execution are unchanged.

The corrected independent-verifier fixture then passed all seven tests. Complete
CI passed on frozen candidate `f28b82de` (`ci3-result.json`,
`fix-ci-snapshot3.log`). The Registry API ran 158 passing tests with one optional
upstream Rust-client skip; the lifecycle Node consumer had one optional retained
continuous-node evidence skip. Standard ignored measurement/live-node cases
retain their documented boundaries. The subsequent full backend passed compiler,
checker, Fiber, clippy, dual-backend package, contract/resource and strict
contract/syntax checks, then failed before a fresh CKB build: its generated
bounded input/output manifests still used the default compiler requirement `*`.
Both now explicitly bind the generating compiler via `cellscript::VERSION`.
`backend7-result.json` retains that failed gate; stateful and node acceptance
remain open on the corrected candidate.

The focused stateful retry on `d4500568` compiled both corrected bounded
packages, then rejected stale transaction-recipe ELF pins before any node build.
Of 60 scoped artifacts, 58 have new identities. The candidate recipe refresh
rebinds only those code hashes and 58 derived Script identities computed with
the locked native CKB `packed::Script::calc_script_hash` implementation. All
216 transaction labels/topology, 43 action cases, 17 lock cases, 26 stateful
scenarios, business values and rejection expectations are retained. Its two-pass
identity map converged; 536 identity-bearing values change, including dependent
owner fields/witnesses. This is not byte equivalence or VM acceptance. The recipe
header now describes historical topology provenance and requires a separate
current-source acceptance report instead of asserting a current `passed` status.
The failed preflight and candidate mapping are retained in
`stateful8-result.json` and `ckb-recipe-rebinding-review.json`. Fresh node replay
remains required before accepting the refreshed pins.

The refreshed recipes then passed a fresh pinned-node production acceptance on
frozen candidate `ccc97035fea56cca4a46c6f508854ffcb5bc699c`: 43 actions,
17 valid and 17 invalid lock spends, 26 stateful scenarios and 46 stateful steps.
The independent production-evidence hardening validator passed. A focused
`counter_node_acceptance` also passed. Complete `backend` and `ci` gates then
both exited 0 on that same clean candidate (`backend8-result.json`,
`ci4-result.json`, `fix-backend-snapshot8.log`, `fix-ci-snapshot4.log`). The earlier
complete dev checkpoint passed on `f4bb5b55` (`dev5-result.json`).

The tested 1,868-file source manifest is `final-acceptance-source.json`, SHA-256
`98b784725136901b505e2e193a7beafa867cb11832036d8e650b53bcc162f4f3`.
CellScript and the observed fresh CKB build used Rust 1.97.1; CKB source was
pinned to `f7fa4436737756f97a24e254f22c13a36316ecea`. Host tests used optimization
level 1 with debug assertions and overflow checks enabled, incremental builds
disabled and four Cargo build jobs. DSL strategy and CLI/iCKB serialization were
unchanged. CI's Registry API had 158 passes and one optional upstream Rust-client
skip; its lifecycle Node consumer had five passes and one optional retained-node
evidence skip. The backend actually ran the required real-node stateful and
counter cases. These results close the recorded gate gap for this code candidate;
this receipt-only documentation overlay is validated separately with the commit
gate. The frozen acceptance worktree remains at the tested code revision.

Gate completion does not remediate the separately reviewed GLM findings:
byte-string EOF acceptance, incomplete Fiber workflow aggregation, and IPv6 /
resolved-address artifact admission remain open. Registry audit-event rollback
also retains a concurrent-write concern, without a new full-service reproduction
in this run. ABI/helper-facts coverage requests remain validation gaps rather than
proved wrong-code cases. The typed source-loop V2420 admission boundary above
also remains fail-closed. Detailed review and native reproducers are retained in
`target/post-alpha1-audit/glm-assessment.txt` and its companion receipts. The
metadata-path and historical-report wording was corrected under A2-13. Version
0.33.0-alpha.1 is unchanged; release requirements and publication remain open.

The documentation follow-up also corrects the metadata-path promise:
`compile_metadata` performs mandatory front-end/IR validation without the eight
optional transformation groups, so its function/helper records need not equal
optimized artifact metadata. Browser summaries do not prove ELF admission.
The old read-boundary report now explicitly identifies its historical alpha.1/v129
scope and points to current retirement/strategy contracts. No retired alias or
browser optimization path was added.

## Suggested delivery order

1. A2-01 and A2-02: small, independently verifiable correctness fixes.
2. A2-03: LSP responsiveness, independently of the type representation work.
3. A2-04: identity characterization, behavior-preserving foundation, then the
   complete three-type IR migration in separate reviewable checkpoints.
4. A2-05 through A2-08: explicit runtime, phase, emission, and effect contracts.
5. A2-09 through A2-13: bounded maintenance work with preserved behavior.
6. A2-14: acceptance on the actual final candidate and accurate release claims.

The broader Cell/selection type migration is a follow-up decision after the nested
slice is accepted. This backlog does not authorize a general type-system rewrite
or weaken existing ownership, runtime, or independent-checker gates.

Local acceptance preparation: a separate clean CKB worktree at the exact
`f7fa4436737756f97a24e254f22c13a36316ecea` pin is available under ignored
`target/alpha2-tools/ckb-pinned`; the existing sibling checkout is unchanged.
Native v147 release data, the website static build and VS Code manifest validation
pass. A local detached acceptance snapshot now captures the compiler and all initialized
submodule changes without changing the maintainer working tree or its staging.
The candidate and submodule commits are recorded in
`target/post-alpha1-audit/acceptance-snapshot.json`; they are unsigned local
validation artifacts and have not been published. No source-cleanliness bypass
or publication was performed.

The first dev attempts exposed an incomplete positive generic-interface fixture
(schema v5 with a v4 number, then omitted mandatory fields). It now constructs the
complete current wire model from the checker-owned types/constants. All 85
standalone-checker unit tests pass; production admission remains strict. The
prior source manifest is retained, and the sole source delta is this test fixture.

The v148 follow-up corrects ordinary allocated-value result propagation. An
arena get returned by a helper previously looked non-returning to compiler
inference, dropping facts of later calls; the independent checker rejected the
valid bundle. Results now remain ordinary values, and a focused native/checker
regression preserves the following input read in both optimization modes.
The three failing dev mutations now pass: scalar continuations, exact-role policy
changes (including the earlier inferred-read rejection), and fixed-copy helper
mutation with its unchanged exact machine rejection code. Error-code acceptance
sets were not broadened. Cache v148 invalidates intermediate outputs. The fresh
browser bundle passes 82 cases and the static build; complete dev/CI acceptance
remains pending. The AgoraSeal manifest template now retains the same explicit
compiler requirement as its generated manifest, and both source forms construct
Hash explicitly from raw 32-byte arguments. The five bounded-context tests pass,
including real parent/child executions; no implicit type compatibility was added.
