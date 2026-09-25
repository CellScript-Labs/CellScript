# CellScript 0.30.0 Release Notes

**Published (2026-09-23):** signed tag `v0.30.0` identifies
`352b4950e7dc7489a4ee7e7ed6d6edf4c7fefd71`. The accepted GitHub assets and
production/testnet websites are available. See the
[publication closure](CELLSCRIPT_0_30_ISSUE_STATUS.md#publication-closure--2026-09-25)
for workflow and readback receipts; older candidate records remain historical.

**Publication decision (2026-09-21):** the maintainer authorized the 0.30.0
release and the cellscript.dev update, superseding the earlier no-TAC hold.
Publication requires a fresh full release gate on the final clean source.
GitHub assets and the website are released together; crates.io and extension
marketplace publication retain their own independently verified status.

## Highlights

The 0.30 line combines the unreleased 0.26/0.26b implementation with the bounded
capability portfolio. The stable predecessor is 0.25; there is no intervening
stable 0.26 release requirement.

- Bounded GroupInput consumption and GroupOutput plans enforce cardinality,
  ordering, exact identities, linear discharge, and output correspondence.
- ProtocolBundle composes independently checked artifacts, closed roles,
  witnesses, deployment identities, live resolution, external signing, node
  admission, submission, and confirmation through distinct evidence states.
- Typed Cell, Script, witness, HeaderDep, and Since views expose bounded CKB
  transaction data. Fixed-width commitments authenticate openings before
  materializing their values. External verifiers retain an explicit trusted
  boundary.
- Lock-authoritative packages enforce compiler requirements, canonical workspace
  graphs, exact chain identity, one instance per package coordinate, deterministic
  build plans, and transactional upgrade receipts.
- Eight frozen business families bind positive and adversarial cases to exact
  artifacts and transactions, with maximum-shape resource budgets and independent
  artifact checks.
- Cost checks fail when required tooling is missing and produce fresh execution
  reports. Matched samples have independent byte/cycle ceilings; bounded growth
  cases measure ELF, stack frames, witness bytes, and cycles. See the
  [cost regression contract](../CELLSCRIPT_COST_REGRESSION.md).

## Compatibility boundary

| Axis | 0.30.0 contract |
|---|---|
| Compiler and checker | `0.30.0`; Rust toolchain remains `1.97.1` |
| Default source edition | `2026`, `cellscript-source-semantics-2026` |
| Opt-in authoring edition | `2027`, `cellscript-source-semantics-2027-0.30-v1` |
| Compile/source/artifact/constraints metadata | `72` / `2` / `1` / `4` |
| Typed semantics / lowering record / source map | `v8` / `v8` / `v2` |
| Package lock | `Cell.lock` v5; old locks migrate only by explicit repinning |
| Entry witness | `cellscript-entry-witness-v1`; WitnessArgs input_type placement v2 |
| Explicit Type policy witness | `cellscript-policy-witness-v1`; policy input_type placement v1 |
| Bounded outputs / composition | `bounded-output-plan-v1` / `cellscript-protocol-bundle-v1` |

The accepted 2027 identity freezes the bounded source contract implemented on
the 0.30 line. It does not redefine `preview4`, `authoring1`, or `0.30-dev1`,
make Edition 2027 the default, or promise unrestricted language support.
Compiler/checker versions and source identities change sidecar and bundle
identities even when executable bytes remain identical. Regenerate sidecars and
builders together; never relabel an old verified bundle as a new compiler build.
Existing Data1 deployments retain their identity; VM2/Data2 is a separate target
and deployment contract.

## Scope and evidence limits

The comparison is the frozen bounded business portfolio, not arbitrary Rust
equivalence. Generic compatible-open handles (#28), runtime-selected open roles
(#29), circuit DSL/ZK research (#22), unbounded collections, and unrestricted
cryptography remain outside this release. Independent security review is
explicitly waived for this release line; tests do not constitute such a review.

Pudge deployment evidence records twelve immutable code Cells in two transactions
and retains the actual artifact-source commit `3cf60a10`. Reproduction must
compare newly built executable bytes against those recorded identities before
reusing deployment evidence. The record is testnet evidence, not mainnet
deployment or compiler proof of an external verifier's internals.

The browser product compiles metadata only. Its canonical WASM bundle must be
rebuilt from the candidate and remain within 600 KB gzip. Native CLI, generated
builders, VS Code, website, Registry interfaces, and the standalone checker must
agree on the compatibility boundary above.

## Validation and publication

Run on the final source candidate:

```bash
./scripts/cellscript_gate.sh dev
./scripts/cellscript_gate.sh ci
./scripts/cellscript_gate.sh backend
./scripts/cellscript_gate.sh release
```

Both release modes require a clean checkout and invoke
`check-business-corpus --release` before their expensive checks. Candidate
ledgers, incomplete required capabilities, stale evidence, and pending release
layers fail closed. `release-quick` remains compile-only preflight.

The GitHub release workflow is triggered by `v*` tags or manual dispatch. Its
full release gate must pass before binary builds and public publication. Deploy
the website only after its matching release assets are available, and retain
the previous site directory for rollback. A local gate result alone does not
establish publication on any channel.

## Detailed contracts

- [Capability ledger](../CELLSCRIPT_0_30_CAPABILITY_LEDGER.md)
- [2026-09-19 issue status and candidate evidence](CELLSCRIPT_0_30_ISSUE_STATUS.md)
- [Business corpus](../CELLSCRIPT_0_30_BUSINESS_CORPUS.md)
- [Cryptographic capability matrix](../CELLSCRIPT_0_30_CRYPTOGRAPHIC_CAPABILITY_MATRIX.md)
- [Edition policy](../CELLSCRIPT_EDITION_POLICY.md)
- [ProtocolBundle](../CELLSCRIPT_PROTOCOL_BUNDLE.md)
- [Gate policy](../CELLSCRIPT_GATE_POLICY.md)
