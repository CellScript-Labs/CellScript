# NovaSeal RWA Receipt Profile v0 Security

## Implemented Guards

- Materialisation is issuer-authorised by BIP340 pubkey equality and signature
  checks.
- Claim is holder-authorised by BIP340 pubkey equality and signature checks.
- Settlement requires both issuer and holder signatures.
- Lifecycle transitions preserve `registry_hash`, `asset_commitment_hash`,
  `document_hash`, issuer, holder, amount, expiry, and increment nonce exactly
  once.
- Every lifecycle edge emits an immutable `NovaRwaReceiptEventV0` audit event.
- All value fields are `u64`; no floating point or decimal rounding exists in
  this profile.

## Not Implemented

- Live devnet materialise -> claim -> settle acceptance.
- Builder-backed valid and invalid RWA receipt transaction evidence.
- Legal registry, custodian, oracle, or title-system verification.
- Wallet signing vectors.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.

## Risk Posture

This package is source-level receipt-lifecycle evidence, not production RWA
evidence. V1 readiness must remain blocked until stateful devnet evidence,
wallet vectors, external attestations, and domain-specific legal/registry review
exist.
