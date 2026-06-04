# NovaSeal Fiber Candidate Profile v0

**Status**: reviewable application profile package with live CKB stateful
candidate settlement evidence. It is not production-ready Fiber execution
evidence until the external Fiber-node workflow suite passes.

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
| Fiber workflow discovery | implemented | `target/novaseal-fiber-node-experiments.json` |
| Live Fiber-node execution evidence | pending | `fiber_node_execution_v0` live suites not yet run |
| Wallet signing vectors | missing | missing-wallet-evidence |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `future_fiber_test_path` as a package
implementation only when the certification gate sees this manifest, source
action, schemas, fixtures, docs, and invariant matrix. The business scenario
`fiber_candidate_path` is CKB-stateful evidence for the NovaSeal profile. It
does not by itself prove Fiber node/channel execution; that requires
`scripts/novaseal_fiber_node_experiments.py` to report all required Fiber
workflow suites executed and passed.
