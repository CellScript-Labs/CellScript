# CellScript 0.32 implementation and acceptance

Status: implementation in progress. No completion or release claim.

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
| #41 | Validated low-mask immediates | Implemented at `8017a7bd`; historical evidence in `reports/0.32/IMMEDIATE_MASKS.md`; replay required after integration |
| #36 | Versioned observed per-VM stack measurements | Implemented diagnostic replay; 13 focused tests passed; final gates pending |
| #37 | Measured dispatch alternatives and explicit selection | 402 tree and 804 saved-selector cases measured; retain current routing; final gates pending |
| #38 | Separately measured borrowing, word copies and staged loading | 18 borrowed-span, 96 word-copy and 24 staged-loader cases measured; default ownership/loading retained pending full equivalence and checker contracts |
| #39 | Bounded register-retention experiment and scratch-ownership decision | Candidate patch archived and rejected after 1,707 unchanged runs; conservative scratch ownership retained |
| #40 | Cost attribution, experiment decisions, combined comparison and residual-cost report | Attribution companion and [research inventory](reports/0.32/RESEARCH.md) added; clean integrated comparison and remaining probes pending |
| #22 | One exact typed ZK profile, real verifier execution and state binding | Canonical codec and exact child research implemented; real pairing/context mutations pass; application circuit and parent/compiler/checker/builder closure pending |

#42 remains post-0.32 application tooling. Compatible-open handles (#28) and
runtime-selected open roles (#29) are not prerequisites for the exact profile.
An experimental optimization can be rejected on measured evidence; an
unimplemented or unmeasured experiment must not be described as complete.

## Integration baseline

Integrate the published 0.31 history through
`5fd4c4690512b3df4cd5a46d64194d7d4d3b9491` with the 0.32 low-mask changes.
The 0.31 release includes loop-fusion and assembly diagnostic corrections as
well as version, package and artifact-identity updates. Preserve the low-mask
planner and regenerate affected evidence rather than choosing an old hash
merely to resolve a merge conflict. Historical deployment receipts remain
bound to their original artifacts.

## Validation still required

- Focused positive, negative and boundary tests for each implementation.
- Exact independent-checker machine mutations for new executable contracts.
- Real CKB-VM and stateful transaction execution for the admitted ZK profile.
- Frozen cost budgets, separate new ZK budgets and version-aligned comparison.
- `./scripts/cellscript_gate.sh dev`, `ci` and `backend` on the final candidate.

The independent artifact checker is a software verification boundary and
remains required. Waiving human independent review does not waive its checks.
