# Agreement Profile Devnet Stateful Acceptance

Status: Agreement originate -> repay and originate -> claim live RPC passed.

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
- submit one originate through RPC and repay against that exact active outpoint;
- submit a second originate through RPC, advance devnet epoch past expiry, and
  claim against that exact active outpoint;
- dry-run wrong lender signature, non-CKB asset kind, wrong borrower signature,
  payout capacity short, payout lock args mismatch, wrong payout amount, early
  claim, and wrong lender claim signature rejects;
- verify the valid terminal paths consume active cells and leave the closed
  agreement, payout, and receipt cells live.

The shared gate currently reports `local_devnet_passed_external_endpoint_required`
when NovaSeal core has live bootstrap -> key-auth transition evidence and this
Agreement Profile has live originate -> repay plus originate -> claim evidence,
but public BTC/Fiber endpoint evidence is still outstanding. It reports
`passed` only after that external endpoint evidence is complete too.

See [DEVNET_FULL_ACCEPTANCE_RUNBOOK.md](../../DEVNET_FULL_ACCEPTANCE_RUNBOOK.md) for prerequisites, freshness rules, and the full command sequence.
