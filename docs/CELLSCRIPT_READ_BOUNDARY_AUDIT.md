# Read boundary and scratch-register audit — 2026-10-27

**Status: repaired candidate; final identified-source gates pending.**

This follows the [stack-layout audit](CELLSCRIPT_STACK_LAYOUT_AUDIT.md) on base
`f34c175756b4d310812dad3dfae9bc8ff60724d3`. Current artifact-cache generation is v128.
No release/version change, TAC or external security scan is included.

## Consensus-field convergence (v128)

The whole-data accessor now lowers to the existing
`__ckb_cell_data_hash_field`, shared by `ckb::cell_data_hash`,
`ckb::cell_data_hash_field` and typed Cell `.data_hash`. The duplicate
512-byte read and memory-hash dependency are removed. The private helper frame
changes from **544 used + 0 padding = 544** to **64 used + 0 padding = 64**:
length at 0, 32-byte result at 8, caller output pointer at 40, caller size pointer
at 48, and RA at 56. This saves 480 bytes in that helper alone; it does not claim
a measured whole-program peak reduction. Caller-owned result/size buffers remain.

The oracle is CKB's
[CellOutput::calc_data_hash](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/gen-types/src/extension/calc_hash.rs),
used by the
[DATA_HASH syscall](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/script/src/syscalls/load_cell.rs).
Empty data returns zero bytes, not BLAKE2b(empty); data larger than 512 bytes is
supported. Source decoding remains unchanged. Invalid/missing Cell sources and
failed field reads use the existing field accessor's error 44; the removed
buffer implementation used error 3 for failed reads. Null output/size pointers
still fail with error 18. This is an intentional semantic correction, not merely
a layout optimization.

Raw 32-byte projection is named `ckb::cell_data_bytes32(view, offset)`;
`cell_data_hash_at` is a compatibility alias with identical machine output.
It retains the Hash value representation but performs no hashing. Both spellings
emit the same 32-byte-frame helper, request exactly 32 bytes, require at least
32 remaining bytes, and publish 32 only after that check. Metadata names the
canonical operation `cell-data-bytes32`; whole-data hashing records the existing
`cell-data-hash-field` / `LOAD_CELL_BY_FIELD` contract. Custom-range hashing
continues to use the separate bounded incremental span interface.

The independent checker validates the complete field/slice machine bodies even
for direct calls outside collection owners, including source decoding, syscall,
field selector, returned length, copy bounds, output pointers and restoration.
It continues to prove caller buffer disjointness in composed lifetime checks.
Disabled targets may only return the established error without touching memory.

Focused v128 checks completed:

| Check | Receipt |
|---|---|
| Six read-boundary/composition tests, O0/O3 | `/tmp/cellscript-v128-crypto-read.log`: empty/1/511/512/513/600/4096-byte consensus hashes across all five Cell sources; raw short slices and longer suffixes; invalid tags/HeaderDep/missing coordinates; alias and formatter machine parity; live Cell/collection copies |
| Five stack-layout behavior tests, O0/O3 | `/tmp/cellscript-v128-read-stack.log`: 64-byte field frame, nested fixed returns, large frames/arguments and preserved legacy behavior |
| Rebound machine mutations, O0/O3 | `/tmp/cellscript-v128-checker.log`: request capacity, omitted/inverted length guard, output and size writes, saved RA restore, syscall, field selector, copy bound and source decoder |
| Complete runtime/stdlib machine inventory | `/tmp/cellscript-v128-inventory.log`: 246 entries / 358 stack adjustments at O0/O3, plus instance profile delta test; removing one duplicate helper accounts for the 247/360 reduction |
| Editor and browser metadata-only boundary | `/tmp/cellscript-v128-editor.log` and `/tmp/cellscript-v128-browser.log`: canonical completions, alias features, invalid-offset diagnostics; no native access/machine records added to browser summaries |
| Cryptographic regression | Eight tests in `/tmp/cellscript-v128-crypto-read.log`, including unchanged exact-range/empty-span hashing |
| Compiler/checker and WASM clippy, all targets, warnings denied | `/tmp/cellscript-v128-clippy.log` and `/tmp/cellscript-v128-wasm-clippy.log`; native and WASM feature configurations checked separately |
| iCKB differential suite, serialized and updates disabled | 220 tests in `/tmp/cellscript-v128-ickb.log`; no further baseline or numeric-ceiling changes |

Final identified-source backend/dev/ci validation remains pending. Prior receipts
below remain evidence of the earlier repairs, not final acceptance of this change.

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

The v127 hash repair retained the bounded-buffer design; v128 supersedes that
choice with the consensus-field implementation described above. The separate
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
