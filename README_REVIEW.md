# CellScript 0.17 Review Packet

> **Historical review packet.** This file is kept in the source package because
> `Cargo.toml` includes it, but it records the 0.17 review boundary. It is not a
> current 0.21 RC audit index. For current branch semantics, read `BRANCHES.md`,
> `CHANGELOG.md`, and `docs/CELLSCRIPT_GATE_POLICY.md` first.

This packet was for reviewing the 0.17 iCKB differential-evidence branch without
overstating production equivalence.

## Branch Scope

- Formal proposal baseline: 0.12-era work, not the current `main` state.
- 0.16: audit-hardening preview.
- 0.17: research and differential-evidence branch, not the original grant
  acceptance baseline.

## Recommended Review Surface

- `BRANCHES.md`
- `docs/archive/0.18/CELLSCRIPT_0_18_ROADMAP.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_PRODUCTION_EQUIVALENCE_GATE.md`
- `tests/benchmarks/ickb_diff/matrix.json`

## Historical Evidence Boundary

The 0.17 review matrix recorded:

- 75 original-vs-CellScript CKB VM differential rows.
- 14 CellScript-only CKB VM rows.
- 8 original-side CKB VM rows.
- 0 active `MODEL` rows.

That was broad partial CKB VM differential evidence for selected normalized
iCKB fixture classes. It was not a production-equivalence claim. Later 0.18
work carried the iCKB line forward; do not use this packet to infer the current
matrix counts.

The production gate intentionally remains `NOT_PROVEN` because the following
items are still unresolved:

- non-executable legacy assumptions registry closure;
- real owner-auth witness production fixtures;
- first-class `Script` support, now explicitly scoped to 0.18;
- generic aggregate lowering;
- byte-accurate receipt decoding;
- complete DAO redeem accounting;
- complete production evidence-manifest closure.
