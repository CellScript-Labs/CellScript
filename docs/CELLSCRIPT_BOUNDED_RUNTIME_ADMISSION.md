# Bounded runtime admission review

**Status: candidate implementation; final identified-source backend/dev/ci gates
are pending. Issues #47, #48 and #49 remain open.**

Review date: 2026-10-27. Review disposition: implementation-author scope and
evidence audit. No independent human review, inherited waiver, release acceptance
or deployment approval is claimed. The workspace version and toolchain are
unchanged. This record supersedes the blanket-blocker status in earlier progress
entries in the [foundations history](CELLSCRIPT_BOUNDED_RUNTIME_FOUNDATIONS.md);
those entries describe the evidence available at each implementation step.

## Candidate decision and boundaries

Admit the existing bounded profile through the strict executable-surface policy,
without the four unconditional `*-artifact-proof` flags. Retain all nominal,
codec, allocation, machine, control-flow, transport and lifetime checks. Older
bundles carrying those flags still report their legacy admission restriction;
they must be rebuilt and independently checked against the current contracts.
Artifact-cache generation is v129. Numeric limits and wire formats remain
unchanged; artifact admission does not authorize transaction submission.

The surface classifier now recognizes typed named-role projections and bounded
nested indexing, length and record/Cell fields. A nested successor initializer
must have the exact declared bounded type; the existing output verifier checks
the complete table and compares the source span. This corrects stale generic
field/dynamic/output classifications. It does not exempt these operations from
independent source, codec, comparison or lifetime validation.

Fixed tuple call results retain their already materialized value classification
through ordinary copies. Known fixed arrays retain checked scalar/fixed-byte indexing in
packed-byte and pointer-backed representations. These classifier corrections are
exercised by the existing copied-value, mutable-projection, tuple-return and
signed/scalar-index VM regressions under strict compilation; dynamic indexing of
other aggregate elements retains its separate boundary.

The initial source API is the October 26 complete-program choice: module-local
`collections`, `data_shape` and `cell_roles` declarations, ordinary copied reads,
and non-escaping read-only helpers. Imported ordinary helpers and fixed-value
generics are covered. Exporting these declarations, mutable/member-level loans,
recursive or escaping nested values, owned dynamic payloads, Cell ownership in
temporary collections and general heap recycling remain outside this profile.

Collections use monotonic invocation-local arenas. `items` is an element ceiling,
not a promise that all member maxima fit the shared budget. Bytes and allocation
events include growth history. Actual-count `reserve` calls remain explicit;
the native/editor budget report explains payload, alignment, record and growth
costs. Existing stack-backed Vec semantics are unchanged.

## Why the four blanket restrictions can be retired

The audit follows the complete composing checker, rather than treating one
module's limited responsibility as the scope of the entire checker. In
[`checker.rs`](../crates/cellscript-artifact-checker/src/checker.rs), selection and
allocation instruction validation produce checked sites; `nested_machine`
checks transport, effects and lifetimes using those sites; `scalar_machine`
checks typed scalar computation, calls, branches and results. Missing required
evidence fails even after artifact and sidecar hashes have been rebound.

| Retired blanket flag | Mandatory evidence in the admitted profile | Negative evidence |
|---|---|---|
| `allocated-values-artifact-proof` | Nominal owner/member contracts, typed loan/owner CFG, exact construction/allocation/descriptor/element/search/sorted-operation instructions, argument/result transport, operation order, arena isolation and copied-result preservation | Allocation machine, isolation, parameter, call, scalar, path, query, sorted-input, wide-transport and copied-lifetime mutations in `tests/artifact_checker/` and `tests/artifact_checker.rs` |
| `nested-values-artifact-proof` | Canonical bounded decoding, witness/policy ABI and Cell source identity, helper spans and lengths, typed reads, retained buffers and mandatory successor comparisons | Decoder, source/length, output, helper-effect, overwritten-buffer, skipped-check and erased-marker mutations in `tests/artifact_checker.rs` |
| `exact-role-selection-artifact-proof` | Complete bounded scans, nominal catalogs and immutable projections, Script identity, role/source coordinates, local lifecycle authority, correspondence, table/coordinate lifetime and mandatory verification order | Rebound predicate/load/store/policy mutations, mismatched schemas, source substitution, skipped construction/verification and erased evidence |
| `cell-data-view-artifact-proof` | Nominal schema/field restrictions, checked Cell load and canonical codec, coordinate construction/alias/helper transport, fixed/nested field and property reads, retained result buffers and pointer slots | Rebound fields, source coordinates, helper spills, scalar/hash property results, ordinary interfering writes and erased evidence |

This audit found a concrete collection-copy preservation gap: an ordinary helper
result store could overwrite a retained collection-copy buffer after its checked
publication. Commit `8631c8439c63e600c2231bcf48ba898b3d51b524` connects both the
buffer and its pointer slot to the enclosing memory-effect analysis. Regression
tests cover owned/borrowed results at 1/8/9/16/32/64 bytes, small/large arenas,
O0/O3, VM-changing rebound writes, mutable records, snapshots and owner drop.
The unchanged 64-byte element limit is intentional.

The checker report continues to set `semantic_equivalence_claimed = false`,
`ckb_vm_evidence = not-executed` and `chain_evidence = not-provided`. General
aggregate producer equivalence and arbitrary whole-language equivalence are not
new claims of these bounded primitives. Source computations still require normal
compiler and application validation. A source policy pass, metadata summary or
checker report alone is not a consensus or deployment receipt.

## Acceptance evidence mapped to the issues

| Requirement | Evidence and retained limit |
|---|---|
| Complete authoring comparison | Four programs under `examples/language/batches/`: `multi_asset_transfer.cell`, `mixed_settlement.cell`, `nested_migration.cell`, `record_migration.cell`; equivalent explicit/array sources under `tests/selection_bundle/fixtures/`; October 26 decision in foundations |
| #47 ownership, budgets and operation sequences | `tests/owned_collections.rs`, `tests/allocated_runtime.rs`, arena/vector/ordered/sorted primitive tests; stale/interior/foreign pointers, exhaustion and shared budgets; arena reference model and BTreeMap agreement |
| #48 canonical values and actual Cell successors | `tests/bounded_nested.rs`, named shape/derived-helper tests, independent literal-width fixture reader and generated SDK vectors; empty/mixed/max encoding, malformed offsets, one-over bounds, unused inputs and real successor capacity |
| #49 selection, authority and hostile peers | `tests/bounded_cell_selection.rs`, actual transaction tests in `src/codegen/cell_selection/tests/transaction/`, and `tests/selection_bundle/`; every required Type/Lock group, deliberately rejecting peers, role/source/schema/pairing/lifecycle boundaries |
| Builder and adapter parity | Native and generated TypeScript encoders, selected-entry v2 contract reconstruction, canonical packed transaction bytes and adapter replay; unbound conditional sites reject; generated plans still require live/deployment resolution and cannot submit |
| Formatter, editor, browser and package parity | Named syntax matrix seeds, formatter/parser/type tests, `tests/selection_bundle/editor.rs`, LSP and WASM metadata/language-service tests, locked-package ordinary helper execution; browser metadata does not carry full machine evidence |
| Strict admission and required proof | `tests/artifact_checker/bounded_admission.rs` compiles all four complete programs at O0/O3 under `DenyFailClosed`, verifies each bundle and rejects removal of required machine markers after rebinding; runtime and complete-transaction helpers also use strict policy |
| Resource acceptance | 35 selection/migration observations, 24 transfer/array observations and eight 1024-input sorted-construction observations; existing finite per-case cycles/stack/pages/ELF/bytes/capacity ceilings remain unchanged |

Resource observations distinguish successful/late-invalid group cycles from
unavailable rejected-transaction totals. Observed stack and guest pages are not
heap peaks; unavailable heap measurements remain null. Maximum sorted cases
check count/sum with separate small sequence/order differential coverage. These
are frozen complete-business and primitive cases, not a global WCET theorem for
all permitted programs. Full-maximum collection reservation exceeded existing
small-case ceilings and was rejected rather than accommodated by raising budgets.

## Final validation receipt

The identified-source backend run on signed
`eb1c1fc767614c3f9d8b6a45548a2a9191a13424` passed 1,095 compiler unit tests,
42 collection runtime tests, all 205 independent artifact-checker integration
tests and 31 nested-data tests. It then failed three business-corpus cases
because the frozen anchor artifact and transaction identities were stale.
The queued `dev` and `ci` gates did not start; this is not a backend pass.
Re-execution confirmed the positive transaction and five adversarial outcomes,
with unchanged numeric resource observations and ceilings. The anchor fixture,
its scenario-evidence references and persistent-policy resource identities were
refreshed together. All seven original business-corpus tests then passed without
measurement instrumentation. Final gates must run on the committed refresh;
these focused results do not complete admission.

The remaining-suite preflight also found seven CLI policy tests still using a
fixed Hash helper result as an unsupported output example. That shape is now
checked. Their rejection fixtures now use a dynamic `Vec<u64>` helper result,
which still carries the incomplete-output obligation. Existing policy/error
assertions remain, and a paired positive case checks the fixed Hash result.
All 253 CLI tests pass after this test-only correction; no production policy or
checker rule was weakened. The broader preflight and final gates remain separate.

The all-examples inventory was also stale at 33 files instead of 39. It now
compiles every discovered example, using standalone Edition 2027 compilation for
the four named-declaration batch programs and preserving package compilation for
the others. The targeted 39-file regression passes; its parent directory's legacy
2026 manifest is unchanged.

The optional fixed Cell scalar-field checker test also retained an obsolete
compiler rejection for fixed nested projections. Its corrected O0–O3 regression
requires successful executable compilation and bundle/storage checks, while
still requiring the narrower scalar-field profile to reject aggregate fields
with V2420. Signed scalar fields and rebound load mutations remain rejected;
no independent checker rule changed. All 77 policy artifact-checker tests pass
in `/tmp/cellscript-v129-policy-checker-final.log`.

The remaining 68-target integration preflight completed. Eight targets failed
on the earlier test/fixture state (CLI, examples, external verifier, fungible,
Order/AMM, policy artifact checker, policy lifecycle and temporal scenarios);
each now has a separate passing repaired receipt. All 19 selection-bundle
combinations and five stack-layout tests passed in that preflight. These mixed
source results are diagnostic evidence only: the complete backend/dev/ci gates
must still pass on the final clean signed source.

The v129 candidate preserves distinct legacy BLAKE2b and consensus DATA_HASH
semantics and fixes four fallible scalar callers. The v128 backend run on
`e4280a29d6af9b8828a98b4dab2dfcdbed72e010` was stopped during compiler tests
to incorporate these findings; its queued dev/ci runs did not start. The
backend run on `3b8df6c8257720cf540a6ab82c5a033f739e3b3d` was deliberately stopped
after the compiler and collection suites passed, while the checker suite was
still running. The queued dev/ci runs were cancelled before starting. These
partial results do not count as final acceptance of the v129 change. The
[read-boundary audit](CELLSCRIPT_READ_BOUNDARY_AUDIT.md) records the new contract.

The backend run on `f34c175756b4d310812dad3dfae9bc8ff60724d3` passed all 1089
compiler unit tests and 42 collection runtime tests, then stopped with 198
checker tests passing and one failure. The valid runtime-provenance fixture
exposed a 24-byte frame in `__ckb_transaction_hash` that violated the existing
16-byte owned-frame alignment check. The current-Script hash helper used the
same layout. Both helpers now allocate and restore 16 bytes, storing the size
pointer at offset 0 and the return address at offset 8. The new O0/O3 regression combines
both hashes with a live typed Cell view, executes the actual VM, and rejects
rebound misaligned-frame mutations. This failed backend receipt does not count
as a pass; `dev` and `ci` did not run for that revision.

The same combination also exposed two composition gaps. Typed Cell `lock_hash`
and `type_hash`, plus ScriptRef `hash`, now retain the source `ScriptHash` domain through IR,
admission, buffer allocation and independent property checking; `data_hash` and
raw SourceView hash calls retain ordinary Hash. The context-hash helper effects
are checked against their complete finite instruction bodies, including syscall,
length, status, frame restoration and branch targets. Call sites must provide
disjoint private 32-byte output and eight-byte length buffers, with existing
live-view/copy protection retained. Regression mutations rebind outer hashes and
reject writes through a substituted pointer or reads from the wrong length slot.

Composed runtime-view regressions also require finite effects for ScriptRef
code-hash/exact-32-byte args reads, input OutPoint transaction-hash reads and
exact-32-byte WitnessArgs field reads. Their summaries bind the complete helper
bodies and checked resolver call, while the witness resolver independently
proves its private memory effects. Rebound mutations cover copy limits, syscall
fields, resolver bounds and output pointers. ScriptRef/OutPoint projections from
helper parameters retain dynamic `SourceView` provenance and bounded indexes;
they do not invent global-versus-group coordinates. Resource fixture identity
refreshes retain the existing measurements and numeric ceilings.

Focused v125 validation passes: all four bounded-admission tests (O0/O3 positive
bundles and rebound mutations), the original runtime-provenance regression,
all 12 typed runtime-view tests with unchanged numeric resource profiles, and
all-target clippy for the compiler and standalone checker with `-D warnings`.

The subsequent [stack-layout audit](CELLSCRIPT_STACK_LAYOUT_AUDIT.md) expands
this repair across all runtime/stdlib entries and the ordinary frame/call paths.
It compacts additional helpers and independently rejects every unaligned stack
adjustment, including balanced allocation/restoration mutations. Its receipt
records the current uncommitted-source validation and supersedes the v125
focused receipt for modified layouts. No source version is frozen by that audit.

The subsequent [read-boundary audit](CELLSCRIPT_READ_BOUNDARY_AUDIT.md) records
four reproduced length-check defects and the scratch-register repair. Its v127
receipts supersede earlier focused results for the changed code. Final
clean-source `backend`, `dev` and `ci` acceptance remains pending. Production
stateful acceptance requires a clean source tree; record each gate's source,
terminal result and evidence location before changing this status or closing
the issues.

The earlier backend run on `eb19fcc135bfa423bdfe52e4d7fb4d981fbd108c` ended with
exit 143 before completion. Its passing component tests are partial evidence,
not a backend pass for this candidate. No result from that run is substituted
for the final gates.
