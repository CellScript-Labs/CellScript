# NovaSeal RWA Receipt Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate NovaSeal RWA receipt package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| Issuer-only materialisation | source-guard-present |
| Holder-only claim | source-guard-present |
| Dual-authority settlement | source-guard-present |
| Immutable receipt audit trail | source-guard-present |
| Stable lifecycle type action | compiles-to-ckb-elf |
| Live devnet materialise -> claim -> settle | live-devnet-covered |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not the live proof themselves. Live stateful evidence is recorded in
`target/novaseal-rwa-receipt-devnet-stateful-live.json`.

## Production Statement Boundary

Production claims remain blocked by missing public/shared CellDep attestation,
missing external BIP340 TCB review, and missing external legal/registry review.
Local RWA stateful acceptance is covered by the live devnet runner.
