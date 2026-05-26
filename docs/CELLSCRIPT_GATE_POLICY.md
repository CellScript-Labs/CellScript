# CellScript Gate Policy

CellScript uses one top-level gate entry point:

```bash
./scripts/cellscript_gate.sh <dev|ci|backend|release>
```

The lower-level audit scripts remain available for focused debugging, but they
are implementation details of the gate policy. Prefer the unified gate when
deciding whether a change is ready.

## Gate Modes

| Mode | When to run | Evidence boundary |
|---|---|---|
| `dev` | Local development before pushing | Formatting, Rust check, strict backend quick audit, syntax-combination quick audit, whitespace diff check |
| `ci` | Pull requests, pushes, and routine merge readiness | Full Rust tests, clippy, strict backend CI audit, syntax-combination CI audit through the strict backend runner, package verification, script syntax checks |
| `backend` | Changes touching IR, codegen, assembler, ABI, ELF, or RISC-V behavior | Full Rust tests, clippy, and strict backend full audit, including stateful CKB scenarios |
| `release` | Nightly/stable release candidates and any production CKB claim | `ci` plus tooling/docs boundary checks, VS Code validation, builder-backed CKB production acceptance, and stateful scenario/action coverage |

`release-quick` is an internal compatibility mode used by
`scripts/cellscript_ckb_release_gate.sh quick`; it runs the unified CI gate plus
compile-only production acceptance.

## Command Cheatsheet

```bash
# Local fast path
./scripts/cellscript_gate.sh dev

# Default CI/PR gate
./scripts/cellscript_gate.sh ci

# Strict compiler-contract gate for backend work
./scripts/cellscript_gate.sh backend

# Release-facing CKB production gate
./scripts/cellscript_gate.sh release
```

The old release wrapper remains supported:

```bash
./scripts/cellscript_ckb_release_gate.sh quick  # delegates to cellscript_gate.sh release-quick
./scripts/cellscript_ckb_release_gate.sh full   # delegates to cellscript_gate.sh release
```

## Lower-Level Components

Use these only when you need a focused failure:

```bash
./scripts/cellscript_syntax_combo_audit.sh quick
./scripts/cellscript_syntax_combo_audit.sh ci
./scripts/cellscript_strict_backend_audit.sh quick
./scripts/cellscript_strict_backend_audit.sh ci
./scripts/cellscript_strict_backend_audit.sh full
./scripts/ckb_cellscript_acceptance.sh --production --stateful-scenarios
./scripts/cellscript_0_14_scope_audit.sh
```

Passing one component does not imply the corresponding higher-level gate passed.
For example, CKB acceptance proves selected transaction behavior, while the
syntax-combination and strict backend audits prove compiler-layer edge cases and
structural invariants.

## Artifact Reports

The gates write machine-readable reports under `target/`:

- `target/syntax-combo-audit/`
- `target/cellscript-strict-backend-audit/`
- `target/ckb-cellscript-acceptance/`
- `target/cellscript-backend-shape/`
- `target/cellscript-schema-manifest/`

For release evidence, keep the JSON report paths in the release checklist rather
than copying long logs into review threads.
