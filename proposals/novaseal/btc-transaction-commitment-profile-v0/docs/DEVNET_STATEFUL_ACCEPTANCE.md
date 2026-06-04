# NovaSeal BTC Transaction Commitment Profile v0 Devnet Stateful Acceptance

Required V1 evidence is not present yet.

The acceptance target is:

1. Deploy the BIP340 runtime verifier and BTC transaction commitment profile
   code as live CellDeps.
2. Submit a valid CKB state transition bound to a public BTC txid/wtxid/output
   tuple and prove the old Cell is dead plus the committed successor Cell and
   receipt are live.
3. Dry-run wrong-committer, zero txid/wtxid, transition-hash mismatch, stale
   nonce, and expired transition negatives and prove they do not consume state.
4. Attach public BTC proof evidence from an SPV verifier or trusted indexer
   policy before any production claim.

Until this evidence is generated, `btc_transaction_commitment_transition` must
remain missing in the V1 readiness matrix.
