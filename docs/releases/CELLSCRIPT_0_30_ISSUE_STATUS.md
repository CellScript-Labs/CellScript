# CellScript 0.30 issue status

## Publication closure — 2026-09-25

The accepted GitHub-plus-website scope of #27 is complete. The signed
[`v0.30.0` release](https://github.com/CellScript-Labs/CellScript/releases/tag/v0.30.0)
was published on September 23 at
`352b4950e7dc7489a4ee7e7ed6d6edf4c7fefd71`.
[Tag CI](https://github.com/CellScript-Labs/CellScript/actions/runs/35813354116)
and the [release workflow](https://github.com/CellScript-Labs/CellScript/actions/runs/35813354154)
passed on that source. The
[publication/readback record](https://github.com/CellScript-Labs/CellScript/issues/27#issuecomment-5797185434)
records the final clean local release/backend gates, four native archives,
VSIX, installer and checksums, production/testnet website and Registry readback.
The public WASM SHA-256 is
`9f5ed7be566bb865cae1be60310bc5fed32ddaece896ebb89cdcc76996555aa5`
(571,825 gzip bytes). These supersede the candidate identities below.

Independent review was **waived for 0.30 by the maintainer**, not performed.
The September 21 publication authorization superseded the publication hold;
no TAC was invoked. Existing Pudge receipts retain their original twelve-artifact
deployment provenance. They are neither mainnet evidence nor deployment of
0.31 artifacts. Crates.io, Marketplace, and optional production Registry
commitment/reproducer activation are separate, unclaimed channels and do not
reopen the accepted GitHub-plus-website scope. #22/#28/#29 remain deferred.

## Historical candidate audit — 2026-09-19

The following dated audit is preserved as history, including its then-current
publication hold and candidate hashes. It is not the current release status.

All 24 GitHub issues were checked against their bodies, latest comments, the
accepted capability ledger, and candidate evidence. Twenty are closed and four
remain open. Existing closure comments remain authoritative for each bounded
scope; unchecked proposal checklists do not expand that accepted scope or turn
the explicit independent-review waiver into completed review.

## Issue dispositions

| Issue | State | Accepted scope or outstanding work |
| --- | --- | --- |
| [#1](https://github.com/CellScript-Labs/CellScript/issues/1) | Closed | Branch relaxation fixed; the separate diagnostic follow-up #21 is also complete. |
| [#7](https://github.com/CellScript-Labs/CellScript/issues/7) | Closed | Bounded authenticated Type-group input consumption. |
| [#8](https://github.com/CellScript-Labs/CellScript/issues/8) | Closed | Fixed-width bounded output-plan correspondence. |
| [#9](https://github.com/CellScript-Labs/CellScript/issues/9) | Closed | Artifact-only ProtocolBundle and its admitted transaction path. |
| [#10](https://github.com/CellScript-Labs/CellScript/issues/10) | Closed | Exact closed roles; open roles are deferred to #29. |
| [#11](https://github.com/CellScript-Labs/CellScript/issues/11) | Closed | Exact fixed-width handles and exact-version deployment lines; compatible-open handles are deferred to #28. |
| [#12](https://github.com/CellScript-Labs/CellScript/issues/12) | Closed | Accepted typed temporal and Since domains. |
| [#13](https://github.com/CellScript-Labs/CellScript/issues/13) | Closed | Fixed-width commitment openings and successor correspondence. |
| [#14](https://github.com/CellScript-Labs/CellScript/issues/14) | Closed | Capability ledger distinguishes executable support, evidence, release eligibility, and publication. |
| [#15](https://github.com/CellScript-Labs/CellScript/issues/15) | Closed | Canonical workspace graph and dependency-first builds. |
| [#16](https://github.com/CellScript-Labs/CellScript/issues/16) | Closed | Single canonical package instance; Cell.lock v5. |
| [#17](https://github.com/CellScript-Labs/CellScript/issues/17) | Closed | Transitive environment selection binds exact chain identity. |
| [#18](https://github.com/CellScript-Labs/CellScript/issues/18) | Closed | Compiler requirements enforced before source loading. |
| [#19](https://github.com/CellScript-Labs/CellScript/issues/19) | Closed | Stable resolve-graph and build-plan contracts. |
| [#20](https://github.com/CellScript-Labs/CellScript/issues/20) | Closed | Transactional package/interface/deployment upgrade planning. |
| [#21](https://github.com/CellScript-Labs/CellScript/issues/21) | Closed | Generated assembly provenance in assembler diagnostics. |
| [#22](https://github.com/CellScript-Labs/CellScript/issues/22) | Open, deferred | Typed ZK verifier research; not a 0.30 blocker or an implemented generic ZK contract. |
| [#23](https://github.com/CellScript-Labs/CellScript/issues/23) | Closed | Bounded Proposal A public value generics. |
| [#24](https://github.com/CellScript-Labs/CellScript/issues/24) | Closed | Accepted typed runtime-view v1 matrix and its resource profiles. |
| [#25](https://github.com/CellScript-Labs/CellScript/issues/25) | Closed | Frozen cryptographic/authorization matrix plus selected Pudge deployment evidence. |
| [#26](https://github.com/CellScript-Labs/CellScript/issues/26) | Closed | Frozen eight-family business corpus and its admitted release/deployment evidence. |
| [#27](https://github.com/CellScript-Labs/CellScript/issues/27) | Open | Public publication remains withheld under no-TAC; candidate engineering and selected deployment evidence are complete. |
| [#28](https://github.com/CellScript-Labs/CellScript/issues/28) | Open, deferred | Post-v1 generic compatible-open handles. |
| [#29](https://github.com/CellScript-Labs/CellScript/issues/29) | Open, deferred | Post-v1 runtime-selected open roles, dependent on #28. |

## Exact candidate evidence

The verified implementation candidate is
[`74cff5fd86bf01cd812d888eaa7e6b0bd82422e1`](https://github.com/CellScript-Labs/CellScript/commit/74cff5fd86bf01cd812d888eaa7e6b0bd82422e1).
Its compiler/editor version is 0.30.0; Edition 2026 remains default and the
opt-in identity is `cellscript-source-semantics-2027-0.30-v1`.

`dev`, full `ci` within `release`, `backend`, and `release` passed on that clean
candidate on 2026-09-18, including 975 compiler unit tests, 249 CLI tests, the
complete integration suites, canonical WASM and VSIX builds, and fresh pinned
CKB production/stateful acceptance. The CKB revision was
`f7fa4436737756f97a24e254f22c13a36316ecea` in a separate clean worktree; SDK
v5.1.0 and Rust 1.97.1 remained pinned. Both release modes now enforce
`check-business-corpus --release` before expensive checks.

The source-bound local acceptance reports have SHA-256 digests
`0a84a3eecacedfc9afd466ab96cfb432b8558a7368bafb2c76f3d476f7283e6f`
(release) and
`8d50169f791f3f478e9894fcc9470fbc27396a2143c340b57a21e9f961ced80f`
(backend). These identify local evidence; this document does not claim those
reports were uploaded as public release assets. Later documentation changes
do not relabel these results as a full replay at another commit.

The pushed product gitlinks are website
[`6a61e49`](https://github.com/CellScript-Labs/cellscript-website/commit/6a61e49bdb9b87e77788a6a5dc7c063c99a9ebe0)
and VS Code
[`ee0f259a`](https://github.com/CellScript-Labs/cellscript-vscode/commit/ee0f259ae92c1ef1fa96375eb52391c50e541f1f).
Canonical WASM SHA-256 is
`419e8260c3c6acc9f950adf422de90163533c14af96043ebb7429f935b1393d6`,
with 571,696 bytes gzip under the 600 KiB limit.

The [Pudge manifest](../../tests/fixtures/cellscript_0_30_pudge_deployment_manifest.json)
and [deployment report](../../tests/fixtures/cellscript_0_30_pudge_deployment.json)
record twelve immutable code Cells in two confirmed transactions. Live readback
passed on 2026-09-18; candidate reproduction matched all twelve executable
sizes and SHA-256 identities. Historical deployment provenance remains
`3cf60a105c80a33016b8280c990a479657c3bc13`, distinct from candidate reproduction.
This is the named testnet deployment scope, not mainnet evidence or a claim
that every corpus transaction was submitted to Pudge.

## Remaining release action

No new implementation blocker was identified by this issue-status audit.
The required bounded capabilities are accepted, and #25/#26 were closed after
the candidate and its product commits were pushed and read back on 2026-09-19.
Independent review was explicitly waived by the maintainer, not performed or
reported as passed. The wider authoring roadmap and #22/#28/#29 remain outside
the frozen 0.30 boundary.

#27 must remain open until public publication is authorized and verified.
The existing no-TAC constraint still prohibits release tags and GitHub Release
workflow dispatch. Prepared versions, pushed branches, local CLI/VSIX packages,
and accepted capability records do not establish public package, extension,
website, or stable-release publication. See the
[release notes](CELLSCRIPT_0_30_RELEASE_NOTES.md) and
[capability ledger](../CELLSCRIPT_0_30_CAPABILITY_LEDGER.md).
