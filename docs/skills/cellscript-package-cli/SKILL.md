---
name: cellscript-package-cli
description: CellScript package layout, Cell.toml, build/check/fmt/test, canonical command groups, global JSON output, registry/package verification, and legacy alias migration.
references:
  - docs/wiki/Tutorial-04-Packages-and-CLI-Workflow.md
  - roadmap/CELLSCRIPT_0_21_CLI_UX_PLAN.md
  - docs/CELLSCRIPT_GATE_POLICY.md
commands:
  - cellc check
  - cellc build
  - cellc fmt
  - cellc test
  - cellc package verify
  - cellc registry verify
---

# CellScript Package And CLI

Use this skill when working with packages or command-line workflows. Prefer the
current nested command tree. Legacy flat aliases may exist during the
compatibility window, but public docs and agent guidance should teach the
canonical form.

Use global `--json` for one machine-readable stdout result on success or
failure. Do not scrape coloured human text when structured output exists.

Validation defaults:

- run `cellc check --json` for package feedback;
- run `cellc --list` to inspect the canonical command tree;
- run `./scripts/cellscript_gate.sh dev` before claiming local readiness.
