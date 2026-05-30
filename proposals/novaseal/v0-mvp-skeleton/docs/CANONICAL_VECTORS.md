# NovaSeal v0 Canonical Test Vectors

**Date**: 2026-05-30
**Generator**: `scripts/novaseal_canonical_vectors.py`
**Report**: `target/novaseal-canonical-vectors.json`
**Encoding profile**: `packed-fixed-v0-reference`

This slice produces deterministic packed-reference byte vectors from the current fixture JSON files and `target/novaseal-schema-layout.json`.

It is not Molecule output, not CKB VM witness encoding, and not BTC wallet signing material.

## Current Result

Run:

```bash
python3 scripts/novaseal_schema_layout.py --pretty
python3 scripts/novaseal_canonical_vectors.py --pretty
```

Current summary:

```text
vectors=6
intent_vectors=6
receipt_matches=0
resolved_receipt_matches=6
classification=packed_reference_test_vectors
receipt_commitment_status=resolved_candidate_without_intent_hash
```

The generator emits:

- one `NovaSealCellV0` old-cell vector per fixture,
- one `NovaSealIntentV0` vector per fixture,
- one candidate `ProofReceiptV0` vector per fixture,
- one accepted new-cell vector for the positive fixture.

Fixture placeholders such as `0xabc...` are deterministically converted to 32-byte test values with `blake2b-256(person=NovaSealVecV0)`. This keeps test vectors stable without pretending the placeholder strings are real protocol constants.

## Receipt Commitment Rule

The naive full-receipt rule is circular:

1. `NovaSealIntentV0.receipt_hash` is intended to commit to `ProofReceiptV0`.
2. `ProofReceiptV0.intent_hash` commits to the encoded `NovaSealIntentV0`.
3. The encoded `NovaSealIntentV0` includes `receipt_hash`.

The selected packed-reference candidate breaks the cycle by excluding `ProofReceiptV0.intent_hash` from the `receipt_hash` preimage:

```text
receipt_hash = blake2b-256(ProofReceiptV0 fields in schema order, excluding intent_hash)
signed_intent_hash = blake2b-256(full NovaSealIntentV0 including receipt_hash)
ProofReceiptV0.intent_hash = signed_intent_hash
```

The generator records both facts:

```text
computed_receipt_candidate_hash_matches_intent = 0
resolved_receipt_hash_matches_intent = 6
resolved_receipt_verification_preimage_matches = 6
```

The zero is the legacy full-receipt candidate and remains a useful warning. The six resolved matches prove the selected rule is internally consistent for the fixture set.

See `docs/RECEIPT_COMMITMENT_SPEC.md` for the exact preimage rule and remaining production limits. The next layer, `target/novaseal-btc-verifier-vectors.json`, signs `signed_intent_hash_after_resolved_receipt` with the BIP340 profile documented in `docs/BTC_VERIFIER_SPEC.md`.
