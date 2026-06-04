# NovaSeal BTC UTXO Seal Profile v0 Devnet Stateful Acceptance

Required V1 evidence is not present yet.

The acceptance target is:

1. Deploy the BIP340 runtime verifier and BTC UTXO seal profile code as live
   CellDeps.
2. Submit a valid closure transaction and prove the active seal Cell is dead and
   the terminal receipt is live.
3. Dry-run wrong-owner, mismatched UTXO commitment, zero spend txid/wtxid, stale
   nonce, and expired closure negatives and prove they do not consume state.
4. Attach public BTC spend proof evidence from an SPV verifier or trusted
   indexer policy before any production claim.

Until this evidence is generated, `btc_utxo_seal_closure` must remain missing
in the V1 readiness matrix.
