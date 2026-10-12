# ZK lifecycle planning

Implementation contract for #45. The native client and independent TypeScript
consumer provide bounded off-chain lifecycle planning; this does not admit a new
circuit, setup or deployment. Implementer and implementation reviewer: Codex.
Review disposition: the new surface compares exact off-chain identities and
uses the unchanged checked application to identify its existing authorization
path; it adds no proof system, circuit relation or on-chain ABI. Rebound identity,
trust, authority and stale-plan tests cover that boundary. No external human
review is claimed.
The maintainer requested no CI or gate runs for this work on 2026-10-12.
Targeted tests remain required and are distinct from integration acceptance.

## Manifest and trust boundaries

`cellscript-zk-lifecycle-v1` binds an application/version, source and compiled
relation commitments, statement and codec identities, proof profile, PK/VK,
prover executable/build/toolchain, setup record and optional transcript, verifier
bytes, network, parent, code OutPoint, complete application Script and package
lock/deployment record. The reader requires an externally trusted SHA-256 of the
exact JSON bytes. Unknown fields, oversized records, malformed canonical hashes
and malformed Molecule Scripts/OutPoints reject. Reading a manifest does not
verify that its circuit implements its advertised relation.

The 32,768-byte manifest contains commitments only. Compilation and planning
never load PKs or invoke a prover. Runtime-artifact preflight independently
checks supplied VK, verifier, parent and prover bytes. Live code-Cell resolution
remains a provider/transaction preflight obligation, not an offline liveness
claim. Public-test setup, local-single-party setup and unreviewed setup remain
separate dispositions; a lifecycle plan never admits any of them.
Versions are bounded to 0–9,007,199,254,740,991 so the native and TypeScript
consumers preserve exact integer identities. Optional transcripts must be
present as an explicit null when unavailable.

## Change decisions

Reports deterministically retain every changed dimension. Source-only and
proving-only changes do not by themselves change verification of an existing
proof over the identical statement. Statement, codec, VK, relation, profile,
verifier or deployment changes require reproving under the candidate contract.
A different final raw transaction always needs a new proof, including changes
to dependencies: apparent interface compatibility does not waive this binding.
Setup/discovery changes are reported separately from proof validity.

Existing Cells remain governed by their old deployed Scripts. A changed runtime
requires an existing authorization path; planning cannot create one. The legacy
immutable counter has no retrofit migration. The paired migratable counter has
only its pinned old-parent-authorized 0 -> 1 switch, as specified in
[the application contract](../contracts/zk-private-counter/MIGRATION.md).

Candidates must be active, must not decrease the numeric version, and must
advance it for any changed manifest. Yanking is a discovery restriction and
does not revoke consensus spendability. Recomputing a plan against current
trusted manifests rejects stale snapshots and altered decisions. When locks
change, an existing `cellscript-upgrade-plan-v1` receipt is mandatory. Its schema,
compiler and hash are validated and its selected member's exact old/new lock
bytes must match the lifecycle manifests. It retains its lock-only authority.
The deployment bridge consumes the existing `DeployedManifest` representation
and pins SHA-256 of its canonical JSON projection, rather than its TOML
formatting. It checks chain/genesis, actual parent hash and code OutPoint,
redundant artifact/build identities and explicit active status for candidates.
Existing Cell rules remain unchanged if discovery later deprecates or revokes a
record. At most 64 deployment rows and 262,144 encoded bytes are accepted.

`CheckedDeployment::lifecycle_manifest` exports the actual checked migration
versions' circuit, keys, setup, child, parent and instance identities. Prover
build provenance and package records remain explicit caller inputs.
`assess_lifecycle_migration` rechecks a plan against those private checked
deployment identities and identifies the old-parent proof path for selector
0 -> 1. Another VK, codec, relation, parent or code OutPoint cannot substitute
even after the caller recomputes every outer manifest/plan digest. This report
does not establish chain authorization: the existing prepared transaction flow
still checks configuration, proof, custody, signing and liveness. The legacy
counter disposition explicitly rejects retrofit migration.

## Targeted reproduction

Run the native producer tests first, then the independent TypeScript consumer:

```bash
cargo test --locked --release --manifest-path crates/cellscript-zk-counter-client/Cargo.toml --test lifecycle --test migration
cd examples/zk
npm run test:lifecycle
npm run check
```

The migration test consumes the existing application transaction fixtures under
`contracts/zk-private-counter/target/`. Their producer is the focused migration
test in `contracts/zk-private-counter`; no gate execution is needed to generate
them. The lifecycle producer emits eighteen independently asserted change-class
vectors plus exact manifest bytes and a report for the real checked migration
versions. The TypeScript implementation derives the same decisions separately.

To inspect two externally authenticated version manifests:

```bash
npm run lifecycle -- OLD.json TRUSTED_OLD_SHA256 NEW.json TRUSTED_NEW_SHA256
```

An optional fifth argument is an existing package upgrade-plan JSON receipt.
This command prints a report; it neither applies the plan nor signs a transaction.
Do not compute trust merely by hashing an arbitrary downloaded manifest.

## Execution evidence and scope

The focused CCC node replay executes the unchanged published application on
`16b9e583`, with real fee signatures and continuous confirmed predecessor
lineage. The new native planner consumes that run's exact manifest, public-test
VKs, actual prover bytes and deployed code, reconstructs both parents, checks
the deployment records and identifies the old proof's authorization path.
The independent TypeScript consumer recomputes classification and deployment
decisions. This is component-composed evidence with explicit source identities;
it is not a new combined CI or release-gate claim. See the
[acceptance record](reports/0.32/zk-lifecycle/README.md).

The admitted migration remains the exact fixed pair. Other circuit relations,
statements, codecs, VKs and deployments are classified, but cannot acquire an
authorization path merely by appearing in a change report. The checked
application assessment rejects substitutions even after outer digests are
rebound. A new on-chain relation or setup needs separate implementation/admission.
#44's multi-Cell statement and #46's host/application measurements are separate
unfinished tracks; #42's existing P1 evidence does not substitute for them.
