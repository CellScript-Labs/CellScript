# CellScript 0.16.2 Release Notes

**Status**: Released as `v0.16.2`.

**Release date**: 2026-06-21.

**Release tag**: `v0.16.2`.

**Updated**: 2026-06-21.

CellScript 0.16.2 is a builder-ergonomics patch for CKB resource identity
handoff. It keeps the 0.16 compiler/runtime scope, while making external
transaction builders less dependent on copied harness conventions.

## Highlights

- `cellc resource-identity` emits a compiler-owned passive resource identity
  artifact and JSON plan for resource output type scripts.
- `cellc validate-tx --resource-identities` checks created resource outputs
  against the generated passive identity plan.
- `cellc validate-tx --production` rejects known fixture-only resource
  identities, including devnet `always_success` and all-zero placeholders, when
  they appear as real resource output type scripts.
- `cellc explain-assumptions` and `cellc solve-tx` can be scoped with
  `--entry-action` or `--entry-lock`, so external builders can consume the
  selected entrypoint contract instead of whole-module noise.
- Builder-facing contract commands now emit JSON by default; `--json` remains
  accepted for compatibility, and `--human` prints a short terminal summary.
- `cellc builder manifest` and `cellc builder check` are canonical namespace
  forms; `builder-manifest` and `builder-check` remain aliases.
- `cellc entry-witness` now exposes raw script-group witness placement in its
  default JSON output.
- `cellc solve-tx` now exposes `submit_ready: false`, missing builder
  steps, structural builder-evidence requirements, a fillable evidence
  template, and a fixture identity policy.
- `cellc builder manifest` and `cellc builder check` provide a two-step
  builder-facing workflow over the lower-level ABI, constraints, witness,
  assumption, resource identity, and validation commands.
- Builder docs clarify that scoped action artifacts are active verifiers and
  must not be used as passive `MintAuthority`, `Token`, `Pool`, or `LPReceipt`
  resource identities.

## Validation

The release patch was validated with the v0.16 CLI/backend suite:

```bash
cargo test --test v0_16 -- --nocapture
```

The suite covers resource identity plan generation and validation, scoped
assumption/solver output, wildcard structural evidence requirements, entry
witness placement, builder manifest/check flow, active artifact misuse
rejection, and production fixture-identity rejection.

The wiki and builder-facing examples were audited so contract commands use the
default JSON output and the canonical `cellc builder manifest` /
`cellc builder check` namespace. The old dashed forms and `--json` flag remain
accepted for compatibility.

The builder UX was also smoke-tested against
`WuodOdhis/cellscript-swap-builder` `main` at
`479feb004338524d367b6656c6fb356ca7918f28`: the external Rust builder accepted
compiler-generated entry witness bytes and passive `Token:token_out` resource
identity, `cellc builder check --production --primitive-strict 0.16` passed the
positive transaction shape, and negative checks rejected both scoped action
artifact and `always_success` fixture identities for the created token output.
