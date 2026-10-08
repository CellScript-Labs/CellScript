# Second bounded scalar-retention experiment (#39)

Decision: **reject this candidate for the 0.32 default** and retain the existing
stack protocol and immediate stack-reload elimination. This is a finite measured
engineering decision, not a claim that all register retention is useless.

Implementer: Codex. The maintainer's October 3 waiver applies to #39. No independent
human review is claimed. The independent checker and frozen cost budgets remain
required. This patch is archived for reproduction, not applied to the default
backend.

## Candidate and machine contract

The earlier binary-left-only candidate found no improvements across 1,707
executions. This second experiment adds eligible scalar `LoadConst`/`LoadVar`
producers and recognizes a retained **right** operand, preserving operand order
with `mv t1, t0` before loading a different left operand. It retains only one
SSA value in `t0` between adjacent instructions in one basic block, in functions
already admitted by conservative scalar-slot allocation, at optimization levels
above zero. Calls and unclassified instructions invalidate the fact; block-local
state cannot cross joins/backedges. All stack stores and distinct parameter spills
remain. Explicit avoid sets protect a retained operand during large-offset loads.
Wide/resource/borrow/address-exposed exclusions and outgoing arguments are unchanged.
The pre-implementation contract is included in both replay patches as
`docs/CELLSCRIPT_SCALAR_RETENTION_EXPERIMENT.md`.

The existing `eliminate_immediate_stack_reloads` already removes an immediately
redundant same-register reload across comments only. Consequently, exercising a
left-retention source path does not necessarily improve the final machine code.
The second candidate's measured benefit comes from replacing one right-operand
load with one register move.

## Results

| Evidence | Result |
| --- | --- |
| Identical-IR, identical-options paired probes | 35 pairs; 25 save one group cycle, ten tie; no ELF-size, store-count or observed-stack increase |
| Full frozen main and multi-Script comparator | 6,782 comparable metrics pass, zero regressions; Rust references and ceilings unchanged |
| Full main report | Matched Rust, growth and expanded-policy summaries are identical; two of nine scalar summaries improve |
| Scheduler attribution | 1,707 paired executions: four save one group cycle, 1,703 tie; availability and exact exits preserved |
| Executed traffic in the four changed rows | One fewer guest `LD`, one additional `ADDI` register move; stores and observed stack unchanged |

The changed corpus rows are:

| Fixture | Valid group cycles | Rejection group cycles | ELF bytes | Observed stack bytes |
| --- | --- | --- | --- | --- |
| `scalar-overlap-64` | 4,435 → 4,434 | 4,412 → 4,411 | 3,560 → 3,560 | 7,088 → 7,088 |
| `scalar-overlap-260` | 8,645 → 8,644 | 8,614 → 8,613 | 10,024 → 10,024 | 8,656 → 8,656 |

Each changed scalar artifact has one fewer static load and the same static
store count. These are per-case measurements, not transaction-frequency-weighted
savings. Guest access counts do not measure host syscall traffic. Observed peaks
are not all-input stack bounds. Archive compression is not deployed ELF reduction.

The isolated probes cover subtraction/division/remainder/shifts, exact division
and shift failures, signed/narrow normalization, overlapping values, duplicate
SSA operands, joins/backedges, a preserved local call, nine outgoing arguments,
wide-value fallback and a frame larger than 2,048 bytes. Tests explicitly require
intended retention hits and preserve calls against inlining. The duplicate-SSA
probe intentionally mutates typed IR; its report identifies that mutation and
does not claim source equivalence for it.

## Why retain the current backend

The measured improvement is one cycle on two scalar fixtures, without reducing
ELF bytes, stores or stack. That benefit does not justify adding another
cross-instruction producer/consumer register contract alongside the existing
reload elimination for this release. This is a judgment about this measured
candidate, not a universal adoption threshold. No general allocator, dead-store
elimination, parameter-spill coalescing or helper-scratch reuse is admitted.
Conservative helper ownership remains necessary until helper clobbers, escaped
addresses and live scratch regions have explicit contracts and executable evidence.

## Reproduction and provenance

The two experiment snapshots start at `35bf983db30aae80281f97e30bbce08a878d7c58`.
Use a fresh checkout for each patch and apply it without staging its new files;
this preserves the recorded tracked-diff and untracked-file provenance split.
The focused probe snapshot and later complete-corpus snapshot differ by additional
interface-inspection and client work, which the patches retain exactly. Those
changes are not claimed as cost optimizations. `manifest.json` records hashes,
uncompressed byte counts and source identities. The baseline replay has its own
source patch at `622c850550428bbe000017c1b55944a9248717cd`.

The test-only toggle compares retention off/on on identical IR and options; no
production CLI flag was added. The full corpus uses the candidate's ordinary
compiler path. Its report provenance must not be relabeled as a clean commit or
as the baseline CI source.

```bash
git apply probe-source.patch
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked -p cellscript scalar_retention_tests --lib -- --test-threads=1
```

In a separate fresh checkout of the same base:

```bash
git apply corpus-source.patch
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked -p cellscript --test cost_corpus -- --test-threads=1
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked -p cellscript --test business_corpus multi_script_cost_accounts_for_each_group_and_rejection -- --exact --test-threads=1
CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=1 cargo test --locked -p cellscript --test cost_attribution -- --test-threads=1
```

The archived comparator retains its actual input paths and identities; decompress
its four reports and invoke the existing `cellscript-tools compare-cost-evidence`
consumer to reproduce the comparison. The candidate passed these focused/corpus
checks; no full candidate backend/CI gate is claimed. It is rejected, so no new
machine contract is adopted. Full CI passed for the retained implementation plus
the initial interface-inspection groundwork; historical backend/CI receipts for
the retained research baseline remain in `../validation.json` with their original
identities. Release and public deployment admission remain separate.
