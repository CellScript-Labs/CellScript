# Branch Context

## main / 0.12

`main` and the 0.12-era work are the formal proposal baseline for grant-style
acceptance discussions.

## 0.16

0.16 is an audit-hardening preview. It is useful for tracing how earlier review
findings were handled, but it should not be treated as the current iCKB
differential-evidence branch.

## research/protocol-equivalence

`research/protocol-equivalence` is the 0.17 research and differential-evidence
branch. It moves the iCKB benchmark from model-only evidence into broad partial
CKB VM differential evidence for selected normalized fixtures.

Current active matrix counts:

- `DIFFERENTIAL_CKB_VM_EXECUTED`: 66
- `CELL_SCRIPT_CKB_VM_EXECUTED`: 14
- `ORIGINAL_ICKB_CKB_VM_EXECUTED`: 8
- `MODEL`: 0

The branch still keeps `equivalence_status = NOT_PROVEN` and
`production_equivalence_claim = false`. Do not describe it as production
equivalent until the gate has complete evidence-manifest closure and the
non-executable assumptions registry is empty.
