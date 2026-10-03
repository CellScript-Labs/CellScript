# CellScript 0.32 implementation and acceptance

Status: implementation in progress. Cost/research candidate gates pass; #22
application admission remains unfinished. No overall completion or release claim.

## Maintainer decision — 2026-10-03

The maintainer requested completion of the 0.32-related issues, explicitly
included #22 in 0.32, and waived independent review for this work. This
supersedes the September 27 low-mask-only implementation authorization and
the tentative assignment of #22. The waiver does not assert an independent
review occurred and does not waive compiler/checker agreement, adversarial
tests, resource budgets, reproducibility, or the unified gates. It does not
authorize a release publication or a new chain deployment.

## Scope and evidence

| Issue | Required outcome | Current evidence |
| --- | --- | --- |
| #41 | Validated low-mask immediates | Implemented at `8017a7bd`; historical evidence retained; integrated clean replay passes 6,782 comparable metrics |
| #36 | Versioned observed per-VM stack measurements | Implemented diagnostic replay; 13 focused tests and unified gates pass |
| #37 | Measured dispatch alternatives and explicit selection | 402 tree and 804 saved-selector cases measured; retain current routing; unified gates pass |
| #38 | Separately measured borrowing, word copies and staged loading | 18 borrowed-span, 96 word-copy and 24 staged-loader cases measured; default ownership/loading retained pending full equivalence and checker contracts |
| #39 | Bounded register-retention experiment and scratch-ownership decision | Candidate patch archived and rejected after 1,707 unchanged runs; conservative scratch ownership retained |
| #40 | Cost attribution, experiment decisions, combined comparison and residual-cost report | Clean 1,707-run attribution, 6,782-metric replay, paired cost frontier and residual sites archived; wider transformations have explicit blockers; unified gates pass |
| #22 | One exact typed ZK profile, real verifier execution and state binding | Canonical codec and exact child research implemented; real pairing/context mutations pass; application circuit and parent/compiler/checker/builder closure pending |

#42 remains post-0.32 application tooling. Compatible-open handles (#28) and
runtime-selected open roles (#29) are not prerequisites for the exact profile.
An experimental optimization can be rejected on measured evidence; an
unimplemented or unmeasured experiment must not be described as complete.

## Integration baseline

Integrated the published 0.31 history through
`5fd4c4690512b3df4cd5a46d64194d7d4d3b9491` with the 0.32 low-mask changes.
The 0.31 release includes loop-fusion and assembly diagnostic corrections as
well as version, package and artifact-identity updates. Preserve the low-mask
planner and regenerate affected evidence rather than choosing an old hash
merely to resolve a merge conflict. Historical deployment receipts remain
bound to their original artifacts.

The integrated implementation is local commit
`4e1047980bce94d8ad0317b132a6f4c15a45df08`. See the
[evidence index](reports/0.32/README.md) for original versus integrated report
identities. The 220 iCKB differential tests pass with the pinned benchmark
`f2a200a366ed1d97fbb22ac4633ae5abba591b5e`; a pre-existing older benchmark
checkout was preserved and tested separately rather than overwritten.

## Required application input for #22

The first application must specify the relation between old state, new state
and authority, and identify its circuit/VK or authorize a concrete circuit
specification. The current public-seed identity circuit is deliberately
non-authorizing and cannot supply that decision. The draft profile, strict
codec, child ELF and all 13 recorded VM observations reproduce across two
local checkout builds, but production source admission remains unavailable.

After the application relation is fixed, #22 still requires nominal proof
typing, named exact verifier/VK binding, transaction-derived statements,
mandatory parent result checking, ProofPlan and machine/checker closure,
builder/editor parity, real scheduler/stateful tests and production resource
and reproducibility evidence. Independent review is waived; these executable
requirements are not.

## Completed candidate validation

`dev`, `ci` and `backend` pass. The final clean CI/backend source is
`6a453d3015784784a27ba1aa0a4b6b1a73db79a8`, including the inventory correction
for the parent-pinned benchmark. Backend includes a fresh build of pinned CKB
`f7fa4436737756f97a24e254f22c13a36316ecea` and passing local-node stateful
acceptance. The exact dev source adjustment, logs and scope limitations are
recorded in [validation.json](reports/0.32/validation.json).

The node report covers the existing acceptance harness. It retains
`production_resource_identity_claim = false` and the
`always-success-fixture-only` resource-identity scope. These results do not
admit the unfinished ZK application profile.

## Validation still required for #22 and future adopted transformations

- Focused positive, negative and boundary tests for each implementation.
- Exact independent-checker machine mutations for new executable contracts.
- Real CKB-VM and stateful transaction execution for the admitted ZK profile.
- Frozen cost budgets, separate new ZK budgets and version-aligned comparison.
- Re-run `./scripts/cellscript_gate.sh dev`, `ci` and `backend` after the
  remaining executable implementation; run release gates before publication.

The independent artifact checker is a software verification boundary and
remains required. Waiving human independent review does not waive its checks.
