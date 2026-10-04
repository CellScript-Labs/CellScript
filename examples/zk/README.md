# Build a ZK-backed CellScript application

These two examples use the existing private-authorization counter circuit:

1. **Rust application walkthrough:** create a counter, prove `0 -> 1` and `1 -> 2`,
   execute the lifecycle/CellScript parent/Groth16 child in CKB-VM, and reject a
   corrupt proof and a replayed proof.
2. **TypeScript integration:** import an actual generated SDK, consume the first
   walkthrough's proof, reproduce the Rust statement/public inputs/request,
   reject stale public inputs, and inspect the generated action plan.

They are one application with two integration examples, not two different
circuits. The owner secret is private in the protocol; this tutorial deliberately
uses a public test secret. Counter values and owner commitment remain public.

## Run both

Use a checkout of the `0.32` development branch with Rust 1.97.1 and Node supporting
`--experimental-strip-types` (tested with Node 22.23.1). The published 0.31 CLI
and browser bundle do not expose this new profile. From the repository root:

```bash
rustup target add riscv64imac-unknown-none-elf
examples/zk/run.sh target/my-zk-app
```

Choose an output directory that does not exist. A first build downloads/builds
locked dependencies; subsequent runs reuse Cargo caches. No RPC, wallet, Docker,
public deployment or existing PK is needed. The script builds the child and
lifecycle binaries, generates a **public test setup**, runs the Rust example,
generates a TypeScript SDK and executes the TypeScript example.

To exercise a previously generated, matching setup package instead:

```bash
examples/zk/run.sh target/my-zk-app-candidate /absolute/path/to/setup-package
```

Passing a candidate package does not make these tutorial transactions deployable:
the owner secret and proof randomness are still public fixtures. It does not
rewrite existing admission reports. Do not use the tutorial's keys or custody
for real assets.

The final lines should report matching Rust/TypeScript bytes and
`canSubmit=false`. Both report files must say `"status": "passed"`.

## Example 1: the application

Read [`counter-walkthrough.rs`](../../contracts/zk-private-counter/examples/counter-walkthrough.rs).
Its application flow is:

1. Choose the **relation**, not just a proof system: know the owner secret, preserve
   the owner commitment, increment exactly once, reject u64 overflow. The pinned
   Rust circuit implements this relation; CellScript does not compile a circuit.
2. Bind the exact verifier deployment and VK. The shared fixture builder produces
   the named dependency configuration and calls `compile_with_zk_deploy`.
3. Create a zero-valued Cell. A separate lifecycle Type Script controls creation,
   owner/Lock/capacity preservation, unique successor and non-burning. It EXECs
   the generated CellScript parent on an update.
4. Finish the raw transaction, including all dependencies, before deriving the
   statement. It binds domain, action, Script, old/new data, consumed outpoint and
   the complete raw transaction hash.
5. Generate and locally verify the Groth16 proof, encode the action's entry
   witness from compiler metadata, then verify the complete transaction in CKB-VM.
6. Resolve the resulting Cell and repeat. Reusing the previous proof must fail.

Inspect the artifacts:

```bash
cat target/my-zk-app/parent.cell
cat target/my-zk-app/named-deployment.json
cat target/my-zk-app/walkthrough-report.json
```

`parent.cell` is runnable generated source, including the four exact commitment
literals. Its `zk::require_valid` call passes `ZkTransitionProof`, an
`ExactScriptHandle`, the named policy and an explicit dependency index. The
compiler cross-checks that index against the named deployment declaration; the
on-chain parent checks the actual bytes and enforces Spawn/IPC/Wait success.
The generated JSON is `CkbDeployConfig` for the native API, not a `Cell.toml` file.

The example exports `parent.elf`, full compile metadata, the exact handle,
`statement.bin`, `proof.bin`, the first `transaction.bin`, `bridge.json`, and
`walkthrough-report.json`. The default run also writes its public test PK/VK.

The shared deployment/source/witness helpers currently come from
`contracts/zk-private-counter/tests/support/mod.rs`. This is deliberate reuse of
the exercised fixture implementation, **not a stable application SDK**. The
walkthrough exposes that dependency rather than copying it into another API.

This run uses the real CKB-VM scheduler and real Groth16 verification, but its
Cell store and deployments come from `ckb-testtool`. It does not prove mempool
admission, confirmation, live Cell availability or fee balance. Its fixture
Lock is always-success, with no wallet signing. The synthetic receipt network
and genesis identify fixture data, not a queried chain.

## Example 2: a TypeScript client

Read [`accept-proof.ts`](accept-proof.ts). The runner first executes:

```bash
cargo run --locked -p cellscript --bin cellc -- gen-builder \
  --target typescript --metadata target/my-zk-app/parent-metadata.json \
  --output target/my-zk-app/sdk
node --experimental-strip-types examples/zk/accept-proof.ts target/my-zk-app
```

For normal package compilation and the generated SDK's own tests, run:

```bash
npm --prefix target/my-zk-app/sdk install --ignore-scripts
npm --prefix target/my-zk-app/sdk test
```

The client imports `sdk/src/index.ts`, so this checks that the **whole generated
package loads**, not just that a copied codec works. It:

- compares the 228-byte statement, 484-byte public-input encoding and 464-byte
  verifier request with the Rust output;
- accepts the real 128-byte proof produced by Example 1;
- rejects the old public inputs after changing the transaction hash;
- rejects a truncated proof;
- demonstrates that a corrupt proof can still pass the SDK's byte-binding check;
  only the cryptographic verifier determines proof validity;
- calls `planIncrement` and confirms that its plan cannot yet submit.

The statement fields in `bridge.json` were derived by the Rust host from the
finalized fixture transaction. They are **not independently resolved by this
TypeScript client**. A real application must resolve live Cells/deployments,
recompute its own statement, provide the runtime transaction/witness adapter,
reserve capacity and fees, sign and dry-run the final transaction before sending.
Never adopt a prover-supplied statement as the transaction's authority.

The 464-byte request is the parent/child IPC envelope; do not put it directly in
`WitnessArgs`. The parent needs its metadata-defined action witness containing
the proof and exact handle. The Rust example uses `entry_witness_args` for that.

## Changing the application

Changing an instance's owner or initial transaction does not require a new circuit.
Changing the relation (for example an age threshold, membership proof or a private
payment) does: implement and test that external circuit, create/import its setup,
pin its VK and identities, and design its lifecycle and replay bindings. Reusing
the generic composition path is possible; reusing this counter VK for another
relation is not.

Reuse is limited to the current v2 statement and its 15 public-input limbs.
Additional public inputs, a different proof system or a different wire format
require a separately supported profile. A new circuit/VK also needs its own
application admission; it does not inherit the counter's acceptance evidence.

For a real counter instance, use an OS-random owner secret and secure prover
randomness, a trusted setup policy, exact live code/VK deployment receipts and
appropriate custody/fee inputs. The [application guide](../../contracts/zk-private-counter/README.md)
contains the native CLI and node acceptance commands. The current profile rejects
burning, so this example is not a recoverable deposit or payment application.

See the [DX assessment](../../docs/CELLSCRIPT_ZK_DEVELOPER_EXPERIENCE.md) for the
remaining integration work and the bug this walkthrough uncovered.
