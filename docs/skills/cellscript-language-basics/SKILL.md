---
name: cellscript-language-basics
description: CellScript action, transition, resource, capability, finite quantifier, bounded collection, validity, borrow, and fail-closed basics.
references:
  - docs/wiki/Tutorial-02-Language-Basics.md
  - docs/wiki/Tutorial-03-Resources-and-Cell-Effects.md
  - docs/wiki/Tutorial-09-Action-Model-and-Canonical-Syntax.md
  - docs/wiki/Tutorial-11-Scoped-Invariants-and-ProofPlan.md
  - examples/token.cell
commands:
  - cellc check
  - cellc explain
---

# CellScript Language Basics

Use this skill when writing or reviewing CellScript source. Prefer the action
model already documented in the repository: visible actions, explicit
transitions, resource/shared/receipt declarations, finite source views,
bounded collection ownership, validity predicates, borrow regions, and
verification blocks.

Do not introduce actor-style terminology or hidden state transitions. If a
surface is reserved, deferred, metadata-only, runtime-required, or fail-closed,
say so directly and point to current compiler evidence.

Validation defaults:

- run `cellc check --json` for source feedback;
- use `cellc explain <CODE>` for diagnostic recovery;
- inspect bundled examples before inventing syntax.
