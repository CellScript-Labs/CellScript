# NovaSeal Dual Seal Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate dual-seal profile package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| BTC closure binding | source-guard-present |
| CKB maturity guard | source-guard-present |
| Dual authority signatures | source-guard-present |
| Handoff-bound public BTC closure SPV verification | missing-external-spv-evidence |
| Live CKB maturity evidence | live-devnet-covered |
| Live devnet dual-seal finality | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. The
live CKB stateful proof is `target/novaseal-dual-seal-devnet-stateful-live.json`;
fixtures are still not BTC network proof.

## Production Statement Boundary

Production claims remain blocked by missing handoff-bound public BTC closure
SPV evidence, missing public/shared CellDep attestation, and missing external
BIP340 TCB review. The required public BTC report must echo the current live
CKB and service-builder bindings, carry the CKB-side BTC commitment hash, and
include recomputable raw closure transaction, block-header, Merkle,
confirmation, and spend-input binding material. Local CKB maturity/finality
execution is covered by the live dual-seal runner.
