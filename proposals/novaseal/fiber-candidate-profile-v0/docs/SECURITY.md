# NovaSeal Fiber Candidate Profile v0 Security

## Implemented Guards

- Settlement binds channel, route, payment, balance, amount, and payout
  commitments.
- Settlement requires the operator BIP340 authority.
- Settlement rejects no-op balance commitment replay.
- The transition increments nonce exactly once and recomputes the canonical
  NovaSeal envelope before acceptance.

## Not Implemented

- In-contract verification of Fiber node state.
- HTLC, path, route, fee, liquidity, or revocation verification inside the
  NovaSeal transition.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.

## Risk Posture

This package has source-level application-profile evidence, live CKB stateful
candidate evidence, and external Fiber workflow execution evidence is present.
It does not verify Fiber HTLCs, routes, liquidity, fees, or revocations inside
the CellScript profile. General NovaSeal production readiness remains blocked
until the public/shared CellDep attestation, public BTC SPV evidence, RWA
legal/registry review evidence, and external BIP340 TCB review are accepted.
