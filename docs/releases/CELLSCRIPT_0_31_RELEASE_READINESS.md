# CellScript 0.31 release readiness

Status: candidate preparation, 2026-10-01. No 0.31 release has been published.

## Scope and issue review

[Issue #30](https://github.com/CellScript-Labs/CellScript/issues/30) owns the
remaining release work. Its five implementation issues, #31 through #35, were
closed on September 25. The engineering acceptance is recorded in
[the boundary review](../reports/0.31/BOUNDARY_REVIEW.md) and
[the reproduction package](../reports/0.31/README.md). GitHub CI passed on the
preparation baseline `54d639a7` in
[run 36293306996](https://github.com/CellScript-Labs/CellScript/actions/runs/36293306996).
That run predates the 0.31.0 version promotion.

Preparation also incorporates the September 30 correction
[`9c441094`](https://github.com/CellScript-Labs/CellScript/commit/9c441094eccb480fdfb0ca028d1f56b75eea48bb)
from `arthur/fix-audit-bugs`: conservative loop-prefix dependency handling,
fallible immediate sizing, and the self-hosted Registry client-IP boundary.
Fresh compiler/backend and Registry tests must cover this addition.

The release retains the bounded capability portfolio from
[the 0.30 ledger](../CELLSCRIPT_0_30_CAPABILITY_LEDGER.md). The current
machine-readable ledger identifies 0.31 and keeps its acceptance pending.
The published 0.30 acceptance and deployment files remain historical evidence
for their exact bytes. The independent-review waiver granted to 0.30 does not
authorize 0.31.

The deferred #22, #28, #29 and #36–#42 work is outside this release. In
particular, the additional immediate optimization on branch 0.32 is not part
of this candidate.

## Candidate identities

- Compiler, standalone checker, CKB/Fiber adapters, native tools, WASM and
  both Registry verifiers: 0.31.0.
- VS Code extension: 0.31.0, with the existing authoring surface.
- Rust: 1.97.1; source editions and dependency pins retain their contracts.
- Native artifact bundle: lowering record v9, typed semantics v8,
  source-map v2 and metadata schema 72. Rebuild all four files together.
- Browser compiler: metadata only. A rebuilt bundle must pass the canonical
  Docker build and 600 KB gzip limit.

## Required evidence

| Requirement | Candidate status | Completion evidence |
| --- | --- | --- |
| Coordinated versions and dependency locks | Prepared | Workspace, verifier and editor manifests; tooling-release validator |
| Native development and CI checks | Pending | Fresh `dev` and `ci` logs and cost reports from this candidate |
| Backend and pinned local-node replay | Pending | Fresh `backend` report, 43 actions, 17 Lock matrices and 26 stateful scenarios |
| Browser and editor distributions | Built and locally validated | Canonical WASM and VSIX identities below; website production/testnet build passed |
| Independent compiler/checker release review | Pending | Named reviewer, reviewed commit range and finding dispositions |
| Selected-network evidence | Pending | New exact-artifact deployment/readback evidence if included in the release scope |
| Full clean-source release gate | Pending | `./scripts/cellscript_gate.sh release` on the final committed candidate |
| Publication | Not performed | Matching version tag and verified selected distribution channels |

The strict business-corpus preflight must reject this candidate while required
acceptance remains pending. Development checks validate inventory integrity;
they do not promote pending review, deployment or release results to passed.
Before running the final gate, reconcile each requirement with its actual
evidence. Preserve the unchanged budgets and historical report identities.

## Prepared bundles

The canonical Docker build produced a 1,508,199-byte WASM binary (572,053
bytes gzip; 614,400-byte limit). Its SHA-256 is
`2dfc01cf5203df1b74b8a753a1c4cb0a3e610bc401d0028b3ded6152cbe0f874`.
The loaded module reports version 0.31.0 and compiles an Edition 2026 action
through the metadata-only API. Worker and page cache keys bind that digest.
The build script forwards configured download proxies by environment-variable
name; a loopback proxy requires its existing host-network option.

The packaged VSIX is `target/release-preparation/cellscript-vscode-0.31.0.vsix`,
SHA-256 `5ee48a6cbc2c0e771ac768190e8c8710b794043d3eecd73dd39e62b904ac266c`.
The extension manifest validator passed. These are local candidate artifacts;
no marketplace or website deployment receipt is implied.

The imported audit correction has an additional executed regression:
`byte_loop_prefix_offset_preserves_success_and_last_byte_rejection_in_ckb_vm`
checks optimization levels 0 and 3, valid execution, and exact assertion failure
after changing the final compared byte. The five Node client-IP boundary tests
pass. The full CI/backend gates remain the release validation contract.

## Fresh resource evidence

The four-artifact business anchor retains its ELF and transaction identities.
Its lowering/source-map and verified-bundle identities are refreshed because
those records bind the promoted compiler/checker version. The protocol-bundle
identity is refreshed with them; fixture inputs and budget ceilings are unchanged.

The NovaSeal fixed BIP340 envelope was recompiled and executed against the
reproducibly built child. All four cases matched (one acceptance and three
rejections), including the full transaction verifier and both exact IPC transfers.
Measured cycles fell from 3,679,593 to 3,679,149; parent ELF size fell from 13,472
to 12,768 bytes. Stack (15,792), witness (418), transaction (879), and child ELF
(100,912) byte counts are unchanged. The resource profile records these exact
measurements with the existing ceilings. The published 0.30 matrix remains a
historical record. NovaSeal Rust tooling checks and both transaction-measure
unit tests also passed.

## Distribution and compatibility

Native users must pair the compiler with the 0.31 checker and Registry
verification workers. Recompiled ELFs may have different Script hashes and
transaction group ordering. Existing deployed Cells retain their old identity;
the 0.30 Pudge receipts cannot authenticate newly compiled code.

The website can retain its published 0.30 download links while testing the
0.31 browser candidate. Change public release links only when matching assets
exist. If crates.io is selected, publish and verify the standalone checker
before publishing the compiler. See [the candidate release notes](CELLSCRIPT_0_31_RELEASE_NOTES.md).
