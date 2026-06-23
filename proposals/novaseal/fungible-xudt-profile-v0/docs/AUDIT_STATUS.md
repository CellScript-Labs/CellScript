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
| Live devnet issue -> transfer -> settle | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not the live proof themselves. Live stateful evidence is recorded in
`target/novaseal-fungible-xudt-devnet-stateful-live.json`.

## Public/Mainnet Statement Boundary

Source-package readiness is covered by the live devnet runner. Public/mainnet
deployment claims still require public/shared CellDep attestation and external
BIP340 TCB review.
