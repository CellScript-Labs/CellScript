# CellScript Spore and RGB++ Interoperability Plan

**Status**: Partially delivered; executable primitives and bounded identity
adapters, no production-support claim

**Target**: post-0.22 packages/adapters, not the core namespace

**Updated**: 2026-07-21

This plan tracks the signature, CellDep-scan, hashing, and ecosystem gaps as
independently testable vertical slices. CellScript 0.22 now ships the bounded
compiler/runtime pieces described below. It deliberately keeps Spore, RGB++,
and Bitcoin SPV protocol rules out of `std::*`.

## 1. Deployable Signature Verifier Package

Delivered compiler boundary:

- `verifier::btc::bip340::require_signature_from_cell_dep` selects a literal
  resolved CellDep index and emits the executable VM2 pipe/spawn/wait path;
- `ckb::require_cell_data_hash` can bind that resolved dependency to a pinned
  data hash before spawn;
- the fixed 144-byte `cellscript-btc-bip340-ipc-v0` envelope is documented in
  `docs/CELLSCRIPT_SIGNATURE_VERIFIER_ABI.md` and has a compatible no-std
  verifier package under `proposals/novaseal/v0-mvp-skeleton`.

Still required for each application profile and deployment:

- algorithm and canonical public-key/signature encoding;
- message-domain tag and exact CKB sighash construction;
- ScriptGroup and WitnessArgs selection;
- witness-lock placeholder and hashing rules;
- verifier binary/data hash and CellDep identity;
- replay policy across chain, script, action, and proposal identity;
- valid, wrong-message, wrong-key, malformed-signature, wrong-group, wrong-dep,
  and replay fixtures on CKB-VM.

No `std::secp256k1_verify` or fake syscall is introduced. The verifier call
checks only a supplied BIP340 prehash. Domain separation, ScriptGroup/witness
selection, sighash construction, replay policy, authority binding, public
deployment, and external TCB review stay explicit package evidence. The
renamed `Approval` example remains non-cryptographic and relies on a
surrounding Lock Script for authority.

## 2. Executable Bounded CellDep Scan

The first executable slice is delivered as
`ckb::require_bounded_cell_dep_data_hash(max_deps, expected_data_hash)` with a
literal `1..=64` bound. It scans the resolved `Source::CellDep` sequence with
`LOAD_CELL_BY_FIELD(DATA_HASH)`, stops on `INDEX_OUT_OF_BOUND`, and rejects with
stable runtime code `63` when no match is found. The exact-index companion is
`ckb::require_cell_data_hash(source::cell_dep(index), expected_hash)`.

Remaining promotion work:

1. keep out point and `dep_type` pinned in manifest/builder evidence;
2. distinguish direct CellDeps from their original DepGroup container outside
   the VM, whose syscall view exposes only the resolved sequence;
3. add any future script/out-point scan one finite field contract at a time;
4. reject unbounded or unsupported traversal.

Metadata-only quantifiers stay `runtime-helper-required`; documentation and
gates must not promote them based on syntax alone.

## 3. Hash and Merkle Building Blocks

Existing CKB-personalized BLAKE2b helpers remain the CKB-native primitive.
CellScript 0.22 now lowers SHA-256 and SHA256d for exact 32-byte values and
64-byte pairs, plus a SHA256d Merkle path with a literal depth in `0..=16`.
The delivered slice has:

- fixed ownership and length ABI;
- explicit raw-byte leaf/node ordering and caller-owned endianness;
- a compile-time path bound and cycle budget;
- Rust `sha2` differential vectors plus positive and negative CKB-VM tests;
- no claim that the helper implements Bitcoin SPV.

Bitcoin header chains, PoW/difficulty, confirmations, reorg policy, and
deployment identity belong to a separately pinned Bitcoin SPV package.

## 4. Ecosystem Cookbook Packages

The repository now includes two compile-checked, non-production identity
adapters. Each package is promoted independently.

### Spore

- Use the maintained Spore SDK and schema definitions.
- Pin `code_hash`/deployment identity because Spore versions contracts by code
  hash.
- `examples/ecosystem/spore-identity-adapter` binds one transfer-shaped
  input/output pair to the exact Type Script code hash, hash type, and Spore ID.
- SDK-generated invalid schema, cluster, and deployment fixtures remain a
  promotion requirement.

### RGB++

- Use the active `RGBPlusPlus/rgbpp-sdk`; do not pin the archived predecessor.
- Treat the early design repository as background, not normative final ABI.
- `examples/ecosystem/rgbpp-identity-adapter` preserves one Rgbpp Lock
  deployment and exact 36-byte input/output args in a sidecar policy Cell.
- Pin RgbppLock, BtcTimeLock, BTC SPV, witness/commitment, confirmation, and
  deployment identities before protocol-level promotion.
- Keep Bitcoin orchestration in the builder/service layer and include CKB plus
  Bitcoin-side fixtures.
- Add a combined Spore-over-RGB++ fixture only after both independent adapters
  pass their own promotion gates.

## 5. Promotion Gate

An adapter moves from “proposed” to “supported” only when parser/type/lowering,
metadata, LSP, docs, generated builder, pinned upstream sources, artifact
identity, and CKB-VM positive/negative tests agree. A package that depends on
Bitcoin must also carry reproducible external fixtures and reorg/finality
assumptions.

The current user-facing boundary is documented in
[`docs/wiki/Spore-and-RGBPP-Interop-Boundaries.md`](../docs/wiki/Spore-and-RGBPP-Interop-Boundaries.md).
