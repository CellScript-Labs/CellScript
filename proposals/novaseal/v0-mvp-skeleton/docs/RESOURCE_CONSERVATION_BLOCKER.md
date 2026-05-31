# NovaSeal v0 Resource Conservation Status

**Date**: 2026-05-31
**Scope**: `resource-conservation:NovaSealCellV0` and related output records in `key_auth_transition`.
**Status**: open at strict generated ProofPlan level; covered by source, CKB VM, combined transaction, and live local devnet evidence.

## Current Generated Gap

`cellc check --target-profile ckb --primitive-strict 0.16` currently fails with:

```text
PP0150 action:key_auth_transition:create-output:NovaSealCellV0:new_cell
PP0150 action:key_auth_transition:create-output:ProofReceiptV0:receipt
PP0150 action:key_auth_transition:resource-conservation:NovaSealCellV0
```

The generated audit bundle marks these records as `runtime-required`, not
strict-clean on-chain-checked generated obligations.

## Runtime Evidence That Exists

The runtime path is not untested:

- the state-type CKB VM harness executes all eight fixtures at action/type scope;
- the combined lock + type transaction harness executes all eight fixtures
  through `ckb-script` and the local CKB contextual verifier stack;
- the live local devnet runner commits bootstrap -> key-auth transition by RPC;
- the live runner verifies the old state is dead and the new state + receipt
  outputs are live;
- the live runner dry-runs wrong-signature rejection without consuming the state.

This evidence is stronger than source-only review, but it is not the same thing
as strict generated ProofPlan closure.

## What Changed From The Older Note

Earlier documentation said the resource-conservation blocker was closed. That
was no longer accurate after the split-intent and explicit
`ProofReceiptCommitmentV0` refactor. The current honest status is:

```text
source guards: present
state/type harness: passed
combined tx harness: passed
live local devnet: passed
strict generated ProofPlan: still open
```

## Closure Path

The compiler/audit surface needs to recognize the lowered output relation:

```text
new_cell.latest_receipt_hash
  == hash_blake2b_packed(ProofReceiptCommitmentV0)
  == intent.expected_receipt_hash
```

and then mark the matching `create-output:*` and
`resource-conservation:NovaSealCellV0` records as generated checked obligations
when the verifier actually enforces those fields.

## What Not To Claim

- Do not claim strict 0.16 ProofPlan soundness for NovaSeal core.
- Do not claim production readiness from harness success alone.
- Do not claim a historical receipt accumulator; v0 only stores
  `latest_receipt_hash`.
