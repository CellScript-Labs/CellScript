# CellScript ckb-sdk Builder Cookbook

This crate is a cookbook wrapper around the formal CKB adapter crate:

```text
crates/cellscript-ckb-adapter
```

It is intentionally outside compiler core and must not grow a second adapter
implementation.

It demonstrates the boundary:

- CellScript emits an `ActionPlan` through `cellc action build --json`.
- The formal adapter crate materializes a `ResolvedActionTx` with CKB packed
  types.
- `ScriptSpec` constructs arbitrary `ckb_types::packed::Script` values and
  records script hash / args evidence without inventing a second encoding.
- `ScriptRef` reads lock/type scripts back from packed `CellOutput` values so
  adapters can compare live outputs with expected scripts.
- `ScriptCodeDep` binds script code hash / hash type to explicit CellDeps and
  rejects missing or wrong-dep cases before RPC submission.
- `ActionPreview` emits frontend-ready JSON data for consumed inputs, created
  outputs, lineage, witnesses, warnings, and estimated fee without rendering UI.
- `AcceptedActionReport` records cycles, tx-pool acceptance, optional submitted
  tx hash, tx size, occupied capacity, fee, and lineage after node checks.
- `place_entry_witness_payload_before_signing` preserves the existing
  `WitnessArgs.lock` and `output_type` fields while placing the compiler-emitted
  `CSARGv1` payload in canonical `WitnessArgs.input_type`. Call it before any
  lock-script signer because the signature commits to the complete witness.
- `ckb-sdk-rust` owns transaction building, signer integration, RPC cycle
  estimation, tx-pool acceptance, and optional submission.

The placement step is explicit:

```rust
let witness = place_entry_witness_payload_before_signing(
    &base_witness_args,
    EntryWitnessPlacementAbi::WitnessArgsInputTypeV2,
    entry_payload,
)?;
```

`entry_payload` is the raw output of `cellc entry-witness`; the resulting
serialized `WitnessArgs` is the transaction witness. A raw `CSARGv1` payload,
`output_type` alias, or post-signing mutation is not compatible with the 0.23
CKB entry ABI.

The cookbook tests are offline and do not require a running CKB node. Focused
local-node evidence lives in `scripts/cellscript_ckb_adapter_acceptance.sh`.

## Disposable financial node example

On 2026-11-07 the maintainer made external reproduction optional, cancelling
the mandatory named outside developer's clean-checkout report and transaction
evidence. No external reproduction is claimed. Independent security review and
other acceptance requirements remain; see the
[current profile decision](../../docs/CELLSCRIPT_FINANCE_PROFILE.md#ownership-and-acceptance).

`finance_node` runs the public Rust adapter against a temporary pinned CKB node.
It compiles the ordinary packages under `examples/finance`, generates their
metadata and TypeScript SDK manifests, materializes a checked ProtocolBundle,
resolves actual live Cells and code dependencies, signs issuer and holder Lock
groups with the upstream SDK secp256k1 signer, submits, mines, and checks the
committed transaction and confirmation depth. The Rust application reuses
`examples/finance_client` for quotes, immutable economic constraints, and its
durable submission journal.

Prerequisites are the repository toolchain, a locally built `target/debug/cellc`,
the existing sibling `../ckb-sdk-rust` checkout at `v5.1.0`, and the node source
and binary matching `scripts/ckb_acceptance_pin.json`. The example checks the
source revision and binary version, records binary hashes, and uses the actual
genesis **header** hash as the financial network domain. Pass absolute paths
when using a different acceptance output directory:

```bash
cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --ckb-repo /absolute/path/to/pinned-ckb-source \
  --ckb-bin /absolute/path/to/pinned-ckb \
  --cellc /absolute/path/to/cellc \
  --lane complete \
  --run-dir target/finance-node/reference
```

Use a fresh run directory. The defaults use the existing local acceptance
source at `target/source-contract-acceptance/ckb-pinned` and binary at
`target/ckb-cellscript-acceptance/1793922591-production-726194/ckb-runtime/ckb`.
The executable stops its node when it returns. Its deterministic private keys
are disposable local-example keys and must never receive real funds.

The reference flow issues two enforcing assets, initializes a Type ID anchored
pool with genuine LP issuance, swaps in both directions, redeems part and then
all circulating LP, and retains the locked minimum. Two candidate builders
first receive real tx-pool preflight acceptance against the same predecessor;
after one commits, the second receives an actual stale-input rejection. The
unknown-submission case deliberately discards a successful send acknowledgement
and restores only the original transaction's observation capability from disk.
This is explicitly a client fault injection, not a claim that an organic RPC
timeout occurred. Bootstrap mined funding and code deployment use the template
always-success Script; financial assets, reserve custody, and LP authorization
execute their compiled verifiers and real holder signatures.

The complete lane also executes two distinct makers' orders, including
cumulative partial/final fills, donation handling, cancellation and capacity
refunds. It issues a third genuine asset and runs exact-input and exact-output
routes through two different X/Y and Y/Z pools. Public `RouteLeg` quotations
bind complete pool and asset identities; the user supplies no Y inventory.
Both route operations and their authorized unwraps use the checked bundle and
durable journal.

C1 creates another authentic pool and order. A user funded only with A spends
pool A to produce the maker's actual B payment and receives the order's A with
net profit. The combined transaction commits, the user unwraps residual A and
capacity, and the maker cancels the remaining order. C2 authenticates two orders
sharing one aggregate payment of 12, then four credit rows sharing two payments
of 6 each at 32 actual inputs and outputs. A competing plan first passes node
preflight, becomes stale after the other fills, and refunds its four original
Cells with type, data and capacity preserved. The allocation deployment checks
that pinning its own otherwise unused code hash does not change its ELF before
compiling the orders that authenticate it.

`--lane pool-route` omits order, C1 and C2 dependencies while retaining the
pool/LP lifecycle and pure two-pool routes. Funding an intent containing one
asset has one custom Script group; it uses the ordinary action adapter, actual
candidate constraint checks and upstream signing/RPC flow. Its report explicitly
records that this funding step has no ProtocolBundle or durable recovery journal.
Multi-group execution and unwrap use the full checked bundle/recovery path.
Additional pools retain their own circulating LP; the final locked-minimum
check selects the original pool's complete LP Script identity.

An additional profile changes actual issuer and owner keys, fee and reserves:

```bash
cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --lane complete --profile second --fee-bps 50 --pool-nonce 2 \
  --reserve-a 200000 --reserve-b 600000 \
  --run-dir target/finance-node/second
```

`finance-node-report.json` preserves complete signed transactions, input Cell
contexts, compiled source and binary hashes, node admission and commitment
observations, journal references, and available cycles/size/capacity/timing
metrics. The report labels unavailable VM stack measurements explicitly.
Package snapshots, four-file artifacts, generated SDKs, journals, and node logs
are retained in that run directory. These focused runs do not run gates or CI,
freeze complete financial budgets, establish a security review, or substitute
for a developer who did not implement this example. Exact report cases and
compiled source hashes define what each particular run established. Complete
adversarial coverage, all operation budgets and independent developer acceptance
remain separate work; a listed happy-path node flow does not establish them.

## Optional measured resource ceilings

Run the exact reference profile with the reviewed file to enforce the recorded
operational envelope after execution:

```bash
cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --lane complete \
  --resource-limits examples/ckb-sdk-builder/finance-node-resource-limits.json \
  --run-dir target/finance-node/resource-validation
```

The closed JSON schema pins profile parameters, compiler version, node binary,
network identity, every verifier ELF, and each case occurrence/status. Complete
retained signed transaction and witness bytes, capacity, fee and available full
transaction RPC cycle results have integer ceilings. `rpc_cycles` is the largest
available complete-transaction estimate/preflight/commit result in a case; it is
not a sum of RPC work. Every pure quote call has a separate nanosecond sample and
occurrence ceiling. Every successful `Backend::build` has another sample covering
capacity balancing, `ResolvedActionTx` assembly and existing packed materialization.
The latter excludes pre-call metadata data/witness encoding, post-build mutation
and signing. Artifact checks and actual live/dependency RPC resolution remain a
measured aggregate. This seeded profile does not use dynamic indexer discovery.
Unavailable VM stack/memory and separately unmeasured encoding metrics stay null.

Two serialized contender patterns retain monotonic window start/end times.
Each actually calls normal SDK `send_transaction` twice: A commits, and the
same previously signed B is deliberately sent after its predecessor is consumed
and its optional preflight already reports stale. The second send must return
the actual stale/dead-input RPC error. Each records two offered candidates, two
submission attempts, one commit, one submission conflict and zero new economic
retries. These are private-node preflight-bypass experiments, not simultaneous
contenders or network-capacity measurements. The three invalid financial Script
probes still use dry-run only and are never sent.

Ceilings never widen automatically. The declared rules use 25% integer headroom,
three times observed stage duration plus 25 ms, a fixed 1000 ms additive allowance
for durable journal and contention-window experiments, and three times quote/
packing duration plus 1000000 ns. They describe single-host samples with recorded
machine context, not percentiles, timeout mechanisms or a universal throughput
guarantee. A failed resource check preserves `execution_status`, every committed
transaction and all violations, then returns an error. This does not authorize
repeating a committed economic operation.

The [v1 file](resource-limits-history/finance-node-resource-limits-v1.json) and
[explicit v2 decision/history](resource-limits-history/decision-v2.json) preserve
the initial independent timing failures. Only journal/window allowance changed;
all other existing ceilings, including quote/packing limits, were retained.
The second v2 pilot also exceeded three retained stages at the 32-cell case;
those failures were recorded without widening their limits. Functional node
success and `resource_budget_validation.status` must be read separately.

The fresh `reference-operational-v2-validation` run passed all 68 case occurrences
and the frozen measured envelope, with 25 quote and 60 packing/build events.
Its report and hash are recorded in the decision history. This establishes that
particular reference run; the second-profile pilot's three preserved timing
failures remain open evidence against those ceilings.

The earlier 68-case reports hashed the active `target/debug/cellc` path at the
end of a run. That does not prove the same CLI executable served every earlier
lock/builder subprocess while other Cargo builds could replace that path.
Their retained ELF, metadata and actual transaction observations remain evidence
for those exact bytes; continuous CLI provenance is not established by that
late hash. New runs copy an opened compiler executable into the fresh run's
`toolchain/cellc`, use that snapshot for every subprocess, and check its hash
again at completion. On Linux the application hash reads `/proc/self/exe` so a
concurrent build cannot substitute a later executable at the target path.

The separate `--lane operations` experiment exercises three same-shape pool
contention windows, a disjoint-input two-pool control, unsigned re-quotation
after an actual state update, changed funding/fee within the original intent,
same-order fill/cancel conflict, controlled wallet callback failures, an actual
SDK transport timeout and changed inclusion after private-node `truncate`.
The response proxy forwards the original request to the real node, retains its
successful response, then withholds it past the SDK deadline. The observer
follows the same signed original hash after restart. Only the private test driver
rebroadcasts those identical bytes after truncation; no replacement economic
operation or new signature is granted by the observer.

```bash
cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --lane operations --run-dir target/finance-node/operations-pilot
```

`whole_operation_samples` start before quote/argument construction and canonical
data/witness encoding, and cover actual live resolution, packing, adapter checks,
signing, submission and inclusion or an intended late rejection. Named C1/C3
samples include funding and unwrap; their nested phase times overlap. These
samples and the recovery experiment have their own pilot/freeze/validation
namespace. The older v2 ceilings and failed observations are retained separately.
Static identity admission uses no indexer, and missing node VM stack/memory or
rejected-RPC cycle totals retain explicit unavailable reasons.

The new frozen namespaces have separate commands:

```bash
cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --lane complete-c4 \
  --whole-operation-limits examples/ckb-sdk-builder/finance-node-whole-operation-limits.json \
  --run-dir target/finance-node/whole-operation-validation

cargo run --locked -p cellscript-ckb-sdk-builder-example --example finance_node -- \
  --lane operations \
  --operations-limits examples/ckb-sdk-builder/finance-node-recovery-limits.json \
  --run-dir target/finance-node/recovery-validation
```

`complete-c4` adds genuine fourth-party secp inputs with distinct untyped data
and capacities to C1 and both C3 amount modes. It preserves each financial role
prefix, checks full tail-Cell refunds, preflights the canonical and exchanged
tail layouts, and commits only one. C2 uses distinct third-party tail data at
32 inputs/outputs. C1/C3 also dry-run a correctly signed one-shannon fee-overrun
candidate and require rejection by their actual wrapper. This profile has its
own identities and limits; the old `complete`/v2 pass does not cover it.

The two files pin the exact compiler snapshot, running application, compiled
application sources, locked toolchain/SDK inputs, financial ELFs, profile and
sample occurrence inventory. They describe this recorded local build. A clean
rebuild with different executable bytes must record that identity difference
and obtain an explicit reproduction configuration; silently changing hashes or
ceilings is not acceptance. Such configuration work must retain the original
limits and raw results. The new result is `operations_budget_validation`, with
an explicit namespace `kind`; the historical v2 result remains
`resource_budget_validation`.

The [whole-operation freeze history](resource-limits-history/whole-operation-decision-v1.json)
retains pilot identities, the earlier shared scope-label defect and the exact
identity-only correction. All case metrics, availability bindings and numeric
ceilings survived that correction unchanged. A successful business run and a
successful budget validation remain separate outcomes.
