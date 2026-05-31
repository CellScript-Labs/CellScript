# NovaSeal Devnet Stateful Acceptance

Status: core lifecycle and Agreement originate -> repay live RPC passed.

NovaSeal now has local CKB-VM, `ckb-script`, and `ckb-verification` evidence, but
that is not the same as a live devnet lifecycle. The release gate for this is:

```sh
scripts/novaseal_devnet_stateful_acceptance.sh --pretty
```

The command writes `target/novaseal-devnet-stateful-acceptance.json` and exits
non-zero until live devnet RPC execution has actually passed. Use
`--report-only` when a developer only wants to inspect the readiness report.

Resolved lifecycle blockers:

- Core v0 now has `src/nova_state_lifecycle_type.cell:novaseal_lifecycle`,
  with `OP_BOOTSTRAP` for output-only creation and `OP_KEY_AUTH_TRANSITION` for
  stable identity-preserving transitions.
- Agreement Profile now has
  `src/nova_agreement_lifecycle_type.cell:nova_agreement_lifecycle`, with
  `PATH_ORIGINATE`, `PATH_REPAY_BEFORE_EXPIRY`, and
  `PATH_CLAIM_AFTER_EXPIRY` under one type-script identity.

The required end state is a real CKB RPC lifecycle:

1. deploy the runtime verifier and protocol artifacts as live CellDeps;
2. submit valid transactions through devnet RPC;
3. commit each valid step;
4. verify consumed inputs become dead;
5. verify new state, receipt, and payout outputs are live;
6. keep negative cases as dry-run/send-test rejections that do not mutate state.

Do not classify the current combined transaction harness as devnet stateful
acceptance. It is valuable local verifier evidence, but it is still an
in-memory `ResolvedTransaction` path.

Current live evidence:

- `scripts/novaseal_devnet_stateful_live.py --pretty --ckb-repo ../ckb --ckb-bin ../ckb/target/debug/ckb`
  passed for core NovaSeal.
- The runner deployed the BIP340 verifier and `novaseal_lifecycle` type script
  as live CellDeps, submitted bootstrap and key-auth transition transactions,
  verified the bootstrap state output was no longer live, verified the new state
  and receipt outputs were live, and confirmed a wrong-signature transition was
  rejected by dry-run without consuming the live state.
- `scripts/novaseal_agreement_devnet_stateful_live.py --pretty --ckb-repo ../ckb --ckb-bin ../ckb/target/debug/ckb`
  passed for the Agreement Profile.
- The Agreement runner deployed the BIP340 verifier and
  `nova_agreement_lifecycle` type script as live CellDeps, submitted originate,
  confirmed active/principal-payout/receipt outputs were live, dry-ran a wrong
  borrower signature repay and observed rejection, then submitted valid repay
  against that exact active outpoint and confirmed the active output was dead
  plus closed/payout/receipt outputs were live.
- The aggregate gate status is `passed`.
