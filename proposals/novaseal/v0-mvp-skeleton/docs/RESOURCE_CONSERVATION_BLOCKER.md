# NovaSeal v0 Resource Conservation Status

**Date**: 2026-05-31
**Scope**: `resource-conservation:NovaSealCellV0` and related output records in `key_auth_transition`.
**Status**: closed at strict generated ProofPlan level for the current v0 core
transition; still not a production-readiness claim.

## Current Generated Result

`cellc check --target-profile ckb --primitive-strict 0.16` now passes.

After:

```text
cellc audit-bundle --target-profile ckb --json
python3 scripts/novaseal_audit_surface.py --pretty
```

the derived audit surface reports:

```text
proof_plan_records=55
runtime_gaps=0
strict_prediction_errors=0
```

The generated audit bundle marks:

```text
create-output:NovaSealCellV0:new_cell = checked-runtime
create-output:ProofReceiptV0:receipt = checked-runtime
resource-conservation:NovaSealCellV0 = checked-runtime
```

## Runtime Evidence That Exists

The runtime path is also covered by local and live evidence:

- the state-type CKB VM harness executes all eight fixtures at action/type scope;
- the combined lock + type transaction harness executes all eight fixtures
  through `ckb-script` and the local CKB contextual verifier stack;
- the live local devnet runner commits bootstrap -> key-auth transition by RPC;
- the live runner verifies the old state is dead and the new state + receipt
  outputs are live;
- the live runner dry-runs wrong-signature rejection without consuming the state.

This evidence now agrees with strict generated ProofPlan closure.

## What Changed From The Older Note

The split-intent and explicit `ProofReceiptCommitmentV0` refactor temporarily
left output/resource records as strict `PP0150` gaps. The compiler audit surface
now recognizes fixed packed struct literals, nested field aliases, fixed nested
aggregate fields, and the fail-closed equality:

```text
intent.expected_receipt_hash
  == hash_blake2b_packed(ProofReceiptCommitmentV0)
  == new_cell.latest_receipt_hash
```

## What Not To Claim

- Do not claim production readiness from harness success alone.
- Do not claim a historical receipt accumulator; v0 only stores
  `latest_receipt_hash`.
- Do not claim the BTC runtime verifier TCB is externally audited or
  publicly/shared pinned yet.
