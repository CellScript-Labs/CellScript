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
4. Attach public BTC closure SPV evidence before any production claim. The
   report must satisfy the current external evidence handoff bundle, echo the
   live CKB and service-builder binding hashes, carry the CKB-side BTC
   commitment hash, and include raw closure transaction, block-header, Merkle
   proof, confirmation, and spend-input binding material that certification can
   recompute.

The local CKB finality path is no longer a V1 blocker. Public/mainnet
BTC-finality claims still require handoff-bound public BTC closure SPV evidence
and the shared external attestations.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.
