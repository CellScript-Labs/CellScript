# NovaSeal Parent Lock ABI Preflight

**Date**: 2026-05-30
**Script**: `scripts/novaseal_parent_lock_abi_preflight.py`
**Report**: `target/novaseal-parent-lock-abi-preflight.json`
**Classification**: parent lock ELF/ASM ABI preflight.

This preflight builds the `btc_authority` parent lock as both RISC-V assembly and RISC-V ELF, then inspects the generated ABI surface that must be correct before parent/child CKB VM evidence can be meaningful.

## Current Result

Run:

```bash
python3 scripts/novaseal_parent_lock_abi_preflight.py --pretty
```

Current summary:

```text
preflight_passed=true
parent_lock_elf_built=true
ready_for_parent_child_ckb_vm_harness=true
parent_lock_ckb_vm_executed=false
```

## Checked Surface

The preflight currently requires:

- `LOAD_SCRIPT reason=entry_lock_args` is present.
- `expected_btc_authority_hash` consumes exactly 32 Script.args bytes.
- Script.args u32 decoding does not clobber its own base pointer.
- `expected_btc_authority_hash` is not rebound from `Input#N` or `CellDep#N` data.
- the protected `cell` remains bound from `Input#0` cell data.
- `spawn_with_fd`, VM2 spawn, wait, pipe/write, and close syscall surfaces remain visible.

## Boundary

This is not CKB VM transaction evidence. It proves the generated parent lock artifact is structurally ready. The parent-lock CKB VM harness now provides the execution layer and transaction-shape measurement in `docs/PARENT_LOCK_CKB_VM_HARNESS.md`; the remaining gap is resolved transaction execution with real ScriptGroup/cell_deps and fixture coverage.
