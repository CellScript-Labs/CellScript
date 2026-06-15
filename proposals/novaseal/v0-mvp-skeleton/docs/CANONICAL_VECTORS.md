# NovaSeal v0 Canonical Test Vectors

**Date**: 2026-05-31
**Generator**: `scripts/novaseal_canonical_vectors.py`
**Report**: `target/novaseal-canonical-vectors.json`
**Encoding profile**: `packed-fixed-v0-reference`

This slice produces deterministic packed-reference byte vectors from the eleven
current fixture JSON files and `target/novaseal-schema-layout.json`.

It is not Molecule output, not CKB VM witness encoding, and not BTC wallet
signing material. Fixed-width wallet signing vectors are generated separately
by `/home/arthur/a19q3/CellScript/scripts/novaseal_wallet_signing_vectors.py`
into `/home/arthur/a19q3/CellScript/target/novaseal-wallet-signing-vectors.json`.
These canonical vectors are the input/foundation layer, not the final signing
layer.

## Current Result

Run:

```bash
python3 scripts/novaseal_schema_layout.py --pretty
python3 scripts/novaseal_canonical_vectors.py --pretty
```

Current summary:

```text
vectors=11
intent_core_vectors=11
signed_intent_vectors=11
receipt_commitment_vectors=11
accepted_new_cell_vectors=1
resolved_receipt_hash_matches_intent=11
new_cell_latest_receipt_hash_matches=11
classification=packed_reference_test_vectors
receipt_commitment_status=split_intent_and_explicit_receipt_commitment
```

The generator emits:

- one `NovaSealCellV0` old-cell vector per fixture,
- one `NovaSealIntentCoreV0` vector per fixture,
- one `NovaSealSignedIntentV0` vector per fixture,
- one `ProofReceiptCommitmentV0` vector per fixture,
- one materialized `ProofReceiptV0` vector per fixture,
- one accepted new-cell vector for the positive fixture.

Fixture placeholders such as `0xabc...` are deterministically converted to
32-byte test values with `blake2b-256(person=NovaSealVecV0)`. This keeps test
vectors stable without pretending the placeholder strings are real protocol
constants.

## Receipt Commitment Rule

The current packed-reference rule is:

```text
intent_core_hash = hash_blake2b_packed(NovaSealIntentCoreV0)
new_cell_commitment = hash_blake2b_packed(NovaSealCellCommitmentV0)
latest_receipt_hash = hash_blake2b_packed(ProofReceiptCommitmentV0)
signed_intent_hash = hash_blake2b_packed(NovaSealSignedIntentV0)
```

`NovaSealSignedIntentV0` contains `{ core, expected_receipt_hash }`, where
`expected_receipt_hash` must equal the materialized `ProofReceiptCommitmentV0`
hash. `NovaSealCellCommitmentV0` excludes `latest_receipt_hash`; that exclusion
is what prevents a new-cell/receipt commitment cycle.

See `docs/RECEIPT_COMMITMENT_SPEC.md` for the exact preimage rule and remaining
production limits. The next layer, `target/novaseal-btc-verifier-vectors.json`,
signs `signed_intent_hash_after_resolved_receipt` with the BIP340 profile
documented in `docs/BTC_VERIFIER_SPEC.md`.
