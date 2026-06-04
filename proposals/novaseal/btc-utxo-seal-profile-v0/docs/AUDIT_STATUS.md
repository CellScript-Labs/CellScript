# NovaSeal BTC UTXO Seal Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate BTC UTXO seal package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| Sealed UTXO tuple binding | source-guard-present |
| Single-use CKB-side closure | source-guard-present |
| Owner BIP340 authority | source-guard-present |
| Public BTC spend verification | missing-spv-or-indexer-evidence |
| Live devnet BTC UTXO seal closure | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not the live proof themselves and are not BTC network proof. Live stateful
evidence is recorded in
`target/novaseal-btc-utxo-seal-devnet-stateful-live.json`.

## Production Statement Boundary

Production claims remain blocked by missing public BTC spend-verification
evidence, missing public/shared CellDep attestation, and missing external
BIP340 TCB review. Local BTC UTXO seal stateful execution is covered by the
live devnet runner.
