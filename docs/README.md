# CellScript Documentation Map

This directory is organized by document role. Keep new docs in the smallest
stable category that matches how readers should use them.

## Stable Tutorials

`docs/wiki/` contains the GitHub Wiki source. These pages are version-neutral,
reader-facing tutorials and cookbook material. They should teach the current
stable surface rather than act as release logs.

## Release Records

`docs/releases/` contains finalized release notes and active release-note
drafts. Released versions should use non-draft filenames.

- `docs/releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md` is the final 0.13.2
  release note and the canonical 0.13 release summary.
- `docs/releases/CELLSCRIPT_0_13_RELEASE_SCOPE.md` records the closed 0.13
  implementation scope and release evidence boundary.
- `docs/releases/CELLSCRIPT_0_13_2_ACCEPTANCE_COMMUNITY_POST.md` is a
  community-facing summary of the 0.13.2 CKB acceptance and stateful flow
  evidence.
- `docs/releases/CELLSCRIPT_0_14_RELEASE_NOTES.md` is the final 0.14.0 release
  note and release-evidence summary.
- `docs/releases/CELLSCRIPT_0_15_RELEASE_NOTES.md` is the final 0.15.0 release
  note and release-evidence summary.
- `docs/releases/CELLSCRIPT_0_16_RELEASE_NOTES_DRAFT.md` tracks the active
  0.16 nightly release-note draft.

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
- `CELLSCRIPT_GATE_POLICY.md`
- `CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md`
- `CELLSCRIPT_CKB_LANGUAGE_AUDIT.md`
- `CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md`
- `CELLSCRIPT_CKB_DEPLOYMENT_MANIFEST.md`
- `CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md`
- `CELLSCRIPT_DEPENDENCY_PACKAGE_CACHE_AUDIT.md`
- `CELLSCRIPT_ENTRY_WITNESS_ABI.md`
- `CELLSCRIPT_EXAMPLE_BUSINESS_FLOWS.md`
- `CELLSCRIPT_LINEAR_OWNERSHIP.md`
- `CELLSCRIPT_METADATA_SYSTEM_AUDIT.md`
- `CELLSCRIPT_OUTPUT_BINDINGS.md`
- `CELLSCRIPT_RUNTIME_ERROR_CODES.md`
- `CELLSCRIPT_SCHEDULER_HINTS.md`

## Specs And Future Tracks

- `docs/spec/` contains normative or semi-normative specifications. The 0.16
  operational semantics live there.
- `docs/0.17/` contains next-release planning and iCKB investigation material.
  It is not part of the 0.16 release contract.

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

## Archive

`docs/archive/` contains historical plans and superseded execution documents.
Archived files may remain useful for design archaeology, but they are not the
current stable contract.

Current archive:

- `docs/archive/0.13/CELLSCRIPT_0_13_1_PLAN.md`
- `docs/archive/0.13/CELLSCRIPT_SIGNATURE_DIRECTION_EXECUTION_PLAN.md`
- `docs/archive/0.15/CELLSCRIPT_0_15_PRE_RELEASE_AUDIT_AND_HARDENING.md`
- `docs/archive/0.15/CELLSCRIPT_0_15_ROADMAP_SUMMARY.md`

When moving a document into the archive, update all public links and add a short
status note if the file could otherwise be mistaken for active guidance.
