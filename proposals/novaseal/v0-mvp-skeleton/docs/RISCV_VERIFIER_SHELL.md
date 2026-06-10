# NovaSeal v0 RISC-V Verifier Shell

**Date**: 2026-05-31
**Shell crate**: `verifier/novaseal_btc_verifier_riscv`
**Core parser**: `verifier/novaseal_btc_verifier_core`
**Report**: `target/novaseal-btc-verifier-shell-report.json`
**Artifact preflight**: `target/novaseal-riscv-shell-artifact.json`
**Child VM report**: `target/novaseal-ckb-vm-child-verifier-report.json`
**Status**: RISC-V ELF shell exists; inherited-fd spawn input adapter uses the official VM2 buffer/length ABI; no-std BIP340 verification is wired; child-verifier CKB VM, parent-lock CKB VM, official resolved lock-group execution, and official full transaction script-verifier execution exist.

This slice turns the previous parse-only shell into a real BIP340 verifier boundary and then executes the staged child ELF in CKB VM. A separate parent-lock harness now executes parent spawn plus nested child verification, official resolved lock-group verification, and official full transaction script verification; the combined harness now executes all eleven fixtures with both lock and type/action ScriptGroups present, including valid-signature-by-wrong-pubkey rejection, authority mapping mismatch rejection, and implicit authority-rotation rejection. The remaining production gaps are public/shared deployment attestation and external TCB review.

## Current Behaviour

The shell library applies this policy:

| Input | Result |
| --- | --- |
| inherited fd or pipe read failure | reject with `EXIT_REJECT_SPAWN_IO = 11` |
| malformed IPC envelope | reject with `EXIT_REJECT_ENVELOPE = 10` |
| invalid BIP340 key/signature/message | reject with `EXIT_REJECT_CRYPTO = 12` |
| valid BIP340 signature | accept with `EXIT_ACCEPT = 0` |

The binary target provides a `_start` entry that, on `riscv64`, reads inherited fd index `0` using the official VM2 `inherited_fd(buffer, length_ptr)` syscall, then requires exactly 18 little-endian `u64` words with `pipe_read(fd, buffer, length_ptr)`. It probes for a nineteenth word and rejects the stream as spawn-input failure if one exists. The canonical 18 words reconstruct the fixed 144-byte IPC envelope and then feed the same no-std BIP340 policy.

## Current Evidence

Run:

```bash
cargo check --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml
cargo test --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --lib
cargo clippy --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --lib -- -D warnings
cargo clippy --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv -- -D warnings
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
cargo build --manifest-path verifier/novaseal_btc_verifier_riscv/Cargo.toml --release --target riscv64imac-unknown-none-elf --bin novaseal_btc_verifier_riscv
python3 scripts/novaseal_btc_verifier_shell_report.py --pretty
python3 scripts/novaseal_riscv_shell_artifact.py --sync --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
```

Current summary:

```text
core_unit_tests=7
shell_lib_unit_tests=7
riscv_binary_build=passed
riscv_release_elf_size_bytes=187808
shell_vectors_total=105
parse_ok=99
parse_rejected=6
spawn_word_roundtrip=103
spawn_io_rejects=2
accepted=44
rejected=61
matched_expected=105
all_expected_matched=true
staged_release_elf_sha256=b8a7e8e53d36979d74f55252b52f8b8cc6f3bc8277250d2f8d2d08abd5237459
child_vm_executed=true
child_vm_matched_expected=105
child_vm_max_cycles=3487544
```

The staged release ELF at `target/novaseal-btc-verifier-riscv-shell-release.elf` is now checked against the current release build by `scripts/novaseal_riscv_shell_artifact.py`. The preflight also confirms that the generated CellScript audit surface exposes the intended lock spawn/pipe/wait records.
The same staged ELF is executed by `harness/ckb_vm` with harness-provided official VM2 inherited-fd, pipe-read, and close syscalls.

## Unsafe Boundary

The binary target uses Rust 2024's `#[unsafe(no_mangle)]` on `_start` so the bare-metal entry symbol is exported. On `riscv64`, it also uses small inline `ecall` wrappers for the CKB VM v2 inherited-fd, pipe-read, close, and process-exit boundaries.

There is still no raw pointer dereference, transmute, mutable static, or C FFI memory access in this slice. The unsafe boundary is syscall register ABI only.

## Current Limits

This still does not prove criterion 6 on chain:

- the binary requires exactly 18 inherited-fd words at index `0`, rejects complete trailing words, and the child-verifier CKB VM harness now executes that child path,
- `nova_btc_authority_lock.cell` now spawns this binary through `spawn_with_fd`, and the parent-lock CKB VM harness executes that parent/child path,
- the current CellScript VM2 `spawn_with_fd` helper emits executable `ecall`, but only first-CellDep `code` target binding is modelled (see `docs/SPAWN_BACKEND_BLOCKER.md`),
- the generated `btc_authority` lock surface covers Script.args binding and spawn/IPC shell wiring, while the crypto decision is evidenced by verifier vectors, RISC-V build/tests, child-verifier CKB VM execution, parent-lock CKB VM execution, official resolved lock-group execution, and official full transaction script-verifier execution,
- the parent-lock harness now records transaction-shape occupied-capacity, transaction-size, under-capacity checks, resolved lock-group cycles, and full transaction script-verifier cycles, but this is not public/shared deployment attestation.

The next shell slice should attach real public/shared CellDep attestation and external TCB review to the production gate while preserving strict-mode honesty.
