# NovaSeal v0 Field Guard Coverage

**Date**: 2026-05-30
**Scope**: `src/nova_state_type.cell` only.
**Status**: Resource-transition fields are generated-visible through `input_output_relation_checks`; conservative fail-closed equality guards now also emit named `guard-equality:*` ProofPlan records.

This file separates two kinds of evidence:

- **Source guard evidence**: a `require` or output assignment is present in the audited action source.
- **Generated ProofPlan evidence**: `cellc audit-bundle --json` emits a named obligation, coverage record, relation check, or on-chain checked record for the same field-level rule.

## Current Guard Matrix

| Criterion | Field | Source guard evidence | Generated ProofPlan visibility | Current classification |
| --- | --- | --- | --- | --- |
| 3 | `state_hash` | `require intent.old_state_hash == old_cell.state_hash`; `let actual_state_hash_commitment = hash_blake2b(intent.new_state_hash)`; `require actual_state_hash_commitment == state_hash_commitment`; output `state_hash: intent.new_state_hash` | `guard-equality:intent.old_state_hash==old_cell.state_hash`; `guard-equality:hash_blake2b(intent.new_state_hash)==state_hash_commitment`; `resource-field:state_hash=guarded` under covered `resource-conservation:NovaSealCellV0` | `generated_visible` |
| 4 | `nonce` | `require intent.nonce == old_cell.nonce + 1`; output `nonce: intent.nonce` | `guard-equality:intent.nonce==old_cell.nonce+1`; `resource-field:nonce=guarded` under covered `resource-conservation:NovaSealCellV0` | `generated_visible` |
| 5 | `expiry` | `require now <= intent.expiry`; output `expiry: intent.expiry` | `resource-field:expiry=guarded` under covered `resource-conservation:NovaSealCellV0`; timepoint load is also visible | `generated_visible` |
| 7 | `policy_hash` | `require intent.policy_hash == old_cell.policy_hash`; output `policy_hash: old_cell.policy_hash` | `guard-equality:intent.policy_hash==old_cell.policy_hash`; lock-side `guard-equality:cell.policy_hash==intent.policy_hash`; `resource-field:policy_hash=preserved` under covered `resource-conservation:NovaSealCellV0` | `generated_visible` |
| 8 | `receipt_hash` | `require receipt_hash == intent.receipt_hash` | `guard-equality:intent.receipt_hash==receipt_hash` | `generated_visible` |

The derived extractor checks these snippets exactly:

```bash
cellc audit-bundle --target-profile ckb --json
python3 scripts/novaseal_audit_surface.py --pretty
```

The current expected output in `target/novaseal-audit-surface.json` is:

- `source_guard_present: true` for all five fields.
- `generated_named_obligation: true` for `state_hash`, `nonce`, `expiry`, `policy_hash`, and `receipt_hash`.
- `classification: "generated_visible"` for all five tracked field guards.

The resource-conservation record reports:

- `unchecked`: none
- `preserved`: `version`, `btc_authority_hash`, `policy_hash`, `receipt_root`
- `guarded`: `state_hash`, `nonce`, `expiry`
- `allowed fresh`: none

This means strict mode no longer fails on a PP0150 resource-conservation gap, and the package now passes strict 0.16 ProofPlan soundness.

## What This Means

The action source now has generated audit coverage for the transition relation itself. That is materially stronger than merely having `require` statements in source.

The generated audit surface is still not the full protocol target. It does not prove the BTC signature decision, does not materialise a receipt output cell, and does not execute the six fixtures in CKB VM.

## What Not To Do

Do not claim criterion 6 is production-covered: the authority lock and staged verifier have not yet executed together in CKB VM.

Do not treat the receipt equality guard as a generated receipt-output proof. It proves only the fail-closed equality check; no `ProofReceiptV0` output cell is materialised.

Do not add SPV, OP_RETURN, Fiber channel semantics, receipt output cells, or new protocol scope to improve the table.

## Future Closure Paths

The next conservative slices should be:

1. Build a minimal CKB-VM transaction harness for the current action and authority-lock shape, still without pretending the BTC verifier is production-ready.
2. Execute the staged RISC-V BIP340 verifier through parent-lock spawn for valid and invalid IPC envelopes without weakening strict-mode audit honesty.
3. Decide whether `ProofReceiptV0` should remain hash-only for v0 or become a materialised output cell in a later slice.

Any closure must preserve the distinction between generated audit evidence, source evidence, model-level fixture evidence, and external TCB evidence.
