# Read boundary and scratch-register audit — 2026-10-27

**Status: repairs included in 0.33.0-alpha.1; complete compiler tests passed.
The [alpha receipt](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md#final-validation-receipt)
records the maintainer's full-gate exception and the retained coverage limits.**

This follows the [stack-layout audit](CELLSCRIPT_STACK_LAYOUT_AUDIT.md) on base
`f34c175756b4d310812dad3dfae9bc8ff60724d3`. Current artifact-cache generation is v129.
No release/version change, TAC or external security scan is included.

## Return conventions and compatibility (v129)

The four scalar getters `input_out_point_index`,
`input_out_point_tx_hash_low`, `cell_lock_hash_low` and
`cell_type_hash_low` return a value in a0 and status in a1. Their callers
previously ignored a1. Actual VM execution reproduced success after each failed
read at O0 (`/tmp/cellscript-v129-status-before.log`). Registry-driven callers
now terminate with the returned error before saving any scalar value.

`src/codegen/runtime_contract.rs` declares public dispatch and private helper
return conventions: scalar values, scalar/a1 status, internally terminating
errors, checked a0 status, explicit status-valued IPC results, buffer results,
and multi-register results. The generated-machine inventory requires every
runtime entry to have a convention and checks a1 initialization on every
returning path for scalar-status entries, including compatible tail calls.
This structural test is not a semantic proof of every helper or every caller.

The independent checker does not import that registry. It decodes the complete
four getter bodies and all incoming caller guards, including source selection,
field selectors, OutPoint lengths, extraction, a1 success/error values,
restoration and the error edge to the verified terminal sink. Rebound mutations
of the caller guard, status copy and callee statuses reject at O0/O3.

Legacy `ckb::cell_data_hash` keeps its 512-byte raw buffer and **544 used +
0 padding = 544-byte** frame. It checks reported length before hashing, returns
BLAKE2b(empty) for empty input, and rejects more than 512 bytes with error 3.
`cell_data_hash_field` and typed `.data_hash` separately read consensus
DATA_HASH: empty input produces zero, and no 512-byte limit applies. The field
helper retains **64 used + 0 padding = 64 bytes**. Neither frame changes in v129.
The oracle is pinned CKB
[CellOutput::calc_data_hash](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/gen-types/src/extension/calc_hash.rs).

`cell_data_bytes32(view, offset)` names the raw 32-byte read explicitly;
`cell_data_hash_at` is its compatibility alias. Both require at least 32
remaining bytes, permit a longer suffix and compute no digest. Their frame is
32 used + 0 padding = 32 bytes. The complete independent field, bounded legacy
hash and slice contracts remain distinct. Caller lifetime checks preserve
separate result/size buffers and live Cell/collection copies.

The [legacy read contract matrix](CELLSCRIPT_LEGACY_READ_CONTRACTS.md) records
short/long input behavior, failure propagation and migration choices. No legacy
witness projection or exact Script-args read silently changes semantics.

The v128 unification candidate was superseded before publication. Its focused
receipts demonstrate that candidate only. The backend run on signed
`e4280a29d6af9b8828a98b4dab2dfcdbed72e010` was stopped during compiler tests;
queued dev/ci runs did not start. No full acceptance is claimed for it.

Current focused receipts:

| Check | Receipt |
|---|---|
| Runtime inventory: four tests, 248 entries / 362 stack adjustments, O0/O3 | `/tmp/cellscript-v129-contract-inventory.log`; the dispatcher registry also includes the constant-named fungible helper omitted by the earlier literal-only discovery |
| Nine read-boundary, five stack-layout and twelve typed-view tests | `/tmp/cellscript-v129-read-stack-final.log`; valid index 44, all four scalar values alongside collection/fixed-byte results, propagated failures and distinct empty/oversized hash contracts |
| Eight bounded admission checker tests | `/tmp/cellscript-v129-bounded-checker.log` |
| Restored legacy hash plus field/slice rebound mutations | `/tmp/cellscript-v129-read-mutations.log` |
| Four scalar getters' body/caller status mutations, O0/O3 | `/tmp/cellscript-v129-status-mutations.log` |
| iCKB differential suite, serialized, no baseline updates | 220 passed in `/tmp/cellscript-v129-ickb.log` |
| Compiler/checker all-target clippy, warnings denied | `/tmp/cellscript-v129-clippy-final.log` |
| WASM metadata-only parity and separate all-target clippy | `/tmp/cellscript-v129-browser-final.log`, `/tmp/cellscript-v129-wasm-clippy.log` |

Two frozen runtime-view measurements were refreshed after adding caller guards:
input/group/dep/witness 92,306 to 92,318 cycles and 15,032 to 15,056 ELF bytes;
persistent policy 94,794 to 94,799 cycles and 19,320 to 19,328 ELF bytes. Their
stack sizes and all numeric ceilings remain unchanged. Exact artifact, lowering,
source-map and transaction identities were rebound; the complete twelve-test
suite then passed without update mode. Full identified-source backend/dev/ci
gates remain pending.

The full backend attempt on signed
`eb1c1fc767614c3f9d8b6a45548a2a9191a13424` subsequently passed all 205
artifact-checker integration tests but failed three business-corpus cases on
stale anchor identities. No queued `dev`/`ci` gate ran. Re-execution refreshed
only artifact, transaction and bundle identities in `capability_anchor_cases`,
the associated `business_scenario_evidence` records and the persistent-policy
resource profile. Its numeric measurements and all ceilings are unchanged.
The original seven-test business suite then passed in
`/tmp/cellscript-v129-business-strict-final.log`; temporary measurement output
was removed and the test source is unchanged. The failed backend log remains
`/tmp/cellscript-v129-backend-final.log`, not a final acceptance receipt.

The remaining-suite preflight also found stale external-verifier identities.
Its two successful EXEC/SPAWN cases and eight adversarial cases executed before
the frozen-record comparison failed. Only artifact/bundle/transaction hashes and
their scenario references were refreshed; dependency identities, expected exit
codes and rejection stages are unchanged. The original test then passed in
`/tmp/cellscript-v129-external-scenarios-final.log` without instrumentation.

The same preflight found stale authorization, fungible and NFT scenario
identities. Their 31 executed cases retained every expected exit code, rejection
stage and non-identity field. The three fixtures and their scenario references
were refreshed, and all three original tests passed in
`/tmp/cellscript-v129-token-scenarios-final.log`. These are focused replay
receipts, not substitutes for the final integration gates.

The temporal suite needed only three metadata identities refreshed: lowering
record, source map and verified bundle. Its ELF, all nine transaction identities
and all expected outcomes remain unchanged. Four successful and five adversarial
cases pass in `/tmp/cellscript-v129-temporal-final.log`.

The Order/AMM scenario refresh likewise changes only artifact and transaction
identities: the original four merge/swap cases pass in
`/tmp/cellscript-v129-order-amm-final.log`, preserving replay, wrong-price and
reordered-output rejection. The signed persistent-policy lifecycle executes
six live-output transactions for each of eight Edition/optimization combinations.
Its measured ELF decreased from 8,136 to 8,128 bytes; the recorded cycle sample
is 6,309,501, under the unchanged 7,000,000 ceiling and existing
signature-dependent measurement policy. Stack, witness, transaction and
dependency sizes are unchanged. These refreshed observations do not raise any
resource limit. Both lifecycle tests and the cryptographic resource inventory
pass in `/tmp/cellscript-v129-lifecycle-final.log`.

## Historical v127 read contracts and reproduced defects

The pinned CKB source is
[`f7fa4436737756f97a24e254f22c13a36316ecea`](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/syscalls/load_cell_data.rs).
Its successful Cell-data load can copy fewer bytes than requested. The size slot
reports remaining source length, while the copy is bounded by capacity. A zero
status alone establishes neither a complete fixed value nor that the entire
source fits the destination.

All four defects below were reproduced in CKB-VM before repair. The two initial
hash/slice failures are in `/tmp/cellscript-read-bounds-before.log`; the related
xUDT/WitnessArgs failures are in `/tmp/cellscript-read-bounds-related-before.log`.
These are specific input witnesses, not a claim that every artifact triggers
the defect.

| Helper | Capacity / reported length / actual copy / published value | Repair and frame |
|---|---|---|
| `__ckb_cell_data_hash_at` | 32 / remaining suffix / min(32, remaining) / 32 | Set request capacity to 32; reject remaining < 32 before publishing 32. A longer suffix is valid. Keep 32 used + 0 padding = 32 bytes. |
| `__ckb_cell_data_hash` | 512 / full Cell data length / min(512, length) / 32-byte digest | Reject reported length > 512 before passing the buffer to BLAKE2b. Keep 544 used + 0 padding = 544 bytes. |
| xUDT amount-word getter | 8 / remaining bytes from offset 0 or 8 / min(8, remaining) / one u64 | Require at least 8 remaining bytes before loading the word. Keep 24 used + 8 padding = 32 bytes. |
| Legacy WitnessArgs field getter | 512 / full witness length / min(512, length) / projected fixed value | Reject reported length > 512 before parsing offsets against unloaded data. Keep 592 used + 0 padding = 592 bytes. |

The v127 bounded-buffer repair is retained in v129 for compatibility. The separate
field accessor and typed `.data_hash` already used that consensus interface.
The original syscall source checks and runtime error families remain. Raw
`witness::raw` intentionally zero-pads/truncates its fixed result and is distinct
from a required complete structured field. Bounded WitnessArgs resolver/word and
exact32 reads validate ranges against the same immutable backing witness;
Script-prefix reads can allow a larger total when only the checked prefix is
used. Fixed consensus scalar fields have their protocol-defined widths. These
paths were reviewed separately instead of treating every unequal length as an
error.

The independent checker now decodes the entire Cell-data hash and slice helper
bodies, including source decoding, capacity, syscall, length comparison, output
and size pointers, error branches, saved RA and restoration. The memory BLAKE2b
callee and compression dependency retain their independently frozen complete
machine profiles. Caller effects require disjoint private 32-byte output and
eight-byte length regions and preserve live views and collection copies.
No compiler layout metadata substitutes for machine validation.

The xUDT and legacy WitnessArgs fixes have boundary VM regressions; this change
does not add a new complete independent machine profile for either family.

## Register contract

`scratch_register_avoiding` returns no register when all candidates are live,
including numeric aliases. The generator records that failure and refuses to
produce assembly or ELF. It never selects a forbidden fallback register.

Implicit large-offset loads and additions may materialize the address in their
overwritten destination when it differs from the base. Other live registers are
preserved. In-place updates and stores require structured access methods with
explicit live-register constraints. Existing dynamic field, ABI cursor,
collection and ordered-vector updates use those methods. Incoming stack
arguments continue through the structured memory-load path after register
arguments have been spilled. Large address construction with distinct output
registers also preserves both source pointers during collection operations.

The motivating implicit-clobber hazard was a code-review finding, not a
reproduced business transaction failure. The new VM regression directly checks
live-register sentinels at large offsets. No temporary spill frame or hidden
stack adjustment was introduced. The preceding stack-layout inventory remains
247 entry points / 360 adjustments and now also rejects emission errors.

## Focused validation

| Check | Result / receipt |
|---|---|
| Read boundaries and view/collection composition, O0/O3 | Five tests passed in `/tmp/cellscript-v127-focused.log`; slice suffixes 0/1/16/31/32/33/512/600, bounded hashes 0/1/511/512/513/600, amount words around 8/16 bytes and legacy witnesses at/above 512 bytes |
| Stack layout, cryptography, exact handles and typed runtime views | 5 + 8 + 17 + 12 passed in `/tmp/cellscript-v127-focused.log` |
| Independent bounded checker regressions | 28 passed in `/tmp/cellscript-v127-bounded-checker.log`; rebound mutations cover capacity, unsigned comparison, removed branch, output pointer and published-length store at O0/O3 |
| Assembler/register regressions | 54 passed in `/tmp/cellscript-scratch-assembler.log`; actual VM sentinels plus alias/exhaustion failures |
| All-helper CFG inventory and instance body-delta proof | Two passed in `/tmp/cellscript-read-bounds-inventory.log` |
| iCKB differential suite, serialized with updates disabled | 220 passed in `/tmp/cellscript-v127-ickb-verified.log`; measured artifact/context identities refreshed without increasing resource ceilings |
| Compiler and standalone checker all-target clippy, `-D warnings` | Passed in `/tmp/cellscript-v127-clippy.log` |
| Commit hygiene gate | Passed in `/tmp/cellscript-v127-commit-gate.log`; does not substitute for integration gates |

The full compiler library suite passed all 1093 tests in
`/tmp/cellscript-v127-lib.log`. The initial backend attempt on signed revision
`59c147c90dd8ae889bf5b1ee8afba464e9f8cb9e` was interrupted after a separate
transfer-resource identity check exposed stale frozen artifact identities. Its
partial log `/tmp/cellscript-v127-backend.log` is not a gate pass.

Intentional transaction resource refresh uses
`CELLSCRIPT_UPDATE_SELECTION_RESOURCES=1` with the serialized `selection_bundle`
tests (skip `frozen_transfer_resources` and `allocated_transfer_preallocation`
during collection, then rebuild and run all 19 tests without update mode).
The opt-in writer updates existing observation rows only, after the
original numeric ceilings and actual transaction outcomes have passed. It cannot
add cases or change ceilings. Rebuild and rerun the complete suite without that
variable before accepting a refresh; the compiler identity and preallocation
comparison tests must then agree with the new frozen observations.

The refresh run passed 17 measurement/behavior tests in
`/tmp/cellscript-v127-selection-refresh.log`. All 24 transfer and 35
selection/migration observations changed only `__ckb_cell_capacity`'s declared
frame (48 to 32 bytes) and the corresponding ELF SHA-256. Observed stack peaks,
cycles, ELF lengths, transaction/witness bytes, capacity and verdicts did not
change. All ceilings, sources and case keys remain identical. The structured
delta receipt is `/tmp/cellscript-v127-resource-delta.json`; the two omitted
frozen-comparison tests are included in the unfiltered verification run.
That run passed all 19 tests with update mode disabled in
`/tmp/cellscript-v127-selection-verified.log` (738.03 seconds), including
preallocation, frozen identity, cross-package, editor, migration and settlement
checks. Follow-up clippy and commit hygiene passed in
`/tmp/cellscript-v127-resource-clippy.log` and
`/tmp/cellscript-v127-resource-commit-gate.log`.

Final matching gates are still pending. Earlier v125/v126 receipts are
historical evidence and do not count as final v127 acceptance.
These finite contracts are not arbitrary whole-program equivalence, a global
stack/cycle bound or a proof of every indirect address in every legacy helper.
Issue closure is tracked by the [admission review](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md).
