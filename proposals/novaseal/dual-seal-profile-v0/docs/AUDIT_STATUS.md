# NovaSeal Dual Seal Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate dual-seal profile package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| BTC closure binding | source-guard-present |
| CKB maturity guard | source-guard-present |
| Dual authority signatures | source-guard-present |
| Handoff-bound public/mainnet BTC closure SPV verification | external-required |
| Live CKB maturity evidence | live-devnet-covered |
| Live devnet dual-seal finality | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. The
live CKB stateful proof is `target/novaseal-dual-seal-devnet-stateful-live.json`;
fixtures are still not BTC network proof.

## Public/Mainnet Statement Boundary

Source-package readiness is covered by the live dual-seal runner. Public/mainnet
BTC-finality claims still require handoff-bound public BTC closure SPV evidence,
public/shared CellDep attestation, and external BIP340 TCB review. The required
public BTC report must echo the current live CKB and service-builder bindings,
carry the CKB-side BTC commitment hash, and include recomputable raw closure
transaction, block-header, Merkle, confirmation, and spend-input binding
material.
