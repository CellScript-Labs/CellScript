# Stack layout audit — 2026-10-27

**Status: historical v126 repair receipt; final integration acceptance pending.**

Base revision: `f34c175756b4d310812dad3dfae9bc8ff60724d3`. This review incorporates
the existing context-hash/ScriptHash composition repairs without reverting other
workspace work. The layout-only receipt below covers cache generation v126;
the later [read-boundary audit](CELLSCRIPT_READ_BOUNDARY_AUDIT.md) records v127
length checks/register handling and v128 consensus-field convergence. The latter
replaces the 544-byte buffer-hash frame below with the existing 64-byte field
accessor; the table remains the historical v126 layout comparison. No TAC or external security scan was run.
Workspace/package versions, syscall sources, business rules and borrowing are
unchanged. Statements below about pending gates and uncommitted work describe
that v126 receipt, not a completed acceptance of the later changes.

The 851-file source/test/lock receipt in
`/tmp/cellscript-stack-source-manifest-final.json` has aggregate SHA-256
`c7ee9f41ac2bbf4f8f91b56361a9e292c2a1dfb158f05cabd8f0e7a781a7edeb`.
It includes the dirty benchmark matrix and new untracked Rust files; it is a
local reproducibility receipt, not a commit or a clean-source acceptance record.

## Scope and method

The source review follows ordinary function layout, entry adapters, outgoing
stack arguments, runtime helpers, generated standard library/collection code,
stack-backed arenas and selection workspace rebasing. The relevant producers
are [frame](../src/codegen/frame.rs), [calls](../src/codegen/calls.rs),
[ABI](../src/codegen/abi.rs), [runtime](../src/codegen/runtime.rs),
[stdlib](../src/stdlib/mod.rs), [collections](../src/stdlib/collections.rs),
[arena](../src/codegen/value_arena.rs) and the selection/instance emitters.
The Rust adapter/tool crates and initialized contract/example sources were also
searched for handwritten stack manipulation; their Rust-generated machine stack
layouts remain the responsibility of their respective Rust/LLVM backends.

The new [machine inventory](../src/codegen/runtime/stack_audit.rs) generates all
literal runtime dispatch names, core support, header/input helpers and both
standard-library assembly surfaces. It assembles them, decodes the ELF with the
standalone checker, and walks the actual CFG at O0 and O3. The current inventory
contains **247 entry points and 360 stack adjustments**. Every adjustment must
belong to the entry/CFG inventory. Tests check alignment, consistent incoming
depths, direct SP-relative access extents and restored SP at every reachable RET.
Callees are separate roots; returning calls keep their caller's stack depth.
The recognized nonreturning abort sink has no restoration obligation.

This initially found the 552-byte Cell-data hash frame. Strengthening coverage
also exposed three unreachable exact-size return fragments in the legacy witness
helpers. Exact32 already delegates to a different implementation; the obsolete
fragments were removed, reducing the adjustment inventory from 363 to 360.

## Layout changes

All numbers below are bytes in **one helper's private frame**, compared with the
base revision. They are not cumulative call-chain peaks. `U` is storage actually
used by the algorithm (including retained buffers and saved values); `P` is
alignment/spacing, and `D` is a removed write-only saved slot. A separately
identified probe reservation is kept distinct from bytes actually written.
Families share one emitter, so a row can cover multiple symbols.

| Helper or emitter family | Before: used + extra = frame | After: used + padding = frame | Final layout / reason |
|---|---:|---:|---|
| Current Script / transaction hash | 16 U + 8 P = 24 | 16 U + 0 P = 16 | size pointer 0, RA 8; corrects misalignment |
| Cell data hash | 544 U + 8 P = 552 | 544 U + 0 P = 544 | length 0, 512-byte buffer 8, output 520, size pointer 528, RA 536 |
| Cell scalar field, DAO rate/input-rate/data classifier, xUDT amount word | 24 U + 24 P = 48 | 24 U + 8 P = 32 | length 8, result 16, RA 24 |
| Cell data size | 16 U + 8 probe + 24 P = 48 | 16 U + 8 probe + 8 P = 32 | length 8, separate zero-length probe address 16, RA 24 |
| Cell unoccupied capacity | 24 U + 24 P = 48 | 24 U + 8 P = 32 | saved capacity 0, source 8, RA 24 |
| Cell data hash at offset | 32 U + 48 P = 80 | 32 U + 0 P = 32 | offset/output/size/RA at 0/8/16/24 |
| Full Cell hash-field getter | 64 U + 16 P = 80 | 64 U + 0 P = 64 | length 0, hash 8, output 40, size pointer 48, RA 56 |
| Full Cell hash requirement | 56 U + 8 D + 16 P = 80 | 56 U + 8 P = 64 | length 8, hash 16, expected pointer 48, RA 56; removes unread length save |
| OutPoint word/index getter | 52 U + 28 P = 80 | 52 U + 12 P = 64 | length 8, 36-byte OutPoint 16, RA 56 |
| OutPoint hash requirement | 60 U + 8 D + 28 P = 96 | 60 U + 4 P = 64 | length 0, OutPoint 8, expected pointer 48, RA 56 |
| Complete OutPoint requirement | 68 U + 8 D + 36 P = 112 | 68 U + 12 P = 80 | previous row plus expected index 56, RA 64 |
| SHA-256 fixed core | 608 U + 16 P = 624 | 608 U + 0 P = 608 | state 0, schedule 64, input/length/output/RA 576/584/592/600 |
| SHA256d wrapper | 56 U + 24 P = 80 | 56 U + 8 P = 64 | saved pointers 8/16, RA 24, digest 32 |
| SHA-256 pair wrapper | 96 U + 16 P = 112 | 96 U + 0 P = 96 | RA 0, pointers 8/16/24, 64-byte input 32 |
| SHA256d pair wrapper | 128 U + 32 P = 160 | 128 U + 0 P = 128 | previous layout plus 32-byte digest 96 |
| SHA256d Merkle requirement | 112 U + 48 P = 160 | 112 U + 0 P = 112 | RA/arguments/counter 0..48, two hash buffers 48/80 |
| C256 product comparison | 104 U + 24 P = 128 | 104 U + 8 P = 112 | four arguments, two 32-byte products, RA 104 |
| C256 sum-of-two-products comparison | 264 U + 56 P = 320 | 264 U + 8 P = 272 | eight arguments, six 32-byte buffers, RA 264 |
| ScriptRef code-hash getter | 85 U + 75 excess buffer + 32 P = 192 | 85 U + 11 P = 96 | 53-byte prefix at 8; output/size/RA 64/72/80 |
| ScriptRef Args32 getter | 160 U + 32 P = 192 | 160 U + 0 P = 160 | 128-byte read at 8; output/size/RA 136/144/152 |
| ScriptRef hash-type getter | 69 U + 75 excess buffer + 16 P = 160 | 69 U + 11 P = 80 | length 0, 53-byte prefix 8, RA 64 |
| ScriptRef args-empty getter / requirement | 144 U + 16 P = 160 | 144 U + 0 P = 144 | length 0, 128-byte read 8, RA 136 |
| Script code-hash/hash-type requirement | 85 U + 8 D + 35 P = 128 | 85 U + 11 P = 96 | prefix 8, expected pointer/type 64/72, RA 80 |
| Script args-hash requirement | 168 U + 8 D + 16 P = 192 | 168 U + 8 P = 176 | length 0, read 8, source coordinates 136/144, expected pointer 152, RA 160 |
| Current-Script/output args-empty scan | 296 U + 24 P = 320 | 296 U + 8 P = 304 | preserves both 128-byte reads and scan state; RA moves to 296 |
| Legacy WitnessArgs field projection | 592 U + 96 excess buffer = 688 | 592 U + 0 P = 592 | 512-byte witness read retained; extracted field buffer is exactly 32 bytes; header scratch 24 bytes |
| Witness size | 16 U + 16 P = 32 | 16 U + 0 P = 16 | length 0, RA 8 |
| Process wait | 16 U + 16 P = 32 | 16 U + 0 P = 16 | initialized exit-code word 0, RA 8 |
| Spawn with one inherited FD | 72 U + 24 P = 96 | 72 U + 8 P = 80 | FD list/PID/spawn argument record unchanged; RA moves to 72 |
| DAO since lower bound | 32 U + 16 P = 48 | 32 U + 0 P = 32 | length/required/value/RA at 0/8/16/24 |
| xUDT owner Type args | 160 U + 8 D + 24 P = 192 | 160 U + 0 P = 160 | length 0, 128-byte read 8, flags/expected pointer/RA 136/144/152 |
| xUDT group amount delta | 104 U + 24 P = 128 | 104 U + 8 P = 112 | shared scan state and 16-byte amount read retained; RA 104 |
| MetaPoint relative requirement | 144 U + 8 D + 40 P = 192 | 144 U + 16 P = 160 | eight scalar slots 0..64, RA 64, two separately aligned 36-byte buffers 72/112 |
| MetaPoint pair-cardinality family | 176 U + 16 D + 32 P = 224 | 176 U + 0 P = 176 | scalar slots 0..96, two hashes 96/128, data 160, RA 168; removes unread view/length saves |
| Master OutPoint pair scan | 240 U + 8 probe + 56 P = 304 | 240 U + 8 probe + 8 P = 256 | first scalar now 0; keeps both 36-byte OutPoints and zero-length probe separate; RA 248 |
| Open handle requirement | 930 U + 62 spacing/reserve = 992 | 930 U + 46 spacing/reserve = 976 | maximum from-args variant: domain preimages 326/95/253 bytes; removes only the final 16-byte trailer |
| Instance-policy participant | 1739 U + 69 spacing/reserve = 1808 | 1739 U + 53 spacing/reserve = 1792 | maximum consumer/participant/schema/policy areas retained; policy staging ends at 1790, RA already occupies 24 |

The temporary 32-byte context-hash repair was an intermediate workspace state;
the base revision used 24 bytes. It is not counted as a shipped 32-to-16 saving.
Unused saved lengths are removed only where the parameter is checked directly
before clobber and never reloaded. Validation predicates and error codes remain.

## Retained spacing and lifetime boundaries

- `__ckb_input_out_point_tx_hash` remains **80 bytes**: 68 used + 12 spacing.
  OutPoint is 36 bytes; the output/size pointers, read length and RA remain live.
  Moving the leading eight-byte gap would not make a 64-byte frame possible.
- Occupied-capacity remains **48 bytes**: 40 used + 8 padding. The source/index,
  result length, value and RA are all read. Ordinary standard-library frames of
  16/32/48 bytes retain their eight-byte alignment padding.
- The ordinary function layout keeps separate fixed-byte return buffers and
  length/pointer slots, scalar locals and operation-specific buffers. It also
  **unconditionally reserves 128 bytes of expression temporaries plus 1040 bytes
  of paired runtime scratch**, including in functions that do not use all of it.
  This is a conservative shared contract, not alignment padding and not evidence
  that every reserved byte is read. `runtime_expr_temp_offset` and the scratch
  accessors derive their addresses from the final frame tail; commitment and
  Cell-data checker contracts independently use that convention. This audit
  preserves it rather than introducing a second, incomplete demand classifier
  for all emitter paths. Demand-based ordinary-function scratch and lifetime
  coalescing remain explicit optimization work; the repository is not claimed
  to have globally minimal stack usage.
- The legacy 5376-byte entry frame retains its explicitly documented **208-byte
  v1 reserved area**. The compact selected-entry path already computes its own
  smaller private frame. Removing ABI reserve is a separate compatibility change.
- Arenas and nested values occupy reserved function-frame regions. Growth uses
  those regions; it does not make a general dynamic `alloca`. Several live owners
  and nested calls still add to the stack peak. Selection workspace rebasing is
  aligned and paired, with terminating failure paths treated separately.
- Outgoing ABI words use the dedicated outgoing region below the caller's SP.
  Caller-local pointers are formed before the adjustment; callee incoming words
  use the parent frame base. The large-owner/fixed-result regression crosses the
  register/stack argument boundary and retains earlier copied bytes.
- Open-handle domain staging has internal fixed-capacity spacing. Only its
  completely unused final 16 bytes were removed; repacking cryptographic domain
  workspaces would require a separate lifetime/alias analysis.

## Common enforcement and independent contracts

`align_up`/`align_frame` can now be evaluated in constants. Handwritten runtime
SP changes use `emit_stack_adjustment`; its underlying large-immediate path
requires an SP-relative multiple of 16 and never silently rounds. Helpers share
their frame value across allocation/restoration, including constant-expression
and variant-sized layouts. Repaired accesses use the existing stack load/store
and address emitters, preserving their large-offset/scratch rules.

The standalone [stack validator](../crates/cellscript-artifact-checker/src/checker.rs)
independently rejects every decoded unaligned adjustment, even when allocation
and restoration cancel out. Existing range, CFG, source, pointer and lifetime
checks remain. Its full hash-field, scalar-property and ScriptRef finite-body
contracts use independently written offsets; they do not import compiler layout
definitions or trust frame metadata. Body extents, branch destinations, syscall
fields, exact output lengths, saved slots and restoration instructions are checked.

The instance-policy profile also authenticates whole helper bodies. Its instance
and open-handle identities were repinned only after the new machine-delta test
reconstructed the old accepted body hashes by restoring exactly the allocation
and restoration immediates. All other decoded instruction words and normalized
callee identities must remain identical. The independent checker continues to
compare against frozen full-body constants; no metadata-trusting shortcut or
generalized acceptance rule was added.

The accompanying pre-existing composition repairs retain ScriptHash across typed
Cell/Script projections and bind context/Script/OutPoint/WitnessArgs caller writes
to disjoint 32-byte output and eight-byte length regions. These are documented in
the [bounded admission review](CELLSCRIPT_BOUNDED_RUNTIME_ADMISSION.md).

## Validation receipt and limits

Completed terminal results on the current repair:

| Check | Result | Local log |
|---|---|---|
| O0/O3 all-helper machine inventory + instance body-delta proof | 2 tests passed, 247 entries / 360 adjustments at each level | `/tmp/cellscript-stack-inventory-final-v126.log` |
| `stack_layout` | 5 passed at O0/O3, including 270 incoming arguments across FP+2040/2048 | `/tmp/cellscript-stack-layout-final.log` |
| `crypto_primitives`, `typed_runtime_views`, `cryptographic_resource_budgets` | 8 + 12 + 1 tests passed; the last test covers six resource profiles | `/tmp/cellscript-stack-behavior-final.log` |
| Artifact checker `bounded_` filter | 27 passed, including six admission/mutation tests and scalar/hash property tests | `/tmp/cellscript-stack-bounded-checker-final.log` |
| Full `ickb_diff`, serialized and with update mode disabled | 220 passed | `/tmp/cellscript-stack-ickb-verified.log` |
| Ordinary frame and assembler unit tests | 8 + 52 passed | `/tmp/cellscript-stack-frame-tests.log`, `/tmp/cellscript-stack-assembler-tests.log` |
| Full exact/open/instance handle runtime suite | 17 passed after the final instance repair | `/tmp/cellscript-stack-instance-behavior.log` |
| Instance closed-profile mutation regression | passed, including over 200 branch/byte/constant mutations and transitive dependency mutations | `/tmp/cellscript-stack-instance-mutations.log` |
| Original runtime-provenance rejection regression | passed | `/tmp/cellscript-stack-provenance-final.log` |
| Full `allocated_runtime`, serialized | 42 passed, including short/wide/tuple returns and copied-value lifetimes | `/tmp/cellscript-stack-allocated-runtime.log` |
| Standalone checker crate | 75 passed; one producer-dependent counter-set test remains explicitly ignored | `/tmp/cellscript-stack-standalone-checker.log` |
| Compiler + standalone checker all-target clippy, `-D warnings` | passed | `/tmp/cellscript-stack-clippy-final.log` |
| `cellscript_gate.sh commit` | passed; no commit performed | `/tmp/cellscript-stack-commit-gate-final.log` |
| Strict backend `full` component audit | 13 commands passed; only production stateful acceptance failed its clean-tree prerequisite | `/tmp/cellscript-stack-strict-backend-final.log` |
| Syntax combination audit (`ci` mode, within strict audit) | 206 cases, 118 accepted / 88 correctly rejected, zero failures | `target/syntax-combo-audit/20261027-115641-ci-20260503/report.json` |

Rebound mutation tests check
misaligned but balanced frames, wrong saved-RA slots, over-restoration, copy
lengths, output pointers, syscall fields and witness resolver bounds.

Resource ceilings were not raised. Runtime-view observations retained their
numeric values while updating identities. The trusted external execution profile
changed from 79,836 cycles / 1,408 ELF bytes to 79,832 / 1,400. The benchmark
submodule's matrix was refreshed with the existing measured-evidence updater,
then all 220 tests were rerun without it. Success/failure verdicts remain the
same. In two hostile xUDT fixtures the first reported rejecting group switches
between the input Type group (48) and output Type group (-52); the receipt
preserves those actual group/error observations instead of forcing the old code.
Auxiliary code/Script hashes and dependent normalized fixtures were refreshed
together. The submodule remains dirty and uncommitted. The business-corpus
inventory digest was regenerated; candidate/release-pending flags are unchanged.

The first backend attempt stopped at the stale corpus digest
(`/tmp/cellscript-stack-backend.log`). That was repaired. The second attempt was
terminated during compiler unit tests when review found the remaining
instance-policy trailer and its closed machine-profile dependency. It is partial
evidence only (`/tmp/cellscript-stack-backend-retry.log`), not a full backend pass.
The instance-specific repair has now passed its body-delta, VM and mutation
checks. A third backend run was stopped during compiler unit tests to investigate
the incoming-argument immediate boundary; it is also incomplete
(`/tmp/cellscript-stack-backend-final.log`). That suspicion was disproved: the
existing central emitter already expands large offsets, and O0/O3 VM regressions
with 270 parameters pass. No incoming-argument compiler repair was needed.

The production stateful acceptance component requires a clean committed source
tree. This round explicitly retains uncommitted edits, so no final clean-source
acceptance or design freeze can be claimed. The actual production stateful
entrypoint was run and rejected with `production acceptance requires a clean
CellScript source tree` (`/tmp/cellscript-stack-stateful-blocked.log`). The full
backend gate has not completed; neither `dev` nor `ci` ran. A pass in a focused suite must not be
reported as a complete backend/dev/CI gate pass.

The strict-backend `full` component was separately run to completion. All 13
non-production commands passed, including outgoing ABI words 0 through 20,
instruction/branch contracts, fail-closed helper checks and the 206-case syntax
matrix. The only failed command is the clean-tree stateful prerequisite above.
Its terminal report is
`target/cellscript-strict-backend-audit/strict-backend-audit-full-20261027-205303.json`.
This component result does not substitute for the interrupted full compiler-test
phase or the remaining top-level backend components. Earlier focused receipts
precede the final instance-only frame change where noted; that change has its own
full-body delta, 17-case VM and mutation receipts. No single complete backend
receipt for the final source is claimed.

Limits: direct SP-access inventory is not arbitrary indirect-pointer range
analysis. Finite checker contracts and VM fixtures provide additional evidence
for their covered paths, not a proof for every combination or third-party Rust,
C, assembly or dynamically loaded child. The audit does not establish arbitrary
source/machine equivalence, whole-program worst-case stack/cycles, or semantic
completeness of legacy collection prototypes. Per-helper frame savings must not
be added across sequential calls or presented as proportional cycle savings.

A separate coverage probe confirms an existing limitation: a rebound incoming
argument load mutation in a plain scalar-only helper is outside the current
bounded typed-machine binding coverage. The same large-argument mutation in a
helper borrowing a collection is rejected with V2420 at O0/O3, and this is now
a regression. Generic scalar-only parameter/value equivalence remains uncovered;
the positive stack-discipline result must not be read as that stronger claim.
