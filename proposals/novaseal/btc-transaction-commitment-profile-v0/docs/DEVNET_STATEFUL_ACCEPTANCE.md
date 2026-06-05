# NovaSeal BTC Transaction Commitment Profile v0 Devnet Stateful Acceptance

Required local V1 stateful evidence is present in:

```bash
target/novaseal-btc-transaction-commitment-devnet-stateful-live.json
```

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

The `btc_transaction_commitment_transition` scenario is now covered in the V1
readiness matrix. Production remains blocked until public BTC proof evidence
and shared external attestations are supplied.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.
