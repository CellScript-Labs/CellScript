# CellScript 0.33.0

Release date: 2026-11-19. This is the stable 0.33 publication of the line
introduced by [0.33.0-alpha.1](CELLSCRIPT_0_33_ALPHA_1_RELEASE_NOTES.md) and
[0.33.0-alpha.2](CELLSCRIPT_0_33_ALPHA_2_RELEASE_NOTES.md). Relative to
v0.33.0-alpha.2 it contains only the security-hardening fixes below; the
language surface, artifact schemas, metadata identities and public APIs are
unchanged from alpha.2. The
[source contract](../CELLSCRIPT_SOURCE_CONTRACT.md) defines the current
compiler, artifact and consumer boundary.

## Included changes

An independent security review of the exact v0.31.0 → v0.33.0-alpha.2 source
diff reported four findings (one medium, three low). All four are fixed in
this release, each with a focused regression test:

- Compiler resource amplification (medium): a compact source declaring a very
  large fixed-length array type and one projected assignment expanded the
  array element count into per-component IR before any resource budget ran.
  Local aggregate rebuilds now enforce a per-aggregate component budget of
  65,536 and a cumulative module budget of 1,048,576 components, rejecting
  oversized rebuilds during type checking before any expansion work, on the
  native compile, metadata-only, LSP and browser paths alike.
- ZK parent request lifetime (low): the independent checker verified each ZK
  request word's initial store but did not prove the transmitted bytes stayed
  bound until every PIPE WRITE. The checker now rejects any store inside the
  ZK region that touches a request word after the checked initialization,
  including stores through unresolved bases and atomic operations, which are
  outside the admitted region contract.
- Fixed Cell-field copy certification (low): the fixed-cell-field certificate
  trusted the `__cellscript_memcpy_fixed` helper's structural frame rules
  without confining its stores. The exact bounded byte-copy instruction
  sequence — whose only store writes through the certified destination
  argument — is now an unconditional precondition of every certified frame
  copy, not only of fixed-result returns.
- Registry snapshot hash reservation (low): an unverified publish under one
  namespace could reserve a content hash and, after rejection, leave a
  tombstone that permanently denied another namespace's first legitimate
  publication of the same bytes. A tombstoned hash is now reclaimable by the
  next namespace at admission, in both the SQL and in-memory stores, while a
  live unverified admission keeps its cross-tenant conflict protection.

## Packages and rebuilt artifacts

Every participating `Cell.toml` declares the source edition and an explicit
compiler requirement. Requirements written as `>=0.33.0-alpha.2` already admit
this release; for an exact compiler pin:

```toml
[package]
edition = "2027"
cellscript_version = "=0.33.0"
```

Review the package graph, then use `cellc lock` for an intentional repin.
Build/check/test validate existing pins; `--frozen` additionally forbids network
access and writes. Rebuild metadata, lowering records, source maps, interfaces
and generated builders together with their matching compiler/checker. Browser
metadata compilation remains separate from native ELF admission.

## Validation and publication scope

The v0.33.0-alpha.2 tree carrying these fixes passed the focused behavioral
suites owning each changed contract:

```bash
cargo test --locked -p cellscript --lib                      # 1139 passed
cargo test --locked -p cellscript --test frontend_boundaries # 14 passed
cargo test --locked -p cellscript --test artifact_checker    # includes new ZK request-lifetime mutation
cargo test --locked -p cellscript --test policy_artifact_checker
cargo test --locked -p cellscript --test zk_transition
cargo test --locked -p cellscript-artifact-checker
cargo clippy --locked -p cellscript --all-targets -- -D warnings
cargo clippy --locked -p cellscript-artifact-checker --all-targets -- -D warnings
```

The Registry API store contracts ran under vitest against both the in-memory
store and an isolated PostgreSQL instance (184 passed), including the new
cross-namespace snapshot reclaim regression and the existing rejected-snapshot
cleanup suite. The packaging commit re-ran `./scripts/cellscript_gate.sh
commit` on the exact tagged tree; its log ships with the release attachments.

Backend and production/stateful acceptance have their own evidence
requirements. Use the release's matching source-commit receipts for
`./scripts/cellscript_gate.sh backend` and pinned CKB acceptance results;
compile-only checks and the gates above do not establish those results.
No `release` or `release-quick` gate result is claimed here. See the
[gate policy](../CELLSCRIPT_GATE_POLICY.md) for the required evidence.

Each release attachment must identify the same reviewed source commit and
carry its checksum and matching validation receipts. Updating GitHub release
attachments does not establish crates.io publication, website deployment or
Marketplace publication; those distributions require their own evidence.

## Retained boundaries

The alpha.2 scope boundaries are retained unchanged: `ExactCellSelection` is
reserved, default-build alias evidence, general nominal representation and the
remaining Cell/selection family work stay open under
[#96](https://github.com/CellScript-Labs/CellScript/issues/96), unbounded
vectors with non-fixed elements remain outside the admitted codec, and
deferred runtime semantics fail closed with their recorded errors.

Finite source, checker and VM tests do not prove arbitrary program equivalence
or application-specific production safety. The research equivalence line keeps
`equivalence_status = NOT_PROVEN` and `production_equivalence_claim = false`.
The ZK transition profile remains a local-single-party engineering scope; its
`production_admitted` flag does not claim independent review, an MPC ceremony
or public-network deployment. See the
[alpha.2 backlog](CELLSCRIPT_0_33_ALPHA_2_TODO.md) for the remaining owned
work and acceptance requirements that carry into this line.
