# NovaSeal State Type CKB VM Harness

**Date**: 2026-05-30
**Harness**: `harness/ckb_vm/src/bin/novaseal_state_type_harness.rs`
**Report**: `target/novaseal-state-type-ckb-vm-report.json`
**Classification**: state-transition action CKB VM fixture evidence.

## Command

```bash
/Users/arthur/RustroverProjects/CellScript/target/debug/cellc src/nova_state_type.cell --target riscv64-elf --target-profile ckb --entry-action key_auth_transition -o target/novaseal-state-type-action.elf
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_state_type_harness -- --pretty
```

## Current Result

```text
state_type_action_ckb_vm_executed=true
total_cases=6
accepted=2
rejected=4
state_type_matched_expected=6
state_type_mismatched=0
source_fixture_matched_by_state_type_only=5
source_fixture_requires_lock_or_external_context=1
max_cycles=16621
load_witness_calls=6
load_cell_data_calls=12
load_header_by_field_calls=6
wrong_signature_is_lock_scope=true
schema_cell_intent_mismatch_detected=false
schema_cell_intent_aligned=true
```

## Boundary

This executes the compiled `key_auth_transition` action ELF in `ckb-vm` with harnessed `LOAD_WITNESS`, `LOAD_CELL_DATA`, and `LOAD_HEADER_BY_FIELD` syscalls. It covers the state/type transition guards, not the BTC authority lock.

The action now parses the same 389-byte `CSARGv1` witness payload shape as the authority lock: `intent`, `receipt_hash`, `state_hash_commitment`, then `SignaturePayload`. The type/action layer ignores the signature bytes; parsing them here prevents the future combined lock+type transaction harness from needing two incompatible witness formats.

Important: `wrong_signature_reject` is expected to pass at this layer because signature rejection belongs to `btc_authority`. The full fixture result still requires the lock path.

## Schema Alignment

The harness previously found a schema alignment bug. That specific `.cell`/schema mismatch is now closed:

- `schemas/nova_intent_v0.schema` defines `old_cell: OutPoint`, producing a 213-byte canonical intent.
- `src/nova_state_type.cell` and `src/nova_btc_authority_lock.cell` now inline `old_cell: OutPoint`, producing the same 213-byte action ABI.

The harness no longer adapts or shortens canonical intent vectors. Remaining schema work is still non-trivial: publish the Molecule reference encoding, wallet signing vectors, and alignment tests before any production-readiness claim.
