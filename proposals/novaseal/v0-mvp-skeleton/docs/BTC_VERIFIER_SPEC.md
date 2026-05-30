# NovaSeal v0 BTC Verifier Spec

**Date**: 2026-05-30
**Vector generator**: `scripts/novaseal_btc_verifier_vectors.py`
**IPC generator**: `scripts/novaseal_btc_verifier_ipc_vectors.py`
**Host verifier**: `verifier/novaseal_btc_verifier`
**No-std IPC core**: `verifier/novaseal_btc_verifier_core`
**RISC-V shell**: `verifier/novaseal_btc_verifier_riscv`
**CKB VM child harness**: `harness/ckb_vm`
**Report**: `target/novaseal-btc-verifier-vectors.json`
**IPC report**: `target/novaseal-btc-verifier-ipc-vectors.json`
**Child VM report**: `target/novaseal-ckb-vm-child-verifier-report.json`
**Parent VM report**: `target/novaseal-parent-lock-ckb-vm-report.json`
**Status**: reference vectors plus no-std/RISC-V verifier implementation plus child-verifier CKB VM, parent-lock CKB VM, official resolved lock-group verifier execution, and official full transaction script-verifier execution; no live-chain NovaSeal RPC submission evidence yet.

This spec fixes the v0 MVP verifier shape to a single-key BIP340 Schnorr profile. ECDSA and multisig descriptors remain out of scope for this strict MVP slice.

## Scheme

| Field | v0 value |
| --- | --- |
| Scheme | `bip340_schnorr_secp256k1` |
| Curve | `secp256k1` |
| Public key | x-only 32-byte BIP340 pubkey |
| Signature | 64-byte `r || s` BIP340 signature |
| Message | 32-byte `signed_intent_hash_after_resolved_receipt` from `target/novaseal-canonical-vectors.json` |
| Low-S | Not applicable to BIP340 Schnorr; reject `s >= n` |
| Malleability checks | reject `r >= p`; reject `s >= n`; lift x-only pubkey to even-y point; require reconstructed `R` has even y |

The message is already the packed-reference NovaSeal signed intent hash. The verifier must not reinterpret the original fixture JSON.

## Current Vector Evidence

Run:

```bash
python3 scripts/novaseal_schema_layout.py --pretty
python3 scripts/novaseal_canonical_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_ipc_vectors.py --pretty
cargo check --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml --target riscv64imac-unknown-none-elf
cargo test --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml
cargo test --manifest-path verifier/novaseal_btc_verifier/Cargo.toml
cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-vectors --vectors target/novaseal-btc-verifier-vectors.json
cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-ipc-vectors --vectors target/novaseal-btc-verifier-ipc-vectors.json
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
python3 scripts/novaseal_btc_verifier_shell_report.py --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty
```

Current summary:

```text
positive=24
negative=30
positive_self_verified=24
negative_self_rejected=30
host_verifier_checked=54
host_verifier_matched=54
ipc_vectors=54
malformed_ipc_vectors=5
host_ipc_checked=59
host_ipc_matched=59
core_riscv_check=passed
riscv_shell_build=passed
riscv_shell_accepted=24
riscv_shell_rejected=35
riscv_shell_matched_expected=59
child_vm_checked=59
child_vm_matched_expected=59
child_vm_max_cycles=3487024
parent_lock_ckb_vm_executed=true
parent_spawn_executed=true
parent_vm_matched_expected=3
parent_vm_max_cycles=24949
parent_resolved_script_verifier_matched_expected=true
parent_resolved_script_verifier_max_cycles=3678905
parent_full_transaction_verifier_matched_expected=true
parent_full_transaction_verifier_max_cycles=3678905
```

The positive set contains 4 deterministic test signers for each of the 6 fixtures.

The negative set contains 5 mutations per fixture:

- wrong message,
- wrong pubkey,
- signature bit flip,
- `s` out of range,
- `r` out of range.

## Test-Only Secrets

The vector report includes `test_secret_key` values. These are deterministic fixture-derived keys for reproducible tests only. They must never be used as production keys.

## Verifier I/O Contract

A real `novaseal_btc_verifier` binary should accept at minimum:

- `message32`,
- `xonly_pubkey`,
- `signature64`.

The fixed v0 IPC envelope for those fields is documented in `docs/VERIFIER_IPC_CONTRACT.md`. The current envelope is exactly 144 bytes and starts with ASCII `NSBV0IPC`.

It should return:

- success for a valid signature,
- reject for any malformed length,
- reject for `r >= p`,
- reject for `s >= n`,
- reject for invalid x-only pubkey,
- reject for wrong message/pubkey/signature.

## Current Limits

This does not yet prove criterion 6 on chain:

- the `.cell` lock delegates to the RISC-V BIP340 shell and the parent-lock CKB VM harness now executes parent spawn plus nested child verification,
- the generated audit bundle exposes `btc_authority` lock-args binding and spawn/IPC shell wiring, while cryptographic execution evidence remains harness-level rather than generated ProofPlan transaction coverage,
- the current CellScript VM2 spawn helper emits executable VM2 `ecall` wrappers and static spawn targets have a strict first-CellDep `code` manifest-bound builder model; the NovaSeal lock uses `spawn_with_fd` and the fixed 18-word IPC envelope,
- the Rust verifier is implemented in the shared no-std core and reused by the host verifier and RISC-V shell; the staged child ELF now executes in CKB VM with harness-provided inherited-fd input,
- resolved lock-group and full transaction script-verifier evidence now record `cell_deps[0]`, parent lock dep, lock ScriptGroup shape, tx size, occupied capacity, under-capacity shape rejection, and `ckb-script` verifier cycles for the three parent authority cases,
- no live-chain NovaSeal RPC submission exists; six-fixture combined transaction verifier evidence is harness-level only.

The next implementation slice should turn the current full transaction script-verifier execution into live-chain NovaSeal RPC submission evidence without pretending that harness-level VM success alone is production coverage.
