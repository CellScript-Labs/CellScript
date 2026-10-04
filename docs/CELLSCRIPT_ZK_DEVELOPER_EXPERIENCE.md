# ZK application developer experience

Assessment from the executable [Rust and TypeScript walkthroughs](../examples/zk/README.md),
October 4, 2026. **The composition layer works; the application experience still
requires CKB and Rust expertise. It is not yet a complete dApp starter.**
Passing #22's first-profile acceptance does not imply that a wallet application
can be built by writing only a `.cell` file.

## What a developer actually owns

An application currently spans an external circuit/prover, setup PK/VK, the
CellScript parent, an application lifecycle Type Script and an off-chain
transaction builder. The first profile has reusable exact identity, statement,
codec, Spawn/IPC/Wait and checker machinery. Its counter relation and lifecycle
remain application-specific.

CKB transactions consume Cells and create successors; scripts validate that
transaction. Fees and Lock authorization also belong to that transaction. This
matches the [official Cell Model](https://docs.nervos.org/docs/ckb-fundamentals/cell-model)
and explains why a proof alone does not construct an application transaction.

The walkthrough succeeds in creating a counter and advancing it twice in the
real CKB-VM scheduler. Corruption and replay fail with parent error 79. The
TypeScript client imports a generated package and matches the real Rust proof's
statement/public-input/request bytes, while showing that `planIncrement` still
requires runtime resolution. It intentionally does not pretend to implement a
wallet, RPC provider or public deployment.

One local run after compilation took 4.4 seconds inside the Rust walkthrough:
about 2.0 seconds for test setup and about 1.0 second per proof. The two updates
used 110,023,294 and 110,209,180 VM cycles, each with a 128-byte proof and a
917-byte fixture transaction. These are one-machine observations, excluding
Cargo builds, SDK generation and any network/wallet work; they are not latency
or worst-case guarantees. Generated ZK and ordinary Token SDK packages both
passed TypeScript compilation and all ten generated tests.

## Friction observed

| Developer step | Current experience | Assessment |
| --- | --- | --- |
| First successful local run | `examples/zk/run.sh NEW_DIR` now builds and runs both examples; Rust/Node and the RISC-V target remain prerequisites | Adequate for a repository tutorial |
| Author the parent | Short action, but four large commitment literals and an explicit CellDep index are generated through Rust helpers | Too much identity plumbing for ordinary application authoring |
| Start a different application | Counter circuit, setup and lifecycle cannot express another relation by editing the parent | A new circuit remains expert work; do not market an arbitrary-circuit starter |
| Generate a usable SDK | Generation originally succeeded but importing a schema-free package crashed on `entries.map` | Fixed together with generated tests that incorrectly assumed every artifact exposes a shared schema; generation alone was insufficient evidence |
| Build and prove | Finalize dependencies, fees and outputs first; derive the statement; prove; insert witnesses; sign without changing raw fields | Correct sequence is easy to violate without a transaction-state API |
| Connect a TS wallet | Codec functions and generated action plans exist; live resolution, raw hashing, witness construction, fees and wallet adapter are still caller work | Incomplete for a frontend developer |
| Diagnose a failed proof | Corrupt and replayed proofs both surface as parent code 79 | Stable rejection, but little application-level diagnosis |
| Export/deploy | Parent commitment depends on exact child outpoint; current exporter takes seven positional arguments | Discoverability and deployment workflow need work |
| Reuse existing code | Tutorial and exporter import `tests/support/mod.rs` for parent/deployment/witness builders | Evidence plumbing has not become a supported library |
| Understand guarantees | SDK binding accepts a corrupt, correctly sized proof with matching public inputs; on-chain verifier rejects it | API names/docs must distinguish byte binding from cryptographic verification |

The new generator regression test loads an actual schema-free TypeScript SDK
and creates an action plan, then executes its generated test suite. Schema-free packages now test rejection of an absent shared schema instead of requiring a nonexistent one. The fix treats omitted empty schema entries as an
empty list, without changing serialized identities. The examples exercise the
same fix with a real ZK parent. The earlier codec vectors could pass without loading a complete generated SDK;
that coverage gap explains why the import defect survived first-profile checks.
CI now runs the developer walkthrough as well. No cryptographic, RISC-V or
lifecycle rules change.

The current walkthrough has 147 Rust lines and calls a 139-line shared fixture
module; the TypeScript consumer adds 53 lines. Those counts include comments,
reporting and negative cases, so they are not a minimum-size or productivity
benchmark. They do show why a short `.cell` action alone understates integration
work.

## Priorities for the next DX work

These are proposed follow-ups for the application work associated with #42;
they are not features implemented by this walkthrough.

1. **Publish one supported counter client library.** Move exercised parent,
   deployment, statement and witness builders out of tests; use typed inputs and
   actionable errors. Acceptance: an external application crate can execute the
   same flow without importing repository test files or copying their code.
2. **Make transaction finalization explicit.** A prepared transaction should carry
   exact resolved Cells/dependencies, hash, proof statement and witness slots;
   attaching a proof must recheck the raw hash. Acceptance: changing any raw field
   after proof creation forces reproving, while adding permitted witnesses works.
3. **Provide one complete TS/CCC adapter.** Resolve the chain, live inputs and
   exact deployment, reserve fees, generate/attach proof, obtain a wallet signature,
   dry-run, submit and await confirmation. Acceptance: a fresh local-node run needs
   no handwritten Molecule or witness offsets. CCC is the current official
   [TypeScript dApp entry point](https://docs.nervos.org/docs/sdk-and-devtool/ccc).
4. **Replace positional deployment export with a checked manifest flow.** Keep
   exact outpoint and VK checks, but generate the matching named dependency and
   source commitments from one manifest. Acceptance: substitution fails with the
   mismatched field and expected identity named in the error.
5. **Add preflight diagnosis.** Distinguish changed transaction, stale outpoint,
   wrong VK, malformed proof and cryptographic rejection when the evidence permits
   it. Acceptance: each tutorial negative produces an actionable client message
   while preserving the on-chain rejection code; never guess a cryptographic cause.

A useful next milestone is: a developer supplies an admitted profile, an owner
secret and a node connection, then completes a confirmed counter update without
reading compiler internals. The present examples make the local path reproducible
and its missing pieces visible; they do not yet meet that milestone.
