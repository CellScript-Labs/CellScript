# Complete syntax convergence implementation

Objective: implement the complete authoring convergence requested on 2026-11-26,
including the remaining decisions in the [surface review](../../CELLSCRIPT_SYNTAX_SURFACE_REVIEW.md)
and the reproduced differences in the [v8 audit](AUTHORING_V8_SYNTAX_AUDIT.md).
Baseline: `e134d663`, Edition 2027 authoring-v8. Work is isolated from the
uncommitted Finance changes in the original checkout. Status: in progress.

The final source contract must have one current grammar and matching native,
browser and editor consumers. A parser-only change, a replacement diagnostic,
or a retained proposal does not complete the requested implementation. Existing
wire/hash domains and supported business behavior require explicit preservation
or a recorded intentional semantic change.

## Requirements and evidence

| Requirement | Final result required | Status / evidence |
| --- | --- | --- |
| F01, A01 | One grammar authority; distributed WASM bytes and editor pins execute the shared positive/negative source corpus. | The current canonical authoring-v9 WASM is 927338 gzip bytes (1500 KiB budget), SHA-256 `445b9eda9f90d7a1b618cf7df24e466836dc1372734363c76d8fd75b0eeac482`. Its actual distributed bytes pass 56 browser tests, including the real worker loader/handshake, twenty shared positive and ten negative fixtures through all three entrypoints, plus the shared builtin numeric-bound diagnostic rejection. Editor validation and the complete website build pass. Final source audit, gates and child-repository delivery remain pending. |
| F02, A05 | Prefix entry sources and inline named Cell outputs; ordinary value/helper returns retained; removed spellings have precise migration diagnostics and migrated callers. | Prefix-only sources and inline named Cell outputs have E1032 migration diagnostics. Fourteen immutable baselines retain thirteen exact ELF/typed/witness matches and one existing rejection; source migrations use lossless parsing, exhaustive AST comparison and idempotence. Current snippets and distributed corpus pass. The initialized active iCKB specifications also have a reviewed 112-token-edit migration with complete AST comparison, idempotence and exact assembly under the reference compiler. All 77 action/Lock parameter layouts agree across versions after explicit provenance elaboration. Final gates and delivery remain pending. |
| F03, A06 | Select ordinary `lock` declarations through an explicit Lock-group artifact, independent of source declaration order; preserve protected role, args, result and witness contracts. | Ordinary Lock selection, protected/CellDep bindings, Boolean results and helper/import closure pass native, VM and rehashed mutation owners. Single-entry manifests use `entry`; tagged `actions[].action` retains its dispatch meaning. The shared Lock fixture passes all browser entrypoints, editor snippets and website examples. Final gates and delivery remain pending. |
| F04 | Flow clause semicolons, trivia and recovery agree across consumers. | The v8 semicolon repair is retained in the current shared parser. Missing final flow terminators reject in native owners and actual distributed WASM; comments, recovery and formatting owners pass. Named and compact graph declarations retain their identity. Final gates remain pending. |
| F05, A07 | Structured branch-local fresh, retire and pool operations with independently checked occurrence/CFG bindings and exactly-once accepting-path coverage. | Shared lifecycle statements retain actual IR occurrences and independent accepting-path/field-sum checks. Branch VM owners, shared browser fixtures and the current 1259-test native library pass; missing/double dispositions and unsupported repetition reject. Final integration gates remain pending. |
| F06, A08–A09 | One primary authoring route per complete lifecycle intent; preserve anonymous returned Cells, quantity burns, receipt contracts and lower-level semantics where distinct. | Direct and paired kernels retain actual typed input/result bindings and independent occurrence/path checks. Fixed-group kernel successors retain their capacity-weaker contract. Identity and receipt result policies enter public interface identity. Shared unique, receipt and settlement browser fixtures pass. Quantity burns remain explicitly runtime-required; final gates remain pending. |
| F07, A08 | Resolve declaration-owned identity once; derive omitted matching operation policies without weakening explicit identity, absence or envelope obligations. | Fresh/create_unique/replace_unique derive concrete policies from resolved declarations, with literal-equivalent artifacts, import/alias/private-scope checks and explicit conflict rejection. Retirement absence remains mandatory. The final distributed corpus accepts the shared declared-identity source and rejects conflicts; final gates remain pending. |
| F08, A03, A13 | Expansion and hover expose actual coordinate, coverage and explicit/derived/unconstrained/not-applicable envelope policies. | Native expansion/hover share actual selectors, coverage and policy categories. Real VM counterexamples verify absolute versus group coordinate labels. Metadata-only floor analysis is repaired while artifact floor evidence remains mandatory. The browser intentionally omits native policy records and documents native expansion; the actual browser corpus and website build pass. Final gates remain pending. |
| F09, A10 | Complete replacement supplies its own lineage; local equalities and declared state edges retain their separate checks. | Replacement lineage, local field equalities and declared state edges retain distinct checks, including strict tag-only enum updates. NFT and Timelock flat/package examples now agree after parser-proved, exact-ELF reconciliation. All 36 example owners pass. Final gates remain pending. |
| F10, A12 | Explain/hover report actual collection storage, element width, capacity and budget; preserve distinct codec/selection/ownership domains. | Native collection report v2 and hover expose resolved backing, widths, actual capacity and budget. The ten-execution VM owner retains fixed-stack capacity behavior. Heavy native explanation records stay outside the default browser summary; WASM profiles pass 36/40 and the canonical gzip budget passes. Final gates remain pending. |
| F11, A12 | Named policies are canonical; bounded named unsigned constants share one checked resolution rule in supported bounds. | One checked unsigned scalar resolver normalizes array bounds, policy limits/intervals and closed builtin numeric arguments. Eight owners include nineteen independently validated literal-equivalent bundles, defining-module/private scope, contextual names, depth and revisions. Actual distributed bytes accept the shared fixture and reject overflow. Final gates remain pending. |
| F12, A14 | Aggregate shorthand shares resolved semantics; invariant reads/scope and executable/audit claims remain distinct. | Resolved field domains and selected accepting paths prevent false u64/raw-u128 conservation evidence. Eleven aggregate owners cover optimizer modes, explicit/tagged selection, callees, prelude coverage and profile mutations; five Fiber VM owners retain owner-or-conservation behavior. The shared valid aggregate fixture passes actual browser bytes. Final gates remain pending. |
| F13, A11 | Validity has an explicit braced region; shared formatter and purity checks preserve comments and statement/tail distinctions. | Seventeen legacy regions in eight files were migrated with lossless parsing, exhaustive AST equality and idempotence. Braced regions retain purity, contextual field names, fields after validity and statement/tail distinctions. Native owners and shared positive/negative distributed fixtures pass. Final gates remain pending. |
| F14, A14 | Reserved shapes are clearly diagnosed; visibility, value abilities, Cell permissions and source domains remain explicit. | Reserved mutable-reference and launch boundaries have direct diagnostics and are absent from default completion. Ordinary mutable values and read-only views remain admitted; visibility and graph identity are unchanged. The 29 boundary owners, three snippet owners, editor validation and shared browser fixtures pass. Final gates remain pending. |
| A02 | Complete named field lists have the same admission for local, imported, aliased and locked-package schemas; retain source evaluation order and schema-owned storage offsets. | Source admission repaired: 40 combinations pass their expected result across four routes; 16 accepted ELF bundles independently validate. The original strict generic-pool regression now passes after executable IR and independent typed-CFG proofs were added. |
| A04 | Deep syntax generators exercise their intended compile/runtime oracles without retired syntax. | Deep generators use current syntax and test their intended semantic phases. The 96-probe owner passes. The final deep report `target/syntax-combo-audit/20261126-161313-deep-20260503/report.json` records 157 cases: 99 expected accepts, 58 expected rejects and zero failures. Final gates run their own required corpus. |
| Migration | Examples, fixtures, snippets, generated interfaces, locks and source/cache identities agree with the final contract; unrelated user changes remain preserved. | Main source, snippets, generated website data, canonical WASM and package/flat examples are migrated. All 36 example tests and the complete website build pass. The active iCKB specs have their parser-proved signature migration; the final inventory, child delivery and original-checkout reconciliation remain pending; original Finance edits and deleted flat examples are preserved. |
| Validation | Focused behavioral/VM/checker mutation evidence; commit, backend and CI gates for the final candidate; review every requirement against current sources and results. | Focused native/VM/checker, WASM 36/40, distributed browser 54, deep 157, examples 36 and website build checks pass. The commit gate passes after a missing existing unicode-ident dependency edge was restored without version changes. The initial backend run passed 1259 library, eleven aggregate and 43 allocated-runtime owners, then was intentionally stopped after the final audit found stale embedded handoff/interface fixtures. Those inputs are repaired; all eight affected native owners and the real worker regression pass. Complete rebuilt backend/CI and final commit gate remain required. |
| Delivery | Push completed work to `0.36`, verify remote identity, remove generated `target/` after all checks. | Pending. |

Owners are the existing frontend, type/flow, IR/backend, independent checker,
ABI/builders, browser/editor and repository gate components. No requirement is
complete solely because an owning component accepts its syntax.

## Observed checks during implementation

- `cargo test --locked -p cellscript-tools --bin cellscript-tools syntax_combo::tests::seeded_deep_cases_reach_their_declared_semantic_phase -- --nocapture`:
  one test passed, exercising 96 generated probes and all three intended
  semantic rejection families without a parser rejection masking the oracle.
- `CELLC_BIN="$PWD/target/debug/cellc" ./scripts/cellscript_syntax_combo_audit.sh deep`:
  the retained report at `target/syntax-combo-audit/20261126-001138-deep-20260503/report.json`
  records 126 cases, 76 expected accepts, 50 expected rejects and zero failures.
  This is syntax/metadata/assembly evidence, not VM execution evidence. The
  report captured legacy prose mentioning a verification section. The source
  catalog wording is now repaired; final validation must regenerate the report.
- `cargo test --locked -p cellscript --test policy_imports named_lifecycle -- --nocapture`:
  three tests passed. They cover two field orders, missing/duplicate/unknown
  fields, local/import/alias/path-dependency resolution, metadata/ELF parity and
  byte-identical lockfiles before/after compilation. These source-admission
  probes use the existing permissive artifact policy and independent checker;
  they do not assert that the generic pool passes strict production admission.
- `explicit_pool_conservation_reaches_strict_admission_for_arbitrary_numeric_fields`
  originally failed with E2105 (`resource-conservation:Token`, `gap:runtime-required`).
  It now passes unchanged under strict admission. Producer-side admission checks
  the actual IR guards and covered operations; the independent checker separately
  reconstructs typed field accesses, widening moves, u128 sums and dominating
  fail-closed equality guards.
- `cargo test --locked -p cellscript --test policy_imports --test native_lifecycle_binding --test frontend_boundaries -- --nocapture`:
  46 tests passed (22 import/policy tests, seven native lifecycle tests, 17
  frontend boundary tests). The native branch test executes both choices for
  fresh, retire, pool, many-to-many pooling and mixed replacement/retirement.
  It covers wrong output data, nonconservation, a u64 carry into u128, wrapped
  totals and a retired identity appearing in the continuing output. This result
  predates the latest additional strict mixed-disposition and formatter/LSP probes.
- The initial independent-checker focus passed 13 tests, including rehashed
  missing-site, duplicate-site and forged pool field/member evidence. Additional
  mixed, many-to-many and actual claimed-field guard cases are now in the corpus
  and included in the broader run.

## Native implementation checkpoint

`Stmt::Lifecycle` owns each pool/fresh/retire source tree once. Surface entries
store structural references, and optimizers rebuild those references after
rewriting statements. Type checking and IR lowering request a transient kernel
at the actual statement site. Branch ownership joins retain an unavailable
`Disposed` state when alternatives discharge a role through different operations;
the separate business dispositions remain visible to IR and the checker.

Retirement checks now execute at their `Destroy` instruction. The former prelude
union could omit a branch-specific field absence check. A real VM counterexample
also exposed loss of the scan index when a large frame caused syscall argument
setup to reuse t6. Dynamic syscall helpers now capture a3 before preparing buffer
addresses, and the independent exact-role instruction grammar follows that same
checked order. The VM counterexample is retained.

The native wire candidate uses compile metadata 77, typed semantics and lowering
record v14, semantic foundation v6, disposition semantic nodes v3 and compile
cache generation v220 and source epoch authoring-v9. The new epoch explicitly
identifies the removed duplicate Lock declaration and unbraced validity section.
Remaining signature convergence belongs to that same unpublished candidate.
Consumer pins, distributed bytes and the complete fixture/documentation migration
remain pending; this is not an integration-readiness claim.

Remaining closure includes advanced kernel operations mixed with source policies
(the current role-summary suppression must become occurrence-aware), strict mixed
successor/retirement admission, all identity/quantity-policy distinctions, and the
rest of F01–F14. The complete integration command and all final gates remain
required; earlier partial passes do not establish their completion.

## Broader validation checkpoint

The full compiler library run completed with **1232 passed, zero failed** after
updating the independent exact-role instruction template for the captured syscall
index. The earlier broad run had 13 failures at that exact template; they were
not suppressed or reclassified.

The complete command below passed: **1232 library, 277 artifact checker, 17
frontend boundaries, seven native lifecycle and 23 policy import tests**, for
1556 passing tests and zero failures. Its log is
`target/lifecycle-integration-tests.log`:

```bash
cargo test --locked -p cellscript --lib --test artifact_checker --test policy_imports --test native_lifecycle_binding --test frontend_boundaries -- --nocapture
```

The subsequent focused run passed 52 tests: 18 frontend boundaries, four Lock
artifacts, seven native lifecycle and 23 policy imports. This verifies the
formatter/LSP comment probes, lifecycle expansion of actual values/sites, and
runtime-access summaries derived from `IrBody::cell_bindings` rather than
positions in the union of branch operations. Unique-operation access summaries
use the actual input/output bindings as well. The Lock VM probe places an
unrelated input before the protected group input and rejects wrong witness,
wrong args and missing payload. Uniform policy categories and hover remain
part of the full convergence.

The shared Lock parser cutover is a later candidate. One parser now owns
ordinary declarations, audit claims, lossless syntax nodes and diagnostics;
the old `lock_script` declaration receives E1032 while ordinary names and
`ckb::lock_script` retain their domains. The new Lock artifact example needs an
explicit lockfile repin after its manifest/source migration; that repin has now
passed using the authoring-v9 native binary. Nine single-entry manifest selectors
across seven files now use `entry`; the temporary `action` alias is removed.
Tagged `actions[].action` keeps its separate dispatch meaning. Editor/browser
pairing and final gates are still pending.

The Lock/validity focused integration run passed **53 tests**: eight formatter,
18 frontend boundary, 19 grammar contract, five Lock artifact and three
protected-group binding tests. This includes the actual CellDep read port,
contextual `validity` field names, fields after the validity region, preserved
comments, identical before/after typed semantics and shared E1032 diagnostics.
The preceding full-library run passed 1234 tests and failed one stale equality
between transaction and group ProofPlans. That owner now separately checks
their exact coordinate/scope/read/coverage differences and retains equality
for all remaining business guarantees; its focused rerun passed. Final full
gates still need the completed convergence candidate.

Two independent-checker tests pass nine rehashed mutation scenarios. Coherently
forged absolute/group coordinates include updated role and provenance projections
before layered identities are rebound, so the explicit Lock contract is tested
beyond stale-hash rejection. The explicit Lock CLI selection test passes with
serial execution. Eleven validity-owner tests pass, retaining purity, imported
helper evidence, runtime-gap classification and tamper rejection. A further
26 owner tests pass across artifact declarations, semantic expansion, package
manifest contracts and active LSP artifact selection. Full-target compiler
Clippy passes with `-D warnings` after removing unused retired-parser fragments
and keeping optimizer source-order chunks as single-owned indexed statements.
After that optimizer cleanup, all five Lock artifact and seven native lifecycle
tests pass again, including the real VM branch corpus. All four `cellc expand`
CLI owners pass with `--test-threads=1`. Both modified bundled manifests were
explicitly re-locked with the current native binary; the semantic-foundation
example's subsequent CKB-profile check reports `status = ok`.

## Entry signature checkpoint

Entry source markers are now prefix-only. Seventeen parameters in six snippets
across four files passed lossless parser, exhaustive AST semantic equality and
idempotence checks before migration. An executable before/after probe retains
identical ELF, typed records and witness bytes. The focused run passed 32 tests
across frontend boundaries, fuzzy-debug and Lock artifacts. Contextual nominal
types named `witness`, `protected` or `lock_args` remain ordinary types.

The former `ParsedActionBody::finish(inline_outputs)` distinction could make a
punctuation migration add output policies to partial validators. Every action
now has one source-order/occurrence surface, and declared output roles have the
same order in either spelling. Source lifecycle operations retain complete
accepting-path accounting; a declared output alone does not invent data, Lock,
Type Script or capacity guarantees. Test fixtures select their artifact through
an explicit leading test-only context rather than the presence of this surface.
Twenty-five fixture contexts were inserted after semantic AST equality and
idempotence checks; 68 owning integration tests passed.

Fourteen immutable baselines cover kernel creation, partial resource/shared
validators, reversed creation order, named plus anonymous outputs, complete
replacement, anonymous returned Cells and ordinary values, with both absolute
and applicable fixed-group contexts. Thirteen accepted bundles retain exact
ELF, typed semantic and witness bytes under the unified model and after inline
output conversion, and independently validate. One pre-existing fixed-group
kernel-creation case still rejects with V2419 for unsupported preservation
evidence; changing its DSL policies to conceal that boundary is not a migration.

The shared lossless parser prepared 1325 token edits for 196 source fragments
in 79 files. Exhaustive semantic AST equality, matching frontend admission and
formatter/migration idempotence passed before applying those edits; host content
hashes and non-overlapping byte ranges were checked at application. The broad
regression passed 1327 tests: 1238 library (including the two temporary migration
probes), 21 frontend boundaries, 19 grammar contract, eleven native group,
seven native lifecycle, five resolved binding and 26 schema upgrade tests.
The retained log is `target/unified-entry-migrated-regression-tests.log`.
`git diff --check` also passes.

A second parser-proved migration covers 54 fragments through 220 token edits in
five files: the deep generators, JSON audit cases, malformed parser/replace
fixtures and a schema-upgrade template. Rust format templates use mapped sample
projections. Malformed fixtures compare their parsed header structure and retain
the exact original rejection code/message; their invalid bodies are unchanged.
All seventeen indexed NovaSeal historical resource snapshots retain their exact
hashes and byte extents. Their archived source is not an executable current
grammar fixture.

The duplicate named-output parser is now removed. Single and tuple named-arrow
outputs receive E1032 through the parser, formatter, diagnostics, metadata and
LSP. Ordinary scalar/tuple/helper returns and anonymous Cell returns retain
their own domain. Fifty-nine owner tests pass across authoring replacement,
frontend boundaries, grammar contract and resolved bindings, including exact
ELF/typed/witness equality when named output ports are interleaved with other
parameters. Replacement schema/field checks now use resolved typing instead of
a local-declaration-only frontend check. Compiler cache generation v217 rejects
cached admissions from the earlier candidate. Inferred source migration, the
complete documentation/consumer pass and final gates remain pending.

The later library run passed 1235 tests and exposed one stale frontend owner
that expected resolved replacement field/type errors during parsing. That owner
now requires parse acceptance followed by the exact missing-field, mismatched
schema and non-Cell input rejection in resolved typing; its focused rerun passes.
The six syntax-combination owner tests also pass, including all 96 seeded
semantic-phase probes. Website and editor submodules are initialized at their
existing pins; their distributed contract is not yet updated or validated.

The next migration uses resolved declaration kinds and the existing parameter
source/reference/authority checks. It compares the entire AST after making the
resolved provenance explicit, retains all typing and frontend diagnostics, and
checks formatter/migration idempotence. Unknown imported types and differences
such as mutable default-value parameters require separate evidence rather than
spelling-based source guesses. Bare entry references are already rejected by
the current parameter-reference boundary, so they are not a reason to invent
a CellDep or protected binding during this migration.

That migration now applies 1157 prefix insertions to 556 source fragments in
122 files. Every changed source passes exhaustive AST equality after making
the resolved parameter provenance explicit, unchanged typing/frontend
diagnostics, source formatter verification and migration idempotence. Application
rechecks original host hashes, unique edit positions and byte ranges. Ninety
parameters remain classified as unresolved or already invalid; imported Token
schemas, context-dependent snippets and existing reserved/reference failures
are retained for separate owner review. Implicit entry sources have not yet
been retired in this intermediate candidate.

Five executable differential cases (ordinary values, Locks, owned inputs,
partial shared validators and complete replacement) independently validate and
retain identical ELF and witness bytes plus identical typed executable entries
after normalizing only their explicit source labels. A sixth incomplete
disposal rejects identically. The immutable baseline is retained outside the
target tree as `/tmp/cellscript-entry-source-executable-baselines.json` with
SHA-256 `85ea9dc721f361ddb3d733de1bf11240d9b221ee09f228a8f2ca39ec219f9166`.

This proof exposed two existing consistency defects. Inferred and explicit
inputs now share frontend path checks. Two borrow-negative fixtures now dispose
of the root on both branches, so they continue to exercise the intended borrow
boundary instead of an unrelated missing disposition. Ownership diagnostics
also use a stable name order, eliminating hash-map-dependent left/right errors.
The associated broad regression is running; these changes do not establish
final gate completion.

The source formatter probe also found that direct match-arm require messages
could be detached by a newline after their comma. Lookahead now ignores
newlines and comments. A permanent owner regression passes six original/formatted
source variants with equal ELF and typed entries, independent validation and
24 real VM success/failure executions. The positive fixture is included in all
three syntax-combination mode lists. Browser/editor execution remains pending.

The post-migration library run passed 1237 tests and found one display assertion
still expecting markerless scalar arguments. Updating that assertion to its
new witnessed signature leaves all 23 formatter owners passing. The full
independent-checker run passed 278 of 279 tests; its remaining failure was a
test constructor searching for the removed `-> after: Document` substring.
Both affected constructors now remove the inline output port, explicitly create
the anonymous output and insert the ordinary return, with assertions that each
mutation executes. The checker mutation owner and the matching real-VM codec
owner pass. All 33 bounded nested tests and six resolved binding tests pass;
the other owner binaries pass eleven replacement, 25 frontend, 20 grammar,
seven native lifecycle and 23 policy import tests. Six syntax generator owners,
including all 96 semantic-phase probes, pass again.

Loading imported Token schemas through the package boundary reproduced a stale
locked dependency source digest after the intentional syntax edits. Seven
affected example locks (AMM, atomic swap, DAO, NFT, timelock, vesting and launch)
have been explicitly repinned with `cellc lock --json`; all seven report `ok`.
The AMM metadata command subsequently succeeds through the locked graph. The
remaining source migration now uses that compiler-owned loaded project and its
package-aware resolver; it does not supply guessed external schemas.

The locked-graph pass proves and applies another 28 input markers in fourteen
standalone/package example sources. The launch lock is repinned again after its
AMM dependency's source changes. Metadata commands for all seven affected
packages succeed at schema 77. All six AMM typed executable entries remain
identical after normalizing only the five newly explicit input-source labels.
The remaining 62 entries are context-dependent Rust/document snippets or
already-invalid boundary probes; they still require owner review. The temporary
parameter migration probe remains registered until this closure, and must be
removed before final gates. Explicit-source retirement, consumer rebuilds and
the full F01–F14 completion audit remain pending.


## Explicit entry provenance retirement

Captured same-owner source contexts resolve and prove another 24 prefix
insertions in eleven files, including the deep generator's actual Coin schema,
package imports and LSP fixtures. Seventeen reviewed ports in seven files are
partial tutorial/parser/formatter examples: their documented input-Cell role is
made explicit, with lossless AST equality after only that provenance change,
unchanged existing diagnostics and formatter/migration idempotence. This latter
proof does not claim that an absent schema was resolved or that an abbreviated
snippet is independently executable. The remaining 21 default ports are already
invalid reference, collection, generic, opening or reserved-name boundary probes.

Valid action and Lock parameters now require explicit provenance. Resolved
typing, ownership, reference, effect and ABI boundary errors take priority; only
an otherwise valid module gets E1032 with its resolved input/witness spelling.
Ordinary helper arguments remain unqualified, and `read name: T` retains its
explicit CellDep meaning. Compiler cache generation v218 prevents earlier
candidate admissions from bypassing this rule. The syntax-combination matrix
includes an implicit-source rejection in all three modes.

The migration exposed a witness mutability discrepancy: a leading `mut` had
been valid on a default scalar entry argument but rejected on the corresponding
witness argument. Witness provenance now remains independent of local value
mutability; Cell/reference/owner, protected and Lock-args restrictions remain.
The bounded Lock artifact and independent checker agree on this distinction.
Permanent owner cases cover resolved imported aliases, migration diagnostics,
ordinary helpers/read ports and mutable witness values; their broader run is
not yet a completed result.

An immutable eight-case pre-retirement baseline contains seven accepted
ordinary/mutable action and Lock, input, partial shared and complete replacement
cases plus one existing incomplete-disposal rejection. After retirement, all
seven canonical cases retain exact ELF, witness bytes and complete executable
typed entries and independently validate; their old accepted spelling now gets
E1032. The eighth rejects identically. The proof passed in
`target/explicit-provenance-retirement-proof.log`; the immutable baseline is
`/tmp/cellscript-entry-source-with-mutable-executable-baselines.json`, SHA-256
`6984b93d03aee079335f125ca9ec44f76576e5e60df0ce315f55081e823b99e6`.
The temporary migration/verifier module and its registration are removed.

`cargo fmt --all` completed. The broad post-retirement regression could not
finish compiling because the shared disk filled; LLVM/linker reported ENOSPC.
No test pass is inferred from that run. After disk space recovered, the same
library/checker/frontend/grammar/Lock/lifecycle/import selection was restarted
with two build jobs and incremental compilation disabled; its retained log is
`target/explicit-provenance-closure-regression-retry.log`.


## Post-retirement owner findings

The first complete library run after the disk recovery executed 1236 tests:
1233 passed and three migration owners failed. The navigation fixture still
used the character column from before insertion of `input`; the parser owner
still expected `ParamSource::Default`; and the built-in Option specialization
positive fixture retained a markerless value. Those owners now target the actual
type token, explicit input provenance and an explicit witness respectively.
The Option case also exposed an unhelpful internal monomorph name in E1032;
source guidance now uses the existing specialization-aware source-type renderer,
with a permanent `Option<u64>` diagnostic/acceptance regression.

The updated cookbook destruction-policy block independently compiles through
`cellc metadata` at metadata schema 77 with no diagnostics. Retired verification
markers and compatibility prose in that active recipe are removed. Tutorial
signatures and prose now describe input/output ports instead of left/right
arrow positions; Lock predicate examples return their required Boolean value.

F09 now has explicit owner regressions for complete replacement with/without
redundant bare lineage, local preservation field whitelists, and replacement
with/without a declared state edge. The tests compare executable/typed results
where equivalent and execute differing field/state cases in CKB-VM. Results
remain pending until the current run terminates.

F06's previously recorded fixed-group kernel rejection is still outstanding.
The legacy `checked_successor_pairs` producer summarizes a consume/create pair
without a `SuccessorExecutionBinding`, while the independent checker requires
executable relation evidence for group-bound successors. The complete-replace
execution contract also mandates capacity preservation, which this kernel does
not promise. Repair must represent and independently verify the actual weaker
kernel contract and occurrence scope; adding capacity/Lock constraints to the
source or removing the checker's evidence requirement is not equivalent.


The next library run passed all 1236 tests. The independently running checker
binary exposed seven positive fixture generators that still emitted markerless
scalar/key parameters; seven signatures in four allocated-value owner files
now use explicit witness sources. Their mutation oracles are unchanged and
require a focused rerun. A source-corpus inventory also found the scalar-field
borrow generator, now explicitly `input token`, and a stale CLI formatter
assertion, now expecting the already-migrated witness spelling.

A direct mutable-witness boundary probe showed that relaxing the local scalar
binding restriction also admitted a mutable BoundedList plan, whose former
default-source spelling was never supported. The witness source check now
retains immutable bounded-plan, Opening and handle bindings while allowing
ordinary mutable value parameters. Permanent negative owners cover the plan
and Opening cases. This final restriction change is newer than the completed
1236-test library snapshot and needs its focused rerun and final gates.


## Lineage composition and quantity-destruction boundary

The stale full-checker snapshot was intentionally stopped after seven fixture
failures had been identified and repaired on disk. Its 1236 library tests passed;
its incomplete checker run is not a passing result. Focused current-source
checks pass all 28 frontend boundaries, 20 grammar contracts and six Lock
artifact owners. The seven corrected checker generator owners are being rerun
individually with their existing mutation assertions and nonzero exact filters.

F09's original whitelist fixture omitted the independent check for its other
named-output field, so the existing completeness guard correctly rejected every
transaction. The fixture now constrains that field through a separate witness
and verifies both changed and unchanged values, plus mismatches. The compiler
has not relaxed named-output completeness. A separate test distinguishes a
selected state edge from the full declared graph and retains action-bound edge
requirements. Complete replacement with/without the redundant bare transition
retains identical ELF and typed semantics. All fourteen replacement owners now
pass, including the real-VM field and state cases; a shared positive seed is
added to all three syntax-combination modes for the next run.

That strict enum test found an admission mismatch: the complete output verifier
already handles resolved one-byte enum values, but the one-to-one successor
classifier required every non-alias variable update to carry u64 predecessor
roots. It now recognizes an available tag-only enum field assignment after the
existing full output-field/codec checks. Payload enum and numeric update bounds
are unchanged. Cache generation v219 identified that repair before v220 below.

A direct metadata probe reproduced a distinct F06 defect: `burn_amount` was
labelled `Retired` with `checked-runtime` enforcement. Codegen also routed it
through a same-Type-hash output-absence scan. This does not implement the
documented runtime-required quantity delta and can reject valid continuations
for the wrong reason. Quantity destruction is now a consumed quantity obligation
with canonical `runtime-helper-required` enforcement and no object-absence claim.
The strict surface classifier rejects that deferred kernel. Permissive artifacts
abort with the registered error 88 instead of running a retirement scan. A
permanent regression independently validates the artifact and observes error 88
in real CKB-VM while the corresponding explicit singleton retirement retains its
separate metadata kind. That focused regression passes in
`target/quantity-burn-boundary-focus.log`. This is a fail-closed boundary repair,
not a claim that a quantity-delta implementation has shipped. Cache generation
v220 invalidates the earlier erroneous admissions.

F06 still requires occurrence-aware mixed kernel/source summaries and the
previously recorded fixed-group kernel successor evidence repair; F07–F08,
F10–F12, F14, distributed consumers, the final gates and delivery remain open.


The quantity regression first exposed a vocabulary mismatch in the producer:
foundation enforcement uses the closed `runtime-helper-required` label, not the
ProofPlan's `runtime-required` status. The producer now uses that canonical
label without extending the checker vocabulary. After correcting the zero-arg
VM harness invocation, the artifact independently validates and real CKB-VM
returns exactly error 88. The latest frontend owner selection passes 28 tests,
and the replacement selection passes fourteen before switching to the shared
seed. The shared seed adds initial/terminal declarations and is queued for its
own regression; no distributed evidence is inferred from these native passes.


## Verified native checkpoint after the boundary repairs

All seven corrected allocated-value generator owners pass their original
independent-checker mutation suites with exact, nonzero test filters. The
results are retained in `target/entry-source-checker-generator-recheck.json`
and its log. The subsequent current-source selection passes one rehashed
quantity/retirement-forgery owner (including an unreachable-branch control),
five runtime-error registry/doc owners, fourteen replacement owners using the
shared enum-state seed, twenty-eight frontend boundaries, twenty grammar
contracts, six Lock artifacts, eight native lifecycle owners and the serial
CLI formatter owner. The generated executable-surface matrix is refreshed from
the compiler and explains the quantity-delta boundary. The retained aggregate
is `target/syntax-closure-owner-results.json`; every selected Rust target runs
at least one test and exits successfully.

The independent checker rejects relabelling an actual quantity-destruction
operation as a checked object-retirement disposition after disposition,
semantic, source-map and bundle identities are rebound. Its check follows the
executable CFG's constant-branch reachability, so a dead quantity branch cannot
invalidate the actual supported retirement. This check is separate from the
existing occurrence-level validation for structured retirement.

A new deep syntax-combination checkpoint is running against the current native
binary and the expanded shared seed list. Distributed WASM/editor execution,
F06's mixed occurrence/kernel evidence, F07–F08, F10–F12, F14 and final commit,
backend and CI gates remain required. No commit or push is claimed here.


The first new deep audit found five generated manifests still using the removed
single-entry `action` alias and a sixth contract failure: five required shared
branch fixtures were listed by the matrix but never loaded by the seed-directory
scanner. The runner now emits `entry` and loads required `.cell` origins outside
the seed directory, independently of the generated-case budget. Its selection
regression checks every required source across quick/ci/deep/repro and both
normal and reduced budgets. All six syntax-combination owner tests pass,
including the 96 semantic-phase probes. The repeated deep audit passes **137
cases: 84 expected accepts, 53 expected rejects, zero failures**, retaining its
report at `target/syntax-combo-audit/20261126-082447-deep-20260503/report.json`.
This is source/metadata/assembly evidence, separate from the real-VM owners.


## Mixed kernel occurrences and fixed-group successor contracts

The native candidate now uses metadata 78, typed semantics/lowering v15,
semantic foundation v7 and disposition-node domain v4. Package interfaces and
V0 hash/type-rendering domains are unchanged. Cache generation v222 invalidates
older kernel-occurrence candidates.

Direct consume, destroy, create and create-unique summaries carry their actual
typed block, operation index, opcode and local. Structured policies suppress
only their own instructions, so a retire/consume or fresh/create branch pair
retains both paths. The independent checker reconstructs the kernel's limited
envelope, rejects erased/misbound execution and invented capacity policy, and
checks each executable occurrence before combining role events. Source-policy
roles require exactly one event on every accepting path. A plain conditional
output can instead be checked by ordinary requirements on another branch;
the existing output-field completeness rule remains mandatory there.
The frontend discovers complete output roles before path accounting so a raw
create can satisfy that role on the other branch without making all raw output
checks require lifecycle coverage.

The previously retained fixed-group kernel rejection is repaired without adding
capacity equality to its source. A separate `fixed-group-kernel` successor
execution contract binds the actual consume/create sites, field operands,
explicit Lock target and group coordinates. Checked current-Type-group
membership supplies Type Script hash equality. Its capacity remains
`builder-set-and-chain-checked`, while complete source replacement retains its
four mandatory treatments. The real-VM owner accepts equal and unequal output
capacity, rejects changed output data and rejects extra current-group outputs.
The checker owner rejects four rehashed attempts to promote, erase or misbind
this weaker relation. Existing anonymous/receipt/advanced-operation parity still
requires its broader audit; it is not inferred from this one-to-one fixture.

Two mixed branch fixtures and the fixed-group kernel source join every shared
syntax-combination mode. The initial owner checkpoint passes 17 independent
successor/lifecycle checker tests and ten native lifecycle tests, including the
partial-output branch control. The previous six-target regression passed 95
tests before the fixed-kernel contract was added. Its first full-library retry
then passed 1219 and failed 17 tagged-policy cases because the new contract
selected only single-entry artifacts. It was retained as a failed result in
`target/kernel-contract-current-regression.log`. Resolved policy-witness
variants now select the same bounded group-kernel contract; all 22 owning
artifact VM tests pass in `target/kernel-tagged-policy-recheck.log`.
The complete library and six integration owners are being rerun against that
repair in `target/kernel-contract-current-regression-recheck.log`.

No final gate, distributed pairing, commit or push is claimed at this
checkpoint. F06 advanced-operation parity, F07–F08, F10–F12, F14 and the final
consumer/delivery audit remain open.


The post-tagged repair rerun passes all 1236 library tests and all 97 tests
across the six selected integration owners (1333 total). A further permanent
regression first reproduced the legacy quantity guard rejecting a real
retirement because a different branch contains quantity destruction. Direct
kernel records now use their exact instruction policy for that guard. The
latest independent checker owner passes 17 tests (including quantity-retirement
forgery); the separately selected native lifecycle owner passes eleven tests,
including real-VM success for retirement and exact error 88 for the quantity
branch. The zero-test native target accidentally selected by the checker-only
filter is not counted as evidence; the eleven-test standalone run supplies that
owner's result.

The rebuilt native binary and expanded corpus pass the new deep audit: 140
cases, 87 expected accepts, 53 expected rejects, zero failures. Its report is
`target/syntax-combo-audit/20261126-094944-deep-20260503/report.json`.
All six syntax-tool owner tests pass with an explicit binary target, including
96 semantic-phase probes and required physical origins across modes/budgets.
Doc-status checks pass. Compiler/checker clippy over all targets passes with
`-D warnings` after replacing two unnecessary sort/Copy-clone forms; formatting
and `git diff --check` pass. The retained per-owner checkpoint is
`target/kernel-contract-checkpoint.json`. These checks do not replace the final
commit/backend/CI gates or distributed browser/editor execution.

## Paired kernel and public Cell policy checkpoint

The subsequent native candidate uses metadata 79, typed semantics/lowering
v16, semantic foundation v8, disposition-node domain v5, package interface v7
and nominal declaration catalog v4. Cache generation
`project-source-set-v224-authoring-v9-paired-cell-policies` invalidates earlier
candidate records. The source epoch remains authoring-v9; V0 rendering/hash
domains and the typed physical-layout hash v2 are unchanged.

Two permanent interface regressions first reproduced equal public interface
hashes after changing only a declared Cell identity policy or a receipt's
declared Cell result. Interface, typed-semantic and nominal records now carry
those declaration policies. Identity changes produce breaking effects/capability
comparison findings; changed receipt results produce breaking source API
findings. Independent interface inspection compares source declarations, typed
types and the nominal catalog using qualified nominal resolution. A same-layout
`Other` type cannot substitute for a declared `Token` result. Omitted receipt
Cell results remain absent from the declaration records; the old scalar `u64`
fallback belongs only to the claim-operation boundary and remains
runtime-helper-required. Explicit scalar result declarations still reject.

Paired unique replacement, receipt claim and settlement records retain their
actual typed input/result locals, instruction sites, identity and declared result
types. The independent checker reconstructs both ports and their accepting-path
coverage. Removing the binding cannot relabel an advanced operation as a legacy
inferred successor. Cell-result operations retain builder-evidence-required
enforcement and their existing weaker envelope; the new typed binding does not
promote them to machine certification. Expansion exposes the result local and
operation value definitions. Four physical source fixtures join the required
origins in every syntax-combination mode.

A mixed receipt-helper/fresh branch reproduced missing output-field accounting
in the old stdlib-helper path. Frontend role accounting and type-checker branch
obligations now recognize the declared second output and its explicit preserve
fields for the closed lifecycle transfer/settle and receipt claim helpers.
Missing fields still reject. Imported, aliased and fully qualified receipt
results use the existing declaration-owner resolver; all three routes have a
permanent metadata regression.

The full library and six selected integration owners pass 1239 plus 100 tests
(1339 total) before the final mixed-helper repair. Current focused reruns pass
all 13 native lifecycle tests, seven resolved-binding tests, 52 type-owner
tests and 111 standalone-checker library tests. The independent lifecycle owner
passes 19 tests, including rehashed result/identity/binding/enforcement forgeries
and the equal-layout nominal substitution case. All eleven public-interface
owner tests pass. Compiler/checker clippy over all targets passes with
`-D warnings`. Initial failed reproductions and the corrected scalar-result
assumption are retained in the `target/advanced-*.log` checkpoints; an earlier
test's explicit-scalar equivalence expectation was removed because that source
was already invalid. These results do not replace final full-suite/gate or
distributed-consumer evidence. F07, F08, F10, F11 and the final F12/F14 consumer
audit remain open; no commit, push or final target cleanup is claimed here.

The rebuilt native binary subsequently passes the expanded deep corpus:
144 cases, 91 expected accepts and 53 expected rejects, zero failures. The report
is `target/syntax-combo-audit/20261126-110048-deep-20260503/report.json`.
All six syntax-tool owner tests and the current documentation-status check pass;
`git diff --check` also passes. These are focused checkpoint results, with the
final commit/backend/CI gates and actual browser/editor pairing still pending.

## Declaration-derived operation identity checkpoint

Fresh creation may omit its identity clause. Unique creation and replacement
may use empty argument lists, `create_unique::<T>()` and
`replace_unique::<T>() input`. The source AST retains omission separately from
an explicit `identity = none`; formatting preserves that distinction. Type
checking derives the effective policy from the resolved Cell declaration and
rejects conflicts, including an explicit non-unique Fresh request against a
unique declaration. Unique operations still reject a non-unique declaration.
Retirement's exact absence policy, destination Locks, capacity policy and Type
ID builder requirements remain independent.

The frontend ownership walk no longer needs an identity-bearing Fresh kernel
before declaration resolution. Checking/lowering request transient elaboration
with the resolved policy. IR import context now retains declaration identity
policies, and qualified named output types join the retained-type discovery
closure. The typed operation/disposition retains a concrete policy under both
spellings; no extra source spelling enters the wire or V0 hash domains. Source
cache generation v225 invalidates earlier admissions. The current metadata,
typed, interface and nominal schema versions remain 79/v16/v7/v4.

The shared flow walk inspects Fresh fields directly rather than choosing a
pre-resolution non-unique placeholder. State-range checks therefore apply to
both default and unique declarations. Simulation derives local Fresh policies
without presenting opaque unique operations as ordinary creation traces; an
unresolved imported declaration cannot receive a guessed default. Its result
remains simulation-only evidence. Lossless formatting also now receives the
complete generic-argument syntax range for the dedicated read/unique operations,
including empty argument lists with comments and newlines.

Native Type ID metadata previously rejected two lowered aliases of one
declaration as duplicate Type ID declarations. The reader now requires an
unambiguous retained defining module/name and matching declared Type ID before
allowing the shared identifier. Missing catalogs, conflicting or duplicate
bindings and different declarations continue to reject. The four permanent
metadata mutations recompute the outer typed hash before checking this rule;
equal fields or widths never justify shared ownership.

The independent checker now requires a unique creation/replacement's concrete
policy to match its Cell declaration and rejects `none` there. The new rehashed
operation-policy mutation previously reached a machine-binding rejection
(V2420); it now rejects directly at the declaration/typed boundary (V2419).
This is earlier independent policy validation, not evidence that the old ELF
mutation was accepted. The owning checker suite passes twenty tests.

Initial focused checks pass 49 explicit/derived metadata pairs (local plus
imported, aliased and qualified routes), thirteen exact native ELF/typed-record
pairs across the identity policies, and selected witness/VM success/failure
comparisons. They also cover formatter idempotence, LSP acceptance/conflict
diagnostics, and missing unique authority. The Type ID cases retain their
required `#[type_id(...)]` declarations; the first probe omitted that prerequisite
and was corrected instead of weakening metadata validation. A simulator test's
initial unsupported-result expectation was corrected to its existing opaque
simulation contract. The intermediate logs retain those failed attempts.

The current frozen source-input index is
`target/declared-identity-frozen-inputs.json`, SHA-256
`d4f8dcbd058ddd3c033310b1e33ccb5a7b4c533c73fb30065218e1d5f56d653d`.
It passes 1241 library tests plus 88 in five integration owners (1329 total),
twenty independent lifecycle checker tests, 111 standalone checker unit tests
and compiler/checker clippy over all targets with `-D warnings`. Its WASM check
passes 31 and fails four: three retained fixtures still use `lock_script` or the
removed single-entry manifest key `action`, and the new identity conflict is
reported by three separate prechecks.

Fresh target and field-coverage prechecks now walk their source structure
without resolving identity; only the actual statement check reports that policy
error. This avoids duplicate diagnostics while retaining independent errors.
Native/LSP and WASM tests require one conflict diagnostic. The WASM fixture keeps
the old Lock source as an E1032 negative control and selects the current `lock`
through an explicit Lock artifact. Group fixture manifests use `entry`. All
35 WASM-feature tests subsequently pass; the zero doctest count is not evidence
of additional tests.

The repaired source-input index is
`target/declared-identity-final-inputs.json`, file SHA-256
`369a2ee89892415c23d4835a59d1eeee938c37268a3f3b0d08a6b78121f958f1`.
That index was captured while the sequential test command was still formatting
one integration test. Reconstructing its pre-format text reproduces the recorded
hash; the only difference is whitespace in the new diagnostic-count assertions,
applied before Cargo compiled the tests. All compiler/runtime input hashes match.
The verified post-format index is
`target/declared-identity-verified-inputs.json`, file SHA-256
`7f472fa841e46e57cef9d45c86e7487291acc6e1faeb4789acf0638c684c8a1e`.

After the precheck repair, the complete library plus five integration owners
again pass 1241 plus 88 tests (1329 total). Clippy over compiler/checker all
targets passes with `-D warnings`. The repaired WASM suite passes 35 tests;
enabling its optional language service passes 38 tests. The rebuilt native
binary passes the expanded deep corpus: 146 cases, 92 expected accepts,
54 expected rejects, zero failures. The report is
`target/syntax-combo-audit/20261126-114944-deep-20260503/report.json`.
All six syntax-tool owner tests, documentation status and `git diff --check`
pass. These checks do not establish actual distributed browser/editor pairing
or replace final commit/backend/CI gates. F08, F10, F11, F12/F14 closure and the
remaining source/consumer/delivery audit are still open.

## Resolved policy presentation checkpoint

Native `policy_explanation` projects actual typed roles and dispositions into
one representation for semantic expansion and entry/Cell-role hover. Its rows
cover data, identity/accounting, Lock, Type Script, capacity and cardinality.
The four authority categories remain separate from the recorded enforcement or
builder-evidence boundary. Explicit includes declaration-owned and operation
policies, preserving the identical typed records for explicit and omitted
identity requests. The view displays selected entry, physical selector, scope,
membership and coverage. Exact group counts are reported only for fixed-group
coverage. Long hover details are bounded with a pointer to full expansion.
Native policy records remain outside the browser summary.

An actual VM probe executes the current Script on an input, with a correctly
initialized foreign output at absolute index zero and an incorrectly initialized
native output at index one. Ordinary Fresh and Pool accept the foreign output
and reject a wrong value at that same absolute index. The producer previously
claimed checked Type-group membership despite that behavior. The label now
follows the real role membership, and the independent checker reconstructs it
separately. Singleton identity retains its actual Type-hash comparison; CKB Type
ID hash-presence checks keep the builder-plan boundary. No backend behavior is
changed. Exact ELF hashes match the pre-repair baselines:

- Fresh: `47b2bd016d408621a9a79f91024044a1ac9e705aa587383b38e793f95a236d20`.
- Pool: `273e479c64bee2be85148e180a77a61a5b37cbe9e47bae3b0cf779c93c02f7d1`.

The first focused owner log, `target/policy-presentation-fixed-owners.log`, passes
15 native lifecycle and eight policy-presentation tests. It covers ordinary,
fixed and action-defined coordinates, full versus kernel successor capacity,
advanced builder boundaries, retirement scan scope and Lock responsibility.
Additional declaration-identity and bounded-handle presentation cases have since
been added and await the current owner rerun. Bounded handles retain their
resolved scan scope without inventing a scalar `[0]` selector.

A capacity-floor-only source exposed a separate stage mismatch in native
metadata validation. Metadata-only APIs preserve and validate the type's floor
and source, but do not generate artifact constraints. Requiring that duplicate
summary before an artifact exists made valid analysis fail. Its requirement now
begins when artifact hash or size is present, consistent with the existing CKB
summary stage boundary. The regression covers direct, diagnostic and loaded
in-memory APIs; actual artifact metadata still rejects a missing floor summary,
and a zero declaration floor still rejects. No synthetic artifact size or
capacity measurement is supplied to make analysis pass.

The first full-library run, `target/policy-presentation-library.log`, passed
1241 tests and failed one new hover regression. Its fixture omitted the compiler
requirement and therefore used SemVer `*`, which excludes the active alpha
release. File-filtered editor diagnostics were empty while semantic metadata
was unavailable. A direct CLI probe independently rejects that manifest with
E2600 and accepts it with `>=0.33.0-alpha.1`. The fixture now states that
requirement and asserts successful metadata admission before inspecting hover.
The dedicated pre-repair reproduction is retained in
`target/policy-presentation-hover.log`. The corrected LSP suite passes all
77 tests, including action and role hover under fixed versus action-defined
artifact selection. Independent authoring checker tests pass 21, including the
rehashed membership-policy mutations; standalone checker unit tests pass 111.
Consumer owners pass 21 grammar, five selected-artifact CLI, 15 native lifecycle
and initially ten policy-view tests. Compiler/checker clippy over all targets
passes with `-D warnings`.

The final owner rerun, `target/policy-presentation-final-owners.log`, passes all
15 native and eleven policy-view tests. The additional test confirms that a
bounded output plan keeps its dedicated collection contract and capacity floor
without inventing a scalar successor relation or full group counts. Collection
storage/bounds presentation remains F10. The two new physical fixtures now use
distinct module names; their exact ELF hashes above still match. All compiler
inputs since the repaired checkpoint are unchanged; the only three source-input
differences are those two fixture module names and the added policy-view test.
The current source-input index is
`target/policy-presentation-verified-inputs.json`, SHA-256
`3acf4aea730792140e001c29e997c3b1ed314e5a17b769922d7af755c643da72`.

The rebuilt binary passes the expanded deep corpus: 148 cases, 94 expected
accepts, 54 expected rejects, zero failures. Its report is
`target/syntax-combo-audit/20261126-130946-deep-20260503/report.json`.
All six syntax-tool owner tests, documentation status and whitespace checks
pass. These results are not final gates, distributed pairing
evidence or delivery. No commit or push is claimed.

## Collection storage explanation checkpoint

The native report now uses `cellscript-collection-budget-report-v2`: `owners`
retains the existing declaration reserve/growth scenarios, while `storage.values`
and `storage.schema_fields` describe actual resolved IR storage. The shared
projection distinguishes fixed stack vectors, caller-backed vector parameters,
encoded views, arena owners, bounded Cell selections, output plans and nested
read-only codecs. Source-binding hints retain actual local IDs and lexical
declarations; they supply no executable instruction or serialized/hash authority.
The existing runtime schema-pointer analysis owns encoded-versus-local admission.

Four new analysis tests and three new hover tests pass. They cover fixed-width
aliases, disjoint lexical scopes, rejected shadowing, value-block and match
bindings, structured field/index/payload views, read-only arena forwarding and
unsaved imported generic width changes. Three serial CLI owner tests pass,
including report v2 without artifact or lockfile writes. One actual VM regression
executes ten cases under normal and non-optimized compilation: local `Vec<u64>`
capacity remains 32 for requests 0, 4 and 4096; claiming the requested capacity
instead rejects. No allocation behavior is changed.

The completed broader commands retain these logs:

- `target/collection-storage-library.log`: 1249 passed, zero failed.
- `target/collection-storage-integration.log`: 37 passed across structured
  vectors, policy explanation and resolved-binding metadata.
- `target/collection-storage-clippy.log`: compiler and independent checker,
  all targets, `-D warnings`, passed.
- `target/collection-storage-wasm.log`: 35 passed.
- `target/collection-storage-wasm-language-service.log`: 39 passed.

The pure `collection-explanation` feature is enabled by native CLI/LSP and
forwarded by the optional browser language service without enabling the native
protocol runtime. Default WASM excludes this projection and source hints.
Grammar, support matrix, gate policy, Tutorial 06 and the fixed 52-byte context
example now explain the actual storage boundary. These checks predate the
subsequent named-constant bound implementation; final gates and distributed
byte pairing remain required for the completed candidate.

## Named unsigned bound checkpoint

`UnsignedBound` and `BoundReference` retain source names and spans. Named-policy
definitions share their existing structured fields through a bound parameter;
`Item::BoundPolicy`, source array bounds and numeric argument references resolve
to the existing literal AST shapes before concrete semantic admission. They
never manufacture a zero length, encoded nominal name or new wire domain.
The resolver admits declared unsigned scalar integer literals and reference
chains, checks each scalar width, follows defining-module visibility and import
aliases, and rejects computed expressions, inaccessible names, cycles and chains
beyond 32 references. Existing per-family range/alignment/codec checks remain.
Ordinary nominal applications still accept type arguments rather than a guessed
constant argument kind. Cache generation is v227.

Template signatures normalize in the defining module before import/specialization;
instantiated helper bodies retain that module's bound scope. An imported generic
record and helper using a private four-byte width remain four bytes when the
consumer declares a same-name nine-byte constant. Formatting retains source
names, grouping and comments; normalized interface and V0 identities keep their
existing literal domains. Eight owner regressions include nineteen independently
validated ELF bundles, literal-equivalent ELF/interface comparisons, all three
named policy families, contextual names, exact reference-depth boundaries,
private/imported references, unsupported expressions and editor revision changes.
The real CLI probe records 256 arena bytes, three allocation events and three
eight-byte members, with no artifact or lockfile writes.

The first broader library run passed 1255 tests and failed one existing role
parser test that still required a constant name to reject during parsing.
That case now checks successful source parsing followed by inaccessible-constant
rejection; the exact owner rerun passes. The new editor fixture also now states
its visibility explicitly rather than incorrectly treating W2500 as an error.
The final library run passes **1257**, zero failures. Compiler/checker all-target
clippy passes. Default WASM passes **36**, optional language service **40**.
The final deep corpus passes **151** cases: **96** accepts, **55** rejects,
zero failures. The report is
`target/syntax-combo-audit/20261126-151009-deep-20260503/report.json`.
All six syntax-tool tests and documentation status pass. Logs are
`target/named-bounds-final-library.log`, `target/named-bounds-validated-owners.log`,
`target/named-bounds-final-clippy.log`, `target/named-bounds-final-wasm.log`,
`target/named-bounds-final-wasm-language-service.log` and
`target/named-bounds-final-deep.log`.

## Distributed consumer checkpoint

The canonical Docker build first failed while downloading container packages:
the configured download proxies use the host loopback interface. The script's
documented `CELLSCRIPT_WASM_DOCKER_NETWORK=host` option resolves that boundary;
the pinned image, tool versions and hash checks are unchanged. The completed
bundle is 2480975 raw bytes and 924996 gzip bytes, under the 1500 KiB budget.
Its SHA-256 is
`367a2639e1f7d1eac19f718f6a76bd83d457f8bd0df69eca0410aedf8d8dbf50`.
The log is `target/authoring-v9-canonical-wasm-host-network.log`.

The actual distributed WASM passes 54 browser tests after updating the current
source-contract/metadata pins, the single-entry manifest selector and the removed
Lock declaration. Seventeen shared native authoring fixtures execute through the
source-set API; test-only JSON selects the same explicit artifact context as
the native runner. Seven shared rejection fixtures execute through all three
browser compile APIs. The shared Lock fixture also selects its Lock artifact
explicitly, retaining helpers and multiple declarations. Its source admission
does not replace native artifact/VM evidence. The editor's source-contract pin
and validation pass. Logs are `target/authoring-v9-distributed-shared-corpus.log`
and `target/authoring-v9-editor-initial.log`.

Consumer closure is not complete: editor README source fragments, generated
website data, final consumer review and coordinated submodule reconciliation
remain. F12 inspection also reproduces the scoped-example owner's old expectation
that `assert_conserved` stays a gap. Current metadata marks both matching sum and
conservation forms checked, while the conservation relation label still says
metadata-only. The helper classifiers do not themselves inspect the declared
field width/offset. The example declares an eight-byte amount plus eight symbol
bytes, so actual helper-domain/VM evidence must be checked before treating the
failure solely as a stale test. The retained probe and failure are
`target/aggregate-current-cli-probe.json` and
`target/aggregate-scoped-example-owner.log`. No expectation is weakened here.
Final gates, original-checkout reconciliation, commit/push and target cleanup
remain pending.

## Resolved aggregate domains and accepting-path checkpoint

The native raw-u128 helper accepted a real transaction whose declared `u64`
amount changed from `2^64` across two inputs to zero across two outputs. An
adjacent eight-byte symbol supplied the upper limb, preserving the raw u128
sum. The control without that upper limb rejected. The pre-fix native ProofPlan
incorrectly claimed checked conservation; the retained failing log is
`target/aggregate-domain-counterexample.log`.

Conserved shorthand and matching explicit group sum equality now share the
resolved field domain. This helper requires a non-Struct Cell declaration,
u128 at offset zero and exactly sixteen fixed encoded bytes. The source field
may have another name. Shifted, padded, dynamic and u64-plus-bytes domains do
not acquire its evidence. Structured pool relations retain their own typed
field guards and numeric domains.

Coverage comes from reachable accepting paths of the actual selected entry,
including checked callees and emitted action preludes. A helper in a module-wide
access union is insufficient. Early returns, one unchecked branch and unselected
actions remain explicit gaps. A conditional direct helper no longer suppresses
a required unconditional copy/conservation prelude. A real VM regression checks
both branch values and a second group member that changes the total while the
named output remains unchanged. Invariant schema dependencies now survive entry
pruning alongside the retained invariant. Tagged dispatch requires every selected
variant; its existing independent scalar/Unit common-check contract still rejects
an unsupported raw group helper.

The explicit fungible entry uses a structured `IrEntrySelection` profile marker.
Its source invariant is a structural domain template. Profile ProofPlans now use
`selected-profile-invariant` and `selected-profile-aggregate`, with the checked
owner-or-conservation condition rather than checked unconditional equality.
Native validation rejects forged unconditional labels, removed conditional
coverage and duplicate/wrong-category source records. Ordinary source names
cannot select the profile. Owner-authorized issuance/destruction and non-owner
conservation retain the existing runtime helper and real VM behavior.

A delta's source argument is still rendered text, without a typed binding to
the actual helper operand. Its source obligation stays runtime-required even
when a minted/burned helper appears elsewhere. The previous library expectation
that promoted such a delta was corrected, retaining checked conservation for
its actual selected entry. The scoped-invariant example retains its u64-plus-
symbol schema and now honestly reports its metadata-only boundary.

`target/aggregate-domain-native-regression.log` records 1257 library, ten initial
aggregate and five Fiber VM tests passing. The later
`target/aggregate-domain-final-owners.log` records all eleven aggregate tests
passing, including the additional unconditional-prelude VM regression. These
are native compiler/VM and artifact validation results; no independent wire
proof for arbitrary source aggregates was added. Cache generation v229 rejects
results from the earlier classification.

## Reserved boundaries and generated authoring checkpoint

Default LSP action templates now emit prefix input and inline output roles.
The distributed editor's action and Lock templates and current README examples
also use the current contract. Native tests expand the actual editor JSON
snippets, including default references, then use the shared lossless frontend,
exhaustive AST semantic equality and formatter idempotence. The three snippet
owners pass in `target/authoring-v9-consumer-snippets.log`; editor validation
passes in `target/authoring-v9-editor-snippets.log`. Named and compact flow
forms remain distinct admitted declarations; no initial/terminal policy was
invented while checking them.

General mutable-reference diagnostics now explicitly say reserved, separately
from admitted mutable witness/local values and read-only CellDep/helper views.
The native boundary suite passes all 29 tests in
`target/authoring-v9-final-frontend-boundaries.log`. Shared source additions
cover exact-u128 conservation, braced validity with fields afterward, mutable
values/read-only views, and rejection of mutable references, launch and
unbraced validity. Their expanded syntax/browser runs remain pending.

The broader example run exposed two remaining validation failures, retained in
`target/authoring-v9-reserved-and-examples.log`: NFT package/read-source drift
between explicit equalities and preservation whitelists, and AMM compact
metadata at 459970 bytes against 458752. Thirty-four example tests pass. The
original baseline already has the NFT source discrepancy; reconciliation and
measured budget analysis are still required. Neither assertion has been removed
or its budget increased. This checkpoint is not a final gate or push claim.

The expanded shared native deep corpus now passes **157 cases: 99 accepts,
58 expected rejects, zero failures**, including all six new aggregate/validity/
reserved-domain fixtures. Its report is
`target/syntax-combo-audit/20261126-161313-deep-20260503/report.json`.
Compiler all-target Clippy passes with `-D warnings` in
`target/aggregate-and-reserved-clippy-final.log` after removing seven test-only
clones of a Copy diagnostics value. The WASM feature profiles pass **36** and
**40** tests, respectively, in `target/aggregate-and-reserved-wasm-final.log`
and `target/aggregate-and-reserved-wasm-language-service.log`. An earlier
invocation omitted the crate's required `wasm` feature and failed to resolve
native optional dependencies; the corrected commands use the documented
feature boundary without changing dependencies.

The canonical pinned-container rebuild produces 2485321 raw bytes and 927234
gzip bytes, below the 1500 KiB budget. SHA-256 is
`54fa384e80f963078e119cc343bafd56673415c74a197f1e21cf07ed3d805eb7`.
The actual distributed bytes pass all 54 browser tests with twenty shared
positive source fixtures and ten negative fixtures across all three browser
compile APIs. Logs are `target/authoring-v9-final-canonical-wasm.log` and
`target/authoring-v9-final-distributed-corpus.log`. Remaining website excerpts,
generated-data freshness, source/package reconciliation and final gates are
still required.

The native example-size diagnostic now measures all four complete envelopes
before enforcing their existing caps, retaining every assertion. Exact Rust
measurements in `target/authoring-v9-metadata-budget-measurements.log` are:

| Example | Compact total | Base | Nominal evidence | ProofPlan bytes | Records |
| --- | ---: | ---: | ---: | ---: | ---: |
| AMM | 459970 | 449965 | 10005 | 37181 | 39 |
| NFT | 784990 | 766731 | 18259 | 87915 | 92 |
| Token | 196215 | 196215 | 0 | 24784 | 27 |
| Vesting | 451957 | 440909 | 11048 | 57298 | 61 |

These measurements use the existing repository-relative provenance calculation.
The CLI's broader project source inventory is a different envelope and is not
used to authorize a test budget. Budget/source reconciliation remains open.

## Canonical examples and website closure checkpoint

The existing NFT and Timelock package copies used explicit local equalities,
while their reading sources retained preservation whitelists. Shared lossless
parser proofs expanded only the five NFT and two Timelock whitelists, preserving
field order and rejecting any internal comments requiring separate review.
The proofs rebuilt the changed source-order indexes, compared the entire AST
against the package sources, checked formatter idempotence and independently
validated both compiled bundles. Before/after ELF bytes and action parameter
metadata are identical. Input hashes were checked against the captured source
manifest before applying the reviewed candidates. Other source bytes are
unchanged. Temporary proof modules were removed from active compiler sources.
The proof logs and hash receipts are `target/nft-source-reconciliation-proof-final.log`,
`target/nft-source-reconciliation-hashes.json`,
`target/timelock-source-reconciliation-proof.log` and
`target/timelock-source-reconciliation-hashes.json`. The Timelock log's final
human summary inherited the word “five”; its executed assertion checks exactly
two edits. All ten package/reading-source pairs now agree.

Metadata 79 retains typed lifecycle/kernel occurrence and result bindings and
receipt result policy, so the former byte caps were rebaselined from the exact
Rust measurements above. AMM/NFT base caps are tightened to 440/750 KiB while
nominal caps remain 10/18 KiB. Total caps are 450/768 KiB. Token remains 192 KiB;
Vesting has a 431 KiB base and 11 KiB nominal allowance, totaling 442 KiB.
Proof text caps are 37/86/25/58 KiB, with the original record-count caps retained.
ELF ceilings, entry/action counts and source-inventory bounds are unchanged.

The same test now measures all 29 selected entries before enforcing the original
independent checks. Maxima (base bytes / nominal bytes / ELF bytes) are AMM
161756 / 9886 / 7120; NFT 212073 / 17884 / 15656; Token 89448 / 0 / 3328; and
Vesting 155605 / 10929 / 4792. Base caps are 159/208/88/153 KiB respectively,
leaving separate nominal allowances unchanged. Entry proof text and record-count
caps are unchanged. The diagnostic log is
`target/authoring-v9-entry-budget-measurements.log`. The final complete example
suite passes **36 tests**, including all full/entry/cache size checks, in
`target/authoring-v9-final-examples.log`.

The unused website `modelCards` export was removed after confirming no source
or build consumers. It duplicated current source previews and retained retired
entry spellings. Active hero/playground previews continue to read the checked-in
sources. The displayed metadata command now uses the JSON-default metadata CLI. Provenance, pipeline and assurance data
were regenerated with the current native compiler. The full website build
passes for mainnet/testnet, including TypeScript/Astro, 54 actual-WASM tests,
32 pages per build, link/dist checks and deployment-contract checks. Logs are
`target/authoring-v9-website-data-regeneration.log` and
`target/authoring-v9-final-website-build.log`. Canonical Cell formatting uses the
shared lossless formatter and its exhaustive AST/idempotence guards.

The README, branch context, source contract and unreleased changelog now name
the v9 candidate explicitly. Parent/child commit delivery, final complete gates
and preservation of the original dirty checkout remain outstanding.

## Initialized benchmark and preservation checkpoint

The backend gate initially found uninitialized evidence submodules. The pinned
NovaSeal commit was unavailable from its remote; its exact immutable object and
the pinned benchmark object were fetched read-only from the original checkout's
clean child repositories. Their parent pins were unchanged. No original child
files were edited and no historical resource snapshots were rewritten.

The active benchmark specifications at their pin still used implicit sources,
postfix Lock sources and named arrow outputs. A separate detached reference
checkout at `e134d663` used its shared lossless parser to prepare 47/40/25 token
edits for iCKB logic, limit order and owned owner. The complete AST comparison
normalizes only resolved local declaration-owned parameter provenance and the
old empty inline-output presentation surface; it asserts that no audit or
disposition is removed. Migration idempotence and reference-compiler exact
before/after assembly pass for all three sources. The current shared formatter
adds its exhaustive AST/idempotence check and its subsequent check passes.

All 77 action/Lock parameter records retain their names, types, order, mutability,
reference and physical ABI layout across compilers, after making source provenance
and its witness marker explicit. The current compiler accepts all three sources.
Limit-order assembly remains SHA-256
`a1edeb70df77a8262c25b9f08c0fb8b16f078e5abaa075935b9027e2aad547c9`;
owned-owner assembly remains
`cc9275cd7fd10736d5abdcd7ddacd843792179e1daf287e5b814c1679946885b`.
The iCKB logic's 48-byte IckbToken schema is outside the raw helper's exact
16-byte domain. The current compiler correctly omits that automatic prelude,
and the source conservation/delta records report runtime-required rather than
checked evidence. This intentional compiler correction is separate from the
signature migration. Historical transaction matrices, original binaries and
cycle/equivalence reports are unchanged. The benchmark README records this
boundary. Benchmark child publication still requires its own explicit
coordination under AGENTS.md; no benchmark remote push has occurred.

The native migration log is `/tmp/cellscript-benchmark-v9-migration-exact.log`;
the current parameter/source hash receipt is
`target/authoring-v9-benchmark-signature-proof.json`. The three current candidate
compilation logs and shared format check use the `authoring-v9-benchmark-*`
prefix in `target/`. The authoritative business-corpus writer refreshed only
the derived inventory digest after these source changes. The independent
current check passes; it did not change cases or their evidence classification.

A read-only three-way preservation preview found 23 automatically mergeable
overlaps, 34 user-only paths, eleven user deletions, one identical change and
three conflicts. Two source conflicts were prepared from the original user's
whole source through the reference lossless parser, resolving local Struct
arguments as witnesses. Nine order-book and four registry edits pass complete
AST comparison, idempotence and exact reference assembly, retaining every user
body change and comment. Both candidates compile with the current native
compiler. The remaining conflict is the derived business inventory digest,
which must be regenerated after the final live reconciliation. No original
working-tree or index mutation has occurred at this checkpoint. The original
preservation snapshot still matches every recorded file and deletion.

The commit gate has passed after restoring one existing unicode-ident dependency
edge in the Registry verifier lockfile; no dependency version changed. The
ongoing backend run has passed 1259 library, eleven aggregate and 43 allocated
runtime tests and is executing the independent-artifact integration owners.
This is progress evidence, not a completed backend/CI gate or delivery claim.

## Current consumer contract audit

The final current-shape audit found three missed consumers: the actual browser
worker still required v8, the Myelin handoff contract still declared metadata
76/lowering v13, and the effect-interface conformance fixture retained v6/v8
interfaces. The initial backend invocation had already embedded the latter two
JSON files into test binaries. It was intentionally terminated (exit 143),
retaining its completed 1259/11/43 results; it is not a completed backend gate.
The complete gate must rebuild and run again after these corrections.

A new behavioral browser regression executes the actual worker message handler,
adapting only its browser module URL to the local distributed glue. It uses the
real published WASM, SHA-256 manifest, integrity check and source-contract
handshake. Before the fix it returned compiler_source_contract_mismatch for v8;
the other 54 tests passed. After the worker's v9 cutover all **55 tests pass**.
Logs are `target/authoring-v9-worker-regression-before.log` and
`target/authoring-v9-worker-regression-final.log`. This closes a boundary that
direct WASM export tests alone had not exercised.

The Myelin JSON now declares actual v9/79/v16 identities, while external adoption
remains pending an exact release pin. Its owner additionally compares source,
lowering and source-map identities to the actual compiler/checker constants.
The five interface vectors were regenerated through ordinary native compilation,
not metadata-only analysis. Every source, callable effect fact and all eight
comparison outcomes are unchanged. Each interface changes only schema/version,
compatibility-profile identity and its derived deployment hash. All six effect
owners and two Myelin owners pass in
`target/authoring-v9-consumer-contract-owners.log`; the generation receipt is
`target/authoring-v9-effect-interface-regeneration.json`.

Current edition, public-interface, foundation, authoring-implementation, temporal,
trusted-external, Myelin, gate and verification-tutorial prose now names the
matching identities and ordinary Lock selection. Historical records keep their
original versions. The old authoring milestone's default-provenance/cache prose
is distinguished from the current prefix-source candidate.

The inherited standalone DOB proposal and NovaSeal skeleton sources reject in
both immutable v8 and current v9 with the same missing-module-semicolon and
removed-verification diagnostics respectively. Neither was an accepted current
compiler fixture before this change. Their pinned source/TCB bytes and historical
evidence are preserved; no compatibility or execution claim is added. The v8
reference log is `/tmp/cellscript-existing-proposal-admission-v8.log`; current
logs use `target/authoring-v9-inherited-*-admission.log`. The unchanged DOB pin,
like NovaSeal's, was materialized from the original clean local child after its
remote rejected a direct fetch of the fixed commit.

The complete website build now passes with the worker regression included, for
both mainnet and testnet; its log is
`target/authoring-v9-worker-final-website-build.log`. Final editor validation
passes in `target/authoring-v9-editor-final-validation.log`. Ten bundled package
checks (`token`, `nft`, `amm_pool`, `vesting`, `launch`, `multisig`, `timelock`,
`atomic_swap`, `multi_phase_dao`, `registry`) pass with `--frozen`; every lock
remains byte-identical. Their results are retained in
`target/authoring-v9-package-frozen-checks.json`.

The five initialized iCKB benchmark owners pass without changing their historical
matrix. The reviewed child changes are committed locally on
`codex/0.36-authoring-v9`: benchmark
`af2466ebd87d4b3474369535ac719f96b55e3e00`, website
`b4274d8410833e19f6d56ad32778db6f65fa9664`, and editor
`c7423d97eae68fa1a6f81893d391667a03ba061d`. No child remote has been pushed.
The original clean proposal pins remain unchanged. The current consumer commit
gate and documentation status checks pass. The rebuilt complete backend and CI
gates, final live reconciliation, publication coordination and target cleanup
remain required before completion.

## Registry consumer and Finance compatibility checkpoint

The Registry API still admitted package-interface v6 while native and independent
Rust consumers already required v7. Its admission and upgrade comparison now use
v7 and declaration-owned `identity_policy` / receipt `claim_output` fields.
An actual native-compiler fixture covers value, resource, shared and receipt
declarations. Negative cases retain the Rust consumer's policy vocabulary,
existing-field requirement and 512-byte UTF-8 reference boundary. Changing an
identity or receipt result rejects an upgrade even when layouts and capabilities
remain identical. The retired-interface tests now include v6.

Before this repair, fourteen selected cases failed. Afterward, all 61 selected
cases and TypeScript checking pass. The complete API suite passes 207 applicable
cases against both native verifier binaries and PostgreSQL 17.12, matching CI's
`postgres:17-alpine` image. Eight backend-specific parameter cases are
inapplicable; no database-dependent group is omitted. Worker dry-run packaging
and the Node server/verifier bundles pass. No service was deployed. Logs and
the fixture-generation receipt use `target/authoring-v9-registry-*`; the compact
receipt is `target/authoring-v9-registry-consumer-checkpoint.json`.

A separate clean immutable v8 compiler and the current candidate compiled all
ten Finance packages from thirty byte-identical tracked inputs. Each package
was intentionally locked locally, built frozen and checked by its own revision's
independent verifier. All ten ELF pairs and lockfiles are byte-identical; all
twenty artifact bundles verify. The complete Pool ABI report is byte-identical,
as are its nine mode-0-through-8 witness samples (144 bytes each). These are
artifact and encoding comparisons, not execution evidence for those sample
transactions. Results are recorded in
`target/authoring-v9-finance-final-comparison.json` with its artifact and witness
comparison logs. The original Finance working tree remains untouched.

The website README's final v8 reference is updated to the current prefix-source,
inline-output, ordinary-Lock and braced-validity contract. Its local child HEAD
is now `5c7622224f0fa9d0678bc6257c635099bfab3560`; documentation checking passes.
Only these Registry TypeScript/test files and website prose changed after the
rebuilt backend started; compiler/checker Rust and CellScript inputs are
unchanged. The complete backend, CI, remote delivery and cleanup remain pending.

## First complete artifact-checker result and template repair

The rebuilt backend completed all **286 independent artifact integration tests**
with zero failures, followed by the artifact-size, thirteen authoring-parity and
fourteen replacement owners. It then stopped with exit 101 in the bounded byte
context suite: the generated AgoraSeal source included the two fixed-stack
capacity comments added for F10, while `main.cell.in` did not. No executable
source statement differed. The template now contains the same comments; its
complete string-equality assertion is unchanged. All four bounded byte context
owners pass, including exact encoding and real parent/child VM execution, in
`target/authoring-v9-agoraseal-template-owner.log`.

The failed complete invocation remains recorded in
`target/authoring-v9-backend-gate-rebuilt.log`; it is not a passing backend gate.
The remaining integration owners require diagnostic coverage before the final
complete backend and CI invocations. Remote delivery and cleanup remain open.


## Remaining-owner diagnostic and current receipt repair

The diagnostic invocation reached every integration target after the bounded
byte context owner, using `--no-fail-fast` and the existing serial test policy.
It completed with exit 101 and thirty failed tests in seventeen targets. This
was a failure inventory, not a passing integration gate. The exact failures are
retained in `target/authoring-v9-remaining-owner-final-failures.json`.

Two production defects were repaired without changing the accepted positive
source language. Runtime code 88 was already retired with bare named spawn;
quantity destruction now uses code 89, and the original assertion that 88 is
unassigned remains intact. Cache generation v230 prevents reuse of the earlier
runtime result. Numeric builtin-bound parser errors now retain E2111, including
`BoundedCellSet<Token, u64>`. The shared negative fixture is tested by native,
WASM and actual distributed browser APIs. All twenty-four retirement/runtime
owners pass, the malformed-bound owner passes, and the default WASM profile
passes all thirty-six tests.

The CLI owners now expect the adopted explicit-source grammar and the exact
raw-u128 conservation domain. Its Lock builder fixture is copied into its own
temporary directory before module discovery. All 261 CLI tests pass with
TypeScript codec parity required. The rehashed layout-forgery test now changes
resource to shared: both are well-formed Cell-policy declarations, so the
original independent concrete-layout rejection remains the actual oracle.

Current receipts were refreshed only after actual native compilation,
independent bundle validation or VM observation. Four structured fixture owners
pass all twenty-two tests; their source, wire layout and admission fields are
unchanged. The type identity owner changes only the interface/projection hashes
for four cases. Scenario and runtime-view receipt refreshes keep the existing
outcomes, exit codes, numeric measurements and budget ceilings.

Two executable changes were reviewed separately instead of being described as
metadata-only refreshes. The ordinary nested-migration layout keeps its ELF
sizes and changes instruction order only to capture a3 before address setup.
The NFT policy uses identical source bytes under the original and current
compilers; its ELF grows from 8464 to 8472 bytes. A normalized instruction and
control-flow comparison finds only two additional instructions saving/restoring
the absence-scan t6 index and the same early a3 capture. Relocated edges enter
the saved-index loop or its restore block; all other instructions agree. The
new NFT bundle independently verifies, and all ten existing positive/adversarial
scenario records retain their semantic outcomes. The complete fungible,
Order/AMM, temporal and typed-runtime-view owners pass seventeen tests.

The explicit selection-resource observation run passes all eighteen tests while
checking the original ceilings. Exactly thirty-five existing ELF SHA-256 fields
change; transaction bytes, cycles, frame/stack observations, case inventory and
all numeric ceilings remain identical. The transfer-resource fixture is
semantically unchanged. A normal frozen-observation rerun remains required.

The canonical Docker WASM rebuild after these core fixes produces 2485329 raw
bytes and 927338 gzip bytes, SHA-256
`445b9eda9f90d7a1b618cf7df24e466836dc1372734363c76d8fd75b0eeac482`.
All fifty-six actual-byte browser tests and the complete mainnet/testnet website
build pass. The earlier bundle hash is retained above only as checkpoint
history. Complete backend/CI gates, final preservation reconciliation, child
publication and target cleanup remain pending.
