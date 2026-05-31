# NovaSeal v0 Spawn Backend Blocker

**Date**: 2026-05-31
**Report**: `target/novaseal-spawn-backend-probe.json`
**Status**: source-level verifier calls, spawn/pipe lowering, and VM2 syscall helpers now emit executable `ecall` wrappers. The compiler now has a strict manifest-bound `CellDep#0`/`code` model for static spawn targets, including structured transaction/evidence checks for the first CellDep name, `dep_type`, and any manifest-pinned out-point/hash fields. The generic `verifier::btc::bip340::require_signature(...)` helper lowers to the fixed 18-word IPC envelope, a one-fd `spawn_with_fd` call, checked envelope emission, checked child exit status, and the `cellscript_btc_bip340_verifier_riscv` spawn target. Parent/child CKB VM, official resolved lock-group verifier evidence, official full transaction script-verifier evidence, materialized `ProofReceiptV0` output evidence, combined eight-fixture transaction verifier evidence, and live local devnet RPC evidence now exist. The remaining production limit is public/shared CellDep deployment pinning plus Molecule/wallet signing alignment, not the compiler spawn backend.

This is no longer the parent/child VM execution blocker. A source-level verifier spawn without a matching first `Cell.toml [[deploy.ckb.cell_deps]]` entry with `dep_type = "code"` still strict-fails; the current manifest binding makes the target builder-required, and BTC authorisation now has local CKB node-verification-stack evidence for all eight combined fixtures.

## Command

Run from the package root, using the local compiler build:

```bash
python3 scripts/novaseal_spawn_backend_probe.py --cellc /home/arthur/a19q3/CellScript/target/debug/cellc --pretty
```

Current summary:

```text
compile_passed=true
all_spawn_ipc_calls_lowered=true
backend_ecall_boundary_closed=true
generic_btc_bip340_helper_lowered=true
spawn_with_fd_helper_executable=true
spawn_with_fd_helper_fail_closed_stub=false
spawn_with_fd_helper_uses_static_cell_dep0_with_one_inherited_fd=true
fixed_word_envelope_lowered=true
strict_rejects_spawn_target=true
manifest_bound_spawn_target_strict_passes=true
manifest_bound_spawn_target_builder_required=true
combined_ckb_node_verification_stack_verified=true
```

## Exact Meaning

- A protocol-agnostic probe action using `verifier::btc::bip340::require_signature(message, pubkey, signature)` compiles.
- The generated assembly contains calls to the expected helper symbols for the fixed-word one-fd verifier lowering.
- The generated `__ckb_spawn_with_fd1` helper emits syscall `2601` through `ecall`.
- The generated `__ckb_pipe` helper keeps read/write fds in `a0`/`a1` and moves raw status into `a2`, so callers still fail closed without clobbering either fd.
- The generated `__ckb_spawn_with_fd1` helper currently resolves the static target to `CellDep#0` with no argv and an inherited-fd list `[fd, 0]`.
- The fixed-word lowering remains protocol-agnostic: every verifier payload word is formed from fixed bytes and a static word index, not from NovaSeal-specific field recognition.
- Strict 0.16 rejects the source-only probe with `PP0150 action:probe:spawn-target:CellDep#0@0x...`.
- The same generic BIP340 verifier probe passes strict 0.16 when packaged with a matching first `deploy.ckb.cell_deps` `code` entry for `cellscript_btc_bip340_verifier_riscv`; the generated audit-bundle marks the spawn target as `builder-required`, and `validate-tx` requires both transaction `cell_deps[0]` and builder evidence to identify `CellDep#0`, the manifest name, `dep_type = "code"`, and any manifest-pinned out-point/hash fields. Later CellDep positions and dep groups remain strict-failing until codegen can actually select them.

## Consequence for NovaSeal

The next implementation slice should not widen the lock protocol again. The lock and state package surface now call `verifier::btc::bip340::require_signature(...)`; the compiler lowers that generic helper to `spawn_with_fd`, the fixed 18-word IPC envelope, and the manifest-bound `cellscript_btc_bip340_verifier_riscv` target. The next risks are strict generated output/resource ProofPlan closure, public/shared deployment pinning, and Molecule/wallet signing alignment, not another NovaSeal-specific verifier namespace.

The correct order is:

1. Keep unmanifested spawn targets strict-failing and keep manifest-bound targets builder-required until builder evidence is supplied.
2. Preserve the passing parent/child CKB VM, official resolved lock-group evidence, and official full transaction script-verifier evidence.
3. Preserve the combined harness measurements for cycles, occupied capacity, transaction size, under-capacity rejection, and local CKB contextual verifier acceptance.
4. Preserve the live local devnet RPC runner, then add public/shared deployment pinning only after the signing/Molecule preimage is frozen.

It is tempting to write the pretty lock code first. Tempting, and exactly the sort of thing that later requires a very expensive apology.
