# NovaSeal Fungible xUDT Profile v0 Security

## Implemented Guards

- Issue is issuer-authorised by BIP340 pubkey equality and signature checks.
- Transfer and settlement are current-holder-authorised by BIP340 pubkey
  equality and signature checks.
- Transfer preserves `asset_id`, `xudt_type_hash`, issuer, amount, active
  status, expiry, and increments nonce exactly once.
- Settlement is terminal: the source object is active, `new_status` is
  settled, and `new_amount` is zero.
- Every signed intent binds the shared `NovaSealCanonicalEnvelopeV0` hash and a
  materialised receipt hash.

## Not Implemented

- Live devnet issue -> transfer -> settle acceptance.
- Builder-backed xUDT type-script compatibility evidence.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.
- Partial-balance splits, joins, or multi-output ledger accounting.

## Risk Posture

This package is reviewable profile evidence, not production evidence. V1
readiness must continue to fail until the package has wallet vectors,
builder-backed valid and invalid xUDT transactions, live devnet lifecycle
evidence, and the shared external attestations.
