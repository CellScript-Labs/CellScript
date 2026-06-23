# NovaSeal RWA Receipt Profile v0 Devnet Stateful Acceptance

Required local V1 stateful evidence is present in:

```bash
target/novaseal-rwa-receipt-devnet-stateful-live.json
```

The acceptance target is:

1. Deploy the BIP340 runtime verifier and `nova_rwa_receipt_lifecycle` type
   code as live CellDeps.
2. Submit a valid materialisation transaction and prove the receipt Cell and
   immutable event are live.
3. Submit a valid claim transaction and prove the old materialised Cell is dead
   and the claimed Cell plus claim event are live.
4. Submit a valid settlement transaction and prove the claimed Cell is dead and
   the terminal event is live.
5. Dry-run wrong-holder, wrong-issuer, amount-mutation, and stale-status
   transactions and prove they do not consume live state.

The `rwa_receipt_lifecycle` scenario is now covered in the V1 readiness matrix.
Public/mainnet RWA release claims still require the shared external attestation
requirements and the external legal/registry review boundary.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.
