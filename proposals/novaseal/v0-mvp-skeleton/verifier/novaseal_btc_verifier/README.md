# novaseal_btc_verifier

Host reference verifier for the NovaSeal v0 BIP340 profile.

This crate verifies the reference vectors in `target/novaseal-btc-verifier-vectors.json`.
It is not yet the CKB RISC-V verifier binary and is not wired into `nova_btc_authority_lock.cell`.
The fixed IPC envelope parser is shared with the no-std `../novaseal_btc_verifier_core` crate.

```bash
cargo test
cargo check --manifest-path ../novaseal_btc_verifier_core/Cargo.toml --target riscv64imac-unknown-none-elf
cargo run -- verify-vectors --vectors ../../target/novaseal-btc-verifier-vectors.json
cargo run -- verify-ipc-vectors --vectors ../../target/novaseal-btc-verifier-ipc-vectors.json
```

Single-vector form:

```bash
cargo run -- verify \
  --message32 0x... \
  --xonly-pubkey 0x... \
  --signature64 0x...
```

IPC-envelope form:

```bash
cargo run -- verify-ipc --blob 0x...
```

The fixed IPC envelope is documented in `../../docs/VERIFIER_IPC_CONTRACT.md`.
