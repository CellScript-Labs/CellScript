# NovaSeal v0 CKB VM Child Verifier Harness

**Date**: 2026-05-30
**Harness**: `verifier/novaseal_ckb_vm_harness`
**Report**: `target/novaseal-ckb-vm-child-verifier-report.json`
**Classification**: child verifier CKB VM dry-run evidence.

This harness executes the staged `novaseal_btc_verifier_riscv` ELF in `ckb-vm`
0.24. It provides only the child-side VM2 syscalls needed by the shell, using the official `ckb-script` VM2 buffer/length ABI:

- `inherited_fd(buffer, length_ptr)` -> fixed harness fd list
- `pipe_read(fd, buffer, length_ptr)` -> the 18 little-endian `u64` IPC words
- `close(fd)` -> status in `a0`

It does **not** execute the parent CellScript lock, `spawn_with_fd`, `wait`, a
full CKB transaction, ScriptGroup loading, witnesses, cell deps, capacity, or
transaction-size checks.

## Command

Run from the package root:

```bash
cargo run --manifest-path verifier/novaseal_ckb_vm_harness/Cargo.toml --bin novaseal_ckb_vm_harness -- --pretty
```

Expected summary:

```text
child_vm_executed=true
parent_spawn_executed=false
total=59
accepted=24
rejected=35
matched_expected=59
mismatched=0
max_cycles=3487024
```

The staged ELF used by this run is:

```text
target/novaseal-btc-verifier-riscv-shell-release.elf
size_bytes=187768
sha256=d0d1c14c811728c680d8646283cf7961dd850eebea856ac0e281fb493c4bc58d
```

## Evidence Level

This is stronger than host vectors and stronger than a RISC-V build check:
the actual child ELF is loaded into CKB VM and its inherited-fd read path is
exercised against the frozen IPC vector set.

It proves:

- the staged child ELF loads in `ckb-vm`,
- child-side `inherited_fd`, `pipe_read`, and `close` calls follow the intended
  VM2 register convention,
- all 24 valid IPC vectors exit `0`,
- all 35 invalid/malformed IPC vectors exit non-zero,
- malformed truncated input becomes a spawn-input failure instead of accidental
  acceptance,
- cycle counts are collected for the child verifier path.

It does not prove:

- parent `btc_authority` execution,
- VM2 `spawn` syscall 2601,
- VM2 `wait` syscall 2602,
- parent-observed child exit status,
- transaction `cell_deps[0]` identity,
- witness / Script.args / ScriptGroup loading,
- occupied capacity or transaction size,
- builder-backed transaction acceptance.

## Closure Path

The child harness now remains a lower-level oracle under the parent-lock
harness. The remaining production path is builder/full-node acceptance and
six-fixture transaction coverage.
