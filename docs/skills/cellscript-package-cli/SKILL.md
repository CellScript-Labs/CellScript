---
name: cellscript-package-cli
description: CellScript package layout, Cell.toml, build/check/fmt/test, canonical 0.21 command groups, registry/package verification, and legacy alias migration.
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
canonical 0.21 nested command tree. Legacy flat aliases may exist during the
compatibility window, but public docs and agent guidance should teach the
canonical form.

Keep stdout and stderr boundaries intact. `--json` is successful payload output;
`--message-format=json` is diagnostic transport. Do not scrape coloured human
text when structured output exists.

Validation defaults:

- run `cellc check --message-format=json` for package feedback;
- run `cellc --list` to inspect the canonical command tree;
- run `./scripts/cellscript_gate.sh dev` before claiming local readiness.
