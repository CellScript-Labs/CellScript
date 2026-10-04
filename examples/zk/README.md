# Build a ZK counter application

This is the developer entry point for the private-authorization counter: a public
counter advances by one when the caller proves knowledge of its owner's secret.
It demonstrates one circuit with Rust and TypeScript clients. Another relation
needs its own circuit, setup/VK and lifecycle; editing the `.cell` parent alone
cannot turn this counter into a payment or membership application.

## Run locally

Use the repository's Rust 1.97.1 toolchain, the
`riscv64imac-unknown-none-elf` target, Node 22.18+ and npm. The current workspace
also requires `ckb-sdk-rust` at tag `v5.1.0` in a sibling directory:

```bash
git clone --branch v5.1.0 https://github.com/nervosnetwork/ckb-sdk-rust.git ../ckb-sdk-rust
rustup target add riscv64imac-unknown-none-elf
bash examples/zk/run.sh target/my-zk-app
```

The runner builds the scripts, creates a public test setup, verifies creation and
two increments in CKB-VM, generates a TypeScript SDK, then tests the CCC adapter.
It rejects reused/corrupt proofs, changed transactions and overwritten witnesses.
Choose a new output directory for each run. Supply a matching setup package as a
second argument to reuse it. Tutorial secrets and default setup are public test
fixtures; their output is not production admission.

The [Rust walkthrough](../../crates/cellscript-zk-counter-client/examples/counter-walkthrough.rs)
uses the supported `cellscript-zk-counter-client` library. Its chain store is
`ckb-testtool`, with an always-success fixture Lock. Read `walkthrough-report.json`
for VM cycles, and `parent.cell` for the actual generated action. This fixture
run does not submit to a node or establish live deployment availability.

## Rust application API

Add path dependencies on `crates/cellscript-zk-counter-client` and
`contracts/zk-private-counter`. The client is a separate host crate so changes to
wallet/deployment plumbing do not repin the circuit's setup-bound `Cargo.lock`.

```rust
let parent = client::parent(&deployment, &child_elf, &verification_key)?;
// Resolve inputs and complete dependencies, fee inputs and change first.
let prepared = client::PreparedIncrement::new(transaction, &resolved_inputs, &type_script)?;
let proof = counter::prove(&pk, CounterCircuit {
    statement: prepared.statement().clone(), witness,
}, &mut OsRng)?;
let proved = prepared.attach_proof(&parent.compiled, &parent.handle, &verification_key, &proof)?;
let signed = wallet_sign(proved.clone())?; // Your chosen wallet implementation.
prepared.check_signed(&proved, &signed)?;
// Dry-run the signed transaction, submit it and wait for commitment.
```

`PreparedIncrement` derives the statement from the ordered resolved inputs and
final raw transaction, checks the counter transition, verifies Groth16 locally,
and preserves an existing Lock witness while placing the metadata-encoded proof.
`check_signed` rejects raw transaction changes and changes to the counter proof.
The caller's chain provider remains responsible for current liveness and submission.
The executable walkthrough supplies concrete values for every application type;
the wallet call above is an integration point, not a bundled Rust wallet.

## TypeScript / CCC application API

[`client.ts`](client.ts) uses pinned `@ckb-ccc/shell` APIs for live input/code
resolution, genesis and deployment checks, fee completion, statement derivation,
proof placement, signing, node dry-run, submission and confirmation. It accepts a
CCC `Signer`, including a connector wallet's signer. The generated SDK supplies
the exact codecs, parent identities and witness encoder.

```typescript
const prepared = await prepareIncrement(signer, counterOutPoint, deployment, sdk);
const proved = await prepared.prove(prover);
const { hash } = await prepared.signAndSend(signer, proved);
```

`prepareIncrement` reserves the entire proof/handle payload before CCC calculates
fees and prepares wallet dependencies. The returned snapshot cannot be mutated.
The prover receives the finalized statement and counter values. Signing uses
`signOnlyTransaction`; a wallet that requires raw changes must return to preparation
and generate a new proof. Extra Lock dependencies can be supplied in `lockDeps`.
The counter's capacity and Lock stay unchanged; fees come from separate Cells.

For a node script with a local prover and a secp256k1 wallet, use
[`increment.ts`](increment.ts):

```bash
cargo build --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml
node --experimental-strip-types examples/zk/increment.ts /path/to/config.json
```

Configuration paths resolve from the current working directory:

```json
{
  "rpcUrl": "http://127.0.0.1:8114",
  "sdkDirectory": "/path/to/generated-sdk",
  "proverExecutable": "/path/to/cellscript-zk-private-counter",
  "setupPackage": "/path/to/setup-package",
  "ownerSecretFile": "/path/to/owner.bin",
  "walletKeyFile": "/path/to/wallet.hex",
  "counter": { "txHash": "0x<live counter transaction hash>", "index": 0 },
  "deployment": {
    "genesisHash": "0x<genesis hash>",
    "child": { "outPoint": { "txHash": "0x<hash>", "index": 0 }, "dataHash": "0x<child hash>" },
    "verificationKey": { "outPoint": { "txHash": "0x<hash>", "index": 0 }, "dataHash": "0x<VK hash>" },
    "parent": { "outPoint": { "txHash": "0x<hash>", "index": 0 }, "dataHash": "0x<parent hash>" },
    "lifecycle": { "outPoint": { "txHash": "0x<hash>", "index": 0 }, "dataHash": "0x<lifecycle hash>" },
    "handle": "0x<exact-handle.bin bytes>"
  }
}
```

Replace all angle-bracket placeholders with the observed deployment identities.
The owner secret is 32 binary bytes from the native `new-secret` command; the
wallet key is a separate 32-byte hex key. `localProver` uses a private temporary
directory, invokes the native prover without secret command-line arguments,
and removes its witness files on completion. Browser apps supply their own prover
callback; secrets should stay within their chosen prover trust boundary.

The CLI's default known scripts are testnet deployments. For a custom chain,
construct a CCC client with that chain's `scripts` configuration and call the API;
[`node-client.ts`](node-client.ts) demonstrates this with the local genesis secp
script and DAO definition; its wallet limits address discovery to secp because
the integration chain has no ACP deployment. No public deployment is included in the example.

## Export the exact parent

After deploying the child, write a `Deployment` JSON with `chain_id`,
`genesis_hash`, `child_tx_hash`, `child_index`, `child_data_hash` and
`verification_key_hash`. Hashes are 32-byte hex strings; unknown fields reject.
The VM walkthrough exports a complete `deployment.json` as a format example,
with synthetic coordinates that must not be reused on a live chain.

```bash
cargo run --locked --release --manifest-path crates/cellscript-zk-counter-client/Cargo.toml \
  --example export-parent -- deployment.json child.elf SETUP_PACKAGE NEW_OUTPUT_DIR
```

The exporter checks both binary identities and emits source, ELF, compiler
metadata, named deployment configuration and exact handle. Generate the SDK from
that metadata using `cellc gen-builder --target typescript`. Deploy the emitted
parent and lifecycle before creating an instance. A new child outpoint changes
the handle and requires a new parent even when the child bytes are unchanged.
The exporter binds supplied coordinates; the CCC client checks them against live
Cells and genesis. Neither operation replaces setup admission.

## Verify against a real node

After the repository's pinned CKB acceptance harness has produced a passing
`target/ckb-cellscript-acceptance/latest-production.json`, run:

```bash
CELLSCRIPT_CKB_REPO=/path/to/pinned/ckb \
CELLSCRIPT_COUNTER_PACKAGE=/path/to/setup-package \
CELLSCRIPT_COUNTER_CCC=1 \
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml \
  --test node counter_node_acceptance -- --ignored --nocapture
```

This starts a fresh disposable node, deploys code/VK, funds a real secp wallet fee
Cell and calls the shipped CCC adapter. It checks two confirmed client updates,
a corrupt proof's dry-run rejection and a spent input's preflight rejection.
The harness mines blocks; normal `send_transaction` and script verification remain
active. `passthrough` refers only to CKB's output-policy validator. Results are in
`target/counter-node/<run>/ccc/ccc-report.json`. Public fixture keys are used only
on that chain. Install example dependencies and generate `target/debug/cellc`
with the local runner first.

## Diagnostics and remaining DX limits

| Failure | Client response |
| --- | --- |
| Wrong genesis, child or VK | Names the mismatched deployment before proving |
| Spent/missing input | Resolve a new live Cell before preparing |
| Changed fee, output or dependency | Finalize again and generate a new proof |
| Wallet overwrites proof | Preserve `WitnessArgs.input_type`; sign Lock fields |
| Wrong owner, overflow or skipped count | Reject the application transition locally |
| Stale public inputs or short proof | Reject before witness insertion |
| Cryptographic/node rejection | Preserve the node cause; parent 79 alone cannot identify why the child rejected |

The counter integration now has a reusable native library and a complete CCC
transaction path. Deployment still involves several code Cells, setup policy is
an explicit developer responsibility, and new circuits remain specialist work.
This is adequate for building a counter dApp, not a one-command arbitrary-ZK app
scaffold. TypeScript byte binding is not Groth16 verification: native verification
and the node's signed dry-run establish that separately. Concurrent spending can
still invalidate a prepared transaction; re-resolve and reprove instead of retrying
it with changed raw fields.

Keep this guide for development, the [counter reference](../../contracts/zk-private-counter/README.md)
for circuit/setup/admission details, and the [profile specification](../../docs/CELLSCRIPT_ZK_PROFILE.md)
for exact wire and compiler contracts. Acceptance records remain under
[`docs/reports/0.32`](../../docs/reports/0.32/ZK_ACCEPTANCE.md).
