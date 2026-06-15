# novaseal_btc_verifier_riscv

No-std RISC-V verifier shell for NovaSeal v0.

Current status: no-std BIP340 verifier shell. The library classifies fixed IPC envelopes, reconstructs them from the 18 little-endian `u64` words used by the current Spawn/IPC helper surface, and verifies BIP340 Schnorr signatures through the shared no-std core. The RISC-V `_start` reads inherited fd index `0`; well-formed valid envelopes exit with `0`, wrong signatures reject with `EXIT_REJECT_CRYPTO`, and malformed envelopes reject before crypto.

```bash
cargo check
cargo test
cargo clippy --lib -- -D warnings
cargo build --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
```

This crate is evidence that the verifier shell boundary can compile for RISC-V, has a fixed spawn-input adapter, and makes the expected BIP340 decision over the frozen vector set. The staged ELF is executed by `../../harness/ckb_vm` with child-side inherited-fd input, by the parent-lock CKB VM harness, and by the combined lock/type transaction harness. Local devnet CellDep facts are pinned by the NovaSeal manifests and checked by the production gate; public/shared CellDep attestation and external TCB review remain open.
