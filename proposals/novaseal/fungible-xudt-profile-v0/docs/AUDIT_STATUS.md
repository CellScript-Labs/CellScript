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

## Production Statement Boundary

Production claims remain blocked by missing public/shared CellDep attestation
and missing external BIP340 TCB review. Local xUDT stateful acceptance is
covered by the live devnet runner.
