# NovaSeal Fungible xUDT Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate NovaSeal profile package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| Issuer-only issue | source-guard-present |
| Holder-only transfer | source-guard-present |
| Amount-preserving transfer | source-guard-present |
| Terminal settlement receipt | source-guard-present |
| Stable lifecycle type action | compiles-to-ckb-elf |
| Live devnet issue -> transfer -> settle | missing-live-devnet-evidence |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not yet a builder-backed resolved transaction harness and are not live
devnet proof.

## Production Statement Boundary

Production claims remain blocked by missing xUDT stateful acceptance, missing
wallet vectors, missing public/shared CellDep attestation, and missing external
BIP340 TCB review.
