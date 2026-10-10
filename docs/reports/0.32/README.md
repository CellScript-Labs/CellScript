# 0.32 evidence index

Storage: final reports and concise comparisons remain local. Intermediate JSON
payloads named below are in the [pinned historical archive](#historical-json-storage),
with exact recovery instructions and checksums. Historical manifests retain
their original scope and may require those archived payloads.

This directory contains measured research, not a 0.32 release receipt.
Independent human review was waived by the maintainer on 2026-10-03.
The standalone artifact checker, executable negative tests and gates remain
required. The accepted scope and application admission boundary are recorded in
[the implementation checklist](../../CELLSCRIPT_0_32_IMPLEMENTATION.md).

## Separate evidence sets

| Set | Artifacts | Meaning |
| --- | --- | --- |
| Historical low-mask optimization | `baseline.json`, `baseline-multi.json`, `candidate.json`, `candidate-multi.json`, `comparison.json` | Original clean before/after comparison; see [IMMEDIATE_MASKS.md](IMMEDIATE_MASKS.md) for exact identities |
| Integrated 0.31 + 0.32 baseline | `integrated-main.json`, `integrated-multi.json` | Fresh clean replay of commit `4e1047980bce94d8ad0317b132a6f4c15a45df08`; existing budgets and Rust references retained |
| Same-commit replay comparison | `integrated-replay-main.json`, `integrated-replay-comparison.json` | A second clean gate run passes all 6,782 comparable metrics; multi-Script bytes equal `integrated-multi.json` |
| Execution attribution | `execution-attribution.json.gz`, `attribution-summary.json`, `residual-sites.json` | 1,707 separate diagnostic scheduler replays, each checked against ordinary group verdict and cycles; 38 distinct ELFs |
| Isolated alternatives | `experiments/`, `experiment-frontier.json` | Finite paired probes and explicit cost tradeoffs; admission decisions and blockers in [RESEARCH.md](RESEARCH.md) |
| Second register-retention candidate | [retention-second/](retention-second/README.md) | Identical-IR probes, complete cost comparison and scheduler traces; four of 1,707 corpus executions save one cycle, with no ELF/store/stack saving; rejected for the default backend; separate exact replay patches and source identities |
| ZK child research | `zk-child-research.json` | Real child pairing with modeled syscalls and a public-seed, non-authorizing test circuit; no production profile or stateful transaction claim |
| ZK v2 composition | [zk-composition-scheduler.json](zk-composition-scheduler.json) | Exact generated parent and real child under the CKB scheduler; two accepted successive transitions and five rejected replay/substitution cases, with artifact hashes and resource measurements; non-authorizing test circuit |
| Private authorization counter | [private-counter/evidence.json](private-counter/evidence.json), [admission](private-counter/admission.json), [setup](private-counter/setup.json) | Real secret-knowledge circuit, OS-random candidate VK, successful scheduler and local-node transactions, replay rejection and reproduced ELFs; engineering admission under explicit local single-party setup trust; no public-network deployment |
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

## Historical JSON storage

Intermediate report payloads listed below were removed from the current tree on
2026-10-10. They remain byte-for-byte recoverable at Git commit
`fe953b19a0a0311942691cb478d8ecf6f0574085`; each filename links to that exact snapshot.
This is a storage cleanup, not a new validation run or a change to any measured
result. Current summaries, comparisons, reproduction patches, final acceptance
records and the retained baseline keep their original evidence boundaries.

Historical manifests and raw-artifact fields may name these archived files.
Resolve those paths in the pinned snapshot before checking a complete historical
manifest; missing local payloads do not count as verified evidence. Current
checksum lists cover retained files; the table preserves the stored-byte hashes
of removed payloads. For gzip files these hashes cover the compressed bytes.

From the repository root, recover one report into an ignored directory:

```bash
mkdir -p target/report-archive
git show fe953b19a0a0311942691cb478d8ecf6f0574085:docs/reports/0.32/baseline-multi.json > target/report-archive/baseline-multi.json
sha256sum target/report-archive/baseline-multi.json
```

For a complete historical checksum/manifest replay, use a separate checkout at
the pinned commit so its original documentation and report paths are restored
together. Ordinary report generation belongs under `target/`; promote only
intentional final evidence or a necessary reproduction baseline into `docs/reports/`.
Deleting current-tree files does not shrink existing Git history.

| Historical payload | Stored bytes | SHA-256 |
| --- | ---: | --- |
| [baseline-multi.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/baseline-multi.json) | 28758 | `43829d306b6bd38182139476d733eb66376ae4d20dbadf03a6ab7c8e11330a75` |
| [baseline.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/baseline.json) | 6450693 | `c0497f1980d757297c140f98c8bb008e39f0332d96c86ced5110271c7644a028` |
| [candidate-multi.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/candidate-multi.json) | 28758 | `58daaf45b414e826d8bcda3c5b6427a789b5dec814a609b304f116682018bf0d` |
| [candidate.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/candidate.json) | 6450691 | `01886a9d968f17838734f059395c8ce3d1be388b4f06a725129591c11aa1bb72` |
| [execution-attribution.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/execution-attribution.json.gz) | 6114455 | `487914f0262747dc59b7b41f08139a31d3d4f387048a7a9e1f8820ec0d91f45b` |
| [experiments/adjacent-scalar-retention.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/adjacent-scalar-retention.json) | 18389 | `f00a1d6a205fc9c6a0196056ba0357fd59d31ac71f4f5bc14c886a62aaa717a5` |
| [experiments/borrowed-span-experiment.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/borrowed-span-experiment.json) | 6534 | `9b1b4698cf41b6378a73f53107b862361b0e6495625b549d558e14b5619c1bce` |
| [experiments/compressed-probe.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/compressed-probe.json) | 1151 | `dc81bc0adcce30d804dcaf47d6e345c388a569aadb586ae2afb3ed8b5bd7eb74` |
| [experiments/dispatch-experiment.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/dispatch-experiment.json) | 102261 | `1a22560992e2ace298186f0804db9d15c96dd02acadd5fecb81beddcd260ba7e` |
| [experiments/pure-expression-experiment.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/pure-expression-experiment.json) | 4156 | `452e29d7e3ba0badb37035fd798e75002747563607e8446ae55d566b167e7f9c` |
| [experiments/saved-selector-experiment.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/saved-selector-experiment.json) | 211510 | `0acdfb6e54222ca2fe6e8d90dc9c30c454e3abc17b141d8d104ec6bdfe4d5c36` |
| [experiments/staged-loader-experiment.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/staged-loader-experiment.json) | 6600 | `303e4d946dc5b5f0d164b61cce6b9800481c2a24cc1e0ca69afeb2c449cce373` |
| [experiments/word-copy-experiment.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/experiments/word-copy-experiment.json) | 21778 | `004c4140e760b4387b85b27e458a615d7fb8a361d69dd391dce43dc1ed8074a0` |
| [field-register-replay/ccc-migration.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/field-register-replay/ccc-migration.json.gz) | 568 | `695b9962276a12bb12192cad23455d4cfc4b3dd95398964d21963d8a78af2447` |
| [field-register-replay/ckb-acceptance.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/field-register-replay/ckb-acceptance.json.gz) | 63195 | `fceff7ec9fea7dfa8cb8ad05fd58c4cb9e49d96aa67be9f501d505b3ab3f97f2` |
| [field-register-replay/counter-node.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/field-register-replay/counter-node.json.gz) | 5240 | `327847f564f38e6936aaff7a13a03339186342cd0cc90c955569740b30c54aec` |
| [field-register-replay/strict-backend-full.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/field-register-replay/strict-backend-full.json.gz) | 5364 | `2b681fc82d0cebfccc25a1f2c1088d508797867b7741e86eb091a6a722089963` |
| [integrated-main.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/integrated-main.json) | 6450691 | `099391d1c7c45a2c0851504d22bb80e27317db96f223adf0a0666482963c7939` |
| [integrated-multi.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/integrated-multi.json) | 28758 | `7274dbc04b4d1b5c6e3b09e09a45e3da1b6b73bb2afc3a8d09cd314a8c55f523` |
| [integrated-replay-main.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/integrated-replay-main.json) | 6450691 | `a23f6c683e74b572f313cf4d9f498bbd581fc605a39b59df7dd2c4e628d017df` |
| [pool-snapshot-replay/ccc-migration.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/pool-snapshot-replay/ccc-migration.json.gz) | 569 | `f5fa2ade2f377dc447cff4ca53c88edc2f710501da3cd9efd3d884906e9f0a26` |
| [pool-snapshot-replay/clean-ckb-acceptance.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/pool-snapshot-replay/clean-ckb-acceptance.json.gz) | 63111 | `63268469a41e8cae9114d76a3808bfbae2180aa3e1fc2b7b53844000182397d1` |
| [pool-snapshot-replay/counter-node.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/pool-snapshot-replay/counter-node.json.gz) | 5235 | `196d1b6ec37ce51d7891c80698dec48eb05e850a7fa10b3a57ceaf59dbc7c457` |
| [pool-snapshot-replay/strict-backend.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/pool-snapshot-replay/strict-backend.json.gz) | 5401 | `d78062be4120231e2fbdfb3bdf9ea8d6729c6d6ef1db3664881749a4dec8c539` |
| [read-storage-replay/ccc-migration.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/read-storage-replay/ccc-migration.json.gz) | 569 | `ac14142cf9ccc6c140a2a0a67e13700c10f783011b919f814a1377240eee0b7b` |
| [read-storage-replay/ckb-acceptance.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/read-storage-replay/ckb-acceptance.json.gz) | 63173 | `3123e8c569dd41c53751af2fff6f81633dc4da297f4139749bc1ec21797728f4` |
| [read-storage-replay/counter-node.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/read-storage-replay/counter-node.json.gz) | 5248 | `d71b26a796dcba86c15b5815e82286a1da3bb287719a57a53449a3cf7efda55e` |
| [read-storage-replay/strict-backend-full.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/read-storage-replay/strict-backend-full.json.gz) | 5376 | `8b2c583b851f4584c9b44bea328d616347abd84d9c1a9a4b1af17706b1a25ed2` |
| [resumed-ccc/ccc-client.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/resumed-ccc/ccc-client.json) | 776 | `4a4b0f6b01a26cc6abc25a7510fbde35f136ea6364916654a396c4b25ee2d951` |
| [resumed-ccc/ccc-migration.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/resumed-ccc/ccc-migration.json) | 1382 | `127e9590eca43037ad595ef0cb2856762d8c43488e77dfe114754485fe308016` |
| [resumed-ccc/clean-ckb-acceptance.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/resumed-ccc/clean-ckb-acceptance.json.gz) | 63122 | `48c9403181c37f67d93efcc662204d6f328c20f5e1105f20dd84b85fdc350609` |
| [resumed-ccc/counter-node.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/resumed-ccc/counter-node.json.gz) | 5210 | `7207f74c4db6d911a883dda7d4db8696b4d07780ce6f5b280f82379b104aac2f` |
| [resumed/clean-ckb-acceptance.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/resumed/clean-ckb-acceptance.json.gz) | 61774 | `8b6d5a5b3897d43fdff44f43fe25f2e1a0e53c6b5b8ab05c74574a0a85c00ad7` |
| [resumed/counter-node.json](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/resumed/counter-node.json) | 7931 | `a11112643a2186c9ab2febbc44291cc199b8e2ebf72ae065b72d0f6282ea0427` |
| [retention-second/baseline-attribution.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/retention-second/baseline-attribution.json.gz) | 6114492 | `b0a1c52e12232ad3b1c8e4477e0e41a85422119ad8545f29cf08ce3b8af987e9` |
| [retention-second/baseline-main.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/retention-second/baseline-main.json.gz) | 347360 | `6ce09c282fd5b97917631c86b2b0d62997d64f66fd2582c7acfa096abe1d0f96` |
| [retention-second/baseline-multi.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/retention-second/baseline-multi.json.gz) | 3606 | `c0a62e396b9350aecdd316ea7d46ce9f973faedbda4193cdf953467bd6110ad0` |
| [retention-second/candidate-attribution.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/retention-second/candidate-attribution.json.gz) | 6114496 | `e015899a1198489d271c0a4b155acced878ee3e20c3a628cf554c422549cccf6` |
| [retention-second/candidate-main.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/retention-second/candidate-main.json.gz) | 347373 | `09ae51f4023b31876de000926f42f3e9520652d4912a08b3c7d0b98df727d53b` |
| [retention-second/candidate-multi.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/retention-second/candidate-multi.json.gz) | 3611 | `ea4a8ca515b2903f48f6c92ad923b5f7c30cb9b411337ab1bc9a768b39a1143c` |
| [retention-second/probe.json.gz](https://github.com/CellScript-Labs/CellScript/blob/fe953b19a0a0311942691cb478d8ecf6f0574085/docs/reports/0.32/retention-second/probe.json.gz) | 168193 | `dc609203fa6644838e40c655893bbeed4b85415ebde235863a687bf12b47c831` |
