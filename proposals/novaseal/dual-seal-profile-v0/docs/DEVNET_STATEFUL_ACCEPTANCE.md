# NovaSeal Dual Seal Profile v0 Devnet Stateful Acceptance

Required local V1 CKB stateful evidence is present in:

```bash
target/novaseal-dual-seal-devnet-stateful-live.json
```

The acceptance target is:

1. Deploy the BIP340 runtime verifier and dual-seal profile code as live
   CellDeps.
2. Submit a valid finalisation transaction after the CKB maturity timepoint and
   prove the active dual-seal Cell is dead and the terminal receipt is live.
3. Dry-run wrong BTC owner, wrong CKB authority, and missing BTC closure
   negatives and prove they do not consume state.
4. Attach public BTC closure proof evidence from an SPV verifier or trusted
   indexer policy before any production claim.

The local CKB finality path is no longer a V1 blocker. Production remains
blocked until public BTC closure proof evidence and the shared external
attestations are supplied.
