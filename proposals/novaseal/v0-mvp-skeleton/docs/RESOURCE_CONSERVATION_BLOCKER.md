# NovaSeal v0 Resource Conservation Status

**Date**: 2026-05-30
**Scope**: `resource-conservation:NovaSealCellV0` in the current `key_auth_transition` action.
**Status**: Closed for the current guarded transition shape; strict 0.16 ProofPlan soundness now passes.

## Current Generated Record

`cellc audit-bundle --target-profile ckb --json` now emits a covered conservation record:

```text
feature: resource-conservation:NovaSealCellV0
status: checked-runtime
codegen_coverage_status: covered
on_chain_checked: true
detail: Compiler-emitted runtime verifier checks one consumed 'NovaSealCellV0' Input is advanced into one created Output by a guarded resource transition; resource-conservation=checked-runtime; preserved fields: version, btc_authority_hash, policy_hash, receipt_root; guarded fields: state_hash, nonce, expiry; allowed fresh fields: -; unchecked fields: -
```

The generated ProofPlan record exposes the same facts as machine-readable relation checks:

```text
resource-conservation
resource-field:version=preserved
resource-field:btc_authority_hash=preserved
resource-field:policy_hash=preserved
resource-field:receipt_root=preserved
resource-field:state_hash=guarded
resource-field:nonce=guarded
resource-field:expiry=guarded
resource-conservation:NovaSealCellV0=checked-runtime
```

The derived extractor records zero `runtime_gaps` in `target/novaseal-audit-surface.json`.

## What Changed

The action now includes a generic hash-commitment guard for the changed state field:

```text
let actual_state_hash_commitment = hash_blake2b(intent.new_state_hash)
require actual_state_hash_commitment == state_hash_commitment
```

The compiler recogniser remains protocol-agnostic. It does not know about NovaSeal fields; it sees a one-input/one-output resource transition where every output field is preserved, explicitly guarded, or explicitly allowed fresh. Because `state_hash`, `nonce`, and `expiry` are now guarded, no output field remains unchecked.

The compiler backend was also tightened so CKB metadata recognises verifier-coverable fixed-byte comparisons involving `hash_blake2b(...)` outputs. Without that, this source-level guard would merely swap the old PP0150 blocker for a `fixed-byte-comparison` fail-closed path.

## Strict 0.16 Impact

`cellc check --target-profile ckb --primitive-strict 0.16` now passes.

- No `PP0150` remains for `resource-conservation:NovaSealCellV0`.
- No `PP0103` remains for `ckb-runtime` context records. Strict PP0103 now applies only to true `checked-runtime` records whose `on_chain_checked` flag is missing.

This is useful progress: the NovaSeal protocol-level conservation relation is generated-audit-covered, and the strict soundness gate no longer reports metadata consistency errors for this package.

## What This Does Not Prove

This does not prove on-chain BTC signature correctness. The `btc_authority` lock now has generated spawn/IPC shell wiring and the delegated RISC-V shell has local plus child-verifier CKB VM BIP340 evidence, but no parent/child CKB VM transaction result is generated in the ProofPlan.

Receipt output materialisation is now covered separately by `create-output:ProofReceiptV0:receipt`; resource conservation still only proves the `NovaSealCellV0` linear transition.

This does not provide live-chain NovaSeal transaction submission. Harness-level CKB VM, cycle, transaction-size, and occupied-capacity evidence is tracked separately.

## What Not To Do

Do not reclassify the whole MVP as production-ready because conservation is now covered.

Do not treat strict ProofPlan soundness as production readiness.

Do not treat the model-level fixture harness as CKB VM execution evidence.

Do not add SPV, OP_RETURN, Fiber channel logic, or receipt output cells as part of this closure; those remain outside the v0 slice.
