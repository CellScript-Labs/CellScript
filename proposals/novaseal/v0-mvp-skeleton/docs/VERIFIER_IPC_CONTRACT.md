# NovaSeal v0 Verifier IPC Contract

**Date**: 2026-05-31
**Generator**: `scripts/novaseal_btc_verifier_ipc_vectors.py`
**No-std core**: `verifier/novaseal_btc_verifier_core`
**RISC-V shell**: `verifier/novaseal_btc_verifier_riscv`
**Report**: `target/novaseal-btc-verifier-ipc-vectors.json`
**Status**: fixed lock-to-verifier envelope for host-reference validation, no-std/RISC-V BIP340 shell, child-verifier CKB VM, parent-lock CKB VM harness execution, official resolved lock-group verifier evidence, official full transaction script-verifier evidence, live local devnet pinning, and local TCB review bundle; no public/shared deployment attestation or external TCB review yet.

This document freezes the first v0 binary request shape that `nova_btc_authority_lock.cell` must eventually pass to `novaseal_btc_verifier`.

## Request Envelope

All integer fields are little-endian. Total size is exactly 144 bytes.

| Offset | Size | Field | Value |
| --- | ---: | --- | --- |
| 0 | 8 | `magic` | ASCII `NSBV0IPC` |
| 8 | 2 | `version_u16_le` | `0` |
| 10 | 2 | `scheme_u16_le` | `1` = BIP340 Schnorr secp256k1 |
| 12 | 4 | `flags_u32_le` | `0` |
| 16 | 32 | `message32` | `signed_intent_hash_after_resolved_receipt` |
| 48 | 32 | `xonly_pubkey` | BIP340 x-only pubkey |
| 80 | 64 | `signature64` | BIP340 `r || s` |

The verifier must reject:

- wrong total length,
- wrong magic,
- unsupported version,
- unsupported scheme,
- non-zero flags,
- invalid key/signature encoding,
- invalid signature.

## Return Contract

For the eventual CKB/RISC-V verifier binary:

- exit code `0`: accept,
- any non-zero exit code: reject,
- stdout/stderr are not consensus inputs,
- no JSON is required on chain.

The host CLI still prints JSON for local automation.

## Current Evidence

Run:

```bash
python3 scripts/novaseal_btc_verifier_vectors.py --pretty
python3 scripts/novaseal_btc_verifier_ipc_vectors.py --pretty
cargo check --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml --target riscv64imac-unknown-none-elf
cargo test --manifest-path verifier/novaseal_btc_verifier_core/Cargo.toml
cargo run --manifest-path verifier/novaseal_btc_verifier/Cargo.toml -- verify-ipc-vectors --vectors target/novaseal-btc-verifier-ipc-vectors.json
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
python3 scripts/novaseal_btc_verifier_shell_report.py --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_parent_lock_harness -- --pretty
```

Current summary:

```text
source_positive=32
source_negative=40
ipc_vectors=72
malformed_vectors=5
total_vectors=77
expected_accept=32
expected_reject=45
host_ipc_checked=77
host_ipc_matched=77
core_unit_tests=2
core_riscv_check=passed
riscv_shell_build=passed
riscv_shell_accepted=32
riscv_shell_rejected=45
riscv_shell_matched_expected=77
child_vm_matched_expected=77
parent_vm_matched_expected=4
parent_resolved_script_verifier_matched_expected=true
parent_full_transaction_verifier_matched_expected=true
parent_lock_transaction_shape_constructed=true
parent_lock_max_consensus_tx_size_bytes=859
parent_lock_max_output_occupied_capacity_shannons=21900000000
```

The malformed set covers wrong magic, unsupported version, unsupported scheme, non-zero flags, and truncated blob.

## Current Limits

This is still not production/public/shared criterion 6 evidence:

- `nova_btc_authority_lock.cell` constructs this envelope and sends it through `spawn_with_fd`,
- lock-level verifier spawn is generated and points at the BIP340 shell; the parent-lock CKB VM harness now executes parent spawn plus nested child verification,
- current CellScript VM2 spawn helper lowering emits executable VM2 `ecall` wrappers and unmanifested spawn targets still strict-fail, while first-CellDep `code` manifest-bound targets become builder-required; the `spawn_with_fd(target, fd)` helper now supplies a one-entry inherited-fd list,
- the compiler now has a protocol-agnostic `fixed_u64_le(bytes, word_index)` extractor for Hash/Address/[u8; N] values, so a lock can build the 18-word envelope without NovaSeal-specific compiler recognition,
- the current RISC-V verifier binary shell reads inherited fd index `0` as 18 little-endian `u64` words, matches all BIP340 IPC vectors, and now runs in the child-verifier CKB VM harness,
- the no-std core currently covers both the envelope parser and BIP340 verification,
- child-verifier and parent-lock CKB VM harness evidence exists, and the parent-lock harness now records transaction-shape tx-size/capacity facts plus official resolved lock-group verifier success and full transaction script-verifier success,
- no public/shared CellDep attestation exists yet,
- no external BIP340 TCB review exists yet,
- the generated `btc_authority` lock surface proves Script.args binding and spawn/IPC wiring, while crypto execution remains external harness evidence.

This contract is deliberately small so the next slice can attach public/shared CellDep attestation and external TCB review without changing the signed message or public vector set.
