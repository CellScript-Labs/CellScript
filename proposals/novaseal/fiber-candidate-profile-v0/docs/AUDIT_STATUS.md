# NovaSeal Fiber Candidate Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate Fiber candidate profile package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| Candidate settlement binding | source-guard-present |
| Operator BIP340 authority | source-guard-present |
| Balance commitment progress | source-guard-present |
| Live Fiber execution | missing-live-fiber-evidence |
| Live devnet Fiber candidate path | missing-live-devnet-evidence |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not builder-backed resolved transaction evidence, live devnet proof, or
Fiber network proof.

## Production Statement Boundary

Production claims remain blocked by missing live Fiber candidate evidence,
missing wallet vectors, missing public/shared CellDep attestation, and missing
external BIP340 TCB review.
