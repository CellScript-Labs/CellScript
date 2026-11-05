# Branch Context

## dev — Edition 2027

`dev` starts from the alpha.2 preparation at `54eb2220`. Source and artifact
admission use Edition 2027, shared authoring semantics and explicit versioned
metadata contracts. The edition remains evolving; this branch does not imply
a new release or production acceptance. See the
[source contract](docs/CELLSCRIPT_EDITION_POLICY.md).

The compiler code at `a3d28009` passed the commit, dev and ci gates. The current
compiler implementation retains that checked code. Full backend and production
acceptance results belong to the production receipt for the exact candidate
commit and its release attachments.

## v0.33.0-alpha.2 — initial bounded runtime prerelease

The original `v0.33.0-alpha.2` tag published `codex/bounded-runtime-foundations`
under the maintainer's 2026-11-05 instruction. The v193 snapshot
(`c50266ad`) passed the commit and dev gates, every backend step before the
node-stateful replay, and the canonical browser/Registry/PostgreSQL/
certification consumer evidence; the stateful replay failure recorded there is
superseded by the v194 schema-vector field-span repair. A fresh full stateful
pass and the full CI/release gates are not claimed. This is an alpha with an
evolving public API. See the
[release scope](docs/releases/CELLSCRIPT_0_33_ALPHA_2_RELEASE_NOTES.md) and
the [alpha.2 backlog](docs/releases/CELLSCRIPT_0_33_ALPHA_2_TODO.md).

## 0.33.0-alpha.1 — bounded runtime prerelease

`v0.33.0-alpha.1` publishes `codex/bounded-runtime-foundations` under the
maintainer's 2026-10-28 instruction. The full compiler test run identifies signed
implementation source `309eb23e`; the later packaging commit updates native
workspace versions and publication documentation. Complete backend/dev/ci and
release gates are not claimed. This is an alpha with an evolving public API.
See the [release scope](docs/releases/CELLSCRIPT_0_33_ALPHA_1_RELEASE_NOTES.md).
The version/branch descriptions below retain their historical dates.

## Branch decision — 2026-09-21

`0.30` is retained as the published release baseline. Its final signed
`v0.30.0` tag points at `352b4950e7dc7489a4ee7e7ed6d6edf4c7fefd71`.
Release tags and submodule repositories are preserved. Retired root branch
tips match the verified September 21 Git bundle. The descriptions below
preserve historical evidence scope.

## 0.30

`0.30` is the capability-closure release branch, forked from
`0.26b` at `08c0ef38`. The current planning target permits the next stable
release after 0.25 to be 0.30. The experimental `0.26b` work may be absorbed
into that release without a published 0.26 stable line. No stable tag, Cargo
version, source edition, metadata schema, ABI, or deployment identity exists
merely because the branch and roadmap use the 0.30 name. See
[`docs/CELLSCRIPT_0_30_CAPABILITY_CLOSURE_RFC.md`](docs/CELLSCRIPT_0_30_CAPABILITY_CLOSURE_RFC.md)
for the issue coverage, bounded Rust-comparable business portfolio, missing work
owners, staging, and release acceptance criteria.

The first Stage 1 slice gives authoring successor relations a precise complete
Script-hash domain: `lock = exact_hash(value)` accepts only `ScriptHash`, typed
CKB transaction-view hash fields produce that type, and
`ckb::script_hash(hash)` performs an explicit conversion from a trusted raw
`Hash`. This conversion does not establish Script existence, deployment, or
authorization. Real CKB-VM tests cover matching and substituted output Lock
Script hashes while the existing `lock = exact(address)` form remains intact.

The issue #18 package-compiler prerequisite is implemented through enforced
`[package].cellscript_version` SemVer ranges, aggregated root/transitive
pre-source diagnostics and compiler-aware Registry selection. The current
`Cell.lock` v5 retains those compiler-requirement bindings and adds the
single-package-coordinate model described below.
Compiler compatibility remains separate from Edition, target/profile,
metadata/checker schema, and exact reproducible compiler identity.

Issue #16's package-instance prerequisite is also implemented. Package
coordinates are namespace plus declared name, aliases are edges, and each
selected graph admits one exact version/source/feature/environment instance per
coordinate. Conflicts fail during resolution with E2601, while `Cell.lock` v5
declares the `single-package-coordinate-v1` model and applies it unchanged in
locked and frozen materialization. Supporting multiple versions remains a
future schema and package-qualified module-identity change.

Issue #15's workspace prerequisite now uses a canonical versioned member graph
instead of declaration-order loops. It implements exclusions, unique names and
paths, dependency-first selected closure, member-lock preflight, cycle and
failure propagation, and member-local build-identity refresh. Virtual workspace
roots reject synthetic `Cell.lock` artifact lists. The frozen/offline diamond
fixture is part of both local and CI gates.

Issue #19 is implemented through separately negotiated resolve-graph and
build-plan v1 schemas. Queries are locked, offline, deterministic, and
side-effect free; build results validate the same unit identity. Package audit,
LSP/VS Code, and both routine gates use the shared schemas. `cellc metadata`
continues to mean compiled-program metadata.

Issue #20 is implemented through `cellscript-upgrade-plan-v1`. Candidate locks
are resolved and compiled in memory, exact graph changes and reverse-dependent
interface/build evidence are retained, and `cellc update` remains read-only
unless `--apply-plan` names a reviewed receipt. Apply checks plan and compiler
identity, exact old lock bytes, policy acknowledgements, canonical candidates,
and confined paths before replacing member locks. It never edits deployment
records or supplies deployment authorization. This closes the #15-#20
workspace/resolver/toolchain composition prerequisite on the branch; it does
not by itself satisfy the broader 0.30 product or Rust-comparable release gate.

Until this branch passes those criteria, treat it as development work, use
`0.26b` only as its experimental implementation baseline, and retain 0.25 as
the predecessor release contract. Do not publish a 0.26 tag solely to preserve
sequential version numbering.

## 0.26b

The [adopted authoring target](docs/CELLSCRIPT_AUTHORING_TARGET.md) preserves
`resource`, `action`, `lock`, and `require` while requiring concise successor
relations and multiple actions under one persistent deployed policy. It is
design direction for the next iteration, not functionality already provided
by preview4. The versioned semantic model remains reusable; the verbose
preview4 text is not the final authoring target.

## nightly-0.26

`nightly-0.26` is the active consensus-runtime development line for bounded
Cell-group consumption and bounded output-plan correspondence. The first
supported shape is deliberately limited to exact Type Script groups with a
compile-time cardinality bound, canonical Molecule decoding, deterministic
group-relative order, and independently checked machine evidence. Treat the
line as non-production until the positive CKB-VM/stateful fixtures, mutation
checks, resource measurements, independent security review, and the `dev`,
`ci`, and `backend` gates agree.

## nightly-0.25

`nightly-0.25` is the language-completeness predecessor. It adds
bounded value generics, explicit visibility and package interfaces, exhaustive
IR-surface classification, and the typed-semantics v2 / lowering-record v3
boundary. Treat it as merge-ready only when compiler, independent checker,
Registry, editor/Playground, docs, and the `dev`, `ci`, and `backend` gates all
agree. It is not a stable release or production CKB evidence claim; the crate
version identifies the 0.25 development line but is not a substitute for a
signed release tag or the coordinated release gate.

## 0.12-era proposal baseline

The 0.12-era work is the formal proposal baseline for grant-style acceptance
discussions. Do not use that historical baseline to describe the current
`main` branch state.

## nightly-0.22

`nightly-0.22` is the historical implementation line for the 0.22 type-and-set
theory release. The stable release boundary is the `v0.22.0` tag, not the
nightly branch name.

## main

`main` is the integration baseline. Use an exact release tag for stable-release
comparisons and an exact nightly branch for development-scope comparisons;
do not infer release evidence from `main` alone.

## v0.24.0

`v0.24.0` is the current stable release for the verified-artifact checker,
executable package scenarios, lock-authoritative package graph, and LS-IDL
Registry path. Use the exact tag ref `refs/tags/v0.24.0` for stable
comparisons. The release does not promote the separately pending Myelin,
Fiber, or RGB++ external evidence boundaries.

## v0.22.0

`v0.22.0` is the historical stable baseline for the type-and-set-theory line.
Use the exact tag ref `refs/tags/v0.22.0` when reproducing that release rather
than treating a later nightly branch as equivalent evidence.

## 0.16

0.16 is an audit-hardening preview. It is useful for tracing how earlier review
findings were handled, but it should not be treated as the current iCKB
differential-evidence branch.

## research/protocol-equivalence

`research/protocol-equivalence` is the 0.17 research and differential-evidence
branch. It moves the iCKB benchmark from model-only evidence into broad partial
CKB VM differential evidence for selected normalized fixtures.

Current active matrix counts:

- `DIFFERENTIAL_CKB_VM_EXECUTED`: 66
- `CELL_SCRIPT_CKB_VM_EXECUTED`: 14
- `ORIGINAL_ICKB_CKB_VM_EXECUTED`: 8
- `MODEL`: 0

The branch still keeps `equivalence_status = NOT_PROVEN` and
`production_equivalence_claim = false`. Do not describe it as production
equivalent until the gate has complete evidence-manifest closure and the
non-executable assumptions registry is empty.
