# Agreement Profile CKB VM Harness

This Rust harness is local evidence tooling only. It is not part of the deployed
Agreement Profile contract surface and is not a verifier CellDep.

The default binary executes the three compiled Agreement Profile action ELFs in
`ckb-vm`:

- `originate_agreement`
- `repay_before_expiry`
- `claim_after_expiry`

The harness supplies deterministic `LOAD_WITNESS`, `LOAD_CELL_DATA`, and
`LOAD_HEADER_BY_FIELD` syscall responses. It covers action/type-script guards
for valid terminal paths, time rejects, party rejects, nonce mismatch,
receipt-root mismatch, and preserved-field mutation.

`novaseal_agreement_tx_harness` constructs deterministic in-memory resolved CKB
transactions and runs both `ckb-script` and the CKB non-contextual/contextual
transaction verification stack. It uses the action ELF as the type/action script
CellDep and a local `always_success_lock.cell` CellDep only so the transaction
can reach the Agreement Profile script group in the harness.

```bash
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_ckb_vm_harness -- --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_tx_harness -- --pretty
```

The transaction harness covers resolved originate, repay, claim, time rejects,
party rejects, nonce mismatch, receipt-root mismatch, preserved-field mutation,
and occupied-capacity rejection. It also fails if the fixtures outside tx-harness
coverage differ from the known blocker set: canonical terms hash, canonical
receipt hash, and payout-cell settlement binding. It still does not prove
live-chain deployment, native CKB payout-cell binding, canonical terms/receipt
hash preimages, or borrower/lender cryptographic authority locks.
