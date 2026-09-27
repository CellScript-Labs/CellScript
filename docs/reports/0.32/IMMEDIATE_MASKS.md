# 0.32 low-mask immediate construction

Scope: [#41](https://github.com/CellScript-Labs/CellScript/issues/41), a bounded
implementation slice of [#40](https://github.com/CellScript-Labs/CellScript/issues/40).
Authorized September 27, 2026. Baseline: clean commit
`54d639a7230060cc0bcc009dcf26e1830e2011c6` (same tree as `648bb836`).
Implementer: Codex. This record is engineering evidence, not independent
security review, release approval, or deployment evidence.

## Decision and boundary

Keep the 0.31 short forms, signed-12-bit recursive construction, and validated
legacy fallback. For a nonempty low-bit mask other than all ones, also consider
`addi rd, zero, -1; srli rd, rd, leading_zeros(mask)`. Adopt it only if the exact
64-bit result is validated and it is strictly shorter. Ties retain the 0.31
sequence. The shift is always 1..63. Masks with 31..63 low set bits improve
from three instructions to two; smaller masks retain their previous encoding.

The candidate uses no extra register and no new instruction-set extension.
Source editions, wire formats, lowering record v9, metadata schema 72 and the
fixed 20-byte entry trampoline stay unchanged. The independent checker already
allows SRLI and interprets it in policy-tag constant tracking. Advance the
incremental artifact cache to `project-source-set-v55-0.32-low-mask-immediates`
so unchanged package versions cannot return pre-optimization cached artifacts.
Changed ELF/sidecar/transaction identities must be regenerated; historical
deployments retain their original identity.

This slice does not implement a general shortest-sequence search, ADDIW,
constant pools, cross-instruction register reuse, compressed instructions,
dispatch alternatives or witness-copy changes. No global optimum is claimed.

## Focused execution evidence

`low_mask_encoding_saves_one_executed_instruction_per_materialization`
compares the explicit 0.31 sequence for `0x7fffffff` with the actual new `li`
encoding. Every materialization is compared against an independent `.rodata`
value in CKB-VM2 using the pinned cost model. A golden encoding assertion
requires exactly `0xfff00293; 0x0212d293` for register t0.

| Materialization sites | Whole ELF bytes, old → new | VM cycles, old → new |
| --- | ---: | ---: |
| 1 | 488 → 488 | 518 → 517 |
| 8 | 688 → 656 | 581 → 573 |
| 64 | 2,256 → 2,000 | 1,085 → 1,021 |

Each site removes four instruction bytes and one executed cycle. Whole-file
alignment absorbs the four-byte saving in the one-site fixture. These are
synthetic VM programs, not whole-transaction or application speedup claims.

The full-width unit corpus retains its 100,000 deterministic samples and checks
non-growth against both legacy and 0.31 recursive plans. All 63 low-mask widths
are checked explicitly. Encoded VM tests cover neighboring values, complement
patterns, endpoints, zero destination and relaxed branches.

`low_mask_policy_tags_reject_rebound_shift_and_seed_mutations` exercises
`0x7fffffff` and `0xffffffff` in both editions at optimization levels 0..3.
Both the precheck and final dispatch must use the short sequence. The existing
independent checker must reject changed seeds, neighboring shifts and arithmetic
right shifts after artifact/sidecar hashes and block digests are rebound.

## Corpus comparison and validation

The unchanged cost corpus and multi-Script companion are measured separately
on the baseline and candidate, in distinct worktrees/build directories.
Frozen fixtures, Rust references, acceptance rules and budgets are retained.
The archived comparison passes 6,782 comparable metrics with no regression.
Both report pairs declare clean source. The baseline is the commit above;
the measured implementation is `0e544ce1858288c09758617adc929b31a38cd2a4`.
The later evidence-packaging and acceptance-recipe refresh do not change the
compiler implementation or frozen cost fixtures. Raw reports are retained as
`baseline.json`, `baseline-multi.json`, `candidate.json`, `candidate-multi.json`
and `comparison.json`; `SHA256SUMS` binds those files from the repository root.

| Existing fixture | ELF bytes, before → after | Worst successful transaction cycles | Selected-group rejection cycles |
| --- | ---: | ---: | ---: |
| High tags, 8 actions, 32-byte argument | 8,112 → 8,104 | 13,221 → 13,218 | 11,523 → 11,520 |
| High tags, 32 actions, 32-byte argument | 23,472 → 23,464 | 17,205 → 17,202 | 15,507 → 15,504 |
| High tags, 64 actions, 32-byte argument | 44,152 → 44,152 | 22,567 → 22,566 | 20,869 → 20,868 |

The other 68 main-report summary rows and all six multi-Script cases retain
their cost measurements. Stack reservations and witness formats do not change.
Whole-file alignment and branch relaxation mean file-byte and execution-cycle
deltas need not equal four bytes/one cycle times the number of rewritten sites.

The separate fixed transaction/header/temporal runtime-view fixture improves
from 9,896 to 9,864 ELF bytes and from 18,329 to 18,313 successful transaction
cycles. Its acceptance budget is unchanged. Authorization, fungible, NFT and
temporal scenario fixtures are replayed to refresh changed artifact/sidecar and
transaction hashes while preserving their outcomes, error codes and inputs.

The compile-only CKB acceptance matrix independently regenerated 69 artifacts.
Exactly three transaction-recipe identities changed: timelock `request_release`,
`execute_release` and `can_unlock_lock`. Their 22 code-hash references and 13
derived full-Script-hash references in owner fields and witness arguments were
rebound while preserving inputs, field layouts, expected results and budgets.
The live replay rejected a partial refresh that left the old owner identities;
a native `ckb-types` regression now derives the current Script hash and checks
both timelock release actions' owner fields and witness parameters before replay.
Historical transaction keys remain replay identifiers: the existing
replayer replaces them with actual newly submitted transaction hashes.

## Reproduction

Use separate checkouts with the pinned SDK sibling, Rust toolchain and strip
tool. At the baseline and candidate revisions, run the same existing commands:

```bash
cargo test --locked -p cellscript --test cost_corpus -- --test-threads=1
cargo test --locked -p cellscript --test business_corpus multi_script_cost_accounts_for_each_group_and_rejection -- --exact
```

The output paths default to `target/cellscript-cost/cost-corpus-report.json`
and `target/cellscript-cost/multi-script-cost.json`. Compare both pairs with
the existing native `cellscript-tools compare-cost-evidence` command, using
the baseline pair first and the candidate pair second. The command rejects
fixture/budget drift, changed outcomes, missing measurements and regressions.
Focused encoding measurements are reproduced by:

```bash
cargo test --locked -p cellscript --lib low_mask_encoding_saves_one_executed_instruction_per_materialization -- --nocapture
cargo test --locked -p cellscript --test policy_artifact_checker low_mask_policy_tags_reject_rebound_shift_and_seed_mutations -- --exact
```

The implementation commit passed `dev` and `ci`. The final evidence/recipe
commit is checked with `dev`, `ci` and `backend`; exact results and commit
identities are recorded in [#41](https://github.com/CellScript-Labs/CellScript/issues/41).
The backend gate includes live local-node stateful replay. Compile-only evidence
alone is not that replay, and neither is a release publication. The microbenchmark
is not a substitute for the full corpus.
