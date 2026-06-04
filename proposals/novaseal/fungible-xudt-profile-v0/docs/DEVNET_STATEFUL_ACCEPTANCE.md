# NovaSeal Fungible xUDT Profile v0 Devnet Stateful Acceptance

Required local V1 stateful evidence is present in:

```bash
target/novaseal-fungible-xudt-devnet-stateful-live.json
```

The acceptance target is:

1. Deploy the BIP340 runtime verifier and
   `nova_fungible_xudt_lifecycle` type code as live CellDeps.
2. Submit a valid issue transaction and prove the issued xUDT profile Cell and
   receipt are live.
3. Submit a valid transfer transaction and prove the old holder Cell is dead
   and the new holder Cell and receipt are live.
4. Submit a valid settlement transaction and prove the transferred Cell is dead
   and the terminal receipt is live.
5. Dry-run invalid wrong-signer and amount-mutation transactions and prove they
   do not consume live state.

The `fungible_xudt_value_flow` scenario is now covered in the V1 readiness
matrix. Production remains blocked only by the shared external attestation
requirements.
