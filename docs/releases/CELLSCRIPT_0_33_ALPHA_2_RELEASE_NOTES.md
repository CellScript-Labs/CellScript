# CellScript 0.33.0-alpha.2

Release date: 2026-11-05. This second 0.33 alpha uses Edition 2027 as its
sole source-language contract. Language and public APIs may change during the
alpha series. The [source contract](../CELLSCRIPT_SOURCE_CONTRACT.md) defines
the current compiler, artifact and consumer boundary.

## Included changes

- Native compilation, package manifests, language services and browser metadata
  use Edition 2027. Ordinary actions, locks, lifecycle operations and explicit
  or omitted `verification` share one checked grammar; native `type_script`
  and `lock_script` containers retain their bounded authoring rules.
- Current artifact bundles use metadata 75, lowering records v12, typed
  semantics v12, semantic foundation v4 and package interface v6. Operation
  records describe actual dispositions and current policy boundaries. Omitted
  public visibility is represented as `implicit-public`; public generic value
  layouts require explicit fixed, serializable, non-linear parameter bounds.
- Deployment manifests use `cellscript-deployed-v2` with numeric version 2;
  generated action builders use `cellscript-generated-action-builder-v1`.
  Each reader requires the current shape. Source edition, compiler release,
  artifact schemas, target, assurance policy and witness ABI remain independent
  identities.
- Structured effect facts describe context reads, lifecycle operations and
  process operations. The compiler infers them and the standalone checker
  reconstructs them independently from typed operations and callees.
- The structured-type work retains source applications, unsigned constant
  arguments and specialization origins through the relevant compiler phases.
  Imported compatible-open interface roles are checked in their defining
  source scope. This is a checkpoint toward
  [#96](https://github.com/CellScript-Labs/CellScript/issues/96), whose broader
  representation and identity work remains incomplete.
- LSP diagnostics and semantic hover share manifest-aware captured inputs,
  including unsaved package files and their imports. Invalid package manifests
  cannot produce successful semantic metadata through a standalone fallback.
  Revision checks prevent stale analysis publication.
- Machine admission binds internal PIPE success paths, SPAWN status guards,
  return ABI classes, single-image ELF layout and return-address slots to the
  decoded artifact. Fixed-element structural vector fields retain their checked
  byte-length span storage; unsupported dynamic-element shapes remain
  fail-closed. Unterminated literals/comments and incomplete pool assignments
  produce source diagnostics.
- The browser WASM bundle, website data, editor tooling, generated builders,
  Registry readers/verifiers, adapters, examples and proposal consumers are
  aligned with the current source and wire contracts. Current VM observations
  refresh fixture sidecar identities while preserving executable/transaction
  identities, measured budgets and rejection expectations where those values
  are unchanged.

## Packages and rebuilt artifacts

Every participating `Cell.toml` declares the source edition and an explicit
compiler requirement. For an exact compiler pin:

```toml
[package]
edition = "2027"
cellscript_version = "=0.33.0-alpha.2"
```

Review the package graph, then use `cellc lock` for an intentional repin.
Build/check/test validate existing pins; `--frozen` additionally forbids network
access and writes. Rebuild metadata, lowering records, source maps, interfaces
and generated builders together with their matching compiler/checker. Browser
metadata compilation remains separate from native ELF admission.

## Validation and publication scope

The maintainer explicitly waived further gates and CI for the 2026-11-05
publication at `f7a7a0f6458dec594f75bd0e2c8cd65fa914a1e2`. Its release body and
attached receipt record the interrupted backend run and unexecuted final-node
checks. This authorization applies only to that publication.

The following gates passed on the compiler/checker implementation described
here, before this documentation refresh:

```bash
./scripts/cellscript_gate.sh commit
./scripts/cellscript_gate.sh dev
./scripts/cellscript_gate.sh ci
```

Backend and production/stateful acceptance have their own evidence requirements.
Use the release's matching source-commit receipts for
`./scripts/cellscript_gate.sh backend` and pinned CKB acceptance results;
compile-only checks and the gates above do not establish those results.
No `release` or `release-quick` gate result is claimed here. See the
[gate policy](../CELLSCRIPT_GATE_POLICY.md) for the required evidence.

Each release attachment must identify the same reviewed source commit and
carry its checksum and matching validation receipts. Updating GitHub release
attachments does not establish crates.io publication, website deployment or
Marketplace publication; those distributions require their own evidence.

## Retained boundaries

`ExactCellSelection` is reserved. Default-build alias evidence, general nominal
representation and the remaining Cell/selection family work stay open under
#96. Open-interface projection does not gain a new nested-view ABI. Unbounded
vectors with non-fixed elements remain outside the admitted codec, and deferred
runtime semantics fail closed with their recorded errors.

Finite source, checker and VM tests do not prove arbitrary program equivalence
or application-specific production safety. The research equivalence line keeps
`equivalence_status = NOT_PROVEN` and `production_equivalence_claim = false`.
The [alpha.2 backlog](CELLSCRIPT_0_33_ALPHA_2_TODO.md) records the remaining
owned work and acceptance requirements.
