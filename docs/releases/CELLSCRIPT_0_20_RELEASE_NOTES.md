# CellScript 0.20 Release Notes

**Status**: Final release notes for CellScript 0.20.0.

**Updated**: 2026-06-28.

CellScript 0.20 hardens the generated-builder and live/devnet acceptance path.
The important post-audit change is that CKB-facing acceptance now checks the
ELF loader/ABI boundary before treating a local devnet result as release
evidence.

The post-audit scope is CellScript only. External repositories and unrelated
VM findings are intentionally out of scope for this release note.

## ELF Entry ABI Gate

The CKB/devnet acceptance script now emits and validates a
`ckb_elf_entry_abi_gate` section for every compiled CKB RISC-V ELF artefact.
The gate fails closed unless:

- the executable `PT_LOAD` segment is readable and executable only;
- the executable segment has `filesz == memsz`, so the ELF does not fake stack
  memory;
- the entry trampoline starts with the expected call sequence into the real
  entry point;
- the trampoline preserves the CKB VM-provided `sp` stack pointer instead of
  initialising a private stack address.

This gate is required before local-node dry-run, tx-pool acceptance, submitted
stateful flows, and production evidence validation.

## Exact Artifact Build Reports

The CKB acceptance report now includes a `cellscript_build_reports` index. Each
row binds one CKB-deployable RISC-V ELF to:

- the CKB blake2b deployable ELF hash;
- the SHA-256 host-file hash;
- `cellc verify-artifact` status and target profile;
- ELF entry ABI gate status;
- ABI-trailer stripped status;
- live code-cell data hash when a devnet deployment is executed.

Production evidence validation fails closed when a live code-cell data hash does
not match the compiled deployable ELF hash. This closes the previous
auditability gap where compile evidence, verifier evidence, and live deployment
evidence could be reported without a single exact-artifact identity row.

## Cell Data Codec Manifest

Compile metadata now includes a `cell_data_codec_manifest`. Molecule-native
contracts continue to declare `abi = "molecule"`. Contracts that use raw
`LOAD_CELL_DATA` runtime accesses declare `abi = "molecule+raw-bytes-v1"` and
list the raw cell-data accesses that require external codec materialisation.

`cellc gen-builder --target typescript` now exports this manifest in both the
builder manifest and generated TypeScript action plans. Generated builders still
delegate transaction materialisation to runtime adapters; they do not claim to
implement raw cell-data encoders by themselves.

`Cell.lock`, `Deployed.toml`, deployment records, and generated builder identity
checks now carry `cell_data_codec_manifest_hash` as a first-class build identity
field. A mismatched or missing codec manifest hash fails registry and builder
verification instead of being treated as an opaque metadata difference.

## Metadata Schema Partitioning

Compile metadata now keeps the top-level `metadata_schema_version` as the
envelope compatibility wall and adds component versions for source/package
identity, artifact-binding facts, and CKB constraint summaries:

- `source_metadata_schema_version`
- `artifact_metadata_schema_version`
- `constraints_metadata_schema_version`

`verify-artifact` rejects mismatches in any component. CLI JSON reports,
deploy plans, dependency locks, audit bundles, and generated-builder ABI hashes
also expose the split schema object so downstream tools can tell which metadata
surface changed.

## Multi-File Project Boundary

The 0.20 compiler path now treats multi-file packages as a validated source
graph instead of an entry-file side effect:

- package compilation loads the entry package and local path dependency
  `.cell` sources before frontend checks;
- `use` imports are exact-path and fail closed when the module or symbol is not
  present;
- package diagnostics run type, flow, and IR checks across loaded modules;
- incremental cache identity includes dependency `.cell` files, `Cell.toml`,
  and `Cell.lock`;
- file-backed LSP diagnostics use the package graph; and
- the WASM package keeps the legacy single-source functions while adding an
  additive multi-source metadata diagnostics API for browser tools.

This is still one artifact per entry. Cross-file type/schema imports and helper
calls are resolved at compile time and inlined into the entry artifact,
including aliased imports, fully-qualified calls, and transitive helper calls.
Any ELF-linker-style or cross-script runtime-linking claim remains outside the
0.20 production boundary.

NovaSeal fungible-xUDT now includes the first protocol-source multi-file
candidate: shared witness and commitment schema structs live in
`src/nova_fungible_xudt_schema.cell` and are imported by both the profile action
entry and the lifecycle type entry. Metadata and artifact-preparation evidence
show the shared schema in the compiled source graph, and the live local devnet
stateful profile passes issue, transfer, settle, and required negative cases
for lifecycle artifact data hash
`0x394da78133cb2f5a5d6cd911feceeab9e97e6ad5d36c0e50f18be56653af85e5`.

iCKB benchmark sources are unchanged because the current files do not expose a
natural shared-schema boundary, and the checked-out DobEvo / DOB-EVO proposal
contains no `.cell` source to refactor. Future protocol-source changes must
still carry proposal-specific evidence: NovaSeal profile certification plus
live local devnet/profile reports, iCKB CKB VM differential matrix refreshes,
or DobEvo / DOB-EVO devnet workflow and registry-pressure reruns.

## Critical Example Coverage

The 0.20 devnet acceptance path explicitly keeps launch.cell, token.cell, and
amm_pool.cell in the ABI gate. These examples are the builder-facing bootstrap
path for token launch and AMM flows, so their reliability now depends on both
business-flow evidence and the lower-level ELF entry ABI evidence.

The existing local CKB acceptance still covers:

- strict original bundled example compilation;
- builder-backed action transactions;
- valid and invalid lock-spend checks;
- measured cycles;
- consensus-serialized transaction size;
- occupied-capacity checks;
- stateful lifecycle scenarios, including launch-to-mint and AMM
  seed/add/swap/remove flows.

## Browser Playground Scope

The playground exposes the new multi-source WASM boundary through a
browser-local file tree, explicit entry file, file-aware diagnostics, and local
import/export. This UI remains client-side: no server compile API, no uploaded
source archive, no server-owned project state, and no server-side cache.
Import/export uses local file selection, multiple `.cell` files, and
downloadable workspace JSON generated in the browser, with source-count and
total-byte limits so browser CPU and memory stay bounded.

The release claim is a client-side playground workspace over the existing
WASM compiler path, not a server-backed workspace service.

## Final Polish After rc.2

The release notes above were frozen at the second release candidate. A small
set of later commits deepened the developer experience without changing the
shipped boundary. They are recorded here so the 0.20 line and the release
trail agree.

### Multi-Diagnostic Recovery and Reporting

The CLI diagnostic surface now has explicit recovery semantics. When one
frontend error hides another, both are surfaced with their own source
context, and the recovery report groups them by file so the next fix is
unambiguous. The changes stack on top of the 0.20-rc.2 "multi-diagnostic
package checks" claim:

- `cellc` direct parse, lex, and compile errors already print
  `file:line:column` source snippets at the second release candidate; the final
  polish improves how those snippets group, deduplicate, and recover
  after a partial parse failure.
- The `--explain <CODE>` alias and the per-error source context render
  the same way whether the error comes from the package graph, the
  entry file, or a dependency `.cell` file imported through `use`.
- `ErrorReporter::has_errors()` continues to only treat error severity
  as release-blocking; the post-rc.2 polish tightens the warning-vs-
  error boundary so a partial-failure recovery report never silently
  reclassifies a warning as a release blocker.

### Wiki Tutorial Accuracy

Tutorial 12 (Phase 1 registry end-to-end) and the surrounding tutorial
chain were reviewed end-to-end. The post-rc.2 fix corrects stale
references in the Phase 1 navigation, the `cellc install` /
`cellc registry verify` examples, and the generated TypeScript builder
section. A developer following the tutorial from `cellc init` to a
live CKB-validated deploy now reads commands that match the shipped
CLI surface and the shipped trust-policy flags.

### Website Highlighting Audit

The website code, syntax, and docs pages received a coordinated
highlighting audit. The audit covers the playground, the docs tree,
and the syntax sample pages; it is a polish-only change and carries
its own Playwright smoke evidence through `website/scripts/`. This is
not a protocol or CKB VM evidence change; it is the public-website
leg of the release.

## Validation Commands

For 0.20 release readiness, run:

```bash
./scripts/ckb_cellscript_acceptance.sh --production --stateful-scenarios
python3 scripts/validate_ckb_cellscript_production_evidence.py <report.json>
```

For a bounded local preflight without a CKB node:

```bash
./scripts/ckb_cellscript_acceptance.sh --compile-only --production
```

Compile-only evidence is useful for checking the ABI and compiler boundary, but
it is not sufficient for external release because it skips the local devnet
dry-run, tx-pool, commit, and live/dead lineage checks.

For website/playground changes, also run:

```bash
website/scripts/build-wasm.sh
(cd website && npm run build)
```
