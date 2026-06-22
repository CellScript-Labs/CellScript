# NovaSeal Fiber Candidate Profile v0

**Status**: production-ready source package with live CKB stateful
candidate settlement evidence and passing external Fiber-node workflow
execution evidence. It has local operator-fixture and service-builder binding
for the NovaSeal witness path; public/mainnet deployment still requires the
normal external deployment and verifier attestations.

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
| Profile-specific wallet/service fixtures | implemented | `target/novaseal-profile-operator-fixtures.json` + `target/novaseal-service-builder-fixtures.json` |
| Public/shared CellDep attestation | external-required | public/mainnet deployment evidence |
| External BIP340 TCB review | external-required | public/mainnet deployment evidence |

## Validation Boundary

The V1 readiness matrix may count `future_fiber_test_path` as a package
implementation only when the certification gate sees this manifest, source
actions, lifecycle dispatcher, schemas, fixtures, docs, invariant matrix, live
stateful report, and Fiber-node experiment report. The business scenario
`fiber_candidate_path` is CKB-stateful evidence for the NovaSeal profile, while
`scripts/novaseal_fiber_node_experiments.py` supplies the separate Fiber
node/channel execution evidence. `scripts/novaseal_profile_operator_fixtures.py`
binds that profile evidence to a wallet/operator witness fixture, and
`scripts/novaseal_service_builder_fixtures.py` binds it to a service
request/response skeleton.
