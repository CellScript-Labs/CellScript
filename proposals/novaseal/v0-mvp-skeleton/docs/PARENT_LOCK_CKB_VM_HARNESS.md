# NovaSeal Parent Lock CKB VM Harness

**Date**: 2026-05-30
**Harness**: `harness/ckb_vm/src/bin/novaseal_parent_lock_harness.rs`
**Report**: `target/novaseal-parent-lock-ckb-vm-report.json`
**Classification**: parent-lock + child-verifier CKB VM harness evidence, consensus-packed transaction-shape measurement, official `ckb-script` resolved lock-group verifier evidence, and official full transaction script-verifier evidence.

## Command

```bash
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty
```

## What It Executes

The harness runs the compiled parent `btc_authority` lock ELF in `ckb-vm` and implements the narrow syscall set needed by that lock:

- `load_script`
- `load_witness`
- `load_cell_data`
- VM2 `pipe`
- VM2 `pipe_write`
- VM2 `spawn`
- VM2 `wait`
- VM2 `close`

The manual parent syscall layer follows the official VM2 pipe ordering: the parent spawns the child reader first, writes the 18-word IPC envelope, closes the write fd, and waits. The nested child VM is executed when the parent waits, after the envelope is complete.

The same run also builds a conservative `ckb-types` `ResolvedTransaction` for each case and runs both official `ckb-script::TransactionScriptsVerifier::verify_single` lock-group verification and full `TransactionScriptsVerifier::verify` transaction script verification. The shape records:

- `cell_deps[0]` as the child verifier `code` dep used by the `spawn_with_fd` ABI,
- a second parent-lock code dep,
- a one-input lock `ScriptGroup` shape,
- consensus serialized tx size,
- output occupied capacity,
- a positive capacity margin,
- an under-capacity output capacity that the shape checker rejects.

## Current Result

```text
parent_lock_ckb_vm_executed=true
parent_spawn_executed=true
child_verifier_ckb_vm_executed=true
transaction_shape_constructed=true
consensus_packed_tx_constructed=true
resolved_transaction_constructed=true
resolved_script_verifier_executed=true
resolved_script_verifier_matched_expected=true
full_transaction_constructed=true
full_transaction_executed=true
full_transaction_verifier_matched_expected=true
total_cases=4
accepted=1
rejected=3
matched_expected=4
mismatched=0
parent_max_cycles=48783
child_max_cycles=3487016
resolved_script_verifier_max_cycles=3702919
full_transaction_verifier_max_cycles=3702919
max_consensus_tx_size_bytes=859
max_output_occupied_capacity_shannons=21900000000
min_capacity_margin_shannons=10000000000
capacity_shape_checks_passed=true
under_capacity_shape_rejects=true
```

Cases:

- valid parent-computed digest + BIP340 signature: accepted
- signature bitflip: rejected after parent spawn + child verifier reject
- valid BIP340 signature from a non-authority x-only pubkey: rejected before spawn
- authority hash mismatch: rejected before spawn

## Boundary

This is stronger than child-only evidence because the parent lock now constructs the IPC envelope, calls VM2 spawn, waits for the child, observes the child exit status, and the official `ckb-script` lock-group and full transaction script verifiers match the expected result for the four authority cases.

The parent lock now parses the same 398-byte `CSARGv1` witness payload shape as the state action: `NovaSealSignedIntentV0`, `state_hash_commitment`, then `SignaturePayload`. The signed intent already contains `expected_receipt_hash`; the lock ignores the state commitment, but the shared payload removes the former witness-format split between lock and type/action execution.

It is still not production acceptance evidence:

- official `ckb-script` full transaction script verification is executed for the four parent authority cases, but not a public/shared deployment attestation flow,
- the resolved transaction is harness-constructed and not yet produced by a production builder,
- capacity, occupied-capacity, tx-size, and under-capacity rejection are still shape-level measurements,
- this parent-lock harness covers the four authority cases; full lock+type
  transaction evidence for all eleven transition fixtures is recorded in
  `docs/COMBINED_TX_HARNESS.md` and `target/novaseal-combined-tx-report.json`.
