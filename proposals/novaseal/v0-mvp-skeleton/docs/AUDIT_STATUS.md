# NovaSeal v0 MVP Skeleton — Audit Status

**Date of this snapshot**: 2026-05-31
**Package**: `proposals/novaseal/v0-mvp-skeleton`
**Status**: production-prep evidence exists, but production readiness is still blocked.

This document is the current evidence ledger for NovaSeal core. It intentionally
separates generated audit evidence, local verifier harness evidence, live local
devnet stateful evidence, and remaining TCB/deployment gaps.

## Current Passes

Package and script checks:

- `cellc check --target-profile ckb` passes.
- `cellc src/nova_state_type.cell --target-profile ckb` passes.
- `cellc src/nova_state_lifecycle_type.cell --target-profile ckb --entry-action novaseal_lifecycle` passes.
- `cellc src/nova_btc_authority_lock.cell --target-profile ckb` passes.
- `cellc src/nova_receipt_type.cell --target-profile ckb` passes.

Live local devnet:

- `scripts/novaseal_devnet_stateful_live.py` passes.
- It deploys the BIP340 runtime verifier as a live CellDep.
- It deploys `novaseal_lifecycle` as a live VM2/data2 type-script CellDep.
- It commits bootstrap -> key-auth transition by RPC.
- It verifies the old state is dead and the new state + receipt outputs are live.
- It dry-runs wrong-signature rejection without consuming the live state.

Aggregate stateful gate:

- `scripts/novaseal_devnet_stateful_acceptance.py --pretty --report-only` reports `status=passed`, `live_devnet_rpc_executed=true`, `blockers=0`.
- The same aggregate gate includes Agreement Profile originate -> repay, originate -> claim, and live negative dry-runs.

## Current Generated Audit Surface

After:

```bash
cellc audit-bundle --target-profile ckb --json
python3 scripts/novaseal_audit_surface.py --pretty
```

the derived audit surface reports:

```text
actions=1
locks=1
source_units=4
proof_plan_records=52
builder_assumptions=48
runtime_gaps=3
strict_prediction_errors=3
classification=non_production_audit_surface
```

The generated bundle exposes:

- action: `key_auth_transition`
- lock: `btc_authority`
- source units:
  - `src/nova_btc_authority_lock.cell`
  - `src/nova_receipt_type.cell`
  - `src/nova_state_lifecycle_type.cell`
  - `src/nova_state_type.cell`
- generic BTC BIP340 verifier wiring through `verifier::btc::bip340::require_signature(...)`
- manifest-bound spawn target obligations for the runtime verifier
- checked IPC envelope and child exit-status records

The generated bundle still leaves three strict ProofPlan gaps:

```text
create-output:NovaSealCellV0:new_cell
create-output:ProofReceiptV0:receipt
resource-conservation:NovaSealCellV0
```

This means `cellc check --target-profile ckb --primitive-strict 0.16` currently
fails with `PP0150` for those three records. The local transaction harness and
live devnet evidence are stronger runtime evidence, but they do not turn those
generated ProofPlan records into strict generated audit coverage.

## Schema And Vectors

`python3 scripts/novaseal_schema_layout.py --pretty` reports:

```text
NovaSealCellV0: fields=7 size=146 bytes
NovaSealCellCommitmentV0: fields=6 size=114 bytes
NovaSealIntentCoreV0: fields=11 size=222 bytes
NovaSealSignedIntentV0: fields=2 size=254 bytes
ProofReceiptCommitmentV0: fields=13 size=310 bytes
ProofReceiptV0: fields=16 size=382 bytes
```

`python3 scripts/novaseal_canonical_vectors.py --pretty` reports:

```text
vectors=8
signed_intent_vectors=8
resolved_receipt_matches=8
latest_receipt_matches=8
receipt_commitment_status=split_intent_and_explicit_receipt_commitment
```

The current receipt rule is:

```text
intent_core_hash = hash_blake2b_packed(NovaSealIntentCoreV0)
new_cell_commitment = hash_blake2b_packed(NovaSealCellCommitmentV0)
latest_receipt_hash = hash_blake2b_packed(ProofReceiptCommitmentV0)
signed_intent_hash = hash_blake2b_packed(NovaSealSignedIntentV0)
```

The old "ProofReceiptV0 excluding intent_hash" candidate is obsolete.

## Harness Evidence

State type CKB VM harness:

```text
total_cases=8
accepted=2
rejected=6
state_type_matched_expected=8
source_fixture_matched_by_state_type_only=7
source_fixture_requires_lock_or_external_context=1
shared witness payload size=398 bytes
```

The unmatched source fixture is expected: `wrong_signature_reject` belongs to
authority-lock/runtime-verifier scope, not type-action scope.

Combined lock + type transaction harness:

```text
total_cases=8
expected_accept=1
expected_reject=7
matched_expected=8
node_stack_matched_expected=8
shared_witness_size_bytes=398
max_full_transaction_cycles=7651736
max_node_stack_cycles=7651736
max_consensus_tx_size_bytes=1484
max_output_occupied_capacity_shannons=70700000000
```

This is local CKB node-verification-stack evidence over deterministic
transactions. It is not a public/shared devnet deployment pin.

## TCB Position

The BTC verifier remains an external runtime-verifier TCB item. Current evidence
for it includes:

- reference BIP340 vectors,
- fixed IPC envelope vectors,
- no-std/RISC-V verifier core,
- staged RISC-V shell ELF,
- child-verifier CKB VM execution,
- parent-lock CKB VM execution,
- resolved lock-group and full transaction script-verifier evidence,
- combined eight-fixture local CKB contextual verifier evidence,
- live local devnet key-auth transition evidence.

This is a strong local evidence stack, but it is not a substitute for publishing
and pinning production/testnet CellDeps with audited artifact hashes.

## Remaining Production Blockers

- Strict generated ProofPlan still has the three runtime-required gaps listed above.
- Molecule/reference encoding and wallet signing vectors are not frozen.
- Public/shared devnet or testnet CellDep publication is not pinned in the release manifest.
- The runtime BIP340 verifier binary still needs external review as TCB.
- v0 has only `latest_receipt_hash`; it does not provide a historical receipt accumulator.

Any claim of "production ready", "mainnet ready", or "fully audited" is false
until those blockers are closed.

## Related Docs

- `docs/RECEIPT_COMMITMENT_SPEC.md`
- `docs/CANONICAL_VECTORS.md`
- `docs/SCHEMA_LAYOUT.md`
- `docs/FIXTURE_HARNESS.md`
- `docs/STATE_TYPE_CKB_VM_HARNESS.md`
- `docs/COMBINED_TX_HARNESS.md`
- `docs/BTC_VERIFIER_SPEC.md`
- `docs/VERIFIER_IPC_CONTRACT.md`
- `docs/RISCV_VERIFIER_SHELL.md`
- `docs/RISCV_SHELL_ARTIFACT.md`
- `docs/CKB_VM_CHILD_VERIFIER.md`
- `docs/PARENT_LOCK_CKB_VM_HARNESS.md`
- `docs/SPAWN_BACKEND_BLOCKER.md`
- `docs/DEVNET_STATEFUL_ACCEPTANCE.md`
