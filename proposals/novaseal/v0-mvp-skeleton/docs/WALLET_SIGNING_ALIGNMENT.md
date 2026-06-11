# NovaSeal Wallet Signing Alignment

**Status**: Alignment probe passes for the current packed-intent lock digest.

This document records the current relationship between:

- the canonical packed-reference wallet message, and
- the actual digest currently passed by `btc_authority` to the delegated BIP340 verifier.

The current state and lock sources sign `hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })`, matching the canonical packed-reference vector digest.

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

For each of the eleven v0 fixtures the script records:

- `canonical_wallet_message32`: `signed_intent_hash_after_resolved_receipt` from `target/novaseal-canonical-vectors.json`
- `current_lock_message32`: the same `hash_blake2b_packed(NovaSealSignedIntentV0 { core, expected_receipt_hash })` digest used by the current lock source
- whether the two 32-byte messages match
- one deterministic BIP340 positive vector for the canonical wallet message
- one deterministic BIP340 positive vector for the current lock message
- cross-checks proving that both signatures verify under the same message

The report is intentionally fail-closed if the source model reintroduces the old domain-hash rule. The expected current result is:

```json
"wallet_lock_alignment_ready": true,
"production_wallet_ready": true
```

for local wallet/lock digest alignment. This is not a substitute for public CellDep pinning, public BTC SPV evidence, or external BIP340 TCB attestation.

---

## Current Result

Expected current summary:

```text
fixtures=11
canonical_wallet_vectors_self_verified=11
current_lock_digest_matches_canonical=11
current_lock_digest_mismatches=0
wallet_lock_alignment_ready=True
```

The active lock rule is:

```cell
let digest = hash_blake2b_packed(intent)
verifier::btc::bip340::require_signature(digest, sig.pubkey, sig.signature)
```

The canonical vector rule signs the resolved packed-reference intent after receipt-hash resolution:

```text
signed_intent_hash_after_resolved_receipt
```

---

## Production Gate

Wallet/lock digest readiness is claimed only when the mechanical gate passes:

- all eleven fixtures have `canonical_vs_current_lock_digest_match=true`
- canonical wallet signatures verify under the lock digest
- current lock signatures verify under the canonical wallet digest
- the combined lock+type harness signs the production message
- source checks show `hash_blake2b_packed(intent)` and no legacy `compute_intent_hash` or `hash_blake2b(intent.domain)` path

NovaSeal production readiness still additionally requires the external gates tracked by `cellc certify --plugin novaseal-profile-v0 --require-production`.
