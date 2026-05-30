# NovaSeal v0 Receipt Commitment Spec

**Date**: 2026-05-30
**Status**: packed-reference candidate rule.
**Applies to**: `scripts/novaseal_canonical_vectors.py` and `target/novaseal-canonical-vectors.json`.

This document resolves the direct `receipt_hash` / `intent_hash` commitment cycle for the current packed-reference vectors. It does not yet change on-chain `.cell` logic or implement a BTC verifier.

## Problem

The naive full-receipt rule is circular:

1. `NovaSealIntentV0.receipt_hash` commits to a receipt.
2. `ProofReceiptV0.intent_hash` commits to the signed intent.
3. The signed intent includes `receipt_hash`.

Hashing the full `ProofReceiptV0` including `intent_hash` therefore cannot produce the `receipt_hash` that is already inside the signed intent.

## Selected v0 Candidate Rule

For the packed-reference vectors:

```text
receipt_hash =
  blake2b-256(
    label = "ProofReceiptV0.receipt_commitment_without_intent_hash",
    preimage = packed ProofReceiptV0 fields in schema order, excluding intent_hash
  )
```

Then:

```text
signed_intent_hash =
  blake2b-256(full packed NovaSealIntentV0 including receipt_hash)
```

And the materialised `ProofReceiptV0.intent_hash` field is:

```text
intent_hash = signed_intent_hash
```

The excluded field list is exactly:

```text
intent_hash
```

## Why This Breaks The Cycle

`receipt_hash` is computed first and does not depend on `intent_hash`.

The BTC-signed intent then commits to that `receipt_hash`.

The receipt can later carry the exact signed intent hash without changing the receipt commitment.

## Current Vector Evidence

`python3 scripts/novaseal_canonical_vectors.py --pretty` currently reports:

```text
computed_receipt_candidate_hash_matches_intent = 0
resolved_receipt_hash_matches_intent = 6
resolved_receipt_verification_preimage_matches = 6
receipt_commitment_status = resolved_candidate_without_intent_hash
```

The first number is the legacy full-receipt candidate. It remains zero, as expected.

The latter two numbers prove the selected candidate rule is internally consistent across all six fixtures.

## Remaining Limits

This is still not production evidence:

- not Molecule output,
- not CKB VM witness encoding,
- not BTC wallet signing material,
- not implemented in `nova_btc_authority_lock.cell`,
- not enforced as a named generated ProofPlan obligation.

Before production, the same preimage rule must be adopted by:

1. off-chain signer / wallet tooling,
2. `nova_btc_authority_lock.cell` intent hash construction,
3. external `novaseal_btc_verifier`,
4. receipt materialisation or witness receipt checker,
5. fixture/VM transaction harness.
