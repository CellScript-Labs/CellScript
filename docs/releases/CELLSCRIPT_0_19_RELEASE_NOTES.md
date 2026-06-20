# CellScript 0.19 Closure Notes

**Status**: Final closure notes for the 0.19 scope.

**Updated**: 2026-05-09.

**Release tag**: TBD.

## Release Boundary

CellScript 0.19 closes the first package/provenance layer needed before
generated transaction builders can be trusted. The release scope is:

- CKB ecosystem reuse boundary;
- `ckb-std` compatibility and ABI parity;
- grammar / syntax governance for the current public surface;
- Phase 1 Git-backed source package registry;
- lockfile-bound package/build/deployment identity;
- fail-closed `cellc package verify` and `cellc registry verify`.

0.19 deliberately does not ship generated TypeScript builders, live-chain
registry certification, stateful transaction-flow runners, publisher signature
policy, an on-chain registry/index/proxy, wallet UI, or CellFabric intent-DAG
composition. Those are now 0.20 or later scope.

## Shipped Capabilities

Package identity:

- `cellc init --namespace` writes namespace-aware package manifests.
- `cellc publish` writes Git registry records with source roots, explicit entry
  parent handling, and source hashes that include `Cell.toml`.
- registry resolution supports namespace/name/version/tag/source-hash records.
- `CELLSCRIPT_REGISTRY_URL` can point to local or private Git-backed registries.

Build identity:

- `cellc build` writes `Cell.lock` identity for compiler version, target
  profile, artifact hash, metadata hash, schema hash, ABI hash, and constraints
  hash.
- compile-time dependency loading resolves path, git, and registry
  dependencies through the package manager.
- registry dependencies are checked out at the recorded tag and verified
  against the recorded source hash before source loading.

Verification:

- `cellc package verify` validates source/build identity and fails closed on
  mismatches.
- `cellc registry verify` validates off-chain deployment facts against
  lockfile-bound build/package identity and fails closed in text and JSON modes.
- local registry fixtures cover publish, resolve, source-root hashing,
  source-hash mismatch rejection, and JSON fail-closed verification.

CKB ecosystem boundary:

- `src/ckb_abi.rs` centralizes inline CKB ABI constants.
- `tests/ckb_std_compat.rs` covers `ckb-std` / `ckb-types` parity for ABI
  constants, SourceView, WitnessArgs, TYPE_ID, since/epoch, and occupied
  capacity.
- `crates/cellscript-ckb-adapter` remains the Rust-side headless transaction
  materialization boundary; compiler core does not become a wallet, indexer, or
  chain submission layer.

## Acceptance Evidence

Required validation for the closed 0.19 scope:

```text
cargo fmt --all
cargo check --locked -p cellscript --all-targets
cargo test --locked -p cellscript
cargo clippy --locked -p cellscript --all-targets -- -D warnings
git diff --check
```

Focused Phase 1 evidence lives in:

- `tests/registry.rs`
- `tests/cli.rs`
- `tests/e2e_registry_devnet.rs` for broader offline/headless registry and
  deployment identity scenarios, with live devnet tests intentionally ignored by
  default
- `docs/CELLSCRIPT_REGISTRY_PHASE1.md`
- `docs/CELLSCRIPT_0_19_ROADMAP.md`
- `docs/CELLSCRIPT_0_20_ROADMAP.md`

Implementation and scope commits:

- `c84a1d6 Complete registry phase1 verification flow`
- `9624fe8 Document 0.19 scope closure and 0.20 handoff`

## Explicit Non-Claims

0.19 does not prove:

- that a deployment cell is live on a real network;
- that generated TypeScript builders can construct valid transactions;
- that CCC wallet/signing integration is complete;
- that stateful multi-transaction business flows are committed on CKB;
- that deployment records are signed by publishers or auditors;
- that an on-chain registry, type-script index, or proxy exists;
- that CellFabric cross-protocol intent planning is implemented.

Those claims require 0.20 evidence.

## 0.20 Handoff

0.20 starts from the 0.19 identity layer and should add:

- `cellc gen-builder --target typescript`;
- generated-builder tests and negative builder-shape rejection fixtures;
- live-chain deployment verification through CKB RPC / indexer APIs;
- stateful flow runner evidence for canonical examples;
- registry trust hardening for signatures, trust anchors, mutable channels,
  yanking/revocation, and optional on-chain registry/index/proxy design.
