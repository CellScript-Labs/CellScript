# NovaSeal v0 Receipt Commitment Spec

**Date**: 2026-05-31
**Status**: split-intent v0 rule implemented in CellScript source and packed-reference vectors.
**Applies to**: `src/nova_state_type.cell`, `scripts/novaseal_canonical_vectors.py`, and `target/novaseal-canonical-vectors.json`.

NovaSeal v0 no longer uses the old "hash ProofReceiptV0 while excluding
intent_hash" candidate. The current rule splits the signed intent from the
receipt commitment so the hash graph is acyclic and auditable.

## Hash Rule

All hashes below use CellScript's canonical packed hash:

```text
hash_blake2b_packed(value) =
  blake2b-256(
    "CellScriptPackedHashV0\0" ||
    canonical_type_name ||
    "\0" ||
    u32_le(byte_len) ||
    packed_bytes
  )
```

For a successful transition:

```text
intent_core_hash =
  hash_blake2b_packed(NovaSealIntentCoreV0)

new_cell_commitment =
  hash_blake2b_packed(NovaSealCellCommitmentV0)

materialized_receipt_hash =
  hash_blake2b_packed(ProofReceiptCommitmentV0)

signed_intent_hash =
  hash_blake2b_packed(NovaSealSignedIntentV0 {
    core,
    expected_receipt_hash: materialized_receipt_hash
  })
```

`ProofReceiptCommitmentV0` is an explicit commitment type, not an implicit
"ProofReceiptV0 minus fields" rule. Its fields are:

```text
protocol_id
package_hash
policy_hash
action
terminal_path
old_cell
new_cell_commitment
old_state_hash
new_state_hash
old_nonce
new_nonce
intent_core_hash
payout_commitment_hash
```

`NovaSealCellCommitmentV0` excludes `latest_receipt_hash`, so the new cell can
commit to its semantic fields without creating a cycle.

## Checks

The `.cell` transition checks:

```text
intent.core.old_cell == source::group_input(0).previous_outpoint
intent_core_hash == hash_blake2b_packed(intent.core)
materialized_receipt_hash == hash_blake2b_packed(ProofReceiptCommitmentV0)
signed_intent_hash == hash_blake2b_packed(intent)
BTC signature verifies signed_intent_hash
intent.expected_receipt_hash == materialized_receipt_hash
new_cell.latest_receipt_hash == materialized_receipt_hash
receipt.intent_core_hash == intent_core_hash
receipt.signed_intent_hash == signed_intent_hash
```

In v0, `latest_receipt_hash` is only the hash of the current successful
transition's receipt commitment. It is not a rolling root, MMR, Merkle root, or
historical accumulator.

## Current Vector Evidence

Run from `proposals/novaseal/v0-mvp-skeleton`:

```bash
python3 scripts/novaseal_schema_layout.py --pretty
python3 scripts/novaseal_canonical_vectors.py --pretty
```

Current summary:

```text
vectors=11
signed_intent_vectors=11
resolved_receipt_matches=11
latest_receipt_matches=11
receipt_commitment_status=split_intent_and_explicit_receipt_commitment
```

## Remaining Limits

This is still packed-reference evidence:

- not Molecule output,
- fixed-width wallet signing material now exists and is included in the local
  production gate,
- not a public/shared deployment attestation,
- not a historical receipt accumulator,
- not real wallet UX/PSBT integration.

Before production, the same preimage rule must be adopted by wallet tooling,
Molecule/reference encoders, deployment manifests, and any external signer UX.
