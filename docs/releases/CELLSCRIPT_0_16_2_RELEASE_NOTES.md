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
- `cellc solve-tx --json` now exposes structural builder-evidence requirements
  and a fixture identity policy.
- Builder docs clarify that scoped action artifacts are active verifiers and
  must not be used as passive `MintAuthority`, `Token`, `Pool`, or `LPReceipt`
  resource identities.

## Validation

The release patch was validated with the v0.16 CLI/backend suite:

```bash
cargo test --test v0_16 -- --nocapture
```

The suite covers resource identity plan generation and validation, scoped
assumption/solver output, wildcard structural evidence requirements, active
artifact misuse rejection, and production fixture-identity rejection.
