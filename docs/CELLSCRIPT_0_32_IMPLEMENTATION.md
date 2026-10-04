# CellScript 0.32 implementation and acceptance

Status: #22 exact-profile implementation and scoped application admission are
implemented; see [acceptance evidence](reports/0.32/ZK_ACCEPTANCE.md). No overall
0.32 release or public deployment claim.

## Maintainer decision — 2026-10-03

The maintainer requested completion of the 0.32-related issues, explicitly
included #22 in 0.32, and waived independent review for this work. This
supersedes the September 27 low-mask-only implementation authorization and
the tentative assignment of #22. The waiver does not assert an independent
review occurred and does not waive compiler/checker agreement, adversarial
tests, resource budgets, reproducibility, or the unified gates. It does not
authorize a release publication or a new chain deployment.

## Follow-up scope decision — 2026-10-04

The maintainer requested that #28, #29, #36–#40 and #42 be resolved within
0.32 and receive individual issue comments. This promotes the previously
post-v1/post-0.32 work into the implementation scope. It does not make an
unfinished contract implemented. The separate finance plan (#43) is not part
of this closure request.

Implementer for this follow-up work: Codex. Independent security review for
the newly promoted compatible-open handles and open roles is still required
before stable admission; the earlier waiver is not assumed to cover these
new contracts. Their reviewer remains to be assigned. Work can proceed through
implementation and executable verification before that final admission step.

The scope of each issue remains its stated acceptance contract. In particular,
the cost research can conclude by rejecting a measured candidate and retaining
the current implementation; this does not implement a deferred optimization.
#42 requires executable P1 evidence and accepted outcomes or linked scoped
follow-ups for its remaining tracks. A roadmap alone cannot close it.
The [open-participant work plan](CELLSCRIPT_OPEN_PARTICIPANTS_PLAN.md) records
the missing #28/#29 contracts and their implementation/acceptance order.

## Scope and evidence

| Issue | Required outcome | Current evidence |
| --- | --- | --- |
| #41 | Validated low-mask immediates | Implemented at `8017a7bd`; historical evidence retained; integrated clean replay passes 6,782 comparable metrics |
| #36 | Versioned observed per-VM stack measurements | Implemented; historical gate archives verified and 13 focused tests replayed; [closed with evidence](https://github.com/CellScript-Labs/CellScript/issues/36#issuecomment-5980845985) |
| #37 | Measured dispatch alternatives and explicit selection | 402 tree and 804 saved-selector cases measured; retain current routing; checker/experiment replay passed; [closed with evidence](https://github.com/CellScript-Labs/CellScript/issues/37#issuecomment-5980846593) |
| #38 | Separately measured borrowing, word copies and staged loading | 18 borrowed-span, 96 word-copy and 24 staged-loader cases measured; default ownership/loading retained pending full equivalence and checker contracts |
| #39 | Bounded register-retention experiment and scratch-ownership decision | Candidate patch archived and rejected after 1,707 unchanged runs; conservative scratch ownership retained |
| #40 | Cost attribution, experiment decisions, combined comparison and residual-cost report | Clean 1,707-run attribution, 6,782-metric replay, paired cost frontier and residual sites archived; wider transformations have explicit blockers; unified gates pass |
| #22 | One exact typed ZK profile, real verifier execution and state binding | V2 composition plus private-counter application circuit, proving/VK package, lifecycle and explicit single-party setup admission implemented |
| #28 | Independently verified compatible-open Script/verifier handle selection | Exact handles and deployment lines are foundations; the new compatible-set authority, receipt, source/runtime and checker closure remains pending |
| #29 | Bounded runtime-selected cross-Script roles | Depends on #28; participant/claim attribution, builder materialization and current-Script enforcement remain pending |
| #42 | Usable ZK application workflow and explicit disposition of remaining tracks | Native/CCC proof snapshot binding and cancellation/retry pass the walkthrough, nine TypeScript tests and pinned-node acceptance (two confirmed updates; corrupted proof rejects with parent error 79); migration and remaining-track acceptance are still open |

#28 and #29 are now independent 0.32 deliverables; they remain unnecessary for
the already implemented exact ZK profile. #42 no longer has a post-0.32 target.
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

## Private authorization application for #22

The authorized first application is a single-Cell counter: prove knowledge of
its owner's secret, preserve the owner commitment, and increment exactly once
without overflow. The [application implementation](../contracts/zk-private-counter/README.md)
computes the commitment and old/new data hashes inside R1CS. Its lifecycle
wrapper handles unique zero initialization and executes the generated
CellScript parent on updates. The v2 parent binds the entire raw transaction
and calls the real pinned child through Spawn/IPC/Wait.

The application includes OS-random setup, exact PK/VK manifests, a proof CLI,
deployment-specific parent/handle export, scheduler and local-node acceptance,
and independent-target-directory ELF reproduction. Admission is explicitly
scoped to this relation and local single-party setup trust. It does not claim
MPC, an independent audit or public-network deployment. Named package-level
verifier resolution now checks the exact receipt, source index and VK before
code generation.

The historical [v2 scheduler record](reports/0.32/zk-composition-scheduler.json)
uses a public-seed non-authorizing circuit; it is not application evidence.
The counter's reports and commands are documented with its implementation.
Independent human review was waived; executable gates and their budgets remain
required.

## Completed cost/research candidate validation (before ZK composition)

`dev`, `ci` and `backend` pass. The final clean CI/backend source is
`6a453d3015784784a27ba1aa0a4b6b1a73db79a8`, including the inventory correction
for the parent-pinned benchmark. Backend includes a fresh build of pinned CKB
`f7fa4436737756f97a24e254f22c13a36316ecea` and passing local-node stateful
acceptance. The exact dev source adjustment, logs and scope limitations are
recorded in [validation.json](reports/0.32/validation.json).

The node report covers the existing acceptance harness. It retains
`production_resource_identity_claim = false` and the
`always-success-fixture-only` resource-identity scope. These results do not
admit the counter profile; its application-specific evidence is separate.

## Required validation for application changes and future transformations

- Focused positive, negative and boundary tests for each implementation.
- Exact independent-checker machine mutations for new executable contracts.
- Real CKB-VM and stateful transaction execution for the admitted ZK profile.
- Frozen cost budgets, separate new ZK budgets and version-aligned comparison.
- Re-run `./scripts/cellscript_gate.sh dev`, `ci` and `backend` after the
  executable implementation; run release gates before publication.

The independent artifact checker is a software verification boundary and
remains required. Waiving human independent review does not waive its checks.
