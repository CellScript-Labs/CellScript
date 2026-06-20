# CellScript 0.20 Release Notes

**Status**: In progress for the 0.20 nightly line.

**Updated**: 2026-06-19.

CellScript 0.20 hardens the generated-builder and live/devnet acceptance path.
The important post-audit change is that CKB-facing acceptance now checks the
ELF loader/ABI boundary before treating a local devnet result as release
evidence.

## ELF Entry ABI Gate

The CKB/devnet acceptance script now emits and validates a
`ckb_elf_entry_abi_gate` section for every compiled CKB RISC-V ELF artefact.
The gate fails closed unless:

- the executable `PT_LOAD` segment is readable and executable only;
- the executable segment has `filesz == memsz`, so the ELF does not fake stack
  memory;
- the entry trampoline starts with the expected call sequence into the real
  entry point;
- the trampoline preserves the CKB VM-provided `sp` stack pointer instead of
  initialising a private stack address.

This gate is required before local-node dry-run, tx-pool acceptance, submitted
stateful flows, and production evidence validation.

## Critical Example Coverage

The 0.20 devnet acceptance path explicitly keeps launch.cell, token.cell, and
amm_pool.cell in the ABI gate. These examples are the builder-facing bootstrap
path for token launch and AMM flows, so their reliability now depends on both
business-flow evidence and the lower-level ELF entry ABI evidence.

The existing local CKB acceptance still covers:

- strict original bundled example compilation;
- builder-backed action transactions;
- valid and invalid lock-spend checks;
- measured cycles;
- consensus-serialized transaction size;
- occupied-capacity checks;
- stateful lifecycle scenarios, including launch-to-mint and AMM
  seed/add/swap/remove flows.

## Validation Commands

For 0.20 release readiness, run:

```bash
./scripts/ckb_cellscript_acceptance.sh --production --stateful-scenarios
python3 scripts/validate_ckb_cellscript_production_evidence.py <report.json>
```

For a bounded local preflight without a CKB node:

```bash
./scripts/ckb_cellscript_acceptance.sh --compile-only --production
```

Compile-only evidence is useful for checking the ABI and compiler boundary, but
it is not sufficient for external release because it skips the local devnet
dry-run, tx-pool, commit, and live/dead lineage checks.
