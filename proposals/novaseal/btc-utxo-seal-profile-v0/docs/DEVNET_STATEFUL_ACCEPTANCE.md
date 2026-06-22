# NovaSeal BTC UTXO Seal Profile v0 Devnet Stateful Acceptance

Required local V1 stateful evidence is present in:

```bash
target/novaseal-btc-utxo-seal-devnet-stateful-live.json
```

The acceptance target is:

1. Deploy the BIP340 runtime verifier and BTC UTXO seal profile code as live
   CellDeps.
2. Submit a valid closure transaction and prove the active seal Cell is dead and
   the terminal receipt is live.
3. Dry-run wrong-owner, mismatched UTXO commitment, zero spend txid/wtxid, stale
   nonce, and expired closure negatives and prove they do not consume state.
4. Attach public BTC spend SPV evidence before any public/mainnet BTC spend claim. The report
   must satisfy the current external evidence handoff bundle, echo the live CKB
   and service-builder binding hashes, carry the CKB-side sealed UTXO
   commitment hash, and include raw spend/sealed transaction, block-header,
   Merkle proof, confirmation, and sealed-output binding material that
   certification can recompute.

The `btc_utxo_seal_closure` scenario is now covered in the V1 readiness matrix.
Public/mainnet BTC spend claims still require handoff-bound public BTC spend
SPV evidence and shared external attestations.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.
