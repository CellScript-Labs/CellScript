# CellScript 0.21 Release Candidate Notes

**Status**: Stable release notes for CellScript 0.21.0.

**Updated**: 2026-07-03.

CellScript 0.21 is a semantic-closure and tooling release candidate. It
promotes the most common aggregate invariant shape into executable verifier
lowering, makes flow transitions statically accountable, adds authenticated
compile receipts, and gives auditors and agents a canonical command and MCP
surface.

This is not a production CKB release claim by itself. Production claims still
require the repository `ci`, backend, and release gates, plus the CKB acceptance
evidence described in `docs/CELLSCRIPT_GATE_POLICY.md`.

## Aggregate Invariant Lowering

The compiler now recognises xUDT group amount conservation invariants of the
form:

```text
assert_sum(group_outputs<T>.amount) == assert_sum(group_inputs<T>.amount)
```

When an action has the matching consumed-to-created amount evidence, codegen
auto-lowers the action prelude to call
`__xudt_require_group_amount_conserved`. ProofPlan records expose three
coverage states:

- `metadata-only`: the compiler records an obligation but emits no executable
  helper;
- `runtime-helper-required`: the invariant maps to a helper, but no matching
  generated runtime access covers it;
- `checked-runtime`: the generated helper access covers the invariant.

Strict `0.17` metadata validation now rejects stale helper gaps instead of
accepting module-level runtime accesses as a silent discharge.

## Flow Edge Validation

Declared `flow` blocks now define the valid state-transition edge set for a
flow-aware type. Actions that claim a transition on the flow state field must
use a declared edge. Cyclic flow edges remain valid when explicitly declared.

This is a static compiler contract. It does not introduce new runtime
transition codegen by itself.

## Builder And Adapter Resolution

The CKB adapter can resolve materialised `ActionPlan` JSON into transaction
candidates. The new resolution path validates action scan selector evidence,
supports variable-length script args through `args_parts`, and can use a
deployment manifest to complete matching CellDeps.

Missing or mismatched selector evidence fails before a `ResolvedActionTx` is
constructed. A compile-only `ActionPlan` is still not a submitted transaction
or a chain acceptance claim.

## Authenticated Compile Receipts

`cellc receipt`, `cellc sign-receipt`, and `cellc verify-receipt` introduce a
`cellscript-compile-receipt-v1` envelope over compiler evidence:

- compiler and metadata schema versions;
- source and source-content hashes;
- ProofPlan, ProtocolGraph, and TemplateLayout hashes;
- artifact and metadata hashes;
- optional Ed25519 signatures for `compiler` and `publisher` roles.

The receipt deliberately records `ast_normalised_hash` and
`ir_normalised_hash` as `null` with
`normalisation_status = "ast-ir-normalised-hashes-not-yet-emitted"`. Source,
metadata, report, and artifact identity are bound in this RC; canonical AST/IR
normalisation remains future work.

## CLI And Diagnostic Surface

The public command tree now uses canonical nested command groups:

```bash
cellc explain profile|proof|assumptions|generics|graph
cellc tx validate|solve|trace
cellc deploy plan|verify|diff|lock-deps
cellc registry verify|add|edit
cellc package verify
cellc auth capability create|submit|revoke
```

Legacy flat aliases remain executable for compatibility, but they are hidden
from public help and command-list output.

Diagnostics can now be emitted as structured JSON with
`--message-format=json`, while successful command payloads continue to use
`--json`. Colour handling is explicit through `--color=auto|always|never` and
respects `NO_COLOR`.

## ProtocolGraph And TemplateLayout Metadata

`cellc explain graph` derives a `cellscript-protocol-graph-v0.21` view from
existing metadata. It records action-derived state transitions, type-pattern
edges, proof-plan links, runtime accesses, builder assumptions, and cycle
detection. The graph is an audit view, not a new consensus layer.

Compile metadata now includes `template_layouts` records for resource, shared,
and receipt types. These records are metadata-only in this release candidate:
layouts are flat, cyclic state machines require a root policy marker, and
`consensus_checked = true` is rejected until a backend verifier enforces a
TemplateLayout commitment.

## MCP And Skill Pack

The new `cellscript-mcp` binary exposes a read-only MCP JSON-RPC server over
stdio. It delegates compiler facts to `cellc` and bounded project
documentation instead of becoming a second compiler or deployment client.

The repository also ships six CellScript programming skills under
`docs/skills/`. The unified dev, CI, and release-auxiliary gates run
`scripts/check_cellscript_skill_pack.py` to ensure the skill pack still points
at current docs and command names.

## Release-Candidate Validation Hardening

The 0.21 RC validation boundary was tightened after the initial candidate cut:

- Added focused regression coverage for every new 0.21 contract: flow-edge
  membership (undeclared/cyclic/linear edges, create-state contract, duplicate
  and out-of-range states), xUDT conserved lowering and the three ProofPlan
  coverage states, TemplateLayout `RootRequired`/`PathOnlyAllowed` assignment
  and `consensus_checked=true` rejection, CKB adapter `args_parts`,
  manifest-backed CellDep resolution, and fail-closed scan-selector evidence.
- Added two non-production business-flow examples (`atomic_swap.cell`,
  `multi_phase_dao.cell`) under `examples/` to exercise flow-edge validation,
  state transitions, and cross-module composition end-to-end. They are not part
  of the production bundled-contract deployment matrix.
- Extended the syntax-combination audit with three new governance bug classes
  (`SCA-BUG-FLOW-EDGE-UNDECLARED`, `SCA-BUG-FLOW-CREATE-STATE-CONTRACT`,
  `SCA-BUG-AGGREGATE-INVARIANT-CONTRACT`).
- Added the 0.21 schema tokens (`cellscript-template-layout-v0.21`,
  `cellscript-protocol-graph-v0.21`, `cellscript-action-scan-selectors-v0.21`)
  to the gate's acceptance-boundary audit.
- Removed tautological registry tests that re-implemented comparison logic
  inline instead of exercising the production verification path, and trimmed
  unreachable dead code from `scripts/cellscript_ckb_release_gate.sh`.

## Deferred Work

The following remain out of the 0.21 RC production scope:

- Template Merkleisation backend and consensus-checked TemplateLayout
  commitments;
- new observation syntax;
- canonical AST and IR normalised receipt hashes;
- any production CKB claim without the release gate and acceptance evidence.

## Validation Boundary

Routine local work should still pass:

```bash
./scripts/cellscript_gate.sh dev
```

Merge-readiness requires:

```bash
./scripts/cellscript_gate.sh ci
```

Backend-affecting changes require:

```bash
./scripts/cellscript_gate.sh backend
```

Production CKB release claims require:

```bash
./scripts/cellscript_gate.sh release
```
