# One authorized verifier migration

Implementation contract for #42 P1. This is a new application lifecycle under
development, not an admission or deployment of the migration profile. The
existing `cellscript-counter-lifecycle` remains immutable and non-burnable;
its existing Cells cannot be converted by changing client configuration.

## Authority and exact identities

Two immutable `data2` Scripts implement the new profile. Its manifest must pin
both actual ELF data hashes, both exact generated parents and their child/VK
receipts, the circuit relation/setup dispositions, network and complete instance
Scripts. An arbitrary Script with the same argument length is not an admitted
configuration guard. This is a contract/application change, with no new
CellScript syntax, open handles or generic runtime-selected roles.

The counter Script has exactly 64 argument bytes:
`counter_type_id[32] || configuration_script_hash[32]`.
Its state stays `CSZKCNT1 || owner[32] || counter_u64_le` (48 bytes). The accepted
counter circuit and its increment relation are unchanged. Initialization creates
counter zero; every later transition preserves owner, Lock and capacity, and
increments exactly once without overflow under the selected parent proof.

The configuration Script has exactly 160 argument bytes:
`config_type_id[32] || counter_type_id[32] || counter_code_hash[32] || parent0_data_hash[32] || parent1_data_hash[32]`.
The parent hashes must be distinct and nonzero. Its data is exactly
`CSZKCFG1 || selector_u8 || parent_data_hash[32]` (41 bytes). The selector is 0
or 1 and its hash must equal the corresponding immutable argument. Both groups
allow at most one input and exactly one output, and reject burning.

Both Type IDs use the serialized first transaction input and their respective
global output indices, as enforced by pinned `ckb-std 1.1.0`. Construct the
configuration Script first, using the already-known counter ELF hash and Type
ID; then put its complete Script hash into the counter args. The configuration
guard matches the counter's complete code hash, `data2` hash type and both args,
not just a Type ID prefix. There is no circular hash construction.

Creation must create both unique Cells in one transaction, with selector 0 and
counter zero. It cannot attach a new configuration to an existing counter.
Configuration Lock and capacity are immutable during its one allowed spend.

## Updates and migration

An ordinary update selects exactly one matching live configuration Cell from
resolved CellDeps, with no matching configuration inputs or outputs. A migration
consumes the configuration and recreates it with selector 1, while consuming and
recreating the same counter instance. It permits no duplicate configuration in
CellDeps. Selection is explicit; there is no input-versus-dependency fallback.

The counter always authorizes the migration using the **old input configuration's
parent 0**, whose proof binds the entire final raw transaction, including the
configuration successor. The new parent cannot authorize its own installation.
Migration increments the counter once; it does not reset the value or owner.
Subsequent updates use parent 1. Selector 1 cannot be spent again, rolled back,
or used to authorize another migration. The configuration remains observable as
a dependency. Fees require separate inputs; neither state Cell funds them.

The configuration guard requires the actual associated counter input and
successor. This is a required peer Script group, not a peer call: CKB consensus
must execute both groups successfully. The counter group EXECs its exact parent,
which derives its usual single-state statement and Spawn/IPC verifier contract.
The proof's raw-transaction commitment additionally authorizes the configuration
transition. This fixed pair does not implement #44's variable protected-state
set, or #45's general cross-version lifecycle tooling.

Only consensus/node acceptance establishes dependency liveness and rejection of
already-spent configuration Cells. A testtool fixture can inject arbitrary Cell
history; successful script verification alone must not be labeled freshness.
Pinned CKB resolves current inputs before dependencies but updates `seen_inputs`
afterward, so an input/dependency overlap needs an explicit application rejection.
See [the pinned resolver](https://github.com/nervosnetwork/ckb/blob/f7fa4436737756f97a24e254f22c13a36316ecea/util/types/src/core/cell.rs#L683).

## Frozen initial bounds

These limits precede measurement and do not relax the first profile or #40:

| Boundary | Limit |
| --- | --- |
| Transaction-wide inputs, outputs, resolved CellDeps | 64 each, checked before scans |
| Complete serialized transaction, including witnesses | 16,384 bytes, checked via bounded length query |
| Counter/config group input and output cardinality | 0/1 -> 1 each; simultaneous creation or coupled migration |
| Versions/migrations | exactly two pinned parents; one 0 -> 1 switch |
| Whole-transaction verification cycle budget | 250,000,000 |
| Each new lifecycle/config ELF | 131,072 bytes (existing lifecycle is about 64 KiB) |
| Each new Script allocator budget | 4,096 + 32,768 bytes; not an observed memory peak |
| Each observed VM/EXEC-generation stack in the migration replay | 32,768 bytes; measured separately, never summed |
| Proof/VK bytes and verifier calls | existing 128/744 bytes; one verifier call per update |

Stack observations, actual occupied capacity and execution costs must be
published separately; allocation limits are not substitute observations.
Unknown/malformed codecs, ambiguous selections and unsupported transitions
reject. Errors 100–110 distinguish context, args, bounds, identity, state,
configuration, parent, EXEC, pairing, migration and custody respectively. The
existing lifecycle's 90–97 errors and verifier's parent error 79 are unchanged.

## Required acceptance

- Continuous creation, update under parent 0, old-proof-authorized migration,
  and update of the actual successor under parent 1, with distinct VKs.
- Both Type Script groups run on the identical transaction; prove/sign ordering,
  generated-builder/adapter byte parity and raw-transaction freezing remain
  applicable. A new instance is never counted as migration of an old one.
- Reject config-only spend, missing/wrong peer, wrong owner, new-key proof at
  migration, changed successor after proving, duplicate/missing config, overlap
  between input and dependency, burn, duplicate successors, rollback, second
  migration, old-key proof afterward, replay, and legacy migration attempts.
- Exercise admitted maxima and one-past limits, preserve signed fee inputs and
  explain each rejection by its intended boundary. Continuous node acceptance
  must prove actual liveness/stale-input behavior independently of testtool.
- Reproduce both new ELFs, retain exact proof/transaction/artifact identities,
  measure resources and run the unified gates. Keep setup trust and final
  admission separate from executable test results.

None of these acceptance items is marked complete by this design record alone.

## Native client integration

`cellscript-zk-counter-client::migration` implements the fixed-pair native
workflow. `CheckedDeployment::load` takes the exact manifest bytes and an
externally trusted CKB data hash of those bytes. It also requires the observed
chain/genesis and the actual lifecycle, guard, child and VK artifacts. It checks
the setup/circuit identities, reconstructs both generated parents and verifies
their pinned deployment hashes. The manifest is not its own trust anchor:
fetching an arbitrary manifest and trusting its freshly computed digest does
not authenticate an application. Review/admission of the pinned code and setup
is a separate requirement; public-test setup records remain non-production.

The versioned `cellscript-counter-migration-v1` manifest records code OutPoints
and data hashes for both guards, both parents and VKs, the exact child deployment
for each version, network identity and both setup manifests. The checked object
also pins the complete counter/configuration Scripts in the manifest's
`instance` record, including their Type IDs. A different internally consistent
pair using the same code and parents cannot substitute for that instance. The object
owns immutable parent metadata/handles and VK bytes. `instance` derives the two
complete Scripts; `bind_instance` checks an existing pair against these pins.
`check_creation` recomputes both Type IDs from the final transaction's actual
first input and global output positions, then requires the zero counter and
parent-0 configuration. A fee builder that changes those coordinates must
regenerate both Scripts before signing.

For later transactions, `prepare` checks resolved input correspondence and
expands raw code/dep-group dependencies in order, preserving duplicates. It
requires either one matching configuration dependency or one configuration
input/output pair. It preserves configuration custody and permits only 0 -> 1.
The selected input/dependency configuration chooses the proof parent; callers
cannot supply alternative metadata or a new VK to `attach_proof`. The child
must occupy the generated parent's resolved dependency slot 0, and required
code/VK bytes must occur uniquely at the pinned OutPoints. The original counter
client derives the statement and enforces owner, capacity, Lock and increment.

After native proof verification, witness encoding uses the generated metadata.
`check_signed` retains the accepted proof snapshot, allows only Lock-witness
changes and checks the complete transaction size again. A failed or oversized
proof attachment clears the accepted snapshot. The 64-Cell/16,384-byte on-chain
bounds still apply. Additional client transport limits are 32,768 manifest
bytes, 128 dependency observations and 16 MiB aggregate dependency data; each
child artifact is at most 4 MiB. These are client rejection limits, not new
consensus resource claims.

The unified gate produces continuous transaction fixtures in the application
test, then the native client reuses their actual proofs and checks complete
byte agreement and all-group CKB-VM execution. Mutations cover manifest/network
substitution, wrong guard/Script identity, changed creation coordinates,
configuration rollback/custody/overlap, missing or ambiguous dependencies,
dep-group expansion, post-proof mutation and signed/attached witness size.
Reports are under the application's ignored `target/` directory. This evidence
does not establish provider truth, live dependency freshness, node commitment
or final profile admission. The separate node exercise below covers the
application's actual successor lineage and stale-Cell rejection.

## CCC and node integration

The [CCC migration adapter](../../examples/zk/migration.ts) consumes the same
manifest bytes and complete instance pins. It verifies both generated SDK/handle
bindings before use, checks both versions' live code/VK artifacts before a
switch, expands code/dep-group dependencies in order and derives the same final
statement as the native client. The two clients share actual application
transactions/proofs in the parity suite, including the 64-dependency case.
The existing CCC proof/cancellation/signature snapshot flow now has application
validation hooks; the immutable original counter retains its original checks.

The builder reserves proof/handle space before fee completion. Its successor
order is counter at output 0 and, when migrating, configuration at output 1;
wallet reordering rejects before proving. Fees use separate wallet Cells and
both state capacities/Locks stay fixed. Fresh input and dependency observations
are checked before wallet signing and again before dry-run. TypeScript validates
bindings/encoding; the node remains the cryptographic and consensus verifier.

With `CELLSCRIPT_COUNTER_CCC=1`, the pinned-node acceptance test creates a new
paired instance and calls this adapter for old-key update, migration and
new-key update of the actual committed successor. It records normal
`send_transaction` admission, real secp256k1 fee signatures, per-transaction
cycles/bytes, dead predecessor Cells and the live configuration successor.
Proofs under the other VK are generated for the finalized statement and must
reject at parent error 79. Stale configuration and second migration reject
before proving. Test parameters/owner secrets are public fixtures unless a
candidate setup package is explicitly provided; public deployment and setup
admission are separate.

The [developer guide](../../examples/zk/README.md#one-authorized-migration)
documents the API, runnable CLI, manifest and SDK trust, recovery and linkage
leakage. The configuration selector and both parent identities are public;
migration provides no new anonymity or hidden-state guarantee. The exact source,
final gates and publication evidence still govern 0.32 acceptance.
