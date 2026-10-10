# Historical WIP validation logs

These logs were retained with WIP commit `ba86b1c5` on 2026-10-08. They
describe evolving sources before that snapshot, not a passing gate for the
containing commit or the current branch. `SHA256SUMS` binds their original
bytes; failed and partial runs are preserved without rewriting them.

| Log | Recorded result and limit |
| --- | --- |
| `value-return-runtime-tests.log` | One test passed and three failed before codegen with `E2105` field-access errors; only the zero-width case passed. |
| `value-return-expanded-tests.log` | 21 policy/artifact tests passed, followed by one passing and two failing struct-return tests. |
| `generic-struct-return-tests.log` | The imported generic struct-return regression passed at O0–O3; later changes are not covered. |
| `generic-shape-other-tests.log` | Eight interface tests passed, followed by 20 passing and one failing policy test; the expanded log records the subsequent fixture correction. |

A register-liveness edit followed the latest runtime run and was untested at
the pause. No complete dev, backend or CI pass was established for that final
WIP snapshot. Later corrections and validation have their own source-bound
[resumed evidence](../resumed/README.md) and
[catalog replay records](../code-catalog-replay/README.md).

The superseded session narrative remains available in Git history at
`dca23a27:docs/HANDOFF_0_32_WIP.md`. Current scope and unfinished acceptance work
belong to the [implementation record](../../../CELLSCRIPT_0_32_IMPLEMENTATION.md).
