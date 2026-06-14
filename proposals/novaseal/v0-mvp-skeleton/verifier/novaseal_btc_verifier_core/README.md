# novaseal_btc_verifier_core

No-std parser core for the NovaSeal v0 verifier IPC envelope.

This crate intentionally contains no BIP340 crypto, JSON, CLI, heap allocation, or CKB syscall code. It only validates the fixed 144-byte request envelope documented in `../../docs/VERIFIER_IPC_CONTRACT.md`.

```bash
cargo check
cargo test
cargo clippy --all-targets -- -D warnings
cargo check --target riscv64imac-unknown-none-elf
```

Current status: RISC-V-checkable parser core only. It is not a CKB executable verifier binary.
