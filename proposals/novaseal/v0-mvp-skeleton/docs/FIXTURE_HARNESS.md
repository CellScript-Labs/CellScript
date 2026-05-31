# NovaSeal v0 Fixture Harness

**Date**: 2026-05-31
**Harness**: `scripts/novaseal_fixture_harness.py`
**Report**: `target/novaseal-fixture-report.json`
**Classification**: model-level fixture evidence.

This harness is the deterministic runner for the eight NovaSeal v0 fixture JSON files. It intentionally does **not** execute the fixtures itself as parent-lock CKB VM transactions or construct fixture-specific full transactions. It attaches the separate child-verifier, parent-lock, state-type, and combined transaction reports when available.

## What It Executes

The harness applies the source-level guard semantics from `src/nova_state_type.cell`:

- `intent.core.old_cell` matches the actual previous outpoint,
- `intent.core.old_state_hash == old_cell.state_hash`,
- `hash_blake2b(intent.core.new_state_hash) == state_hash_commitment` in the source model,
- `intent.core.policy_hash == old_cell.policy_hash`,
- `intent.core.old_nonce == old_cell.nonce`,
- `intent.core.new_nonce == old_cell.nonce + 1`,
- `now <= intent.core.expiry`,
- `intent.expected_receipt_hash == materialized_receipt_hash`.

It also applies a fixture-declared BTC signature delegate result:

- `"valid ..."` / explicit success means the model proceeds to the type-script guards.
- `"invalid ..."` / explicit failure means the model rejects with `btc_signature_verification_failed`.

Finally, it reads `target/novaseal-audit-surface.json` to attach current artifact facts:

- one generated action,
- one generated lock,
- visible consume of `old_cell`,
- visible output of `new_cell`,
- current ProofPlan/runtime-gap classification.

If `target/novaseal-canonical-vectors.json` exists, the report also attaches its summary and receipt commitment status. The harness does not require those vectors to pass.

If `target/novaseal-btc-verifier-vectors.json` exists, the report also attaches its BIP340 vector summary. The harness still treats BTC signature verification as fixture-declared model input.

If `target/novaseal-btc-verifier-ipc-vectors.json` exists, the report also attaches its fixed IPC envelope summary and layout contract. The harness still does not execute CKB spawn.

If `target/novaseal-btc-verifier-shell-report.json` exists, the report also attaches the RISC-V BIP340 shell summary. The shell report is local verifier evidence, not CKB VM transaction evidence.

If `target/novaseal-ckb-vm-child-verifier-report.json` exists, the report also attaches the child-verifier CKB VM summary. The fixture harness still does not execute the parent lock or a full transaction.

If `target/novaseal-parent-lock-abi-preflight.json` exists, the report also attaches the parent-lock ASM/ELF ABI preflight summary. This proves parent artifact shape, not VM execution by itself.

If `target/novaseal-parent-lock-ckb-vm-report.json` exists, the report also attaches the parent-lock CKB VM summary. This proves the parent lock ELF can construct the IPC envelope, spawn the staged child verifier ELF, wait, and observe valid/wrong signature outcomes in a harnessed VM setting. It also attaches the current consensus-packed transaction-shape measurements, official resolved lock-group verifier evidence, and official full transaction script-verifier evidence for the three parent authority cases: tx size, ScriptGroup shape, `cell_deps[0]` spawn-target model, occupied capacity, under-capacity shape rejection, and `ckb-script` verifier cycles.

If `target/novaseal-state-type-ckb-vm-report.json` exists, the report also attaches the state type action CKB VM summary. This executes `key_auth_transition` for all eight fixtures at action/type scope. It is not final lock evidence: `wrong_signature_reject` must still be rejected by `btc_authority`, and the current state harness records that explicitly. The `.cell` action ABI now uses the same split `NovaSealSignedIntentV0 { core, expected_receipt_hash }` shape as the canonical schema vectors.

## Current Expected Result

Run:

```bash
cellc audit-bundle --target-profile ckb --json
python3 scripts/novaseal_audit_surface.py --pretty
python3 scripts/novaseal_schema_layout.py --pretty
python3 scripts/novaseal_canonical_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_ipc_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_shell_report.py --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
python3 scripts/novaseal_parent_lock_abi_preflight.py --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty
python3 scripts/novaseal_fixture_harness.py --pretty
```

Current summary:

```text
fixtures=8
matched=8
mismatched=0
ckb_vm_executed=false
child_verifier_ckb_vm_executed=true
parent_lock_abi_preflight_passed=true
parent_lock_ckb_vm_executed=true
parent_lock_spawn_executed=true
parent_lock_transaction_shape_constructed=true
parent_lock_resolved_script_verifier_executed=true
parent_lock_resolved_script_verifier_matched_expected=true
parent_lock_full_transaction_executed=true
parent_lock_full_transaction_verifier_matched_expected=true
state_type_action_ckb_vm_executed=true
state_type_action_matched_expected=true
state_type_source_fixture_matched_by_state_type_only=7
state_type_source_fixture_requires_lock_or_external_context=1
state_type_schema_cell_intent_mismatch_detected=false
state_type_schema_cell_intent_aligned=true
shared_lock_type_witness_abi_aligned=true
shared_lock_type_witness_size_bytes=398
combined_full_transaction_executed=true
combined_full_transaction_matched_expected=true
combined_full_transaction_total_cases=8
combined_full_transaction_accepted=1
combined_full_transaction_rejected=7
combined_lock_and_type_script_groups_present=true
combined_shared_witness_abi_aligned=true
combined_builder_shape_checks_passed=true
combined_fee_shape_checks_passed=true
combined_under_capacity_shape_rejects=true
combined_min_fee_shannons=100000
combined_max_fee_shannons=100000
combined_full_transaction_max_cycles=7651736
combined_max_consensus_tx_size_bytes=1484
combined_max_output_occupied_capacity_shannons=70700000000
parent_lock_max_consensus_tx_size_bytes=859
parent_lock_max_output_occupied_capacity_shannons=21900000000
```

## Evidence Level

This harness is useful because it makes the fixture set executable and repeatable. It is not production evidence.

It does **not** prove:

- public/shared devnet or testnet CellDep publication pins the staged verifier shell,
- external BIP340 TCB review has accepted the staged verifier shell,
- production/public deployment capacity and fee policy are final.

It does prove:

- the fixture expectations are internally consistent with the current source guard semantics,
- all eight fixtures can be deterministically evaluated,
- the current audit surface facts are attached to the report rather than hidden,
- wrong-signature coverage now has combined full transaction verifier evidence when `target/novaseal-combined-tx-report.json` is present,
- parent-lock ABI preflight facts are attached when present,
- the separate child-verifier report records CKB VM execution of the staged RISC-V verifier ELF across the frozen IPC corpus when present.
- the separate parent-lock report records parent ELF execution, VM2 spawn, nested child-verifier execution, valid-signature accept, and wrong-signature reject when present.
- the separate parent-lock report records consensus-packed transaction-shape size, occupied-capacity, under-capacity shape checks, resolved lock-group verifier execution, and full transaction script-verifier execution when present.
- the separate state-type report records all eight `key_auth_transition` fixture runs in CKB VM at action/type scope when present.
- the parent-lock and state-type reports now both exercise the same 398-byte `CSARGv1` witness payload order (`NovaSealSignedIntentV0`, `state_hash_commitment`, `SignaturePayload`), which removes a concrete blocker for same-input lock+type transaction evidence.
- the separate combined transaction report records all eight fixtures through official `ckb-script` full transaction verification with both lock and type/action ScriptGroups present when available.
- the separate combined transaction report records builder-shape fee, occupied-capacity, under-capacity, and code-dep role checks when available.
- the separate state-type report records that `wrong_signature_reject` is lock scope and that schema/.cell intent layout alignment is now closed for `old_cell: OutPoint`.

## Closure Path

The next harness slice should promote the current harness evidence toward production one step at a time:

1. Keep fixed-width wallet signing vectors aligned with `docs/CANONICAL_VECTORS.md`.
2. Keep the live local devnet RPC runner aligned with the combined eight-fixture transaction harness.
3. Record production-style cycles, transaction size, occupied capacity, and under-capacity rejection.
4. Record valid and invalid authority-lock runs against the same staged ELF hash.
5. Keep the resolved `NovaSealIntentCoreV0.old_cell: OutPoint` schema/.cell alignment covered by wallet signing vectors.
6. Attach public/shared CellDep attestation and external BIP340 TCB review before any production claim.
