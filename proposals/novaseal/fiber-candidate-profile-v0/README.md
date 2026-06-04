# NovaSeal Fiber Candidate Profile v0

**Status**: reviewable application profile package. It is not V1-ready and not
production ready because live Fiber candidate evidence, wallet vectors, and
external attestations are still missing.

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
| Live devnet Fiber candidate path | missing | missing-live-devnet-evidence |
| Live Fiber execution evidence | missing | missing-live-fiber-evidence |
| Wallet signing vectors | missing | missing-wallet-evidence |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `future_fiber_test_path` as a package
implementation only when the certification gate sees this manifest, source
action, schemas, fixtures, docs, and invariant matrix. The business scenario
`fiber_candidate_path` must remain missing until live devnet stateful evidence
and Fiber execution evidence are generated and checked.
