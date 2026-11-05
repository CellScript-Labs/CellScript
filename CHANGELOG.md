# Changelog

## Unreleased

Run the complete compiler suite once in CI/backend, with separate syntax and
stateful acceptance steps. Dev quick checks select their owning targets and
reject zero-test results. Remove duplicate cost/ZK/type-check invocations and
historical release-wording requirements.

Register shared helper assertions through explicit test owners, retaining
separate coverage where dependency closures differ. Remove duplicate consumer
test cases and repeated identical browser compilations. Consolidate byte-access
guidance and remove superseded execution journals.

## 0.33.0-alpha.2 - 2026-11-05

Use Edition 2027 across native compilation, packages, language services, browser
metadata and independent artifact admission. The shared authoring grammar keeps
ordinary actions, locks, lifecycle operations and structured successor rules.
Current metadata, lowering, typed semantics, semantic foundation and public
interfaces use their explicit versioned contracts. Generated builders and
deployment records use edition-neutral schema identities. See the
[source contract](docs/CELLSCRIPT_EDITION_POLICY.md).

The refreshed tag identifies `f7a7a0f6`. The compiler implementation at
`a3d28009` passed commit, dev and ci; the refreshed publication includes later
documentation, recipe-identity and ledger changes. The maintainer explicitly
waived remaining gates and CI for this publication. Its attached receipt records
the interrupted backend run and unexecuted final stateful acceptance.

Initial alpha.2 publication, under the maintainer's 2026-11-05 instruction
after the v193 acceptance closeout and the v194 schema-vector correction.
APIs may change during the alpha series. The commit and dev gates, every
backend step before the node-stateful replay, and the canonical
WASM/browser/Registry/PostgreSQL/certification consumer evidence passed on
the v193 snapshot; the stateful replay failure recorded there is superseded
by the v194 field-span repair, and a fresh full stateful pass is not claimed
by this publication. See the
[alpha release notes](docs/releases/CELLSCRIPT_0_33_ALPHA_2_RELEASE_NOTES.md)
and the [alpha.2 backlog](docs/releases/CELLSCRIPT_0_33_ALPHA_2_TODO.md).

Admit narrow integer literals in `Vec::extend_from_slice` array sources by
inferring and lowering them with the receiver's element type, matching the
existing push/insert/set admission; literals outside the element width still
reject at type checking, and non-literal sources keep their own declared type.

Check compatible-open interface roles in their original source scope before
generic expansion, including constructor/call arguments and generic parameter
shadowing. Materialized arguments retain their separate module-binding check,
so imported templates and transitive instantiations can name their defining
module's interface. Resolve value bindings directly; a module path prefix does
not make a submodule a value type. Explicit source self-designations, value
parameters and missing modules still reject. Cache v192 excludes the intermediate
source-role candidates. Focused type/identity and browser/Registry acceptance
pass; the final combined acceptance is recorded with the v193 entry above.

Admit the exact ZK parent in the finite SPAWN caller inventory, alongside
BIP340. Each retained typed call binds its own failure code (76 for ZK, 51 for
BIP340); machine calls still require an exact count, ordered correspondence and
an independent status guard. Mixed caller orders and guard bypasses have focused
regressions. Cache v189 excludes the incomplete v188 SPAWN admission candidate; its final
combined acceptance is recorded with the v193 entry above.

Keep runtime caller dispatch and fixed-result evidence on retained helper identities.
The independent checker now owns separate record, ELF, typed-semantic, policy,
Script-hash and committed-state modules; its admission order is unchanged.
Decoded ABI checks cover return-register/status obligations and the bounded
spawn transport, including incoming guards and hostile machine mutations.

Share one captured dependency observation across LSP views and diagnostics;
reject conflicting canonical aliases, stale analysis and stale publication.
Numeric/input/generated-assembly diagnostic consumers retain typed identities.
CLI and iCKB suites keep their test identities while moving to behavior files;
ordinary tests use private example copies and cannot update iCKB baselines.
NovaSeal certification archives carry indexed unit resources without assuming a
parent checkout. Native tools select the native CLI library profile, and dev/CI
recipes include workspace feature-unification checking. Cache v188 excludes the
intermediate caller/diagnostic/source-observation candidates. Combined commit,
dev, pre-stateful backend and consumer acceptance now pass on the final v193
snapshot (see the v193 entry above); the backend stateful replay remains
blocked as recorded there. CI is excluded for this work.

Keep compiler-owned `Option<T>` identity independent of the module emitting its
instance, with resolved nominal arguments and an independently checked closed
prelude declaration. Retain qualified layout/instantiation closure for inferred
imported results. Enum payload availability follows its retained scalar/handle
or aggregate storage contract, including records of eight bytes or less. The
existing serialized instance provenance and hash constructions remain intact;
cache v187 excludes the intermediate prelude/storage/projection candidates.
Imported generic results reuse an available specialization only when template
owner and resolved arguments agree. Independent declaration substitution keeps
generic binders, retained arguments and free nominal names in their defining
scopes, including ability constraints. Internal semantic comparison is separate
from the existing module-projection-v3 output contract. Full combined acceptance
is pending.

Resolve conflicting argument names before external specialization and namespace
colliding linked layouts from retained owners. Metadata pointer propagation also
uses those nominal keys. Imported callable signatures cannot capture unrelated
same-name caller types; their resolved layout closure reaches lowering. Require
native owner evidence for ambiguous bindings. Preserve qualified identifier
case in independent type comparison and retain unambiguous V0 packed spelling.
Cache v180 excludes the intermediate scope/link/signature candidates. These
corrections retain the v11 wire grammar and hash constructions; ambiguous scope
identity values can change. Combined acceptance remains open; see the current
prelude checkpoint, structured-identity RFC decision and raw receipts.

Compare concrete nominal specializations by their template declaration owner
and retained resolved arguments, including phantom arguments. Keep source
provenance and emitted names separate. Independent ability checks require every
concrete contract for an equivalent qualified instance. Forward ordinary record
arguments, fields and declaration owners to external template owners, without
marking ordinary private declarations public. Imported qualified record labels
retain their field layouts through lowering.

Keep V0 commitment spelling and owner together: source alias equality cannot
retag a packed domain, and imports must preserve its original type bindings.
The checker distinguishes source-domain spelling from normalized result wire
types, including Hash, Address and aggregates with unit fields. Commitment
construction uses its declared source domain; ordinary digest results copy
into caller-owned 32-byte buffers with independently checked extent/source
contracts. Cache v176 excludes the earlier specialization/packed-domain and
foreign-argument stages. Final combined gates remain pending.

Retain nominal constructor, read-reference, unique Cell and aggregate-target
arguments as syntax nodes until specialization. Unknown applications cannot
hide in executable instruction operands, local values, returns or external
signatures. Interface handles retain a closed class and module designation;
bounded witness views retain their owner and byte extent. Explicit lock witness
annotations parse without making `lock` an ordinary type name. Default native
builds retain the declaration catalogs required by interface handles. Ordinary
aliases compare defining owners, including fixed-result length propagation;
equal-width different owners remain distinct. Source, typed-wire and allocated
member output formats share an explicit IR traversal while identity/symbol
encoders retain their existing domains. Exact selection results/projections
retain catalog descriptions through source inference and IR; explicit source
declarations remain rejected, and independent wire admission stays separate.
Cache v170 also excludes incomplete projection emission, markers and feature
classification from the v168/v169 stages; v168
separated pre-migration selection strings, v167's runtime designation
stage and v166's earlier nominal-owner-only checkpoint. Final
combined browser/backend/dev acceptance remains open.

Retain independently checked nominal declaration/owner catalogs in default
native builds when multiple lowered labels name one declaration. Valid imported
nested-record aliases no longer require opting into source-contract export to
pass ELF admission. Missing/budget-exhausted required catalogs reject during
metadata construction; the independent checker retains missing/rebound-owner
rejection. Optional source-contract export still controls constant-value records.
Cache v165 excludes earlier incomplete alias evidence.

Retain Commitment/Opening arguments through source validation, IR, fixed layout,
packed hashing, metadata and ABI selection. Keep fixed 32-byte digests and
length-delimited, explicit one-shot opening witnesses; materialize fixed arrays
and tuples using their actual retained layout. Remove the inline IR string-type
fallback. Preserve the existing packed-hash spelling domain and digest parameter
transport. Independently verify the post-authentication copy or byte assembly,
including source, width, result pointer, frame range and incoming control flow;
rebound materialization mutations reject with V2420. Reserved source selection
applications retain their local-construction diagnostic, while unresolved
applications identify the missing template/admitted family with E2111. Cache
v163 excludes earlier wrapper admission and materialization results. The wider
nominal identity and diagnostic migration remains open.

Retain bounded Cell-set/list elements and cardinalities through source checking,
IR, runtime-profile selection, metadata and semantic-foundation evidence. Keep
linear ownership, explicit source roles and the existing finite runtime profiles;
unavailable shapes still abort with 24 and reject under production policy.
Defer generic collection-record widths until specialization and resolved layout
validation. Traverse enum payloads without classifying every linear type argument
as an enum. Stabilize imported callable discovery before emitting bodies so hash
iteration cannot change ELF identity. Cache v162 excludes earlier admission and
nondeterministic import-layout results. The complete type migration remains open.

Retain NestedValues and derived-view arguments as source type and constant
nodes through shape expansion, field/index checking and resolved IR admission.
Cache source descriptions by their syntax tree, resolve record aliases by their
defining declarations and keep external wire readers separate. Localize imported
application arguments recursively. Independently compare nested call aliases
only with a validated nominal declaration/layout catalog; missing identity
evidence still rejects. Cache v161 separates the source-admission change.
General nominal identity and the other builtin-family migrations remain open.

Retain transaction context schemas as type nodes through checking, lowering,
effect-source inference, metadata and backend transport. Preserve the existing
source/wire spellings and two-word helper ABI. A nominal name shaped like a view
cannot grant source facts; validate builtin view constructors and projections
against shadowing declarations. Keep structured view/vector parameter metadata
consistent with their emitted pointer/length transport and existing interface
identity. Cache v160 excludes earlier structured-span metadata results.
Other application families and resolved nominal identities remain unfinished.

Retain Vec elements as type nodes through source checking, IR, collection
construction, metadata and ABI selection. Copy local stack vectors into bounded
caller-owned Molecule spans for ordinary helper arguments, preserving forwarding,
empty values and stack-passed parameters. Preserve small aggregate results and
tuple projection facts, use intrinsic CKB domain widths, and validate gather-hash
arguments structurally. Cache v158 excludes earlier vector transport results.
The wider structured application/nominal migration remains unfinished; temporary
rejection of other builtin families is not a completed language boundary.

Retain specialization declaration owners, template bindings and type arguments
through monomorphization and IR. Mark generated declarations explicitly and
reject missing, conflicting or cyclic provenance. Infer generic dependencies,
source contracts and backend storage categories from retained records instead
of decoding or searching machine labels. Validate wire labels by re-encoding
their separately supplied arguments. Source/wire/interface/ELF characterization
precedes the remaining structured application and constant-argument migration.
Bind each source declaration's type parameters before expanding concrete
specializations, including parameters that shadow generated symbol spellings.
Cache v155 excludes earlier interface projections and provenance-era results.

Emit memory accesses and add-immediates from typed operations and explicit
operands. Remove instruction-text recovery from both generic emission and
allocated-store bounds guards. Preserve destination-owned large addressing,
explicit scratch exclusions and stack/outgoing-frame ownership. Native source
policy rejects reintroducing raw memory/address strings at emitter call sites.

Retain ordinary-function versus registered-runtime identities in IR calls.
Normalize only source function labels, infer runtime effects from the selected
helper ID, and dispatch all 219 runtime caller identities exhaustively. Select
BIP-340 IPC lowering from its typed runtime contract. Invalid specialized call
operands reject instead of falling through to generic ABI emission. Ordinary
labels do not acquire runtime abort/status/child-process behavior. Cache v154
excludes results produced before the call-target contract.

Make quote escapes symmetric across ordinary and byte strings. Assign stable
lexical diagnostics E1025-E1029 to unsupported escapes, malformed byte hex,
non-ASCII byte strings, invalid integers and source-size limits. Recurse into
remaining textual generic arguments when rejecting unstructured nested IR views.
Cache v153 excludes results with earlier lexical diagnostics. Retain closed typed
diagnostic identities in compiler errors and preserve checker rejection ownership;
wire and editor presentation continue to emit their existing string codes.

Use the supplied request clock and one exclusive cooldown cutoff for direct and
session namespace claims in Memory and PostgreSQL. Package creation requires an
already active namespace owned by the caller; it cannot implicitly claim one.

Move NovaSeal certification evidence and summary policy into an optional native
package behind the `novaseal-certification` Cargo feature. Default installations
reject unavailable certification with CLI0002, including supplied reports. Keep
package/Registry hashing independent; share producer canonical JSON without
importing compiler semantics into the proposal package. Summary schema v0.2
records the new implementation identity and source provenance. Existing evidence
status rules and verifier/path restrictions remain unchanged.

Pin immutable artifact downloads to their admitted DNS answers; reject private
IPv6/mapped addresses and mixed unsafe answers, preserve TLS hostname identity,
and disable proxy/redirect re-resolution for these downloads. Require complete,
unique, successful and receipted Fiber workflow inventories at report consumers.
Serialize all Memory Registry authorisation-state writers and reads with session
completion, and automatically resume durable rejected-source deletions in bounded
rotating worker batches. Check pruned callable signatures without granting them
machine effects. Document all twelve runtime return conventions and the exact
limits of their independent machine coverage.

Commit rejected-upload tombstones before deleting external objects. Keep a
pending deletion owned by the rejecting job, attempt and error when deletion or
its acknowledgement fails; retrying that identity completes cleanup without
reviving a missing source. Memory and real PostgreSQL regressions cover commit
failure, lost acknowledgements and snapshot identity changes.

Consume the pipe helper's status before receiving its two descriptor values.
Keep the helper's internal status/a1/a2 ABI and map successful source tuples from
the descriptor registers; a failed pipe creation cannot yield apparent fd values.
Replace the obsolete bare-spawn playground example with checked pipe endpoint
cleanup. Real VM and independent checker regressions own this boundary. Cache
v152 excludes prior pipe call results.

Reject unterminated byte strings and trailing byte-string escapes with E1022 and
E1023, preserving ordinary string diagnostics and valid byte escapes. Reject
unknown scheduler hint keys with E1024 instead of silently ignoring them. Resolve
selection read type arguments by defining nominal owner, then keep executable
view types bound to the admitted catalog schema. Equal-layout declarations from
different owners remain distinct. Cache v151 excludes earlier results.

Add `interface-diff --checked` for bounded, complete ELF bundles: independent
verification and qualified nominal projection precede directional compatibility.
Both interface comparison modes emit one JSON failure envelope containing the
report and E2501 diagnostic. The existing declaration wire/hash remains unchanged and may reflect import
spellings; checked compatibility does not infer identity from equal byte widths.
Generate protocol-builder schemas, versions and fixed encodings from native
constants while preserving generated TypeScript bytes. Share the closed
WitnessBytesView syntax and conformance vectors while retaining independent
source, range and machine admission. Route formatter idempotence through the
explicit source edition. Extract checker ISA predicates and relocate mixed u128
arithmetic without changing the corresponding machine algorithms.

Preserve trapping prefixes in all three source-byte loop matchers, including
source-to-memory and zero scans. Advance artifact caches to v149 so earlier
miscompiled results cannot be reused. Real CKB-VM regressions retain division
error 20 under standard and diagnostic compilation. Align entry-selection tests
with inlining and defining-owner-qualified generic labels while keeping runtime
selection and independent bundle checks.

Preserve independent IR lowering diagnostics during effect inference instead of
reporting only the first failure. Replace the retired effect-category CLI fixture
with two dynamic exact-handle commitments that must still fail lowering, and
retain both source ranges through the diagnostic API and JSON CLI output.

Refresh current-strategy scenario identities against unchanged VM verdicts and
fixed resource ceilings. Construct Hash explicitly from raw issuer args in
fungible/NFT fixtures. Record the new committed-state resource measurements
without increasing their budgets.

Read skill-pack metadata freshness from the checker-owned envelope constant,
sharing the documentation validator instead of accidentally matching the source
component's schema version. Require the fallible LSP entry point to route errors
through native CLI diagnostics in the tooling boundary check.

Require numeric deployment-line receipt version 3 in the CKB adapter, matching
the current v3 schema and native producer. Preserve rejection of other versions
when ProtocolBundle hashes are rebound; valid current bundles can materialize.

Make in-memory quota admission atomic and serialize it with session rollback.
Keep best-effort audit storage failures from replacing authentication responses.
Purge permanently rejected uploads through locked snapshot/reference admission,
retain relational tombstones and audit identity, and reject requeue or admission
of purged sources. Retryable infrastructure failures and other retained release
references keep their source bytes. Memory and PostgreSQL regressions cover
deletion failure, stale workers, concurrent retry and quota admission.

Infer three structured effect-fact sets for context reads, Cell lifecycle
operations and process operations. Remove the five-category model, handwritten
declarations, the native `effects` wrapper and earlier readers/translation paths.
The compiler and standalone checker reconstruct facts independently; interface
replacement checks inclusion in each dimension alongside the existing lifecycle
and binding obligations. Current-only metadata/interface/receipt consumers,
Registry admission, generated SDKs and browser/editor presentation use this
contract. Old or mixed generations require complete regeneration. See the
[effect contract](docs/CELLSCRIPT_EFFECT_CONTRACTS.md) and
[alpha.2 acceptance backlog](docs/releases/CELLSCRIPT_0_33_ALPHA_2_TODO.md).

Retain resolved nested-view owners, layouts and bounds through structured IR;
remove executable string reparsing and duplicate record reconstruction. Make
runtime helper availability and dispatch exhaustive. Reject bare named spawn
before metadata or artifact generation. Reject
unrepresentable fixed layouts with E1020, incomplete pool assignments and
unfinished string/comment tokens with source diagnostics. LSP analysis uses
revision-bound snapshots and shared view indexes outside its session lock;
invalid response locations fail without partial edits. Extract cohesive
compiler/native certification modules and select test binaries deterministically.
Cache v148 excludes intermediate artifacts. Focused checks and the canonical
browser build pass; final-source backend/dev/CI acceptance remains in progress.

Reject deferred maturity/time/epoch constraints and occupied-capacity floors
under production policy; development artifacts abort with runtime errors 86/87.
IPC reads return a complete u64 or abort, and inherited-fd selection checks both
the reported count and the 64-entry copied prefix. Advance artifact caches to
v137. Namespace claims enforce cooldown once in the store, reject competing
owners with HTTP 409, and consume no claim quota on failure or same-owner replay.
Memory claims share session completion's rollback lock; SQL claims serialize
with session completion through the principal row. Behavioral regressions cover
real VM failures, controlled IPC boundaries, HTTP routes and concurrent claims.

Preserve trapping byte-loop prefixes and invalidate older compiled cache entries
(v135). Keep parser expression budgets across nested operands and reject partial
or overlapping writes to tracked saved return-address slots. Preserve signed
Registry lockfile hashes, exclude withdrawn public releases without hiding active
siblings, and reject oversized discovery pages explicitly instead of truncating
their package/release contents. Focused regressions cover these boundaries;
complete backend/CI acceptance remains separate.

Unify normal compilation under one strategy and remove numeric optimization
levels, old CLI controls and the four-level comparison command without
compatibility aliases. Keep internal pass isolation for compiler diagnosis.
Build/cache/evidence identities now name the strategy; rebuilt ELF/code hashes
require explicit checking. Validation is in progress; see the
[compilation policy](docs/CELLSCRIPT_COMPILATION_POLICY.md).

Retire digest-shaped raw byte buffers and misleading read names. Exact Cell,
Script-args and witness reads now return `[u8; 32]`; variable-size fields retain
bounded complete views. Remove the legacy 512-byte whole-Cell hash buffer;
choose consensus DATA_HASH or explicit streaming BLAKE2b, preserving their
different empty-input semantics. Update source diagnostics, metadata, editor,
browser source, examples and VM fixtures. Independently validate complete
streaming/exact-witness machine contracts and retain rebound-mutation rejection.
Raise the browser gzip budget from 600 to 1500 KiB under the maintainer's
October 28 instruction. Rebuild the canonical browser bundle within that limit;
its 63 interface checks pass. Browser summaries explicitly reject native proof
and provenance records, while unbound external delegation remains rejected.
Clean-source full backend acceptance remains deferred; see the
[retirement record](https://github.com/CellScript-Labs/CellScript/blob/f7a7a0f6458dec594f75bd0e2c8cd65fa914a1e2/docs/CELLSCRIPT_BYTE_BUFFER_RETIREMENT.md).

Share U128 owned storage between fixed-byte and arithmetic emitters, and omit
unused parameter scratch without changing the checked pointer/length ABI.
A five-U128-argument fixture reduces its helper frame from 1632 to 1456 bytes
with unchanged ELF size and successful execution cycles. Align nominal
ScriptHash/SighashAllDigest parameter and result contracts without making them
interchangeable with Hash. Preserve explicit hash construction and full-byte
ScriptHash serialization. Prevent branch fusion from comparing packed epoch
fractions as integers; equivalent fractions retain their semantic equality.
See the [wide-value record](docs/CELLSCRIPT_WIDE_VALUE_ARGUMENTS.md).

## 0.33.0-alpha.1 - 2026-10-28

First 0.33 alpha, published under the maintainer's October 28 instruction after
compiler integration tests. APIs may change. Full backend/dev/ci/release gates
and peripheral review were explicitly deferred; no skipped check is a pass.
See the [alpha release notes](docs/releases/CELLSCRIPT_0_33_ALPHA_1_RELEASE_NOTES.md)
and [identified-source test receipt](docs/reports/bounded-runtime-v129-alpha-tests.json).
The implementation entries below retain their historical validation status;
the alpha receipt records the publication decision without claiming stability.

### Implementation history since 0.31

Declare runtime return conventions alongside helper dispatch and make scalar
callers check `a1` for all fallible value getters, including the four legacy
OutPoint/low-hash getters. Independently validate those four machine bodies
and their caller error guards. Preserve the legacy `cell_data_hash` contract:
BLAKE2b of at most 512 bytes, including BLAKE2b(empty). The separate consensus
`cell_data_hash_field` / typed `.data_hash` returns zero for empty data and has
no 512-byte limit. Expose `cell_data_bytes32` for raw 32-byte slices while retaining
`cell_data_hash_at` as a compatibility alias. Independently check field and
slice bodies, including direct calls; canonicalize metadata and editor/browser
surfaces. Advance artifact-cache generation to v129;
final gates remain pending.

Refresh the canonical multi-Script anchor and persistent-policy artifact,
transaction and bundle identities after the runtime changes. Preserve every
numeric observation and resource ceiling; the original seven-test business
suite passes with the refreshed references. The identified-source backend
attempt stopped at the stale fixtures, so full integration gates remain pending.
Update seven CLI policy rejection examples to use an unadmitted dynamic-vector
helper result; retain their policy assertions and pair them with an accepted
fixed Hash helper output. All 253 CLI tests pass without weakening admission.

Separate read capacity, syscall-reported remaining length and published value
length. Reject short fixed Cell-data slices and xUDT amount words, and reject
oversized legacy WitnessArgs projections before reading beyond their 512-byte
buffers. Reject Cell data larger than the legacy hash buffer before hashing.
Independently bind Cell-data hash/slice machine
contracts and reject rebound capacity, length-branch and pointer mutations.
Large-immediate expansion preserves unrelated live registers; implicit scratch
use and scratch exhaustion fail compilation. Advance artifact-cache generation
to v127. See the [read-boundary audit](https://github.com/CellScript-Labs/CellScript/blob/f7a7a0f6458dec594f75bd0e2c8cd65fa914a1e2/docs/CELLSCRIPT_READ_BOUNDARY_AUDIT.md).

Audit runtime, ordinary-function, outgoing-argument and generated-library stack
layouts. Centralize aligned SP adjustments, compact hash/Script/WitnessArgs,
SHA, OutPoint, MetaPoint, DAO, xUDT and process helper frames, and remove obsolete
legacy witness return fragments. Independently reject balanced but misaligned
machine frames; retain exact source, copy, output-pointer and saved-slot checks.
The O0/O3 all-helper machine inventory covers 247 entries and 360 adjustments.
Advance artifact-cache generation to v126. See the
[layout audit](https://github.com/CellScript-Labs/CellScript/blob/f7a7a0f6458dec594f75bd0e2c8cd65fa914a1e2/docs/CELLSCRIPT_STACK_LAYOUT_AUDIT.md) for per-frame sizes, retained
ABI reservations and validation limits. This remains candidate work,
not final bounded-runtime acceptance.

Align the current-Script and transaction-hash runtime helpers to compact 16-byte frames
so they compose with live typed Cell views under the existing 16-byte alignment
check. Cover both helpers with strict O0/O3 compilation, actual VM execution and
rebound frame mutations. Advance artifact-cache generation to v125; final
bounded-runtime gates remain pending.
Retain `ScriptHash` for typed Cell `lock_hash`/`type_hash` and ScriptRef `hash` properties through IR,
buffer allocation, admission and independent result checking. Keep raw SourceView
hash calls and `data_hash` in the ordinary Hash domain. Independently recognize
the context-hash, ScriptRef field, input OutPoint and exact WitnessArgs helpers'
finite caller-buffer writes and reject overlap with live protected buffers.
Keep ScriptRef/OutPoint projections from helper parameters bound to the caller's
dynamic source/index rather than inventing a global Input coordinate.

Add the bounded-runtime admission candidate for #47–#49: retire four blanket
experimental proof flags while retaining nominal, machine, transport and lifetime
checks. Strict-policy tests cover complete business programs and rebound removal
of required proof; editor/browser summaries and the support matrix use the same
boundary. Advance artifact-cache generation to v124. See the
[scope review](docs/CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md); final source gates
and issue closure remain pending. No release, numeric-limit or wire-format change.
Recognize checked named-role projections and bounded nested reads/successors in
the executable-surface and output-obligation classifiers; retain independent
source/codec/comparison validation for each.
Preserve fixed tuple result availability through copies, and recognize checked
scalar/fixed-byte indexing of known packed-byte and pointer-backed fixed arrays. Existing
collection/value regression programs now exercise these paths in strict mode.

Protect copied collection results between checked publications: ordinary stores,
helper effects and syscalls cannot overwrite the retained bytes or their pointer
slots. Mutable record bindings still copy into separate storage, and snapshots
remain usable after owner drop. Add O0/O3 rebound mutations with VM comparisons
and advance artifact-cache generation to v123. Stable admission remains pending.

Bind typed Input/Output `data_hash`, `lock_hash` and `type_hash` property calls
to live coordinates, checked runtime implementations, status handling and
published pointers. Require distinct private result buffers, protected from
intervening stores and calls. Advance artifact-cache generation to v122.
Return-tail mutation tests locate the actual restoration/RET block even when
source evidence shares its address; restoration, partial-entry and skipped-field
mutations retain their original coverage. Stable admission remains pending.

Bind typed Input/Output view `capacity`, `occupied_capacity`,
`unoccupied_capacity`, `data_size` and `output_index` calls to their source
coordinate, runtime implementation, error path and scalar destination. Require
these calls in bounded-body operation order. Independent runtime checks cover
source decoding, syscall arguments/results, capacity subtraction and frame
restoration. Rebound runtime values, call sources/results, ignored errors and
erased evidence reject. Property reads also require their coordinate to remain
live after intervening writes and helper calls. Advance artifact-cache generation to v121; stable
admission and final integration gates remain pending.

Bind direct collection-read returns of `u128` and `[u8; 1..8]` to the
copied result pointer and exact little-endian return-register assembly. Require
the return evidence and preserve both `u128` limbs through frame restoration.
This extends the checked return boundary for collection results produced in
the same typed block; general aggregate computation, aliases and tuple returns
remain separate. Advance artifact-cache generation to v120.
Materialize wide fixed-byte call literals in immutable data with their exact
length, instead of passing a null pointer and zero length. This fixes missing-key
`OrderedMap<u64, u128>` defaults passed as literals through readonly helpers;
register and outgoing-stack calls retain the same value representation.
Keep complete authoring-comparison sources under the selection-bundle fixtures,
outside the package scenarios' sibling-module directory. Their contents and
named/explicit and collection/array comparisons are unchanged; the frozen
array-source record changes only its path. Package scenario discovery continues
to run the same scenario tests.

Bind bool/u8/u16/u32/i32/u64 returns in bounded collection, Cell and nested-view
bodies to their evaluated typed source. Check the complete return inventory,
mandatory execution, lock predicate failure and shared frame restoration while
preserving `a0`. Rebound zeroed/substituted returns, epilogue result clobbers and
erased return evidence reject. Advance artifact-cache generation to v119;
aggregate return provenance and final admission remain separate obligations.
Compose returning observation sites only after actual-register and bounded-write
lifetime checks, preserving standalone nested Cell creation followed by a return.
Sites seen as EXIT remain ineligible for returning summaries.

Bind non-fused branches and jumps in bounded collection, Cell and nested-view
bodies to their typed conditions and successors. Require complete branch
evidence and preserve decoded control flow in the operation-order check.
Only actual machine fusion exempts a branch from this separate inventory;
unoptimized comparisons retain their own branch checks. Advance artifact-cache
generation to v118. Stable admission and final acceptance remain pending.

Freeze eight O3 resource observations for 1024-input ordered-map sorted
construction: unique keys, duplicate compaction, late disorder and shared-arena
exhaustion. Bind exact source/ELF/transaction identities, Type-group cycles,
observed stack and guest pages, witness/transaction sizes and explicit unavailable
heap/rejected-transaction totals. Independent scheduler replay must agree with
the authoritative group result. Existing business resource ceilings are unchanged.

Extend nested codec reference evidence with a test-only Molecule reader using
literal fixture field widths and limits. Native and generated TypeScript bytes
round-trip every admitted fixture value, including empty/mixed-size byte and
record lists, 256-byte payloads, and both shapes at exactly 4096 encoded bytes.
Truncation, trailing bytes, rebound total sizes and invalid offsets reject;
record count/payload/total one-over cases reject in both builders. This adds
acceptance evidence without changing runtime behavior or stable admission.

Require successful typed-block paths to execute checked collection operations
in source order, alongside scalar definitions and Cell operations. Bind each
owned or borrowed top-level primitive to its typed operation; owner drop must
retire every declared member. Constructor coverage now composes with completed
nested/Cell instruction and lifetime proofs. Rebound jumps that skip a push,
member retirement, or borrowed query/result reject. Advance artifact-cache
generation to v117 without changing emitted instructions or resource ceilings.
This establishes per-block operation coverage, not arbitrary source-CFG or
value equivalence; final admission gates remain pending.

Bind owned collection `len`, `capacity` and `is_empty` descriptor checks, using
the same independent instruction grammar as readonly borrowed queries. Bind
these queries and ordered Map/Set membership results to their typed scalar destinations
for both owner forms. Reject replaced result stores, descriptor-result
instructions, omitted query evidence and entries that bypass the producer.
Advance artifact-cache generation to v116. Query labels add evidence without
changing emitted instructions; complete operation-path coverage and final
admission gates remain separate obligations.

Keep distinct scalar slots in ordinary wrappers that call bounded helpers, so
checked argument preparation and result reception use the same layout as the
compiler. Other scalar-only functions retain slot reuse. O0/O3 regressions cover
repeated Cell reads through wrappers and rebound result-store mutations. Advance
artifact-cache generation to v115; this can change wrapper ELF/frame layouts.
Browser language-service checks distinguish declaration and effect completion
positions. Final integration and source admission remain pending.

Protect owned arena state, allocation records, elements and descriptors between
independently checked collection operations. The write-effect checker now also
covers local-only owners; ordinary stores, calls and syscalls cannot rewrite
their storage. Only the matching checked constructor or primitive may write an
arena. Recognize finite private byte-copy loops and constant multiplication so
existing tuple reception and stack Vec operations retain their behavior.
Rebound mutations corrupt ordinary result stores and inline copies without
altering collection bodies; O0/O3 execution demonstrates changed outcomes.
Advance artifact-cache generation to v114. Complete source/value admission and
final integration gates remain separate obligations.

Bind scalar results of bounded helper calls to their typed caller destination.
Recognize the actual call continuation, exact outgoing-stack restoration and
complete `a0` store for bool/u8/u16/u32/i32/u64, including helpers without scalar
parameters. O0/O3 rebound mutations cover zeroed results, redirected stores and
incorrect frame restoration across collection, Cell and nested-view helpers.
Advance artifact-cache generation to v113. This proves result reception, not
the callee's value computation or preservation across later arbitrary writes.
Record the initial authoring-form decision against complete settlement,
migration and transfer programs: named declarations, ordinary reads/helpers,
and explicit count-based reservations with CLI/editor budget explanations.
Stable admission and final integration gates remain pending.

Bind ordinary scalar entry parameters and evaluated call arguments for Cell and
nested-view helpers, as well as collection helpers. Count expanded coordinate,
fixed-value and pointer/extent ABI words before scalar register/stack arguments.
Require complete preparation/invocation evidence and preserve the evaluated
arguments to their consuming call across every reaching path. O0/O3 mutations
cover zeroed/substituted spills, caller source replacement, erased evidence,
bypassed preparation and later register clobbers. Advance artifact-cache
generation to v112. Arbitrary upstream value and aggregate provenance remain
separate obligations; this does not open stable admission.

Extend scalar definition and successful-path checks to bodies using Cell
coordinates, exact selections and nested views, including named-shape helpers
without an arena. Compose complete nested read/load/output bodies only after
their instruction and lifetime proofs pass; these bodies cannot hide required
scalar operations. Recognize register-local logical-right-shift constants and
retained coordinate arguments after full-slot stores, while invalidating them
on overlapping writes, calls and joins. Advance artifact-cache generation to
v111. Refresh sidecar and ProtocolBundle identities for the additional evidence;
executable/transaction identities and resource ceilings remain unchanged.
Complete aggregate/source provenance and stable admission remain pending.

Require successful typed-block exits in collection bodies to execute every
checked scalar definition in source order. Reject whole-definition bypasses,
including rebound branches that change actual VM results while leaving the
definition's instructions intact. Fused comparisons expose both successors to
the path checker; arithmetic bodies retain their independently checked fatal
guards. Compose independently checked Cell reads and selections in the same
operation order, including inline returning syscalls. Advance artifact-cache
generation to v110. Unchecked syscall-number annotations cannot authorize
fallthrough past required operations. This does not establish
arbitrary value preservation or complete source-to-machine CFG equivalence.

Bind scalar arguments at readonly collection-helper calls to their evaluated
typed locals/constants and complete expanded ABI positions. Require preparation
and invocation evidence, recognize every load/store, and preserve each argument
through intervening code to the exact consuming call on all reaching paths.
Consume argument facts after calls so later invocations cannot reuse them.
O0/O3 runtime and rebound tests cover forwarding, nested calls, source
substitution, erased evidence and preparation bypasses. Advance artifact-cache
generation to v109; complete upstream source/value proofs remain pending.

Bind ordinary scalar parameters alongside readonly collection handles to their
exact incoming helper ABI words. Count pointer/extent pairs when locating later
register or stack arguments, and reject substituted sources or zeroed spills.
Rebound tests include all six scalar types, mixed Hash/owner parameters and a
mutation that changes actual VM behavior. This establishes entry spills only;
caller scalar provenance and subsequent value preservation remain separate.

Extend scalar machine binding to all six integer comparisons and boolean
equality. Check materialized boolean values and fused conditional branches,
including signedness, swapped operands, inverted predicates, both successors
and eliminated empty jump blocks. A fused result must have exactly one typed
use, its adjacent branch condition. O0/O3 VM cases compare extreme/equal values
with independent models; rebound mutations cover result instructions, missing
markers and redirected branches. Advance artifact-cache generation to v108.
Parameter/aggregate provenance and complete CFG lifetime remain open.

Bind scalar definitions in collection bodies to their typed constants, operands,
operations and destination slots. Check integer arithmetic, width normalization,
division/shift guards, boolean operations and copies against decoded instructions;
require complete marker inventory and reject partial entry. These checks do not
yet establish parameter decoding, aggregate provenance or complete CFG lifetime.
Keep contextual nonnegative i32 literals signed through assignment, casts and
arithmetic, and sign-extend i32 array/vector index reads. O0/O3 fixtures compare
integer operations with host models and reject rebound instruction/semantic
mutations. Advance artifact-cache generation to v107 for the new boundary.

Preserve complete fixed primitive tuple values returned by helpers. Keep the
eight-register field ABI, read returns from current value storage, and copy all
returned fields into the caller's storage before another call can reuse the
callee frame. Copies, forwarding, conditional updates and nested short-aggregate
fields retain their values. O0/O3 VM fixtures cover packed and wide fields,
empty fields, signed scalars, all eight registers and large field offsets.
More than eight return fields fail closed instead of silently truncating.
Resource tuples retain separate handling; general tuple source/lifetime proof
remains open. Advance the artifact-cache generation to invalidate older output.

Bind nonempty `load_sorted` inputs to their evaluated typed local and adjacent
checked member primitive, including packed arrays and pointer-backed storage.
Reject source substitution, omitted preparation, forged member coordinates and
branches that bypass preparation. Re-read mutable record fields after whole-value
assignment while preserving saved snapshots and explicit field/index writes.
O0/O3 VM checks cover field snapshots through reassignment and loops, 1024-item
sorted maps, last-item disorder/duplicates and shared arena exhaustion. Upstream
value computation and complete source/lifetime equivalence remain unproved.

Materialize projected assignments to mutable local records, tuples and fixed
arrays by rebuilding each affected parent in separate storage before replacing
the root. Copies, collection inputs and by-value helpers observe those writes;
saved values remain independent. Type literals against their destination field
or element, including u8 assignments. Array writes retain constant-index and
bound checks. Refresh iteration, destructuring and match reads across loop
backedges. VM/checker fixtures cover mixed nested paths, conditional/repeated
writes, imported generic records, packed arrays and 1024-byte inputs. Serialize
fixed primitive tuple/array fields in records, and preserve addressed short-byte tuple
fields, including register-returned fields read on conditional paths. Cell
resource transitions keep their separate lowering contract.

Preserve 1–8-byte array values when initializing or copying mutable locals.
Pack byte-array literals and moves consistently, and resolve actual byte storage
for indexing instead of treating every local slot as a pointer. Copied fields
retain their own value after reassignment. VM regressions cover all short-array
widths, literal/witness/record/collection sources, readonly helper returns,
owner drop and unsigned index bounds. These correctness fixes do not remove the
remaining independent source/value proof obligations.

Keep copied fixed-value locals independent of their original field and witness
length metadata. Small records now copy their bytes as wider records do;
reassigning a parent cannot redirect a saved field. Bound pointer-alias analysis
and reject incompatible reference bindings instead of looping indefinitely.
O0/O3 VM regressions cover 1/8/16/32-byte fields through local snapshots,
collection arguments and readonly helper defaults.

Bind complete scalar and pointer-backed fixed-value collection argument lists to
typed local slots/literals and the consuming checked member primitive. Verify byte order, full scalar width,
Vec indexes and Map get_or defaults for owned operations and readonly helpers.
Reject substituted sources, missing preparation and entry after preparation,
including large stack offsets. Wide and record copies read their evaluated typed
locals after retaining existing source/length checks, with one preserved pointer
load per copy. Derive record copy widths independently. Short-array arguments
bind packed words/literals and pointer-backed field, index and collection values
to their typed producers; reference conversions preserve that representation.
Fix indexing of packed aggregate parameters and preserve the addressed storage
of short-array elements. Upstream sorted-array provenance and complete
source/value/lifetime equivalence remain separate admission obligations.
The transfer replay reduces the main ELF from 41,392 to 40,536 bytes and the
32-distinct-asset transaction from 2,052,694 to 2,040,832 cycles. Refresh measured
observations while retaining every existing ceiling and canonical transaction.

Reject coherently rebound Cell field copies that alias another data result or
load buffer. Derive each copied result's full extent from its checked schema
and compare data-producer write regions within the invocation. Preserve those
buffers across ordinary stores and summarized memory effects. Fixed and nested
table regressions include a VM comparison changed by the forged alias; stable
source/value admission remains blocked.

Snapshot mutable local values when creating a new immutable binding, including
explicitly typed bindings. Reassigning the original local must not change the
saved value. Verify copied Cell fields with scalar, u128, Hash and byte-array
VM cases, including branch assignment and fixed/nested data layouts. Collection
record snapshots also survive later local assignment, collection mutation and
owner drop for 8–64-byte values. Refresh the canonical persistent-policy sidecar
and bundle identities from replay; executable bytes and resource ceilings stay
unchanged.

Preserve named Cell role navigation and rename across receiver comments and
parenthesized receivers, including multiline comments and range endpoints.
Resolve references within their own catalog and reject renames initiated from
comments or string text.
Check the complete settlement program after edits and the optional browser
language service's definition locations.

Extend the complete mixed-role settlement bundle comparison to named and
positional authoring at O0/O3, including generated SDK bindings, canonical
transaction bytes, required peer execution and rebound hostile reports. Retain
the named resource baseline and report the positional cost difference separately.
Refresh the business-corpus evidence digest after the bounded-runtime changes;
this preserves its candidate status and all existing admission requirements.

Bind copied collection `u128`, fixed-byte and record results to the checked
producer, full value width, private destination buffer and typed pointer slot.
Reject truncated/shifted copies, missing publication evidence and destinations
overlapping scalar slots, collection storage or other copied results. These
local transport checks retain the allocation admission blocker; sorted input
provenance and complete source/machine ownership remain separate work.

Show collection budget scenarios directly on declaration hover, including
per-member reservation costs, cumulative growth and shared byte/event shortfalls.
CLI and editor use one calculation; imported generic layouts follow open package
buffers, and unresolved layouts do not claim a fit. The optional browser language
service exposes the same hint without adding fields to ordinary compile metadata.

Fix missing editor field completion when Cell schemas or nested records come
from a dependency package. Resolve fields through nominal imports and package
visibility, including aliases, instead of searching only the current file.
Honor open dependency buffers and retain readonly payload methods; unrelated
same-named declarations and inaccessible private types do not supply fields.
Trailing-dot completion recovery preserves those open dependency buffers and
their visibility without replacing the unfinished document's diagnostics.

Add locked dependency-package fixtures for readonly helpers in complete mixed
settlement and byte/record migration, preserving canonical transaction bytes and
required peer execution. Reject mismatched helper bounds and source/schema
substitution. Verify imported generic record copies after local collection drop,
including wide values, budget widths and standalone checker evidence. Package
variants retain existing resource ceilings; module-local declaration exports,
complete source equivalence and stable API admission remain separate work.

Add experimental `cellc explain collections [INPUT] [--json]` for named collection
budgets, including unused declarations and resolved record/generic layouts. Show
per-member payload, alignment, allocation records and retained growth costs;
compare reserving every maximum once with incremental growth from an empty owner.
Report shared byte/event shortfalls and distinguish fixed owner storage from total
stack use. This analysis changes neither allocation policy nor source syntax and
does not establish a program's worst-case budget or freeze the authoring API.
Cross-check its byte/event thresholds against an independent model and 68 O0/O3
VM executions. Compare actual-count and full-maximum reservations in the complete
transfer corpus: full maxima increase small-transaction initialization costs and
exceed existing resource ceilings. Preserve the baseline and record candidate
exceedances; automatic full-maximum reservation is not adopted as the default.

Independently bind collection index arguments to typed constants/local slots and
copied scalar results to their typed destinations. Compose this transport with
the same checked owned/readonly member primitive, including Map entry/value
offsets and signed `i32` publication. Require complete typed inventories and reject
branch entry that bypasses preparation or the result producer. Evidence labels
preserve existing immediate stack-load forwarding. Fixed-width result publication
is covered by the transport checks above. Wide/record input copies, upstream
arithmetic and complete source/lifetime equivalence remain pending; retain the
allocation admission blocker.

Independently check readonly collection helper bodies against caller-relative
descriptors: exact extent admission, live allocation record walks, length/capacity,
ordered searches, indexed copies and conditional/required Map lookup. Require
typed site inventories even with all borrowed-read markers erased,
and bind each site's pointer/extent to checked incoming facts through the CFG.
Copied bytes stay within disjoint private scratch. Index and copied-result
transport are covered by the checks above. Key staging, upstream value provenance
and complete source/lifetime correspondence remain pending; retain the allocation
admission blocker.

Independently bind arena helper calls to their typed owner identities and paired
extents through the machine CFG. Pointer facts originate at checked constructors
or checked readonly parameters, survive actual copies and joins, and disappear
after overlapping writes or reconstruction. Check call targets/order, expanded
register/stack ABI and forwarded extent identity. Rebound tests swap same-type
owners, substitute lengths and overwrite retained bindings on either branch
before a join. Consume straight-line proof states after propagation while
retaining branch and summary checkpoints within unchanged checker budgets.
Complete machine retirement, borrowed read values and source correspondence
remain pending.

Allow coordinate-bearing helper calls after guarded collection writes in the
same typed block. Share exact spill/guard/store/restore recognition between the
independent transport and memory-effect checks; prove private write bounds and
require fresh coordinate argument loads after each dynamic write. Rebound tests
cover broken guards, spills, restoration, interior entry and argument loads;
CKB-VM cases cover register/stack calls, forwarding and large arenas. Complete
arena lifetime and borrowed read admission remain pending.

Independently bind readonly collection helper pointer/extent spills to their
typed incoming ABI words, including register/stack splits and mixed nested/Cell
view signatures. Preserve both slots through the helper's checked memory-effect
CFG and reject full or partial overwrites. Complete machine owner lifetime, borrowed
read bodies and complete source correspondence remain pending; retain the
allocation admission blocker.

Independently recognize owned collection reads into bounded private result
buffers, read-before-remove ordering, pop's last index, conditional Map copies
and mandatory missing-key failures. Bind `load_sorted` to its typed fixed input
count, empty destination, non-overlap/overflow checks, nondecreasing key scan,
unique-key reservation and last-value-wins copying. Rebound tests cover small
and large owners, numeric and byte keys, empty/singleton batches, omitted ranges
and child bypasses. Borrowed access, input/result provenance and whole-operation
source correspondence remain pending; retain the allocation admission blocker.

Independently bind owned Map/Set binary search to member key width, unsigned
integer or lexicographic byte order, shrinking bounds and returned found/index
values. Compose checked search and element bodies with exact insertion,
replacement, duplicate-set and absent-removal branches. Rebound tests cover
comparison direction, byte/stride arithmetic, return values and child bypasses;
all admitted key profiles retain O0/O3 coverage. Borrowed helper search,
argument/result transport and whole-operation correspondence remain
pending; retain the allocation admission blocker.

Independently recognize owned-vector push/insert/set/remove instructions shared
by Vec and ordered collections. Check index bounds, complete element copying,
overlap-safe shift direction, vacated-byte clearing and length publication.
Bind growth to a checked reserve for the same member, and require typed site
inventories for both ordered-map replacement/insertion paths. Input provenance,
argument/result transport and complete admission remain pending.

Bind allocated reserve/drop descriptor transitions to the typed member's address,
element width and limit. Independently check descriptor admission, bounded growth,
resize/release child ownership and pointer/capacity publication or full clearing.
Recognize early-return restore tails against the actual private frame, excluding
temporary spill budgets. Readonly borrowed descriptors and complete source
correspondence remain pending; retain admission blockers.

Independently bind Cell-view constructors and aliases to their typed slots,
index operands, source tags and checked runtime helpers. Track live coordinates
through data projections, selected-role reads and helper calls; reject rebound
producer bypasses and intervening full/partial slot overwrites. Complete scalar
index and field-result provenance remain pending; admission blockers stay in
force.

Add experimental Edition 2027 `collections` declarations for named Vec,
OrderedMap and OrderedSet members sharing explicit byte/allocation budgets.
Validate member limits and fixed type boundaries through parsing and type
checking, with formatter, editor, doc and WASM coverage. Add experimental local
owner moves, copied member reads, bounded iteration, lexical borrows and
non-escaping read-only `&Owner` helper parameters. Reject Cell lifecycle use,
alias escape and mutations while a call is still evaluating borrowed arguments.
Lower local construction, moves, copied reads, mutation and ordered iteration
through a shared bounded frame arena. Record structured allocation contracts
and retain the mandatory `allocated-values-artifact-proof` admission blocker;
read-only helpers retain the caller's arena and checked extent while using
callee-local copy scratch. Helpers support forwarding, early return and copied
results. An unused declaration creates no allocation or wire interface. Source
CKB-VM tests cover local operations, helper calls, exhaustion and value copies;
complete business acceptance remains pending.

Independently track allocated owner availability and read-only loans through
the typed control-flow graph. Reject rebound stale moves/drops, borrowed
mutation, invalidated helper arguments and branch/loop lifetime violations.
Retain lexical alias identities used only for nested reborrows. Allocation
machine-state/source proof and its mandatory admission blocker remain separate.

Bind experimental owner constructors to independently checked machine stores
for zero arena state, empty live descriptors and the published owner pointer.
Check private-frame extents and successful-path initialization using the shared
typed-block CFG validation. Rebound tests cover small/large arenas and multiple
owners. Complete collection machine/source admission remains pending.

Independently recognize bounded arena allocation instructions: admission checks,
monotonic cursor/event updates, record fields, guarded stores and zeroing.
Require allocation-site counts from typed operations and constructor storage;
rebound tests reject mutated predicates/writes and erased or extra ranges.
Bind arena resize/release to exact live-record lookup, their checked allocation
children, bounded copying and retirement without budget refunds. Rebound tests
cover record walks, copy/retirement mutations and parent/child bypasses at large
owner offsets. Descriptor state and complete source correspondence remain
pending; retain the mandatory admission blocker and existing arena policy.

Compose allocated collections with retained Cell data using checked dynamic-write
address bounds and a separately validated spill/restore sequence. Preserve the
checker state budgets and allocation admission blocker. Add a transfer-only
multi-asset Cell example with Map aggregation, Set duplicate detection, a borrowed
helper and O0/O3 canonical transaction/all-group VM comparisons against an
independent host model. Full allocation semantics and application resource
acceptance remain pending.

Compare the complete transfer with an equivalent 32-item fixed-array program
over 44 cases at O0/O3, requiring identical packed transaction bytes. Freeze
24 O3 resource observations with source/ELF/transaction identity and separate
numeric ceilings. Repeated and distinct asset labels show different cycle
tradeoffs; this finite comparison is not a global worst-case bound.

Add native and generated TypeScript fixed Cell encoders for supported flat
schemas. Derive field order/width and total size from metadata, enforce canonical
booleans and reject missing/extra fields or unsupported schemas. The complete
multi-asset transfer fixture uses actual SDK-produced Cell bytes and checks
native/reference parity while retaining its proof-admission blockers.

Extend experimental collection values to ordinary fixed records and concrete
generic records within the existing 64-byte Vec / 32-byte Map value bounds.
Resolve nested field layouts before checking budgets, retain unused member
layouts in scoped evidence, and copy records through ordinary helper ABI storage.
The independent checker derives record widths and field offsets separately;
hidden Cells, references, recursion and dynamic owned fields reject. Record keys
remain unsupported, and the allocation proof blocker remains mandatory.

Add experimental empty-member `load_sorted` for ordered collection arrays of
at most 1024 entries. Validate order before allocation, deduplicate Set keys,
keep the last Map value and reserve once for the distinct count. Materialize
fixed array literals and contextually type tuple fields; independently check
array shapes. Record-valued batches, arbitrary-order sorting and allocation
machine/ownership admission remain pending.

Extend the caller-owned fixed-result ABI to Hash, Address and 9–64 byte arrays,
so later calls cannot overwrite a returned pointer into a retired callee frame.
Arrays of 1–8 bytes retain the scalar ABI, packing copied or borrowed storage
into a value at helper argument and return boundaries. Independent checking
derives the exact buffered result width and checks the copy before
frame teardown. Collection allocation/ownership proof remains separate.

The arena, vector and ordered Map/Set primitives account for retired growth
buffers and check deterministic exhaustion, initialization, stable insertion
and removal against independent CKB-VM models. Numeric/byte ordering is compared
with `BTreeMap`, including adversarial descending insertion costs. Independent
machine/ownership admission, complete business examples and frozen source-level
resource budgets remain pending.

Bind experimental mandatory exact-role selection sites into generated builder
manifests and v2 ProtocolBundle reports. Reconstruct membership and nested
codecs from concrete transaction data and recheck packed outputs during
materialization. Retain v1 for bundles without selection; generated consumers
reject catalog removal, altered bounds and schema downgrade. Conditional-path
binding, complete business builders and stable admission remain pending; all
existing proof-admission blockers remain enforced.

Add an experimental fixed mixed-role settlement example with 1–6 full-fill
orders, two exact asset groups and checked read-only helpers. Compare complete
named/positional programs through all-group VM cases and reject late balance,
identity, custody and peer substitutions. Recognize complete shared return tails
without treating them as bypasses of a final typed field-check block; altered
restores and skipped checks still reject. Full builder/source admission, resource
freeze and stable artifact admission remain pending.

Exercise that complete settlement through generated SDK bindings, Type-hash
deployment-line receipts and the existing ProtocolBundle transaction adapter.
Compare canonical transaction bytes and run every required Script group for
1/3/6 orders. Local resolver checks reject input, code, admission and witness
substitution; package transfer predicates still require VM execution. This is
experimental local evidence, not live-chain or stable source admission.

Add an experimental named-role nested batch migration with extracted read-only
helpers. Preserve history, append a witness release note and advance document
revisions with an independently executing Registry. Full-group VM tests cover
one through eight documents, maximum payloads and hostile successors. Mixed
transaction-view/nested helper signatures now reuse the independently checked
parameter-decoder prefix while still binding coordinate spills. Add complete
generated SDK/ProtocolBundle fixtures for empty, mixed and maximum histories:
native, TypeScript and independent Molecule vectors agree, generated data retains
canonical transaction bytes, and migration/Registry/Lock groups execute them.
Reject malformed tables, changed bounds, substituted histories and late-invalid
notes. Stable builder/source admission, resource ceilings and final gates remain
pending.

Add the complete record-list migration counterpart with positive, increasing
historical revision identities and a checked next-revision note. Compare named
shape and explicit-bound programs at O0/O3 through generated SDKs, independent
Molecule vectors, canonical adapter bytes and all required Script groups. Cover
the exact aggregate byte budget and reject over-budget batches, duplicate history
and incorrect new identities. Editor/WASM checks retain derived helper bounds;
independent checker mutations reject paired payload argument substitution after
hash rebinding. Stable proof admission remains pending.

Freeze O3 SDK/ProtocolBundle business resource fixtures for settlement and
byte/record-list migration. Require standard-verifier parity for each traced
Script group, measure combined stack use and touched guest pages, and record
maximum-batch late-invalid cycles. Bind complete ELF/transaction identities and
source hashes to exact observations and separate numeric ceilings.
Complete 32-Cell global scans retain the same protected membership; separate
33rd-input and 33rd-output cases reject in both native reconstruction and the VM.
Combine full scans with maximum-batch late business failures in all three programs;
retain canonical adapter bytes and per-group failure evidence within the existing
scan ceilings. The frozen inventory contains 35 cases.
Failed transaction totals and heap-only measurements remain explicitly unavailable;
these finite cases do not establish a global worst-case bound or stable admission.

Protect helper-carried transaction-coordinate parameter slots throughout the
independent CFG memory-effect proof. Previously a rebound ordinary body store
could overwrite a parameter after its checked spill; full, partial and overlapping
writes now reject, including helpers without `.data` reads. Preserve checked
mixed nested parameter prefixes and derive outgoing call extents from expanded,
aligned callee ABIs. This strengthens the experimental proof without changing
source syntax, wire bytes or stable-admission blockers.

Extend immutable transaction `.data` reads to canonical bounded nested tables
of at most 32768 encoded bytes. Validate every field on each read and retain
nested pointer/length results in disjoint frame buffers across later reads,
selection and admitted read-only helpers. The independent checker binds full
codecs, coordinates, result spans and buffer lifetimes; coherent buffer-alias
mutations reject. Global/group VM fixtures cover large offsets, malformed unused
fields and fixed siblings. Stable source/interface and builder admission remain
pending, with the existing nested and data-view blockers retained.

Add an internal bounded nested-table selection primitive for #49. Validate every
field, including unused nested payloads, before extracting a fixed identity
field at its checked Molecule offset. Keep payload scratch disjoint from retained
selection tables and charge actual loaded bytes against the aggregate budget.
Mixed fixed/nested layouts, variable payload replacement and hostile peer groups
have primitive and real CKB transaction tests. Named/positional source catalogs
now derive `bounded-v2` for nested schemas, retaining `fixed-v1` for fixed-only
catalogs. Independently check ordered layouts, full codecs, dynamic frame
extents, normalized identities and actual-byte accounting. Selected coordinates
compose with live nested witnesses. Complete business examples, source/interface
proof and final builder admission remain pending. Selection and nested
proof-admission blockers remain mandatory.

Add experimental Edition 2027 `data_shape` declarations with reusable named
item, payload-byte and encoded-byte limits. Resolve witness/Cell field uses
to the existing concrete nested codec, preserving interface/builder identities
and wire bytes. Declaration errors, inferred view completion, WASM, native /
generated builder vectors and witness/successor VM parity have focused tests.
Require the nested proof-admission blocker in independently checked metadata;
removing it and rebinding sidecars now rejects. Shape exports and complete source
proof remain pending. Check forward declarations
and declaration-scoped generic resolution. Editor templates prefer named
catalog calls and offer both declarations only in the authoring source mode.

Allow immutable whole nested values, including named data shapes, as ordinary
parameters of Pure/ReadOnly helpers. Preserve bounds through aliases, forwarding,
and register/stack arguments; validate the complete codec on every callee entry.
The independent checker binds both pointer and length to the selected source;
substituting another valid value of the same type rejects. Byte/record iteration
and mixed Cell/value helper calls have focused VM tests.

Extend immutable helpers to derived byte and record views. `Shape::Item` and
`Shape::Payload` resolve to the declaration's element/payload view and bounds.
Each helper validates its received element encoding; the checker binds incoming
spills, outgoing spans and transitive forwarding to their selected sources.
Derived views remain forbidden at external witness/Cell boundaries and cannot
be returned, stored or mutated. Allocation and stable admission remain pending.

Add experimental Edition 2027 module-local `cell_roles` declarations with named
scan/byte limits, schema fields, authority and correspondence policies, and
inclusive count intervals. `ckb::verify_cell_roles<Catalog>()` elaborates
directly to the existing checked IR contract. Named selection supports
`selected.orders.input(i)`, output/successor reads, role counts and immutable
role aliases. Typed contracts retain catalog/role/schema identity through
projections; the independent checker binds pointer copies and read ordinals.
Catalog exports and complete source proof remain pending; stable artifact
admission stays blocked.

Permit immutable typed transaction input/output views in read-only helpers,
including transitive forwarding and local fixed-schema `.data` predicates.
Preserve nominal source/schema checks and report inherited helper coordinates
without claiming a fixed global/group source. Runtime, checker mutation,
editor and browser tests cover the experimental slice; complete call-site
source/value proof and stable data-view admission remain pending.
Independently bind view helper call targets, ordering, source-slot loads,
outgoing register/stack words and incoming parameter spills. Rebound offset
and spill substitutions reject; repeated/branched calls, wide arguments and
fixed-result returns retain coverage without changing the runtime ABI.

Share the complete capacity-field scanner with `ckb::cell_count` and add its
internal bounded end-probe form for #49. Preserve the existing unbounded API;
the bounded primitive is not yet source-level heterogeneous selection support.
Add an internal fixed-role selection/correspondence emitter with full identity,
layout, uniqueness, group-coordinate and budget checks. Differential VM and
all-group CKB transaction fixtures accompany an experimental literal-policy
`ckb::verify_cell_roles` source operation. Derive layouts from named Cell types,
retain complete runtime-access provenance and reject runtime-selected catalogs.
Check declaration mutations and retain the `exact-role-selection-artifact-proof`
blocker; complete machine/source proof, nested selection and
complete transaction builder admission remain pending.

Add experimental immutable `ckb::select_cell_roles` handles, role counts and
typed InputView/OutputView reads, including correspondence-selected successors.
Keep role-local ordinals distinct from global transaction indices and retain
separate tables for simultaneous catalogs. Reject handle storage, mutation,
escape and nominal-role mismatches. Independently check constructor pointer
materialization, alias and read instructions, slot/operand bindings, fatal
guards and successful-path order; reject hash-rebound substitutions. Exercise
ordinary and maximum catalogs in real CKB transactions at O0–O3, with invalid
ordinals and rejecting peers. Track constructed handles and retained tables
through enclosing CFG paths, protect them against ordinary and checked-helper
writes, and reject aliased physical handle slots. Complete aggregate/source
proof and transaction builder binding remain pending.
Compose selected views with live nested witness views by checking the complete
Cell-field hash helper and its disjoint caller result/size writes. Reject
rebound helper instructions and caller pointers redirected into nested storage.
Check the existing cold/hot exact-byte cache and scalar wrapper instructions,
initialization, retained pointer invariants and disjoint caller-owned write
regions. Prove saved-register restoration and bind borrowed cache effects
through helper calls and CFG joins. Retain actual emission order for typed
ownership while sorting serialized co-located boundary sets canonically,
including the nine-argument nested/fixed-result case.

Add experimental immutable `.data` projections from typed input/output views,
including selected and group views, for fixed 1–512 byte Cell schemas. Read
unsigned scalars and complete fixed bytes, checking exact whole-Cell length;
reject mutation, escape and unsupported layouts. Independently check field-load
instructions, source-domain guards, full-width copies, fatal exits and bounded
write effects. Compose reads with maximum selected catalogs and live nested
witnesses without increasing analysis budgets. Editor, formatter, source example,
real CKB execution and rebound-machine mutations cover this boundary. Retain
`cell-data-view-artifact-proof` pending complete source/value provenance and
transaction builder binding; observation grants no lifecycle authority.

Independently recognize fixed-role verification instructions from typed policy
and layout evidence. Check complete scans, exact identities, bounded records,
uniqueness, correspondence and fatal exits, including relaxed branches and
proved immediate reload forwarding. Bind candidate ranges to typed operations
and require successful typed-block paths to execute them in order. Reject
hash-rebound instruction and well-formed policy substitutions. Full enclosing
memory/source proofs and the existing experimental admission blocker remain.

Compose independently recognized fixed-role scratch effects with nested-view
lifetime checks. Reject coherently rebound scratch aliases into nested lengths
or live Cell buffers, and preserve witness/Cell views across verification.
Retain actual emission order for co-located typed labels so an empty fallthrough
block cannot capture the next block's nested reads. Exercise nested witness
views across repeated role scans in complete CKB transactions, including maximum
catalogs, substituted payloads and rejecting peers. This does not yet admit
variable-payload selection or complete transaction-builder binding.

Reject unadmitted dynamic-element vector length operations instead of treating
encoded byte size as element count. Preserve fixed-element vector semantics.
Add independent host validation, CKB-VM decoding and experimental immutable
`NestedValues<T, N, P, B>` witness and Cell views as prerequisites for #48.
Validate complete values before use, preserve wide fields and check indexes;
reject view mutation/escape and rehashed invalid typed operations. Derive
typed Cell buffers from declared byte bounds and check actual input/successor
encodings and expected output values. Generate accurate bounded nested Molecule
schemas and native/TypeScript value and complete Cell-data encoders with
independent vectors, budget rejection and existing-adapter capacity checks.
Independently check nested witness decoder instructions against typed bounds,
bind inputs to incoming ABI words through mandatory entry spills/reloads, and
reject hash-rebound instruction, argument and bypass mutations. Check the actual
single-entry nested witness envelope and positional argument transport, including
mixed Script.args, disjoint buffers, live cursors and outgoing stack arguments.
Check policy-selected nested arguments from the bounded private copy through
the exact action call, including positional lengths and outgoing stack words.
Bind nested length/index/field/move instructions to typed operations, scalar
slots, exact record offsets, fatal range checks and disjoint view-size slots.
Preserve co-located read and typed CFG labels in bounded optional evidence.
Check decoded witness definitions across CFG joins and reject unrelated writes
to nested pointer/length storage. Check private callee effects, bounded syscall
destinations and independently verified fixed copies. Track caller-owned fixed
struct results through actual hidden register/stack arguments, disjoint caller
buffers and transitive return copies; distinguish owned frames from outgoing
scratch. Retain explicit analysis budgets and reject unproved writes.
Check typed Cell load arguments and complete table codecs independently,
including source/index, disjoint bounded buffers, every fixed/nested field and
fatal exits. Reject missing codecs for unused fields and hash-rebound transport
or decoder mutations. Bind nested Cell field projections to explicit Molecule
field indices, bounded spans and disjoint view lengths. Preserve declaration
order independently of dynamic byte-offset sorting, include indices in layout
hashes, and check failed-load rejection and input/read-only pointer assignment.
Track direct Cell-root initialization through checked loads and CFG joins;
protect loaded buffers, root pointers and size slots against ordinary/callee
writes. Reject definition bypasses and full/partial storage clobbers while
retaining mixed witness/Cell reads. Bind read-only borrowed Cell helper calls to
their selected roots through actual register/stack arguments and transitive
forwarding; reject another validated Cell with the same layout. Derive private
frames from decoded prologues separately from outgoing argument space.
Bind nested successor byte comparisons to the selected output, declared field
index and exact typed expected value. Check span/length/byte-loop instructions
and require their completion on every successful CFG path, after the checked
Cell load and value definition. Check fixed sibling comparisons against typed
literal bytes or local slots: full unsigned scalar equality, complete u128/
Hash/Address/array bytes, and the distinct inline ABI of short byte-array
parameters and their aliases. Include fixed comparisons in successful-path
obligations. Fixed-value producer provenance, additional caller-owned write
effects and complete transaction builder parity remain pending. See
`docs/CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md` for the implementation ledger.

Freeze the first #49 exact-role fixed-layout selection/correspondence reference
contract and independent host oracle. Cover complete protected membership,
custody failures, explicit lifecycle shapes, role-local identity, global/group
coordinates, bounded proposals and equivalent transaction permutations. This
reference work does not admit new executable Cell selection syntax or replace
the required VM, checker, builder and all-group transaction evidence. See
`docs/CELLSCRIPT_BOUNDED_CELL_SELECTION.md`.

Add versioned bounded Script-instance admission alongside the existing exact
and compatible implementation policies. Authenticate concrete args, consumer
configuration and instance identity in CKB-VM and the independent artifact
checker; preserve old exact/open encodings. Native, CLI, generated TypeScript
and SDK paths agree on signed transaction bytes. Retain all-group document and
two-pool fixtures, independent codec vectors, rebound-machine mutations and
maximum/late-invalid resource observations. The technical acceptance and gate
provenance are recorded in `docs/CELLSCRIPT_INSTANCE_POLICY.md`; SSRI/IPC,
constrained spawned-verifier inputs, release and public deployment remain
separate. Serialize backend test fixtures consistently with CI.

Publish a controlled ZK cost study for ordinary/fixed-pair updates and complete
1/2/4-counter proof sets. Retain raw pilot/acceptance observations under separately
frozen host limits, authoritative all-group/per-VM accounting and explicit missing
heap metrics. Compare thread count and checked-key reuse, reject gzip proof
transport and slower thread defaults, and retain existing CLI/proof-system
defaults with concrete defer decisions for aggregation and additional profiles.
See `docs/CELLSCRIPT_ZK_COST_STUDY.md`.

Add the bounded `cellscript-zk-counter-set-v1` application for 1–4 independently
owned counter Cells. Derive complete ordered input/successor membership on chain,
preserve custody and require each unchanged exact counter proof over the common
transaction. Add the versioned Molecule set codec, independent checker, native
adapter/client and CCC consumer with generated leaf SDK parity. Focused tests
cover maximum bounds, all Script groups and local-node conflict/reprove recovery;
CI and gates were excluded by maintainer instruction. See
`docs/CELLSCRIPT_ZK_COUNTER_SET.md` for trust and execution scope.

Add bounded ZK lifecycle manifests and deterministic change reports in the native
counter client and independent TypeScript consumer. Separate source/prover,
statement/codec, circuit/VK, setup, verifier, deployment and package changes;
bind package upgrade receipts and existing deployment records; reject stale,
yanked and downgraded candidates. Checked fixed-pair migration identifies the
old-parent proof path, while immutable legacy counters retain an explicit
unsupported retrofit disposition. No new circuit, setup or on-chain ABI is
admitted. See `docs/CELLSCRIPT_ZK_LIFECYCLE.md` for evidence and bounds.

Complete compatible-open Lock/Type/verifier admission through independent public
receipts, immutable source policies and complete ProtocolBundle transactions.
Add machine-checked single-entry WitnessArgs/CSARG decoding and bounded
SPAWN/WAIT target/exit proofs. Lock groups derive from actual input Locks;
parameterless spawned verifiers inherit the consumer context and propagate
child rejection. Generated builders support Lock consumers and both nominal
handle classes. O0–O3 tests cover CLI/native reports, TypeScript/SDK byte parity,
all-group execution, missing peers, rebound machine code and unbound delegation.
The admitted codecs and fail-closed limits are specified in
`docs/CELLSCRIPT_OPEN_HANDLE_DESIGN.md`; external human review is waived, while
independent checking and CI/backend acceptance remain required.

Measure bundled-example metadata budgets with repository-relative source-unit
paths so checkout location cannot change the result. Preserve the existing
byte ceilings and the compiler's actual provenance records. Make the external
resolver test fixture consume its request before exiting, avoiding a race with
the parent process's pipe writes.

Fix compatible-open handle runtime bindings: keep the saved selection length
separate from the current-Script load length, and compare verifier dependency
data against the selected artifact hash. Restore the committed-args-root VM
regression and cover both verifier root sources at O0–O3. Advance the artifact
cache identity so earlier helper code is not reused. The maintainer cancelled
#28 and #29's external audit requirements; independent artifact checking and the remaining
technical acceptance criteria still apply. See `docs/CELLSCRIPT_OPEN_HANDLE_DESIGN.md`.

Bind both open handle classes to actual code bytes, exact raw OutPoints and
unique data2/Type identities in 1–64 direct CellDeps; dep groups fail closed.
Enforce exact receipt selection and admission sequence bounds on-chain.
Independently check bounded Type-ID creation/replacement histories and bind
private lineage proofs into native source policies and final dependency
snapshots. Preserve no-history record identities. Native admission composes these
proofs with the consumer and generated-builder boundary.

Add a bounded native ProtocolBundle admission path for private source policies,
one committed-args handle witness per Lock/Type consumer, and independently
checked Type-policy, single-entry Script or spawned-verifier participants. Compare the complete builder transaction with every concrete
skeleton field and input snapshot, then recheck byte identity in the existing
adapter serializer. Bind complete peer groups and optional typed input/output
observations to source declarations and fixed Cell schemas. The new
`ckb::require_open_script_participant_from_args` helper checks membership, code
and observed Script identity without granting foreign lifecycle authority.
O0–O3 integration tests execute every Script group, including a selected peer
that rejects despite valid admission. Reconstruct private source policies through
`cellc protocol bundle check --open-admission`, and provide generated TypeScript
selection-to-witness transport. Verify identical CLI/native reports, SDK bytes,
real secp256k1 signatures and post-signing rejection at O0–O3.

Normalize the pinned byte-context child ELF by removing its non-allocated
linker comment with the pinned `rust-objcopy`. The old fixture and fresh builds
normalize to identical bytes; update its fixture and example hashes together.

Separate ordinary commit checks from integration and release acceptance. The
default `commit` gate checks source/docs hygiene, changed shell syntax and
affected Rust package formatting/host compilation. `dev` remains a broader
integration checkpoint; CI, backend and release validation retain their scope.

Bind a checked direct data2 dependency and finite Type-policy receipt to complete
canonical transaction/witness bytes and every supplied input Cell. Derive Type
group indices from complete Scripts, validate all policy records and the
selected tag/cardinality/fixed args, and reject later witness/input substitution.
Compose private source/version proofs with this host snapshot. This component
alone does not establish resolution, signatures or root authority; see
`docs/DIRECT_TYPE_GROUP.md`.

Bind checked data2 code creation identities to canonical final raw direct deps
and every supplied Cell snapshot under finite preparse limits. Reject missing,
extra, copied or duplicate code, changed selected outputs, malformed unselected
Cells, dep groups and Type history without proof. Give source/version policies
and selections distinct native types and compose both proofs with source
rechecks. Supplied snapshots do not establish consensus resolution, root
permission or final witness/signature freezing.
See `docs/DIRECT_CODE_DEPENDENCY.md`.

Bind each frozen code candidate's actual source context, defining module and
manifest coordinate/edition/version to its finite artifact receipt. Add an
explicit source-policy constructor with resolver-owned required identity and
source receipt members: exact and compatible selections enforce the actual
baseline SemVer precedence floor; compatible selections require stable versions
and the same major. Preserve artifact-only policy v1 and advance the experimental
native catalog to v4. Manifest facts do not authenticate Registry releases or
authorize a root. See `docs/FROZEN_CODE_POLICY.md`.

Bind every member of a declared finite policy snapshot to actual resolver-owned
sources and private receipts. Check Header/API/network/runtime and complete
candidate/deployment fields before returning host selection evidence; reject
later source/input changes and Type-hash history without proof. This data2
binding alone does not authorize a root; see
`docs/FROZEN_CODE_POLICY.md`.

Compose independently checked finite Type-policy receipts from actual API,
codec, code-origin and target proofs; bind every original bundle file and reject
later input substitution. Require these private receipts for every frozen code
catalog member and advance the experimental native catalog to v3. Public
constants reject where values lack proof; generic declarations remain
non-executable. Policy authorization is checked separately; see
`docs/FIXED_POLICY_RECEIPT.md`.

Require every frozen code-catalog candidate's selected Script hash type to match
its independently checked target profile. Legal byte-bound data/data1 selections
reject for the VM2 `ckb` profile; preserve the separate byte-origin API and bind
actual target evidence in the experimental native catalog v2. This does not
prove chain activation or authorize Type replacements; see `docs/CODE_CELL_ORIGIN.md`.

Capture parsed, lock-pinned native package sources before consumer type checking
and reuse the defining-owner matcher against actual checked code catalogs.
Recheck unchanged real sources and reject upgrade-planning lock overrides;
unknown consumer types remain rejected. This does not introduce generic handles
or certify a consumer ELF; see `docs/FROZEN_PACKAGE_SOURCES.md`.

Bind the finite frozen code catalog's baseline to its actual defining owner in
a consumer's locked source closure. Dependency aliases preserve owner identity;
different owners, changed snapshots/versions and transitive origins reject.
Include the selected defining dependency closure in the owner hash. This host
ownership evidence does not introduce generic source handles or authorization;
see `docs/RESOLVED_CODE_CATALOG.md`.

Freeze native required/candidate source snapshots together with every checked
finite policy codec and actual code-output/Script byte origin. Check all 1–32
members under shared preparse byte ceilings and pinned chain identity; reject
duplicate concrete deployments and any invalid member. This host catalog does
not authorize immutable roots, Type history or nominal open handles; see
`docs/FROZEN_CODE_CATALOG.md`.

Add optional code Cell byte-origin checking from actual canonical CKB raw
transaction, selected output and complete Script bytes. Recompute transaction,
Script and data/Type identities; reject changed ELF bytes, malformed/unselected
outputs and bounded-set violations before constructing private evidence. This
does not authorize deployment/history or prove liveness/open admission; see
`docs/CODE_CELL_ORIGIN.md`.

Combine optional checked fixed policy codecs with actual public external
availability and scalar ABI reception. Reject pruned or helper-only public
callables, unsupported public layouts, changed witness-register spills and
retained generic instances without an external profile. Uninstantiated generic
declarations remain non-executable. This finite unit-result/flat-unsigned-Cell
profile grants no full receipt or open admission; see
`docs/FIXED_EXTERNAL_CODEC.md`.

Independently check direct unsigned Cell field materializations, receiver
dominance, protected caller storage and the membership helper's private-frame
memory boundary. Bound inline capacity/hash writes and fix missed RV64 word
register writes and stale caller/syscall register facts. Rebound byte/offset/
register changes and unsupported profiles reject. This component proves decoded
registers; receipts and admission compose it with the remaining private proofs. See `docs/FIXED_CELL_SCALAR_FIELDS.md`.

Bind eligible fixed Cell source parameters to their actual ABI spills and
post-read pointer receivers under a finite uncompressed scalar-slot profile.
Reject rebound source IDs, overlapping argument spills and changed receivers.
This component proves pointer reception; field decoding, receipts and admission
compose additional independent evidence. See
`docs/FIXED_CELL_PARAMETER_STORAGE.md`.

Add optional independent fixed Cell-data read gate evidence: actual source and
index, 512-byte buffer setup, checked terminating status and exact length.
Malformed machine gates and unsupported profiles reject. This partial evidence
does not certify field decoding, helper closure or compatible-open admission;
see `docs/FIXED_CELL_READ_GATES.md`.

Check required and all 1–32 candidate module bundles under a shared 16 MiB input
ceiling before parsing; reject incompatible or invalid unselected candidates and
freeze every exact four-file tuple. This is API coherence evidence, separate from
complete codec, deployment receipt and compatible-open policy admission. Native
catalogs own actual frozen source contexts and require matching pinned chain
identities; declaration comparisons preserve semantic order while normalizing
declaration identity order.

Add optional independent machine evidence for bounded fixed policy parameter
decoding, with exact length/magic, byte offsets, ABI registers and
signed extension checks. Unsupported profiles reject without weakening ordinary
inspection. This prerequisite grants no complete codec or open-handle admission;
see `docs/FIXED_POLICY_PARAMETER_DECODER.md`.

Synchronize local CKB acceptance submissions with the transaction pool's actual
chain snapshot before spending freshly generated funds and after commitment.
Bounded waits fail closed on stale/malformed tips and do not retry rejected
transactions. Script semantics and existing resource ceilings are unchanged.

Make bounded source declaration catalogs explicit through native
`CompileOptions::source_contracts` and `cellc build --source-contracts`.
Ordinary builds retain existing metadata size budgets. Build plans and cache
entries distinguish the evidence mode; absent catalogs still reject open module
projection. The option grants no compatibility or deployment admission.

Retain bounded source declaration catalogs before optimization, including absent
public templates and pruned public functions. Bind ordinary imported layouts to
their qualified declaration owners and independently reject inconsistent scopes,
field order and alias substitutions. Add a separate symbolic type/ability check
using parameter minima rather than observed instances. This is a prerequisite
for compatible-open receipts; receipt admission and on-chain open handles remain
pending. Historical bundles remain inspectable without granting open admission.

Derive a bounded private checked module projection from actual bundle bytes.
Preserve qualified nested nominal layouts, symbolic binders, declared versus
inferred effects, fixed Cell bindings and external dispatch tags. Directional
matching permits candidate additions while conservatively preserving required
contracts. Artifact identities remain separate from API identities; source
equivalence, codec, package and deployment admission are not inferred.

Construct a private native frozen-source context from actual locked package
sources, module owners and pinned chain identity, then independently inspect the
immutable bundle. Reject missing/stale locks and statically oversized or linked
source trees without repinning. This prerequisite supplies source provenance;
complete compatible receipts and open runtime admission remain pending.

Bind fixed ordinary-struct helper results to a feature-specific caller-owned
buffer ABI checked independently from decoded RISC-V, including source pointer,
hidden argument placement, bounded copy and frame teardown. Preserve nominal
types and declaration field order through nested calls and zero-width fields.
Propagate typed local Vec constructor context through typing and IR, and retain
valid annotations during diagnostic recovery. Add the
[52-byte encoding example](examples/bounded-byte-context/README.md) with a pinned
real Rust child, byte oracle, O0–O3 CKB-VM replay and separate resource ceilings.
Cell, enum/dynamic helper results and allocation-backed collections retain
their existing boundaries.

Add the experimental `zk::require_valid` exact Groth16 transition interface,
nominal proof/VK types, transaction-derived v2 statements and bounded parent/child
transport. Bind metadata, ProofPlan and machine checks to the same exact handle,
VK and raw transaction; expose builder and browser metadata plus LSP completion.
Real scheduler tests cover successor transitions and rejected replay/substitution.
The public-seed fixture circuit is non-authorizing; production application
admission is scoped to the private counter under explicit local single-party
setup trust. Named package resolution, shared Rust/TypeScript codecs and
separate stack-resource replay complete the first profile acceptance surface. See the [composition boundary](contracts/zk-transition-verifier/README.md).

Add a runnable counter walkthrough and a TypeScript example importing its
generated SDK. Fix generated packages crashing on import when their Molecule
schema list is empty; exercise the complete package in a Node regression test.
Add a native counter client, checked manifest export and a CCC adapter that
resolves Cells, completes fees before proving, preserves proof witnesses during
signing, dry-runs and awaits commitment. Consolidate the DX assessment into the
[developer guide](examples/zk/README.md); retain the profile and acceptance records.

Add a diagnostic execution-trace contract that replays the pinned CKB scheduler,
checks exact exit/cycle agreement with the existing oracle, and records separate
observed stack peaks for each VM and EXEC generation. Preserve static bounds
and unavailable outcomes as distinct categories. Add isolated dispatch and
copy experiments; these do not change the production emitter's routing or
witness ownership. Broader acceptance remains tracked in the
[0.32 implementation record](docs/CELLSCRIPT_0_32_IMPLEMENTATION.md).

## 0.31.0 - Cost optimization (2026-10-01)

Implement deterministic RV64 immediate plans, shared policy decoders behind
direct action stubs, bounded private adapter frames, and CFG-liveness scalar
slot reuse. Keep witness formats and the outer 4,096-byte loading boundary.
Lowering record v9 and the independent checker bind the changed machine shapes.

Cost-report v2 adds measured nonzero group exits, explicit unavailable results,
static call-chain bounds, expanded policy/scalar sweeps, and multi-Script
accounting with frozen per-metric ceilings. See the
[implementation and acceptance report](docs/CELLSCRIPT_0_31_COST_IMPLEMENTATION.md).
Publication is authorized with a release-specific waiver of the pending review
and evidence records; see the [release notes](docs/releases/CELLSCRIPT_0_31_RELEASE_NOTES.md).

Reject source-byte loop fusion when discarded loop-prefix instructions define
an offset or memory pointer. Propagate immediate-planner errors through assembly
layout. The self-hosted Registry derives client identity from the socket peer
and an explicit trusted-proxy count, discarding caller-supplied IP/ASN headers.
These corrections originate in `9c441094` on `arthur/fix-audit-bugs`.

## 0.30.0 - Capability closure

The entries below retain the evidence state at each implementation step. The
release notes and capability ledger record the current acceptance state.

- Correct runtime-error sidecars so generated label ordinals are not mistaken
  for exit codes. Validate large policy dispatchers after conditional-branch
  relaxation, with mutations that reject changed conditions, skip distances,
  and call destinations. Generated contract instructions remain unchanged.
  Run the Registry PostgreSQL suite in the release workflow and require the
  latest migration and its interface lookup index.

- Make cost evidence mandatory in CI/backend and both release gates: missing
  RISC-V target libraries or the selected Rust toolchain's `llvm-strip` fail
  instead of silently skipping. Require a fresh execution report, independent
  byte/cycle ceilings for the three matched Rust samples, and bounded growth
  checks for action count, fixed-byte witness width, and input-group size.

- Record the selected-network deployment on Pudge testnet without creating a
  release tag or invoking GitHub release automation. Two confirmed
  transactions keep twelve immutable `data2` code Cells live: the seven strict
  business contracts and Registry Type Script in
  `0x49572cfd3258b49ff214c0dc5cd60388826e7f989acf904a9016c4cfbcb2a562`,
  plus the bounded GroupInput/GroupOutput verifiers and exact NovaSeal
  parent/BIP340-child boundary in
  `0x24273eb1f62a6a34a629f8306aa0c5a1434e47365538115002d2aec7563e54e9`.
  The checked-in manifest and report bind Pudge's genesis, the clean artifact
  source commit, every ELF SHA-256/CKB data hash, output index, occupied
  capacity, deployment authority, dry-run result, block identity, and minimum
  confirmation policy. Offline Rust validation and the CCC-based online
  verifier reject scope, transaction, Cell liveness, lock, and byte-identity
  drift. Publication remains separate and must not bypass the maintainer's
  no-TAC constraint.

- Complete the repository-local 0.30 release replay after updating NovaSeal's
  parent-lock ABI preflight to require the compiler's exact `GroupInput#0`
  binding. Record the unified release, repository, cryptographic, and business
  gates as passed while leaving selected-network deployment and publication
  explicitly unauthorized and incomplete. Refresh the Registry API's Wrangler,
  Vitest, esbuild, and Workers type toolchain, removing all moderate/high
  advisories while retaining only the JoyID/CKB low-severity `elliptic` chain.

- Re-accept retrievable release-evidence pins for the VS Code extension,
  website, NovaSeal, and iCKB comparison corpus after the recorded commits
  disappeared from their configured remotes. Refresh the editor and website
  dependency graphs, eliminating all editor and all moderate-or-higher website
  npm advisories without taking npm's unsafe CKB connector downgrade. Make Git
  fixture tests independent of the developer's global commit-signing setting,
  and retain explicit-source wrappers for repository-wide scenario
  discovery while refreshing their exact sidecar and iCKB differential
  identities. Rebuild the canonical playground compiler twice to the identical
  SHA-256 `82e76c029af7647fa206684e023fc95abdfd8e15ea6ce12e8993edb75da91b3b`
  within the 600 KB gzip budget.

- Complete issue #26's external-verifier family with exact EXEC, SPAWN/WAIT,
  and verifier-handle CKB-VM fixtures. Seven independently checked parent
  artifacts and one pinned argument-checking child cover two accepted rows and
  dependency-position, hash, adapter, argument, scope/receipt, semantic-
  statement, child-exit, and post-build substitution failures.

- Complete issue #26's NFT/DOB scenario family with a deterministic signed
  policy lifecycle for unique mint, derived metadata-state update, ownership
  transfer, and burn, plus an independently checked typed-view capacity
  adjustment artifact. Pin five accepted and five rejected CKB transactions,
  including duplicate output, stale state, wrong Lock/Type, and missing owner
  authorization, to exact artifact and sidecar identities.

- Add an exact persistent-policy runtime-view resource profile. The stateful
  `partial_fill`/`settle`/`cancel` path now gates its canonical policy artifact
  and sidecars, six transaction identities, 17,341 maximum cycles, 7,040-byte
  ELF, 5,376-byte stack frame, 145 witness bytes, 493 transaction bytes, and
  14,224 dependency bytes. Fixed domain-separated input OutPoints make the
  measurement stable under parallel test execution. The frozen corpus also
  explicitly excludes full-header hashing and an in-language multisig-prefix
  signing transform: it uses typed header fields and the exact standard
  multisig Lock boundary instead.

- Split the deliberately deferred portions of the implemented #10/#11
  contracts into post-v1 issues #29 and #28 respectively. The 0.30 boundary
  remains exact closed cross-Script roles plus exact fixed-width Script,
  verifier, and deployment-line handles; runtime-selected open roles and
  generic compatible-open handles cannot silently expand that release scope.

- Pin the raw and serialized CKB transaction identities for all five rejected
  composition-anchor mutations and verify every case in one exhaustive test.
  Bind the four inventory-mapped substitutions—authorization, Type state,
  output, and CellDep—and exact ProtocolBundle materialization to their four
  artifact identities in the scenario-evidence manifest. Complete exact-
  artifact coverage for the multi-Script family by replacing nondeterministic
  state input allocation with fixed fixture OutPoints, pinning the
  `partial_fill`, successor `settle`, and independent `cancel` transactions,
  and exercising a `PB200` exclusive-role conflict that the adapter rejects
  before materialization.

- Exercise typed `GroupInput` `data_size` and `capacity` reads inside every
  action of the persistent `partial_fill`/`settle`/`cancel` policy. The same
  policy artifact now passes direct action dispatch, exact generated-builder
  and ProtocolBundle materialization, the four-artifact composition anchor,
  and prior-output stateful CKB-VM execution. Refresh its four sidecar
  identities, transaction and ProtocolBundle hashes, and measured 41,822-cycle,
  16,424-byte combined-ELF profile under an explicit 17,000-byte ceiling.

- Extend issue #24's exact non-cryptographic runtime-view resource evidence to
  global Output, current-group GroupOutput, and complete Lock/Type Script
  projections. The CKB-VM fixture covers absent Type Scripts, exact 32-byte
  args, maximum 459-byte Script args, and one-past-last indexes while pinning
  artifact, sidecar, raw-transaction, and serialized-transaction identities.
  The standalone checker now accepts only the closed Cell-view to internal
  Script-reference lowering used by these public projections.

- Add a fourth exact issue #24 resource profile for complete Input, GroupInput,
  CellDep, OutPoint, Since, Lock/Type Script, and fixed WitnessArgs projections.
  Pin its artifact/sidecar and raw/serialized transaction identities and teach
  the standalone checker the closed `InputView<T>` to internal OutPoint
  reference boundary without admitting Output, CellDep, raw Source, or integer
  retyping.

- Close issue #24's repository-local persistent-policy and generated-builder
  parity with a fifth exact profile. A canonical `PolicyWitnessV1` bundle drives
  a complete runtime-view Type-group action and pins the independently checked
  artifact, sidecars, transaction identities, and bounded resource measurements.

- Add the bounded fixed-width committed-substate profile for issue #13.
  `Commitment<T>` is a nominal 32-byte CKB Blake2b-256 value over the existing
  `CellScriptPackedHashV0` domain; `Opening<T>` is an explicit, linear,
  witness-only value that can be consumed exactly once by
  `commitment::open`. The compiler rejects aliases, ordinary-parameter
  injection, type mismatch, storage, return, nesting, Cell-backed/dynamic
  payloads, and scratch-overflow shapes. Generated code hashes and compares
  before materializing `T`, returning stable error 73 on mismatch. Metadata,
  generated builders, checked-runtime ProofPlan records, typed-checker
  mutations, formatter/LSP paths, output-field correspondence, golden vectors,
  and real CKB-VM pass/tamper tests share the same fixed-width contract. An
  exact maximum-shape profile fills the 512-byte scratch contract and gates
  cycles, ELF, stack, witness, transaction, dependency, artifact, sidecar, and
  transaction identities. The standalone checker binds and mutation-tests the
  inline header/copy/hash/compare/materialize/recommit machine sequence.
  Independent review and release acceptance remain pending.

- Complete issue #26's committed-state scenario family with exact CKB-VM
  successor, stale, malformed, wrong-root, wrong-index, and wrong-successor
  transaction fixtures. The standalone checker now binds a Commitment field's
  machine pointer to its typed root stack slot and exact schema offset and
  rejects offset mutation before authentication.

- Complete issue #26's temporal scenario family with four positive and five
  adversarial typed-Since/HeaderDep transactions. Pin one checked artifact and
  all sidecar, raw-transaction, and serialized-transaction identities while
  preserving stable errors for cross-domain, missing/one-past HeaderDep,
  early-release, and epoch-overflow failures.

- Complete issue #26's fungible scenario family with an independently checked
  five-action policy and exact signed transactions for mint, transfer, bounded
  split, bounded merge, and burn. Make lifecycle genesis OutPoints
  deterministic and pin replay, cardinality, amount, overflow, issuer, and
  multisig-authority rejections to their exact transaction identities.

- Complete issue #26's authorization scenario family with exact standard
  multisig-v2, threshold, issuer, and policy-Script identity evidence. Pin
  post-signing mutation, copied-owner, wrong-domain, wrong-key, partial-signature,
  and live-set replay failures to their exact signed transaction identities.

- Complete issue #26's order/AMM scenario family. Reuse the exact anchor for
  partial fill, cancel, settlement, partial-settlement rejection, and
  authenticated dependency substitution; bind pool merge to the matched
  CellScript/Rust artifact; reject replay at the local live-set boundary; and
  reject wrong price and output reordering in a focused CKB-VM AMM artifact.
  Keep the unreachable pinned iCKB reference as a reproducibility blocker.

- Record the exact clean-replay boundary after rechecking every 0.30 gitlink:
  the pinned VS Code, website, NovaSeal, and iCKB commits are no longer
  available from their configured GitHub repositories, while the pinned DOB
  commit remains retrievable. Do not replace the missing identities with
  current remote heads; they require explicit restoration or re-acceptance.

- Add the versioned 0.30 product-capability ledger for issues #7 through #27
  and validate its claims, evidence links, dispositions, dependencies, and
  release gates independently from executable-surface closure. Preserve
  generated assembly coordinates in internal assembler diagnostics and expose
  them under a distinct JSON `generated_assembly` object instead of presenting
  them as CellScript source spans.

- Add per-scenario business evidence grades and exact manifest hashing. The
  record distinguishes owner/test inventory, partial transaction hashes, and
  exact artifact fixtures, and downgrades unsupported family-layer claims. It
  also records that the two parent-pinned external corpus commits are no longer
  fetchable from their configured remotes; current remote heads are not used as
  substitutes for frozen evidence. The committed-state family now has its
  first exact-artifact authenticated-opening row, and the shared CKB harness
  uses domain-separated deterministic fixture OutPoints so transaction hashes
  remain reproducible across processes.

- Promote the four-artifact/five-group business anchor's positive settlement
  row to an exact-artifact fixture. Its canonical record now pins every ELF,
  lowering record, source map, and verified-bundle identity in addition to the
  executed raw transaction, serialized transaction, ProtocolBundle, and
  resource measurements.

- Preserve the CKB OutPoint index as `u32` through builtin typing, stdlib IR,
  direct/property lowering, metadata, and independently verified lowering.
  This fixes current NovaSeal state Type compilation and aligns its action and
  Lock harness witnesses with the canonical `WitnessArgs.input_type` envelope.

- Freeze a machine-validated cryptographic and authorization matrix for the
  eight-family business corpus. Ten admitted capabilities and twelve value
  domains now distinguish native hashing and Script identity, bounded signing
  messages, exact standard Locks, exact Script handles, committed openings,
  and trusted external verification. `check-business-corpus` requires the
  complete matrix and content-binds all cited CKB-VM, checker, measurement, and
  dependency evidence while retaining maximum-bound measurements, deployment,
  release, and independent review as explicit candidate requirements.

- Freeze the candidate eight-family business corpus and execute its canonical
  settlement anchor with four independently checked CellScript artifacts and
  five actual Script groups. The anchor combines fungible conservation, Lock
  authorization, two bounded GroupInput orders, exact GroupOutput Plan
  correspondence, a persistent `partial_fill`/`settle`/`cancel` policy, and an
  authenticated CellDep. ProtocolBundle now hash-binds the bounded Plan
  evidence, projects concrete witness bytes, Script hashes, and capacities into
  per-artifact validation, and materializes the exact Molecule bytes executed
  in CKB-VM. The fixture pins transaction and bundle hashes plus cycle, ELF,
  stack, witness, transaction-size, and occupied-capacity budgets. Network
  admission, deployment, and independent review remain release-candidate
  evidence.

- Complete issue #8 for the admitted bounded output plan contract. Native
  `create_each` accepts a fixed-width witness `BoundedList<Plan, N>` when one
  complete output template maps every data field and the Lock directly from
  the same Plan element and the resource declares a positive capacity floor.
  `CSBPLv1` order is bound one-to-one to the current Type Script's canonical
  `GroupOutput` array; missing, extra, malformed, foreign, field/Lock/Type,
  capacity, and predicate failures terminate with stable errors. Metadata
  schema 72 exposes the complete mapping. Generated TypeScript builders,
  `cellc tx validate`, and the CKB adapter independently materialize or verify
  the exact witness and concrete outputs. The standalone artifact checker binds
  the typed contract to decoder, syscall, comparison, ordinal, and error paths;
  rebound mutations fail with V2420. The shared corpus covers 14 semantic and
  adversarial cases, and a separate 101-output fixture guards the 4,084-byte
  Plan payload boundary. Clean-source live CKB acceptance passed all 14 cases
  on 2026-09-09, and full production-evidence validation binds that report to
  the current fixture and unique deployed ELF. The required `dev`, `ci`, and
  `backend` gates passed at `00f39a16`. Computed and dynamic templates remain
  fail-closed.

- Complete issue #7 for the accepted bounded Type-group input contract. Native
  `input BoundedCellSet<T, N>` authoring selects the current complete Type
  Script's canonical `GroupInput` array, accepts `0..=N` fixed-width resources,
  executes each pure predicate and numeric accumulator update exactly once, and
  discharges every selected linear input. The simulator and CKB-VM share a
  ten-case corpus; a separate 1024-element fixture guards ELF size and cycles.
  The standalone checker binds syscall ordinals, buffers, lengths, complete
  Type/Lock hashes, success/absent results, and predicate machine evidence;
  rebound mutations fail with V2420. Live CKB acceptance passed all ten cases
  on 2026-09-08, and production-evidence validation binds that report to the
  current fixture and unique deployed ELF. The required `dev`, `ci`, and
  `backend` gates passed on 2026-09-09 at `43e58780`. Dynamic outputs remain
  under issue #8.

- Stabilize public value generics under issue #23 Proposal A. The new
  `fixed_value` profile normalizes the six ordinary fixed-value abilities,
  generic aggregates derive omitted abilities from their fields, public
  cross-package templates retain bounded monomorphization, and equivalent
  spellings produce the same canonical interface hash. `cellc interface`
  defaults to a concise human view while `--json` and `--output` retain the
  complete record; compatibility analysis, Registry admission, the independent
  artifact checker, LSP, VS Code, and the browser metadata compiler validate the
  same generic parameter contract. The required `dev`, `ci`, and `backend`
  gates, including clean-source CKB production/stateful acceptance, passed on
  2026-09-08 at `69488316`.

- Add transactional package and workspace upgrades through
  `cellscript-upgrade-plan-v1`. `cellc update-plan` and the default `cellc
  update` resolve candidates in memory, preserve unrelated package-scoped
  nodes, diff exact nodes and edges, compile every reverse dependent, and keep
  source API, serialized layout, runtime ABI, effects/capabilities, builder,
  deployment, ProtocolBundle inputs, and upgrade authorization as separate
  evidence. Only `cellc update --apply-plan` mutates locks after plan/compiler
  hash, exact old bytes, canonical candidate, confined path, hard-blocker, and
  acknowledgement checks. Per-file synced renames and runtime rollback cover
  multi-member apply; the command never edits `Deployed.toml`, deploys,
  publishes, signs, or performs state migration. CLI/LSP/VS Code, v1 fixtures,
  package-scoped and stale/tamper negative tests, and dev/CI schema checks share
  the same contract.

- Add negotiated, read-only `cellscript-resolve-graph-v1` and
  `cellscript-build-plan-v1` inspection through `cellc resolve-graph` and
  `cellc build-plan`. The schemas retain roots, aliases, edge provenance,
  feature/test/environment selection, immutable source and lock identity,
  stale nodes, compatibility/VM/codec axes, expected outputs, and stable cache
  reasons. Builds validate the same unit IDs; package audit, LSP, VS Code, dev,
  and CI consume the shared contract. Existing `cellc metadata` keeps its
  compiled-program meaning.

- Replace declaration-order workspace member loops and synthetic root locks
  with `cellscript-workspace-resolve-graph-v1`. Explicit members/exclusions,
  canonical path and package-name uniqueness, independently authoritative
  member locks, cross-root package-instance consistency, cycle rejection,
  dependency-first ordering, `-p` transitive closure, and dependency failure
  propagation are enforced before compilation. Successful non-frozen builds
  refresh member-local `package_build` identities only after the selected graph
  and every build succeed. Virtual roots reject `Cell.lock`; artifact hashes are
  no longer disguised as dependency nodes. Dev and CI run the checked-in
  reverse-declared workspace diamond frozen and offline.

- Enforce `[package].cellscript_version` as a real compiler SemVer
  requirement before source loading. Legacy omission remains `*`, historical
  bare versions mean a minimum, and malformed or incompatible requirements
  fail with stable `E2600`. Dependency preflight aggregates every discovered
  incompatible path/Git/Registry package and incoming edge in machine JSON;
  Registry selection deterministically chooses the newest compiler-compatible
  package version while keeping the exact build compiler as separate evidence.
  Carry the compiler requirement evidence into `Cell.lock` version 5 and
  `cellscript-lock-v0.30-single-package-coordinate-v1`: the root, every dependency,
  and canonical node identity bind the declared requirement and resolving
  compiler release. Frozen builds revalidate without selecting replacements,
  while explicit `cellc lock`/`update` may repin version 1-4 locks.

- Define the package coordinate as `(declared namespace, declared package
  name)` and enforce one selected version/source/feature/environment instance
  per coordinate in each runtime or test graph. Compatible Registry diamond
  edges reuse the selected candidate; incompatible ranges, implicit
  Path/Git/Registry substitutions, divergent exact feature roots, and
  environment mismatches fail during resolution with structured `E2601`
  incoming-edge evidence. Locked and frozen materialization applies the same
  fail-closed rule and discards partial graphs. `Cell.lock` v5 declares
  `resolver_model = "single-package-coordinate-v1"`; multi-version graphs
  require a future lock and package-qualified module-identity contract.

- Rebind the CKB acceptance transaction recipes to the typed timelock
  contract. `request_release`, `execute_release`, and `can_unlock_lock` now
  supply canonical absolute-epoch `since` values for the selected `TimeLock`;
  the stateful release path carries epoch 11 from the created lock. A focused
  fixture assertion prevents these inputs from silently reverting to the
  pre-typed all-zero block-number encoding, which the current machine contract
  correctly rejects with `ckb-since-malformed` (37).

- Rebind the HeaderDep-authenticated multisig recipes to epoch 11. Every
  action and lock that compares `reported_time` with `HeaderDep.epoch_number`
  now carries that HeaderDep and matching witness value; created wallet,
  proposal, confirmation, and execution-record data use the same epoch. The
  stateful create/propose/approve/execute chain therefore preserves one
  authenticated time domain across its live Cells.

- Add focused schema-change acknowledgements for Edition 2027
  `data = same except` relations. `cellc schema-ack` emits a canonical plan,
  blocks a newly added implicitly preserved field with `SACK1001`, creates a
  receipt only after the field receives an explicit policy and the reviewer
  supplies an identity and rationale, and rejects tampered or stale receipts.
  Plan and receipt hashes bind the module, selected action/roles, old/new
  interface and field-layout identities, canonical relations, and changed
  field treatments while ignoring formatting changes. The decisive
  `approval_nonce` fixture now proves that an explicit constant reset remains
  executable under `DenyFailClosed`; ProofPlan recognizes fixed constants as
  verifier-checked successor assignments. Every schema delta still reports a
  separate state-migration requirement, and the workflow does not mutate
  `Cell.lock`, `Deployed.toml`, or deployment authorization. Graph-wide
  transactional upgrade plans now consume the graph-wide evidence under issue
  #20 while keeping the focused acknowledgement receipt independently useful.

- Introduce the first authoring-level CKB Script identity contract. Edition
  2027 successor relations now accept `lock = exact_hash(script_hash)` and
  require the expression to have the dedicated `ScriptHash` type. Typed CKB
  transaction views retain that type for complete Lock/Type Script hashes;
  `ckb::script_hash(hash)` is the explicit conversion from a trusted raw
  `Hash`. The conversion is a type-domain assertion and does not prove that a
  Script exists, is deployed, or authorizes the transaction. Parser, AST,
  typing, formatter, LSP completion, IR lowering, optimizer traversal, syntax
  audit, and real CKB-VM positive/substitution-negative tests share the same
  boundary. The existing `lock = exact(address)` form and preview4's legacy
  `Address`-typed `exact_hash` surface remain compatible. Advance the
  experimental Edition 2027 source identity to
  `cellscript-source-semantics-2027-0.30-dev1` and the artifact cache identity
  to `project-source-set-v31-0.30-dev1-script-hash`; the final 0.30 source
  identity remains gated on the complete adopted grammar.

- Establish `cellscript-ckb-runtime-view-v1` in compile metadata schema 68.
  Typed Cell views add occupied/unoccupied capacity, consensus data hash, and
  input `since`; typed HeaderDep views add the three CKB
  `LOAD_HEADER_BY_FIELD` epoch fields with exact 8-byte reads and stable errors;
  and `ScriptView` now keeps complete Script hash separate from raw code/args
  hashes. Real CKB-VM tests cover nonzero epoch values, derived epoch-start
  block number, one-past-last HeaderDep rejection, and CellDep data-hash
  substitution. Advance the artifact cache identity to
  `project-source-set-v32-0.30-dev1-runtime-view-v1`.

- Complete the RFC0017 mode/metric product with distinct absolute and relative
  block-number, epoch-fraction, and timestamp `Since` types. Add checked
  block/timestamp constructors, `DecodedSince`, strict decoding from opaque
  input or explicit raw bits, six mode/metric narrowing operations, and typed
  flag/metric/value projections. Decoding rejects reserved bits, metric `11`,
  malformed epoch fractions, and timestamp values whose consensus
  seconds-to-milliseconds conversion overflows. CKB-VM tests bind exact vectors
  for all six domains and stable error 37 for bad flags, scalar bounds,
  timestamp overflow, and mismatched narrowing; checker mutations reject both
  changed mode and changed metric. Advance the artifact cache identity to
  `project-source-set-v34-0.30-dev1-since-domains` and bind target/deployment
  metadata to `since_abi = ckb-since-rfc0017-typed-v1`. Header
  timestamp/full-block readers, checked duration arithmetic, migration, and the
  complete business corpus remain open under issue #12.

- Add checked `EpochDuration` arithmetic to the typed temporal contract.
  `ckb::epoch_duration` validates the CKB 24-bit epoch-number domain;
  `ckb::epoch_add` rejects overflow and `ckb::epoch_sub` rejects underflow;
  `ckb::epoch_duration_to_u64` is the explicit representation escape hatch.
  The dedicated type cannot be mixed with `EpochNumber` or raw integers, and
  the three checked operations remain named in IR, typed semantics, runtime
  access metadata, and CKB-VM failure evidence. Invalid arithmetic uses stable
  error 20, `numeric-or-discriminant-invalid`. Advance the artifact cache
  identity to `project-source-set-v35-0.30-dev1-epoch-duration`.

- Complete the typed HeaderDep temporal readers that CKB does not expose via
  `LOAD_HEADER_BY_FIELD`. `HeaderDepView.block_number` returns `BlockNumber`
  and `HeaderDepView.timestamp` returns the distinct `TimestampMillis` domain.
  The runtime loads the fixed 208-byte Molecule `Header`, requires the exact
  size, and reads the official RawHeader offsets 16 and 8 respectively;
  missing headers retain error 45 and malformed lengths use error 4. Tests bind
  those constants to `ckb-types`, execute nonzero values in CKB-VM, and keep
  milliseconds distinct from timestamp-Since seconds. Advance the artifact
  cache identity to `project-source-set-v36-0.30-dev1-full-header-time`.

- Add targeted `W3012` warnings and a language-server workspace edit for every
  legacy raw temporal call with a total typed-domain replacement. The edit
  preserves comments and the surrounding `u64` result; the untyped GroupInput#0
  reader migrates to the explicitly named `ckb::input_since_raw()` alias. Move
  canonical package interfaces to `cellscript-package-interface-v3`, where the
  fixed representation, RFC0017 constructor/decoder set, domain inventory, and
  migration identity are hash-bound and runtime-ABI changes are classified as
  breaking. The Registry API continues to read v2 and validates the complete
  v3 temporal contract. Advance the artifact cache identity to
  `project-source-set-v37-0.30-dev1-temporal-migration-interface`.

- Complete the issue #12 implementation surface across formatter, VS Code,
  generated TypeScript builders, locked package fixtures, metadata-only WASM,
  and the website Playground. Migrate timelock, DAO, vesting, NFT-expiry,
  governance, and atomic-swap examples to typed HeaderDep, `Since`, and checked
  epoch-duration operations. Split browser metadata construction from the full
  native evidence path so the canonical Rust 1.97.1 / wasm-bindgen 0.2.121 /
  Binaryen 131 build retains its bounded summary at 543,507 bytes gzip. Advance
  the artifact cache identity to
  `project-source-set-v38-0.30-dev1-temporal-product`.

- Advance compile metadata to schema 69 and add
  `cellscript-ckb-runtime-access-provenance-v1`. Every CKB runtime access now
  records its resolved source, source origin, static/dynamic/bounded index,
  optional maximum, and fixed/whole/bounded byte range. The legacy numeric
  `index` remains a compatibility projection and is zero for dynamic accesses;
  the structured record is authoritative. Runtime source-view constructors
  accept dynamic `u64` parameters but terminate with stable error 44 when a
  value exceeds the packed 32-bit source-index domain. Generated TypeScript
  builders preserve the same records and reject out-of-domain parameters. The
  standalone artifact checker independently rejects source, index, range,
  contract, and module/entry projection mutations even after outer sidecar
  hashes are rebound. The canonical browser summary is 544,037 bytes gzip with
  SHA-256 `f0128b364ca624506ddf78639ff2be8850ec88424f19707f72fc009a3536407a`.
  Advance the artifact cache identity to
  `project-source-set-v39-0.30-dev1-runtime-provenance`.

- Add bounded, read-only variable-length witness views with explicit field
  ownership. `witness::bounded_raw`, `bounded_lock`, `bounded_entry`, and
  `bounded_output_type` produce `WitnessBytesView<owner,max>` values whose
  maximum is a compile-time literal in `0..=65536`. The views expose exact
  `.size`, byte/u32/u64 reads, and full-view streaming CKB Blake2b without a
  witness-sized allocation. `bounded_entry` names the one
  `WitnessArgs.input_type` field shared by the `CSARGv1` entry ABI and bounded
  plan/authorization consumers; it does not create a second payload. Missing
  fields and values above their declared bound use stable errors 67 and 68.
  Typed `WitnessArgsView.lock/input_type/output_type` now require and stream an
  exact 32-byte field regardless of total WitnessArgs or sibling-field size,
  while the legacy direct helpers retain their historical zero-pad/truncate
  behavior. CKB-VM tests cover all four owners, fields larger
  than the old 512-byte helper buffer, `Some(empty)` versus absent fields,
  malformed Molecule tables, range failures, GroupOutput provenance, and
  streaming hashes. Metadata schema 70 binds owner, maximum, source, and byte
  range; compiler metadata validation, including the metadata-only WASM path,
  and the standalone checker reject their mutation after outer hashes are
  rebound. The rebuilt browser bundle is 554,564 bytes gzip with SHA-256
  `e57adc617c36a7946c706d4b4e420d3463d4e97249f343ace84a56fe0df79a39`,
  within the 600 KiB budget. Advance the artifact cache identity to
  `project-source-set-v40-0.30-dev1-bounded-witness`.

- Add `ckb::transaction_hash() -> Hash` as the canonical fixed-width
  transaction identity primitive. The CKB backend calls `LOAD_TX_HASH`,
  requires success and an exact 32-byte result, and preserves the access as
  `transaction-hash` / `Transaction` provenance in metadata schema 70.
  Metadata validation and the standalone checker bind the operation, syscall,
  source, authoring name, implicit index, and exact range; rebound mutations
  cannot relabel it as a generic transaction read. Real CKB-VM evidence proves
  that a nonzero hash is available in an ordinary Type Script transaction, and
  the syntax audit, LSP, example, generated metadata path, and WASM tests share
  the same surface. This primitive is the raw transaction-hash prefix required
  by canonical signing domains; it does not by itself implement
  `env::sighash_all`, which remains fail-closed until group-witness ownership,
  first-witness lock replacement, extra-witness inclusion, and bounded message
  construction are explicit. The rebuilt browser bundle is 554,741 bytes gzip
  with SHA-256
  `7884304db93b50abadb8e7ca082d23afe60ee2d5d73886fd5f3572a4f525d179`,
  within the 600 KiB budget. Advance the artifact cache identity to
  `project-source-set-v41-0.30-dev1-transaction-hash`.

- Add the bounded `env::sighash_all_zero_lock(max_group_inputs, max_inputs,
  max_extra_witnesses, max_witness_bytes) -> SighashAllDigest` signing-message
  contract. The CKB backend hashes the exact transaction hash, the first
  current-group witness with its complete `WitnessArgs.lock` payload replaced
  by equal-length zero bytes, later present group witnesses, and witnesses after
  the transaction input count, with canonical little-endian `u64` length
  prefixes. All counts and each included witness are bounded; excess runtime
  shape uses stable error 69. A real CKB-VM differential matches the pinned
  `ckb-sdk-rust` generator across a non-contiguous Script group, an unrelated
  input witness, and a transaction-level extra witness. Metadata schema 71
  binds the exact domain, scope, transform, ordering, result type, and four
  literal limits. The generated TypeScript builder manifest and action plan
  preserve the domain and mark pre-signing witness placement as required; the
  browser summary exposes the same record. The standalone checker cross-checks
  those records against
  typed call operands and runtime access provenance after outer hashes are
  rebound. The generic `env::sighash_all(source)` path remains deferred, and
  this all-zero placeholder contract does not claim prefix-preserving multisig
  layouts. The rebuilt browser bundle is 560,647 bytes gzip with SHA-256
  `fb58a9463bbf83f056b496b505e44fb0072f8b0e6c8d392678c959771279ed15`,
  within the 600 KiB budget. Advance the artifact cache identity to
  `project-source-set-v42-0.30-dev1-sighash-zero-lock`.

- Make package environment propagation chain-identity-safe before the
  ProtocolBundle work begins. A dependency edge may name its dependency-local
  environment with `use_environment = "..."`, inherit the unique environment
  whose `chain_id` and normalized 32-byte genesis hash match the root, or
  declare `environment_independent = true`. Equal display names no longer
  select transitive overrides. Missing, mismatched, or ambiguous mappings fail
  with compile diagnostics, while update-time external resolvers receive the
  already validated chain identity without looking up an inherited name in the
  dependency manifest. Canonical dependency node IDs bind the root name,
  dependency-local name, selection policy, chain ID, and genesis hash, so
  frozen/offline materialization rejects stale mappings even when manifest and
  source hashes are rebound. Update-time resolver requests advance to
  `cellscript-dependency-resolver-request-v2`, carrying separate `root_name`
  and optional dependency `local_name` fields beside the validated identity.
  Also compare transitive path dependencies after
  resolving each path from its own manifest root. Unit coverage includes a
  three-package diamond with different local environment names, explicit
  independence, same-name/different-genesis rejection, ambiguous matches,
  rebound lock evidence, and the former external-resolver panic path.

- Add the Phase 0 and deterministic offline-composition core for issue #9.
  `cellc protocol bundle check` reads a bounded
  `cellscript-protocol-bundle-input-v1`, confines artifact paths, admits every
  referenced ELF/metadata/lowering/source-map set through the standalone
  checker, binds exact entry/package/lock/deployment/interface/typed-semantic
  identities, and emits a canonical `cellscript-protocol-bundle-v1` plus CKB
  bundle hash. The resolver sorts declarative inputs while retaining physical
  transaction array order and rejects all documented ownership, output,
  witness, dependency ordering, Script/resource identity, capacity,
  fee/change, network/profile, signing-domain, and missing-index conflicts with
  stable `PB200`-`PB211` codes. A real three-artifact order/token/authorization
  CLI fixture proves canonical order independence and fail-before-signing
  conflict output. The Phase 1 builder-contract slice additionally requires a
  generated builder manifest for action entries, checks its identities,
  runtime contract, structural manifests, and selected-action projection
  against admitted metadata, and validates the shared skeleton plus explicit
  builder-assumption evidence against every artifact. Missing or invalid
  builder evidence reports `PB212`; a tampered builder projection fails
  artifact admission. Runtime transaction serialization, per-Script-Group
  CKB-VM execution, RPC/signing/submission, and chain evidence remain
  explicitly unexecuted until the later #9 phases.

- Add the first #9 runtime-adapter materialization boundary. ProtocolBundle
  cell slots may now bind concrete input OutPoints, `since`, and exact cell
  data, while witness commitments may carry separately verified exact field
  bytes. `cellscript-ckb-adapter` independently rechecks the canonical bundle
  hash and complete per-artifact admission/metadata evidence, preserves the
  ordered transaction arrays in a packed CKB `TransactionView`, checks
  occupied output capacity, computes the fee remainder, and emits raw and
  full-serialization hashes and sizes. Each selected Lock or Type artifact is
  attributed to exact global and group-relative indexes and the same complete
  transaction byte hash. CKB-VM group execution, signing, RPC, and chain
  evidence remain explicitly unexecuted.

- Add a hash-checked ProtocolBundle node dry-run receipt. The CKB adapter sends
  the exact materialized transaction to `estimate_cycles`, rejects any
  transaction/materialization mismatch, retains aggregate cycles, and records
  every direct Lock/Type artifact as accepted under the same complete
  serialization hash. Per-group cycle fields remain null because this RPC
  exposes only the aggregate count; spawned verifiers remain independently
  unobserved. Tx-pool and committed-chain evidence are not inferred from the
  dry-run.

- Add fail-closed ProtocolBundle live-input resolution. The adapter verifies
  the connected chain ID and genesis hash, queries every input with full cell
  data, requires `live` status, and compares the ordered OutPoint, packed
  CellOutput hash, data hash, capacity, and resulting fee against the exact
  materialization. The new
  `cellscript-protocol-bundle-live-resolution-v1` record upgrades
  `capacity_source` from the bundle skeleton to `live-node` while remaining
  explicit that this is uncommitted state.

- Add live ProtocolBundle deployment-dependency resolution. Materialization
  now binds each admitted artifact hash and Script code identity to its exact
  transaction CellDep index. The adapter rechecks the connected chain, requires
  every code Cell to remain live, verifies the admitted ELF data hash for
  `data`/`data1`/`data2` and `type` deployments, and expands Molecule dep-group
  member OutPoints before accepting a match. The resulting dependency record
  is bound to the exact transaction and prior live-input evidence and remains
  explicitly uncommitted.

- Add the ProtocolBundle signing and submission state machine. Exact live-input
  and live-dependency receipts produce `ReadyToSignProtocolBundleTx`; the
  adapter runs caller-supplied CKB SDK unlockers, refuses remaining Lock Script
  Groups, preserves compiler-owned witness fields, and permits only witness
  lock changes over the same raw transaction. Signed node dry-run verifies
  signatures and every direct group, `test_tx_pool_accept` binds node cycles
  and fee, and submission is refused before RPC unless the exact signed bytes
  already carry tx-pool evidence. Submission receipts remain uncommitted.

- Extend generated TypeScript action builders with a typed ProtocolBundle v1
  client. Each package exports its metadata/artifact/interface identity, binds
  an exact deployment without accepting a mismatched ELF hash, and sequences
  offline check, materialization, live inputs, live dependencies, readiness,
  resumable external signing, signed dry-run, tx-pool acceptance, and
  submission. Signing requests contain an opaque transaction handle and
  explicitly contain no private keys. Generated packages compile with `tsc`
  and exercise both the positive state order and wrong-state rejection.

- Complete the first reorg-aware ProtocolBundle confirmation boundary. The CKB
  adapter polls the submitted transaction's canonical `get_transaction`
  location, derives depth from `get_tip_header`, restarts observation if an
  inclusion disappears or changes, and rechecks the location before emitting
  `cellscript-protocol-bundle-confirmation-v1`. The final
  `ConfirmedProtocolBundleTx` records inclusion block/index, observed tip,
  required and observed depth, network identity, and reorg count while
  explicitly refusing an absolute-finality claim. Generated TypeScript clients
  expose the same ninth state and validate bounded polling policy.

- Make ProtocolBundle eligibility discoverable through Registry verification
  evidence. Both independent Registry verifiers emit the exact bundle schema,
  artifact-binding schema, and runtime-adapter identity only for CKB ELF
  bundles that carry complete sidecars and pass the standalone checker. The
  Registry worker requires the versioned triple to appear together, refuses it
  outside structurally verified CKB ELF evidence, and preserves it in accepted
  build evidence. Generic CKB executables and source-only snapshots remain
  unmarked.

- Publish the numbered ProtocolBundle end-to-end wiki tutorial consumed by the
  website docs renderer. It connects standalone artifact admission, Registry
  discovery, offline conflict checks, Rust adapter states, generated TypeScript
  external-signing flow, and the bounded confirmation/finality boundary.

- Implement closed typed cross-Script roles for issue #10. The versioned
  `cellscript-protocol-closed-role-v1` record reuses ProtocolBundle Cell and
  witness claims, requires one exclusive provider plus shared-read consumers
  at the identical physical source, and checks an exact Molecule type/hash in
  every participant's independently admitted metadata. Resolved roles bind
  package, entry, interface, ELF, and deployment Script identities into the
  canonical bundle; `PB213` reports incompatible type, ownership,
  correspondence, or closed-foreign identity. Generated TypeScript packages
  export the same schema and a `bindClosedProtocolRole` constructor. Open or
  runtime-selected roles remain assigned to the separate Script-handle work.

- Add issue #11's exact-artifact receipt and fixed-width value foundation.
  Every independently admitted ProtocolBundle artifact now carries a
  `cellscript-exact-script-handle-receipt-v1` record that binds its selected
  entry, exact `Cell.lock` node, Lock/Type/spawned-verifier role, interface,
  typed semantics, exact ELF, target profile, existing package/Registry ABI
  hash, verified bundle,
  complete deployed CKB Script, code CellDep, and chain identity. Its ordinary
  `CSHDLv1-fixed-202` value commits to the receipt plus the complete Script,
  interface, artifact, profile, and ABI identities without granting Cell
  lifecycle authority. Data-hash deployment policy remains distinct from
  Type-hash code-Cell identity, and the CKB Script hash implementation is
  checked against `ckb-types` for all four supported hash types. Closed-role
  participants copy the exact handle, generated TypeScript packages expose a
  checked-handle binder, and builder manifests bind the same ABI/profile and
  handle schemas. Source-level generic `ScriptHandle<I>` /
  `VerifierHandle<I>` construction and runtime-selected/open roles remain
  later issue #11 phases.

- Add the source/runtime exact-handle phase of issue #11. `ExactScriptHandle`
  is an ordinary fixed 202-byte ABI value accepted by three checked helpers:
  `ckb::require_cell_lock_exact_handle`,
  `ckb::require_cell_type_exact_handle`, and
  `ckb::require_cell_dep_exact_verifier_handle`. Each call requires the CKB
  Blake2b-256 hash of the complete handle as a compile-time `Hash` literal,
  validates magic/class/role and every committed byte, then compares the
  selected full Lock/Type Script hash or CellDep data hash. Stable error 70
  covers malformed values and every substitution class. Typed semantics,
  ProofPlan, runtime provenance, LSP completion, syntax governance, the
  standalone checker, and real CKB-VM positive/negative tests share this
  contract. ProtocolBundle artifacts and closed-role participants expose the
  already recomputed `exact_handle_hash`, and generated TypeScript binders
  retain it as `handleHash`. Generic `ScriptHandle<I>`, compatible/open handles, and
  deployment-line upgrades remain later phases. Advance the artifact cache
  identity to `project-source-set-v43-0.30-dev1-exact-script-handle`.

- Close the exact-handle pre-signing validation slice of issue #11. Exact
  helper ProofPlans now retain their typed source kind and handle parameter and
  emit mandatory `exact_script_handle` builder assumptions. `cellc tx validate`
  checks the canonical 202-byte value and compile-time full-handle hash,
  recomputes the selected Lock/Type Script or verifier CellDep data identity,
  decodes raw or projected `WitnessArgs.input_type`, and verifies the handle at
  its compiled `CSARGv1` parameter position. The regression matrix covers
  profile/ABI and every other handle region, wrong source and witness indexes,
  CellDep substitution, complete Script args changes, and swapped handle
  parameter positions. Advance the artifact cache identity to
  `project-source-set-v44-0.30-dev2-exact-handle-tx-validation`.

- Add issue #11's off-chain deployment-line foundation without weakening the
  exact-artifact boundary. `cellscript-deployment-line-receipt-v1` accepts only
  Type-hash exact receipts, derives six-dimensional compatibility against both
  the baseline and immediate predecessor, preserves stable Script/network/
  role/profile/ABI identity, hash-links a monotonic sequence, and represents
  yanking as a terminal successor. `CSLINv1-fixed-386` embeds the full current
  exact handle plus line, policy, receipt, predecessor, availability, and
  admission-Cell identities; `CSREGv1 || handle_hash` commits the whole value.
  Source/runtime use remains deferred until a distinct Type-hash target and
  unique live admission-Cell replacement contract can reject stale versions
  in consensus. Advance the artifact cache identity to
  `project-source-set-v45-0.30-dev3-deployment-line-foundation`.

- Add the distinct `ckb-type-hash` generated-artifact profile for deployment
  lines. It reuses the CKB VM2/Zbb code-generation ABI while admitting only
  `hash_type = "type"`; the existing `ckb` profile remains pinned to `data2`.
  `Cell.toml`, compile metadata, constraints, compatibility identity, CLI help,
  and the standalone artifact checker preserve the distinction and reject
  cross-profile hash-type substitution. This does not treat Type ID as
  compatibility evidence or implement the unique admission Cell/runtime path.
  Advance the artifact cache identity to
  `project-source-set-v46-0.30-dev4-type-hash-profile`.

- Add the standard Type ID deployment-line admission path. The new
  `cellscript-deployment-line-admission-evidence-v1` record binds an active
  `CSLINv1` receipt to distinct admission/code TYPE_ID Cells, their exact data
  hashes and direct transaction CellDep positions. `ckb-type-hash`
  ProtocolBundle artifacts now fail closed when that evidence is missing,
  predecessor/data-stale, yanked, code-substituted, or position-substituted.
  The separate
  `cellscript-deployment-line-admission-transition-v1` validator recomputes
  TYPE_ID creation args and checks exact one-input/one-output receipt
  replacements for upgrades and yanks. Node liveness and on-chain consumer
  helpers remain later closure. Advance the artifact cache identity to
  `project-source-set-v47-0.30-dev5-deployment-line-admission`.

- Close node-backed deployment-line liveness in the CKB adapter. Resolved
  ProtocolBundle artifacts now retain the target-profile name, so the adapter
  independently requires one deployment line for every `ckb-type-hash`
  artifact and rechecks the `data2`/`type` split. Dependency resolution queries
  the union of ordinary code, admission, and Type ID code CellDeps; verifies
  each live out point, Lock Script, Type Script, data hash, chain identity, and
  transaction position; and blocks `ReadyToSignProtocolBundleTx` on any stale
  or substituted Cell. On-chain source/runtime checks remain open. Advance the
  artifact cache identity to
  `project-source-set-v48-0.30-dev6-live-deployment-line`.

- Close the bounded on-chain consumer path for exact deployment lines.
  `DeploymentLineHandle` is a fixed 386-byte source/ABI value consumed by
  role-specific Lock, Type, and spawned-verifier helpers. Each helper requires
  a compile-time full-handle hash, active canonical `CSLINv1`, the exact
  admission CellDep Type hash and `CSREGv1 || handle_hash` data, and the exact
  code CellDep data hash; Script roles additionally bind the selected complete
  Script hash. `cellc tx validate` binds the handle to its compiled `CSARGv1`
  witness position and both direct CellDep indexes. Generated TypeScript
  builders extract the checked handle and construct that evidence shape.
  Typed semantics, ProofPlan, LSP, syntax governance, standalone-checker
  rebound mutations, and real CKB-VM stale/yank/substitution cases retain the
  same contract under stable error 71. Generic compatible/open handles remain
  outside this bounded exact-line path. Advance the artifact cache identity to
  `project-source-set-v49-0.30-dev7-deployment-line-runtime`.

- Bind typed HeaderDep reads to an independent machine contract. Lowering
  record v7 now records the exact `LOAD_HEADER_BY_FIELD` or `LOAD_HEADER`
  syscall number, field-specific contract, bounded SourceView index domain,
  and 8/208-byte buffer for epoch number, epoch start, epoch length, block
  number, and timestamp. The standalone checker validates those declarations
  against typed calls, runtime-access provenance, the generated syscall and
  field selector, full-header offsets, exact-length checks, and terminal errors
  44/45/4. Hash-rebound record and instruction mutations reject. Also clear
  schema-size facts at every generated function boundary; this fixes a backend
  bug where one runtime helper could suppress another helper's required syscall
  result-length check. Advance the artifact cache identity to
  `project-source-set-v50-0.30-dev8-header-dep-machine-contract`.

- Add canonical source-level CKB Script hashing. `script::args` accepts fixed
  bytes or a `Hash`, `script::new` constructs the typed `Script` value, and
  `script::hash` returns `ScriptHash` after serializing the exact Molecule
  table and applying CKB Blake2b-256. The fixed runtime scratch contract admits
  at most 459 args bytes, validates hash types `data`, `type`, `data1`, and
  `data2`, and returns stable error 72 for an invalid dynamic construction.
  Real CKB-VM tests differentially match `ckb-types::Script::calc_script_hash`
  at empty, ordinary, 32-byte, and maximum args widths and reject substituted
  hashes. Lowering record v8 independently binds the typed call, 53-byte
  Molecule prefix, 512-byte preimage bound, byte-copy loops, valid hash-type
  set, Blake2b delegation, and error return to decoded machine instructions;
  hash-rebound metadata, typed-record, and instruction mutations reject.
  Advance the artifact cache identity to
  `project-source-set-v51-0.30-dev9-script-hash`.

- Repair two stale CLI gate boundaries encountered by the 0.30 backend audit.
  `cellc test` now matches `expect-error` text through aggregate related
  diagnostics instead of seeing only the top-level diagnostic count, and the
  atomic-swap action-plan fixture now requires its current
  checked-runtime/verifier-covered selector state rather than the older
  runtime-resolution gap. Refresh the checked-in language-example cardinality
  for the added zero-lock sighash example and give the schema-71 NFT metadata
  envelope 768 KiB, retaining about seven percent headroom over its measured
  735,136-byte compact output. Rebind the iCKB differential matrix to the
  bounded typed-witness backend with a single-process 220-case replay and a
  second read-only 220-case replay; the 22 affected probe artifacts and their
  transaction-context hashes change while acceptance, rejection, failure-mode,
  original-binary, and normalized-fixture evidence remains stable. Refresh the
  production evidence inventory for the typed-temporal and zero-lock sighash
  language examples while retaining their non-production classification.
  Rebind the 26 affected action/Lock artifacts in the audited production
  transaction recipes, including 123 generated-Script code-hash references,
  48 dependency data-hash references, and 88 embedded complete Script hashes;
  transaction structure and business payload fields remain unchanged.

## 0.26b - Experimental semantic-foundation branch

- Complete the 0.26 economic-backend tranche across layout, code generation,
  runtime access and cryptographic kernels. In addition to the compact ELF and
  small-`li` work below, the backend now shares byte primitives, removes
  dominated schema checks and immediate stack reloads, folds exact byte loops
  structurally in IR, uses source-bound four-way read windows with a
  profitability-gated register hot window, decodes `SourceView` with shifts
  instead of DIV/REM, keeps hash state in registers, and emits VM2 Zbb
  `rori`/`roriw` rotations. These transformations preserve bounds, source kind,
  exact length and fail-closed syscall handling; they do not delete semantic
  obligations or match business/action names.

  The matched three-scenario corpus is now smaller and faster than its
  size-tuned Rust references in every row: pool merge 2,512/2,816 bytes and
  6,000/9,232 cycles, schema roll 2,272/2,760 bytes and 8,661/10,350 cycles,
  ownership Lock 2,232/2,304 bytes and 5,583/6,333 cycles (CellScript/Rust).
  The real Spore+Agent closure runs the same 52,960-byte Agent ELF in 131 paired
  transactions; the current CellScript Spore ELF is 53,000 bytes versus 66,840
  for the matched Rust `z-fat` build, and every one of its 11 accepting paths
  is faster (2.8-11.7%, 5.9% aggregate). The Fiber commitment ELF is 63,336
  bytes versus 69,176 for Rust `z-thin`; all 256 accepted paths preserve
  behavior and the transaction-level aggregate remains lower, while individual
  rows include signature-dependent cycle variance and are not presented as
  isolated Script-group measurements. These are finite, reproducible corpus
  results, not a theorem that arbitrary CellScript beats arbitrary Rust.

- Add bounded, explicit trusted-external verifier delegation. New
  `trusted_exec_cell_dep_u8_args`, `trusted_exec_cell_dep_hex4`, and
  `trusted_spawn_wait_cell_dep_hex4` source intrinsics require a compile-time
  32-byte CKB data hash plus an exact
  `cellscript-trusted-external-verifier-v1` declaration in `Cell.toml` for the
  semantic scope, operation, and exact argument adapter. Generated code
  resolves the selected CellDep,
  checks its complete `DATA_HASH`, and only then performs EXEC or SPAWN/WAIT;
  SPAWN also requires a zero child status. Raw delegation remains E2105 and an
  undeclared trusted call remains E2113 under `DenyFailClosed`;
  unused/duplicate/non-canonical declarations are rejected with E2113, and raw
  and trusted calls cannot mix within one scope. Advance compile metadata to
  schema 66 and typed semantics to v8 with a separate
  `trusted-external` evidence tier and an explicit
  `compiler_proves_internal_semantics = false` boundary. Compile metadata is
  now schema 67 after the later VM2 deployment-contract addition. The independent
  checker requires one ordered source/hash/delegate sequence over the same
  CellDep operand and cross-checks runtime, constraints, typed semantics,
  ProofPlan, and machine lowering. Real CKB-VM tests prove exact-hash EXEC and
  SPAWN positives plus CellDep-substitution rejection; checker mutations reject
  changed hashes, removed records, and evidence-tier inflation. See
  [Trusted External Verifiers](docs/CELLSCRIPT_TRUSTED_EXTERNAL_VERIFIERS.md).

- Bind live Fiber business experiments to the exact CellScript fungible ELF
  and both clean repository revisions. `fiber-node-experiments` can
  temporarily install `--cellscript-fungible-artifact` in Fiber's dev
  SimpleUDT slot, records its SHA-256, CKB data hash, byte length and Data2
  identity, restores the original fixture before reporting, and refuses to
  reuse prior execution when either repository or the artifact changes. Live
  runs default to Fiber CI's Bruno CLI `1.20.0`, accept only an explicitly
  pinned `--bruno-cli @usebruno/cli@MAJOR.MINOR.PATCH` override, and override
  `UDT_CODE_HASH` with the installed CellScript artifact's data hash. The exact
  Bruno, Node, npm, CKB and ckb-cli versions are part of the report and cache
  identity. Bruno's `safe`/`developer` sandbox selection is likewise explicit.
  The Fiber router-pay compatibility workspace expresses its final JavaScript
  checks as equivalent declarative assertions, avoiding a Bruno post-response
  hang without removing requests or weakening expected values. If Bruno retains
  RPC handles after its terminal summary, the runner requires a non-empty suite,
  every request at `200 OK`, no failed assertion marker, and a five-second exit
  grace before cleaning it up; the exceptional completion basis is recorded.
  Runtime-generated, untracked node backups are reported after restoration but
  do not masquerade as tracked source drift.

- Complete the final 0.26b business regression replay against clean, exact
  parent runtimes. CKB production acceptance at pinned CKB
  `f7fa4436737756f97a24e254f22c13a36316ecea` passes all 43 actions, 17 Locks,
  26 stateful scenarios / 46 committed steps, and 67 exact live artifact-hash
  bindings. The 218-case iCKB differential suite and full ecosystem-reuse gate
  also pass. Fiber at upstream
  `f9232d52254a5aa52195ecae296c896de7078887` runs the exact 2,032-byte Data2
  fungible ELF through direct UDT, routed payment, and UDT watchtower settlement
  workflows: 15/15, 16/16, and 28/28 requests pass with natural process exits.
  This is complete CKB production-inventory coverage and selected 3/16 Fiber
  workflow coverage, not a universal regression proof or mainnet certificate.
  See the [0.26b business end-to-end regression report](docs/releases/CELLSCRIPT_0_26_BUSINESS_E2E_REPORT.md).

- Add the bounded real-contract interoperability primitives exercised by the
  separate Spore and Fiber comparison work: exact witness/data/Lock/Type byte
  and size reads, exact Script/data-hash fields, Input `since`, transaction
  preimage reads, checked span and gathered BLAKE2b hashing, bounded byte-vector
  materialization, and the u8/hex EXEC plus hex SPAWN/WAIT adapters. These are
  typed, lowered through structured IR, emitted by the internal assembler,
  surfaced in LSP completion, and classified in the executable-surface matrix.
  The external verifier itself is not promoted by those raw adapters; production
  admission requires the trusted-external route above.

- Add a matched cost corpus (`tests/cost_corpus.rs` plus Rust references in
  `tests/fixtures/cost_corpus/`): three scenarios — a two-input pool merge
  with a checked sum and output lock binding, a two-field schema-roll
  successor with one updated field, and an ownership-claim Lock — each
  compiled on both sides with VM accept/reject parity, byte sizes, cycles
  and a growth budget. After the complete economic-backend tranche, CellScript
  is smaller on bytes (0.82-0.97x) and uses fewer cycles (12-35% less on the
  measured positives) in all three rows. Deployed sizes
  of the real DAO, secp256k1, secp-data and xUDT system scripts are printed
  as context with their different feature scopes called out; they are not
  matched comparisons.

- Record a WASM playground budget blocker: the canonical container rebuild
  after the 0.26b tranches produces a 643 KB gzip bundle, 44,650 bytes over
  the enforced 600 KB budget. The previously committed 576 KB bundle is kept;
  the policy/artifact/binding/authoring surface reachable from the
  metadata-only path must be trimmed or gated before the release gate's
  WASM bundle check can pass.

- Rebind the audited CKB acceptance recipes to the current artifact and CKB
  Script identities following the established refresh precedent. This closes
  all three identity layers together: sixty case artifacts and 417 exact
  code-hash references, 253 generated-Script selectors upgraded to `data2`,
  and 143 embedded full-Script-hash payload occurrences across 30 identities.
  External dependency selectors and identities remain unchanged. The clean
  production replay passes all 43 action cases, 17 Lock cases and 26 stateful
  scenarios / 46 committed steps, including all seven end-to-end lifecycles.

- Fail closed when a runtime `source::*` view index reaches the 32-bit index
  space. The generated SourceView helper previously added an unchecked index
  to the tagged view word, so an index at or above 2^32 carried into the view
  tag and silently re-routed an `Input` request to an `Output` view. The
  helper now rejects such indexes with `ckb-source-view-invalid` (44) through
  the existing fail-closed scalar status channel, and real-VM policy tests
  cover the forged issuer-authority transaction built from that carry.

- Add explicit bounded persistent Type policies through package artifact
  declarations and in-memory/virtual-source APIs. One ELF dispatches declared
  numeric tags from canonical, full Script-hash keyed policy witness records;
  common Unit checks precede the selected action. Fixed group roles and exact
  counts are checked at runtime. Add independent host/adapter codecs,
  pre-signing placement and typed policy/builder projection validation.
  Focused VM tests cover four action cardinalities and malformed requests.
  Chain/deployment closure for the authenticated lifecycle, independent
  machine dispatch proof, executable branch-alternative successors, remaining
  relation policies, schema acknowledgements and full product support remain
  implementation work.

- Correct the unsupported `env::sighash_all` boundary. Classify canonical
  digest construction as `ckb-sighash-all-deferred`, reject it under
  `DenyFailClosed`, and terminate audit execution with runtime error 66 rather
  than return a synthetic digest pointer. Preserve this effect through unused
  results and helper calls. Explicit BIP340 message verification and standard
  CKB Lock signing remain separate supported routes.

- Share exact artifact entry selection across codegen, metadata and CLI,
  including explicitly selected actions whose dependencies retain `main`.
  Permit witness fallback to outputs only when the input group is empty;
  a missing input witness must not authorize reading an unrelated output-side
  witness. Generated builders no longer require witness payloads for entries
  whose parameters consume no witness bytes.

- Add the parser-independent `cellscript-semantic-foundation-v3` record with a
  hash-consed value-provenance DAG, explicit artifact entry contract, typed
  transaction roles, exhaustive Cell dispositions, enforcement-classified
  claims, operation boundaries, and layered `CoreSemanticId`,
  `EntryContractId`, and `ArtifactContractId` identities. Keep source spans in
  source-map v2 and source bytes under a separate `SourceDigest`; neither is a
  core semantic hash input.
- Bind every executable ordinary `require` or native `enforce` claim to its
  canonical condition, condition-provenance node, ordered typed
  success/failure branch, and exact fail-closed runtime error. Keep supporting
  ProofPlan claims distinct, make changed conditions change `CoreSemanticId`,
  and reject broken condition, branch, or error links in the independent
  checker. Map each executable claim node to its originating condition or
  generated-sugar range in source-map v2 without including that moving span in
  the semantic hash.
- Advance compile metadata to schema 65, typed semantics to v7, verified
  lowering records to v6, source maps to v2, and the verified-artifact metadata
  carrier to v2. Extend the independent
  artifact checker and mutation corpus across the new schemas and bind
  deployable-artifact and verified-bundle identities separately.
- Give generated fatal verification failures a separate current-VM-process
  EXIT contract while preserving normal callable return values and deliberate
  raw status APIs. Record explicit typed failures as terminal operations and
  bind static error constants to a checked non-returning machine exit. The
  shared exit does not require restoring caller frames or consuming a return
  register reserved for status. Full helper/optimizer regression closure remains
  part of the authoring production gate.

## 0.26.0 - Unreleased

- Implement the first bounded consensus-runtime contracts. Fixed-width
  `BoundedCellSet<Resource, N>` now scans the exact current Type Script
  `GroupInput`, checks role, exact data size, runtime cardinality including an
  `N + 1` probe, and executes predicates once per element. Fixed-width
  `BoundedList<Plan, N>` now uses the versioned `CSBPLv1\0` Molecule FixVec
  witness plan and verifies one canonical `GroupOutput` per element, including
  complete data, exact lock, declared capacity floor, predicate execution, and
  final count. Bounded bodies admit only pure predicates, one create template,
  and numeric outer `+=` accumulators. Dynamic layouts, other sources, custom
  identities, incomplete templates, and implicit lock/capacity policy remain
  fail-closed.

- Add production-policy and real CKB-VM coverage for zero/one/N/N+1 inputs,
  malformed codecs and cell data, predicate failures, output
  count/order/data/lock/capacity mismatches, and typed/machine artifact
  mutations. Add checked `.cell` examples for variable-cardinality claims,
  1–16 order settlement, fragmented Cell merging, and bridge/rollup batches.
  Advance compile metadata to schema 62, constraints metadata to schema 3,
  verified lowering records to v4, and typed semantics to v3.

## 0.25.0 - Unreleased

- Rebase the 0.25 development line on the complete 0.24 trust boundary,
  including the lock-authoritative package graph, standalone artifact checker,
  sole internal assembler, modular code generator, executable scenarios,
  LS-IDL Registry path, and production/testnet website parity. Align the
  compiler, checker, adapters, verifiers, workspace lockfiles, and editor with
  the 0.25 development identity. Preserve the independently deployed Registry
  Type Script as the byte-identical 0.24.0 artifact whose package version and
  CKB data hash are already part of its published trust identity. Keep the
  website's public release badge on stable `v0.24.0` while giving the 0.25
  Playground compiler its own asset identity
  (`20260824-v0.25.0-32dc571c`, SHA-256
  `32dc571c2e8e32134460cb45e2329ddd29d754959d9cfe4478c638aa5fc4c7d7`).

- Harden bounded lifecycle collections at the production boundary. The typed
  IR now retains `consume_each` predicates and the `create_each` output
  template and lowers both constructs to an explicit registered fail-closed
  call instead of silently replacing the body with `Unit`. Non-production
  CKB artifacts reject the entry with stable runtime error 24
  (`collection-runtime-unsupported`); `--production` and
  `--deny-fail-closed` stop before ASM/ELF generation with E2105 and report the
  operation, source origin, missing ProofPlan enforcement, and remediation.
  Entry ABI metadata no longer presents either bounded collection as a
  supported schema pointer, and ProofPlan no longer fabricates a
  runtime-observed cardinality. Runtime Cell selection/decoding and witness
  plan/output correspondence remain deliberately unsupported until their
  consensus semantics are specified.

- Bound local compiler and gate storage without weakening the verified-artifact
  contract. Incremental build caches now retain at most 32 recent identities
  per cache root, syntax-combination success runs discard reproducible
  per-case intermediates, production CKB acceptance removes its multi-gigabyte
  transient Cargo target and stopped-node database, and managed gate streams
  retain the latest three runs by default. Duplicate acceptance files within
  and across retained runs are hardlinked when their SHA-256 identities match,
  while stable `latest-*.json`
  indexes point to the full reports. Failed syntax runs and explicit repro
  runs keep their diagnostic artifacts; release automation can request
  unlimited local retention explicitly. `cellc clean --cache` now removes
  nested workspace `.cell/build/cache` directories as well as the root cache.
  Cache payloads must be regular files, stores create a fresh entry instead of
  overwriting a pre-populated one, and recency updates use create-new plus
  rename so malicious cache symlinks cannot redirect compiler writes.

- Begin the 0.25 language-completeness implementation with a compiler-owned
  executable-surface registry, pre-codegen production rejection, full-range
  `u128` decimal literals, integer bitwise/shift lowering, and exact scalar and
  wide division-by-zero guards. Add the parameterized non-Cell value kernel:
  explicit struct/enum/function parameters, value abilities separate from Cell
  lifecycle capabilities, phantom identity rules, deterministic bounded
  monomorphization before IR, built-in `Option<T>`, generic fixed arrays,
  metadata and `cellc explain generics`, editor/Playground highlighting, and
  CKB-VM execution evidence. Ordinary generic layouts continue to reject
  Cell-backed values. Add recursive enum/tuple/struct patterns and binding-free
  or-patterns, and close the fixed tuple/array materialization and projection
  paths required for nested payload patterns to execute in CKB-VM. Extend
  explicit Cell borrows with field paths and canonical-root read-only
  reborrowing while preserving non-escape and lifecycle-crossing rejection.
  Add explicit public/package/private visibility, canonical package interfaces,
  six-dimensional interface compatibility with stable E2501 rejection,
  Registry interface/hash admission, metadata schema 61 typed-semantics-v2 records,
  lowering-record v3 and independent V2419/V2420 typed-to-machine checks. Generic
  templates now specialize across package boundaries in their owning module;
  implementation monomorphizations no longer pollute package interfaces. Add
  labeled break/continue CFG lowering and CKB-VM evidence. Upgrade the VS Code
  grammar and Playground highlighting. Keep the default Playground bundle
  under its 600 KB gzip budget with a bounded authoring summary; complete
  public-interface, typed-semantics, ProofPlan, verified-artifact, and semantic
  language-service output remains on native `cellc` and the VS Code extension.
  Route all wide u128 binary operands
  through shared stack-spilled loading so dynamic Molecule-table field
  validation can no longer clobber live limbs, keep constant folding from
  wrapping arithmetic the runtime would trap on, decide match exhaustiveness
  with a bounded constructor-matrix computation that merges nested and
  or-pattern payload coverage, derive Cell-backed value abilities structurally
  through struct and enum fields, and register E2501 with the shared error
  registry. Dynamic-schema u128 bitwise, addition, and shift paths now carry
  exact CKB-VM execution vectors.

## 0.24.0 - 2026-08-22

- Align the complete 0.24 release identity across every workspace and verifier
  crate, the independent checker dependency, lockfiles, Registry Type Script,
  Myelin handoff, VS Code extension, website WASM bundle, README, and release
  documentation. Restore the 0.23 release hardening that propagates one pinned
  CKB checkout through backend scenarios and transaction-measure tooling.

- Remove the unreachable external RISC-V toolchain fallback and make the
  audited internal assembler the sole ELF-emission path. Reassign `E2400` to
  the verified lowering/source-map boundary that already uses it, so the
  compiler error registry now matches live diagnostics.
- Split the code generator into its documented ABI, assembler, call,
  collection, expression, frame, runtime, schema, and Cell-operation modules;
  remove crate-wide Clippy exemptions; and replace long positional helper
  signatures with named context records.
- Harden the final wide-integer boundary: resolve dynamic Molecule-backed
  `u128` fields before loading limbs, preserve the left operand across a
  second dynamic load, and make `u128 +/- u64` overflow and underflow fail
  closed with runtime error 49. Add exact CKB-VM regression vectors, remove
  zero-divisor paths from the NFT and vesting examples, and reject
  non-canonical SemVer at Registry admission.
- Remove the CKB adapter's deprecated, permanently fail-closed automatic
  deployment methods. Callers must build a verified unsigned deployment
  transaction and hand signing to an external wallet.
- Replace the deprecated `serde_yaml` crate with the maintained
  `serde_yaml_ng` continuation in the Fiber configuration renderer.
- Remove tracked browser-session traces and unused design captures, ignore
  local Codex state, and make the native source-policy check reject future
  `.playwright-mcp` artifacts.
- Keep the production and Pudge Testnet Registry websites on one UI contract.
  The website gate now builds both environments from the same source, verifies
  six shared Registry routes, and requires every generated CSS/JavaScript asset
  to be byte-identical. Testnet now ships the LS-IDL route, defaults LS-IDL
  lookups and API examples to `testnet`, and no longer preloads production
  package records into Manage or artifact-detail fallbacks. Network-specific
  origins, chain selection, sandbox expiry, no-index policy, and storage remain
  isolated.
- Preserve the corrected website release lineage that removed stale 0.22
  metadata. Publish the homepage as `v0.24.0` while keeping the Playground on
  the matching 0.24 compiler identity. The canonical WASM bundle uses asset
  identity `20260819-v0.24.0-19ce8898` and SHA-256
  `19ce8898e8161f100edebf6f982d856f3e59bfac31572642b53f2e01c70a1a17`;
  distribution checks bind the current stable release URL and displayed tag
  separately from the compiler version, asset identity, and digest.
  Remove inherited 0.25-only package-interface, typed-semantics, and future-
  syntax presentation fields from the 0.24 website branch while retaining the
  0.24 LS-IDL surface. Publish the exact 0.24 and 0.25 website gitlinks on
  separate release branches so both parent lines clone without hidden commits.
- Add first-class LS-IDL publication and discovery for CKB Lock Scripts.
  `cellc artifact ls-idl` validates the bounded 0.1 schema, appends
  `SHA-256(raw idl.json)` to an executable, generates a publish-ready bundle,
  and fetches byte-exact IDL by deployed Script identity. Registry admission,
  both verifier boundaries, immutable object storage, Postgres lookup,
  canonical `/v1/ckb/scripts/:code_hash/interfaces/ls-idl` reads, and the
  compatibility `/idl/:code_hash` route all enforce the same schema and
  executable-suffix contract. Pin all 17 current upstream client vectors and
  seven derive/example IDLs, and add an opt-in test that runs the actual
  upstream Rust client against Registry's compatibility handler. Extend that
  opt-in acceptance through the fixes merged upstream in `ckb_sudt_script`
  PR #7, real RISC-V contract builds, LS-IDL-bound ELFs, and all 25 example
  CKB-VM tests without a local compatibility overlay. Add a
  runnable Rust example, website lookup/detail surfaces, and VS Code
  validate/bind/fetch commands. Name the website tab `LS-IDL` rather than the
  ambiguous `Interface`, give it the canonical `/registry/LS-IDL` route with a
  permanent redirect from `/registry/interface`, and align its lookup panel
  with the full-width Browse surface.
  Keep implementation correctness and security review outside this
  byte-identity claim.
- Ship the 0.24 package and Registry trust closure, informed by Sui Move's
  package-alt separation of resolution from compilation. Replace permissive
  custom version checks with standard SemVer; make `Cell.lock` v3 a
  manifest-digest-bound dependency graph with exact source/content identity,
  outgoing alias edges, runtime/test feature roots, and genesis-bound CKB
  environments. Add explicit `cellc lock`, lock-authoritative build/check/test,
  `--locked`/`--frozen`/`--offline`, package aliases, optional features,
  test-only dependencies, environment overrides, immutable Git-commit and
  Registry-snapshot caches, and bounded hash-pinned external resolvers that
  normalize to an ordinary source pin and never execute during locked builds.
  Keep build dependencies fail-closed until isolated execution exists. Replace
  scattered Registry artifact-profile conditionals with the versioned,
  fail-closed `cellscript-registry-profile-catalog-v1`; only CellScript source
  profiles are dependency-resolving, while executable, reproducible, and copy
  profiles remain explicit non-resolving artifacts. Add a portable
  `examples/package_graph` fixture that executes alias, SemVer, feature,
  test-only, environment, and override selection from the frozen graph.
- Implement the 0.24 trust-closure core. CKB ELF builds now emit canonical
  `cellscript-verified-lowering-record-v1` and
  `cellscript-source-artifact-map-v1` sidecars, bound by metadata schema 58 and
  checked by the compiler-independent, budgeted
  `cellscript-artifact-checker`. The checker independently parses static
  ELF64/RISC-V layout, decodes the emitted instruction/call/branch surface,
  checks CFG reachability, frames and stack restoration, ABI/ProofPlan/syscall
  contracts, block digests, source ranges, and cross-file identities with
  stable `V2400`-`V2418` rejection codes and deterministic mutations. Package
  the checker independently and require checker-first crates.io publication
  before the matching compiler crate. Extend
  `verify-artifact` with separate binding, structural, lowering-record,
  CKB-VM, chain, and semantic-equivalence states. Make `cellc test` require an
  explicit simulator/CKB-VM backend for execution and add versioned,
  fail-closed scenarios with exact runtime errors, local multi-step live-Cell
  replacement, source-linked coverage, cycle/size/capacity limits, and exact
  artifact/checker bindings. Add a least-privilege Registry artifact worker
  whose production graph excludes the compiler. Freeze the CellScript side of
  the Myelin handoff without a new profile or raw-witness alias; keep external
  Myelin adoption and the incomplete Fiber/RGB++ matrices explicitly pending.
  Add `examples/scenario_basics` as the runnable positive/exact-negative
  scenario and four-file verified-artifact walkthrough.

## 0.23.0 - 2026-08-11

- Make Registry chain confirmation compatible with the standard CKB v0.207.0
  RPC schema by resolving a live Cell's committed block through
  `get_transaction.tx_status` instead of depending on a proxy-specific
  `get_live_cell.block_hash` extension. Recorded evidence now names both RPC
  methods while historical evidence identifiers remain readable. Make the
  tooling-release gate parse website scripts structurally and enforce the
  stable build steps in order, so adding intermediate regression checks no
  longer breaks CI through an obsolete exact-string comparison. Let the full
  backend stateful audit use an explicit isolated pinned CKB checkout through
  `CELLSCRIPT_CKB_REPO`, avoiding any need to modify an unrelated sibling CKB
  worktree during release validation. Propagate the release gate's existing
  `--ckb-repo` selection to its independent `ckb-tx-measure` workspace as
  well, so every CKB-dependent release check resolves against the same pin.
- Turn the browser Playground into a recoverable Cell-oriented workbench.
  Browser-local workspace snapshots now retain source files, entry selection,
  active panels, and an honest saved/dirty state across refreshes. Failed
  compiles preserve the last valid output as explicitly stale evidence, and a
  failed compiler Worker can be restarted without reloading the page. Add a
  metadata-derived Cell Flow view, source-linked action/type selection, a
  contextual Inspector, and an optional three-step guide while keeping raw
  actions, types, metadata, diagnostics, and the existing no-ELF WASM boundary
  available. Unify the site's interactive controls around dense, standard, and
  workflow button sizes with distinct neutral, selected, and primary states.
  Registry and Playground actions now share the same contrast-safe treatment,
  compact copy controls, focus rings, press feedback, and Phosphor interaction
  icons. The Playground compile action keeps a stable label and exposes busy
  state without turning the action itself into a transient status display.
- Bound Registry discovery requests so the interface can no longer remain in
  an indefinite loading state. The browser now delays skeletons to avoid
  flashes on fast responses, reports slow and retrying requests, retries once
  with a strict deadline, preserves stale or mirrored results when available,
  and otherwise presents an explicit recovery action. Registry rows and empty
  states use compact artifact identity marks and low-motion transitions instead
  of generic placeholder panels. Redesign the global navigation around three
  primary destinations, quieter utility controls, Phosphor SVG icons, and a
  touch-safe mobile drawer with focus containment, Escape/backdrop dismissal,
  scroll locking, and persistent theme and language controls. Source discovery
  now has a quiet hover/focus label, while fixed full and compact language
  controls prevent locale changes from shifting the desktop navigation.
  The Playground now places its toolbar, compiler panels, and status bar in a
  centred wide-screen Studio frame instead of switching ambiguously between
  the site frame and an edge-to-edge editor. An explicit, persisted focus mode
  removes site chrome and expands the same workbench to the viewport without a
  first-paint flash; phones retain the existing panel switcher and site header.
  Registry discovery now translates verification, deployment, availability,
  and consumption mode into one consumer-facing use conclusion, supports
  URL-restored intent filters, and shows the latest release date without
  replacing the canonical status axes. Artifact details split the consumer
  action from the maintainer's current evidence or deployment task, explain
  each accepted evidence kind while keeping full hashes and raw JSON
  accessible, and avoid presenting build verification as a security audit.
  Maintenance keeps the selected task visible while progressively disclosing
  alternate and destructive operations.
- Close the first-publish browser/CLI loop with `cellc publish --authorise`.
  cellc now creates and stores the delegated P-256 publishing key locally,
  opens a 15-minute exact-coordinate wallet session, and resumes publishing
  automatically after Registry approval; `--no-open` supports remote and
  terminal-only environments. Session reads expose neither the polling secret
  nor the resulting key ID to the browser. The publishing key is written to
  the OS keychain as `pending` before the browser opens, promoted to `active`
  only when either successful status returns the matching key ID, and removed
  only after the Registry confirms cancellation or pending-session expiry. A
  local polling deadline performs one final authoritative read and otherwise
  preserves the pending key. Completed sessions remain poll-readable for 24
  hours after their 15-minute approval window, closing the boundary race in
  which wallet approval commits just before the CLI's next poll. This closes
  the process-exit window after wallet approval without treating local state
  as Registry authority. The browser
  token survives same-tab refresh in `sessionStorage` and is removed on
  completion or expiry, with an executable storage-lifecycle regression test.
  Session mode now
  lists only connectors that can actually complete the browser flow and folds
  challenge creation, wallet signing, and completion into one **Approve
  publishing access** action; the full external-wallet directory remains in
  the explicit manual CLI path. Session completion atomically consumes the
  nonce, records the publishing key, claims or reviews the namespace, updates
  the session, and writes its audit trail. Concurrent or replayed completion
  returns the committed result without duplicating authority. The Publish page
  is now session-first: a direct visit presents one `cellc publish --authorise`
  starting command, while a CLI session becomes a one-screen wallet approval
  surface with one current action and end-to-end release progress. Artifact
  identity is read-only in session mode because cellc and the manifest remain
  authoritative. External signing, manifest scaffolding, and existing-key
  checks remain available in a deliberately secondary advanced workspace.
  Technical scope and session identifiers stay collapsed by default, and
  loading, expiry, retry, review-pending, and terminal-continuation states keep
  the same stable layout. Safe publishing-access reads retry once with bounded
  deadlines, while signed writes are never retried automatically; an unchanged
  failed request keeps its signature, and any coordinate or payload change
  clears it with an explicit explanation.
- Add an isolated Pudge Testnet Registry Sandbox. Its API, Postgres database,
  object volume, signing origin, RPC identity, website build, wallet storage,
  and deployment evidence are separate from production. Sandbox releases are
  hidden 72 hours after admission; version JSON is deleted at expiry and source
  objects are deleted after a 24-hour grace period, while minimal audit
  tombstones remain. The API rejects a wrong-network RPC and cross-environment
  deployment payloads. `cellc artifact record-deployment --network testnet`
  defaults to the Pudge Registry API, and `cell-dep` revalidates liveness on the
  network recorded in accepted evidence. Pudge chain history remains immutable:
  expiry removes Registry indexing and off-chain objects, not on-chain Cells.
- Complete the Registry's generalized artifact and chain-evidence path. Rust,
  C, JavaScript, and other CKB artifacts now keep explicit source, build,
  deployment, TCB, and copy-only identities instead of being presented as
  CellScript dependencies. Reproducible profiles require P-256-signed reports
  from two to sixteen policy-approved builders spanning the configured minimum
  number of independent trust domains. Reports bind the signed environment,
  source, recipe, executable, build log, builder identity, and predecessor
  evidence before verification becomes `verified`; deployment is rejected
  until that evidence exists. Add `cellc artifact reproduction-report` and
  `cellc artifact reproduction-evidence`, wallet-ready mainnet commitment
  transaction intents, and `cellc auth reproducer create` for generating a
  builder-local P-256 key plus a public policy enrollment record without
  exposing PKCS#8 material. Explicit CI-key output is mode 0600 on Unix and
  no-overwrite. Add fixed Registry Type/commitment Lock configuration,
  Type-Script-indexed `CSREGv1` scans, and scheduled lifecycle reconciliation
  that demotes spent commitments or stale deployment Cells without deleting
  historical evidence. Both Script code CellDeps must be live and sufficiently
  confirmed before the chain path becomes ready. The chain path is implemented
  but remains operationally disabled until the canonical mainnet Registry Type
  Script, commitment custody Lock, and both CellDeps are deployed and configured.
- Harden the unified artifact Registry boundary: default discovery now hides
  pending/rejected releases and paginates by package coordinate; deployment
  records and admin recovery must match the immutable CKB `hash_type` and
  `dep_type`; generated CellDep descriptors re-query mainnet and reject spent
  code/DepGroup Cells; RPC calls are time- and size-bounded; and deployment
  capability use commits with the chain-verified state. Positive static-mirror
  publication now follows database admission, while suppressive states are
  mirrored first to fail closed; deferred sync is audited rather than
  advertising uncommitted positive state. Add the capability-signed
  `cellc artifact set-availability` publisher path used by Manage, defensive
  frontend page deduplication, and complete `Artifact.toml` plus bundle
  scaffolding for non-CellScript submissions.
- Split delegated Registry authority into independent `publish`, `deployment`,
  and `availability` scopes. Release admission no longer grants permission to
  attach CKB deployment evidence or change a release's public availability;
  exact-coordinate and namespace-wildcard grants remain supported. In a
  package directory the CLI infers only the exact `publish` scope; deployment
  and availability grants require explicit `--scope` flags. The API, Submit command builder,
  validation, tests, and operator documentation now share this contract.
- Redesign the Registry submission and package-maintenance surfaces around
  contextual, task-first workflows: remove the public `Manage` tab and
  redundant form controls, link maintenance from package details, guide first
  publication through explicit connect, sign, submit, and namespace-claim actions, show the publication
  orientation only once per browser, replace the CCC post-connect surface with
  a compact Registry-owned wallet chooser that has no unrelated `Manage`
  action, reveal yank fields only for the yank task, and close write commands
  over verify, dry-run, and publish. Registry route and workflow state changes
  now use reduced-motion-aware transitions instead of abrupt swaps. Browse and
  Submit share one DOM-persistent Registry header through navigation, avoiding
  replacement flicker while retaining the active locale; wallet connection no
  longer gates artifact definition or local preflight, and appears only after
  the developer has chosen an artifact coordinate and the new-capability path.
  Existing capability keys use a read-only server check for live status,
  expiry, exact publish scope, and active namespace ownership; entering a key
  ID never unlocks the UI locally. Final publish commands include the
  server-confirmed `--capability-key-id`. Primary authorisation controls use
  larger, shorter-reach interaction targets. Client-routed returns now
  reinitialize Submit and artifact-detail behavior instead of leaving stale
  event handlers behind. The advanced publisher keeps a per-environment,
  same-tab draft of non-secret artifact fields and UI state while explicitly
  excluding wallet signatures, challenge/browser tokens, capability payloads,
  and private keys. Registry, Publish, and API also share one route-transition,
  vertical-rhythm, active-tab, and localized-title contract; Browse reuses its
  latest in-memory result during background refresh rather than flashing a
  skeleton on every return.
  Browse uses a no-flash loading state, URL-backed server search, and API
  pagination; bundled data appears only as an explicitly labelled error
  fallback. Static and live package details share one responsive view with
  localized statuses and copyable audit values. Publisher authorisation now
  accepts both JoyID
  (`joyid_ckb`) and standard CKB secp256k1 (`ckb_secp256k1`) principals through
  the CCC CKB-signer boundary. Production exposes only mainnet; the separately
  built Pudge Sandbox constructs a testnet client without adding a network
  selector to either environment. The frontend never accepts
  mnemonic words; traditional recovery phrases remain inside the wallet. CLI
  auth commands use `--wallet-signature`, with `--joyid-signature` retained as
  a visible compatibility alias, and the API adds the corresponding typed
  principal migration and signature verification. The compact chooser now
  preserves the complete twelve-wallet CKB directory: compatible CCC CKB
  signers connect directly, while other entries are explicitly labelled as
  external links for importing a compatible `wallet-signature.json`; opening a
  link is never represented as a wallet connection. The browser checks the
  signature shape and principal binding before submission, while the API
  remains authoritative for cryptographic verification. Every entry
  now uses the corresponding official Nervos wallet-directory SVG rather than
  an autogenerated letter mark or a runtime favicon. The chooser header no
  longer reserves space for a hidden back control, so its title, explanatory
  text, and wallet list share one left alignment edge. Submit now asks for the
  artifact kind and source language independently, and Manage groups publish,
  inspection, reproduction, deployment, commitment, and availability as
  isolated task flows; hidden task fields can no longer leak into the selected
  workflow.
- Deploy the public Registry production slice at
  `api.registry.cellscript.dev` and `registry.cellscript.dev`: Postgres 17 is
  the authoritative write store, the Node 22 adapter persists source snapshots
  and version-addressed JSON to an isolated object volume, and a read-only
  nginx service exposes `/packages/*` independently of the API/database
  process. The production stack adds live dependency-aware readiness, bounded
  request bodies, structured logs, health checks, log rotation, generated
  secrets, HTTPS, and an 8 MiB proxy admission limit sized for the 5 MiB source
  snapshot contract. Public package search, package detail, and ordered
  evidence promotion APIs are live. A daily systemd job writes atomic,
  checksum-protected Postgres/object-store backups with bounded retention; its
  first backup passed database and archive restore inspection. Public version
  responses now expose immutable snapshot descriptors, the read-only service
  serves those content-addressed snapshots, and the CLI verifies object SHA-256,
  safe paths, per-file BLAKE2b, and the whole-tree source hash before atomically
  materialising a dependency. The CLI uses the public API's accepted status as
  the default resolution authority while retaining the explicit
  `CELLSCRIPT_REGISTRY_URL` Git/offline override, and the website renders the
  live Registry with a clearly labelled read-only bundled mirror only when the
  API is unavailable. The former Registry Coming Soon surface is removed.
  First-publish admission is now user-reachable end to end: `cellc auth
  namespace claim` and the submit page's **Claim namespace** action explicitly
  establish namespace ownership between capability registration and publish.
  Publish admission now commits package, snapshot, version, capability-use,
  acceptance-audit, and completed-idempotency state in one database transaction;
  pre-admission failures release the request-owned nonce and retry reservation,
  while production readiness verifies both managed object-store prefixes and
  volume initialization repairs their ownership and modes recursively.
  Explicit unverified/quarantined install acknowledgements are persisted in
  dependency tables, preventing lock refreshes and later builds from losing the
  caller's risk policy. Publish admission now transactionally creates a leased,
  bounded verification job. A separate least-privilege worker authenticates the
  immutable snapshot, compiles it with the current CellScript compiler, checks
  the signed manifest and compatibility-profile identities, atomically records
  `verified_build` evidence, and then converges the static version object.
  PostgreSQL `FOR UPDATE SKIP LOCKED` claims, expiring leases, three-attempt
  retry/dead-letter handling, operator queue metrics/requeue endpoints, bounded
  subprocess time/output/memory, and API readiness tied to the worker heartbeat
  make the formerly documented asynchronous queue real. Public search/list now
  excludes `source_published` and `indexed_pending` by default while preserving
  explicit status queries and direct audit URLs. Package-manifest identity uses
  canonical recursively sorted JSON, eliminating cross-process `HashMap` order
  drift between publisher and verifier. Deploy that worker to the live
  production topology and exercise external publish, queue claim, real
  compilation, evidence promotion, static convergence, default visibility, and
  a fresh consumer install/check/build without an unverified override. The
  one-time seeded smoke identity and live objects were removed afterward, queue
  counts returned to zero, and a checksum-verified backup captured the migrated
  clean state. Production Compose now accepts explicit prebuilt API/verifier
  image references so shared hosts can deploy with `--no-build`. Harden the API
  and static Registry response boundary with HSTS, anti-framing, no-sniff,
  permissions policy, cross-domain-policy denial, and a deny-all CSP for JSON
  surfaces. Add a reproducible website production Compose/nginx contract with
  a read-only root filesystem, bounded temporary filesystems, health checks,
  log rotation, `no-new-privileges`, and matching browser security headers. A
  production recovery drill restores the post-`0002` dump into an isolated
  Postgres 17 container, extracts the object archive into an isolated volume,
  verifies both migrations and all seven core Registry tables, and removes the
  temporary restore resources afterward.
- Close the 0.23 syntax-audit consistency gaps: canonical type declarations
  now use comma-terminated fields, syntax-combination gates cover canonical and
  comma-free compatibility input, checked example mirrors use named `U64_MAX`
  overflow expressions, and `dev` / `ci` reject regressions. Rebind the three
  affected timelock transaction recipes to the deterministic scoped ELF data
  hashes produced by those equivalent named expressions. CKB-VM crypto
  primitive fixtures now place `CSARGv1` through the current
  `WitnessArgs.input_type` adapter path instead of the retired raw-witness
  alias.
- Complete the native-tooling cleanup: neutralize migration-era identifiers,
  remove tracked legacy traceback logs and cache exclusions, rename the native
  tooling integration suite, and add a repository-wide source-policy command
  to every gate. The policy traverses initialized submodules and rejects
  retired interpreter sources, generated bytecode/cache artifacts, capture
  logs, and active tooling references before they can re-enter the release
  contract. The canonical WASM container now explicitly selects its already
  installed pinned Rust toolchain, avoiding an unnecessary network sync during
  release builds.
- Add the explicit `cellscript-witnessargs-input-type-v2` placement ABI for
  parameterized CKB entries. Generated wrappers now resolve witnesses relative
  to the active script group, decode the `CSARGv1` payload from
  `WitnessArgs.input_type`, preserve wallet/multisig ownership of `lock`, reject
  malformed or wrongly placed payloads, and reject group-relative raw-v1
  placement. Builders place `input_type` before SDK signing because the
  complete `WitnessArgs` is signed. A canonical signed multisig-v2 CKB-VM
  regression covers a type group whose first input is not transaction input
  zero and rejects post-signing witness mutation. The Rust-native v0.23
  transaction recipes are rebound to the resulting audited ELF data hashes so
  the production stateful gate cannot silently replay stale code identities.

## 0.22.0 - 2026-07-19

- Make GitHub publication depend on the full release gate. Release evidence now
  requires a clean version/tag-matched CellScript tree, an exact clean CKB
  revision and version pin, hashed node/template/genesis provenance, mandatory
  43-action stateful coverage with per-step commit/liveness/measurement checks,
  a freshly built and archived CKB executable, complete 20-byte ELF trampoline verification,
  fresh WASM/VS Code packaging, and tests/clippy across every workspace crate.
  The WASM build now runs in a digest-pinned canonical Linux/amd64 container,
  remaps repository and Cargo source paths, uses SHA-256-pinned official
  wasm-pack 0.13.1, wasm-bindgen 0.2.121, and Binaryen 131 tools, and runs
  `wasm-opt -Oz` before enforcing the 600 KB gzip budget.
  Website provenance and assurance snapshots are regenerated from CellScript
  0.22.0 rather than displaying stale 0.17 compiler and metadata versions.
  Public `action build`/`gen-builder` contracts are verified separately from
  explicitly handwritten Python acceptance transactions; always-success
  resource Type Scripts remain a recorded fixture-only non-claim.
- Make the pinned NovaSeal RISC-V verifier artifact reproducible across the
  audited macOS arm64 and Linux amd64 builders by remapping source paths and
  stripping the release ELF with Rust's pinned `rust-objcopy`.
- Classify field-preserving N-input/N-output resource permutations as checked
  runtime conservation. This closes the strict CKB ProofPlan gap for NFT
  royalty/seller payment pairs without adding action-name-specific backend
  rules.
- Correct the misleading bundled `multisig.cell` surface: the example now
  models explicitly non-cryptographic `Approval` records, removes discarded
  64-byte signature payloads, labels witness time as reported rather than chain
  time, and keeps real signer authentication, sighash binding, witness layout,
  replay policy, and verification in an explicit Lock Script or pinned verifier
  package. The README no longer describes nonexistent CKB signature syscalls.
- Bind every `multi_phase_dao.cell` transition to the same
  `env::current_timepoint()` evidence path instead of mixing it with public
  witness time arguments.
- Harden the bundled contract examples around real Cell identities and asset
  settlement: AMM pools bind both token TypeHashes and geometric LP supply;
  NFT sales consume and relock typed Token payments; timelocks and swaps
  release actual Token outputs; DAO votes lock and redeem voting Tokens; and
  vesting separates repeatable partial claims from the terminal fully-vested
  transition, with an explicit runtime-checked `Active -> Active` self-loop for
  each partial claim. Package mirrors and acceptance action matrices track the
  same canonical sources.
- Document fail-closed Spore and RGB++ adapter boundaries, including maintained
  SDK selection, contract/deployment identity, Molecule and witness layouts,
  Bitcoin confirmation policy, and positive/negative fixture requirements.
- Add compile-checked Spore and RGB++ identity-adapter packages under
  `examples/ecosystem/`. They bind exact CKB Script identities and transaction
  positions while deliberately leaving Spore rules, RGB++ commitments,
  Bitcoin validation, witnesses, confirmations, and orchestration to pinned
  protocol packages and builders.
- Add executable exact and bounded resolved-CellDep data-hash checks. The
  bounded scan requires a literal `1..=64` maximum, uses the real
  `LOAD_CELL_BY_FIELD(DATA_HASH)` syscall path, stops on
  `INDEX_OUT_OF_BOUND`, and fails with stable runtime code `63` when absent.
  Original DepGroup identity remains manifest/builder evidence.
- Add fixed-width executable SHA-256 and SHA256d helpers for 32-byte values and
  64-byte pairs, plus a SHA256d Merkle verifier bounded to 16 siblings. Rust
  reference vectors and positive/negative CKB-VM tests cover the generated
  RISC-V; this is explicitly not a Bitcoin SPV implementation.
- Add `verifier::btc::bip340::require_signature_from_cell_dep` for an explicit
  literal verifier dependency index, retain the index-0 spelling for
  compatibility, and document the fixed 144-byte VM2 IPC envelope. The caller
  still owns message domain, ScriptGroup/WitnessArgs and sighash construction,
  authority binding, replay policy, deployment pinning, and external review.
- Migrate every in-tree Rust crate to Edition 2024 and Rust 1.97.1, adopt the
  Edition 2024 dependency resolver, pin the repository toolchain, and align CI,
  release builds, rustfmt, fixtures, and generated helper manifests.
- Harden diagnostic and CLI ergonomics: make global `--json` the canonical
  machine-output switch and emit exactly one success or failure document on
  stdout; retain hidden `--message-format=json` compatibility, classify exit
  codes, preserve error causes, assign stable `E2xxx` backend diagnostics with
  LSP `codeDescription` links, render Unicode source snippets by terminal
  width, unify VM/simulator run metrics, centralise core command rendering,
  and make MCP documentation reads UTF-8-safe.
- Add the bounded no-profile Fiber interoperability path. Metadata schema 55
  records the structurally derived `fungible-type-group-v1` entry; its ELF
  verifies exact 16-byte little-endian `u128` data, checked full-group
  conservation, legacy owner-lock or tagged policy-Type-authorised
  issuance/destruction, and unauthorised
  mint/burn rejection, while ignoring Fiber's xUDT-compatible witness prefix.
  The separate `cellscript-fiber-adapter` derives and materialises native Fiber
  configuration from compiler and live CKB evidence without a Fiber profile or
  a `fiber-lib` dependency. Bounded local-devnet runs passed Fiber's official
  multi-hop UDT payment and pending-TLC watchtower force-close collections.
  The clean, pinned full lifecycle/negative matrix remains pending, so this is
  not a production-readiness claim. Full external matrix validation requires
  content-addressed evidence files under an explicit confined root for every
  completed row and certified topology report; arbitrary non-empty evidence
  labels no longer qualify.
  Multi-asset packages may select one structurally eligible asset with
  `cellscript-fiber ... --asset <Type>`; omission remains valid only when the
  package contains exactly one candidate.
- Start the `nightly-0.22` language line with checked casts, canonical helper
  and capability registries, transitive callable effects, initial/terminal
  flow evidence, the six-tier ProofPlan taxonomy, and typed aggregate targets.
- Add typed read-only CKB transaction-view handles (`InputView<T>`,
  `OutputView<T>`, `CellDepView`, `HeaderDepView`, `WitnessArgsView`,
  `OutPoint`, and `ScriptView`). Their metadata records source, ownership,
  absence of lifecycle authority, checked-static typing evidence, and
  checked-runtime read evidence. Existing `source::*` functions remain the
  explicit low-level migration surface.
- Add finite invariant quantifiers: `forall <role> <binding> in
  <source_view<T>> { require ... }` and `count(<source_view<T>> where ...)`.
  They share the closed aggregate target model, reject unbounded and impure
  bodies, and emit ProofPlan scan complexity, field reads, cardinality/vacuous
  policy, `u64` count overflow policy, and runtime-helper-required evidence.
- Add source-aware bounded collection contracts: `input`-qualified
  `BoundedCellSet<CellType, N>` values are linearly discharged by
  `consume_each`, while `witness`-qualified fixed-width `BoundedList<Plan, N>`
  values may drive one `create` template per element through `create_each`.
  Metadata records source, ownership, maximum/runtime cardinality and vacuous
  status; ProofPlan keeps consume iteration at `runtime-helper-required` and
  output cardinality/capacity at `builder-evidence-required`. Generic
  `Vec<Resource>` remains rejected.
- Add the versioned capability algebra without inheritance syntax. The closed
  registry is shared by parsing, formatting, type checking, docgen, LSP, and
  metadata; `destroy` derives exactly `consume + burn`, while
  `replace_unique` requires `replace` plus the type's exact declared identity
  policy. Schema 51 records the registry, per-type capability-set version, and
  required/provided/entailed/missing proof fields, and rejects transitive
  authority from container-like resources.
- Add concrete fixed-width payload enums before generic ADTs. Payload variants
  now support constructor calls, exhaustive destructuring, packed one-byte-tag
  layouts, arm-local linear Cell ownership, RISC-V construction/projection, and
  an explicit pure-helper register-pair return ABI up to 16 bytes. Schema 52
  publishes canonical `enum_layouts`; dynamic, recursive, and generic payloads
  remain fail-closed/deferred.
- Add participant-role attribution to the derived ProtocolGraph without adding
  core session/channel syntax. Action metadata retains candidates from explicit
  Address equality predicates, witness or lock-args bindings, and weak
  participant-like Address field names in that precedence order. Graph edges
  publish the selected source, every candidate, deterministic conflict/missing
  lints, `metadata-only` evidence, and `authorization_proven = false`; roles are
  intentionally absent from ProofPlan. Schema 53 carries
  `actions[].protocol_role_candidates`.
- Add canonical type `validity` blocks. Pure field predicates lower to
  fail-closed checks before selected create/constructor instructions; paths
  without concrete lowering remain `runtime-helper-required`. The only
  approved environment read is `env::block_number()`, recorded as an explicit
  `builder-evidence-required` header-dep obligation because CKB-VM has no
  ambient tip-height syscall. Unknown `env::*`, transaction-view reads,
  lifecycle syntax, and non-Pure helper graphs are rejected. Type hover,
  metadata, ProofPlan, formatter, imported helper retention, and syntax-combo
  gates expose the same boundary.
- Add explicit `borrow root as view { ... }` regions for compile-time-only
  `View<T>` access to linear Cells. Views have no layout, storage,
  serialization, or ABI representation; escape, root lifecycle crossing, and
  calls outside `Pure`/`ReadOnly` helpers with dedicated `&T` parameters fail
  closed. Runtime metadata and ProofPlan expose the checked-static evidence.
- Advance compile metadata through schema version 55. The 0.22 schema sequence
  adds callable/flow/evidence fields, `runtime.transaction_view_handles`,
  bounded collection source/ownership/cardinality/vacuity/capacity evidence,
  type validity, borrow regions, capability proofs, enum layouts, protocol-role
  candidates, and the bounded Fiber compatibility contract.
- Publish the complete 0.22 release record, refresh the current wiki/roadmap and
  MCP documentation topics, document extension submodule initialization, and
  replace the unreachable VS Code gitlink with a validated 0.22 extension
  commit.

## 0.21.1 - 2026-07-11

- Close the 0.21 README documentation gap that the 0.21.0 changelog entry
  claimed but did not fully land:
  - Document the published-release install path
    (`scripts/install.sh` one-liner, including the `CELLSCRIPT_VERSION`
    pin) as the recommended way to install `cellc`; keep the source-tree
    `cargo install --path .` flow as the "tracks main" option.
  - Add `cellscript-mcp` to the README tooling-surface table so the 0.21
    agentic-loop surface is discoverable from the project front page.
  - Add `--message-format=json` and `--color=auto|always|never` to the
    README CLI options table, matching what `Tutorial-04` and the 0.21
    release notes already describe.
  - Add the 0.20 release notes, the 0.21 release notes, and the new
    `Tutorial-13: Agentic Loops and cellscript-mcp` link to the README
    docs list (the previous list stopped at 0.19).
  - Bump the wiki `Home.md` last-updated marker from `0.21.0-rc.1` to
    `0.21.0` so it matches the published tag.
- Bump the workspace crate versions (`cellscript`,
  `cellscript-ckb-adapter`, `cellscript-wasm`) from `0.21.0` to `0.21.1`
  so `cellc --version` reports the same value as the new release tag.
- No compiler, runtime, metadata, ABI, or CLI behaviour changes — the
  patch is documentation + version metadata only. The CKB target profile,
  the `--primitive-strict 0.16` / `0.17` gates, the xUDT aggregate
  invariant lowering, the flow edge validation, the CKB adapter
  resolution, the compile receipts, the `cellscript-mcp` server, and the
  CLI surface are byte-identical to 0.21.0.

## 0.21.0 - 2026-07-11

- Promote the common xUDT group amount aggregate invariant shape from
  metadata-only evidence into executable helper-backed lowering. Matching
  transfer-style actions now get an auto-lowered
  `__xudt_require_group_amount_conserved` prelude, ProofPlan records distinguish
  metadata-only, runtime-helper-required, and checked-runtime coverage, and
  strict `0.17` metadata validation rejects stale helper gaps that are not
  backed by generated runtime accesses.
- Add static flow edge membership validation. Actions that claim a state
  transition must use an edge declared by the corresponding `flow` block, while
  declared cyclic flows remain valid.
- Extend the CKB adapter with materialised action-plan resolution,
  action-aware scan selector evidence, variable-length `args_parts` script
  argument construction, manifest-backed CellDep completion, and fail-closed
  validation for missing or mismatched live-cell scan evidence.
- Bump compile metadata to schema version 44 and add type-level
  `template_layouts` plus action `state_transition_edges`. TemplateLayout
  records are metadata-only in this RC: they derive flat layouts, mark cyclic
  flows with `RootRequired`, and reject unsupported `consensus_checked = true`
  claims.
- Add compile receipts as authenticated metadata envelopes. `cellc receipt`,
  `cellc sign-receipt`, and `cellc verify-receipt` bind source, metadata,
  ProofPlan, ProtocolGraph, TemplateLayout, artifact hashes, and optional
  Ed25519 signatures; AST and IR normalised hashes remain explicitly deferred.
- Reorganise the CLI around canonical nested command groups for `explain`,
  `tx`, `deploy`, `registry`, `package`, and `auth capability` while keeping
  legacy flat aliases executable but hidden from public discovery.
- Add structured diagnostic transport through `--message-format=json`, explicit
  colour control through `--color=auto|always|never`, and `NO_COLOR` handling
  without changing successful `--json` payload semantics.
- Add the derived `ProtocolGraph` audit view and embed it in audit bundles. The
  graph remains a metadata-derived view, not a new IR or consensus source of
  truth.
- Add the in-repository read-only `cellscript-mcp` server and six CellScript
  programming skills. The dev and CI gates now run the skill-pack freshness
  check, and release modes inherit it through the embedded CI gate before
  release-only auxiliary checks.
- Reduce gate repetition: release auxiliary checks no longer repeat CI-level
  script, whitespace, and skill-pack checks; website builds avoid duplicate
  registry generation; the standalone website artifact workflow is manual-only.
- Document the 0.21 boundary across the roadmap, README, CKB adapter guide,
  metadata/gate tutorial, ProofPlan tutorial, and agentic tooling tutorial.
  P2 Template Merkleisation and new observation syntax remain deferred.
- Tighten the 0.21 RC validation boundary: add focused regression coverage for
  flow-edge membership, xUDT conserved lowering and ProofPlan coverage states,
  TemplateLayout cycle policy and `consensus_checked` rejection, CKB adapter
  `args_parts`/manifest CellDep/scan-selector evidence; add non-production
  `atomic_swap` and `multi_phase_dao` business-flow examples; extend the
  syntax-combo audit with flow, flow-create-state, and aggregate-invariant bug
  classes; add the 0.21 schema tokens to the acceptance-boundary audit; remove
  tautological registry tests and unreachable dead code in
  `scripts/cellscript_ckb_release_gate.sh`.

## 0.20.0 - 2026-06-28

- Bring `cellc` CLI discovery and direct-source diagnostics closer to Rust's
  developer experience: top-level help now shows package commands plus direct
  compile mode, `cellc --list` enumerates commands, unknown bare commands get
  nearest-command suggestions, and direct parse/lex/compile errors print
  `file:line:column` source snippets. The top-level `cellc --explain <CODE>`
  alias now mirrors the existing `cellc explain` command, and multi-diagnostic
  package checks render each frontend error with its own source context instead
  of collapsing them into one summary string.
- Fix numeric-width soundness by requiring exact non-literal numeric type
  equality while preserving declared integer literal widths through type
  checking and IR lowering.
- Reduce generated branch cost by skipping unconditional jumps to the physical
  fall-through block and by selecting `beqz`/`bnez` branch forms that keep the
  fall-through path implicit.
- Extend `cellc opt-report` and constraint artifact metadata with backend
  shape counters and estimated-cycle deltas across optimisation levels.
- Replace the incremental parallel compiler's identity ordering with a real
  dependency topological sort and cycle-safe fallback ordering.
- Close the registry source-package plan in docs and tooling: registry install,
  build, and update use the two-tier Git resolver with yanked-version skipping
  and `source_hash` verification, while `registry add` now prints next steps
  for the actual cloned discovery worktree.
- Harden the CKB/devnet acceptance path with an ELF entry ABI gate that checks
  RX-only executable segments, `filesz == memsz`, and entry trampoline
  stack-pointer preservation before local-node evidence is accepted.
- Require launch, token, and AMM bootstrap examples to carry passing ELF entry
  ABI evidence alongside existing builder-backed action, lock-spend, cycle,
  transaction-size, occupied-capacity, and stateful lifecycle checks.
- Add 0.20 release notes documenting the strengthened devnet acceptance
  boundary and the remaining difference between compile-only and live local
  devnet evidence.
- Promote multi-file package support into the 0.20 compiler/tooling boundary:
  exact-path imports, source-graph diagnostics, dependency-aware cache keys,
  package-aware LSP diagnostics, and an additive WASM multi-source metadata API.
- Mature cross-file helper reuse by inlining aliased imports, fully-qualified
  calls, same-basename dependency helpers, and transitive helper calls into the
  entry artifact with stable internal labels, while keeping ELF-linker and
  cross-script runtime-linking claims out of scope.
- Record the 0.20 evidence gate for protocol-source multi-file showcases:
  NovaSeal, iCKB, and DobEvo / DOB-EVO source refactors may demonstrate shared
  schema/type imports only when the matching devnet or CKB VM evidence is
  regenerated, and playground multi-file import/export remains browser-local.
- Add the first protocol-source multi-file candidate in NovaSeal
  fungible-xUDT by moving shared schema structs into
  `nova_fungible_xudt_schema.cell` and importing them from both profile and
  lifecycle entries. Metadata/artifact preparation records the shared schema
  source unit, and live local devnet stateful evidence passes issue, transfer,
  settle, and required negative cases for lifecycle data hash
  `0x394da78133cb2f5a5d6cd911feceeab9e97e6ad5d36c0e50f18be56653af85e5`.
- Add Tutorial 13 for agentic `cellc` loops, documenting the
  write-check-explain-fix workflow, `cellc-mcp` wrapper boundary, read-vs-write
  rule, and the distinction between compiler evidence and CKB chain evidence.

## 0.17.0 - 2026-05-04

- Add the research iCKB protocol-equivalence surface with partial CKB VM
  differential evidence, including 75 original-vs-CellScript executed rows,
  14 CellScript-only VM rows, 8 original-side VM rows, and an explicit
  `NOT_PROVEN` production-equivalence gate.
- Add 0.17 strict CKB protocol helpers for SourceView, DAO accumulated-rate
  and maturity checks, xUDT group amount helpers, script args/hash guards,
  MetaPoint/OutPoint relation scans, and C256 product requirements.
- Add executable iCKB benchmark specs and matrix evidence under
  `tests/benchmarks`, while keeping iCKB-specific receipt layout and fixture
  logic out of the generic compiler/runtime surface.
- Keep production equivalence deliberately unclaimed until owner-auth witness
  fixtures, byte-accurate receipt decoding, full DAO redeem accounting,
  generic aggregate lowering, and production manifest closure are complete.

## 0.16.1 - 2026-06-15

- Close the bundled token/AMM/launch bootstrap lifecycle gaps with explicit
  first-cell actions and strict original scoped CKB coverage.
- Rename the token authority mint action to `mint_with_authority` and the
  launch bootstrap action to `bootstrap_token` so builder-facing action names
  match the required input topology.
- Add `nft.cell::create_collection` and stateful coverage for the
  `create_collection -> mint -> create_listing -> buy_from_listing` path.
- Document and validate the CLI-first builder handoff through
  `--entry-action`, `cellc abi`, `cellc entry-witness`,
  `cellc explain-assumptions`, and `cellc validate-tx`.
- Re-run production local CKB acceptance with strict original scoped artifacts,
  complete bundled action coverage, and stateful lifecycle scenarios.

## 0.16.0 - 2026-06-14

- Add the scoped metadata-assurance release surface: operational semantics,
  ProofPlan soundness checks, builder-assumption metadata, transaction-shape
  validation, solver templates, deployment reports, proof diffs, profiling,
  transaction traces, and audit bundles.
- Ship NovaSeal as bundled proposal packages with local devnet/profile
  acceptance tooling, while keeping production claims blocked on external
  BIP340 TCB, public BTC SPV, public/shared CellDep, and profile-specific
  attestations.
- Tighten the NovaSeal public BTC SPV evidence contract so BTC-facing profile
  cases must bind current live CKB report hashes, service-builder hashes,
  CKB-side BTC commitment hashes, raw Bitcoin transaction material, block
  header and Merkle proof data, confirmation heights, and canonical SPV
  material hashes.
- Harden the 0.16 compiler-freeze gate with explicit IR poison rejection,
  instruction-level IR provenance, reserved-register contract checks, syscall
  ABI baseline coverage, and line-exact diagnostic regression directives.
- Align `cellc --help`, README command tables, and the VS Code active-file
  command surface with the 0.16 builder, transaction-template, deployment,
  profile, and audit-bundle tooling.
- Add `--primitive-strict=0.16`, which includes the 0.15 primitive vocabulary
  rules and rejects metadata-only/runtime-required ProofPlan gaps in strict
  assurance mode.
- Add descriptive standard CKB compatibility fixture manifests for sUDT, xUDT,
  ACP, Cheque, Omnilock, NervosDAO since/epoch behavior, Type ID,
  ScriptGroup, and `outputs_data` shapes.
- Add CKB stdlib protocol module schema stubs for sUDT, xUDT, TYPE_ID, HTLC,
  Cheque, ACP, and DAO-facing descriptors while keeping executable protocol
  lowering deferred.
- Carry the 0.15 proof/invariant scope forward without overstating it:
  aggregate invariant lowering, full ProofPlan soundness proofs, macro-only
  lowering, covenant stdlib helpers, strict address/script type separation,
  entry role syntax, versioned layout migration, and executable fixture
  matrices remain tracked for later releases.
- Merge the 0.15 strict syntax and example cleanup into the 0.16 assurance
  branch, including canonical `transition`/`where` action syntax, kernel-effect
  capabilities, stdlib lifecycle metadata, and VS Code packaging dry-runs.
- Keep the 0.16 documentation honest about scope: ProofPlan soundness and
  builder evidence are strict metadata-assurance gates, NovaSeal devnet
  certification is proposal-local evidence, and full production claims still
  require CKB dry-run/commit evidence plus required external attestations.

## 0.15.0 - 2026-05-26

- Add scoped invariant declarations with explicit trigger, scope, reads,
  coverage, and runtime-obligation metadata for CKB covenant auditing.
- Add Covenant ProofPlan records and `cellc explain-proof` so action, lock,
  invariant, aggregate, identity, and lifecycle obligations are inspectable in
  human-readable and JSON form.
- Add aggregate invariant primitives such as `assert_sum`,
  `assert_conserved`, `assert_delta`, `assert_distinct`, and
  `assert_singleton`; these currently emit metadata-only runtime obligations
  until executable aggregate verifier lowering is promoted.
- Promote cell identity policies and identity-aware lifecycle forms through
  `identity(...)`, `create_unique`, and `replace_unique`, including TYPE_ID,
  field, script-args, and singleton-type metadata.
- Add explicit destruction-policy forms and carry destruction policy through
  IR/codegen while keeping bare `destroy` available as the default policy.
- Reset resource capabilities from protocol verbs to 0.15 kernel effects such
  as `create`, `consume`, `replace`, `burn`, `relock`, `retarget_type`, and
  `read_ref`.
- Add `--primitive-compat 0.14` and `--primitive-strict 0.15` migration modes
  across direct `cellc` compilation and package commands, with CS0151-CS0160
  diagnostics for legacy `destroy` capability.
- Allow direct lifecycle operations to be authorized by kernel-effect
  equivalents: `destroy` accepts `consume + burn`.
- Convert canonical bundled examples, language examples, README examples, wiki
  tutorials, and release gates to strict 0.15 kernel-effect capabilities.
- Extend strict acceptance and syntax-combination gates so bundled examples
  compile directly under `--primitive-strict 0.15`, and update release
  documentation to keep 0.15 P0 scope separate from deferred 0.16 proof
  soundness and compatibility-suite work.

## 0.14.0 - 2026-05-09

- Add the CKB semantic-completeness surface for typed Source and WitnessArgs
  views, fixed-width `lock_args`, explicit `env::sighash_all(...)`, and
  profile-visible since, time, and epoch policy helpers.
- Add bounded Spawn/IPC verifier composition through `spawn`, `wait`, `pipe`,
  inherited file descriptors, and close/read/write helpers, with
  metadata-visible script references and type-checker rejection of static
  descriptor leaks, double closes, and use-after-close paths.
- Report a structured CKB target-profile ABI contract for witness data, lock
  args, Source encoding, Spawn/IPC, since/time, CellDep and script references,
  `outputs` / `outputs_data`, capacity floors, TYPE_ID, and CKB transaction
  version.
- Validate profile ABI metadata, runtime-access metadata, ScriptGroup evidence,
  TYPE_ID output plans, script references, and `outputs_data` bindings so
  release evidence fails closed when compiler policy and metadata drift apart.
- Expose declarative output capacity floors through
  `with_capacity_floor(...)` and `occupied_capacity(...)` while keeping builder
  funding, transaction-size, occupied-capacity, and acceptance evidence as
  explicit production responsibilities.
- Add executable fixed-Hash Blake2b support through CKB's
  `ckb-default-hash` personalization and metadata-visible `CKB_BLAKE2B`
  runtime access.
- Complete the state-edge spelling cleanup from legacy `move` to
  `transition`, and refresh examples, docs, formatter behavior, LSP
  completions, VS Code snippets, and syntax highlighting for the 0.14 surface.
- Add language examples for delegate verification, Spawn/IPC pipelines,
  witness/source views, TYPE_ID creation, capacity/time policy, and canonical
  style.
- Harden malformed input handling across metadata tampering, scheduler and CLI
  decoding, LSP incremental edits, static width calculations, entry-witness
  widths, and package-version parsing.
- Add the reusable 0.14 scope audit gate and document the release boundary:
  metadata/tamper validation and strict compilation now, with full
  accepted/rejected CKB transaction fixture matrices left to the later
  compatibility-suite track.

## 0.13.2 - 2026-05-03

- Complete syntax-governance layering for lifecycle semantics by keeping
  `claim`, `settle`, and `transfer` out of the executable core expression
  surface and implementing the corresponding stdlib patterns explicitly.
- Implement `std::cell::same_lock`, `std::cell::preserve_lock`, and
  `std::cell::preserve_capacity` through canonical cell metadata verifier
  checks.
- Make `std::lifecycle::transfer`, `std::receipt::claim`, and
  `std::lifecycle::settle` expand to consumed inputs, locked named outputs,
  and complete output field preservation.
- Harden preserve and require sugar so preserved fields are type-equivalent
  to their canonical require expansion and anonymous require blocks remain
  pure boolean verifier constraints.
- Remove the remaining compiler-level claim witness/signature special cases
  and reserve the old claim-signature runtime error code.
- Add example and editor-tooling coverage for the stdlib lifecycle and cell
  metadata helper surface.
- Add an executable syntax-combination audit runner for parser/formatter/type
  checking/lowering metadata/codegen oracles, wire the quick audit into local
  gates, and run the broader CI matrix in GitHub Actions and the full release
  gate.
- Make CI run on nightly branches and version tags, and add syntax-audit mode
  contracts so accidental coverage shrinkage fails closed.
- Sync the 0.13 roadmap/release scope with the 0.13.2 governance boundary and
  add a release-gate check that keeps those docs aligned.
- Pin VS Code extension packaging to `@vscode/vsce` and make local VSIX
  packaging dry-runs part of the release gate.
- Document the syntax-combination audit as a reusable release acceptance
  preflight that runs before builder-backed CKB acceptance.
- Finalize the 0.13.2 release notes under `docs/releases/`, add a docs map, and
  move historical 0.13 planning documents into `docs/archive/0.13/`.

## 0.13.0 - 2026-04-30

- Complete the internal RISC-V ELF assembler branch surface used by current
  codegen, including `beq`, `bne`, `blt`, `bge`, `bltu`, `bgeu`, `beqz`,
  `bnez`, and branch relaxation coverage.
- Harden the stack-backed `Vec<T>` helper boundary so unsupported receivers,
  invalid `extend_from_slice` element types, and unrefined `Vec::new()` slice
  extension cases fail at compile time instead of drifting into hidden runtime
  paths.
- Add `examples/language/collections/order_book.cell` as a non-production language example
  for local stack-backed order vectors.
- Add the CKB release-gate wrapper script and document the difference between
  quick compile-only evidence and full production acceptance.
- Add builder-backed local CKB valid-spend and invalid-spend acceptance coverage
  for all 16 bundled lock entries, in the same production gate as the 43 action
  flows.
- Fix lock predicate lowering so tail-expression lock results are preserved and
  `false` exits with a stable non-zero CKB script error.
- Complete the low-risk CellScript surface pass: canonicalize bundled example
  module names, capability declarations, field shorthand, typed `Vec<T>`
  literals, and the staged syntax RFC boundaries.
- Add create/struct field shorthand (`field` as `field: field`) and format
  redundant field initializers into shorthand form.
- Add contextual bounded `Vec<T>` literals for typed local bindings and
  create/struct field initializers, lowering to the existing stack collection
  constructor and push path without changing untyped array literal semantics.
- Add lock-boundary surface syntax for `protected` Cell parameters, `witness`
  data parameters, and `require` fail-closed predicates; reserve `lock_args`
  until explicit CKB script-args binding is implemented.
- Keep signer authority out of the 0.13 syntax surface: no implicit `Address`
  signer semantics, no hidden sighash defaults, and no first-class signer values
  before explicit CKB signature verification primitives.
- Split bundled examples into clean business examples and profiled acceptance
  examples, so scheduler/effect hints stay in release evidence without
  crowding the canonical teaching surface.
- Refresh LSP completions and the VS Code grammar/snippets for the new
  lock-boundary syntax.

## 0.12.0 - 2026-04-24

- Add a stable CellScript runtime error registry and expose code/name/hint
  entries through metadata and `cellc constraints`.
- Add CKB Blake2b builder/release helpers with pinned `ckb-default-hash`
  vectors through `cellc ckb-hash`.
- Add manifest-level CKB `hash_type` and `cell_deps`/DepGroup reporting, plus
  structured timelock and capacity evidence contracts.
- Add the standalone `tools/ckb-tx-measure` helper for CKB packed transaction
  size and occupied-capacity evidence, with CKB acceptance building the same
  source through a generated manifest for nested checkouts.
- Add `cellc abi`, `cellc scheduler-plan`, and `cellc opt-report` for entry
  witness inspection, scheduler-hint consumption, and optimization measurement.
- Use CKB Blake2b hashes for compiler metadata and release evidence.
- Expand entry witness tests to cover scalar, fixed-byte, `Vec<Address>`,
  `Vec<Hash>`, opaque nested `Vec<Vec<u8>>`, `Vec<u8>`, missing payload, and
  wrong-width payload cases.
- Add 0.12 production documentation for runtime errors, CKB authoring,
  deployment manifests, capacity, entry witnesses, collections, mutate,
  linear ownership, scheduler hints, migration, examples, and release evidence.
- Keep crates.io package contents narrow by excluding workflow, docs, editor,
  auxiliary tool directories, and unpublished helper binaries from the
  published crate.

## 0.11.0 - 2026-04-23

- Release CellScript 0.11.0 as the standalone CKB compiler package.
- Close the current CKB bundled-example production acceptance suite: all seven
  production examples strict-admit, all 43 actions and 16 locks strict-compile,
  and every bundled business action has an original-scoped on-chain production
  harness. Lock coverage is scoped compile coverage; `registry.cell` remains a
  compiler/tooling language example outside this production action matrix.
- Keep compatibility intact while documenting the remaining
  production hardening track around action builders, malformed matrices, and
  measured mass/cycle constraints.
- Preserve the production safety gates added in the 2026-04-23 development
  log: no CKB policy bypass, no unresolved-call ELF stubs, audit-only
  Wasm, tightened backend shape reporting, narrowed crates.io packaging, and
  explicit profile-aware constraints metadata.
- Promote the VS Code extension to production-grade local tooling with
  compiler-backed validation, formatting, scratch compilation, metadata and
  constraints reports, CKB target-profile arguments, status feedback, and stricter
  extension validation.

## 2026-04-23

- Marked Wasm output as audit-only instead of metadata-only production output.
- Removed the old ELF feature surface from runtime metadata.
- Reduced crates.io package contents by excluding GitHub workflow, wiki, and
  VS Code extension packaging files.
- Cleaned remaining clippy mechanical warnings and documented the intentional
  broad compiler-helper signature allowances so `cargo clippy --locked
  --all-targets -- -D warnings` is a release gate.
- Removed the remaining artifact-validation surface by returning a
  source-free `ValidatedArtifact` for metadata verification instead of building
  a synthetic AST.
- Kept scheduler witness metadata Molecule-only.
- Marked Wasm report output as audit-only and excluded standalone docs from the
  crates.io package contents.
- Stripped externally-linked RISC-V ELF artifacts when an external toolchain is
  available, matching the internal production artifact surface more closely.
- Made external RISC-V toolchains explicit opt-in via `CELLSCRIPT_RISCV_CC` or
  `CELLSCRIPT_RISCV_AS`/`CELLSCRIPT_RISCV_LD`, so production ELF output and
  backend shape budgets no longer depend on tools accidentally present in PATH.
- Hardened those external toolchain overrides to require absolute paths to
  existing executable files instead of accepting relative command names.
- Rebased the multisig bundled-example ELF budget on the deterministic internal
  ELF artifact size while keeping the assembly text/CFG budgets unchanged.
- Removed the executable Wasm pseudo-lowering path; the Wasm module now remains
  audit-only and rejects action/function modules instead of emitting approximate
  code.
- Removed empty module doc comments and simplified duplicated verifier branches
  reported by clippy.
- Kept lifecycle state storage explicit in cell data while allowing lifecycle
  state names in `create` initializers and qualified expressions such as
  `Ticket::Active`, avoiding hidden layout changes and numeric state
  boilerplate.
- Added LSP completions for qualified lifecycle states such as `Ticket::Active`.
- Clarified README CLI docs that `cellc test` is a compiler/policy harness, not
  trusted runtime execution.
- Removed the old CKB acceptance policy exception path so the CKB target
  profile now rejects unsupported CKB artifacts through the normal production policy
  gate.
- Removed unresolved-call ELF stub generation; production ELF emission now
  fails when a generated call target has not been lowered.
- Added executable cross-module callable linking for resolver-backed imports,
  so `launch.cell` links the real `seed_pool` callee and its transitive `isqrt`
  helper instead of relying on a synthetic fail-closed stub.
- Tightened launch example regression coverage to ensure imported callees are
  linked without pulling unrelated AMM actions into the artifact.
- Added `env::current_timepoint()` as a chain-neutral runtime time source:
  CKB lowers it to header epoch number.
- Switched bundled `vesting.cell` to the chain-neutral timepoint API, allowing
  original scoped `grant_vesting` artifacts under the CKB target profile.
- Added original scoped CKB on-chain acceptance for
  `vesting.cell::grant_vesting` with real Token/VestingConfig inputs,
  VestingGrant output verification, header dependency timepoint input, and
  malformed output rejection.
- Marked dynamic Molecule vector `len()` results as verifier-covered u64
  transition sources, so `collection.total_supply += recipients.len()` style
  CKB mutations are checked at runtime instead of reported as mutable-cell
  transition blockers.
- Fixed fixed-aggregate field byte-source lowering so original CKB verifier
  output lock checks can compare tuple-array address fields without fail-closed
  traps.
- Increased verifier expression temp slots and added regression coverage for
  the original launch bootstrap eight-recipient remaining-output sum.
- Switched CKB acceptance launch coverage from a standalone synthetic harness to
  the original scoped launch bootstrap artifact.
- Fixed dynamic Molecule table create-output checks for fixed/scalar fields so
  original `multisig.cell::create_wallet` verifies table fields through
  Molecule offsets instead of fixed-struct offsets.
- Switched the CKB multisig `create_wallet` acceptance harness to the original
  scoped artifact with dynamic `Vec<Address>` signer data.
- Preserved scalar verifier values across expected-expression evaluation and
  dynamic output decoding, fixing original `multisig.cell::propose_transfer`
  CKB checks for `Proposal.proposal_id` and `MultisigWallet.nonce`.
- Switched the CKB multisig `propose_transfer` acceptance harness to the
  original scoped artifact with dynamic `MultisigWallet` and `Proposal`
  Molecule table data.
- Switched CKB multisig `add_signature`, `propose_add_signer`,
  `propose_remove_signer`, and `propose_change_threshold` acceptance to
  original scoped artifacts with dynamic `Proposal` table/vector data.
- Switched CKB multisig `execute_proposal` and `cancel_proposal` acceptance to
  original scoped artifacts, removing the last standalone on-chain action
  harnesses from the bounded CKB matrix.
- Fixed destroy lowering to retain consumed input pointers for post-destroy
  output verification while relying on the checked Output absence scan for the
  actual destroy rule.
- Fixed scalar output verification to prefer schema/prelude expression sources
  but use runtime stack values for ordinary scalar variables, covering
  branch/match-derived bool outputs such as `ExecutionRecord.success`.
- Switched CKB token `mint`, `transfer_token`, `burn`, and `merge` acceptance
  from standalone harness sources to original scoped `token.cell` artifacts.
- Switched CKB NFT non-batch action acceptance from standalone harness sources
  to original scoped `nft.cell` artifacts, including dynamic `Collection`
  Molecule table data for `mint`.
- Switched CKB timelock `create_absolute_lock`, `create_relative_lock`,
  `lock_asset`, `request_release`, `request_emergency_release`, and
  `approve_emergency_release`, `execute_release`, `execute_emergency_release`,
  and `extend_lock` acceptance from standalone harness sources to original
  scoped `timelock.cell` artifacts.
- Fixed the CKB Molecule vector append verifier to compare fixvec payload
  bytes after the 4-byte count header, enabling original dynamic approval-list
  append checks.
- Switched CKB AMM pure-entry `isqrt` and `min` acceptance from standalone
  harness sources to original scoped `amm_pool.cell` artifacts.

## 2026-04-22

- Tightened backend CFG reachability analysis so unreachable-block metrics are rooted at the selected ELF entry label instead of treating every `.global` text symbol as reachable.
- Added a regression test proving unused global exports are still counted as unreachable from the entry root.
- Removed old `global_text_labels` parser storage after entry-root reachability replaced global-root reachability.
- Rebased bundled-example unreachable-block budgets on the stricter entry-root metric while keeping call-edge and CFG shape budgets enforced.
- Declared Rust 1.85.0 as the standalone crate MSRV so CI and users run with Cargo support for Edition 2024 dependencies.
- Updated standalone CI to archive backend-shape reports as release evidence.
- Added a committed standalone `Cargo.lock` and changed standalone CI to run with `--locked`.
