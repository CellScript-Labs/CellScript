# NovaSeal Dual Seal Profile v0 Devnet Stateful Acceptance

Required V1 evidence is not present yet.

The acceptance target is:

1. Deploy the BIP340 runtime verifier and dual-seal profile code as live
   CellDeps.
2. Submit a valid finalisation transaction after the CKB maturity timepoint and
   prove the active dual-seal Cell is dead and the terminal receipt is live.
3. Dry-run early-maturity, wrong BTC owner, wrong CKB authority, missing BTC
   closure, stale nonce, and expired finalisation negatives and prove they do
   not consume state.
4. Attach public BTC closure proof evidence from an SPV verifier or trusted
   indexer policy before any production claim.

Until this evidence is generated, dual-seal finality must remain a V1 blocker.
