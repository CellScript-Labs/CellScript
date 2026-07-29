# Branch Context

## 0.12-era proposal baseline

The 0.12-era work is the formal proposal baseline for grant-style acceptance
discussions. Do not use that historical baseline to describe the current
`main` branch state.

## nightly-0.23

`nightly-0.23` is the active implementation line for the draft 0.23 roadmap.
It begins from the released 0.22 compiler baseline and is currently staging
the operational work packages rather than carrying a 0.23 release claim.

The Python-to-Rust tooling migration is deliberately incremental on this line.
Only `check-skill-pack` and `validate-tooling-release` have Rust ports with
dual-run parity evidence; the retained Python implementations remain
authoritative for every other tool until their own parity gates pass. A commit
that deletes those baselines or points at unpublished submodule commits is not
a reproducible 0.23 baseline.

The registry, Off-Chain Session Runtime, and RGB++ / Fiber pillars remain
roadmap scope until their implementation and matching evidence gates land.
Treat deployed-and-observed and gated-and-certified claims as separate facts.

## main / nightly-0.22

`main` and `nightly-0.22` carry the released 0.22 compiler baseline plus the
draft 0.23 roadmap. Use this line for 0.22 maintenance and for comparison when
reviewing 0.23 changes. Treat features as shipped only when parser, formatter,
type checking, lowering, metadata, LSP, tests, docs, and the matching gate
agree; a branch name is not production evidence by itself.

## nightly-0.21

`nightly-0.21` carries the 0.21.1 maintenance checkpoint. This line includes
the 0.21 compiler, metadata, CLI, MCP, skill-pack, and builder-resolution work,
but it is not a production CKB claim beyond the evidence recorded for that
release line.

Use this line for 0.21 maintenance work. Keep P2 Template Merkleisation and
new observation syntax out of this line unless their parser, metadata,
backend, docs, and gate evidence are all promoted together.

## v0.22.0

`v0.22.0` is the latest stable release baseline. Use the exact tag as the
comparison point for 0.23 compiler, metadata, tooling, adapter, and registry
changes.

## v0.20.0

`v0.20.0` is the stable baseline before the 0.21 line. Use it as a historical
comparison point for 0.21 audits, metadata schema changes, and compatibility
notes. Be explicit when comparing against the tag ref
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
