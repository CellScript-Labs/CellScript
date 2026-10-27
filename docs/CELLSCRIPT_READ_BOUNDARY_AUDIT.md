# Read boundary and scratch-register audit — 2026-10-27

**Status: repaired candidate; final identified-source gates pending.**

This follows the [stack-layout audit](CELLSCRIPT_STACK_LAYOUT_AUDIT.md) on base
`f34c175756b4d310812dad3dfae9bc8ff60724d3`. Artifact-cache generation is v127.
No release/version change, TAC or external security scan is included.

## Read contracts and reproduced defects

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

The hash helper deliberately retains its existing bounded-buffer design. It does
not silently change to a consensus-field syscall or add streaming semantics.
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

The full compiler suite and matching gates are still pending. Earlier v125/v126
receipts are historical evidence and do not count as final v127 acceptance.
These finite contracts are not arbitrary whole-program equivalence, a global
stack/cycle bound or a proof of every indirect address in every legacy helper.
Issue closure is tracked by the [admission review](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md).
