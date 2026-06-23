# NovaSeal BTC Transaction Commitment Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate BTC transaction commitment seal package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| BTC txid/wtxid/output tuple binding | source-guard-present |
| Transition commitment binding | source-guard-present |
| Committer BIP340 authority | source-guard-present |
| Handoff-bound public/mainnet BTC inclusion/finality SPV verification | external-required |
| Live devnet BTC transaction commitment transition | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not the live proof themselves and are not BTC network proof. Live stateful
evidence is recorded in
`target/novaseal-btc-transaction-commitment-devnet-stateful-live.json`.

## Public/Mainnet Statement Boundary

Source-package readiness is covered by the live devnet runner. Public/mainnet
BTC-finality claims still require handoff-bound public BTC SPV evidence,
public/shared CellDep attestation, and external BIP340 TCB review. The required
public BTC report must echo the current live CKB and service-builder bindings,
carry the CKB-side BTC commitment hash, and include recomputable raw
transaction, block-header, Merkle, confirmation, and output-binding material.
