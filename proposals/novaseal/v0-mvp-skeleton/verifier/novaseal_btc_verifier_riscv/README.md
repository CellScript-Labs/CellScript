# novaseal_btc_verifier_riscv

No-std RISC-V verifier shell for NovaSeal v0.

Current status: no-std BIP340 verifier shell. The library classifies fixed IPC envelopes, reconstructs them from the 18 little-endian `u64` words used by the current Spawn/IPC helper surface, and verifies BIP340 Schnorr signatures through the shared no-std core. The RISC-V `_start` reads inherited fd index `0`; well-formed valid envelopes exit with `0`, wrong signatures reject with `EXIT_REJECT_CRYPTO`, and malformed envelopes reject before crypto.

```bash
cargo check
cargo test
cargo clippy --lib -- -D warnings
cargo build --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
```

This crate is evidence that the verifier shell boundary can compile for RISC-V, has a fixed spawn-input adapter, and makes the expected BIP340 decision over the frozen vector set. The staged ELF is also executed by `../novaseal_ckb_vm_harness` with child-side inherited-fd input. It is not yet production on-chain verifier evidence because no CKB VM transaction dry-run has executed the parent lock spawning this exact staged binary.
