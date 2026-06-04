# NovaSeal BTC UTXO Seal Profile v0 Security

## Implemented Guards

- The active seal binds a BTC txid, vout index, satoshi amount, and scriptPubKey
  hash.
- The closure binds a declared spend txid, spend wtxid, input index, transition
  commitment hash, and payout commitment hash.
- The closure consumes the active seal Cell and creates only a terminal receipt.
- The seal owner authority signs the typed closure intent with BIP340.
- The transition increments nonce exactly once and recomputes the canonical
  NovaSeal envelope before acceptance.

## Not Implemented

- BTC SPV proof, indexer proof, inclusion depth, finality, or spend validity.
- Live devnet UTXO seal closure evidence.
- Wallet signing vectors for this profile.
- Public/shared CellDep attestation.
- External BIP340 runtime verifier TCB review.

## Risk Posture

This package is source-level single-use-seal evidence, not a proof that a BTC
UTXO was actually spent. V1 readiness must remain blocked until a live CKB
closure and public BTC spend-verification evidence exist.
