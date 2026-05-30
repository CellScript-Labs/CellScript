# NovaSeal v0 Spawn Backend Blocker

**Date**: 2026-05-30
**Report**: `target/novaseal-spawn-backend-probe.json`
**Status**: source-level spawn/pipe lowers and the VM2 syscall helpers now emit executable `ecall` wrappers. The compiler now has a strict manifest-bound `CellDep#0`/`code` model for static spawn targets, including structured transaction/evidence checks for the first CellDep name, `dep_type`, and any manifest-pinned out-point/hash fields. The protocol-agnostic `spawn_with_fd(target, fd)` helper builds a one-entry, zero-terminated inherited-fd list for VM2 `SpawnArgs`, and `fixed_u64_le(bytes, word_index)` lowers fixed Hash/Address/[u8; N] values into 8-byte little-endian words. NovaSeal lock wiring now calls this generic fixed-word envelope shape and targets the RISC-V BIP340 shell; parent/child CKB VM, official resolved lock-group verifier evidence, and official full transaction script-verifier evidence now exist. Production remains blocked on builder/full-node acceptance and six-fixture transaction coverage.

This is no longer the parent/child VM execution blocker. A source-level verifier spawn without a matching first `Cell.toml [[deploy.ckb.cell_deps]]` entry with `dep_type = "code"` still strict-fails; the current manifest binding makes the target builder-required, and BTC authorisation remains non-production until production builder/full-node evidence exists.

## Command

Run from the package root, using the local compiler build:

```bash
python3 scripts/novaseal_spawn_backend_probe.py --cellc /Users/arthur/RustroverProjects/CellScript/target/debug/cellc --pretty
```

Current summary:

```text
compile_passed=true
all_spawn_ipc_calls_lowered=true
backend_ecall_boundary_closed=true
spawn_with_fd_helper_executable=true
spawn_with_fd_helper_fail_closed_stub=false
spawn_with_fd_helper_uses_static_cell_dep0_with_one_inherited_fd=true
fixed_word_envelope_lowered=true
strict_rejects_spawn_target=true
manifest_bound_spawn_target_strict_passes=true
manifest_bound_spawn_target_builder_required=true
```

## Exact Meaning

- A protocol-agnostic probe action using `pipe`, 18 `pipe_write` calls, 16 `fixed_u64_le` word extractions, `spawn_with_fd`, `wait`, and `close` compiles.
- The generated assembly contains calls to the expected helper symbols for the fixed-word one-fd probe.
- The generated `__ckb_spawn_with_fd1` helper emits syscall `2601` through `ecall`.
- The generated `__ckb_pipe` helper keeps read/write fds in `a0`/`a1` and moves raw status into `a2`, so callers still fail closed without clobbering either fd.
- The generated `__ckb_spawn_with_fd1` helper currently resolves the static target to `CellDep#0` with no argv and an inherited-fd list `[fd, 0]`.
- The fixed-word lowering remains protocol-agnostic: every verifier payload word is formed from fixed bytes and a static word index, not from NovaSeal-specific field recognition.
- Strict 0.16 rejects the source-only probe with `PP0150 action:probe:spawn-target:CellDep#0@0x...`.
- The same protocol-agnostic probe passes strict 0.16 when packaged with a matching first `deploy.ckb.cell_deps` `code` entry for `novaseal_btc_verifier_riscv`; the generated audit-bundle marks the spawn target as `builder-required`, and `validate-tx` requires both transaction `cell_deps[0]` and builder evidence to identify `CellDep#0`, the manifest name, `dep_type = "code"`, and any manifest-pinned out-point/hash fields. Later CellDep positions and dep groups remain strict-failing until codegen can actually select them.

## Consequence for NovaSeal

The next implementation slice should not widen the lock protocol again. The lock now uses `spawn_with_fd("novaseal_btc_verifier_riscv", read_fd)` before writing the IPC envelope, matching official VM2 pipe scheduling. The next risk is production builder/full-node acceptance.

The correct order is:

1. Keep unmanifested spawn targets strict-failing and keep manifest-bound targets builder-required until builder evidence is supplied.
2. Preserve the passing parent/child CKB VM, official resolved lock-group evidence, and official full transaction script-verifier evidence.
3. Record production builder/full-node cycles, occupied capacity, transaction size, and under-capacity rejection.
4. Promote the current six-fixture transaction verifier harness into production builder/full-node acceptance evidence before discussing production hardening.

It is tempting to write the pretty lock code first. Tempting, and exactly the sort of thing that later requires a very expensive apology.
