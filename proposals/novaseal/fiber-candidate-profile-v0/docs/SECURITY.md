# NovaSeal Fiber Candidate Profile v0 Security

## Implemented Guards

- Settlement binds channel, route, payment, balance, amount, and payout
  commitments.
- Settlement requires the operator BIP340 authority.
- Settlement rejects no-op balance commitment replay.
- The transition increments nonce exactly once and recomputes the canonical
  NovaSeal envelope before acceptance.

## Not Implemented

- Live Fiber node/channel execution.
- HTLC, path, route, fee, liquidity, or revocation verification.
- Live devnet Fiber candidate evidence.
- Wallet signing vectors for this profile.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.

## Risk Posture

This package is source-level application-profile evidence, not proof of a Fiber
payment-network execution. V1 readiness must remain blocked until live Fiber
candidate evidence exists.
