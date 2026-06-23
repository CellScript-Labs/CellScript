# CellScript 0.17 Review Packet

This packet is for reviewing the 0.17 iCKB differential-evidence branch without
overstating production equivalence.

## Branch Scope

- Formal proposal baseline: 0.12 / `main`.
- 0.16: audit-hardening preview.
- 0.17: research and differential-evidence branch, not the original grant
  acceptance baseline.

## Recommended Review Surface

- `BRANCHES.md`
- `docs/CELLSCRIPT_0_18_ROADMAP.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_PRODUCTION_EQUIVALENCE_GATE.md`
- `tests/benchmarks/ickb_diff/matrix.json`

## Current Evidence Boundary

The active iCKB matrix currently contains:

- 75 original-vs-CellScript CKB VM differential rows.
- 14 CellScript-only CKB VM rows.
- 8 original-side CKB VM rows.
- 0 active `MODEL` rows.

This is broad partial CKB VM differential evidence for selected normalized iCKB
fixture classes. It is not a production-equivalence claim.

The production gate intentionally remains `NOT_PROVEN` because the following
items are still unresolved:

- non-executable legacy assumptions registry closure;
- real owner-auth witness production fixtures;
- first-class `Script` support, now explicitly scoped to 0.18;
- generic aggregate lowering;
- byte-accurate receipt decoding;
- complete DAO redeem accounting;
- complete production evidence-manifest closure.
