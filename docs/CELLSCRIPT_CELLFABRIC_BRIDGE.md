# CellScript CellFabric Bridge

## Boundary

The CellFabric bridge is a JSON boundary between CellScript action planning and
CellFabric intent ordering. It does not add a Rust dependency from the
CellScript compiler core to CellFabric.

CellScript remains responsible for:

- compiling CellScript source into metadata and artifacts;
- emitting `cellc action build` action plans;
- binding generated builders to package, lockfile, and deployment identity;
- delegating packed CKB transaction materialization to runtime adapters.

CellFabric remains responsible for:

- accepting signed cell-native intents;
- indexing hard consumed-cell conflicts and app-level conflict keys;
- selecting conflict-free bundles;
- issuing explicitly non-final soft-confirmation receipts;
- tracking CKB L1 submitted, rejected, committed, and finalized states.

## CLI

Use `--fabric-intent` to emit a CellFabric intent envelope instead of the raw
CellScript action plan:

```sh
cellc action build examples/token --action mint --target-profile ckb --fabric-intent --json
```

The envelope schema is:

```text
cellscript-cellfabric-intent-envelope-v0.20
```

The envelope embeds the original `cellscript-action-builder-plan-v1` JSON and
records a `source.action_plan_hash` over that plan. CellFabric services should
treat the embedded action plan as the payload to be resolved by a
CellScript-aware runtime builder or `cellscript-ckb-adapter`.

Parent-project CellFabric exposes the matching dev HTTP import boundary as
`POST /cellscript/import` when built with the `http` feature. That endpoint
accepts the envelope plus runtime binding data and returns an unsigned
CellFabric `IntentBody`, an `intent_id`, `action_plan_hash_hex`, and an
operator-facing summary; it does not sign, submit, soft-confirm, or compile CKB
transactions. The HTTP request accepts hex-friendly binding fields such as
`author_lock_script_hash_hex` and `tx_hash_hex`; Rust callers may also use the
typed `CellScriptIntentBinding` shape.

CellFabric gateways that accept imported CellScript app intents should enable
required app-policy validation for app actions and register the imported
`CellScriptAppConflictPolicy` before submission. Otherwise a permissive
admission path may accept an app intent before later bundle or settlement
validation rejects missing policy evidence.

For local contract checks, parent-project CellFabric also provides:

```sh
cargo run --example cellscript_import -- \
  /path/to/cellscript-envelope.json \
  0x0000000000000000000000000000000000000000000000000000000000000000 \
  1
```

The example prints the same unsigned import response as the HTTP endpoint. Use
`--summary-only` before the path for compact CI logs.

For a repeatable cross-repo smoke check from this repo, run:

```sh
scripts/cellscript_cellfabric_bridge_smoke.sh
```

The script builds the CellScript envelope, imports it with sibling CellFabric,
and checks the summary hash, namespace, action, signature requirement, and
non-finality flags. Set `CELLFABRIC_DIR=/path/to/CellFabric` if the sibling repo
is not at `../CellFabric`.

## Non-Claims

The bridge envelope is intentionally not:

- a CellFabric `SignedIntent`;
- an orderer soft confirmation;
- proof of live-cell availability;
- proof of CKB tx-pool acceptance;
- proof of L1 finality.

Runtime services must still add the wallet author binding, nonce, resolved live
outpoints, fee policy, CellFabric auth signature, deployment identity, dry-run
evidence, and L1 status observations.

## Resource Mapping

CellScript compiler metadata can describe resource roles, runtime input
requirements, create sets, mutate sets, verifier obligations, and app conflict
key templates. It cannot know the final CKB `OutPointRef` values until runtime
live-cell resolution.

Therefore:

- `cellfabric_intent_template.resources.consumes` is empty in the compiler
  envelope;
- `resource_access_template.hard_conflicts.consumed_cell_patterns` records the
  CellScript patterns that a runtime resolver must bind to concrete outpoints;
- `cellfabric_intent_template.resources.app_keys` contains deterministic app
  conflict key templates derived from CellScript shared resources, mutate
  bindings, and pool primitives;
- services must recompute or validate app conflict keys before signing a real
  CellFabric intent.

## Recommended Execution Path

```text
CellScript source
  -> cellc action build --fabric-intent
  -> optional CellFabric POST /cellscript/import
  -> service resolves author, nonce, live cells, deployment identity, and fee cap
  -> service registers CellScriptAppConflictPolicy for gateway/orderer validation
  -> service signs CellFabric SignedIntent
  -> CellFabric gateway indexes conflicts
  -> CellFabric orderer selects bundle and signs non-final receipt
  -> CellScript runtime builder or adapter materializes CKB transaction
  -> CKB RPC dry-run / tx-pool / submit
  -> CellFabric tracker records L1 status
```

This keeps the compiler deterministic and keeps CellFabric soft-confirmation
semantics separate from CKB settlement finality.
