# NovaSeal Fiber Candidate Profile v0 Audit Status

## Claim Classification

| Claim | Classification |
| --- | --- |
| Separate Fiber candidate profile package | source-guard-present |
| Canonical envelope binding | source-guard-present |
| Candidate settlement binding | source-guard-present |
| Operator BIP340 authority | source-guard-present |
| Balance commitment progress | source-guard-present |
| Live devnet Fiber candidate path | live-ckb-stateful-evidence |
| Fiber workflow discovery | discovery-ready-live-not-run |
| Live Fiber-node execution | pending-fiber-node-suite-execution |

## Fixture Honesty

The fixtures in `fixtures/` are review targets and negative-case labels. They
are not builder-backed resolved transaction evidence or Fiber network proof.
The live CKB stateful report is separate from the external Fiber-node workflow
report.

## Production Statement Boundary

Production Fiber execution claims remain blocked until
`fiber_node_execution_v0` reports all required Fiber workflow suites executed
and passed. General NovaSeal production claims remain blocked by public/shared
CellDep attestation and external BIP340 TCB review.
