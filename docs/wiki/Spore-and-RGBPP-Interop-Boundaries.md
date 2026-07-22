# Spore and RGB++ Interoperability Boundaries

This page records what CellScript 0.22 does and does not claim for Spore and
RGB++. These integrations belong at the package, adapter, and transaction
builder boundary. They are not reasons to add protocol-specific namespaces to
the core standard library.

## Current Support Matrix

| Ecosystem | 0.22 boundary | What must be pinned before use |
|---|---|---|
| Spore | The bounded `examples/ecosystem/spore-identity-adapter` composition lock checks one input/output pair against a pinned Spore Type Script identity. There is no built-in Spore namespace and the adapter does not reimplement Spore rules. | Contract code hash/version, schema revision, deployment, SDK revision, and positive/negative CKB fixtures. |
| RGB++ | The bounded `examples/ecosystem/rgbpp-identity-adapter` sidecar preserves an Rgbpp Lock deployment and exact 36-byte input/output args. There is no built-in RGB++ or Bitcoin SPV profile. | SDK revision, every referenced script deployment, witness/commitment layout, Bitcoin confirmation policy, and CKB/Bitcoin fixtures. |

“Compiles to RISC-V” is not interoperability evidence. A cookbook entry is
only promoted when its source identities, ABIs, deployments, builder output,
and negative cases are reproducible.

## Spore Uses Contract Identity as Version

Spore recommends its SDK for application builders and treats contract
`code_hash` as the version boundary. A CellScript cookbook package therefore
must import or reproduce the selected Molecule schemas, pin the deployed
contract identity, and use the SDK-compatible builder flow. It must not copy a
few field names and call that Spore compatibility.

Reference: [Spore contracts](https://github.com/sporeprotocol/spore-contract).

## RGB++ Is More Than a Merkle Primitive

RGB++ combines Bitcoin-side transactions and commitments with CKB virtual
transactions, Rgbpp Lock witnesses, deployed scripts, confirmation handling,
and service/builder orchestration. Its early design repository explicitly says
that the documents are not a final standard. The maintained SDK moved to the
`RGBPlusPlus` organization.

CellScript 0.22 provides fixed-width `ckb::hash_sha256`,
`ckb::hash_sha256d`, pair hashing, and a depth-16
`ckb::require_sha256d_merkle_root` helper. These are executable lower-level
building blocks with CKB-VM differential tests; they are not an RGB++ or
Bitcoin verifier.

Consequences for CellScript:

- SHA-256/SHA256d and the bounded Merkle path remain lower-level building
  blocks;
- Bitcoin header validation, difficulty, confirmations, reorg policy,
  RgbppLock/BtcTimeLock layouts, and transaction orchestration remain in a
  pinned profile/package and builder;
- Spore-over-RGB++ combines both protocols' identity and fixture requirements.

References: [active RGB++ SDK](https://github.com/RGBPlusPlus/rgbpp-sdk) and
[RGB++ early design documents](https://github.com/utxostack/RGBPlusPlus-design).

## Evidence Checklist

Before describing an adapter as compatible, require all of the following:

1. immutable upstream revision and license/provenance record;
2. exact CKB `Script` identities and deployment cells;
3. canonical Molecule/witness/message layouts;
4. generated transaction fixtures compared with the maintained SDK;
5. valid and invalid CKB-VM cases, including wrong deployment, wrong witness,
   wrong commitment, and replay/domain mismatch where applicable;
6. an explicit evidence tier in metadata and release notes.

The two repository packages currently pass compiler/package checks and bind the
documented identities. They have not passed the full SDK-generated,
cross-chain, deployment, and negative-fixture checklist. Describe them as
“bounded identity adapters”, not protocol-compatible or production-ready.
