# CellScript CKB Adapter

**Status**: design contract for the Rust-side builder, deployment, and
acceptance bridge.

See also
[`CELLSCRIPT_CKB_ECOSYSTEM_REUSE_AUDIT.md`](CELLSCRIPT_CKB_ECOSYSTEM_REUSE_AUDIT.md)
for the audit of which CKB-facing responsibilities belong to CellScript,
`ckb-std`, `ckb-sdk-rust`, and the adapter.

CellScript is the semantic compiler. `ckb-std` is the contract-side ABI/runtime
oracle. `ckb-sdk-rust` is the transaction realiser. The adapter is the boundary
object between compiler outputs and real CKB transactions.

In practical terms:

```text
CellScript emits verified transaction intent.
cellscript-ckb-adapter realises that intent through ckb-sdk-rust.
CKB node acceptance is the production evidence.
```

The compiler should stay focused on artifacts, metadata, ABI, deployment plans,
action plans, witness bytes, and CKB constraints. The adapter should use those
outputs to construct, sign, preflight, validate, and optionally submit real CKB
transactions with machine-readable evidence.

## Boundary

The compiler core must not depend on `ckb-sdk-rust`.

Keeping the SDK out of compiler core preserves offline compilation, metadata
inspection, static checks, package workflows, and future non-CKB target profiles
without dragging in CKB RPC, indexer, signing, or node-version concerns.

The split is:

| Layer | Responsibility |
|---|---|
| `cellc` compiler | Parse, type-check, lower, emit artifact, metadata, ABI, constraints, action build plan, entry witness bytes, deploy plan. |
| `ckb-std` | Provide the contract-side Rust reference for CKB syscalls, sources, witnesses, TYPE_ID, since, and exec/spawn semantics. |
| `cellscript-ckb-adapter` | Load compiler outputs, verify hashes and schemas, resolve deployments, materialise CKB transaction shape, attach evidence. |
| `ckb-sdk-rust` | Provide CKB data structures, RPC/indexer access, cell collection, CellDep resolution, signing, fee/capacity helpers, tx-pool acceptance, submission. |
| CKB node | Estimate cycles, accept or reject the transaction, and provide the chain-facing evidence boundary. |

This avoids making CellScript a wallet, indexer, signer, or submission layer.
It also avoids pretending that compiler success is the same as node acceptance.

## Inputs And Outputs

The adapter consumes compiler-side records:

```text
compiled artifact bytes
CompileMetadata
cellc action build JSON
cellc entry-witness bytes
cellc deploy-plan JSON
cellc lock-deps JSON
constraints.ckb
```

It should emit chain-side records:

```text
DeploymentManifest
ActionPlan
ResolvedActionTx
AcceptedActionTx
AcceptanceReport
LiveOutputLineage
```

Every adapter-owned JSON/TOML record must include an explicit `schema` and
`version`. Schema drift must fail closed. The adapter should never silently
reinterpret metadata emitted by a newer compiler schema.

## First Implementation Shape

Start as an example crate before promoting a public library:

```text
examples/ckb-sdk-builder/
```

After the shape is proven, promote the stable subset to:

```text
crates/cellscript-ckb-adapter/
```

Do not start with a framework. Start with cookbook-grade examples that complete
real deployment and transaction acceptance loops.

## Deployment Probe

The first useful adapter flow is code-cell deployment:

```text
CellScript artifact binary
+ deploy-plan
+ constraints.ckb
      |
      v
CKB code cell deployment transaction
      |
      v
deployment manifest + evidence
```

The output manifest should bind the CellScript artifact to the on-chain code
cell:

```toml
schema = "cellscript-ckb-deployment-manifest-v1"
version = 1

[script]
name = "identity-token"
artifact_hash = "7efaa134..."
data_hash = "0x..."
code_hash = "0x..."
hash_type = "type"
type_id_args = "0x..."
cell_dep = { out_point = "0x...:0", dep_type = "code" }

[evidence]
occupied_capacity_shannons = 12300000000
tx_size_bytes = 1024
tx_hash = "0x..."
output_index = 0
acceptance = "test_tx_pool_accept"
```

Hash fields must stay distinct:

- `artifact_hash` is the CellScript compiler artifact hash.
- `data_hash` is the CKB code cell data hash.
- `code_hash` is the value later used in `Script.code_hash`.
- when `hash_type = "type"`, `code_hash` is the type script hash for the
  deployed code cell, not the data hash.

The deployment probe answers the production question: "How do I know this
on-chain script cell is the CellScript artifact that was compiled and audited?"

## Action Transaction Materialisation

The second flow turns one action plan into one CKB transaction candidate:

```text
cellc action build JSON
+ entry-witness bytes
+ deployment manifest
+ live-cell inputs
      |
      v
ResolvedActionTx
      |
      v
cellc validate-tx
+ estimate_cycles
+ tx-pool acceptance
      |
      v
AcceptedActionTx
```

Use three distinct states:

| State | Meaning |
|---|---|
| `ActionPlan` | Compiler-side semantic plan. No live cells, no final deps, no signing. |
| `ResolvedActionTx` | Adapter-side CKB transaction with selected cells, outputs, outputs_data, witnesses, CellDeps, capacity evidence, and change policy. |
| `AcceptedActionTx` | Node-facing acceptance result with cycles, tx size, tx hash when submitted, and any rejection diagnostics. |

`cellc action build` remains a semantic plan. The adapter turns that plan into a
chain transaction. Node acceptance is the reality check.

## Validation Loop

A production adapter flow should be:

```text
cellc action build
  -> adapter materialise
  -> cellc validate-tx
  -> ckb-sdk-rust estimate_cycles
  -> ckb-sdk-rust test_tx_pool_accept
  -> optional ckb-sdk-rust send_transaction
  -> acceptance_report.json
```

If a workflow uses `dry_run_transaction`, the adapter must expose an explicit
RPC wrapper and report that exact method. Otherwise reports should say
`test_tx_pool_accept`, `estimate_cycles`, or `send_transaction` instead of
using "dry run" as a loose synonym.

The acceptance report should include at least:

```text
package hash
metadata hash
artifact hash
deployment ref
action selector
input and output bindings
witness layout
CellDeps and HeaderDeps
cycles
serialized transaction size
occupied capacity
fee and change policy
tx-pool acceptance result
submitted tx hash, when submitted
old output -> new output lineage
known limitations
```

## Capacity And CellDeps

Capacity is transaction-specific. The compiler exposes floors and evidence
requirements through `constraints.ckb`; the adapter must compute actual
occupied capacity for the concrete `CellOutput` and `outputs_data` it builds.

The adapter should use CKB packed transaction and capacity APIs for measurement,
not local approximations. Under-capacity outputs must be rejected before
signing.

CellDep resolution must come from deployment records and SDK resolvers. The
adapter must verify that declared hash type, code hash, dep type, out point,
data hash, and Type ID lineage match the compiler metadata and deployment
manifest.

## Witnesses

CellScript entry witness bytes are compiler-owned ABI output. The adapter may
call `cellc entry-witness` or the Rust metadata helper, but it must not invent a
parallel witness encoding.

Final CKB witnesses still belong to the transaction builder. The adapter must
place CellScript entry witness bytes inside the correct `WitnessArgs` field and
leave lock signatures explicit. It must not assume hidden signer authority.

## `solve-tx`

`cellc solve-tx` is a planning and debugging helper. It is not a chain
transaction builder.

It does not perform:

- live-cell collection;
- concrete CellDep or HeaderDep resolution;
- fee/change calculation;
- occupied-capacity measurement;
- final witness placement;
- signing;
- tx-pool acceptance;
- submission.

For real CKB transaction construction, use the CKB adapter example or the later
`cellscript-ckb-adapter` crate.

## Minimal API

The first library surface should stay small:

```rust
load_compile_metadata(path) -> CompileMetadata
load_action_plan(path) -> ActionBuildPlan
load_deployment_manifest(path) -> DeploymentManifest

deploy_artifact_with_type_id(...)
build_action_transaction(...)
emit_acceptance_report(...)
```

Internal modules can exist without becoming stable public API:

```text
ArtifactVerifier
DeploymentBuilder
ActionTxBuilder
WitnessBuilder
CapacityEvidenceBuilder
AcceptanceRunner
```

The public API should remain smaller than the cookbook. Most early value should
come from concrete, inspectable examples.

## Cookbook Order

Initial cookbook topics should be narrow and executable:

```text
01_deploy_cellscript_artifact_with_type_id.md
02_build_action_transaction_from_action_plan.md
03_bind_outputs_and_outputs_data.md
04_resolve_celldeps_from_deployment_manifest.md
05_calculate_occupied_capacity.md
06_generate_entry_witness_bytes.md
07_validate_tx_against_cellscript_metadata.md
08_run_tx_pool_acceptance.md
09_emit_acceptance_report.md
```

These are more important than a broad framework guide. CKB developers need to
see exactly how a real transaction is assembled, measured, accepted, and
reported.

## Non-Goals

- Do not make compiler core depend on `ckb-sdk-rust`.
- Do not replace `ckb-sdk-rust`.
- Do not replace CCC or wallet connectors for TypeScript and browser workflows.
- Do not infer protocol semantics from action names such as `mint`, `claim`, or
  `swap`.
- Do not hide signer authority or sighash defaults.
- Do not mark a deployment mainnet-certified without external audit and chain
  evidence.
- Do not treat package registry resolution as deployment verification.
- Do not treat builder success as CKB node acceptance.

## External Positioning

CellScript does not compete with `ckb-sdk-rust`. It gives CKB developers a
higher-level verifier specification layer with ABI, metadata, witness, action
plans, and constraints. `ckb-sdk-rust` remains the Rust infrastructure for
transaction construction and chain interaction.

CellScript also does not replace `ckb-std`. The CKB backend should stay
compatible with `ckb-std` at the contract-side ABI boundary: syscall numbers,
source encoding, witness handling, TYPE_ID, since, occupied capacity, and
exec/spawn semantics. See
[`CELLSCRIPT_CKB_STD_COMPAT.md`](CELLSCRIPT_CKB_STD_COMPAT.md) for that
compatibility contract.

That is the intended production workflow:

```text
CellScript tells builders what the transaction must mean.
ckb-std tells contract authors what CKB runtime reality means.
ckb-sdk-rust helps builders make it real.
The CKB node proves whether it is accepted.
```
