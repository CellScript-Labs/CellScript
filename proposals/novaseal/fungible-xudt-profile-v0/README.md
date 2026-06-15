# NovaSeal Fungible xUDT Profile v0

**Status**: reviewable profile package with live stateful, operator-fixture,
and service-builder evidence. It is not production ready because external
production attestations are still missing.

This package implements the planned NovaSeal Fungible/xUDT object profile as a
source-level package with schemas, fixtures, invariant matrix, and security
boundary documentation. It also includes
`src/nova_fungible_xudt_lifecycle_type.cell:nova_fungible_xudt_lifecycle`, a
single CKB-facing lifecycle action for issue, transfer, and settlement.

## Boundary

The v0 profile models one balance-bearing xUDT object:

- `issue_xudt`: issuer creates one active balance object for a holder.
- `transfer_xudt`: current holder transfers the whole balance object to a new
  holder without changing amount or xUDT type hash.
- `settle_xudt`: current holder settles the active object into a terminal
  receipt with `new_amount = 0`.

This package intentionally does not implement partial transfers, split/merge
accounting, market flows, or a general ledger.

## Evidence

| Area | Status | Classification |
| --- | --- | --- |
| Separate Fungible xUDT profile package | implemented | source-guard-present |
| Canonical NovaSeal envelope binding | implemented | source-guard-present |
| Issue, transfer, settle actions | implemented | source-guard-present |
| Stable xUDT lifecycle type action | implemented | compiles-to-ckb-elf |
| Schemas and fixture labels | implemented | reviewable |
| Invariant matrix | implemented | reviewable |
| Live devnet issue -> transfer -> settle | implemented | `target/novaseal-fungible-xudt-devnet-stateful-live.json` |
| Profile-specific wallet/service fixtures | implemented | `target/novaseal-profile-operator-fixtures.json` + `target/novaseal-service-builder-fixtures.json` |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `object_profile_fungible_xudt` as a package
implementation only when the certification gate sees this manifest, source
actions, lifecycle dispatcher, schemas, fixtures, docs, invariant matrix, and
live stateful evidence. The business scenario `fungible_xudt_value_flow` now
passes at the live devnet stateful layer, and profile-specific wallet/operator
fixtures are generated and checked by
`scripts/novaseal_profile_operator_fixtures.py`. Service request/response
fixtures are generated and checked by
`scripts/novaseal_service_builder_fixtures.py`.
