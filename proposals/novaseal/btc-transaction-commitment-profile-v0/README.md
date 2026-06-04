# NovaSeal BTC Transaction Commitment Profile v0

**Status**: reviewable seal profile package. It is not V1-ready and not
production ready because profile-specific wallet/service fixtures, public BTC
verification evidence, and external attestations are still missing.

This package implements the planned NovaSeal BTC transaction commitment seal
profile as a source-level package with schemas, fixtures, invariant matrix, and
security boundary documentation.

## Boundary

`commit_btc_transaction_transition` binds a CKB state transition to:

- BTC txid,
- BTC wtxid,
- BTC output index,
- BTC amount in satoshis,
- transition commitment hash,
- committer BIP340 authority.

This package does not prove BTC inclusion, depth, finality, or UTXO spend.

## Evidence

| Area | Status | Classification |
| --- | --- | --- |
| Separate BTC transaction commitment profile package | implemented | source-guard-present |
| Canonical NovaSeal envelope binding | implemented | source-guard-present |
| BTC transaction tuple binding | implemented | source-guard-present |
| Committer authority signature | implemented | source-guard-present |
| Schemas and fixture labels | implemented | reviewable |
| Invariant matrix | implemented | reviewable |
| Live devnet BTC transaction commitment transition | implemented | `target/novaseal-btc-transaction-commitment-devnet-stateful-live.json` |
| Public BTC verification evidence | missing | `proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.json` external-required |
| Lifecycle dispatcher | implemented | `src/nova_btc_transaction_commitment_type.cell:nova_btc_transaction_commitment_lifecycle` |
| Profile-specific wallet/service fixtures | missing | builder-fixture-required |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `seal_profile_btc_transaction_commitment` as
a package implementation only when the certification gate sees this manifest,
source actions, lifecycle dispatcher, schemas, fixtures, docs, invariant matrix,
and live stateful evidence. The business scenario
`btc_transaction_commitment_transition` now passes at the live devnet stateful
layer; production BTC-finality claims remain blocked until public BTC
verification evidence is generated and checked.

The public BTC evidence shape is now machine-readable. A real production report
must follow
`proposals/novaseal/v0-mvp-skeleton/proofs/public_btc_spv_evidence.template.json`
and cover this profile with a non-local BTC transaction, block hash, SPV proof
hash, public SPV client CellDep, source-service provenance, and at least six BTC
confirmations.
