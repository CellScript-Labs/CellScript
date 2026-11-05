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

## Ordered follow-up: checker and caller ownership — 2026-11-02

The ordered follow-up moves existing checker responsibilities into these
private modules. The parent retains admission orchestration and its public API.

| Owner | Contract |
| --- | --- |
| [Record graph](../crates/cellscript-artifact-checker/src/checker/record_graph.rs) | Schemas, declared limits, record edges, entry/block ABI and call reachability |
| [ELF structure](../crates/cellscript-artifact-checker/src/checker/elf_structure.rs) | ELF binding, startup, block digests, control flow, stack discipline and syscall sites |
| [Typed semantics](../crates/cellscript-artifact-checker/src/checker/typed_semantics.rs) | Typed operations, semantic foundation, ownership and dataflow |
| [Policy dispatch](../crates/cellscript-artifact-checker/src/checker/policy_dispatch.rs) | Policy-witness dispatch, bounded entry adapters and their machine contracts |
| [Script hash machine](../crates/cellscript-artifact-checker/src/checker/script_hash_machine.rs) | The finite canonical Script preimage and delegated hash contract |
| [Committed-state machine](../crates/cellscript-artifact-checker/src/checker/commitment_machine.rs) | Packed domains, opening authentication, commit construction and materialization |

All 201 existing top-level function bodies, including admission orchestration,
match the captured source after formatting. Item/field visibility and module
imports change; validation order, rejection codes and function literals do not.
The independent checker library and strict lint pass. These receipts cover the
isolated candidate, not merged backend/dev acceptance.

A separate behavior correction keeps `RuntimeHelper` identities through caller
specialization and result-shape predicates. Fixed-result availability now tests
the retained runtime target rather than its rendered label. A direct-IR
regression first reproduced an ordinary function named
`__ckb_current_script_hash` acquiring runtime Hash storage evidence; the repaired
test rejects that evidence while retaining both genuine runtime hash calls.
This correction is not described as a pure source move. Cache v188 excludes the earlier caller/source-observation candidates. Combined
acceptance remains pending; the maintainer excludes CI.
