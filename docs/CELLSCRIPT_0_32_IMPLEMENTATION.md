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
| #38 | Separately measured borrowing, word copies and staged loading | 18 borrowed-span, 96 word-copy and 24 staged-loader cases measured; retain current ownership/loading and record blockers to adopting alternatives; [evaluation closed with evidence](https://github.com/CellScript-Labs/CellScript/issues/38#issuecomment-5981846754) |
| #39 | Bounded register-retention experiment and scratch-ownership decision | Two candidates measured and rejected: first has 1,707 unchanged runs; [second](reports/0.32/retention-second/README.md) saves one cycle in four runs without ELF/store/stack savings; conservative scratch ownership retained. The archive is published on `0.32`; [resumed verification](reports/0.32/resumed/README.md) checks all twenty archive files and replays the native comparator |
| #40 | Cost attribution, experiment decisions, combined comparison and residual-cost report | Clean 1,707-run attribution, 6,782-metric replay, paired cost frontier and residual sites archived; wider transformations have explicit blockers. The [resumed record](reports/0.32/resumed/README.md) preserves the unchanged default's clean `6dba1768` backend receipt; later queue changes require their own gates |
| #22 | One exact typed ZK profile, real verifier execution and state binding | V2 composition plus private-counter application circuit, proving/VK package, lifecycle and explicit single-party setup admission implemented |
| #28 | Independently verified compatible-open Script/verifier handle selection | Bounded actual-bundle inspection binds concrete layouts and callable signatures to checked records. Optional pre-optimization source catalogs retain qualified ownership, public declaration completeness and absent templates; a separate symbolic checker validates universal type/ability guarantees. The [receipt contract](CELLSCRIPT_OPEN_INTERFACE_RECEIPT.md) documents the remaining H1/H2 boundary. The [host policy codec](CELLSCRIPT_OPEN_HANDLE_POLICY.md) fixes 1–32 members and a 656-byte selection with independent CCC vectors. Complete receipt/admission, nominal interface resolution and CKB runtime enforcement remain pending |
| #29 | Bounded runtime-selected cross-Script roles | Depends on #28; participant/claim attribution, builder materialization and current-Script enforcement remain pending |
| #42 | Usable ZK application workflow and explicit disposition of remaining tracks | P1 has [fresh clean-source CCC node evidence](reports/0.32/resumed-ccc/README.md) on signed `bfb19100`: ordinary updates and interrupted-proving recovery, followed by old-key update → old-key-authorized migration → new-key successor update with actual fee signatures and confirmed lineage. New-key self-installation, old-key reuse, repeated migration, stale configuration and transaction/proof substitutions reject. Current dev/backend gates pass; whole-queue CI remains deferred. P2–P4 remain the scoped #44/#45/#46 follow-ups; no production setup or deployment is claimed |

#28 and #29 are now independent 0.32 deliverables; they remain unnecessary for
the already implemented exact ZK profile. #42 no longer has a post-0.32 target.
An experimental optimization can be rejected on measured evidence; an
unimplemented or unmeasured experiment must not be described as complete.

The duplicate check covered open and closed lifecycle, composition, upgrade,
first-profile and cost issues. #42's remaining tracks have explicit 0.32 scopes:
[P2 multi-Cell binding (#44)](https://github.com/CellScript-Labs/CellScript/issues/44),
[P3 circuit/key/verifier lifecycle (#45)](https://github.com/CellScript-Labs/CellScript/issues/45),
and [P4 application costs/profile decisions (#46)](https://github.com/CellScript-Labs/CellScript/issues/46).
Each records its priority, blocker and acceptance criteria. Creating these
follow-ups does not complete #42 P1 or claim their implementations have passed.
The first authorized migration exercise remains mandatory in #42; #45 extends
it across versions after that application contract exists.

The migration prototype preserves the original immutable lifecycle rather than
claiming to retrofit its existing Cells. It uses a separately guarded unique
configuration, two pinned parent hashes and a single 0 -> 1 transition, which
must be authorized by the old counter proof over the final raw transaction.
Its initial all-group maximum case admits 64 inputs, outputs and dependencies
and a 16,384-byte serialized transaction under the unchanged 250,000,000-cycle
ceiling. The standalone replay measures each VM/EXEC generation separately.
The native migration client now binds a trusted manifest digest, actual
code/VK bytes, network and paired instance Scripts. It validates final creation
coordinates and selects the old parent from the actual configuration, preserving
the existing proof/signature snapshot checks. Its integration test reuses all
three continuous application transactions and checks byte-identical proof
attachment and all-group execution. The CCC adapter and CLI now share the same instance/manifest pins and proof
snapshots; its separate pinned-node exercise confirms actual successor lineage
and rejects stale configuration, new-key self-installation, old-key use after
migration and a second switch. Testtool resource replay remains distinct from
node commitment. Final gates, published source and profile acceptance remain
pending; these fixture results do not admit a production setup or public deployment.

## Follow-up reference map

The current #28 host prerequisites are documented by contract rather than by
the superseded session handoff:

- [Checked module catalogs](CHECKED_MODULE_CATALOG.md),
  [frozen code catalogs](FROZEN_CODE_CATALOG.md),
  [resolver-owned catalogs](RESOLVED_CODE_CATALOG.md), and
  [pre-typing source capture](FROZEN_PACKAGE_SOURCES.md) define source ownership.
- [Finite receipts](FIXED_POLICY_RECEIPT.md) and
  [source policies](FROZEN_CODE_POLICY.md) define bounded selection evidence.
- [Direct dependencies](DIRECT_CODE_DEPENDENCY.md) and
  [complete Type-group snapshots](DIRECT_TYPE_GROUP.md) define host transaction
  bindings; they do not establish execution, signature validity or liveness.

The [catalog replay archive](reports/0.32/code-catalog-replay/README.md) retains
each validation's exact source identity. Its full backend/CCC evidence binds
`16e6b721`; later staged dev records do not extend that provenance. The
[historical WIP logs](reports/0.32/wip-handoff/README.md) retain their original
failure context. Full H1/H2, #29 and #44–#46 remain unfinished, and independent
security review for #28/#29 remains required before stable admission.

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
