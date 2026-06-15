# CellScript 0.13.2 Acceptance Report

CellScript 0.13.2 is ready as the stable 0.13 release line.

This release is not just a compiler build. The release gate ran compiler checks,
syntax-combination audits, builder-backed action transactions, local CKB devnet
transactions, lock rejection matrices, and stateful business-flow scenarios.

## Acceptance Summary

- Compiler checks passed.
- Syntax-combination audit passed.
- Builder-backed actions passed.
- Builder-backed lock valid/invalid spend matrices passed.
- Local CKB devnet transactions were submitted and committed.
- Live outputs were inspected after commits.
- Consumed inputs were checked as no longer live.
- Malformed transaction rejection passed.
- Stateful scenario flows passed.
- Cycles, consensus transaction size, and occupied capacity were measured.
- Under-capacity outputs were rejected by the release gate.

The full release gate is:

```bash
./scripts/cellscript_gate.sh release
```

The old `./scripts/cellscript_ckb_release_gate.sh full` command remains
supported as a compatibility wrapper.

The current release evidence includes:

- 7 production bundled examples.
- 44/44 production acceptance actions covered.
- 17 builder-backed lock valid/invalid spend cases.
- 27 stateful local CKB scenarios.
- 47 committed stateful steps.
- 7 end-to-end business-flow scenarios.
- 20 stateful action-branch scenarios.

## Stateful Business Flows

### Token

Flow:

```text
mint -> transfer -> mint -> merge -> burn
```

Acceptance evidence:

- Mint authority was consumed and updated.
- Minted token output became the live input for transfer.
- Transferred token output became the live input for merge.
- Merged token output became the live input for burn.
- Invalid token amount transitions, including overspend-style malformed
  outputs, were rejected by the builder-backed action matrix.

### Timelock

Flow:

```text
create lock -> lock asset -> request release -> execute release
```

Acceptance evidence:

- A live timelock cell was created.
- A live asset was locked against the timelock dependency.
- A release request was created from the live lock.
- Release execution consumed the live lock, locked asset, and request cells.
- Early/not-expired and malformed release paths were rejected by the lock and
  action rejection matrices.

### NFT

Flow:

```text
mint -> list -> buy -> ownership transfer
```

Acceptance evidence:

- Collection state was consumed and updated during mint.
- The minted NFT became a live dependency for listing.
- The listing and NFT were consumed by purchase.
- Buyer ownership was reflected in the output NFT.
- Royalty and seller payment outputs were checked.
- Invalid payment and malformed sale outputs were rejected by the
  builder-backed action matrix.

### AMM

Flow:

```text
create pool -> add liquidity -> swap -> remove liquidity
```

Acceptance evidence:

- Token pair inputs were consumed to seed a pool.
- The live pool became the input for add-liquidity.
- The updated live pool became the input for swap.
- The swapped live pool plus LP receipt became inputs for remove-liquidity.
- Pool reserves, LP receipt, swap output, and withdrawal outputs were checked.
- Invariant and malformed-output checks were enforced by the action matrix.

### Multisig

Flow:

```text
create wallet -> propose -> approve threshold -> execute
```

Acceptance evidence:

- A live wallet was created.
- The live wallet produced a proposal.
- The proposal was updated by the first signature.
- The proposal was updated again after threshold approval.
- The threshold-satisfied proposal was executed into an execution record.
- Insufficient signatures and invalid signer paths were rejected by the
  builder-backed action and lock matrices.

### Vesting

Flow:

```text
create config -> grant -> revoke
```

Additional covered branch:

```text
claim vested
```

Acceptance evidence:

- A live vesting config was created.
- Grant creation used the live config as a dependency.
- The live grant was revoked into beneficiary/admin token outputs.
- Claiming vested funds is covered by a stateful branch transaction.
- Invalid grant, invalid claim amount, invalid revoke amount, and unauthorized
  revoke paths were rejected by the builder-backed action and lock matrices.

### Launch

Flow:

```text
launch token -> mint again from launched authority
```

Additional covered branch:

```text
simple launch
```

Acceptance evidence:

- Launch consumed paired-token input and created mint authority, pool, LP, and
  recipient token outputs.
- The launched mint authority became the live input for a later mint.
- `bootstrap_token` is covered by a stateful branch transaction.
- Malformed distribution/accounting outputs were rejected by the action matrix.

## What This Proves

The release evidence proves that the current bundled production CellScript
examples compile under the CKB target profile and that the generated artifacts
can participate in concrete local CKB devnet transactions.

It also proves that the release gate observes more than "compile succeeds":
valid transactions dry-run, commit, update live cells, consume inputs, preserve
expected outputs, reject malformed variants, and expose cycles, transaction
size, and occupied-capacity measurements.

## Known Limitations

- Not externally audited.
- Not mainnet-value certified.
- Not exhaustive state-space verification.
- Stateful runner covers representative production workflows and every
  production acceptance action, but not every adversarial branch.
- Local CKB devnet evidence is stronger than compile-only evidence, but it is
  not a substitute for independent protocol-specific review.
- Spawn/IPC, structured `WitnessArgs`, ScriptGroup conformance fixtures, and
  broader Source-view semantics are deferred to v0.14.
- First-class signer values and hidden sighash defaults are intentionally not
  part of 0.13.2.
- Full generic maps, Cell-backed generic collections, and declarative capacity
  policy remain out of scope for this release line.

## Bottom Line

CellScript 0.13.2 closes the 0.13 line with strict compiler, tooling, CKB
acceptance, and stateful business-flow evidence. The release is mature enough to
publish as the stable 0.13 baseline while keeping the remaining CKB semantic
work explicit for v0.14.
