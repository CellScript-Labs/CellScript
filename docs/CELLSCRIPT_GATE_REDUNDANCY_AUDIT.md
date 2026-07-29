# CellScript Gate Redundancy Audit

Status: 2026-07-04

This report audits redundant or overly repetitive work in the CellScript gate
stack. It covers the unified gate entry point, lower-level audit runners,
GitHub workflows, website build checks, and VS Code extension release checks.

## Summary

The audit did not find a safe reason to remove the core evidence gates:

- `cellscript_strict_backend_audit.sh`
- `cellscript_syntax_combo_audit.sh`
- `ckb_cellscript_acceptance.sh`

Those checks overlap in the broad sense that they all exercise compiler output,
but they prove different boundaries. The safe optimisations are in repeated
tooling invocations around the gates, not in the compiler, syntax, or CKB
acceptance coverage itself.

## Fixed Redundancy

| Area | Previous behaviour | Updated behaviour | Risk |
| --- | --- | --- | --- |
| Release auxiliary checks | `release` and `release-quick` run `run_ci_gate`, then repeated `check_cellscript_skill_pack.py`, `check_script_syntax`, and `check_trailing_whitespace` inside `run_release_auxiliary_checks`. | Release modes now inherit those checks from the embedded CI gate and keep release auxiliary checks focused on release-only docs, CKB, NovaSeal, and VS Code evidence. | Low. The checks still run before release-only checks. |
| Website build in the unified gate | `run_website_build_check` ran `npm --prefix website run prepare:registry`, checked generated data, then ran `npm --prefix website run build`; the `build` script ran `prepare:registry` again. | The gate still prepares and checks registry data once, then directly runs `astro check` and `astro build` from `website/`. | Low. The same Astro checks and build still run. |
| Website build workflow | `.github/workflows/website-build.yml` ran automatically on PRs and pushes, duplicating the website build already covered by the unified CI gate. It also ran `npm --prefix website run build`, which generated registry data again. | The workflow is now manual-only via `workflow_dispatch`, keeping the `website/dist` artifact path available on demand. It also generates and checks registry data once, then directly runs `astro check` and `astro build`. | Low. Automatic merge-readiness coverage remains in the unified CI gate. |
| VS Code release path | Release auxiliary checks ran `npm run validate`, which built the extension, then `npm run publish:dry-run`, which explicitly built again and then let `vsce package` run `vscode:prepublish`, building again. | The gate directly runs `vsce package --no-dependencies`, letting `vsce` perform the one required prepublish build, then runs `node scripts/validate.mjs` directly against the built output. | Low. The VSIX dry-run and manifest validation still run. |

The release tooling validator was updated to enforce the new direct-call
contract so this optimisation does not drift silently.

## Intentional Overlap Kept

### Strict Backend Audit After `cargo test`

`ci` and `backend` run broad Rust tests before invoking the strict backend
audit. The strict backend audit then re-runs selected filtered tests to produce
feature-level evidence in `target/cellscript-strict-backend-audit/`.

This is execution overlap, but not redundant evidence. Removing the filtered
audit runs would require a new report model that can derive the same feature
coverage from the broad `cargo test` run.

### Syntax-Combination Audit And CKB Acceptance

The syntax-combination audit covers parser, formatter, type checking, lowering,
metadata, codegen, and negative syntax oracles. CKB acceptance covers concrete
builder-backed transaction behaviour, dry-run evidence, capacity, cycles, and
production hardening. A direct CKB acceptance run does not replace the syntax
preflight.

### `git diff --check` And Full Trailing-Whitespace Checks

`git diff --check` catches whitespace errors in the current diff. The explicit
trailing-whitespace check scans a curated tracked-file set. They are related,
but not equivalent.

### `cargo package --list` And `cargo package`

`cargo package --list` supports the package contents audit. `cargo package`
then validates actual package construction. They should remain separate.

## Cross-Workflow Result

The PR/push path now has one automatic website build source: the unified CI
gate. The standalone website workflow remains available for manual artifact
generation only, so it no longer duplicates merge-readiness checks on every PR
or push.

## Validation

The updated paths were checked with:

```bash
bash -n scripts/cellscript_gate.sh
python3 scripts/validate_cellscript_tooling_release.py
git diff --check
npm --prefix website run prepare:registry
(cd website && npm exec -- astro check && npm exec -- astro build)
(cd editors/vscode-cellscript && npm exec -- vsce package --no-dependencies --out /tmp/cellscript-vscode-dry-run.vsix)
node editors/vscode-cellscript/scripts/validate.mjs
```

Observed website diagnostics were non-fatal existing hints in
`website/public/wasm/cellscript_wasm.js` for unused generated bindings. The
Astro check and build completed successfully.
