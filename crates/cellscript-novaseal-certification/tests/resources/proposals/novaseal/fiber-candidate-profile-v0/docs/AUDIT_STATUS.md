# NovaSeal Fiber Candidate Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate Fiber candidate profile package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| Candidate settlement binding | source-guard-present |
| Operator BIP340 authority | source-guard-present |
| Balance commitment progress | source-guard-present |
| Live devnet Fiber candidate path | live-ckb-stateful-evidence |
| Fiber workflow discovery | live-fiber-workflow-suite-evidence |
| Live Fiber-node execution | live-fiber-node-execution-evidence |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not builder-backed resolved transaction evidence or Fiber network proof.
The live CKB stateful report is separate from the external Fiber-node workflow
report. `target/novaseal-fiber-node-experiments.json` records the pinned
Nervos Fiber workflow execution evidence; it does not turn the CellScript
profile source into an in-contract verifier for Fiber HTLCs, routes, liquidity,
fees, or revocations.

## Production Statement Boundary

The current `fiber_node_execution_v0` report records all required Fiber
workflow suites executed and passed for the pinned devnet workflow evidence.

Latest refreshed evidence, 2026-06-11:

- execution worktree:
  `/Users/arthur/RustroverProjects/CellScript`
- `target/novaseal-fiber-node-experiments.json`: `schema=novaseal-fiber-node-execution-v0.4`, `status=passed`
- required suites present/executed/passed: `16/16`
- aggregate Bruno result: `317/317` requests passed, `473/473` assertions
  passed
- Fiber checkout: `develop`,
  `3bbf5ea0ed7debd83a707b5f28264bee2fd7371f`, dirty `false`

General NovaSeal production claims remain blocked by public/shared CellDep
attestation, public BTC SPV evidence, RWA legal/registry review evidence, and
external BIP340 TCB review.
