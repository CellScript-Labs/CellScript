# NovaSeal v0 BTC Verifier Spec

**Date**: 2026-05-31
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
**TCB review bundle**: `target/novaseal-bip340-tcb-review.json`
**Production gate report**: `target/novaseal-production-gates.json`
**Status**: reference vectors plus no-std/RISC-V verifier implementation plus child-verifier CKB VM, parent-lock CKB VM, official resolved lock-group verifier execution, official full transaction script-verifier execution, local devnet pinning, and local TCB review; no public/shared deployment attestation or external TCB review yet.

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
python3 /home/arthur/a19q3/CellScript/scripts/novaseal_bip340_tcb_review.py --pretty
/home/arthur/a19q3/CellScript/target/debug/cellc certify --plugin novaseal-profile-v0 --json
```

Current summary:

```text
positive=32
negative=40
positive_self_verified=32
negative_self_rejected=40
host_verifier_checked=72
host_verifier_matched=72
ipc_vectors=72
malformed_ipc_vectors=5
host_ipc_checked=77
host_ipc_matched=77
core_riscv_check=passed
riscv_shell_build=passed
riscv_shell_accepted=32
riscv_shell_rejected=45
riscv_shell_matched_expected=77
child_vm_checked=77
child_vm_matched_expected=77
child_vm_max_cycles=3552601
parent_lock_ckb_vm_executed=true
parent_spawn_executed=true
parent_vm_matched_expected=4
parent_vm_max_cycles=48783
parent_resolved_script_verifier_matched_expected=true
parent_resolved_script_verifier_max_cycles=3702919
parent_full_transaction_verifier_matched_expected=true
parent_full_transaction_verifier_max_cycles=3702919
```

The positive set contains 4 deterministic test signers for each of the 9
fixtures.

The negative set contains 5 mutations per fixture:

- wrong message,
- wrong pubkey,
- signature bit flip,
- `s` out of range,
- `r` out of range.

The parent-lock harness also covers `parent_wrong_pubkey_valid_signature_reject`:
the signature is cryptographically valid for the witness pubkey, but that pubkey
does not match the protected Cell's declared BTC authority, so the lock rejects
before spawning the verifier.

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

Criterion 6 now has local and local-devnet evidence, not public/shared production
attestation or external BIP340 TCB acceptance. The audit split is:

- Criterion 6a: invalid BTC signatures reject.
- Criterion 6b: valid BTC signatures from non-authority x-only pubkeys reject.

- the `.cell` lock delegates to the RISC-V BIP340 shell and the parent-lock CKB VM harness now executes parent spawn plus nested child verification,
- the generated audit bundle exposes `btc_authority` lock-args binding and spawn/IPC shell wiring, while cryptographic execution evidence remains harness-level rather than generated ProofPlan transaction coverage,
- the current CellScript VM2 spawn helper emits executable VM2 `ecall` wrappers and static spawn targets have a strict first-CellDep `code` manifest-bound builder model; the NovaSeal lock uses `spawn_with_fd` and the fixed 18-word IPC envelope,
- the Rust verifier is implemented in the shared no-std core and reused by the host verifier and RISC-V shell; the staged child ELF now executes in CKB VM with harness-provided inherited-fd input,
- resolved lock-group and full transaction script-verifier evidence now record `cell_deps[0]`, parent lock dep, lock ScriptGroup shape, tx size, occupied capacity, under-capacity shape rejection, and `ckb-script` verifier cycles for the three parent authority cases,
- the local TCB review bundle is written to `target/novaseal-bip340-tcb-review.json`, and the local production gate report is written to `target/novaseal-production-gates.json`,
- no public/shared deployment attestation or external TCB review exists; eleven-fixture combined transaction verifier evidence is local node-verification-stack evidence.

The next implementation slice should attach real public/shared CellDep attestation and external TCB review without pretending that harness-level VM success alone is production coverage.
