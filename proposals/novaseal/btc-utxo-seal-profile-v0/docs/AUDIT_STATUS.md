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
| Live devnet BTC UTXO seal closure | missing-live-devnet-evidence |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not builder-backed resolved transaction evidence, live devnet proof, or BTC
network proof.

## Production Statement Boundary

Production claims remain blocked by missing live closure evidence, missing
public BTC spend-verification evidence, missing wallet vectors, missing
public/shared CellDep attestation, and missing external BIP340 TCB review.
