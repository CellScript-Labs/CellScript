---
name: cellscript-diagnostics
description: Parser, type, lowering, runtime, and production-gate diagnostics with migration hints and safe next actions.
references:
  - docs/wiki/Tutorial-13-Agentic-Loops-and-cellscript-mcp.md
  - docs/wiki/Tutorial-07-LSP-and-Tooling.md
  - docs/wiki/Tutorial-06-Metadata-Verification-and-Production-Gates.md
commands:
  - cellc check
  - cellc explain
---

# CellScript Diagnostics

Use this skill when an agent or developer needs to recover from compiler
diagnostics. Prefer stable diagnostic codes and JSON fields over parsing human
messages. Preserve source spans, severity, code, and rendered text when
reporting errors back to tools.

Do not hide fail-closed behaviour. If the compiler rejects reserved syntax,
unsupported runtime semantics, metadata-only production claims, or stale
deployment evidence, keep the rejection visible and suggest the smallest safe
next check.

Validation defaults:

- run `cellc check --message-format=json`;
- run `cellc explain <CODE>`;
- keep write-producing commands behind explicit confirmation.
