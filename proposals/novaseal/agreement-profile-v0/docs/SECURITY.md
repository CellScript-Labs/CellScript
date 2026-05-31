# Security Notes

## Implemented Guards

- CKB-only asset kind/hash checks at origination.
- Positive collateral and principal.
- Start timepoint before expiry.
- Origination only during the agreed window.
- Borrower-only repayment terminal path.
- Lender-only default claim terminal path.
- Repayment only before or at expiry.
- Claim only after expiry.
- Status moves from `Active` to a terminal status.
- Nonce increments on terminal paths.
- Receipt output is materialized on every implemented path.
- Typed native CKB payout outputs are materialized for principal payout,
  repayment, collateral return, and default claim paths.
- Terms hash and receipt hash output mismatches reject in resolved transaction
  evidence.
- Local transaction-shape harness checks output occupied-capacity floors and
  CKB economic amounts for origination, repayment, and default claim.
- The legacy per-action CKB VM harness is superseded by the signed-intent
  lifecycle runner; live devnet evidence now checks the terminal lifecycle
  paths directly.
- Resolved transaction harness runs deterministic CKB transactions through
  `ckb-script` and `ckb-verification`, including transaction-layer
  under-capacity rejection.

## Not Implemented

- Canonical terms hash preimage verification.
- Canonical receipt hash preimage verification.
- Wallet/builder plumbing that maps typed payout outputs to production CKB
  recipient locks and change handling.
- Cryptographic borrower/lender authority locks.
- BTC UTXO mirror, SPV, OP_RETURN, or BTC finality.
- iCKB, xUDT, Fiber, or channel execution.
- Dynamic interest, oracle price, margin call, or liquidation bot.

## Risk Posture

This is not production ready. It is a reviewable skeleton for terminal-path
semantics, local builder-shape evidence, resolved transaction evidence, live
devnet lifecycle evidence, and audit surface development.
