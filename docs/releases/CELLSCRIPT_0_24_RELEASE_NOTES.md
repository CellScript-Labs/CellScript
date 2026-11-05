# CellScript 0.24 Release Notes

**Status**: stable release at `refs/tags/v0.24.0`. Final `dev`, `ci`, `backend`,
`release-quick`, and `release` evidence is recorded in the validation section.
The refreshed iCKB
evidence submodule commit `0e18ccd97bd75cac7de9211dc8d344c0bc08942f` is
published and bound by the parent gitlink. External ecosystem claims remain
limited to the explicit integration status below.

**Release date**: 2026-08-22

**Metadata schemas**: 58 / 2 / 1 / 2

**Rust toolchain**: 1.97.1

## Highlights

The 0.24 line closes four trust gaps without adding a new language edition:

1. CKB ELF builds emit a canonical verified lowering record and source map,
   and a smaller compiler-independent checker recomputes bounded structural
   invariants over those sidecars and final machine bytes.
2. `cellc test` requires an execution backend and runs versioned positive,
   exact-negative, and multi-step local Cell scenarios under the simulator,
   CKB-VM, or both.
3. Package resolution becomes lock-authoritative: `Cell.lock` v3 records a
   manifest-bound dependency graph, exact source identities, feature/test and
   CKB-environment roots, while ordinary builds never perform mutable version
   selection.
4. Deployable CKB Lock Scripts can publish LS-IDL as a byte-exact Registry
   interface, with the raw IDL SHA-256 committed in the executable suffix and
   resolvable by deployed Script identity.

The Registry can now admit artifact-only CKB bundles through a least-privilege
worker that depends on the standalone checker, not the CellScript compiler.

## Verified Artifact Files

ELF builds add:

- `ARTIFACT.lowering.json` using
  `cellscript-verified-lowering-record-v1`;
- `ARTIFACT.sourcemap.json` using
  `cellscript-source-artifact-map-v1`; and
- a `verified_artifact` identity in metadata schema 58.

`cellc verify-artifact` reports binding, structural, lowering-record, CKB-VM,
and chain evidence independently. Successful checking is not described as
complete source equivalence, VM execution, deployment, or commitment.

## Checker Evidence

The standalone checker validates canonical JSON, budgets, graph and
reachability policy, typed ABI/frame contracts, ProofPlan links, static ELF
shape, emitted RV64 instructions, canonical call targets, control flow, stack
restoration, syscalls, block digests, and source-map ranges. Stable rejection
codes `V2400` through `V2418` have a deterministic mutation corpus.

The production dependency graph of both the checker and the Registry
artifact-only verifier excludes the CellScript compiler. The Registry records
the checker version, policy schema, and report hash for structurally verified
admission.

The checker is packaged as an independent crates.io dependency. Packaging
gates verify it first and then verify the compiler against an exact local
registry patch; an actual release must publish the checker before the compiler.

## Executable Package Scenarios

`cellc test` requires `--backend simulator|ckb-vm|all` unless `--no-run` is
used. Scenario JSON rejects unknown fields and validates confined source paths,
named live Cell replacement, script identities, deps, headers, `since`,
witness fields, capacities, limits, and exact runtime errors.

Simulator output is development/non-consensus evidence. CKB-VM output is local
authoritative runtime evidence, not chain evidence. The v1 CKB-VM runner
supports no-argument entries; transaction-syscall cases remain with the
stateful CKB oracle.

`examples/scenario_basics` is the checked-in runnable form of this contract. It
executes positive and exact-negative fixtures under both backends and provides
a concrete four-file bundle/checker walkthrough.

Native `cellc run` now includes the VM runner by default. It executes only a
no-argument standalone ELF and fails closed for parameter or transaction/
syscall context. Development interpretation requires explicit `--simulate`;
there is no silent evidence-tier fallback.

## LS-IDL Lock Script Interfaces

0.24 adds an end-to-end LS-IDL path without inventing a second ABI format.
`cellc artifact ls-idl` can validate the bounded upstream 0.1 document, append
`SHA-256(raw idl.json bytes)` to a CKB executable, generate a publish-ready
artifact bundle, and fetch the original bytes by deployed Script identity.

Registry admission accepts the interface only for a deployable
`ckb_executable` Lock Script. The compiler-backed worker and least-privilege
artifact verifier independently check the IDL schema, raw ABI digest, and
executable's final 32 bytes. The API returns the stored bytes directly through
the canonical Script-identity route and the existing-client `/idl/:code_hash`
compatibility route; it never parses and reserialises the committed JSON.

The website now has a standalone Script-identity lookup under the explicit
`LS-IDL` tab and a dedicated LS-IDL document section on matching artifact
pages. The lookup surface is full-width and aligned with Registry browsing
rather than presented as a smaller generic “Interface” utility. The VS Code
extension exposes validate, bind, and fetch commands.

Compatibility evidence pins all 17 current `ckb-idl-client` vectors and all
seven checked-in IDLs from `ckb-idl-derive` and `ckb_sudt_script`. An opt-in
checkout-level acceptance script validates their raw hashes and runs the actual
upstream Rust client against Registry's `/idl/:code_hash` handler, covering
fetch, SHA-256 suffix verification, cache use, and witness decoding. The
runnable `examples/registry_ls_idl` remains the smaller explanatory fixture.

This is deliberately a narrow trust claim. Schema and suffix binding prove
which bytes were published and committed. They do not prove that a Lock Script
implements the described decoder correctly, and they are not a security audit.
The full profile and operator boundary are documented in the
[LS-IDL Registry profile](../CELLSCRIPT_LS_IDL_REGISTRY_PROFILE.md).

## Mainnet And Testnet Registry Parity

The production and Pudge Testnet Registry websites now build from one shared
interface contract. Browse, Publish, LS-IDL, API, Manage, and dynamic artifact
detail are present in both outputs and load the same byte-identical generated
CSS and JavaScript. The website CI build produces both environments and rejects
a missing route, divergent asset, or missing shared workflow hook.

Network context remains explicit rather than cosmetically erased. Testnet uses
its own API and object origins, `ckt` address prefix, Pudge chain, expiry policy,
and isolated records. Its LS-IDL form and copied API example default to
`testnet`; Manage and artifact details do not preload or fall back to mainnet
records. Production continues to default to mainnet. The environment control
is the intended visible distinction between otherwise matching interfaces.

## Registry Type Script Release Identity

The independently versioned Registry Type Script has a reproducible 0.24.0
artifact at
`contracts/registry-type-script/artifacts/v0.24.0/cellscript-registry-type-script`.
The 3,352-byte ELF has SHA-256
`0f48a8736360c121f6ae0f04ab4b0496834f6715d47e3284a0a07add609dede9`
and CKB data hash
`0x0dd596ade29e06e5bcc00f56abf36ecbe9afaa09f1b26a64436aa37854da622b`.
The canonical Linux x86_64 rebuild matches the tracked bytes exactly, and the
Registry API's production configuration gate binds this identity.

This artifact identity is release evidence, not a claim of mainnet
deployment. Production chain commitments remain disabled until the artifact
and all required Script/CellDep values are deployed, confirmed, and explicitly
configured.

## Playground Experience Upgrade

The browser Playground is now a recoverable Cell-oriented workbench rather
than a one-shot compiler demo. Local workspace snapshots retain source files,
entry selection, active panels, and saved/dirty state across refreshes. A
failed compile keeps the previous successful output visible but explicitly
marks it stale, and a failed compiler Worker can restart without a page reload.

The new Cell Flow view derives actions and type transitions from compiler
metadata, with source-linked selection and a contextual Inspector. A short,
optional guide helps first-time users through the workbench while raw actions,
types, diagnostics, and metadata remain directly accessible. Focus mode keeps
the same workbench but expands it to the viewport; mobile retains a compact
panel switcher. The WASM boundary remains metadata-only: the Playground does
not claim to emit or execute a production ELF.

## Website Release Identity

The 0.24 website carries forward the corrected release boundary. Its homepage
names `v0.24.0` as the current stable release, while its Playground worker,
compiler sample, and distribution checks use the matching 0.24 compiler
identity. Its 0.24 release branch removes the forward-looking 0.25-only
package-interface, typed-semantics, and syntax-highlighting presentation
fields; those remain outside the 0.24 release. The canonical Playground asset
is
`20260819-v0.24.0-19ce8898`; its WASM SHA-256 is
`19ce8898e8161f100edebf6f982d856f3e59bfac31572642b53f2e01c70a1a17`.
The raw module is 1,485,936 bytes and 567,048 bytes under the gate's gzip
measurement, below the 600 KiB budget.

The Node 22 website build validates both production and Pudge Testnet outputs,
the six-route byte-identical asset parity boundary, the current `v0.24.0`
release URL and tag, the compiler asset identity, and the exact WASM digest.
The parent repository pins website commit
`9849c0cb051439901bd3d9c01bd6ba58e8e40751` on the published
`codex/nightly-0.24-release` branch.

The 0.25-only Playground verification surface remains on the separate
`codex/nightly-0.25-release` website branch at commit
`3db5838ef1ba4b93fb0fc0f188c5374696e27377`. That is the exact website gitlink
used by parent branch `nightly-0.25`; neither release line depends on a hidden
or unreachable submodule commit.

## Compatibility And Migration

The CKB adapter no longer exports the deprecated, permanently fail-closed
`deploy_artifact` and `build_deploy` methods. Integrations must construct a
verified unsigned transaction with `build_deploy_transaction`, hand signing to
an external wallet, and submit the signed transaction explicitly.

CKB ELF generation now always uses the audited internal assembler. The
unreachable external RISC-V toolchain fallback and its environment-controlled
paths have been removed. Diagnostic `E2400` now identifies failure at the
verified lowering/source-map boundary; assembler layout, encoding, and ELF
failures retain the `E220x`/`E2300` families.

The code generator has been split along its documented ABI, assembler, call,
collection, expression, frame, runtime, schema, and Cell-operation boundaries.
This is an internal ownership refactor, not a new source-language or ABI claim.
The Fiber configuration renderer replaces the deprecated `serde_yaml` crate
with the maintained `serde_yaml_ng` continuation without promoting a Fiber
target profile.

These are repository-local integration changes. The Fiber migration updates
CellScript's Cargo dependency selection and lockfile; it does not patch or
publish upstream Fiber, CKB, CKB SDK, NovaSeal, or third-party crate source.
The website and editor gitlinks are project-owned release components rather
than upstream protocol dependencies.

The Registry website refresh adds route-specific headers, a shared content
grid, clearer evidence hierarchy, refined substrate material, and the restored
homepage brand animation. These presentation changes do not alter Registry
evidence semantics or the mainnet/testnet isolation boundary.

Final release hardening resolves dynamic Molecule-backed `u128` pointers once
before limb loads and spills the first wide operand while loading the second,
so schema validation cannot overwrite live arithmetic state. Mixed
`u128 +/- u64` operations now reject overflow and underflow with runtime error
49. Exact CKB-VM vectors cover both boundaries. Registry admission now rejects
non-canonical SemVer forms such as leading-zero core or numeric prerelease
identifiers.

## Integration Status

- The CellScript side of the Myelin 0.24 handoff is versioned and tested. The
  external Myelin lock update remains pending until the final 0.24 tag supplies
  an exact CellScript source revision. No raw-witness alias or Myelin target
  profile is added.
- Fiber remains no-profile. Static compiler/CKB-VM evidence is retained, but
  the complete external lifecycle and negative matrix has no complete evidence
  bundle and remains pending.
- RGB++ remains an ecosystem identity sidecar. Rgbpp Lock, BTC Time Lock, BTC
  SPV, witness/commitment, deployment, confirmation, reorg, and paired
  CKB/Bitcoin evidence are not complete and are not promoted.

## Package And Registry Evolution: Lessons From Sui Move

The package work was informed by Mysten's Sui Move `move-package-alt` design at
commit `5a9f37431c473fa2f6d49abecbcc6a6d7190f533`. CellScript adopts the parts
that reduce ambiguity in an auditable CKB compiler, while retaining a different
source, artifact, and chain model.

### Lock first; repin explicitly

The central principle is that dependency selection and compilation are
different authorities. `cellc lock`, `cellc update`, `cellc add`,
`cellc remove`, and `cellc install` may resolve mutable requirements and write
a new graph. `build`, `check`, and `test` consume only that graph. A missing
lock, changed manifest digest, missing graph edge, moved source, or changed
content hash fails closed and tells the user to repin explicitly.

`--locked` documents the intent to require the existing dependency graph.
That graph is authoritative even without the flag; the flag is useful in CI
and scripts. `--frozen` additionally implies offline operation and suppresses
all `Cell.lock` writes, including refreshed build evidence. `--offline` permits
only already materialized exact Registry/Git sources.

This follows Move's separation between resolution and pinned compilation, but
CellScript keeps build/deployment evidence in the same file. Therefore an
ordinary non-frozen build may refresh `[package.build]` and deployment facts;
it never changes dependency nodes or root edges.

### A graph, not a flat version list

`cellscript-lock-v0.24-graph-v1` records:

- the exact SHA-256 digest of the root `Cell.toml`;
- canonical dependency nodes with declared package name, SemVer, immutable
  source, whole-tree source hash, dependency-manifest digest, and outgoing
  alias-to-node edges;
- separate runtime and test root edges;
- feature-qualified node identities; and
- named CKB environment roots bound to both `chain_id` and the 32-byte genesis
  hash.

The graph allows two source/version nodes to coexist in resolution. It does
not pretend that two packages declaring the same CellScript module are safe:
the compiler's existing duplicate-module and type-identity checks still fail
closed. This is deliberately narrower than importing Move's package/type
identity wholesale.

Git branches and tags are update-time conveniences only. They resolve to a
full 40-hex commit and an immutable local cache. A later branch movement has no
effect on a locked build; only explicit repinning observes it. Registry sources
are likewise materialized from the exact snapshot URL and `sha256:` identity
recorded in the lock, without repeating discovery or version selection.

### Standard SemVer, aliases, features, tests, and environments

Version requirements now use standard SemVer matching, including correct
`0.x`, prerelease, build-metadata, range, and lower-bound behavior. A bare
CellScript version retains the existing compatible (`^`) meaning.

Dependency aliases are separate from declared package identity through
`package = "..."`. `[features]` supports `default`, feature-to-feature
expansion, and `dep:<alias>` activation for optional dependencies. Feature
cycles and unknown activation targets are rejected. `[dev_dependencies]` enter
only the `cellc test` graph. `[build.dependencies]` remains reserved and fails
closed because executing build scripts without an isolation contract would
expand the trusted computing base.

`[environments.<name>]` binds dependency choice to a concrete CKB chain
identity. `[dependency_overrides.<name>]` can replace declared dependencies for
that environment, but there is no implicit mainnet/testnet selection: callers
must pass `--environment <name>` when overrides exist. This adapts Move's named
environment idea to CKB's genesis-bound Cell Model rather than copying Sui
addresses or published package IDs.

`examples/package_graph` is the portable runnable form of these package
features. Its checked-in graph covers a declared-package alias, standard SemVer
requirements, optional and transitive feature activation, a test-only
dependency, two genesis-bound environments, and an exact testnet override.
Frozen/offline commands prove that those selections are consumed from the lock
without invoking mutable resolution.

### Bounded resolver extension, normalized before trust

`[resolvers.<name>]` is a versioned extension point for package ecosystems that
cannot be expressed directly. The executable path is absolute and SHA-256
bound. CellScript invokes it without a shell or inherited environment, with a
10-second deadline and 1 MiB stdout/stderr limits, over
`cellscript-dependency-resolver-request-v1`. The response must use
`cellscript-dependency-resolver-response-v1` and normalize to either an exact
Registry version or a Git URL plus full commit.

The resolver itself is never stored as build authority and is never executed
by a locked build. `Cell.lock` contains only the normalized source and content
identity. This preserves Move's extensibility insight without permitting an
unbounded plugin system inside compilation.

### Registry profiles are versioned and fail closed

Registry artifact admission now uses
`cellscript-registry-profile-catalog-v1`. Every supported profile names its
validator, allowed kinds/languages/consumption modes, whether a profile
contract is required, and whether it may participate in dependency resolution.
Only `cellscript_source` has `resolver_capability = dependency`; CKB
executables, reproducible builds, and copy material remain discoverable but
non-resolving. Adding a future profile is therefore an explicit versioned
contract change rather than another scattered conditional.

What 0.24 does **not** copy from Move is equally important: there is no Move
bytecode/module ID, Sui address or object identity, implicit environment
selection, unrestricted resolver plugin, source-equivalence claim from hashes,
or conversion of executable/copy artifacts into source dependencies.

## Validation

The exact source identified by `refs/tags/v0.24.0` passed all five canonical
gates on the release date 2026-08-22. The environment used Rust `1.97.1`, Node
`22.23.2`, CKB revision
`f7fa4436737756f97a24e254f22c13a36316ecea`, CKB SDK `v5.1.0`, the
`riscv64imac-unknown-none-elf` target, and the pinned Docker base
`rust:1.97.1-slim-bookworm@sha256:99e09cb2284e2ddbb73a995deee3e91783fd04d177602ccf6eab326d778ee777`:

```bash
./scripts/cellscript_gate.sh dev
./scripts/cellscript_gate.sh ci
./scripts/cellscript_gate.sh backend
./scripts/cellscript_gate.sh release-quick --ckb-repo /path/to/pinned/ckb
./scripts/cellscript_gate.sh release --ckb-repo /path/to/pinned/ckb
```

The run covered compiler and workspace tests, Clippy, the full strict backend
audit, all 218 iCKB differential cases, simulator and CKB-VM package scenarios,
Registry API and independent-verifier tests, package construction, website
production/testnet parity, VS Code packaging, the canonical Docker WASM build,
NovaSeal RISC-V reproducibility and pinning, and the non-compile-only production
stateful CKB acceptance harness. The Registry Type Script rebuilt byte-for-byte
to the 3,352-byte artifact recorded above. The NovaSeal verifier rebuilt to
100,912 bytes with SHA-256
`be66f22507b734c8a432c4c85f0079cc7461caaa92ee3277d50ea8f62ce95ff7`
and CKB data hash
`0x988bab12eab02ebfccc2d2a84da46f4119c5343797ada24104683e61cd07d26e`.
Its RWA profile source binding is
`0x779dad4139b1ffbbfa774d9e9d82b9765d4c23b62a86accd395e0b15bef4db47`.

The refreshed iCKB matrix is versioned in the benchmark submodule rather than
copied into the parent repository. Commit
`0e18ccd97bd75cac7de9211dc8d344c0bc08942f` is published on that submodule's
`main` branch, and the parent repository binds the same gitlink, so a clean
clone can reconstruct the exact evidence tree that passed `backend`.

The canonical website build produced the 0.24 WASM identity recorded above and
passed the 600 KiB gzip budget. Gates establish source and artifact evidence;
they do not publish either crates.io package or deploy the Registry Type
Script. Publication and deployment remain explicit release-operator actions
and must preserve the validated source and artifact identities.

## Detailed References

- [Verified artifact boundary](../CELLSCRIPT_VERIFIED_ARTIFACT_BOUNDARY.md)
- [Executable test scenarios](../CELLSCRIPT_EXECUTABLE_TEST_SCENARIOS.md)
- [LS-IDL Registry profile](../CELLSCRIPT_LS_IDL_REGISTRY_PROFILE.md)
- [Gate policy](../CELLSCRIPT_GATE_POLICY.md)
