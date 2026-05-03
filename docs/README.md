# CellScript Documentation Map

This directory is organized by document role. Keep new docs in the smallest
stable category that matches how readers should use them.

## Stable Tutorials

`docs/wiki/` contains the GitHub Wiki source. These pages are version-neutral,
reader-facing tutorials and cookbook material. They should teach the current
stable surface rather than act as release logs.

## Release Notes

`docs/releases/` contains finalized release notes.

- `docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md` is the final 0.13.2
  release note and the canonical 0.13 release summary.

Release candidates and planning notes should not live here unless they are the
final release record.

## Reference And Evidence Contracts

Top-level `docs/CELLSCRIPT_*.md` files are active reference material when they
describe current compiler behavior, target-profile evidence, runtime errors,
syntax governance, metadata, capacity, deployment manifests, or support
matrices.

High-value active references include:

- `CELLSCRIPT_SYNTAX_GOVERNANCE.md`
- `CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md`
- `CELLSCRIPT_CKB_LANGUAGE_AUDIT.md`
- `CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md`
- `CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md`
- `CELLSCRIPT_RUNTIME_ERROR_CODES.md`

## Examples

`docs/examples/` contains focused example notes and matrices that support the
bundled `.cell` examples. These are not release notes.

## Roadmap

`roadmap/` is outside this directory and contains planning state. Roadmap files
may point to release notes and active reference docs, but they should not
duplicate full release notes.

## Archive

`docs/archive/` contains historical plans and superseded execution documents.
Archived files may remain useful for design archaeology, but they are not the
current stable contract.

Current archive:

- `docs/archive/0.13/CELLSCRIPT_0_13_1_PLAN.md`
- `docs/archive/0.13/CELLSCRIPT_SIGNATURE_DIRECTION_EXECUTION_PLAN.md`

When moving a document into the archive, update all public links and add a short
status note if the file could otherwise be mistaken for active guidance.
