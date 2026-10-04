# 0.32 evidence index

This directory contains measured research, not a 0.32 release receipt.
Independent human review was waived by the maintainer on 2026-10-03.
The standalone artifact checker, executable negative tests and gates remain
required. The accepted scope and unfinished ZK admission work are recorded in
[the implementation checklist](../../CELLSCRIPT_0_32_IMPLEMENTATION.md).

## Separate evidence sets

| Set | Artifacts | Meaning |
| --- | --- | --- |
| Historical low-mask optimization | `baseline.json`, `baseline-multi.json`, `candidate.json`, `candidate-multi.json`, `comparison.json` | Original clean before/after comparison; see [IMMEDIATE_MASKS.md](IMMEDIATE_MASKS.md) for exact identities |
| Integrated 0.31 + 0.32 baseline | `integrated-main.json`, `integrated-multi.json` | Fresh clean replay of commit `4e1047980bce94d8ad0317b132a6f4c15a45df08`; existing budgets and Rust references retained |
| Same-commit replay comparison | `integrated-replay-main.json`, `integrated-replay-comparison.json` | A second clean gate run passes all 6,782 comparable metrics; multi-Script bytes equal `integrated-multi.json` |
| Execution attribution | `execution-attribution.json.gz`, `attribution-summary.json`, `residual-sites.json` | 1,707 separate diagnostic scheduler replays, each checked against ordinary group verdict and cycles; 38 distinct ELFs |
| Isolated alternatives | `experiments/`, `experiment-frontier.json` | Finite paired probes and explicit cost tradeoffs; admission decisions and blockers in [RESEARCH.md](RESEARCH.md) |
| ZK child research | `zk-child-research.json` | Real child pairing with modeled syscalls and a public-seed, non-authorizing test circuit; no production profile or stateful transaction claim |
| ZK v2 composition | [zk-composition-scheduler.json](zk-composition-scheduler.json) | Exact generated parent and real child under the CKB scheduler; two accepted successive transitions and five rejected replay/substitution cases, with artifact hashes and resource measurements; non-authorizing test circuit |
| Final gate acceptance | `validation.json`, `gate-*.log.gz`, `strict-backend-full.json.gz`, `ckb-stateful-acceptance.json.gz` | Passed dev/CI/backend records; final clean CI/backend commit is `6a453d30`; exact scopes and dev adjustment are recorded |
| Final corrected-source costs | `final-main.json.gz`, `final-multi.json.gz`, `final-comparison.json` | All 6,782 comparable metrics pass after the one-field inventory correction and evidence packaging; no compiler-code change |

The integrated replay is a new baseline, not another claimed optimization.
The two main reports differ only in 12 dynamically assigned transaction hash
fields across six executions; the native comparator reports zero regressions.
The later business-corpus inventory correction binds the parent-pinned iCKB
matrix instead of the pre-existing older local checkout. It changes only
`inventory_sha256`, not scenario expectations, cost budgets or compiler code.
The clean cost baseline is not itself a claim that its full CI gate passed.
The rejected retention patch records an earlier reconstructible dirty
integration snapshot and is labeled accordingly. Do not relabel its source
as the later clean packaging commit.

The full attribution JSON is gzip-compressed only for repository storage.
`attribution-summary.json` records the SHA-256 of its uncompressed bytes.
Archive compression does not reduce a deployed ELF. Summary totals weight
each recorded execution once; they do not model production transaction
frequency. Copied-byte semantics, host syscall memory traffic and unexercised
paths are not inferred from guest load/store counts.

`validation.json` records uncompressed SHA-256 hashes for the compressed gate
logs and native reports. The CKB report is for the existing local-node
acceptance harness, with `production_resource_identity_claim = false` and
`always-success-fixture-only` resource-identity evidence. It is not #22 ZK
application acceptance. No release gate or external-chain deployment is
claimed by this evidence package.

## Reproduction

Use a clean checkout at the report's recorded commit, the pinned Rust
toolchain and exact submodule gitlinks. In particular, an older local
`tests/benchmarks` checkout can disagree with the current compiler's iCKB
artifact hashes even when verdicts agree. Keep private or historical local
benchmark work in its own checkout instead of refreshing its receipts to
silence those mismatches.

The existing test targets regenerate the integrated report pair and detailed
diagnostics:

```bash
CARGO_INCREMENTAL=0 cargo test --locked -p cellscript --test cost_corpus -- --test-threads=1
CARGO_INCREMENTAL=0 cargo test --locked -p cellscript --test business_corpus multi_script_cost_accounts_for_each_group_and_rejection -- --exact
CARGO_INCREMENTAL=0 cargo test --locked -p cellscript --test cost_attribution -- --test-threads=1
CARGO_INCREMENTAL=0 cargo test --locked -p cellscript --test cost_measurement -- --test-threads=1
CARGO_INCREMENTAL=0 cargo test --locked -p cellscript cost_experiments --lib -- --test-threads=1
CARGO_INCREMENTAL=0 cargo test --locked -p cellscript --test cost_ir_experiments -- --test-threads=1
```

The diagnostic output goes to `target/cellscript-cost/`. Gate runs select
mode-specific report names. Use the existing `cellscript-tools
compare-cost-evidence` consumer for main/multi-Script report pairs; it checks
measurement compatibility and frozen ceilings. A passing isolated test is
not a substitute for `./scripts/cellscript_gate.sh ci` or `backend`.
