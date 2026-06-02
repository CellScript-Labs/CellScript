# NovaSeal Wallet Signing Alignment

**Status**: Alignment probe added, production wallet readiness remains false.

This document records the current relationship between:

- the canonical packed-reference wallet message, and
- the actual digest currently passed by `btc_authority` to the delegated BIP340 verifier.

The short version is deliberately unfashionable: the vectors exist, but the lock is not yet signing the canonical intent digest. One cannot polish a mismatch into a protocol, not even with a very nice blazer.

---

## Command

Run from `proposals/novaseal/v0-mvp-skeleton/` after schema and canonical vector generation:

```bash
python3 scripts/novaseal_wallet_signing_alignment.py --pretty
```

This writes:

```text
target/novaseal-wallet-signing-alignment.json
```

`scripts/novaseal_fixture_harness.py --pretty` attaches the report when it exists.

---

## What The Report Checks

For each of the six v0 fixtures the script records:

- `canonical_wallet_message32`: `signed_intent_hash_after_resolved_receipt` from `target/novaseal-canonical-vectors.json`
- `current_lock_message32`: `ckb_blake2b_256(intent.domain)`, matching the current `compute_intent_hash` placeholder
- whether the two 32-byte messages match
- one deterministic BIP340 positive vector for the canonical wallet message
- one deterministic BIP340 positive vector for the current lock compatibility message
- cross-checks proving that each signature is rejected under the other message

The report is intentionally fail-closed:

```json
"wallet_lock_alignment_ready": false,
"production_wallet_ready": false
```

until every fixture signs exactly the same digest in wallet, lock, verifier, and harness.

---

## Current Result

Expected current summary:

```text
fixtures=6
canonical_wallet_vectors_self_verified=6
current_lock_digest_matches_canonical=0
current_lock_digest_mismatches=6
wallet_lock_alignment_ready=False
```

This is not a regression. It makes the already-known placeholder explicit:

```cell
fn compute_intent_hash(intent: &NovaSealIntentV0) -> Hash {
    hash_blake2b(intent.domain)
}
```

The canonical vector rule instead signs the resolved packed-reference intent after receipt-hash resolution:

```text
signed_intent_hash_after_resolved_receipt
```

---

## Production Gate

Before wallet readiness can be claimed, one of these must happen:

1. CellScript gains a reviewed way to hash the exact canonical `NovaSealIntentV0` preimage used by the wallet vectors.
2. The protocol intentionally freezes a different message rule and regenerates canonical vectors, verifier vectors, IPC vectors, and lock/type transaction evidence around that rule.

Either way, the acceptance gate is mechanical:

- all six fixtures have `canonical_vs_current_lock_digest_match=true`
- canonical wallet signatures verify under the lock digest
- current lock signatures are no longer a separate compatibility-only path
- the combined lock+type harness signs the production message

Until then, NovaSeal has strong harness evidence for the current compatibility digest, but not production wallet signing alignment.
