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
| Live devnet materialise -> claim -> settle | missing-live-devnet-evidence |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not yet builder-backed resolved transaction evidence and are not live devnet
proof.

## Production Statement Boundary

Production claims remain blocked by missing RWA stateful acceptance, missing
wallet vectors, missing public/shared CellDep attestation, missing external
BIP340 TCB review, and missing external legal/registry review.
