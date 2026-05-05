# CellScript Documentation Map

This directory is organized by document role. Keep new docs in the smallest
stable category that matches how readers should use them.

## Stable Tutorials

`docs/wiki/` contains the GitHub Wiki source. These pages are version-neutral,
reader-facing tutorials and cookbook material. They should teach the current
stable surface rather than act as release logs.

## Release Notes

`docs/releases/` contains finalized release notes and active release-note
drafts.

- `docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md` is the final 0.13.2
  release note and the canonical 0.13 release summary.
- `docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md` records the closed 0.13
  implementation scope and release evidence boundary.
- `docs/releases/CELLSCRIPT_0_13_2_ACCEPTANCE_COMMUNITY_POST.md` is a
  community-facing summary of the 0.13.2 CKB acceptance and stateful flow
  evidence.
- `docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES_DRAFT.md` tracks the active
  0.14 nightly release-note draft.

Release candidates and planning notes should not live here unless they are the
final release record.

## Reference And Evidence Contracts

Top-level `docs/CELLSCRIPT_*.md` files are active reference material when they
describe current compiler behavior, target-profile evidence, runtime errors,
syntax governance, metadata, capacity, deployment manifests, or support
matrices.

High-value active references include:

- `releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md` for the final 0.13 syntax
  governance summary
- `CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md`
- `CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md` for the active grammar-governance
  direction around transition shape, `verification`, `require`, and accounting
  syntax
- `CELLSCRIPT_SURFACE_ELEGANCE_RFC.md` for deferred syntax candidates that
  require full parser/typechecker/lowering/metadata/formatter/LSP coverage
- `CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md`
- `CELLSCRIPT_CKB_ADAPTER.md`
- `CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md`
- `CELLSCRIPT_RUNTIME_ERROR_CODES.md`

## Examples

`docs/examples/` contains focused example notes and matrices that support the
bundled `.cell` examples. These are not release notes.

## Roadmap

`roadmap/` is outside this directory and contains planning state. Roadmap files
may point to release notes and active reference docs, but they should not
duplicate full release notes.

Active later-stage roadmap notes that live under `docs/` because they are tied
to branch-specific evidence or forward design:

- `0.17/CELLSCRIPT_0_17_ROADMAP.md`
- `CELLSCRIPT_0_18_ROADMAP.md`
- `CELLSCRIPT_0_19_ROADMAP.md`
- `CELLSCRIPT_CKB_ECOSYSTEM_REUSE_AUDIT.md` for 0.19 CKB ecosystem reuse scope
- `CELLSCRIPT_CKB_STD_COMPAT.md` for 0.19 ckb-std compatibility scope
- `CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md` and
  `CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md` for 0.19 grammar/syntax
  governance scope

## Archive

`docs/archive/` contains historical plans and superseded execution documents.
Archived files may remain useful for design archaeology, but they are not the
current stable contract.

Current archive:

- `docs/archive/0.13/CELLSCRIPT_0_13_1_PLAN.md`
- `docs/archive/0.13/CELLSCRIPT_SIGNATURE_DIRECTION_EXECUTION_PLAN.md`

When moving a document into the archive, update all public links and add a short
status note if the file could otherwise be mistaken for active guidance.
