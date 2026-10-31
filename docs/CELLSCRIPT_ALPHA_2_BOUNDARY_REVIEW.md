# Alpha.2 boundary and invariant review

Status: implementation review; final-source gates remain required.
Date: 2026-10-30.

## Retired effect contracts

The [effect contract](CELLSCRIPT_EFFECT_CONTRACTS.md) is the sole current model.
There is no category reader, declaration override, translation mode or old cache
reader. Explicit rejection code and negative fixtures are retained to prevent
accidental reintroduction. Historical release reports are evidence, not current
language specifications. Ignored build caches are invalidated by the current
cache identity and do not constitute an accepted protocol.

The Registry source-interface reader now accepts only v5 and uses an independent
closed fact validator plus component-wise subset checks. Shared wire vectors
cover Rust, Registry and browser readers; compiler-produced interface fixtures
also check the Rust and Registry replacement relation. A reproduced extra-field
acceptance case in Rust context-source parsing is fixed by strict current-shape
deserialization. It is not a compatibility translation.

The browser's action reader also rejects every former declaration/inference
field. A direct-reader reproduction accepted current facts combined with
`declared_effect_class`, `effect_declared` or `inferred_effect_class`; its guard
now rejects all three, alongside the other retired spellings. The presentation
regression covers each mixed field and the derived Cell-flow path. The standalone
checker already rejected these fields. Rejection is not a reader or translator
for the former model.

The generated SDK no longer keeps a separately written numeric receipt version:
the native exact-handle and deployment-line constants generate its schema,
version type and validation guard. This closes the reproduced v3-schema/v2-number
self-rejection. The fixed-width handle encodings remain separate contracts.

The runtime availability review also removes the bare named-spawn status stub.
It previously advertised an implementation while only returning ZK error 76.
The earlier v146 checkpoint replaced that stub with deferred `NoReturn` error
88. The 2026-10-31 follow-up supersedes it: bare spawn now rejects with source
error E1021 in all policies, including metadata-only analysis. Its runtime helper,
deferred feature and error emitter are removed. Facts do not reserve a spawn operation
for an unimplemented path. Actual CellDep spawn/exec transports retain their
independent implemented contracts. At v146, seven focused semantic tests, including six
real-VM bare-spawn cases across optimization configurations, 24 fact-contract
tests and four decoded runtime-inventory tests pass. The source-function shadow
case remains an ordinary resolved call. Cache v146 invalidates earlier outputs.

Ordinary function IR is now `IrItem::Function` / `IrFunction`, without aliases
for the former names. These IR types derive `Debug` and `Clone`, not wire
serialization. The checked callable kind remains `helper`; signatures, source
syntax, inferred facts and ABI are unchanged by the internal rename. Diagnostic
E2104 keeps its code and describes function lowering without a purity claim.

## Retained boundaries and their owners

| Boundary | Why it remains | Serialization and compatibility decision |
| --- | --- | --- |
| Edition 2026 / Edition 2027 frontends | Explicitly supported source editions with different authoring policies and one shared AST kernel | Edition/profile identity remains explicit. The private 2026 route is named `edition_2026`, with no retired route alias. Neither edition accepts manual effects. |
| Implicit / explicit / artifact entry selection | Selects a single callable or a declared policy dispatcher; distinct from retained helper reachability | `IrEntrySelection` is not serialized. Its default is named `Implicit`; no old variant alias remains. Existing entry-selection behavior is retained. |
| Implicit public visibility | The current source policy exports an unqualified declaration; explicit public/package/private visibility also exists | `Visibility::LegacyPublic` renders `legacy-public` in the current interface profile. Changing that policy is a separate language/API decision, not an effect compatibility reader. This delivery does not change visibility admission. |
| Stack collections / arena owners / borrowed views | Different storage, allocation, lifetime and ownership guarantees | Lengths do not make these interchangeable. Keep their independent machine contracts, provenance and error behavior. |
| Removed byte-buffer APIs | Reject names that formerly confused raw bytes with hashes | `legacy_bytes` returns removal diagnostics and blocks retired direct runtime symbols. It provides no conversion or executable implementation. |
| Compiler / standalone artifact checker | Separate production and independent admission responsibilities | Shared vocabulary and conformance data are allowed; the checker reconstructs facts and machine obligations independently. It must not depend on the compiler. |

Assembler closure is owned by `CODING_STYLE.md` and tested against generated
assembly. `AGENTS.md` links that inventory instead of duplicating a conflicting
Tier list. The documented emitted surface includes `mulhu`, `rori` and `roriw`;
`sgt` was already an admitted alias. No encoding defect is inferred from the
former documentation discrepancy. Test guidance now refers to behavior-owned
suites, removes deleted version-suite references and retains required large-suite
serialization. Module ownership replaces stale file-byte estimates.

Dependency pins and the exact toolchain remain unchanged. Deduplicating compiler
canonical JSON does not merge independent checking into production inference:
the checker retains its implementation and both consume exact-byte conformance
vectors. No dependency-version upgrade is implied by this review.

## Library responsibility extraction

`compile_pipeline.rs` owns structural/source checks, optional optimization and
artifact construction. `compile_cache.rs` owns persistent result validation,
source-set/cache identities, scoped suppression and safe eviction.
`metadata_build.rs` owns native evidence and bounded browser-summary construction
from completed IR. Root APIs delegate through these private modules; there are
no alternative implementations or serialized module references. Diagnostic
tests live under `compile_pipeline/` and retain their existing test names and
source-span expectations. The all-target compiler check and seven diagnostic
tests pass; final-source integration remains required.

## Emission failure and live registers

`CodeGenerator` owns assembly and a private first-error slot. Address helpers
either emit using a permitted register or record failure; they never reset that
slot. Both consuming generator entry points reach the same checked completion
path. That path rejects before assembly bytes, ELF bytes or machine evidence
can be returned. The free `generate` and `generate_with_evidence` functions only
construct a generator and delegate to those consuming entry points.

Keeping this single completion check avoids making every low-level emitter
return a second, inconsistently propagated result. A separate public failed
emitter type would add no current artifact-returning path. This is a scoped
assessment, not a proof that future entry points cannot introduce a mistake.

Regression coverage forces physical scratch exhaustion using ABI/register-name
aliases, checks that no rejected store is emitted, and exercises both consuming
entry points for both assembly and ELF. A later valid instruction and a second
error cannot clear or replace the first diagnostic. Separate cases reject large
textual operations that require an unstated live-register contract. Existing
VM tests check actual large-address arithmetic while preserving live registers.

Destination reuse in textual normalization remains intentional: a distinct,
overwritten destination can materialize an address without changing another
register. Aliased destinations, stores and stack rebasing require structured
helpers with explicit live registers. Scratch choice compares physical register
identity, including `t6` / `x31`; no caller may assume that a temporary is free.

## Phase-invariant classification

The audit's raw search counts are not panic counts. Parser methods named
`expect` return diagnostics; test-only assertions do not describe production
input handling. The following changed boundaries have been reviewed separately:

| Sites | Owning guarantee | Disposition |
| --- | --- | --- |
| Native pool output assignments | Complete, unique, ordered role assignments before lowering | Input-reachable expectations removed. Lowering returns a diagnostic with a field span and typed source diagnostic identity. |
| Unterminated ordinary strings, escapes and nested comments | Lexer must report unfinished tokens | Explicit typed diagnostics at the delimiter/escape, tested through recovery, metadata and LSP paths. |
| Resolved effect bindings and inference budgets | Every admitted call/operation has complete bounded evidence | Explicit E1016/E1017 failures, closed vocabularies and no unknown-as-empty branch. Independent reconstruction is mandatory. |
| Typed callable lookup in semantic projection | Typed entries are constructed from the same retained IR items before lookup | Internal assertion; source declarations do not choose the entry IDs. Keep this distinct from a nullable generic source-template contract. |
| Canonical JSON of typed semantic records | Closed serializable structs, primitive values and string-keyed maps | Internal serialization assertions, not malformed-source recovery. Canonical bytes remain covered by conformance/identity tests. |
| Single-value provenance selection | The immediately enclosing cardinality check proves one element | Internal nonempty assertion, distinct from unresolved/ambiguous provenance admission. |
| Native successor/pool/retire/fresh projection | Frontend admission, physical role binding and lowering must agree | Cross-phase assertions remain an explicit review boundary; do not erase them with defaults. Broader validated-representation work remains tracked in A2-06. |
| Nested fixed-field extraction | Compilation-scoped record admission supplies an ordered layout through structured IR | Repeated codegen admission and both checked-prefix width expectations are removed. Scalar/payload types and widths come from the admitted record; the checker still validates runtime proof independently. |
| Internal monomorphization-name decoding | Source admission rejects the reserved specialization marker before internal identity decoding | Both editions diagnose E2110, including non-ASCII suffixes, through native and metadata APIs. This guarded internal path is not an established source-input panic. |
| Fixed array, tuple and nominal-field extents | Admitted fixed widths and captured frames must fit the current typed artifact's `u32` extent; products, sums and final alignment must be checked | Reproduced native ABI-length overflow and typed-parameter alignment overflow are fixed. Source admission returns E1020 with the source span; IR admission repeats intrinsic validation. This representation limit is not a new allocation or VM-memory budget. |
| Typed parameter storage | Fixed-byte metadata must supply a representable width; alignment is bounded before rounding | Missing or unrepresentable widths return a boundary error. No default eight-byte width or saturating conversion remains. Boundary tests include `u32::MAX`, missing width and one beyond the wire limit. |
| Generated lockfile read | Successful generation does not guarantee a later filesystem read still finds the file | Removed the expectation. CLI0003 is an I/O diagnostic with exit 74, tested without message-substring matching. |
| Package upgrade environment analysis | Environment selection belongs to the manifest snapshot already validated for the candidate | Removed the fallible second manifest read and its expectation. Candidate environments are retained from that snapshot; no guessed defaults replace a missing reread. |
| LSP locations, symbols and multi-file edits | Core response locations must convert to transport URIs before a complete response is returned | Conversion is fallible and propagates JSON-RPC `InvalidParams`. A bad edit location rejects the complete edit instead of silently dropping part of it. Three tests cover valid, virtual, absolute-file and invalid relative locations. |
| LSP runtime startup | Runtime construction is fallible native I/O | Returns LSP0001 with the original error source and I/O category; `main` uses the existing diagnostic/exit path. No runtime-construction expectation remains. |

Stable source codes are owned by `SourceDiagnosticCode`, separate from English
messages. Changed frontend consumers assert codes and spans, not message
substrings. This incremental improvement does not claim a typed identity for
every compiler diagnostic or an exhaustive proof of all production assertions.

The review inventory masks Rust comments/literals and test-only items before
classifying candidates. It records 345 lexical candidates on this checkpoint;
two are additional fallible parser fragment methods, leaving 343 assertions.
The 205 `self.expect` parser methods are separately excluded. These figures
describe the scanned implementation, not all possible panics: arithmetic overflow
and indexing need separate review, as the fixed-layout reproductions demonstrate.
The source-hashed inventory and its limitations are retained in
`target/alpha2-work/effect-facts-migration/phase-invariant-inventory.json`.

The retained assertions fall into these owning contracts:

- Parser/lexer shape guards prove a token, required treatment or singleton list
  exists in the same function. The parser's `expect` API itself returns `Result`.
- Native CLI extraction consumes successfully validated Clap matches. The seven
  authentication extraction sites have the same required-argument contract;
  generated builder JSON is constructed locally, and externally supplied
  metadata passes validation before policy-action lookup.
- Byte decoders check exact slices, fixed headers, monotone offsets or chunk
  sizes first. BLAKE2b outputs are explicitly configured to 32 bytes. Hex-decoder
  unreachable arms follow canonical-hex admission.
- Assembler and emitter enum arms have closed dispatch guards. Register parsing
  follows validation or uses fixed register constants. Fixed tuple widths follow
  successful recursive fixed-layout admission. Source and captured-frame extents
  are now checked before address emission; outgoing argument indices are compiler
  generated, with source/ABI argument inventory retained.
- Generic requests, optimizer scopes, graph membership and diff keys are built
  and consumed by the same algorithm. Upgrade union-key iteration cannot produce
  a pair of absent nodes; an internal edge key is constructed with its separator.
- Typed entries, source dispositions, policy bindings, nested buffers, require
  provenance and ZK operands depend on the preceding admitted IR and retained
  binding evidence. Their private assertions remain compiler-invariant checks,
  not a replacement for source admission or independent machine validation.
- Closed record/JSON serialization and writing into a `String` have no
  source-controlled serializer or I/O callback. Independent canonical-byte
  conformance tests remain required.
- Private LSP session/cache mutexes protect short state transitions. Compilation,
  workspace I/O and client callbacks run outside the session lock. Poisoning
  signifies an internal failed critical section; this review adds no blanket
  recovery that could publish inconsistent revisions.

This is classification of current assertions and correction of demonstrated
input/I/O failures, not a proof that arbitrary unchecked AST/IR constructors,
resource exhaustion, future code or every arithmetic expression are panic-free.
Seven frontend boundary tests, the typed-parameter boundary test, three transport
conversion tests, 19 CLI command tests and four upgrade tests pass. Real offline
CLI probes reject both reproduced oversized-array cases and the `u32`-width
overflow in metadata/ELF modes with E1020; the normal fixed-array control succeeds.
The cache advances to v145 so earlier crashing artifacts cannot be reused.
Full final-source gates remain tracked separately under A2-14.

## Frozen resource observations

The collection-transfer and selection-business fixtures retain every frozen
ceiling. Comparing all numeric JSON paths to the review baseline finds changes
only in measured declared frames and observed stack bytes, each decreasing;
cycles, transaction sizes, guest-page observations and the other numeric values
are unchanged. These observations must still replay on the final candidate.
The resource fixtures do not claim byte-identical machine artifacts across the
effect and callable-identity cutovers. Source hashes, artifact hashes and frame
measurements are recorded explicitly; no numeric ceiling is raised to admit the
new artifacts. The local comparison receipt is
`target/alpha2-work/effect-facts-migration/resource-numeric-parity-v145.json`.

Metadata budgets are reviewed separately from VM resource ceilings. The current
structured facts and resolved bindings intentionally retain more serialized
evidence than the former category fields. The measured examples require these
metadata-only allowances; executable size, ProofPlan and VM ceilings are unchanged:

| Metadata scope | Measured compact bytes | Previous allowance | Current allowance |
| --- | ---: | ---: | ---: |
| NFT selected entry (`accept_offer`) | 210302 | 204 KiB | 206 KiB |
| Token selected entry (`mint_with_authority`) | 88145 | 85 KiB | 87 KiB |
| Vesting full module | 428199 | 416 KiB | 419 KiB |
| Vesting selected entry (`revoke_grant`) | 152277 | 148 KiB | 149 KiB |

The complete four-example full/entry metadata regression passes with persistent
caches disabled. Earlier failed size-check logs are retained. Cache v147,
`0.33-current-effect-runtime-closure`, invalidates all intermediate native
artifacts from this delivery. The current source manifest is
`target/alpha2-work/effect-facts-migration/v147-acceptance-source.json`; final
backend/dev/CI outcomes remain required before an acceptance claim.
