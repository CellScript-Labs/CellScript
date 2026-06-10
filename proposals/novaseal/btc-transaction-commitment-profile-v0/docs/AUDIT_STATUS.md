# NovaSeal BTC Transaction Commitment Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate BTC transaction commitment seal package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| BTC txid/wtxid/output tuple binding | source-guard-present |
| Transition commitment binding | source-guard-present |
| Committer BIP340 authority | source-guard-present |
| Public BTC inclusion/finality verification | missing-spv-or-indexer-evidence |
| Live devnet BTC transaction commitment transition | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not the live proof themselves and are not BTC network proof. Live stateful
evidence is recorded in
`target/novaseal-btc-transaction-commitment-devnet-stateful-live.json`.

## Production Statement Boundary

Production claims remain blocked by missing public BTC verification evidence,
missing public/shared CellDep attestation, and missing external BIP340 TCB
review. Local BTC transaction commitment stateful execution is covered by the
live devnet runner.
