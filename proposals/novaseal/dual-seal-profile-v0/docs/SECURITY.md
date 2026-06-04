# NovaSeal Dual Seal Profile v0 Security

## Implemented Guards

- Finalisation binds the sealed BTC UTXO commitment and declared BTC closure
  commitment hash.
- Finalisation is rejected before the CKB maturity timepoint.
- Finalisation requires both BTC owner and CKB authority BIP340 signatures.
- The active dual-seal Cell is consumed and only a terminal receipt is
  materialised.
- The transition increments nonce exactly once and recomputes the canonical
  NovaSeal envelope before acceptance.

## Not Implemented

- BTC SPV proof, indexer proof, inclusion depth, finality, or spend validity.
- Live CKB maturity/finality evidence.
- Live devnet dual-seal finalisation evidence.
- Wallet signing vectors for this profile.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.

## Risk Posture

This package is source-level dual-seal evidence, not production finality
evidence. V1 readiness must remain blocked until live CKB maturity evidence and
public BTC closure-verification evidence exist.
