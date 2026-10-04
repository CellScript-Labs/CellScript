# Private authorization counter

This application uses the v2 ZK composition profile to authorize a single CKB
Cell's counter update. The witness proves knowledge of a 32-byte secret. The
counter and its owner commitment are public; this is private authorization,
not confidential state or a payment protocol.

Start with the [application walkthrough](../../examples/zk/README.md) for a
single command that runs the counter in CKB-VM and imports its generated
TypeScript SDK. It includes corruption/replay negatives and a concrete
[developer-experience assessment](../../docs/CELLSCRIPT_ZK_DEVELOPER_EXPERIENCE.md).

## Relation and lifecycle

Cell data is exactly 48 bytes: `CSZKCNT1 || owner[32] || counter_u64_le`.
`owner = CKB_BLAKE2b256("CS-counter-key-1" || secret)`.
The circuit computes that commitment and both old/new Cell data hashes inside
R1CS. It requires the same owner and `new_counter = old_counter + 1`, with no
u64 overflow. Native prechecks are convenience checks; R1CS also enforces each
rule. The circuit has 149,179 constraints and 15 public inputs. Its canonical
matrix digest is recorded in every setup manifest.

The v2 statement binds domain, action, current Script hash, both data hashes,
the consumed outpoint and the full raw transaction hash. The canonical byte
packing constrains every public-input limb. Changing capacity, recipient,
dependencies or any other raw transaction field invalidates an existing proof.
Finalize the raw transaction before proving; witnesses are filled afterwards.

The Rust lifecycle Type Script has exactly 64 argument bytes:
`type_id[32] || parent_data_hash[32]`. Initialization checks the standard Type ID
rule and counter zero. Updates preserve owner, capacity and Lock Script, then
EXEC the exact generated CellScript parent. That parent derives the statement
from CKB syscalls, checks the literal child handle/VK identities, and uses
Spawn/IPC/Wait to run the pinned Groth16 child. EXEC preserves Script context.
There is at most one input/output in the group. Burning is deliberately rejected;
the Cell's capacity cannot be reclaimed under this version. Fees require a
separate input. At `u64::MAX` no further update is possible.

An always-success Lock is used in tests to demonstrate that the Type Script
itself enforces private authorization. A deployed instance must use the exact
exported parent commitment, lifecycle hash and chosen Type ID. Creating another
instance with a different parent produces a different Script identity; clients
must not accept it as this instance. Code/VK availability remains a deployment
responsibility; the test chain's disposable code Cells are not production
custody arrangements.

## Build and use

Run from the repository root with its pinned Rust 1.97.1 toolchain:

```bash
cargo build --locked --manifest-path contracts/zk-transition-verifier/Cargo.toml --target riscv64imac-unknown-none-elf --release
cargo build --locked --manifest-path contracts/zk-private-counter/script/Cargo.toml --target riscv64imac-unknown-none-elf --release
cargo build --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml
```

The host executable is
`contracts/zk-private-counter/target/release/cellscript-zk-private-counter`.
Its commands are:

```text
identity
setup-local DIR
inspect DIR
new-secret FILE
state SECRET_FILE COUNTER OUTPUT
prove PACKAGE STATEMENT_BIN WITNESS_JSON PROOF_OUTPUT
check-repro FIRST_BUILD_DIR SECOND_BUILD_DIR OUTPUT
bundle PACKAGE SCHEDULER_JSON NODE_JSON REPRO_JSON OUTPUT
admit PACKAGE EVIDENCE_JSON --trust-local-setup
```

`setup-local` uses the operating system random source. It exports the proving
key, 744-byte VK and a manifest binding their hashes, the constraint matrices
and Cargo.lock. PK/VK are public artifacts; no setup seed is exported. The
proof is 128 bytes. `new-secret` creates a 0600 file on Unix without printing its
contents. `prove` accepts the exact 228-byte statement and a JSON object with
`secret` (32 byte values), `old_counter`, and `new_counter`. Protect that JSON
as a secret. Existing output files are not overwritten.

After deploying the exact child, export its deployment-specific parent:

```bash
cargo run --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml \
  --example export-parent -- CHAIN_ID GENESIS_HASH CHILD_TX_HASH CHILD_INDEX \
  PACKAGE OUTPUT_DIR EXPECTED_CHILD_DATA_HASH
```

This writes the parent ELF, compile metadata, encoded ExactScriptHandle and
binding identities. It does not attest that a supplied outpoint is live.
`tests/support/mod.rs` contains the same transaction/statement/witness builders
used by the scheduler and node suites. CellDep index 0 is the child; the exact
VK, parent and lifecycle code must also be available. Script hash type `data2`
is used on the node. A fresh deployment outpoint changes the exact handle and
therefore requires a new parent, even if the child ELF bytes are identical.

## Evidence and admission

```bash
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml -- --test-threads=1
bash contracts/zk-private-counter/reproduce.sh
CELLSCRIPT_CKB_REPO=/path/to/pinned/ckb \
  cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml \
  --test node counter_node_acceptance -- --ignored --nocapture
```

The node suite requires a passing `latest-production.json` from the repository's
CKB acceptance harness. It hashes that report and its archived CKB binary,
checks the pinned clean node checkout/version, and starts a new integration
chain. `CELLSCRIPT_COUNTER_CKB_BIN` may select a binary only if it matches that
receipt. State transactions use normal `send_transaction` with the passthrough
**output-policy validator**, not `send_test_transaction`; script verification
and block commitment still run. Code-cell deployment uses the existing test
harness helper. This is local node admission, not public-network deployment.

Tests normally generate a deterministic **public test setup**, which the
admission command rejects. Set `CELLSCRIPT_COUNTER_PACKAGE=/path/to/package`
for both scheduler and node tests to exercise an OS-random candidate VK instead.
Reports are written beneath this crate's ignored `target/`; save candidate
reports before running the default public-seed suite again. Reproducibility
compares complete child and lifecycle ELF bytes from two independent Cargo
target directories under the same pinned sources/toolchain.

`bundle` binds candidate circuit/VK, scheduler, node and reproduced ELF hashes.
`admit` requires the matching single-party candidate, successful creation and
two committed updates, replay rejection, cycle budgets and passing reproduction.
Admission is scoped to this application under the recorded trust model:
**the setup operator and host are trusted; erasure is not independently proven**.
It is not a multi-party ceremony, an independent audit, or a mainnet deployment.
A manifest hash alone cannot prove that a malicious parameter generator used
the declared relation. Use a separately reviewed ceremony/import workflow if
that local trust assumption is unacceptable; this crate does not implement MPC.
Run the repository `dev`, `ci` and `backend` gates for release readiness as well.

Stable lifecycle errors: 90 context, 91 args, 92 state/owner/lock/capacity,
93 nonzero creation, 94 burn, 95 Type ID/group uniqueness, 96 parent dependency,
97 EXEC failure. Child rejection appears as parent error 79. The hard call
budget remains 250,000,000 cycles.

## Package and resource closure

The [accepted profile](../../docs/CELLSCRIPT_ZK_PROFILE.md) defines the named
`Cell.toml` policy. Parent generation now validates that policy through the same
compiler path as packages; export also includes source and `named-deployment.json`.
`reproduce.sh` compares parents from two fresh compiler processes as well as
two independent build directories for the child/lifecycle. Deployment coordinates
for the parent reproduction fixture are synthetic.

The application test exports `target/resource-fixture.json`. The root
`zk_resources` ignored test replays it with the maintained diagnostic scheduler,
requires exact agreement with the normal group verifier, and writes
`target/resources.json` in this crate. That report distinguishes per-VM/EXEC
stack observations from allocator budgets. It runs explicitly in all three gates.
