# CellScript 0.30 Capability Ledger

**Status:** accepted bounded capability contract for the 0.30.0 release
candidate; public publication is withheld under the no-TAC constraint.

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
- #7, #8, #12, #13, and #24 were closed after their complete repository-local
  evidence and the maintainer's independent-review waiver were pushed and read
  back on 2026-09-12.
- #13 includes its bounded fixed-width typed opening, successor,
  builder, typed-checker, exact schema-field pointer binding, standalone
  machine-checker mutation, and complete positive/adversarial committed-state
  CKB-VM inventory slice.
- #25 and #26 were closed on 2026-09-19 after candidate `74cff5fd` and its
  product gitlinks were pushed and read back. Pudge transactions
  `0x49572cfd…a562` and `0x24273eb1…54e9` keep the accepted twelve-artifact
  deployment scope live;
  the machine report binds every output and the online verifier rechecks all
  Cell bytes. Final candidate reproduction matched all twelve deployed ELF
  identities; the historical deployment source remains `3cf60a10`. Local
  production admission also passed on pinned CKB `f7fa4436`.
- #22 is research and deferred from the 0.30 core; a circuit DSL remains a
  non-goal.
- #27 separates accepted release engineering from public product publication.
  Its `stable` eligibility and `split-and-close` recommendation mean the bounded
  capability is eligible for release; they do not claim the packages are already
  published or close the GitHub issue. Selected-network evidence is complete,
  but no tag or release workflow may be created while the maintainer's no-TAC
  constraint remains in force. The maintainer has explicitly waived
  independent review for this release line. The previously
  unreachable gitlinks were re-accepted on 2026-09-11 at retrievable release
  evidence commits: VS Code `4df04807`, website `320d54e`, NovaSeal
  `1e7c812`, and iCKB equivalence `53c5078a`; DOB remains pinned at
  `30709c97`. The two product pins include validated dependency refreshes:
  VS Code reports zero npm advisories, and the website reports no
  moderate/high/critical advisories. Its remaining twenty low-severity
  `elliptic`-chain findings are retained rather than accepting npm's proposed
  downgrade from the maintained CKB connector 1.3.0 to 0.0.4. The Registry API
  likewise reports no moderate/high/critical advisories after its release-tool
  refresh; its three remaining low findings are the same unsafe-fix class.

The current product gitlinks supersede the historical recovery pins above:
VS Code `ee0f259a` and website `6a61e49` were pushed and read back with root
`74cff5fd` on 2026-09-19. That candidate passed `dev`, full `ci` within
`release`, `backend`, and `release` on 2026-09-18. Its canonical WASM is
571,696 bytes gzip with SHA-256
`419e8260c3c6acc9f950adf422de90163533c14af96043ebb7429f935b1393d6`.
The [dated issue audit](releases/CELLSCRIPT_0_30_ISSUE_STATUS.md) records all
24 issue dispositions and the remaining publication boundary.

This list is a dependency and closure map, not a delivery date or release
promise. The `0.30` branch remains unpublished until the separate publication
action is authorized and verified. Final candidate replay must pass on clean source.

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
passed, and every required capability stable and no longer marked `keep-open`.
An assigned reviewer is required unless independent review is explicitly waived.
Both release modes run this stricter check before CI. Eligibility is a
pre-publication contract; it never authorizes a tag or overrides no-TAC.
