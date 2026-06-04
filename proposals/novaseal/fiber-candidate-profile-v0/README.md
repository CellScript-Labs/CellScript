# NovaSeal Fiber Candidate Profile v0

**Status**: reviewable application profile package with live CKB stateful
candidate settlement evidence and passing external Fiber-node workflow
execution evidence. It is not yet operator-ready production evidence until
wallet/operator fixtures bind those Fiber workflows to NovaSeal witness hashes
and channel-state summaries.

This package implements the planned NovaSeal Fiber-facing candidate test path as
a source-level package with schemas, fixtures, invariant matrix, and security
boundary documentation.

## Boundary

`settle_fiber_candidate` binds:

- candidate id,
- channel id,
- route commitment,
- payment hash,
- old and new balance commitments,
- settlement amount,
- operator BIP340 authority.

This package does not verify Fiber node state, HTLCs, route liquidity, fees,
revocations, or payment-network execution.

## Evidence

| Area | Status | Classification |
| --- | --- | --- |
| Separate Fiber candidate profile package | implemented | source-guard-present |
| Canonical NovaSeal envelope binding | implemented | source-guard-present |
| Candidate settlement binding | implemented | source-guard-present |
| Operator authority signature | implemented | source-guard-present |
| Schemas and fixture labels | implemented | reviewable |
| Invariant matrix | implemented | reviewable |
| Live devnet Fiber candidate path | implemented | `target/novaseal-fiber-candidate-devnet-stateful-live.json` |
| Lifecycle dispatcher | implemented | `src/nova_fiber_candidate_type.cell:nova_fiber_candidate_lifecycle` |
| Fiber workflow discovery | implemented | `target/novaseal-fiber-node-experiments.json` |
| Live Fiber-node execution evidence | implemented | `16/16` required suites executed and passed |
| Wallet signing vectors | implemented | production-gate-covered |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `future_fiber_test_path` as a package
implementation only when the certification gate sees this manifest, source
actions, lifecycle dispatcher, schemas, fixtures, docs, invariant matrix, live
stateful report, and Fiber-node experiment report. The business scenario
`fiber_candidate_path` is CKB-stateful evidence for the NovaSeal profile, while
`scripts/novaseal_fiber_node_experiments.py` supplies the separate Fiber
node/channel execution evidence. The next production-hardening step is
operator-ready fixture binding, not basic Fiber-suite execution.
