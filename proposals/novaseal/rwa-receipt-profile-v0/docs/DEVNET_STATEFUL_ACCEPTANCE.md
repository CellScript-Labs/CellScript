# NovaSeal RWA Receipt Profile v0 Devnet Stateful Acceptance

Required V1 evidence is not present yet.

The acceptance target is:

1. Deploy the BIP340 runtime verifier and RWA receipt profile code as live
   CellDeps.
2. Submit a valid materialisation transaction and prove the receipt Cell and
   immutable event are live.
3. Submit a valid claim transaction and prove the old materialised Cell is dead
   and the claimed Cell plus claim event are live.
4. Submit a valid settlement transaction and prove the claimed Cell is dead and
   the terminal event is live.
5. Dry-run wrong-holder, wrong-issuer, amount-mutation, and stale-status
   transactions and prove they do not consume live state.

Until this evidence is generated, `rwa_receipt_lifecycle` must remain missing
in the V1 readiness matrix.
