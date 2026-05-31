# Agreement Profile Devnet Stateful Acceptance

Status: Agreement originate -> repay live RPC passed.

The resolved transaction harness proves each Agreement action shape locally, but
it is not a live devnet lifecycle. The shared NovaSeal gate is:

```sh
scripts/novaseal_devnet_stateful_acceptance.sh --pretty
```

The original Agreement blocker was script identity, not the economics model:
`originate_agreement`, `repay_before_expiry`, and `claim_after_expiry` were
compiled as separate entry-action ELFs. That blocker is now resolved by
`src/nova_agreement_lifecycle_type.cell:nova_agreement_lifecycle`, a single
stable type-script entry that routes `PATH_ORIGINATE`,
`PATH_REPAY_BEFORE_EXPIRY`, and `PATH_CLAIM_AFTER_EXPIRY`.

The Agreement live runner now:

- deploy live CellDeps;
- submit originate through RPC;
- read the live agreement cell;
- dry-run a wrong-borrower-signature repay and confirm rejection;
- submit repay against that exact outpoint;
- verify the resulting receipt and payout cells through RPC.

The shared gate currently reports `passed`: NovaSeal core has live bootstrap ->
key-auth transition evidence, and this Agreement Profile has live originate ->
repay evidence.
