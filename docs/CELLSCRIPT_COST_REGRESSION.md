# Cost regression contract

The 0.30 CI, backend, release and release-quick gates require executed cost
evidence. These are bounded regression samples, not a cost guarantee for every
program or proof of arbitrary Rust equivalence. The witness ABI is unchanged.

## Required tools and reports

`tests/cost_corpus.rs` and `tests/artifact_size.rs` require the selected Rust
toolchain's `riscv64imac-unknown-none-elf` core libraries and `llvm-strip` from
its `llvm-tools-preview` component. Missing tools, a failed compiler query, or
a strip executable that fails its version check stop the test. A system
`llvm-strip` on PATH is not used. These components are declared in
`rust-toolchain.toml` and installed by CI.

The gates reset `CELLSCRIPT_COST_CORPUS_REPORT` before testing and require its
completion marker after compiler tests. The default gate report is
`target/cellscript-cost/cost-corpus-report-<mode>.json`; focused runs use
`target/cellscript-cost/cost-corpus-report.json`. The corpus also invalidates
its previous report before measuring. Only completion of all matched and
growth rows, including rejection cases and budgets, writes `status: passed`.
CI and release workflows retain the JSON report as an artifact. The report
names the strip tool, compilation profile, measurements and enforced budgets.

## Matched samples

Both implementations must retain their existing accept/reject outcomes. The
CellScript byte size and positive cycles must remain no greater than the
matched Rust reference **and** below independent absolute ceilings. This
prevents a regression on both sides from hiding behind their ratio.

| Sample | Measured ELF / ceiling (bytes) | Measured positive cycles / ceiling |
|---|---:|---:|
| Pool merge | 2,512 / 2,600 | 6,000 / 6,200 |
| Schema roll | 2,272 / 2,400 | 8,661 / 9,000 |
| Ownership-claim Lock | 2,232 / 2,350 | 5,583 / 5,800 |

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
