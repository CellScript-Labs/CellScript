# Structured type and specialization identity migration

Status: implementation contract for [#96](https://github.com/CellScript-Labs/CellScript/issues/96).
Final integration of the representation migration is incomplete. This document
makes the internal choices explicit; it does not announce new generic syntax or
an artifact schema.
Owners are the frontend/type/IR maintainers, with independent-checker and
interface consumer review.

## Schema-vector field-span correction — 2026-11-05

The v193 timelock failure was not evidence that fixed-element receipt vectors
had lost their runtime codec by design. The checked Molecule length/index paths
still existed, but frame allocation required a named destination and omitted
the byte-length slot for structural `IrType::Vector` field projections. The
field then emitted an unresolved access and downstream length/index failures.
Use the retained span ABI for that allocation, with the existing layout,
element-width and bounds checks. Wire bytes and business examples retain their
existing contract; dynamic-element vectors keep their separate fail-closed
boundary. Cache v194 excludes the defective allocation.

Edition 2027 ordinary authoring supports multiple actions and selected entries;
the single-entry limit is a bounded automatic-migration-tool restriction.
Nineteen focused regressions pass: the structural-vector suite (14), legacy
dynamic-element rejection (2), fixed-element witness cardinality (1), bounded
iteration (1), and multiple-action entry selection (1). The new receipt tests
execute both editions and compiler profiles in CKB-VM, including the recorded
timelock Cell/witness bytes without conversion. No gate scripts are run under
the maintainer's 2026-11-05 instruction, so the recorded v193 backend failure
is not converted into a passing result. Strict root-package clippy and the
native CLI build also pass. Direct selected-entry assembly no longer contains
the unresolved approver-field, vector-length or index paths. Focused commands
and outcomes are recorded in `target/schema-vector-focused/state.json`.

## Integration caller-guard checkpoint — 2026-11-03

The v192 snapshot passed commit, compiler library 1,139 and allocation/runtime
42. Its backend was deliberately interrupted during the independent checker
matrix after a separate CLI probe accepted a rehashed branch that skipped an
internal PIPE call and entered its success path. A focused Rust regression also
reproduced the acceptance. All 1,978 copied sources, HEADs and the main index were
unchanged at interruption. Dev and documentation closeout did not run; the
interrupted gate remains separate from completed component checks.

The independent result-entry check now protects both tuple receivers and the
first success instruction of internal BIP340/ZK raw-register receivers. Five
focused PIPE tests pass, including the new positive/mutation matrix across both
editions, both compiler profiles and both BIP340 entry forms. Cache v193 excludes
the incomplete caller guard. This changes machine admission, without changing
the source type/identity or wire contracts. Matching commit/backend/dev and
affected browser/Registry acceptance remain required; CI is excluded. Probe
bundles, the original acceptance, the pre-fix Rust failure and the source-equality
receipt remain in `target/ordered-followup-20261102/`.

All 18 v193 consumer commands now pass. Canonical Docker WASM remains
byte-identical to v192 at 1,910,788 raw / 722,665 gzip bytes (SHA-256
`4ddc378a4eb89dfd87240b5072a7b2fc4ebebaf352216f6802f9e0771de23fab`).
The browser compiler passes 86; session/presentation/build/distribution and both
freshly rebuilt Registry verifiers pass. PostgreSQL 18.6 passes 184 with eight
existing conditional skips; cleanup succeeds. Nine immutable identity/structured
targets pass 63 with one explicit export ignore. Eight frozen CLI controls and
the old valid bundle pass; the preserved former bypass rejects. Current strict
compiler/checker lint and WASM host 32 pass.

Final v193 acceptance, 2026-11-04: the frozen snapshot passed the commit gate
(388 s, unstaged/untracked basis) and the complete dev gate; the backend gate
passed compiler library 1,139, allocated runtime 42, all serial integration,
checker/Fiber suites, strict clippy, both scenario backends, the 208-combination
syntax matrix and the zk child chain, then failed at the production stateful
replay — `timelock.cell:approve_emergency_release` reached unresolved field and
length paths (errors 16/24). The initial codec-retirement and edition-migration
explanation is corrected by the v194 field-span diagnosis above.
The two-pass recipe rebind performed on the way and the root-cause analysis are
receipted under `target/ordered-followup-20261104/`
(`v193-final-stateful-analysis.json`, `v193-final-recipe-rebind.json`); the gate
is failed, not waived. Canonical
WASM, browser 86, Registry verifiers, PostgreSQL 18.6 (184/8) and the
certification detached archive were re-verified on the same snapshot. CI is
excluded.

## Source interface roles checkpoint — 2026-11-03

The v190 snapshot passed commit, compiler library 1,138 and allocation/runtime
42. Its backend was deliberately interrupted during the independent checker
matrix after a separate CLI probe reproduced a valid imported-template failure:
`qual::main` could import `Wrapper` from `qual::types`, but the concrete owner's
type checker treated `ScriptHandle<qual::types>` as a source self-designation.
Both `check --frozen` and ELF build failed. Local-template and other-owner
controls passed. The interrupted gate retains source equality across all 1,977
copied files; dev did not run, and documentation closeout did not execute.
Receipts remain in `v190-inventory-isolated-snapshot-*` and
`v190-imported-marker-*` under the ordered-follow-up target directory.

Original source interface roles are now checked before monomorphization, using
the source resolver and lexical generic parameter bindings. The shared AST
walk covers declarations, nested arguments, constructor/call type arguments,
casts, statement bodies and authoring expressions. Generated declarations use
the retained class/module contract, without replaying the original source-only
self restriction in a template's physical owner. Source diagnostics retain the
requesting expression's span. Actual declaration bindings distinguish value
types from modules; `qual::types` can be imported by `module qual`. A bound
value type parameter cannot stand for a module even when a same-named module is
loaded. Ordinary type admission stays with its existing owning checks.

The 17-test import suite passes with three handle classes, both editions and
compiler profiles, default/source-exported builds, all-module diagnostics,
imported and transitive generic templates, two requesters and source-self/value
parameter negatives. Aggregate-return forwarding uses the ordinary executable
profile; the policy-witness-v1 scalar-call boundary remains enforced. The
source-role scope unit passes. The v191 API-path and profile-fixture failures,
the two reproduced lexical-binding failures, and the strict-lint test-module
ordering failure are preserved separately. Cache v192 excludes those
intermediate source-role candidates.

Current-source focused checks pass: import suite 17, role-scope unit one, strict
native lint and WASM host 32. Nine identity/structured suites pass 63 with the
single explicit fixture-export helper ignored; no fixture is regenerated.
All eight frozen CLI check/build controls pass. Canonical Docker WASM is
1,910,788 raw / 722,665 gzip bytes, below 1,500 KiB (SHA-256
`4ddc378a4eb89dfd87240b5072a7b2fc4ebebaf352216f6802f9e0771de23fab`).
Browser compiler 86, session/presentation, local build and distribution checks
pass. The rebuilt Registry verifier, strict checks, normal Node bundles and
PostgreSQL 18.6 pass (184 tests, eight existing conditional skips); the temporary
database is stopped and removed. Matching commit/backend/dev acceptance remains
pending; CI is excluded.

## Imported-interface identity checkpoint — 2026-11-03

The v189 clean-snapshot backend passed library 1,138, allocation/runtime 42,
artifact checker 245, CLI 259 and examples 36, then failed three policy-import
regressions. A retained generic argument such as `ScriptHandle<qual::types>`
was incorrectly resolving its module designation as a nominal value declaration.
The source and full failed gate remain recorded under
`target/ordered-followup-20261102/v189-isolated-snapshot-*`.

V190 gives the existing closed handle class and module designation a separate
internal identity. Source checking validates specialization argument roles,
including phantom/nested uses, before metadata-only output. Ordinary unresolved
nominals remain unequal; neither a matching layout nor a symbol spelling grants
identity. Focused policy imports pass 14 tests, covering the three handle classes,
both editions/profiles, default/source-exported builds and invalid/cross-identity
arguments. Source spelling, specialization symbols, wire versions and executable
handle boundaries retain their existing contracts. Matching integration and
affected consumer acceptance are pending; CI remains excluded.

The subsequent diagnostic tail passes after correcting a stale array-Vec
negative to real owned-snapshot execution (144 VM cases, both editions/profiles,
through 256-byte fixed elements). Dynamic/oversized elements retain source
rejection. The original failed diagnostic run remains preserved. Composed
diagnostic coverage does not substitute for the required matching gates.

## Verified specialization checkpoint — 2026-11-01

The compiler now retains [specialization origins](../src/ast/specialization.rs)
and explicitly marks generated struct, enum and function declarations. The AST
and IR contracts share one immutable origin index. Semantic inference,
dependency closure, source projections and backend storage categories use these
records; the compiler's specialization-label decoder and backend marker checks
have been removed. Metadata verifies the canonical symbol by encoding its
independently supplied argument fields. Missing payloads, wrong declaration
roles/owners and cyclic origins reject before projection or executable lowering.

The [characterization suite](../tests/generic_identities.rs) pins ten records:
source primitive/constant spellings and executable empty/nested generics across
both editions and compiler profiles. It compares wire arguments, declared and
independently checked interfaces, actual ELF hashes and VM behavior. Existing
alias and different-owner regressions remain part of the checkpoint. The vector
fixture was generated before changing provenance consumers and stayed unchanged.
Its ignored export helper requires an explicit output path and does not update
tracked fixtures during ordinary tests.

A further regression binds a type parameter whose name is exactly an existing
generated symbol. The source renderer now carries the declaration's lexical
parameter set, so that parameter shadows the concrete index. It remains a
parameter in the declared interface, generic catalog and callable contract;
both editions and compiler profiles execute successfully in the VM. The first
regression failed with V2419, and the checker was not weakened to admit it.

Validation passed: compiler library 1,111; source/interface/owner/runtime suites
97; independent generic mutations five; tools policy nine; separate native and
tools strict clippy; commit gate. Source and command receipts are retained under
`target/type-identity-20261101/`. These checks do not establish whole-alpha
integration or release acceptance, and CI remains excluded by instruction.
After the cache namespace advanced to v155, all seven focused cache regressions
and all three characterization/provenance/scope tests passed; the explicit
pre-migration export helper stays ignored during ordinary runs.

This checkpoint predates the application-node work below. Its passing checks
do not validate that later source, and it does not close #96.

## Application nodes in progress — 2026-11-01

The parser now retains `Type::Applied` and `TypeArgument::UnsignedConstant(u128)`.
AST-to-IR conversion preserves the application tree rather than converting it
back to a nominal name. Generic inference and substitution traverse the tree,
and value-generic declarations reject constant arguments. Provenance depth and
cycle validation, dependency collection and foreign-seed forwarding descend
through nested applications. Source display is shared by parser, formatter,
docgen, diagnostics and LSP; executable wire and scoped identity rendering
remain explicit output contracts.

This is an unfinished working-tree migration, not a usable release checkpoint.
The first all-target check failed at 29 exhaustive matches; those consumers
have been updated and the next all-target check passed. Built-in family
consumers are not migrated yet. Source checking (including metadata-only builds)
and executable admission reject unresolved application families explicitly,
rather than treating them as nominal scalars. These temporary barriers must be
replaced by each family's structural admission and complete consumer migration;
keeping supported families rejected is not the intended end state.

The initial focused tests exposed the existing distinction between parser empty
tuples and the text reader's `Unit`: retaining the former directly changed four
recorded ELF hashes. Generic arguments now normalize empty tuples structurally
to `Unit` before retaining provenance or selecting a symbol. Type equality and
expression transport preserve the two existing source/IR spellings at their
boundaries. Evaluation occurs before converting a unit-valued result; function
calls and arithmetic failures cannot be discarded by that conversion.

Value admission now consults the source callable's explicit return declaration.
Unit results can be bound and forwarded, including generic locals and match
results. A missing result produces typed diagnostic `E2114` in value positions;
calls with no result remain valid statements. Arguments, tuples, arrays, casts,
ranges and match arms cannot manufacture a unit value from such calls. The
existing no-result statement/binding/return tests remain part of this check.

A further actual-VM regression exposed an emitter gap for known zero-width unit
fields in fixed aggregates. Their width is now explicitly zero, and projection
prepares the containing source before producing the empty value. Unknown
layouts still reject. Both editions and compiler profiles pass successful unit
argument/return/aggregate executions and retain error 20 for division by zero
inside a unit-valued call. Cyclic provenance hidden inside an application also
rejects. The ten original source/wire/interface/ELF records match unchanged;
the fixture was not regenerated. Frontend twelve and formatter six tests pass.
No-payload enum literals also retain their resolved enum type on their SSA
value, so direct matching and ordinary function arguments preserve nominal
identity instead of passing an untyped integer ordinal. Both editions and
compiler profiles pass these cases in the actual VM, with the independent
checker retained.
Cache v156 excludes pre-migration results and their previous admission behavior.
These bounded checks do not establish whole-surface wire/schema stability.
Commands, source manifests and failures are retained under
`target/applied-types-20261101/`.

Admitted families, remaining rendering/diagnostic contracts and final
browser/backend/dev acceptance remain open. CI is still excluded.

## Vector family migration — 2026-11-01

This working-tree stage migrates `Vec<T>` consumers rather than retaining the
temporary application rejection. Source applications keep their type argument;
IR uses `IrType::Vector` with a structural element. Its constructor inference
hole is separate from a concrete element type. Collection construction also
carries an IR type, and local identity/refinement compares retained types rather
than rendered names. Source and typed-record spellings remain output contracts.
Molecule schema generation and named dependency closure traverse the element;
they no longer recover it from a `Vec<...>` nominal string. The compiler source
policy checks this migrated boundary, while the independent checker retains its
versioned wire reader.

Admission requires exactly one type argument and rejects references, hidden
Cell-backed values and a locally bound declaration masquerading as the builtin
constructor. Element layout retains arrays, tuples and record names. A dynamic
vector does not acquire a fixed aggregate width merely because its element has
one. Local stack storage remains bounded to 256 payload bytes, independently of
the owned arena collection model and external borrowed spans.

Ordinary helper parameters consume a canonical Molecule span. A local stack
vector instead has a separate machine-word length before its data. Passing its
raw pointer with a zero or guessed size is incorrect. Calls now reserve a
caller-owned bounded copy, check the element count against its payload capacity,
write the four-byte little-endian item count and copy the active data. This
matches the [Molecule fixed-vector encoding](https://github.com/nervosnetwork/molecule/blob/master/docs/encoding_spec.md).
Argument preparation preserves earlier argument registers and the outgoing
stack region. Incoming canonical spans forward with their original checked
length. This does not establish an ownership ABI for escaping dynamic returns.

Actual-VM regressions cover inference/annotations, stack operations, fixed-byte
and record elements, small array results, tuple projections, empty arguments,
forwarding, repeated arguments, stack-passed arguments and gather-hash helpers.
Wrong argument roles and unowned elements are also tested in metadata and
executable modes. Earlier runs exposed missing small-result storage, invalid
parameter transport, incomplete aggregate availability and a diagnostic
regression; their failures are retained under `target/vector-types-20261101/`.
The expanded run passed 60 existing vector-related library tests and all six
new regressions. Five new VM programs run in both editions and compiler profiles;
the source-rejection matrix covers both metadata and executable modes. The
recorded native inputs stayed unchanged during that run. The separate tools
guard then removed a first-`cfg(test)` scan truncation and added a file-level
regression for later production code. Its eleven policy unit tests passed.
After removing seven redundant borrows, the final-source run passed formatter
six, frontend twelve, generic identities six (plus one intentional export-only
ignore) and vector regressions six. The ten original identity records remain
unchanged. Native and tools all-target strict clippy and the commit gate passed.
Cache v158 separates the updated vector transport from earlier candidates.

The full compiler library run is explicitly failing: **1,066 passed, 48 failed**.
The failure inventory is retained with the source manifest and logs under
`target/vector-types-20261101/`. It includes unadmitted Commitment/Opening,
ExactCellSelection, bounded and nested families; InputView source substitution
and equivalence consumers; nested IR validation, LSP/navigation and diagnostics.
Not all failures are admission barriers: an effect-source regression and
same-spelling-but-different-node comparisons require actual consumer migration.
Do not remove the barriers or weaken those tests to make this checkpoint green.

Other builtin applications and resolved nominal consumers remain unfinished.
The preceding specialization/unit receipts do not validate this stage, and
neither a vector-only checkpoint nor a source-policy check closes #96 or A2-14.

## Transaction context views — 2026-11-01

Source `InputView<T>`, `OutputView<T>` and `CellDataView<T>` retain their typed
schema argument. Lowering maps them and the four atomic context view names to
`IrType::Context`, whose closed roles distinguish input, output, Cell data,
CellDep, HeaderDep, WitnessArgs and Script. Cell schemas must still resolve to a
resource, shared type or receipt. A declaration shadowing a builtin view cannot
manufacture that role, including inferred constructors and Script projections.

Field checking, selected reads, runtime argument preparation, Cell-data access,
effect-source substitution and metadata schema-pointer discovery consume these
nodes. Source-kind alternatives come from context roles and resolved runtime
calls. A nominal `InputView<Token>` spelling alone supplies no source evidence.
Internal lock/type Script references retain their separate opaque provenance.
Readonly context handles keep the existing two-word helper transport; they do
not gain Cell lifecycle ownership, packed-record layout or an escaping ABI.
Source/wire strings are output boundaries, and independent checker wire readers
remain separate. The source-policy guard covers migrated checking/lowering/
effects/codegen consumers without truncating files at an early test item.

The first legal-source run passed **1,084 library tests and failed 31**. It
confirms the selection-container guard repair and the forged nominal-view
effect-source rejection. Two new VM fixtures failed because they omitted the
group input; the Cell-data fixture also placed its bytes in Script args rather
than Cell data. The corrected fixtures now explicitly bind the group input and
populate both Cells. Their four regressions passed, including both editions and
compiler profiles, changed-data rejection and inferred ScriptView shadowing.
That candidate's frontend/formatter/identity/selection/checked-interface/Vec
targets also passed; the full library still had the same 31 failures. Native and
tools strict clippy and thirteen source-policy unit tests passed on that source.

Comparing three identical legal programs against the retained pre-tree compiler
then found another migration defect: their ELF bytes and transaction-view
metadata matched, but helper parameter metadata omitted schema pointer/length
flags for structured views. This changed builder/interface and typed-semantic
hashes despite unchanged machine ABI. Vectors shared the omission. Metadata and
codegen now share the structural transport predicate, including referenced
spans. Three immutable old-binary byte/interface/typed-semantic vectors and
parameter-flag assertions cover the correction. The v160 source passed all
three old-byte/interface/typed-semantic vectors, and actual CLI comparisons also
match helper parameter and transaction-view metadata. Its seven integration
targets passed **41 tests**, with one existing export-only ignore. The full
compiler library now passes **1,085 tests and fails 30**: fixing vector span
metadata also restores the entry-witness encoding regression. Native and tools
all-target strict clippy and the commit gate passed. Cache v160 excludes the earlier metadata.
Failed runs, source manifests and the complete remaining-failure inventory live under
`target/context-views-20261101/`. The original generic identity fixture is not
regenerated.

This stage does not close #96. Exact selections, nested/bounded families,
Commitment/Opening and general resolved nominal identity still require consumer
migration. The preceding vector receipt is historical; neither strict clippy
nor a passing narrow suite establishes whole-library or final gate acceptance.

## Nested source applications — 2026-11-01

NestedValues, NestedBytesView and NestedRecordView now consume type/constant
nodes before producing the existing resolved nested IR. An explicit source
element category distinguishes bytes from a nominal record; compilation-scoped
description caches use AST keys. Shape expansion and field/index projections
return types directly. Record alias comparison resolves its defining owner and
layout; it does not compare widths. External builder/editor and independent
checker wire readers keep their existing grammar and rendering contracts.

The initial full-library candidate restored nine failures from the context
checkpoint (1,094 passed, 21 failed). Its two immutable old-compiler byte and
interface vectors and all nine Cell-data regressions passed. Existing generic
shape and new alias-call cases exposed two remaining consumer gaps. The parser
now validates limits before expansion and admits element semantics afterward;
the checker compares nested alias calls using its independently checked nominal
catalog. Missing catalogs, owner rebinding and changed bounds must reject.
Resolver localization now traverses application children. Typed and machine
argument checks use the same independently verified nominal comparison while
retaining separate memory provenance. Final-source focused validation passes
88 integration tests (one existing export-only ignore), all 33 nested and nine
Cell-data cases included, plus four bounded-call tests and checker library 90.
The full compiler library passes 1,095 tests and fails 20, restoring ten previous
failures without new library failures. Native/checker/tools strict clippy and
fourteen source-policy unit tests pass. Both historical byte/interface vectors
and actual CLI metadata comparisons match. New cases cover full/derived views
through imported helpers, annotation aliases, same-width different owners and
rebound catalog/bound mutations. Source manifests and failed/successful runs live
under `target/nested-source-types-20261101/`.

This is not completion of #96. Default compilation must retain enough
independently checked alias identity; requiring the optional declaration flag
forever is not the intended end state. Complete open-interface projection still
rejects nested view abilities at its existing universal-contract boundary; no
new open view ABI is claimed. Default alias evidence, other admitted
families, general nominal representation and remaining diagnostics still need
work. Source/checker sharing stops at closed syntax/contracts; resolving a
compiler reference is never machine or source-identity proof for the checker.

## Bounded source collections — 2026-11-01

`BoundedCellSet<T, N>` and `BoundedList<T, N>` now read retained type/unsigned
constant arguments and lower to an explicit bounded IR description. Its element
is an IR type, not a rendered application. Bounded operation records also retain
both types. Source validation keeps the cardinality range, explicit input versus
witness roles, linear Resource admission and fixed non-Cell plan restrictions.
References cannot masquerade as an owned Cell-set element.

Runtime selection still requires the original named Resource/Plan layouts and
finite consume/create shapes; dynamic Cell data or incomplete output templates
retain abort 24 and production rejection. Fixed layout, nested projection,
metadata dependencies, ABI classification and semantic-foundation roles/provenance
visit the structure. Group authority comes from an admitted input parameter,
not `BoundedCellSet<` text. External metadata and the independent checker keep
their existing wire contracts. Encoded legacy IR names reject recursively.

Before changing the consumers, a retained pre-tree release binary characterized
four existing source programs: executable consumption/output creation and both
fail-closed counterparts. Their source, ELF, interface and typed-semantic hashes
are pinned in the [bounded characterization](../tests/fixtures/bounded_source_identity_characterization.json).
The [regression suite](../tests/structured_bounded_types.rs) compares those
identities and covers both editions/profiles, argument roles, bounds and forged
legacy IR. Candidate checks are retained under `target/bounded-source-types-20261101/`;
the final library run passes 1,111 tests and fails four, restoring sixteen prior
failures with no new library failures. Eight integration targets pass 75 tests
(one existing export-only ignore), and 35 bounded checker mutation/VM tests pass.
All four pre-tree ELF/interface/typed-semantic vectors and actual CLI
byte/metadata comparisons match. The first run recovered nine library failures but
exposed a separate enum classifier that treated any linear argument as a linear
enum payload; it now recursively checks enum payloads. Generic record applications
in owned collection declarations defer width until specialization and resolved
admission, without inventing a width. The same run exposed nondeterministic
external function body order: hash-set discovery is sorted before emitting bodies.
Localization chooses equivalent import aliases deterministically. Alias tests
clear only their temporary package's build cache and compare fresh ELF, interface
and typed-semantic hashes as well as renamed declarations. The second run exposed
over-deferred dynamic Vec elements; parser-time rejection is retained while
nominal generic records still require later resolved-width validation.
Cache v162 excludes the previous family admission and import-layout results.
This stage does not complete the other families, nominal identity, diagnostics,
#96 or A2-14.

## Committed source values — 2026-11-01

Unary Commitment/Opening arguments stay in AST nodes and lower to distinct IR
variants. Source checking uses the retained value type for fixed-layout
admission, opaque witness use, one-shot ownership and open/commit inference.
Widths, fixed-byte storage, opening materialization, metadata, proof plans and
helper ABI classification use those variants. The compiler's now-unused inline
IR text parser is removed. The existing 32-byte digest pointer/length ABI keeps
pointer-only spilling; the fixed digest extent is supplied by the validated
entry contract, while opening spans retain their actual lengths and size checks.

The packed domain remains `CellScriptPackedHashV0\0 + type spelling + \0 + width`.
Rendering that retained type for a preimage is an explicit wire contract. This
stage neither replaces the domain with structural width nor silently qualifies
old preimages. General owner/alias identity remains separate work under #96.
Nested wrappers, Cell values, references and dynamic committed values retain
their existing rejection; accepting a same-width different type is not a fix.

A retained pre-tree native compiler characterized three successful source
programs (record opening, scalar opening and commitment construction) before
migration. Its array-opening probe failed independently at the machine boundary:
the old string width query could not materialize its admitted array. Structured
array/tuple widths now reach hashing and result storage, with real VM positive
and tampering cases across both editions/compiler profiles. The historical
failure is retained separately, rather than promoted to a permanent boundary.
The [successful identity vectors](../tests/fixtures/committed_source_identity_characterization.json)
remain immutable. The first executable candidate passed library 1,115 but
exposed an extra unused digest-length spill in ELF/resource characterization;
the typed ABI now preserves the original pointer spill before repeating checks.

New [machine mutations](../tests/artifact_checker/committed_aggregates.rs)
exposed an existing admission gap: after authenticating the original opening,
a different pointer could supply its materialized value. The original checker
accepted that rebound mutation. The independent
[materialization recognizer](../crates/cellscript-artifact-checker/src/checker/commitment_materialization.rs)
requires the actual opening pointer, exact extent, safe result frame region,
matching stored result pointer or finite scalar byte reconstruction, and an
exclusive authenticated incoming branch. Unknown forms reject; the compiler's
reference alone is not proof. The existing memcpy runtime remains a distinct
callee contract. Final combined validation is in progress under
`target/commitment-source-types-20261101/`; cache v163 separates the change.

The two remaining reserved/unresolved application diagnostics are also restored:
ExactCellSelection declarations cannot supply invocation-local handles, and an
unknown application reports E2111 without fabricating a local template. This
does not complete general nominal identity, default-build alias evidence,
diagnostic classification, #96 or A2-14.

## Nominal owners and remaining runtime descriptions — 2026-11-02

This working-tree stage shares `DeclarationIdentity` between resolved source
declarations, retained specialization templates and IR origin records. Ordinary
and nested aliases compare defining owners; equal-width different declarations
remain distinct. Native default builds retain the bounded declaration catalogs
required by aliases or interface handles, while optional source export still
controls constant records. Required missing/over-budget evidence fails closed.

Interface handles retain their class and module designation; WitnessBytesView
retains its owner and byte bound. Nominal constructor expressions, read_ref,
unique Cell operations and aggregate targets retain type arguments until
specialization. The old reconstruct-and-parse path in source checking is gone,
including nominal base-name stripping for field/layout lookup. Exact-selection
results and role projections now retain an invocation-local description in
AST type inference and IR. Explicit source applications remain rejected. The
independent versioned reader retains its grammar and semantic checks.

Executable type admission now traverses all instruction destinations/operands,
return/branch values and external callable signatures. Direct codegen uses the
same structural guard. A malformed retained catalog with extra role names
cannot hide its extra fields through encode/parse normalization. The source
policy forbids nominal family readers in migrated IR/type/backend consumers.

The IR renderer owns distinct source, typed-wire and allocated-member formats.
Typed-wire uses `unit`, `hash` and `address` for ordinary leaves, preserving the
existing source-spelling domain of family arguments. Allocated-member records
lowercase only top-level Hash/Address and retain source spelling in children.
The source-declaration renderer still binds lexical type parameters before
consulting specialization origins. Compact identity and symbol encoding retain
their explicit existing domains; no renderer supplies nominal equality.

Formatter seven, frontend twelve, immutable generic identity six (one explicit
export-only ignore) and application units four passed before selection migration.
The earlier 1,117-test library result is also historical. The new final-source
library/VM run now passes 1,120 library plus 41 integration tests, with the one
explicit identity-export ignore. Independent checker library 90, two named-role
machine mutation tests, strict native clippy and the commit gate pass. The native
source manifest remained unchanged. Browser/backend/dev gates remain pending;
the canonical browser command stopped before building because Docker was not
running. Cache v170 separates the current candidate. Receipts live under
`target/ordered-followup-20261102/`; these checks do not close #96 or A2-14.

The subsequent v172 candidate also binds imported/local declaration identities
before body lowering. Array/Vec literal compatibility and independent typed
collection/aggregate operations compare those owners, retaining missing and
wrong-owner rejection. Local declared-record array extensions use retained
element snapshots; a width alone cannot manufacture contiguous storage. The
primitive copy path is unchanged. Expanded native VM and rebound tests pass;
the final combined-source library, browser and backend/dev acceptance remains
pending under the ordered follow-up receipts. The v170 checkpoint above does
not validate this later candidate.

## Later nominal and packed-domain checkpoint — 2026-11-02

Cache v176 adds compiler-owned semantic keys for concrete nominal types:
template declaration identity plus recursively resolved arguments, including
phantom arguments. Declaration provenance remains a separate index, and neither
key is decoded from an emitted name. Ordinary aliases can therefore remain
equivalent inside nested specializations while different argument/template
owners stay distinct. The independent ability graph requires all concrete
contracts for one qualified instance to establish a requested ability.
Ordinary records/enums also forward their owner, fields and abilities to an
external template owner; ordinary private declarations are not promoted to
public declarations for this transport. Imported qualified records retain
their IR field layouts.

Packed wrappers add their exact V0 source spelling to ordinary owner equality.
Import localization preserves this domain and checks its original bindings.
The checker normalizes materialized result types without normalizing preimage
domains. Hash/Address and aggregates with unit fields now open/recommit under
their source domains. Ordinary Commitment results use caller-owned digest
storage, with independent copy/source/extent checks. No Opening return ABI is
introduced. The successful committed and generic characterization fixtures were not regenerated.

Focused v175 VM checks passed before the qualified/foreign-argument repairs;
their new hostile caller and exact-type domain mutations pass on v176. Strict
native clippy and independent checker library 91 pass. Complete compiler and
combined backend/dev/browser acceptance are pending; #96 and A2-14 remain open.

## Existing behavior to preserve

### Linked scope correction and decision — 2026-11-02

The v176 source checkpoint passed compiler library 1,120, four integration
targets 29 (one explicit export ignore), independent checker library 91 and
strict native clippy. Its source receipt covers 1,033 files. Those results do
not validate the following v180 correction.

A multiple-requester probe reproduced a late V2419: two modules used `Arg` for
different equal-width records while instantiating one external template. Project
expansion now records declaration meanings in reachable scopes and qualifies
conflicting arguments before assigning specialization symbols. Unambiguous
spellings retain their existing forwarded bindings. This distinction preserves
V0 packed domains, including a helper that commits a generic record member.
External IR linking namespaces colliding layouts, enums, operand/parameter
types and Cell schema records from retained declaration identities. It does not
infer an owner or arguments from a symbol. A required packed-domain rename
rejects with E2111 instead of silently changing its preimage.

Metadata storage propagation now uses the retained nominal keys. Missing or
ambiguous origin evidence cannot disable required native catalog construction.
Imported function signatures resolve each defining declaration; a caller's
unrelated same-name type cannot capture a dependency signature. Resolved
signature layouts, enum payloads and referenced fields close before lowering.
Already qualified spellings and packed wrappers keep their source domains.
The independent ABI normalizer preserves qualified module/declaration case:
`Hash::Value` and `hash::Value`, or `owner::Hash` and `owner::hash`, are distinct.
Its owner-comparison regression first validates the complete declaration catalog.

Decision: retain the v11 record schemas and existing hash constructions.
Qualified identifiers and owner catalogs are already part of those contracts.
Correcting an ambiguous scope can change generated symbols and identity values;
this is an owner-resolution correction, not a renderer-only change or a new
hash domain. Cache v180 excludes the earlier v177 argument, v178 linkage and
v179 signature candidates. Existing successful characterization fixtures are
not regenerated. Missing/rebound catalogs, mismatched argument/template owners,
type-domain substitutions and wrong machine copy extents remain negative tests.
Full current-source acceptance is pending. A cross-module builtin Option probe
also remains under investigation; this checkpoint does not claim that boundary
or #96/A2-14 is complete.

### Closed prelude and payload storage checkpoint — 2026-11-02

The cross-module Option investigation found a real scope mismatch: a consumer's
annotated `Option<u64>` and a provider's generated instance had different nominal
keys. Source-only correction was insufficient: an inferred result without any
local Option annotation still failed at the independent ELF boundary (V2419).

Cache v183 retains a compiler-only `PreludeOption(argument)` semantic key from
the closed retained origin and concrete enum shape. Arguments still resolve in
their declaring scope; distinct user nominal owners do not become equivalent.
Public signatures can introduce the prelude implementation type without an
explicit generated-symbol import. This exception does not change ordinary
generic or argument-declaration visibility. Imported signature closure retains
the qualified layout, generic contract and semantic key. The checker's bounded
alias projection replaces complete qualified tokens; an unknown owner cannot
inherit an unqualified alias by sharing its final component.

The independent checker admits prelude identity only for the reserved, closed
one-parameter declaration with all six fixed-value abilities and the ordered
`None` / `Some(T)` variants. Ordinary declarations cannot claim the reserved
name. Its comparison key is independent of the emitting module; serialized
instantiation identity and source-module provenance keep their existing format.
This adds no new wire schema, hash construction or packed-value domain rule.

The nominal-argument regression also exposed a separate metadata availability
mistake: an eight-byte record projected from an enum was classified as a scalar
solely because of its width. Metadata, enum emission and frame allocation now
consume the retained enum field's storage contract. The existing emitter already
copied such aggregate payloads into local storage; no scalar representation is
inferred from a record's width.

Focused positives cover explicit and inferred imported results, forwarding and
subsequent calls, narrow payloads and same-owner argument aliases in both
editions/profiles and both source-export modes, with actual VM execution.
Negatives retain distinct-owner source rejection and independently rebound
call-operand substitution. A coherent `Some` to `Present` declaration/layout/
operation mutation is rejected after layered identities are recomputed. Raw
logs are under `target/ordered-followup-20261102/`; full v183 acceptance remains
pending. Earlier failed fixtures/probes and the zero-selected initial owner
test invocation are retained and are not passing evidence.

The v183 frozen candidate passed compiler library 1,121, ten focused family/
return targets (70 tests, one explicit export-only ignore), checker library 93
and native strict clippy. It did **not** pass the nominal-call mutation suite:
the suite's valid imported generic layout used an equivalent argument alias
that differed from the retained argument's spelling. The diagnostic is retained
in `prelude-final-nominal-mutations.log` and the more specific projection probe.
These passes do not validate the following correction.

Cache v185 keeps a valid existing local source binding before selecting another
alias. Imported generic signatures can reuse a requesting scope's available
specialization only when its semantic template owner and resolved arguments
agree. A source probe reproduced the rejected `provider::Box<Arg>` result versus
`Imported<Alias>` annotation before this correction. The new real-VM regression
covers construction, another helper call, forwarding, both editions/profiles and
source-export modes; a distinct argument owner still rejects before ELF.

Independent generic shape projection now substitutes binders using artifact
argument identities while qualifying free declaration names in the template's
own scope. Packed domains stay exact before this comparison. This is a bounded
projection of catalog data; full catalog and layout validation remain mandatory
before operation admission. Its unit test also binds a same-name free declaration
in two scopes to different owners. Both the five-test nominal mutation suite
and focused projection tests pass; full current-source acceptance is pending.

The v185 library (1,121) and commit gate passed, but the family run stopped at
the immutable nested-generic fixture. Retained arguments also need their emitting
module's source bindings, rather than the artifact module's bindings. The same
rule now governs nested layout identity and independent argument ability
constraints. A constraint-phase regression proves that the artifact's unrelated
copyable `Item` cannot grant `copy` to a provider's same-name non-copyable `Item`.

The first v186 normalization admitted the fixture but changed four checked-module
projection hashes; ELF hashes, declared interface hashes and instance/local type
records were unchanged. The observation was exported only to an ignored target
file for comparison; the tracked fixture was not rewritten. Cache v187 separates
the internal semantic-comparison contract from public module-projection-v3
output in one projection traversal. The immutable fixture now passes again,
along with all thirteen nominal target tests. The v185 passes do not validate
v187; fresh combined acceptance is required.

The v187 frozen native checks now pass compiler library 1,121, ten focused
family/return targets 71 (one explicit export-only ignore), nominal/committed
mutation targets 5+4, independent checker library 95, native strict clippy and
WASM host 32. The canonical pinned-container WASM build also passes: raw
1,902,424 bytes, gzip 719,562 bytes, SHA-256
`03dc9c343b214eba6356f00deedde9453f150956a469f189e7f43e298ac8f53c`.
Browser compiler tests pass 86/86; session/presentation tests, local Astro build
and distribution checks pass. No tracked characterization fixture was changed.
The temporary isolated rootless Docker engine was stopped after use. These are
focused and browser receipts, not backend/dev or CI/release gate passes.

At the recorded pre-migration baseline the parser embedded applications such as
`Vec<u64>` and numeric arguments inside `Type::Named`. Specialization consumers
reconstructed arguments from `__mono__` labels; the provenance checkpoint removed
those decoders. Remaining Cell/selection family consumers still recover
arguments from rendered names and must migrate. Alias fixes or admission guards
do not by themselves remove those dependencies.

Four distinct outputs must retain their contracts during migration:

| Output | Existing examples and responsibility |
| --- | --- |
| Source/diagnostic spelling | `Hash`, `Address`, `()`, spaces after tuple/generic commas; aliases can remain visible in diagnostics |
| Typed executable wire | `hash`, `address`, `unit`; qualified nominal owners come from the admitted catalog |
| Specialization symbol | Existing deterministic `__mono__` encoding, generated from retained arguments |
| Interface identity | Existing declared interface preserves declaration spelling; checked interface uses resolved nominal identity |

Changing a source alias must retain the checked interface and executable bytes.
The declared interface's spelling-sensitive identity remains an explicit
existing contract, rather than silently changing with the representation.
Different declarations with identical layouts must remain distinct. Stack
vectors, allocation-backed owners and borrowed views retain separate ownership,
lifetime and ABI admission rules.

## Internal representation and phase ownership

Source AST applications will be `Applied { constructor, arguments }`.
`GenericArgument` distinguishes `Type(Type)` and `UnsignedConstant(u128)`;
a numeric token is never a nominal declaration. The constant width follows the
existing lexer. Each family still enforces its existing value range. This does
not introduce constant parameters, constant expressions, inference of new
parameter kinds or acceptance of constants by value-generic declarations.
Plain source paths retain their source spelling until resolution.

Resolution supplies a declaration identity consisting of the defining module
identity and declared name. The module identity comes from the existing resolved
package/module catalog, not a caller-provided alias or layout hash. Executable
IR carries this identity, structured arguments and the admitted family variant.
Diagnostic spelling and machine labels are separate presentation data.

Monomorphization retains a specialization origin for each generated declaration:
its kind, defining template identity and ordered structured arguments. Imported
requests, external instantiation seeds and scoped IR merges must transport this
origin. Consumers may look up a generated label in that table, but must not
decode a label to discover whether it is a specialization or what its arguments
mean. Private visibility, transitive type dependencies, source projection,
interface emission and generic contract emission all use retained origins.
Missing or conflicting origins reject at the owning phase.

An encoder may keep the current symbol bytes by rendering those structured
arguments under the characterized symbol contract and encoding them once.
The encoder is an output boundary, not a source of semantic facts. Removing a
semantic decoder while retaining a generated-name prefix check would leave the
migration incomplete.

## Render and reader boundaries

Renderers select an explicit contract. Source spelling, typed wire spelling,
compact identity spelling and machine-symbol encoding must not be collapsed
into one implicit `Display` implementation. Shared traversal can remove duplicate
algorithms while keeping these choices visible. Hash/unit spelling is a format
choice, not a case-insensitive nominal comparison.

Source parsing and versioned metadata reading are the allowed text-to-structure
boundaries. A closed shared syntax description/parser may be owned by the
checker crate, as with WitnessBytesView. It may identify argument kinds and
syntax only. The checker independently resolves catalogs, reconstructs type and
effect constraints and validates machine evidence. Producer resolution results
must not become checker admission flags.

After boundary parsing, executable family consumers switch on structured
variants. They must not rebuild `CellDataView<...>`, selection names, witness
views or generic applications and parse them again. Existing structured nested
views remain structured; no family is converted back to a generic string as an
intermediate migration step.

## Wire, identity and cache decision

The first migration target preserves the current versioned public strings,
canonical identities and generated machine bytes. Internal AST/IR changes alone
do not justify changing the metadata envelope, package lock domain or interface
hash domain. Metadata renderers are the only writers of the retained text form;
readers parse it once and then perform their own admission.

This compatibility target is subject to characterization, not an assumption of
success. If a required identity correction changes a public hash or wire record,
that correction needs an explicit schema/domain decision, stale/mixed-record
rejection tests, regenerated current products and a cache generation change.
It must not be hidden as a harmless renderer refactor. Any artifact-affecting
semantic correction invalidates old persistent results even if its wire shape
stays the same.

## Checkpoints and evidence

1. Record source/typed-wire/symbol/interface vectors before switching consumers.
   Cover both editions, primitive case and unit spelling, arrays, tuples, nested
   applications, constant family arguments, aliases and imported equal-layout
   owners. Record actual ELF/checker/VM behavior and the relevant resource limits.
2. Introduce AST applications and constant arguments under the parser owner.
   Preserve vectors while migrating traversal, substitution and renderer entry
   points. Numeric `Named` construction and parser flattening become source-policy
   violations after this checkpoint, not permitted compatibility fallbacks.
3. Retain specialization origins and remove every semantic decoder/prefix
   consumer, including external request and source projection paths. Preserve
   symbol bytes through the encoder; require missing/conflicting-origin negatives.
4. Migrate every admitted family through type checking, resolved IR, metadata,
   codegen and checker reading together. Require independent stale catalog,
   missing evidence, same-width different-owner and malformed argument mutations.
   Extend native source policy to prohibit reconstruct-and-reparse in migrated
   consumers.
5. Bind the final combined source to focused native/checker/VM evidence and the
   pinned canonical browser build and size measurement. Retain failed receipts.
   Matching backend/dev acceptance remains required for readiness claims. The
   maintainer's instruction currently excludes CI; do not convert that exclusion
   into a CI pass or release acceptance.

The closed typed call target, the recursive E2105 guard and the NovaSeal package
are separate work. None establishes completion of these checkpoints.
