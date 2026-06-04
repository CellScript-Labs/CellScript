# NovaSeal RWA Receipt Profile v0

**Status**: reviewable profile package. It is not V1-ready and not production
ready because live devnet RWA receipt lifecycle evidence, wallet vectors,
external attestations, and legal/registry review are still missing.

This package implements the planned NovaSeal RWA/receipt object profile as a
source-level package with schemas, fixtures, invariant matrix, and security
boundary documentation.

## Boundary

The v0 profile models an immutable receipt lifecycle:

- `materialize_rwa_receipt`: issuer creates a materialised receipt Cell and
  event for a non-zero integer amount.
- `claim_rwa_receipt`: holder claims the materialised receipt without changing
  amount or registry/document commitments.
- `settle_rwa_receipt`: issuer and holder jointly settle the claimed receipt
  into a terminal event.

This package does not verify off-chain title, custody, registry state, market
price, legal enforceability, or oracle facts.

## Evidence

| Area | Status | Classification |
| --- | --- | --- |
| Separate RWA receipt profile package | implemented | source-guard-present |
| Canonical NovaSeal envelope binding | implemented | source-guard-present |
| Materialise, claim, settle actions | implemented | source-guard-present |
| Integer-only amount model | implemented | source-guard-present |
| Immutable event audit trail | implemented | source-guard-present |
| Schemas and fixture labels | implemented | reviewable |
| Invariant matrix | implemented | reviewable |
| Live devnet materialise -> claim -> settle | missing | missing-live-devnet-evidence |
| Wallet signing vectors | missing | missing-wallet-evidence |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `object_profile_rwa_receipt` as a package
implementation only when the certification gate sees this manifest, source
actions, schemas, fixtures, docs, and invariant matrix. The business scenario
`rwa_receipt_lifecycle` must remain missing until live devnet stateful evidence
is generated and checked.
