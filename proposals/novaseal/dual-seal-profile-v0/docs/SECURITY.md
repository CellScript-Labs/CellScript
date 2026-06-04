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
- The live devnet runner executes initialise -> finalise, wrong BTC owner,
  wrong CKB authority, and missing BTC closure dry-runs against the deployed
  lifecycle CellDep.

## Not Implemented

- BTC SPV proof, indexer proof, inclusion depth, finality, or spend validity.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.

## Risk Posture

This package has source-level and live CKB stateful dual-seal evidence, not
production BTC finality evidence. Production readiness remains blocked until
public BTC closure-verification evidence and external attestations exist.
