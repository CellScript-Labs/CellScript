# Retire digest-shaped byte buffers

Status: implemented candidate after `v0.33.0-alpha.1`, with native and rebuilt
browser regression evidence; clean-source backend admission remains deferred.
This document is
the completion contract for the maintainer's October 28 buffer-retirement
request. The published alpha tag and its historical evidence remain unchanged.

## Required result

Raw bytes, exact fixed-width byte reads and cryptographic digests must have
different source contracts. A raw read must not silently truncate or zero-pad
its input into a `Hash`. An exact 32-byte read returns `[u8; 32]`; an actual
digest returns `Hash`. Bounded byte views preserve their source and length.

Retirement includes the frontend, IR, generated runtime, independent checker,
metadata, editor/browser surface, active examples, tests and migration docs.
Reject removed source names with a useful replacement. Do not leave callable
runtime aliases or a successful fallback for removed byte-buffer helpers.

| Existing operation | Required disposition |
| --- | --- |
| `witness::raw`, `lock`, `input_type`, `output_type` | Remove the truncating/zero-padding projections; use bounded complete views or explicit exact reads |
| WitnessArgsView's implicit fixed-hash field projections | Remove; use the corresponding bounded field view or explicit exact32 read |
| `witness::bytes32` and the three `*_exact32` field reads | Return `[u8; 32]`; preserve complete read/field-width checks |
| `ckb::cell_data_hash_at` | Remove the misleading alias; use `cell_data_bytes32` |
| `ckb::cell_data_bytes32` | Return `[u8; 32]`; require at least 32 remaining bytes and allow a longer suffix |
| `ckb::cell_data_hash` | Remove the legacy 512-byte whole-buffer path; explicitly choose consensus DATA_HASH or bounded streaming BLAKE2b |
| Script `.args_hash` and `cell_*_args_hash` | Remove misleading names; exact raw args use `args32` and `[u8; 32]` |
| `require_cell_*_args_hash`, prefix/suffix variants | Remove misleading names; compare explicit bytes with exact or named prefix32/suffix32 operations |
| Real Cell/Script/transaction/code hashes and digest functions | Retain actual digest semantics and necessary bounded cryptographic scratch |

Consensus DATA_HASH and BLAKE2b of raw Cell data remain distinct. Empty Cell
data produces zero32 for the former and BLAKE2b(empty) for the latter. Do not
reinterpret the removed helper as a consensus-field alias.

## Verification required before completion

- Actual VM execution with the standard strategy and internal unoptimized diagnostics for empty, short, exact and longer inputs,
  including fields above the old 512-byte buffer and bounded exhaustion.
- Different suffixes and different original lengths remain distinguishable
  through complete byte views; removed projections reject during compilation.
- Fixed byte results compose with indexing, comparisons, helper returns,
  collection copies, live Cell views and outgoing stack arguments.
- The independent checker validates source, capacity, actual length, copy
  destinations, saved slots and returns; rebound mutations still reject.
- Scan all active producers/consumers for removed aliases and digest-shaped
  raw values. Historical release records may retain their original contracts.
- Run focused behavior checks and matching repository gates, preserving the
  maintainer's no-TAC/no-CI instruction; record every unexecuted gate explicitly.

The Talk draft is unrelated existing work and must be preserved. No new release
or tag is authorized merely by this retirement work.

## Implemented candidate

- Removed the old whole-Cell 512-byte hash emitter, witness padding/truncation
  emitters, old helper dispatch/advertisements and unreachable independent
  checker profile. Deny lists are migration/rejection mechanisms, not aliases.
- Exact32 raw results and Script `.args32` use arrays throughout the frontend,
  IR, runtime return storage, metadata, editor and independent typed checks.
- Dynamic `Hash::from_bytes(array32)` is an explicit one-field constructor;
  implicit raw-to-hash retyping remains rejected. Real digest `.0` and full
  ScriptHash `.0` preserve all 32 bytes through helper copies and returns.
- Independent streaming instruction contracts bind source tags, checked spans,
  128-byte block capacity, syscall status/reported length, 32-byte output writes
  and failure/success restoration. The existing pinned compression body remains
  unchanged. Exact raw witness and complete bounded WitnessArgs resolver bodies
  now have independent instruction contracts even without a live Cell caller.
- Updated active runtime-view tests, templates, syntax seed, docs, editor labels,
  website example source and iCKB spec source. DAO header indices are explicitly
  decoded as eight-byte integers instead of padded into Hash values.

## Local verification so far

| Check | Actual result |
| --- | --- |
| Compiler library tests | 1096 passed |
| Raw byte retirement | 11 passed |
| Runtime read boundaries | 9 passed |
| Value argument/result ABI | 12 passed |
| Stack layout scenarios | 5 passed |
| Typed runtime views and fixed resource profiles | 12 passed; ceilings unchanged |
| Unified strategy and epoch branch semantics | 5 passed |
| Bounded admission/machine mutations | 12 passed, including raw witness, resolver, streaming and raw-domain/retired-runtime rebinding mutations |
| iCKB suite after explicit source/evidence refresh, refresh disabled | 220 passed with the complete resolver recognizer |
| WASM metadata crate | 30 passed, including native-record rejection and unbound-delegation regression |
| Native external delegation | 8 VM/independent-checker tests passed after factoring the shared call inventory |
| Native release compiler build | Passed before the final browser-summary refinement; current native library/CLI/delegation regressions passed afterwards |
| WASM target compilation | `wasm32-unknown-unknown` check passed on the current source; this does not rebuild the distributed browser bundle |
| Commit gate | Passed after the additional resolver recognizer |
| CLI regression suite | 254 passed |
| Sighash and maximum cryptographic shapes | 6 sighash tests passed, including computed nominal helper results; the resource-matrix test also passed after refreshing observations; ceilings unchanged |
| Compiler/checker strict clippy | Passed |
| Frozen-interface suite | 47 passed |
| Cost corpus and four-contract composition | 4 cost tests and 7 composition tests passed; acceptance envelopes unchanged |
| Standalone checker package | 75 passed; one existing counter-set producer-dependent test ignored |
| Quick syntax-combination audit | 164 cases: 92 accepted, 72 expected rejects, zero failures |
| Bundled examples | 35 passed after example compiler requirements and lockfiles were migrated to the alpha line; resource ceilings unchanged |
| Quick backend audit | All 12 command groups passed after the example compiler/lock migration; stateful scenarios remain outside this quick mode |
| Distributed browser compiler contracts | 63 passed against the rebuilt 0.33.0-alpha.1 bundle, through all three exports and both editions |
| Canonical browser build | Passed; 1,606,472 raw bytes and 612,606 gzip bytes, below the then-applicable 614,400-byte limit |
| WASM control-flow optimization parity | 158 cases from 79 source files, both editions and all three exports: 474 metadata/diagnostic results identical before/after optimization |
| Complete artifact-checker integration suite | 212 passed, zero failures or ignored tests, before the final browser-summary refinement; independent checker and on-chain lowering were unchanged by that refinement |
| Workspace graph and inspection | Frozen/offline workspace check and offline build-plan schema v2 passed after example compiler/lock migration |

The iCKB refresh preserves the 187-row scenario set, normalized fixtures,
expected statuses, original binaries and numeric acceptance envelopes. Updated
artifact identities/measurements were subsequently bound to signed compiler
source `22e3247aa71112cdf340942d9e56b80d4f06abf1`. All 220 differential tests
passed again with refresh disabled. The matrix pin identifies that compiler
snapshot; the following integration commit updates evidence and the benchmark
gitlink without changing compiler source. The existing equivalence claim covers
the selected executed fixtures, not general production readiness.

## Remaining acceptance work

The compiler/checker snapshot passed all 212 integration tests, including
the additional recognizers, signing conversion and corrected unused-helper
mutation fixture. Compiler/checker source remained unchanged throughout the
run. Example manifests and lockfiles were migrated during that run and passed
their separate 35-test suite, quick backend audit and frozen/offline workspace
check. The recorded source fingerprint was revalidated after completion.
This does not constitute an immutable clean-source backend or release receipt.
The later browser-summary refinement rejects native proof/provenance records
instead of carrying full validators for records the browser does not produce, and keeps trusted
delegation fail-closed without an authenticated manifest. Its shared call
inventory was factored verbatim from the native path. Current-source compiler
library, native external-delegation, WASM crate and distributed-browser tests
cover that refinement; the full 212-test run preceded it.
The ZK lifecycle schema consumer passed five native tests, TypeScript checking
and five Node tests; its existing continuous-node evidence test was skipped.
Backend/dev/release gates and a clean-source stateful receipt have not been
completed. The fix-only validation phase prohibited commits and CI/TAC; the
receipts describe that phase, and no skipped check is counted as a pass.
The maintainer subsequently authorized signed commits and pushes on October 28.
CI/TAC remain disabled, and publication does not establish full backend admission.

The quick backend audit initially failed before compiling a bundled example:
an omitted compiler requirement defaults to `*`, which excludes prereleases.
Seventeen example manifests now declare `>=0.33.0-alpha.1`; sixteen explicit
lock commands succeeded (the excluded workspace member is not locked).
The complete examples suite and all twelve quick-audit command groups then
passed. Quick mode does not run stateful acceptance or the full syntax audit;
it is not a complete backend receipt.

## Rebuilt browser artifact

Docker was started and the prescribed `website/scripts/build-wasm.sh` completed
in the pinned Linux/amd64 container. The browser bundle now rejects the removed
APIs and keeps raw byte results distinct from true digests. A bounded summary
boundary rejects native proof/provenance records rather than processing them;
native record validation and the independent checker retain their contracts.
The fixed Binaryen pipeline normalizes nested control flow and converges to a
smaller module. It uses the existing pinned tool version and passed the original
gzip budget. A separate execution of that transform produced the identical bytes.
The maintainer subsequently raised the current gzip limit to 1500 KiB
(1,536,000 bytes) on October 28. Earlier receipts retain the limit actually used
for those builds.
The website's `test:playground-compiler` executes that distributed bundle through
all three metadata entrypoints. It checks both editions, raw-byte return types,
actual digest types, explicit conversion and rejection of all 14 retired calls
and five retired fields, plus unbound and mixed external delegation. All 63
tests pass. The additional 474-result corpus comparison preserves successful
metadata and rejection diagnostics exactly. Generated pipeline/provenance
data was refreshed with the current native release compiler; no website visual
design was changed.
The existing Talk draft and published alpha tag are preserved.

[Machine-readable local check record](reports/byte-retirement-local-checks.json)
records the source fingerprint, test outcomes and unfinished boundaries.
The [browser artifact receipt](reports/browser-wasm-retirement.json) binds the
rebuilt bundle, its source files, size, interface checks and corpus comparison.
The [signed publication receipt](reports/byte-retirement-publication.json)
binds the compiler source and both submodule commits, the repeated 220-test
result and the canonical WASM rebuilt from that signed source.
