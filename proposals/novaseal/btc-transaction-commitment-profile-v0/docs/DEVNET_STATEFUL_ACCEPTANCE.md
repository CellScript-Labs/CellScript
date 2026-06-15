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
4. Attach public BTC SPV evidence before any production claim. The report must
   satisfy the current external evidence handoff bundle, echo the live CKB and
   service-builder binding hashes, carry the CKB-side BTC commitment hash, and
   include raw BTC transaction, block-header, Merkle proof, confirmation, and
   output-binding material that certification can recompute.

The `btc_transaction_commitment_transition` scenario is now covered in the V1
readiness matrix. Production remains blocked until handoff-bound public BTC SPV
evidence and shared external attestations are supplied.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.
