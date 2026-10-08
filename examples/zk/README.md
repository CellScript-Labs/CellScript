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
It also runs the paired migration's contract/native fixtures and generates both
SDKs for CCC byte-parity and rejection tests in the output's `migration/` directory.
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
let mut prepared = client::PreparedIncrement::new(transaction, &resolved_inputs, &type_script)?;
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
It checks against the privately retained result of `attach_proof`, so replacing
both caller-supplied comparison arguments cannot authorize a different proof.
An unsuccessful proof replacement invalidates the prior signing state; attach
a valid proof again before signing.
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
  "proverSha256": "<64 lowercase hex digits from the trusted native build>",
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
The required `proverSha256` approves the executable bytes. Record that digest
from a reviewed, pinned build; do not recompute it from an untrusted download
at invocation time. Each invocation checks a bounded snapshot (at most 64 MiB)
before reading the owner secret and executes those checked bytes from the
private directory, so replacing the original path cannot switch the program
after validation. A changed tool requires an explicit configuration update.
The local-node test harness approves its freshly built public fixture tool and
records its digest; that is not an external release attestation. This pin does
not attest the host OS, dynamic libraries, Node runtime or a custom callback.
The local process is asynchronous, has a 120-second timeout and bounded output,
and its captured diagnostics are not included in thrown errors because a prover
could print private witness data. To cancel, call
`prepared.prove(prover, { signal: controller.signal })` with an `AbortController`.
Custom callbacks receive that signal as their third argument and should stop
their underlying work. Even if a callback ignores cancellation, a late result
cannot become the accepted proof or replace a successful retry. Only one proof
request per prepared transaction may be active at once.

Cancelled or failed proving leaves the prepared transaction unsigned. Retry
`prove` against the same finalized transaction if its inputs remain live;
otherwise resolve the intended successor and prepare a new transaction and
proof. The adapter checks live inputs and deployment before invoking the wallet
and again before dry-run/submission. These checks cannot prevent another
transaction spending an input immediately afterward. The adapter retains its
own accepted proof witness and rejects caller-side proof substitution even when
the supplied comparison arguments agree. Only Lock witness fields may change.

The CLI's default known scripts are testnet deployments. For a custom chain,
construct a CCC client with that chain's `scripts` configuration and call the API;
[`node-client.ts`](node-client.ts) demonstrates this with the local genesis secp
script and DAO definition; its wallet limits address discovery to secp because
the integration chain has no ACP deployment. No public deployment is included in the example.

## One authorized migration

This uses a separately created paired counter/configuration instance. The legacy
immutable counter cannot migrate. The [migration contract](../../contracts/zk-private-counter/MIGRATION.md)
defines the two pinned parents and the one allowed 0 -> 1 switch. The circuit
still proves the owner's secret and one increment; migration also consumes and
recreates the configuration in that same proof-bound transaction.

The native `migration::CheckedDeployment` and CCC
[`MigrationDeployment`](migration.ts) consume the same
`cellscript-counter-migration-v1` manifest. It includes both setup records,
code OutPoints/data hashes, network and COMPLETE instance Scripts. Pin the exact
manifest bytes from the trusted application build; computing a new digest from
an arbitrary download does not authenticate it. The two generated SDK modules
and exact handles are trusted client code from the corresponding parent builds.
Export each parent with the existing exporter, using its version's child and
setup package, then generate its SDK. Changing the manifest's pins is a new
application trust decision, not an automatic discovery operation.

`prepareMigration(signer, counterOutPoint, configurationOutPoint, deployment,
migrate, lockDeps)` checks both versions' live code/VK Cells before proving. It
reserves the selected parent's proof witness, prepares the wallet and fees, and
checks the complete final transaction. `migrate = false` uses a configuration
dependency; `true` consumes selector 0 and produces selector 1, authorized by
parent 0. The returned `selectedVersion` chooses the matching prover package.
Counter output 0 and migration configuration output 1 are fixed by this builder;
a wallet that changes that order must re-prepare. Other output/fee changes must
finish before proving. The low-level constructor can validate the contract's
other permitted layouts against the final transaction.

Both state capacities and Locks are preserved. Separate wallet Cells pay fees;
include any required state/configuration Lock dependencies explicitly. Before
wallet invocation and again before dry-run, the client checks fresh inputs,
configuration dependencies and both versions' artifacts. Node consensus remains
authoritative if another transaction consumes a Cell after those observations.
Cancellation, bounded pinned prover execution, private witness handling and
proof/signature snapshots use the same flow as the original counter client.

For a testnet client with a local prover and secp wallet:

```bash
node --experimental-strip-types examples/zk/migration-cli.ts /path/to/migration-config.json
```

The CLI configuration has these fields (paths resolve from the current directory):

```json
{
  "rpcUrl": "http://127.0.0.1:8114",
  "manifestFile": "/path/to/manifest.json",
  "manifestDigest": "0x<trusted manifest CKB data hash>",
  "sdkDirectories": ["/path/to/parent-0-sdk", "/path/to/parent-1-sdk"],
  "handles": ["0x<parent-0 exact handle bytes>", "0x<parent-1 exact handle bytes>"],
  "counter": { "txHash": "0x<live counter transaction hash>", "index": 0 },
  "configuration": { "txHash": "0x<live configuration transaction hash>", "index": 1 },
  "mode": "migrate",
  "lockDeps": [],
  "proverExecutable": "/path/to/cellscript-zk-private-counter",
  "proverSha256": "<trusted native prover SHA-256>",
  "setupPackages": ["/path/to/setup-0", "/path/to/setup-1"],
  "ownerSecretFile": "/path/to/owner.bin",
  "walletKeyFile": "/path/to/wallet.hex"
}
```

Use observed, matching identities for every placeholder and supply the Locks'
required dependencies. Select `update` explicitly for an ordinary increment.
After confirmed migration, use the returned counter and configuration OutPoints
and the new selected prover package for the next update. The prepared hash is
printed to stderr before signing; reconcile it after an uncertain submission.
A spent configuration or already-selected version 1 cannot trigger an automatic
second migration. The CLI does not create or deploy an instance, change trust
pins, or admit a setup. For a custom chain, use the API with its actual CCC script
configuration as shown by [the node exercise](node-migration.ts).

The standalone fixture runner accepts a new output directory:

```bash
bash examples/zk/run-migration.sh target/my-migration-check
```

It builds the scripts, generates contract fixtures and native manifest/parent
exports, then checks the generated SDKs and CCC adapter. By default its setup
parameters and owner secret are public test fixtures. To reuse matching setup
packages, set `CELLSCRIPT_COUNTER_PACKAGE` and optionally
`CELLSCRIPT_COUNTER_MIGRATION_PACKAGE`. This is runtime/byte-parity evidence;
the node exercise below establishes commitment and stale-Cell rejection.

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
It additionally creates a paired instance and confirms old-key update, old-key-authorized migration and new-key successor update. Wrong-key proofs reach the node and reject at parent 79; spent configuration and second-migration attempts reject. The three signed transactions record cycles and byte sizes in `ccc-migration/ccc-migration-report.json`.
The harness mines blocks; normal `send_transaction` and script verification remain
active. `passthrough` refers only to CKB's output-policy validator. Results are in
`target/counter-node/<run>/ccc/ccc-report.json`. Public fixture keys are used only
on that chain. Install example dependencies and generate `target/debug/cellc`
with the local runner first.

## Diagnostics and remaining DX limits

### Privacy and transaction linkage

The protected witness is the 32-byte owner secret. The owner commitment and
counter are public Cell data; the input outpoint, successor, capacities, Locks,
dependencies, fee inputs and final raw transaction are public as well. The
statement binds the Script hash, old/new data hashes, consumed outpoint and raw
transaction hash. Its public-input encoding does not hide those values.
Successive states remain linkable through the consumed outpoint and instance
Script, and funding/fee inputs can link the application to the signing wallet.
Migration additionally exposes both parent identities, the configuration selector, its consumed/successor OutPoints and the exact switch transaction. It does not break the existing instance or wallet linkage.
Proof validity is therefore a secret-knowledge authorization claim, not an
anonymity, hidden-balance or hidden-counter claim.

The local prover process and its host see the secret. Temporary witness files
are removed on success/failure; this is not verified secure erasure and does not
remove the caller's original secret file. A remote/browser prover callback has
its own trust and logging boundary. Default walkthrough setup and secrets are
public fixtures. Circuit correctness, setup trust, proof validity and these
privacy properties must be assessed separately.

### Recovery boundaries

Before calling `signAndSend`, retain `prepared.transactionHash` with the user
intent. A dry-run error stops this attempt before submission; inspect its cause
to distinguish execution rejection from an unavailable RPC result. An exception from
submission or confirmation can leave the outcome unknown: reconcile that exact
hash with the node before preparing a replacement increment. A timeout does not
prove that the transaction failed. The adapter does not automatically retry a
different transaction or authorize an additional increment.

An already spent input must be resolved against the application's intended
instance and expected counter before preparing again. The client deliberately
does not discover an arbitrary successor or silently move the user's intent to
another instance. Changing raw fields requires a fresh statement/proof; retrying
interrupted proving against an unchanged, still-live transaction is supported.

| Failure | Client response |
| --- | --- |
| Wrong genesis, child or VK | Names the mismatched deployment before proving |
| Missing/wrong prover SHA-256 | Reject before reading the owner secret; check the approved build/configuration |
| Spent/missing input | Resolve a new live Cell before preparing |
| Changed fee, output or dependency | Finalize again and generate a new proof |
| Wallet overwrites proof | Preserve `WitnessArgs.input_type`; sign Lock fields |
| Wrong owner, overflow or skipped count | Reject the application transition locally |
| Stale public inputs or short proof | Reject before witness insertion |
| Cancelled/failed proving | Invalidate signing state; re-run proving before signing |
| Uncertain submission/confirmation | Reconcile the saved transaction hash before a new increment |
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
