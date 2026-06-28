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
- `docs/releases/CELLSCRIPT_0_16_RELEASE_NOTES.md` is the initial 0.16.0 release
  note and release-evidence summary.
- `docs/releases/CELLSCRIPT_0_16_1_RELEASE_NOTES.md` is the final 0.16.1 release
  note and release-evidence summary.

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
- `CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md` for the active grammar-governance
  direction around transition shape, `verification`, `require`, and accounting
  syntax
- `CELLSCRIPT_SURFACE_ELEGANCE_RFC.md` for deferred syntax candidates that
  require full parser/typechecker/lowering/metadata/formatter/LSP coverage
- `CELLSCRIPT_CAPACITY_AND_BUILDER_CONTRACT.md`
- `CELLSCRIPT_CKB_ADAPTER.md`
- `CELLSCRIPT_CELLFABRIC_BRIDGE.md`
- `CELLSCRIPT_PACKAGE_PROVENANCE_AND_DEPLOYMENT_IDENTITY.md`
- `CELLSCRIPT_REGISTRY_PRODUCTION_BOUNDARY_ADR.md` for the accepted production
  boundary of the JoyID-rooted public registry write/read architecture
- `../services/registry-api/README.md` for the Cloudflare Workers + R2 + Neon
  write API implementation and deployment checklist
- `CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md`
- `CELLSCRIPT_ENTRY_WITNESS_ABI.md`
- `CELLSCRIPT_EXAMPLE_BUSINESS_FLOWS.md`
- `CELLSCRIPT_LINEAR_OWNERSHIP.md`
- `CELLSCRIPT_OUTPUT_BINDINGS.md`
- `CELLSCRIPT_RUNTIME_ERROR_CODES.md`
- `CELLSCRIPT_SCHEDULER_HINTS.md`

## Specs And Future Tracks

- `docs/spec/` contains normative or semi-normative specifications. The 0.16
  operational semantics live there.
- `docs/0.20/` now keeps only release-facing evidence material for the 0.20
  line, including `CELLSCRIPT_PROTOCOL_MULTI_FILE_EVIDENCE.md` for the
  evidence-gated NovaSeal/iCKB/DobEvo protocol-source boundary. Superseded
  0.20 audit notes have been removed from the main branch.

## Examples

`docs/examples/` contains focused example notes and matrices that support the
bundled `.cell` examples. These are not release notes.

- `docs/examples/token_amm_bootstrap.md` records the concrete token authority
  bootstrap and AMM builder path for the bundled `launch`, `token`, and
  `amm_pool` examples.

## Roadmap

`roadmap/` is outside this directory and contains planning state. Roadmap files
may point to release notes and active reference docs, but they should not
duplicate full release notes.

Active later-stage roadmap notes that live under `docs/` because they are tied
to branch-specific evidence or forward design:

- `archive/0.17/CELLSCRIPT_0_17_ROADMAP.md`
- `CELLSCRIPT_0_18_ROADMAP.md`
- `CELLSCRIPT_0_19_ROADMAP.md`
- `releases/CELLSCRIPT_0_16_TO_0_20_RELEASE_NOTES.md`
- `CELLSCRIPT_0_20_ROADMAP.md`
- `CELLSCRIPT_0_21_ROADMAP.md`
- `CELLSCRIPT_CKB_STD_COMPAT.md` for 0.19 ckb-std compatibility scope
- `CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md` and
  `CELLSCRIPT_SYNTAX_COMBO_AUDIT_METHODOLOGY.md` for 0.19 grammar/syntax
  governance scope
- `CELLSCRIPT_REGISTRY_PHASE1.md` for the 0.19 package/deployment identity
  registry closure and 0.20 handoff boundary
- `CELLSCRIPT_0_20_ROADMAP.md` for generated TypeScript action builders,
  live-chain registry verification, stateful flow evidence, and the bounded
  CellFabric JSON bridge
- `CELLSCRIPT_0_21_ROADMAP.md` for semantic closure, authenticated compiler
  evidence, derived cyclic graph views, type-level TemplateLayout metadata, and
  deferred optional template Merkleisation

## Archive

`docs/archive/` contains historical plans and superseded execution documents.
Archived files may remain useful for design archaeology, but they are not the
current stable contract.

Current archive:

- `docs/archive/0.13/CELLSCRIPT_0_13_1_PLAN.md`
- `docs/archive/0.13/CELLSCRIPT_SIGNATURE_DIRECTION_EXECUTION_PLAN.md`
- `docs/archive/0.15/CELLSCRIPT_0_15_ROADMAP_SUMMARY.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_ROADMAP.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_FINAL_REPORT.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_PRODUCTION_EQUIVALENCE_GATE.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_REVIEW_FINDINGS_CLOSURE.md`

When moving a document into the archive, update all public links and add a short
status note if the file could otherwise be mistaken for active guidance.
