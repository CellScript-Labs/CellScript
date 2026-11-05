# CellScript 0.32 implementation and acceptance

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

Implementer for this follow-up work: Codex. Independent security review was
originally required for the newly promoted compatible-open handles and open
roles. Both requirements are superseded by the 2026-10-11 decision below.

The scope of each issue remains its stated acceptance contract. In particular,
the cost research can conclude by rejecting a measured candidate and retaining
the current implementation; this does not implement a deferred optimization.
#42 requires executable P1 evidence and accepted outcomes or linked scoped
follow-ups for its remaining tracks. A roadmap alone cannot close it.
The [open-participant work plan](CELLSCRIPT_OPEN_PARTICIPANTS_PLAN.md) records
the missing #28/#29 contracts and their implementation/acceptance order.

## Maintainer decision — 2026-10-11 (#28 external audit)

The maintainer cancelled the external audit / independent human security
review requirement for #28 and #29. This explicitly supersedes their original
stable-admission review condition and earlier statements that the 2026-10-03
waiver did not cover this contract. No external audit is claimed to have
occurred, and assigning an external reviewer is no longer a closure gate for
either issue. The maintainer explicitly extended the cancellation to #29.

The waiver does not waive the independent artifact checker, receipt recomputation,
compiler/checker agreement, adversarial CKB-VM execution, Type-hash history
binding, generated-builder/ProtocolBundle byte parity, resource budgets,
reproducibility or the applicable unified gates. Incomplete technical acceptance
still prevents closing either issue; this decision grants no release or deployment claim.

## Scope and evidence

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

## Integration baseline

Integrated the published 0.31 history through
`5fd4c4690512b3df4cd5a46d64194d7d4d3b9491` with the 0.32 low-mask changes.
The 0.31 release includes loop-fusion and assembly diagnostic corrections as
well as version, package and artifact-identity updates. Preserve the low-mask
planner and regenerate affected evidence rather than choosing an old hash
merely to resolve a merge conflict. Historical deployment receipts remain
bound to their original artifacts.

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

## Completed cost/research candidate validation (before ZK composition)

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
