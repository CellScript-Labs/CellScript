# NovaSeal v0 Fixture Harness

**Date**: 2026-05-30
**Harness**: `scripts/novaseal_fixture_harness.py`
**Report**: `target/novaseal-fixture-report.json`
**Classification**: model-level fixture evidence.

This harness is the deterministic runner for the six NovaSeal v0 fixture JSON files. It intentionally does **not** execute the six fixtures as parent-lock CKB VM transactions or construct fixture-specific full transactions. It now attaches the separate child-verifier CKB VM report and parent-lock CKB VM report when available.

## What It Executes

The harness applies the source-level guard semantics from `src/nova_state_type.cell`:

- `intent.old_state_hash == old_cell.state_hash`
- `hash_blake2b(intent.new_state_hash) == state_hash_commitment` (modelled with a deterministic harness hash, not CKB VM execution)
- `intent.policy_hash == old_cell.policy_hash`
- `intent.nonce == old_cell.nonce + 1`
- `now <= intent.expiry`
- `receipt_hash == intent.receipt_hash`

It also applies a fixture-declared BTC signature delegate result:

- `"valid ..."` / explicit success means the model proceeds to the type-script guards.
- `"invalid ..."` / explicit failure means the model rejects with `btc_signature_verification_failed`.

Finally, it reads `target/novaseal-audit-surface.json` to attach current artifact facts:

- one generated action,
- one generated lock,
- visible consume of `old_cell`,
- visible output of `new_cell`,
- current `resource-conservation:NovaSealCellV0` checked-runtime relation coverage.

If `target/novaseal-canonical-vectors.json` exists, the report also attaches its summary and receipt commitment status. The harness does not require those vectors to pass.

If `target/novaseal-btc-verifier-vectors.json` exists, the report also attaches its BIP340 vector summary. The harness still treats BTC signature verification as fixture-declared model input.

If `target/novaseal-btc-verifier-ipc-vectors.json` exists, the report also attaches its fixed IPC envelope summary and layout contract. The harness still does not execute CKB spawn.

If `target/novaseal-btc-verifier-shell-report.json` exists, the report also attaches the RISC-V BIP340 shell summary. The shell report is local verifier evidence, not CKB VM transaction evidence.

If `target/novaseal-ckb-vm-child-verifier-report.json` exists, the report also attaches the child-verifier CKB VM summary. The fixture harness still does not execute the parent lock or a full transaction.

If `target/novaseal-parent-lock-abi-preflight.json` exists, the report also attaches the parent-lock ASM/ELF ABI preflight summary. This proves parent artifact shape, not VM execution by itself.

If `target/novaseal-parent-lock-ckb-vm-report.json` exists, the report also attaches the parent-lock CKB VM summary. This proves the parent lock ELF can construct the IPC envelope, spawn the staged child verifier ELF, wait, and observe valid/wrong signature outcomes in a harnessed VM setting. It also attaches the current consensus-packed transaction-shape measurements, official resolved lock-group verifier evidence, and official full transaction script-verifier evidence for the three parent authority cases: tx size, ScriptGroup shape, `cell_deps[0]` spawn-target model, occupied capacity, under-capacity shape rejection, and `ckb-script` verifier cycles. It is still not a six-fixture transaction run or production builder/full-node acceptance.

If `target/novaseal-state-type-ckb-vm-report.json` exists, the report also attaches the state type action CKB VM summary. This executes `key_auth_transition` for all six fixtures at action/type scope. It is not lock execution: `wrong_signature_reject` must still be rejected by `btc_authority`, and the current state harness records that explicitly. The `.cell` action ABI now uses the same 213-byte `old_cell: OutPoint` intent shape as the canonical schema vectors.

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
cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
python3 scripts/novaseal_parent_lock_abi_preflight.py --pretty
cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty
python3 scripts/novaseal_fixture_harness.py --pretty
```

Current summary:

```text
fixtures=6
matched=6
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
state_type_source_fixture_matched_by_state_type_only=5
state_type_source_fixture_requires_lock_or_external_context=1
state_type_schema_cell_intent_mismatch_detected=false
state_type_schema_cell_intent_aligned=true
shared_lock_type_witness_abi_aligned=true
shared_lock_type_witness_size_bytes=389
combined_full_transaction_executed=true
combined_full_transaction_matched_expected=true
combined_full_transaction_total_cases=6
combined_full_transaction_accepted=1
combined_full_transaction_rejected=5
combined_lock_and_type_script_groups_present=true
combined_shared_witness_abi_aligned=true
combined_builder_shape_checks_passed=true
combined_fee_shape_checks_passed=true
combined_under_capacity_shape_rejects=true
combined_min_fee_shannons=100000
combined_max_fee_shannons=100000
combined_full_transaction_max_cycles=3703418
combined_max_consensus_tx_size_bytes=972
combined_max_output_occupied_capacity_shannons=25200000000
parent_lock_max_consensus_tx_size_bytes=850
parent_lock_max_output_occupied_capacity_shannons=21900000000
```

## Evidence Level

This harness is useful because it makes the fixture set executable and repeatable. It is not production evidence.

It does **not** prove:

- live-chain ScriptGroup/cell_deps resolution selects the staged verifier shell,
- Molecule/wallet signing encoders are aligned with the `.schema` and `.cell` intent layout,
- production builder/full-node capacity/cycles/transaction size are acceptable.

It does prove:

- the fixture expectations are internally consistent with the current source guard semantics,
- all six fixtures can be deterministically evaluated,
- the current audit surface facts are attached to the report rather than hidden,
- wrong-signature coverage now has combined full transaction verifier evidence when `target/novaseal-combined-tx-report.json` is present,
- parent-lock ABI preflight facts are attached when present,
- the separate child-verifier report records CKB VM execution of the staged RISC-V verifier ELF across the frozen IPC corpus when present.
- the separate parent-lock report records parent ELF execution, VM2 spawn, nested child-verifier execution, valid-signature accept, and wrong-signature reject when present.
- the separate parent-lock report records consensus-packed transaction-shape size, occupied-capacity, under-capacity shape checks, resolved lock-group verifier execution, and full transaction script-verifier execution when present.
- the separate state-type report records all six `key_auth_transition` fixture runs in CKB VM at action/type scope when present.
- the parent-lock and state-type reports now both exercise the same 389-byte `CSARGv1` witness payload order, which removes a concrete blocker for a future same-input lock+type transaction harness.
- the separate combined transaction report records all six fixtures through official `ckb-script` full transaction verification with both lock and type/action ScriptGroups present when available.
- the separate combined transaction report records builder-shape fee, occupied-capacity, under-capacity, and code-dep role checks when available.
- the separate state-type report records that `wrong_signature_reject` is lock scope and that schema/.cell intent layout alignment is now closed for `old_cell: OutPoint`.

## Closure Path

The next harness slice should promote the current harness evidence toward production one step at a time:

1. Generate canonical Molecule bytes from the schema files.
2. Keep the receipt commitment rule from `docs/CANONICAL_VECTORS.md` aligned with those Molecule bytes.
3. Compare real Molecule bytes against `target/novaseal-schema-layout.json`.
4. Build real entry witnesses from each fixture.
5. Feed the combined six-fixture transactions through the production builder/full-node acceptance path.
6. Record production-style cycles, transaction size, occupied capacity, and under-capacity rejection.
7. Record valid and invalid authority-lock runs against the same staged ELF hash.
8. Keep the resolved `NovaSealIntentV0.old_cell: OutPoint` schema/.cell alignment covered by Molecule and wallet signing vectors.
9. Extend the current full transaction script-verifier layer into production builder/full-node acceptance and combined six-fixture lock+type transaction coverage.
