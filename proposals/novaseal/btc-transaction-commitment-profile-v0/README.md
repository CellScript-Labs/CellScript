# NovaSeal BTC Transaction Commitment Profile v0

**Status**: reviewable seal profile package. It is not V1-ready and not
production ready because live devnet transition evidence, wallet vectors, public
BTC verification evidence, and external attestations are still missing.

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
| Live devnet BTC transaction commitment transition | missing | missing-live-devnet-evidence |
| Public BTC verification evidence | missing | missing-spv-or-indexer-evidence |
| Wallet signing vectors | missing | missing-wallet-evidence |
| Public/shared CellDep attestation | missing | external-required |
| External BIP340 TCB review | missing | external-required |

## Validation Boundary

The V1 readiness matrix may count `seal_profile_btc_transaction_commitment` as
a package implementation only when the certification gate sees this manifest,
source action, schemas, fixtures, docs, and invariant matrix. The business
scenario `btc_transaction_commitment_transition` must remain missing until live
devnet stateful evidence and public BTC verification evidence are generated and
checked.
