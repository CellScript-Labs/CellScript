# NovaSeal BTC Transaction Commitment Profile v0 Security

## Implemented Guards

- The committed BTC tuple contains non-zero `btc_txid` and `btc_wtxid`.
- The commitment binds `btc_output_index`, `btc_amount_sats`, and a transition
  commitment hash.
- The transition commitment hash must match the new CKB state hash.
- The committer authority signs the typed intent with BIP340.
- The transition increments nonce exactly once and materialises a receipt.
- The shared `NovaSealCanonicalEnvelopeV0` is recomputed before acceptance.

## Not Implemented

- BTC SPV proof, indexer proof, mempool proof, inclusion depth, or finality.
- Live devnet transition evidence.
- Wallet signing vectors for this profile.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.

## Risk Posture

This package is a source-level commitment profile, not a BTC-public-finality
proof. V1 readiness must remain blocked until a live transition and public BTC
verification evidence exist.
