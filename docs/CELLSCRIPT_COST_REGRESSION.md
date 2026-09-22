# Cost regression contract

The CI, backend, release and release-quick gates require executed cost
evidence. These are bounded regression samples, not a cost guarantee for every
program or proof of arbitrary Rust equivalence. The witness ABI is unchanged.

## Required tools and reports

`tests/cost_corpus.rs` and `tests/artifact_size.rs` require the selected Rust
toolchain's `riscv64imac-unknown-none-elf` core libraries and `llvm-strip` from
its `llvm-tools-preview` component. Missing tools, a failed compiler query, or
a strip executable that fails its version check stop the test. A system
`llvm-strip` on PATH is not used. These components are declared in
`rust-toolchain.toml` and installed by CI.

The 0.31 measurement contract is `cellscript-cost-corpus-v2`. Gates reset
`CELLSCRIPT_COST_CORPUS_REPORT` and `CELLSCRIPT_MULTI_SCRIPT_COST_REPORT` before
testing. After compiler tests, `cellscript-tools check-cost-evidence` validates
both reports, their measurement availability, fixture coverage, and frozen
ceilings. The default main report is
`target/cellscript-cost/cost-corpus-report-<mode>.json`; focused runs use
`target/cellscript-cost/cost-corpus-report.json`. The corpus also invalidates
its previous report before measuring. Only completion of all matched and
growth rows, including rejection cases and budgets, writes `status: passed`.
CI and release workflows retain the JSON report as an artifact. The report
names the strip tool, compilation profile, measurements and enforced budgets.

The companion `cellscript-multi-script-cost-v2` report executes the canonical
four-artifact, five-Script-group transaction and its five rejection mutations.
Its default gate path is `target/cellscript-cost/multi-script-cost-report-<mode>.json`.
Each group is replayed independently, including groups the ordinary verifier
might not reach after an earlier rejection. A rejected transaction therefore
has no reported transaction-total cycles. Its group measurements remain useful,
but adding them does not reconstruct the verifier's rejected execution path.

## Measurement availability and stack scope

The ordinary transaction verifier remains the accept/reject oracle. Successful
transaction totals retain their existing meaning. Group cycles come from the
locked CKB scheduler's `detailed_run`, including child VM execution, and retain
ordinary nonzero exits. Trap, setup, and cycle-limit failures without a reliable
count are `unavailable` with a reason. Zero and the configured cycle limit are
never substitutes for a measurement. Every required budget row needs an actual
positive count.

`max_stack_frame_bytes` is one function's largest frame.
`static_call_chain_stack_bound_bytes` is a separate static single-VM bound from
the decoded machine CFG. It includes simultaneous caller/callee frames and
temporary outgoing argument reservations. Recursion, unresolved transitions,
inconsistent stack joins, and external EXEC/SPAWN produce an unknown bound.
Separate Script VMs do not share a call stack. Neither metric is an observed
stack high-water mark.

## Expanded 0.31 baseline

The frozen [policy fixture list](../tests/fixtures/cost_corpus/expanded_fixtures.json)
has 41 rows: 21 dense-tag combinations of 1/2/4/8/16/32/64 actions and
8/32/128-byte arguments; six sparse/high-tag combinations at 8/32/64 actions;
and 14 first/last-record combinations at 1/2/4/8 records with matching or mixed
layouts and an admitted common check. Every action has a successful and a late
rejection case. Additional cases cover unknown tags, truncated/extra arguments,
truncated bundles, malformed final records, and whole WitnessArgs of 4,096 and
4,097 bytes, including optional lock/output fields.

Nine scalar/call fixtures cover short and long lifetimes, overlapping values,
loop joins, stack addresses crossing 2,040/2,048 bytes, three nested calls with
nine arguments, and repeated/distinct generic instantiations. Their ceilings
and the policy/multi-Script ceilings are checked-in measurements taken before
the optimization pass. Tests never rewrite these files. Source and ELF hashes,
compiler commit and dirty state, lockfile hashes, VM configuration, and tool
identities accompany the measurements. These are maxima over named fixtures.

## Matched samples

Both implementations must retain their existing accept/reject outcomes. The
CellScript byte size and positive cycles must remain no greater than the
matched Rust reference **and** below independent absolute ceilings. This
prevents a regression on both sides from hiding behind their ratio.

| Sample | Measured ELF / ceiling (bytes) | Measured positive cycles / ceiling |
|---|---:|---:|
| Pool merge | 2,312 / 2,600 | 5,924 / 6,200 |
| Schema roll | 2,272 / 2,400 | 8,661 / 9,000 |
| Ownership-claim Lock | 1,992 / 2,350 | 5,487 / 5,800 |

The Rust build remains no_std, ckb-std 1.1.0, opt-level z, thin LTO, one
codegen unit and aborting panics. The separate artifact-size test retains its
ELF and LOAD-size ceilings.

## Bounded growth

Edition 2027, opt-level 3 fixtures cover 18 rows:

- Three single-action controls with fixed-byte witness widths 8, 32 and 128.
- Twelve persistent Type policy combinations: 1, 2, 4 or 8 actions at each
  width. Every action checks the selected Cell's amount and compares the full
  witness against its stored fixed-byte value before consumption. Every
  action runs with a valid value and a rejected last-byte mutation.
- Three bounded input groups with bounds 1, 4 and 16. Each runs at its bound,
  rejects one extra Cell, and rejects a zero amount in the last Cell.

Every compiled growth artifact passes independent artifact validation. The
checked-in [growth budgets](../tests/fixtures/cost_corpus/growth_budgets.json)
cap ELF bytes, positive cycles and the largest recorded individual stack frame.
Initial ELF/cycle ceilings allow 5% over the measured baseline, rounded up to
64 bytes / 100 cycles. Frames are capped at the measured 5,376 bytes; this is
**not** a measurement of peak dynamic stack use. Witness sizes are exact.
Budgets are static: tests never regenerate them. Changing a budget requires
review of the source and measured behavior.

For the 32-byte witness sample:

| Form | ELF bytes | Maximum positive cycles | WitnessArgs bytes |
|---|---:|---:|---:|
| Single action | 1,848 | 4,489 | 60 |
| Policy, 1 action | 3,984 | 11,163 | 137 |
| Policy, 2 actions | 5,080 | 11,441 | 137 |
| Policy, 4 actions | 7,280 | 11,999 | 137 |
| Policy, 8 actions | 11,680 | 13,115 | 137 |

The single-record policy witness adds exactly 77 bytes at all three widths,
independent of action count. Dispatch cycles report the maximum across all
declared actions, including the last action. The bounded-group ELF remains
1,712 bytes at all three bounds; measured cycles are 4,398, 11,337 and 39,093.
These cases expose dispatch and fixed-buffer costs without changing protocol
framing or removing validation checks. Multirecord policies, arbitrary
composition, and dynamic call-stack peaks need their own evidence.
