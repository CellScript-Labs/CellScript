# Isolated cost attribution — 2026-09-25

These controls finish the D2-only and D3 component reporting requested in #31
and #32. They use the existing frozen fixtures, toolchain, reference binaries,
measurement definitions and budgets. They are reproducible dirty engineering
snapshots, not release builds. All 1,830 executions and the six multi-Script
transactions ran for each control; neither control rewrites a budget.

## Reproduction and identities

From this checkout, with the pinned toolchain and a clean sibling SDK v5.1.0:

```bash
./scripts/cellscript_cost_reproduce.sh ../CellScript-cost-immediate immediate
./scripts/cellscript_cost_reproduce.sh ../CellScript-cost-unshared unshared
cargo run --locked -p cellscript-tools --bin cellscript-tools -- \
  compare-cost-evidence docs/reports/0.31/baseline.json docs/reports/0.31/baseline-multi.json \
  ../CellScript-cost-immediate/target/cellscript-cost/immediate.json \
  ../CellScript-cost-immediate/target/cellscript-cost/immediate-multi.json \
  --output target/cellscript-cost/immediate-comparison.json
cargo run --locked -p cellscript-tools --bin cellscript-tools -- \
  compare-cost-evidence ../CellScript-cost-unshared/target/cellscript-cost/unshared.json \
  ../CellScript-cost-unshared/target/cellscript-cost/unshared-multi.json \
  docs/reports/0.31/candidate.json docs/reports/0.31/candidate-multi.json \
  --output target/cellscript-cost/sharing-comparison.json
```

Each destination must be new and uses its own build directory. The default
reproduction stage remains the unchanged baseline. Both controls start at
`65df937f45c6ba5ab45cf5ee69778bfa3e05cdc5`:

- `immediate` applies `baseline-replay.patch`, then restores only
  `src/codegen/assembler.rs` and `src/codegen/assembler/immediate.rs` from that
  base commit. This isolates D2 from the released 0.30 backend; it retains v8.
  Normalized diff SHA-256:
  `72ae3a94305a584d03b84931d80025321f3648071287c6d61274963948cac78a`.
- `unshared` applies `candidate-source-replay.patch`, then restores only the
  policy orchestrator from published `352b4950` and applies
  [unshared-harness.patch](unshared-harness.patch). The latter expects zero
  shared decoders; inputs, execution oracles, measurements and ceilings remain
  unchanged. D2, D4, D5 and v9 remain enabled. Normalized diff SHA-256:
  `c161108a92dae72adead02a48848870685570f738ee352cacffb359e40cd5165`.

Both reports declare the empty untracked-source digest. Their exact provenance
and artifact identities are in [immediate.json](immediate.json),
[immediate-multi.json](immediate-multi.json), [unshared.json](unshared.json) and
[unshared-multi.json](unshared-multi.json). The native
[D2 comparison](immediate-comparison.json) and
[D3 comparison](sharing-comparison.json) each pass **6,782** comparable metrics
with no regression. Checksums are in [ATTRIBUTION_SHA256SUMS](ATTRIBUTION_SHA256SUMS).
The unshared control emits expected unused-sharing-helper compiler warnings;
it is an experimental source replay, not an alternative supported build mode.

## D2 alone

Values are whole-artifact ELF bytes, worst successful transaction cycles and
worst measured selected-group rejection cycles. Policy constants include wide
magic/identity and fixed-byte comparison materialization; exact literal
endpoints and 2^56 are separately covered by the encoded-VM immediate tests.

| Fixture | ELF bytes: baseline → D2 | Success cycles | Rejection cycles |
| --- | ---: | ---: | ---: |
| 8 actions, 32-byte argument | 11,680 → 10,000 | 14,158 → 13,678 | 12,455 → 11,975 |
| 64 actions, 32-byte argument | 73,440 → 61,008 | 29,822 → 26,654 | 28,119 → 24,951 |

D2 does not shrink the private adapter or scalar frame. In both policy rows the
maximum individual frame remains 5,376 bytes and the static single-VM call-chain
bound remains 11,472 bytes. The final combined savings cannot all be attributed
to immediate encoding.

## D3 alone with D2/D4/D5 held constant

The byte columns below are decoded lowering-record code ranges, not ELF file
sizes. They exclude headers, alignment, rodata and the unchanged other-runtime
code (524 bytes in these samples). `Dedicated` includes the old per-action
decoder/adapter; `decoder + stubs` separates the shared helper and direct stubs.

| Fixture | Dispatch before / after | Dedicated before | Decoder + stubs after | Action before / after |
| --- | ---: | ---: | ---: | ---: |
| 8 actions, 32-byte argument | 2,152 / 2,152 | 2,208 | 252 + 320 | 4,320 / 4,320 |
| 64 actions, 32-byte argument | 5,220 / 5,220 | 17,664 | 252 + 2,560 | 34,560 / 34,560 |

| Fixture | ELF bytes: unshared → shared | Success cycles | Rejection cycles |
| --- | ---: | ---: | ---: |
| 8 actions, 32-byte argument | 9,680 → 8,048 | 13,588 → 13,189 | 11,890 → 11,491 |
| 64 actions, 32-byte argument | 58,448 → 43,592 | 26,004 → 22,299 | 24,306 → 20,601 |

Private adapter reservations for these fixed 32-byte layouts are 64 bytes;
the shared decoder reserves zero additional bytes. The largest individual
frame is the unchanged 4,192-byte parent, while the complete static call-chain
bound is 6,160 bytes. These are distinct metrics, not observed stack peaks.
All 41 policy rows retain their component ranges in the full reports, including
mixed layouts, sparse/high tags and record-count boundaries. Trees, tables,
saved selectors, borrowing and register retention remain outside this work.
