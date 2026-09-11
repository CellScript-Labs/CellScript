# CellScript 0.30 Capability Ledger

**Status:** candidate governance contract; not a release announcement

This document separates two different completeness claims:

- **Executable-surface closure** means that every syntax and IR shape already
  admitted by the compiler is classified by the executable-surface registry as
  complete, bounded, shape-gated, compile-time-only, reserved, or fail-closed.
- **Product-capability completeness** means that a named CKB workflow has the
  required language, on-chain, artifact, builder, interface, and evidence
  paths. The capability ledger, not the executable-surface registry, records
  that wider claim.

The executable-surface registry remains authoritative for admitted compiler
entries. This ledger references product capabilities and must not copy or
override executable-surface statuses.

The canonical machine-readable ledger is
[`tests/fixtures/capability_ledger.json`](../tests/fixtures/capability_ledger.json).
It records issues #7 through #27, including priority, accountable issue owner,
reviewer assignment state, dependencies, evidence, release scope, closure
recommendation, remaining work, and non-goals. The business-corpus gate rejects
unknown taxonomy values, duplicate or missing issues, broken local design links,
noncanonical GitHub issue links, and any absent or research capability described
as executable, checked, or stable.

## Taxonomy

| Field | Allowed values and meaning |
|---|---|
| `category` | `language`, `consensus-runtime`, `artifact`, `builder`, `tooling`, `assurance`, `ecosystem`, or `product` |
| `admission_state` | `absent`, `research`, `reserved`, `shape-gated`, `bounded`, or `complete` |
| `on_chain_status` | `none`, `metadata-only`, `fail-closed`, or `executable` |
| `builder_status` | `none`, `scaffold`, `checked`, or `complete-transaction-path` |
| `evidence_status` | `none`, `simulator`, `ckb-vm`, `stateful`, or `chain`; these are distinct evidence ceilings |
| `interface_status` | `absent`, `private`, `package`, or `public-versioned` |
| `release_scope` | `required`, `non-blocking`, or `deferred` |
| `release_eligibility` | `experimental`, `candidate`, or `stable` |
| `issue_disposition` | `keep-open`, `close-after-merge`, `split-and-close`, or `deferred` |

`complete` is never sufficient by itself. Every complete claim must name its
universe: compiler surface, selected bounded shapes, artifact boundary, builder
path, evidence tier, product surface, or deployment scope. CKB-VM evidence is
not node admission; stateful local evidence is not selected-network deployment;
an implementation-complete issue is not independently reviewed merely because
repository tests pass.

## Current closure view

The machine ledger is the detailed source of truth. At this candidate point:

- #9 through #11, #14 through #21, and #23 were closed after their implemented
  bounded scopes and issue evidence were read back on 2026-09-11. Generic compatible-open
  handles are split to [#28](https://github.com/CellScript-Labs/CellScript/issues/28),
  and runtime-selected open roles are split to
  [#29](https://github.com/CellScript-Labs/CellScript/issues/29); neither expands
  the 0.30 release boundary.
- #7, #8, #12, #24, #25, and #26 keep their release-facing review or evidence
  work open even where their admitted implementation slice is executable.
- #13 remains open after its bounded fixed-width typed opening, successor,
  builder, typed-checker, exact schema-field pointer binding, standalone
  machine-checker mutation, and complete positive/adversarial committed-state
  CKB-VM inventory slice landed; independent review is still required.
- #22 is research and deferred from the 0.30 core; a circuit DSL remains a
  non-goal.
- #27 remains open until exact candidate replay, product publication, node
  admission, selected-network evidence, and independent review are complete.
  The replay cannot currently reproduce four superproject gitlinks: VS Code
  `7742a0c5c10ff92419869a2a03b1676e182923b4`, website
  `422b9f4740e37b00a16ea50df42ff0c88f3098e8`, NovaSeal
  `ee7dbff27af8b8ebe960b815794fcd76281d8164`, and iCKB equivalence
  `c27bac620293400409b73ec62c6514324c6c2855` are absent from their configured
  GitHub repositories as of 2026-09-11. The DOB pin remains retrievable. A
  current remote head is not a substitute for an accepted candidate identity.

This list is a dependency and closure map, not a delivery date or release
promise. The `0.30` branch remains a development line while any required ledger
entry or release requirement is incomplete.

## Strategy wording

Bounded lifecycle collection execution was the highest-risk
consensus-semantic gap already admitted into CellScript syntax and IR. Its
accepted bounded shapes do not establish that no important capability is
missing outside the admitted compiler universe. ProtocolBundle, typed roles,
exact handles, temporal domains, committed openings, product parity, and
deployment evidence each retain their own ledger classification.

Issue creation is not feature acceptance. No capability receives a release
promise without an owner, a reviewer, acceptance criteria, and the named
evidence gate. The 0.25 line remains the stable predecessor unless a separately
accepted release record says otherwise.

## Updating the ledger

Update the JSON entry and its design/evidence paths together. Then run the
business-corpus writer so the frozen evidence digest includes the change:

```bash
cargo run --quiet --locked -p cellscript-tools --bin cellscript-tools -- \
  --root . check-business-corpus --write
```

Development validation accepts an honest `candidate` ledger. Release
validation additionally requires an `accepted` ledger, all release requirements
passed, and every required capability stable, assigned to a reviewer, and no
longer marked `keep-open`.
