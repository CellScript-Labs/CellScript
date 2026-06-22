# NovaSeal BTC UTXO Seal Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate BTC UTXO seal package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| Sealed UTXO tuple binding | source-guard-present |
| Single-use CKB-side closure | source-guard-present |
| Owner BIP340 authority | source-guard-present |
| Handoff-bound public/mainnet BTC spend SPV verification | external-required |
| Live devnet BTC UTXO seal closure | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not the live proof themselves and are not BTC network proof. Live stateful
evidence is recorded in
`target/novaseal-btc-utxo-seal-devnet-stateful-live.json`.

## Public/Mainnet Statement Boundary

Source-package readiness is covered by the live devnet runner. Public/mainnet
BTC spend claims still require handoff-bound public BTC spend SPV evidence,
public/shared CellDep attestation, and external BIP340 TCB review. The required
public BTC report must echo the current live CKB and service-builder bindings,
carry the CKB-side sealed UTXO commitment hash, and include recomputable raw
spend/sealed transaction, block-header, Merkle, confirmation, and sealed-output
binding material.
