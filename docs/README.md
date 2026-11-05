# CellScript Documentation Map

This directory is organized by document role. Keep new docs in the smallest
stable category that matches how readers should use them.

The current development contract uses metadata schema 76; its runtime
matrix is [`cellscript-ckb-runtime-view-v1`](CELLSCRIPT_0_30_CKB_RUNTIME_VIEW_MATRIX.md).
The [native certification boundary](CELLSCRIPT_NATIVE_CERTIFICATION_BOUNDARY.md)
records CLI, proposal-engine, packaging, dependency and validation ownership.
The additive six-domain `Since`, typed HeaderDep, and checked `EpochDuration`
subset of issue #12 is recorded in the
[typed CKB temporal-domain contract](CELLSCRIPT_0_30_TEMPORAL_DOMAINS.md).
The runtime-view matrix also records the bounded source-level
`script::new`/`script::hash` contract and its lowering-record-v13 machine
evidence.
The [source contract](CELLSCRIPT_SOURCE_CONTRACT.md) defines the single current
source, artifact, interface and consumer vocabulary.

## Stable Tutorials

`docs/wiki/` contains the GitHub Wiki source. These pages are version-neutral,
reader-facing tutorials and cookbook material. They should teach the current
stable surface rather than act as release logs.

The [ProtocolBundle end-to-end tutorial](wiki/Tutorial-17-ProtocolBundle-End-to-End.md)
separates the stable closed-role workflow from bounded 0.32 development profiles.
The [Chinese Nervos Talk draft](tutorials/CELLSCRIPT_CROSS_CONTRACT_NERVOS_TALK.md)
explains cross-contract organization, a raw Rust responsibility comparison,
the unfinished #47/#48/#49 roadmap and the SSRI/IPC design reviews (#81–#83).
The community draft is not a release announcement or a published forum post.

## Release Records

`docs/releases/` contains finalized release notes and active release-note
drafts. Released versions should use non-draft filenames.

Release candidates and planning notes should not live here unless they are the
final release record.

## Reference And Evidence Contracts

Top-level `docs/CELLSCRIPT_*.md` files are active reference material when they
describe current compiler behavior, target-profile evidence, runtime errors,
syntax governance, metadata, capacity, deployment manifests, or support
matrices.

High-value active references include:

- [CELLSCRIPT_SOURCE_CONTRACT.md](CELLSCRIPT_SOURCE_CONTRACT.md) for the current
  source, artifact, interface and consumer vocabulary
- [CELLSCRIPT_BYTE_ACCESS.md](CELLSCRIPT_BYTE_ACCESS.md) for raw byte, bounded
  view and digest contracts
- [CELLSCRIPT_0_30_CAPABILITY_CLOSURE_RFC.md](CELLSCRIPT_0_30_CAPABILITY_CLOSURE_RFC.md)
  for capability coverage, remaining work owners and the bounded business
  portfolio
- [CELLSCRIPT_0_30_CKB_RUNTIME_VIEW_MATRIX.md](CELLSCRIPT_0_30_CKB_RUNTIME_VIEW_MATRIX.md)
  for typed CKB transaction-view fields, bounded syscall families, stable
  failures, executable evidence, and the remaining issue #24 work
- [CELLSCRIPT_0_30_CRYPTOGRAPHIC_CAPABILITY_MATRIX.md](CELLSCRIPT_0_30_CRYPTOGRAPHIC_CAPABILITY_MATRIX.md)
  for the native, standard-Lock, exact-identity, and trusted-external
  cryptographic capabilities used by the frozen eight-family business corpus
- [CELLSCRIPT_COMMITTED_SUBSTATE.md](CELLSCRIPT_COMMITTED_SUBSTATE.md)
  for the fixed-width typed commitment/opening profile, canonical hash
  preimage, witness encoding, and post-validation materialization rule
- [CELLSCRIPT_COMPILER_REQUIREMENTS.md](CELLSCRIPT_COMPILER_REQUIREMENTS.md)
  for the enforced package SemVer range, dependency preflight diagnostics,
  Registry selection, and `Cell.lock` v5 evidence
- [CELLSCRIPT_WORKSPACE_GRAPH.md](CELLSCRIPT_WORKSPACE_GRAPH.md) and
  [CELLSCRIPT_PACKAGE_INSPECTION.md](CELLSCRIPT_PACKAGE_INSPECTION.md) for the
  canonical member graph and stable resolve/build inspection schemas
- [CELLSCRIPT_TRANSACTIONAL_UPGRADES.md](CELLSCRIPT_TRANSACTIONAL_UPGRADES.md)
  for candidate resolution, graph/interface/build evidence, policy
  acknowledgements, stale-plan rejection, and explicit lock-only apply
- [CELLSCRIPT_AUTHORING_TARGET.md](CELLSCRIPT_AUTHORING_TARGET.md) for the
  adopted familiar authoring direction, shared-policy multi-action contracts,
  schema review, authorization boundaries, and required acceptance examples
- [CELLSCRIPT_AUTHORING_IMPLEMENTATION.md](CELLSCRIPT_AUTHORING_IMPLEMENTATION.md)
  for the complete implementation goal, supported-language coverage, current
  evidence, and remaining production acceptance work
- [CELLSCRIPT_ALPHA_2_BOUNDARY_REVIEW.md](CELLSCRIPT_ALPHA_2_BOUNDARY_REVIEW.md)
  for retained source/storage boundaries, removed effect protocols, emission
  failure propagation and the scoped phase-invariant review
- [CELLSCRIPT_PUBLIC_VALUE_GENERICS.md](CELLSCRIPT_PUBLIC_VALUE_GENERICS.md)
  for the accepted restricted public-generic decision, `fixed_value` profile,
  structural derivation, interface compatibility, and migration contract
- [CELLSCRIPT_BOUNDED_GROUP_INPUT_CONTRACT.md](CELLSCRIPT_BOUNDED_GROUP_INPUT_CONTRACT.md)
  for the accepted bounded dynamic Type Script `GroupInput` semantics,
  fail-closed boundary, independent machine checks, shared fixtures, and
  maximum-resource budget
- [CELLSCRIPT_SCHEMA_ACKNOWLEDGEMENTS.md](CELLSCRIPT_SCHEMA_ACKNOWLEDGEMENTS.md)
  for the versioned `same except` schema-change plan, explicit new-field rule,
  review receipt, stale verification, and deployment boundary
- [CELLSCRIPT_EXACT_SCRIPT_HANDLES.md](CELLSCRIPT_EXACT_SCRIPT_HANDLES.md) for
  the fixed exact-artifact handle layout, source API, on-chain checks, and the
  boundary before generic compatible/open handles
- [CELLSCRIPT_OPEN_HANDLE_POLICY.md](CELLSCRIPT_OPEN_HANDLE_POLICY.md) for the
  experimental bounded authorization-set codec, independent wire vectors and
  remaining compatible-handle admission/runtime requirements
- [CELLSCRIPT_DEPLOYMENT_LINE_HANDLES.md](CELLSCRIPT_DEPLOYMENT_LINE_HANDLES.md)
  for the Type-hash upgrade receipt chain, six-dimensional compatibility
  policy, fixed line-handle encoding, and remaining unique admission-Cell and
  runtime blockers
- [CELLSCRIPT_POLICY_WITNESS_ABI.md](CELLSCRIPT_POLICY_WITNESS_ABI.md) for
  explicit policy selection, tagged multi-record witnesses, builder contracts,
  and signature-field ownership
- [CELLSCRIPT_TRUSTED_EXTERNAL_VERIFIERS.md](CELLSCRIPT_TRUSTED_EXTERNAL_VERIFIERS.md)
  for exact-hash EXEC/SPAWN composition, manifest declarations, the
  `trusted-external` evidence tier, and its explicit no-proof-of-internals
  boundary
- [CELLSCRIPT_SEMANTIC_FOUNDATION.md](CELLSCRIPT_SEMANTIC_FOUNDATION.md) for the
  current provenance, operation, identity and independent checking contract
- [CELLSCRIPT_AUTHORING_GRAMMAR.md](CELLSCRIPT_AUTHORING_GRAMMAR.md) for ordinary
  action/lock authoring, explicit Script-group bindings, complete successor
  relations, bounded pooling, retirement, fresh creation and metadata-only audits
- `releases/CELLSCRIPT_0_13_2_RELEASE_NOTES.md` for the final 0.13 syntax
  governance summary
- `CELLSCRIPT_GATE_POLICY.md`
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
  boundary of the wallet-rooted public registry write/read architecture and
  isolated Pudge testnet sandbox
- `CELLSCRIPT_LS_IDL_REGISTRY_PROFILE.md` for byte-exact LS-IDL admission,
  executable suffix commitment, Script-identity lookup, tooling, and operator
  boundaries
- `../services/registry-api/README.md` for the Cloudflare Workers + R2 + Neon
  write API implementation and deployment checklist
- `CELLSCRIPT_COLLECTIONS_SUPPORT_MATRIX.md`
- `CELLSCRIPT_ENTRY_WITNESS_ABI.md`
- `CELLSCRIPT_EXAMPLE_BUSINESS_FLOWS.md`
- `CELLSCRIPT_LINEAR_OWNERSHIP.md`
- `CELLSCRIPT_OUTPUT_BINDINGS.md`
- `CELLSCRIPT_RUNTIME_ERROR_CODES.md`
- `CELLSCRIPT_VERIFIED_ARTIFACT_BOUNDARY.md`
- [Runtime ABI evidence coverage](../crates/cellscript-artifact-checker/docs/RUNTIME_ABI_COVERAGE.md)
- `CELLSCRIPT_PUBLIC_INTERFACES.md`
- `CELLSCRIPT_EXECUTABLE_TEST_SCENARIOS.md`
- `CELLSCRIPT_MYELIN_HANDOFF.md`
- `CELLSCRIPT_COMPILER_ERROR_CODES.md`
- `CELLSCRIPT_SCHEDULER_HINTS.md`
- `../examples/fiber/README.md` for the bounded 0.22 Fiber interoperability
  operator workflow

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

## Design And Release Records

Shipped behavior is recorded in `releases/`, while forward-looking work must
be owned by a concrete RFC under `docs/` or a scoped implementation proposal
under `proposals/`. The documentation set does not maintain a separate roadmap
layer.

Active design and evidence records include:

- `CELLSCRIPT_INSTANCE_POLICY.md` for the #80 constrained-instance contract,
  implementation boundaries and acceptance ledger
- `CELLSCRIPT_OPEN_HANDLE_DESIGN.md` for the #28 compatible-open handle design
  record and threat model over the implemented authorization-set boundary
- `CELLSCRIPT_CKB_STD_COMPAT.md` for the ckb-std compatibility boundary
- `CELLSCRIPT_GRAMMAR_GOVERNANCE_RFC.md` for 0.19 grammar/syntax governance
  scope
- `CELLSCRIPT_REGISTRY_PHASE1.md` for the current artifact, verification,
  deployment-evidence, and public API contract
- `CELLSCRIPT_LS_IDL_REGISTRY_PROFILE.md` for the 0.24 Lock Script interface
  profile and compatibility evidence
- `releases/CELLSCRIPT_0_22_RELEASE_NOTES.md` for the typed transaction-view,
  bounded collection, validity, borrow, capability, payload-enum, and Fiber
  release boundary
- `releases/CELLSCRIPT_0_24_RELEASE_NOTES.md` for the Edition/ABI, Registry,
  verified-artifact, executable-test, and ecosystem evidence boundaries

## Archive

`docs/archive/` contains historical plans and superseded execution documents.
Archived files may remain useful for design archaeology, but they are not the
current stable contract.

Current archive:

- `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_FINAL_REPORT.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_ICKB_PRODUCTION_EQUIVALENCE_GATE.md`
- `docs/archive/0.17/CELLSCRIPT_0_17_REVIEW_FINDINGS_CLOSURE.md`

When moving a document into the archive, update all public links and add a short
status note if the file could otherwise be mistaken for active guidance.
