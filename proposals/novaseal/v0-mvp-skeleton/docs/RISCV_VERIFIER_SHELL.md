# NovaSeal v0 RISC-V Verifier Shell

**Date**: 2026-05-30
**Shell crate**: `verifier/novaseal_btc_verifier_riscv`
**Core parser**: `verifier/novaseal_btc_verifier_core`
**Report**: `target/novaseal-btc-verifier-shell-report.json`
**Artifact preflight**: `target/novaseal-riscv-shell-artifact.json`
**Child VM report**: `target/novaseal-ckb-vm-child-verifier-report.json`
**Status**: RISC-V ELF shell exists; inherited-fd spawn input adapter uses the official VM2 buffer/length ABI; no-std BIP340 verification is wired; child-verifier CKB VM, parent-lock CKB VM, official resolved lock-group execution, and official full transaction script-verifier execution exist.

This slice turns the previous parse-only shell into a real BIP340 verifier boundary and then executes the staged child ELF in CKB VM. A separate parent-lock harness now executes parent spawn plus nested child verification, official resolved lock-group verification, and official full transaction script verification; the combined harness now executes all six fixtures with both lock and type/action ScriptGroups present. The remaining gap is production builder/full-node acceptance.

## Current Behaviour

The shell library applies this policy:

| Input | Result |
| --- | --- |
| inherited fd or pipe read failure | reject with `EXIT_REJECT_SPAWN_IO = 11` |
| malformed IPC envelope | reject with `EXIT_REJECT_ENVELOPE = 10` |
| invalid BIP340 key/signature/message | reject with `EXIT_REJECT_CRYPTO = 12` |
| valid BIP340 signature | accept with `EXIT_ACCEPT = 0` |

The binary target provides a `_start` entry that, on `riscv64`, reads inherited fd index `0` using the official VM2 `inherited_fd(buffer, length_ptr)` syscall, then reads exactly 18 little-endian `u64` words with `pipe_read(fd, buffer, length_ptr)`. Those words reconstruct the fixed 144-byte IPC envelope and then feed the same no-std BIP340 policy.

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
cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
```

Current summary:

```text
core_unit_tests=4
shell_lib_unit_tests=5
riscv_binary_build=passed
riscv_debug_elf_size_bytes=3360424
riscv_release_elf_size_bytes=187768
shell_vectors_total=59
parse_ok=54
parse_rejected=5
spawn_word_roundtrip=58
spawn_io_rejects=1
accepted=24
rejected=35
matched_expected=59
all_expected_matched=true
staged_release_elf_sha256=d0d1c14c811728c680d8646283cf7961dd850eebea856ac0e281fb493c4bc58d
child_vm_executed=true
child_vm_matched_expected=59
child_vm_max_cycles=3487024
```

The staged release ELF at `target/novaseal-btc-verifier-riscv-shell-release.elf` is now checked against the current release build by `scripts/novaseal_riscv_shell_artifact.py`. The preflight also confirms that the generated CellScript audit surface exposes the intended lock spawn/pipe/wait records.
The same staged ELF is executed by `verifier/novaseal_ckb_vm_harness` with harness-provided official VM2 inherited-fd, pipe-read, and close syscalls.

## Unsafe Boundary

The binary target uses Rust 2024's `#[unsafe(no_mangle)]` on `_start` so the bare-metal entry symbol is exported. On `riscv64`, it also uses small inline `ecall` wrappers for the CKB VM v2 inherited-fd, pipe-read, close, and process-exit boundaries.

There is still no raw pointer dereference, transmute, mutable static, or C FFI memory access in this slice. The unsafe boundary is syscall register ABI only.

## Current Limits

This still does not prove criterion 6 on chain:

- the binary reads inherited fd index `0`, and the child-verifier CKB VM harness now executes that child path,
- `nova_btc_authority_lock.cell` now spawns this binary through `spawn_with_fd`, and the parent-lock CKB VM harness executes that parent/child path,
- the current CellScript VM2 `spawn_with_fd` helper emits executable `ecall`, but only first-CellDep `code` target binding is modelled (see `docs/SPAWN_BACKEND_BLOCKER.md`),
- the generated `btc_authority` lock surface covers Script.args binding and spawn/IPC shell wiring, while the crypto decision is evidenced by verifier vectors, RISC-V build/tests, child-verifier CKB VM execution, parent-lock CKB VM execution, official resolved lock-group execution, and official full transaction script-verifier execution,
- the parent-lock harness now records transaction-shape occupied-capacity, transaction-size, under-capacity checks, resolved lock-group cycles, and full transaction script-verifier cycles, but this is not production builder/full-node acceptance.

The next shell slice should extend the full transaction script-verifier layer into production builder/full-node evidence while preserving strict-mode honesty.
