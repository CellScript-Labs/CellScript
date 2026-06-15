# NovaSeal v0 Field Guard Coverage

**Date**: 2026-05-31
**Scope**: `src/nova_state_type.cell`
**Status**: source guards, generated ProofPlan visibility, strict mode, harness
evidence, and live local devnet evidence are aligned for the current v0 core
transition.

This file separates:

- **Source guard evidence**: the `.cell` action contains the `require` or output assignment.
- **Generated ProofPlan evidence**: `cellc audit-bundle --json` emits a named generated obligation.
- **Harness/live evidence**: CKB VM, resolved transaction, or live devnet execution proves the guard at runtime.

## Current Guard Matrix

| Criterion | Rule | Source evidence | Generated ProofPlan visibility | Harness/live evidence |
| --- | --- | --- | --- | --- |
| 3 | state changes only through signed intent core | `intent.core.old_state_hash`, `intent.core.new_state_hash`, and `state_hash_commitment` guards | generated-visible; `state_hash` is guarded in resource conservation | state-type CKB VM, combined tx harness, live devnet key-auth transition |
| 4 | nonce increments by exactly 1 | `intent.core.old_nonce == old_cell.nonce`; `intent.core.new_nonce == old_cell.nonce + 1` | generated-visible; `nonce` is guarded in resource conservation | state-type CKB VM, combined tx harness, live devnet key-auth transition |
| 5 | expiry is enforced | `now <= intent.core.expiry`; output expiry from `intent.core.expiry` | generated-visible; `expiry` is guarded in resource conservation | state-type CKB VM, combined tx harness |
| 7 | policy hash is preserved | `intent.core.policy_hash == old_cell.policy_hash`; output `policy_hash: old_cell.policy_hash` | generated-visible; `policy_hash` is preserved in resource conservation | state-type CKB VM, combined tx harness |
| 8 | receipt commitment matches signed intent and new cell | `intent.expected_receipt_hash == materialized_receipt_hash`; output `latest_receipt_hash: materialized_receipt_hash`; receipt carries `intent_core_hash` and `signed_intent_hash` | generated-visible; `latest_receipt_hash` is guarded in resource conservation | state-type CKB VM, combined tx harness, live devnet receipt output |

## Current Strict Result

`cellc check --target-profile ckb --primitive-strict 0.16` now passes for the
NovaSeal core package.

After:

```text
cellc audit-bundle --target-profile ckb --json
python3 scripts/novaseal_audit_surface.py --pretty
```

the current audit surface reports:

```text
proof_plan_records=55
runtime_gaps=0
strict_prediction_errors=0
```

The generated resource-conservation record classifies:

```text
version=preserved
btc_authority_hash=preserved
policy_hash=preserved
state_hash=guarded
latest_receipt_hash=guarded
nonce=guarded
expiry=guarded
```

## What Not To Claim

- Do not claim generated audit coverage alone proves the BTC signature decision.
- Do not claim `latest_receipt_hash` is a historical accumulator.
- Do not replace live/devnet/harness evidence with generated-audit language.

## Remaining Boundaries

- The BTC signature decision remains delegated to the runtime verifier TCB.
- Public/shared CellDep attestation and external verifier review remain
  production blockers.
- Fixed-width wallet signing vectors now exist; dynamic Molecule table/vector
  profiles remain future extensions.

The old generated-audit gap in this file is closed; production readiness is
still a deployment-attestation and verifier-TCB question.
