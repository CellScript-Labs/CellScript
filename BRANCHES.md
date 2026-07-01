# Branch Context

## 0.12-era proposal baseline

The 0.12-era work is the formal proposal baseline for grant-style acceptance
discussions. Do not use that historical baseline to describe the current
`main` branch state.

## main / nightly-0.21

`main` and `nightly-0.21` currently carry the 0.21 release-candidate
implementation checkpoint. This line includes the 0.21 compiler, metadata,
CLI, MCP, skill-pack, and builder-resolution work, but it is not a production
CKB release claim until the matching `ci`, backend, and release gates have
recorded passing evidence.

Use this line for 0.21 RC integration work. Keep P2 Template Merkleisation and
new observation syntax out of this line unless their parser, metadata,
backend, docs, and gate evidence are all promoted together.

## v0.20.0

`v0.20.0` is the latest stable release baseline before the 0.21 RC line. Use it
as the comparison point for 0.21 audits, metadata schema changes, and
compatibility notes. Be explicit when comparing against the tag ref
`refs/tags/v0.20.0`, because local branches may also be named `v0.20.0`.

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
