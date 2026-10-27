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
Artifact-cache generation is v124. Numeric limits and wire formats remain
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

Pending: a signed candidate source revision and completed `backend`, `dev` and
`ci` gates on that unchanged revision. Focused tests and the commit gate permit
an intermediate candidate commit; they do not close these issues. Record each
gate's source, terminal result and evidence location before changing this status.

The earlier backend run on `eb19fcc135bfa423bdfe52e4d7fb4d981fbd108c` ended with
exit 143 before completion. Its passing component tests are partial evidence,
not a backend pass for this candidate. No result from that run is substituted
for the final gates.
