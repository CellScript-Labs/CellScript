# 0.31 cost evidence and reproduction

This package closes the reproducibility and static memory-count reporting work
for issues #34 and #35. It does not constitute independent boundary review or
authorize a release. The compiler's source editions and witness formats remain
unchanged; native artifact checking uses lowering record v9.

## Baseline identity

The original September 21 reports declared dirty source. Their full source
snapshot was not retained, so they are historical measurements, not the
reproduction baseline supplied here. Do not retrospectively relabel them clean.

The new baseline is commit `65df937f45c6ba5ab45cf5ee69778bfa3e05cdc5` plus
[baseline-replay.patch](baseline-replay.patch). The patch restores `src/codegen`,
`src/verified_artifact.rs`, `src/lib.rs`, and the independent artifact checker to
the published 0.30 commit `352b4950e7dc7489a4ee7e7ed6d6edf4c7fefd71`, before D2–D5.
This retains 0.30's final runtime-exit and relaxed-branch checker corrections;
the earlier decision commit cannot validate the full 32/64-action corpus.
The replay retains the frozen
cost fixtures, measurement harness, Rust references and budgets. It adds the
same static memory counter as the candidate. The policy structural assertion
expects zero shared decoders in the baseline; no fixture, acceptance oracle,
measurement formula or budget is weakened. Baseline artifacts use lowering v8.

The baseline is intentionally reported as dirty. Applying the patch with
`git apply --unidiff-zero --intent-to-add` against that exact base commit
reproduces its tracked-diff digest and leaves no
untracked source. The reports bind that digest, the base commit, compiler and
reference lockfiles, Rust/strip versions and hashes, target, edition, optimizer,
VM configuration, and each source/ELF identity.

## Reproduce

Use the repository's pinned Rust toolchain with `llvm-tools-preview` and
`riscv64imac-unknown-none-elf`. Both compiler directories need a sibling
`ckb-sdk-rust` checkout at tag `v5.1.0`. Run from the candidate checkout; the
baseline destination must not exist. Normal Cargo dependency acquisition may
require network access. No compiler, checker, fixture or budget files in the
candidate are changed by the reproduction script.

The baseline uses its own Cargo target directory because integration binaries
embed `CARGO_MANIFEST_DIR`. Sharing a target directory between compiler checkouts
can reuse a binary with the wrong source root. To place the baseline build cache
on a larger volume, set `CELLSCRIPT_COST_BASELINE_TARGET_DIR` to a dedicated,
otherwise unused directory. Do not point it at the candidate's target directory.

```bash
./scripts/cellscript_cost_reproduce.sh ../CellScript-cost-baseline
cargo test --locked -p cellscript --test cost_corpus -- --test-threads=1
cargo test --locked -p cellscript --test business_corpus multi_script_cost_accounts_for_each_group_and_rejection -- --exact
cargo run --locked -p cellscript-tools --bin cellscript-tools -- \
  compare-cost-evidence \
  ../CellScript-cost-baseline/target/cellscript-cost/baseline.json \
  ../CellScript-cost-baseline/target/cellscript-cost/baseline-multi.json \
  target/cellscript-cost/cost-corpus-report.json \
  target/cellscript-cost/multi-script-cost.json \
  --output target/cellscript-cost/comparison.json
```

The comparison validates both report pairs against the checked-in ceilings,
requires identical fixture and execution sets, matches Script groups by role
and input/output membership, and checks outcomes, availability, ABI byte sizes,
reference measurements and per-metric regressions. It invalidates any previous
output before validation. Missing data cannot preserve an earlier passing result.
Source and executable hashes may change only where the comparison permits them;
matching lengths are not treated as matching identities.

## Coverage map

The [isolated attribution controls](ATTRIBUTION.md) add a reproducible D2-only
comparison and a D3 sharing ablation, with component code ranges and independent
byte/cycle comparisons. They preserve the frozen matrix below.

| Family | Report rows | Frozen budget / fixture source |
| --- | --- | --- |
| Matched Rust | `matched`: pool-merge, schema-roll, nft-lock | `tests/cost_corpus.rs`; `tests/fixtures/cost_corpus/*.cell` and Rust sources |
| Retained growth | `growth`: 18 rows | `growth_budgets.json`; `tests/support/cost_growth.rs` |
| Policy dispatch and decoding | `expanded`: 41 rows, each action executed | `expanded_fixtures.json`, `expanded_budgets.json` |
| Rejection and witness boundaries | `executions`, including late mismatches, malformed records and 4,096/4,097-byte witnesses | `tests/support/cost_expanded.rs` |
| Scalars, calls and generics | `scalar`: 9 rows; success and rejection | `scalar_budgets.json`; `tests/support/cost_scalar.rs` |
| Multi-Script composition | companion report: 6 transactions, 5 groups each, 4 artifact stack bounds | `multi_script_budgets.json`; `tests/business_corpus.rs` |

Budget filenames in this table are under `tests/fixtures/cost_corpus/`.
`tests/cost_measurement.rs` separately tests nonzero exit accounting, unavailable
traps/limits/setup failures, nested frames and static instruction counting.

## Static memory metric

`static_memory` counts decoded integer load/store instructions across the
validated ELF text, including unreachable code. It excludes non-text bytes,
syscall implementation work and child VMs. It does not weight loops or count
executed accesses. The load/store comparison is diagnostic; existing frozen
ELF, cycle, witness and stack ceilings remain independent acceptance criteria.
Slot reuse does not promise load/store elimination or register allocation.

## Recorded engineering checkpoint — 2026-09-24

The [baseline](baseline.json) and its [multi-Script companion](baseline-multi.json)
are compared with the [candidate](candidate.json) and its
[multi-Script companion](candidate-multi.json). The native
[comparison](comparison.json) passes **6,782** individual comparable metrics
with no regression. Its nine static memory rows are reported separately:

| Fixture | Loads before → after | Stores before → after |
| --- | ---: | ---: |
| generic-distinct | 55 → 55 | 26 → 26 |
| generic-repeated | 46 → 46 | 18 → 18 |
| nested-nine-arguments | 124 → 124 | 46 → 46 |
| scalar-chain-260 | 49 → 43 | 271 → 271 |
| scalar-chain-64 | 43 → 43 | 75 → 75 |
| scalar-chain-8 | 43 → 43 | 19 → 19 |
| scalar-loop-join | 51 → 51 | 20 → 20 |
| scalar-overlap-260 | 820 → 818 | 529 → 529 |
| scalar-overlap-64 | 231 → 231 | 137 → 137 |

Stores are unchanged throughout this corpus. The two large-offset fixtures
have fewer static loads. Frame and address-generation savings must not be
described as general elimination of memory traffic. The reproduced baseline
also matches all 387 comparable summary metrics in the original historical
baseline; that numerical agreement does not recover its missing source snapshot.

Both archived report pairs honestly declare dirty source. The candidate uses
base commit `65df937f45c6ba5ab45cf5ee69778bfa3e05cdc5` plus
[candidate-source-replay.patch](candidate-source-replay.patch). Its normalized
tracked-diff SHA-256 is
`73b1064bb604945046c2a3990f13db0a13bb9d289b2ef880f62ceac344489c96`;
the baseline digest is
`6fbdfb498ea89cc1a3d24e618bc835fd440041158c8c6d6f461d27605715e37a`.
Both declare the empty untracked-source digest. These are reconstructible
engineering snapshots, not clean-source release receipts. All seven evidence
files have checksums in [SHA256SUMS](SHA256SUMS).

To reconstruct the exact measured candidate from this checkout:

```bash
git clone --no-hardlinks --no-checkout . ../CellScript-cost-candidate
git -C ../CellScript-cost-candidate checkout --detach 65df937f45c6ba5ab45cf5ee69778bfa3e05cdc5
git -C ../CellScript-cost-candidate apply --unidiff-zero --intent-to-add "$PWD/docs/reports/0.31/candidate-source-replay.patch"
```

Use a dedicated Cargo target directory and the same pinned sibling SDK as for
the baseline, then run the focused cost commands above from that candidate.
The snapshot includes the comparison tool and fixture hash inventory; subsequent
archival documentation is intentionally outside the measured source snapshot.

The engineering snapshot passed the full `dev` and `ci` gates locally, including
the website build and Registry tests. The CI invocation skipped the optional
PostgreSQL and upstream IDL-client integrations. A separate Registry API run
against an isolated PostgreSQL 17.11 container passed 80 tests; only the optional
upstream IDL-client integration remained skipped because its checkout was not
configured. Static-memory measurement tests pass;
the native comparison rejects a substituted Rust reference ELF and invalidates
its previous output. The compiler/backend implementation remains the one at
`65df937f`; this checkpoint changes measurement, validation and documentation.
Release versions and bundles have not been promoted.

## Independent review

The [September 25 engineering review](BOUNDARY_REVIEW.md) records the source
review, acceptance map and additional shared-frame mutation regression. It is
automated engineering evidence. The requested child-issue closure excludes #30
release work and the 0.32 scope.

The independent compiler/checker review remains pending. Its scope is the
optimization diff plus the following reporting changes; an implementation-author
self-check is not an independent review and the 0.30 waiver does not transfer.

| Decision | Review focus | Existing evidence |
| --- | --- | --- |
| D1 | group identity, nonzero exits, unavailable measurements, unknown stack effects, fixed ceilings | `tests/cost_measurement.rs`; cost report pair and native validator |
| D2 | signed immediate boundaries, sizing/emission agreement, fixed trampoline, branch relaxation | `src/codegen/assembler/immediate.rs` tests and encoded VM tests |
| D3 | complete sharing key, exact tag/decoder/action routing, unknown-tag/common-check order | `tests/policy_artifact_checker.rs`; all-action policy matrix |
| D4 | private ownership, saved return address, outer witness capacity, outgoing arguments, fallback | adapter mutation tests and boundary fixtures |
| D5 | backedges, joins, simultaneous parameter spills, values live across calls, conservative exclusions | `src/codegen/scalar_slots.rs` tests; scalar/call fixture matrix |
| D6 | v9 reader rejection, machine mutations, unchanged source/wire contract, regenerated identities | standalone checker and artifact/policy mutation suites |

A release requires the named review result, fresh downstream bundles and a full
clean-source release gate. Existing 0.30 deployment records identify their
original artifacts and cannot establish deployment of new 0.31 bytes.
