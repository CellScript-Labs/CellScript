---
name: cellscript-language-basics
description: CellScript action, transition, resource, shared, receipt, capability, flow, and fail-closed basics.
references:
  - docs/wiki/Tutorial-02-Language-Basics.md
  - docs/wiki/Tutorial-03-Resources-and-Cell-Effects.md
  - docs/wiki/Tutorial-09-Action-Model-and-0-13-Syntax.md
  - examples/token.cell
commands:
  - cellc check
  - cellc explain
---

# CellScript Language Basics

Use this skill when writing or reviewing CellScript source. Prefer the action
model already documented in the repository: visible actions, explicit
transitions, resource/shared/receipt declarations, and verification blocks.

Do not introduce actor-style terminology or hidden state transitions. If a
surface is reserved, deferred, metadata-only, runtime-required, or fail-closed,
say so directly and point to current compiler evidence.

Validation defaults:

- run `cellc check --message-format=json` for source feedback;
- use `cellc explain <CODE>` for diagnostic recovery;
- inspect bundled examples before inventing syntax.
