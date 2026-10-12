# ZK lifecycle acceptance evidence

Scope: issue #45's bounded off-chain manifest comparison and the existing
old-parent-authorized fixed-pair migration. The
[implementation contract](../../../CELLSCRIPT_ZK_LIFECYCLE.md) separates source,
proving, statement, codec, relation, profile, VK, setup, verifier, deployment,
discovery and package identities. No new relation, on-chain ABI, setup trust,
public deployment or immutable-Cell retrofit is admitted.

The maintainer explicitly requested no CI or gates on 2026-10-12. None were run
for this work. Targeted results below do not assert fresh integration or release
acceptance. The earlier acceptance receipt used to start the unchanged node
application is identified separately from the new off-chain implementation.

## Source and artifact identities

`manifest.json` records exact source-file SHA-256 values and the raw/stored
hashes and sizes of each deterministic gzip archive (`mtime = 0`). The native
and TypeScript source snapshots are distinct from the published application
source `16b9e58394aa182f071303e6b5ab15dc94d304f4` used for the node execution.
The application, circuit, verifier, compiler and CCC migration flow were
unchanged at that replay. The new planner consumes its retained exact public
VKs, prover executable, manifest and deployed code; it reconstructs both
parents and binds the results to the node/CCC evidence.

This is component-composed evidence, not a clean-source execution of the new
planner on the node. The planner does not execute on chain. It cannot convert
a discovery report into spending permission. Public-test setup remains
non-production, and its absent transcript is recorded as null.

## Acceptance mapping

| Requirement | Executable evidence |
| --- | --- |
| Deterministic manifest/change decisions | Eighteen native change-class vectors; independent TypeScript recomputation; CLI equality over exact node manifest bytes |
| Package bridge and stale plans | Existing upgrade-plan schema/compiler/digest and selected member's exact old/new lock bytes; rebound lock/member/schema substitutions and forged/stale decisions reject |
| Deployment bridge | Thirteen rebound record cases, exact parent/OutPoint/genesis/profile checks, candidate status and discovery revocation separation; 64 rows accepted, 65 and oversized records rejected |
| Circuit/key/codec/prover/verifier substitutions | Manifest canonicality, artifact preflight and private checked-deployment assessment; recomputing outer digests cannot replace the fixed pair's checked identities |
| Continuous state/owner lineage | Confirmed old-key update, old-key-authorized migration and new-key successor update; actual predecessor linkage, fee signatures and distinct VK/parent identities |
| Invalid on-chain transitions | Targeted migration test executes all groups for unauthorized/new-key migration, wrong owner, missing/duplicate successors, old-proof replay, wrong destination, rollback and repeated migration |
| Immutable legacy disposition | Native legacy disposition test reports unsupported retrofit when runtime changes; the application regression rejects legacy migration |
| Resource bounds | Existing pre-frozen 64/64/64 Cell envelope and 16,384-byte maximum, 250,000,000-cycle budget and separate 32,768-byte observed VM stack ceilings; targeted maximum migration replay |

The node's three successful transactions used respectively 111,811,931,
112,070,818 and 111,992,278 cycles, with 1,237, 1,632 and 1,237 serialized bytes.
Each has a real fee signature and confirmation. A plan still reports
`chain_authorization_established = false`; execution evidence is separate.
The migration resource report retains each VM/EXEC generation's stack peak and
occupied capacities. Heap peaks remain explicitly unavailable, not zero.

## Targeted reproduction

Use the pinned toolchain and already-built exact application artifacts. Run
the producer before its native/TypeScript consumers:

```bash
cargo test --locked --release --manifest-path contracts/zk-private-counter/Cargo.toml --test migration -- --nocapture
cargo test --locked --release -p cellscript --test zk_resources exact_migration_resource_replay -- --ignored --nocapture
cargo test --locked --release --manifest-path crates/cellscript-zk-counter-client/Cargo.toml --test lifecycle --test migration
cargo clippy --locked --release --manifest-path crates/cellscript-zk-counter-client/Cargo.toml --all-targets -- -D warnings
cargo fmt --manifest-path crates/cellscript-zk-counter-client/Cargo.toml -- --check
cd examples/zk
npm run test:lifecycle
npm run check
```

The separately identified clean application checkout ran the existing focused
`counter_node_acceptance` test with `CELLSCRIPT_COUNTER_CCC=1` and the pinned CKB
checkout. It reused an already-passing source receipt; it did not run a gate.
The first attempt hit the existing asynchronous local-node block-template race;
the second lacked the checkout's Node dependencies. Neither is a passing run.
The third completed successfully after the dependency installation was supplied.

To consume a retained run, set `CELLSCRIPT_ZK_LIFECYCLE_NODE_EVIDENCE` to the
directory holding its exact public artifacts and run the client crate's ignored
`lifecycle_node` test. Set the same variable for `npm run test:lifecycle` to
execute the independent node-manifest comparison, otherwise that one test is
explicitly skipped. The report archives contain commitments and public evidence;
large executables, proving keys and owner/wallet secrets are not committed.

Review disposition: Codex implemented and reviewed this off-chain identity
boundary using native and independent-consumer mutation tests. No independent
human review is claimed. Cryptographic/setup admission remains the existing
profile's separate contract. #44's protected multi-Cell set and #46's controlled
host proving comparison remain separate work.
