# NovaSeal v0 RISC-V Shell Artifact Preflight

**Date**: 2026-05-31
**Report**: `target/novaseal-riscv-shell-artifact.json`
**Staged ELF**: `target/novaseal-btc-verifier-riscv-shell-release.elf`
**Status**: staged release ELF is synced to the current release build; BIP340 vector-matching shell; child-verifier and parent-lock CKB VM harnesses exist; parent-lock transaction-shape measurement, official resolved lock-group verifier evidence, and official full transaction script-verifier evidence exist; no public/shared deployment pinning evidence yet.

This document records the exact verifier shell artifact that the current lock wiring targets. It does not itself claim CKB VM execution or production readiness; child-verifier VM evidence is recorded separately in `docs/CKB_VM_CHILD_VERIFIER.md`, and parent-lock VM evidence is recorded in `docs/PARENT_LOCK_CKB_VM_HARNESS.md`.

## Command

Run from the package root:

```bash
python3 scripts/novaseal_riscv_shell_artifact.py --sync --pretty
```

Current summary:

```text
preflight_passed=true
staged_matches_release=true
staged_release_elf_size_bytes=187768
staged_release_elf_sha256=d0d1c14c811728c680d8646283cf7961dd850eebea856ac0e281fb493c4bc58d
generated_spawn_visible=true
lock_wiring_status=wired_to_bip340_shell
ready_for_ckb_vm_dry_run=true
production_ready=false
```

## What This Proves

- The staged `target/` ELF is byte-for-byte equal to the current release RISC-V shell build.
- The `.sha256` sidecar matches the staged ELF.
- The shell report matches all 77 IPC vectors: 24 accepts, 35 rejects.
- The shell input contract remains inherited fd index `0`, 18 little-endian `u64` words, 144-byte IPC envelope, implemented over the official VM2 buffer/length syscalls.
- The generated CellScript audit surface exposes lock spawn/pipe/wait records and the manifest-bound spawn target.

## What This Does Not Prove

- This preflight does not execute the ELF; the child-verifier and parent-lock CKB VM harnesses do that separately.
- The parent-lock CKB VM harness now spawns this ELF, observes child status, records transaction-shape tx-size/capacity facts, and runs the official resolved lock-group verifier plus full transaction script verifier for the three parent authority cases; no public/shared deployment pinning path has executed it.
- The `.cell` lock constructs and sends the 18-word IPC envelope to this BIP340 shell in the parent-lock harness.
- The staged ELF is not production-ready until public/shared deployment pinning evidence exists. Eight-fixture transaction coverage exists locally.

The value of this preflight is simple: every VM or transaction run can point at a pinned artifact and a mechanical guard against stale ELF evidence. A small thing, but very much the sort of small thing that saves one from explaining oneself to auditors over cold coffee.
