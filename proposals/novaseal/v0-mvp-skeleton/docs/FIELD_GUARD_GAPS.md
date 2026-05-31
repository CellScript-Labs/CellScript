# NovaSeal v0 Field Guard Coverage

**Date**: 2026-05-31
**Scope**: `src/nova_state_type.cell`
**Status**: source guards and harness evidence are strong; strict generated ProofPlan coverage still has gaps.

This file separates:

- **Source guard evidence**: the `.cell` action contains the `require` or output assignment.
- **Generated ProofPlan evidence**: `cellc audit-bundle --json` emits a named generated obligation.
- **Harness/live evidence**: CKB VM, resolved transaction, or live devnet execution proves the guard at runtime.

## Current Guard Matrix

| Criterion | Rule | Source evidence | Generated ProofPlan visibility | Harness/live evidence |
| --- | --- | --- | --- | --- |
| 3 | state changes only through signed intent core | `intent.core.old_state_hash`, `intent.core.new_state_hash`, and `state_hash_commitment` guards | not fully named after split-intent nesting | state-type CKB VM, combined tx harness, live devnet key-auth transition |
| 4 | nonce increments by exactly 1 | `intent.core.old_nonce == old_cell.nonce`; `intent.core.new_nonce == old_cell.nonce + 1` | not fully named after split-intent nesting | state-type CKB VM, combined tx harness, live devnet key-auth transition |
| 5 | expiry is enforced | `now <= intent.core.expiry`; output expiry from `intent.core.expiry` | timepoint load visible; standalone expiry guard not fully named | state-type CKB VM, combined tx harness |
| 7 | policy hash is preserved | `intent.core.policy_hash == old_cell.policy_hash`; output `policy_hash: old_cell.policy_hash` | lock-side policy guard visible; action-side nested guard not fully named | state-type CKB VM, combined tx harness |
| 8 | receipt commitment matches signed intent and new cell | `intent.expected_receipt_hash == materialized_receipt_hash`; output `latest_receipt_hash: materialized_receipt_hash`; receipt carries `intent_core_hash` and `signed_intent_hash` | create-output and resource-conservation are still `runtime-required` in strict ProofPlan | state-type CKB VM, combined tx harness, live devnet receipt output |

## Current Strict Gap

`cellc check --target-profile ckb --primitive-strict 0.16` currently fails with:

```text
PP0150 action:key_auth_transition:create-output:NovaSealCellV0:new_cell
PP0150 action:key_auth_transition:create-output:ProofReceiptV0:receipt
PP0150 action:key_auth_transition:resource-conservation:NovaSealCellV0
```

This is a generated-audit limitation around output/resource proof records after
the split-intent and explicit receipt commitment refactor. It does not mean the
runtime path is untested: the combined transaction harness and local devnet
stateful runner both execute the materialized transition. It does mean strict
generated ProofPlan coverage is not closed.

## What Not To Claim

- Do not claim strict 0.16 generated ProofPlan soundness for NovaSeal core.
- Do not claim generated audit coverage alone proves the BTC signature decision.
- Do not claim `latest_receipt_hash` is a historical accumulator.
- Do not replace live/devnet/harness evidence with generated-audit language.

## Closure Path

The next compiler-facing closure is to teach the generated ProofPlan/output
verifier surface to recognize the split-intent output relation:

```text
NovaSealSignedIntentV0.expected_receipt_hash
  == hash_blake2b_packed(ProofReceiptCommitmentV0)
  == new_cell.latest_receipt_hash
```

and to mark the corresponding `create-output:*` and
`resource-conservation:NovaSealCellV0` records as on-chain checked when the
lowered verifier actually enforces those fields.
