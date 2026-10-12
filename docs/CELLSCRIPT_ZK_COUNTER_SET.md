# Bounded private-counter set transition

Design contract for #44, fixed before implementation and final measurement.
Implementer and boundary reviewer: Codex; no external human review is claimed.
Status: implemented with focused all-group VM, independent checker, native/CCC
byte-parity and pinned local-node conflict/recovery tests. This document does
not establish setup trust or production deployment. CI and gates are excluded
by the maintainer's current instruction; targeted evidence identifies that limit.

## Application and proof composition

One transaction updates 1–4 independently owned counters in one application
family. Minimum, representative and maximum acceptance cases are 1, 2 and 4
protected input Cells, each with exactly one successor. Each counter retains the
existing 48-byte `CSZKCNT1 || owner[32] || counter_u64_le` state and increments
exactly once without overflow, preserving owner, complete Lock and capacity.

The application is a bounded conjunction of the existing exact counter proofs:
one canonical 128-byte proof for each protected input, under the exact existing
counter circuit/VK/child profile. It is not an aggregated proof or a new circuit.
Every leaf statement binds the same final raw transaction hash, its own complete
Script hash, actual input OutPoint and old/new data hashes. CKB must execute all
protected Type groups; no leaf can authorize a peer's lifecycle or omit its proof.
This explicitly preserves the accepted single-state verifier contract while
adding a transaction-wide protected-set and correspondence contract.

The new profile is `cellscript-zk-counter-set-v1`. Its canonical set statement
contains the complete ordered leaf statements and their global input/output
indices. Its identity includes count, order, selection rule, exact lifecycle,
parent/child/VK and transaction identities. Normal compilation never loads a
proving key or invokes a prover. Existing DSL proof syntax, leaf metadata and
editor behavior stay the same; the new set codec/adapter/checker are separately
versioned and may not be represented as an existing single-state receipt.

## Membership and authority

Each protected counter has a distinct complete Type Script using one exact new
`data2` lifecycle ELF. Script args are exactly:

```text
CSZKSET1[8] || family_id[32] || lane_u8[1] || exact_parent_data_hash[32]
```

Lanes are 0–3. The family groups related counters but does not confer ownership.
Every lifecycle invocation scans the complete transaction-wide input and output
sets before EXEC of its exact CellScript parent. Membership selects the exact
lifecycle code identity and family. Same-family malformed args, wrong hash type,
changed parent identity, duplicate lane/Script or noncanonical ordering reject;
they cannot silently become fee Cells. Protected inputs and outputs appear in
strictly increasing lane order among their global indices. Fee/change Cells may
occur between them and remain covered by every proof's raw transaction hash.

The complete input and output lane sets must be equal on updates. For each lane,
the complete successor Script must equal the input Script; state, Lock and
capacity preservation are checked by that lane's lifecycle, followed by its
own proof. The global scans derive membership on chain. No witness-provided
selection list is trusted. A host set statement is a reproducible observation,
not an authority token accepted instead of the scan.

An update may consume any nonempty subset of the family's live lanes. Complete
selection means every protected input present in this transaction, not all
unspent family Cells elsewhere on chain. Removing a lane from a prepared
transaction changes its raw hash, invalidating every retained proof; rebuilding
and reproving a smaller complete subset is an explicitly valid new operation.
Inserting a protected input requires its matching successor and its own proof.

Zero-input behavior is creation only: create 1–4 lanes in canonical contiguous
order, starting at lane 0, each at counter zero. `family_id` is the CKB-personalized
BLAKE2b-256 of the complete serialized first transaction input followed by the
first family output's global index as u64 little-endian. All created lanes share
that family and exact parent. Later transactions cannot add or remove a lane
within an update. Existing immutable or fixed-pair counter Cells cannot be
retrofitted into this new lifecycle.

## Replay and conflicts

There is no nullifier tree, privacy-set or global uniqueness claim. Each counter
uses CKB input consumption. Reusing a proof on successor OutPoints or changing
fees, ordering, inputs, outputs or dependencies fails its statement binding.
Two otherwise valid transactions sharing any protected input conflict; after one
commits, the other has a spent input. Recovery resolves the current live subset,
rebuilds the entire final transaction and reproves every leaf before signing.
Independent transactions over disjoint lanes remain possible.

## Frozen initial bounds

These are separate application bounds; they do not relax the existing leaf
profile or #40 budgets.

| Boundary | Frozen maximum |
| --- | ---: |
| Protected inputs / successors / proofs per update | 4 / 4 / 4 |
| Transaction-wide inputs / outputs / resolved dependencies | 64 each |
| Serialized transaction including all witnesses | 16,384 bytes |
| Whole-transaction verification | 500,000,000 cycles |
| Each existing leaf parent + child verification | existing 250,000,000 cycles |
| Each VM/EXEC generation observed stack | 32,768 bytes, recorded separately |
| New lifecycle ELF | 131,072 bytes |
| Existing verifier ELF | 262,144 bytes |
| Each generated parent ELF | 32,768 bytes |
| Lifecycle allocator budgets | 4,096 + 32,768 bytes, not an observed peak |
| Each leaf proof / VK / public-input vector | existing 128 / 744 / 484 bytes |

The codec must reject zero or >4 update leaves, duplicate/global-index drift,
wrong order, unknown version, length/trailing data and mismatched set identities.
The independent checker must reconstruct selection/count/correspondence and
every public-input byte from the complete transaction and resolved input
snapshots, with rebound negative vectors. Native builder/adapter and an
independent consumer must agree byte-for-byte.

Required execution evidence includes min/representative/max all-group success,
missing/inserted/reordered/duplicate/substituted state, unmatched successors,
fee/change confusion, post-proof mutation, replay and real conflicting-spend
recovery. Record exact artifact/source identities and per-case cycles, stack,
witness/full transaction sizes and occupied capacity. Heap peaks remain explicit
if unavailable. Extend #46 with these actual protected-set cases after they run;
its fixed-pair maximum cannot substitute for them.

## Canonical transport and consumers

The set statement is a six-field Molecule table. Its seven leading little-endian
u32 values are total length and offsets `[28, 36, 40, 72, 76, 108]`. Fields are
eight-byte `CSZKSET1`, u32 version 1, 32-byte family, u32 leaf count, 32-byte raw
transaction hash and a FixVec of leaves. The vector repeats the u32 count. Each
236-byte leaf contains global input/output u32 indices and the unchanged
228-byte exact statement. Total length is `112 + 236 * count`, at most 1,056
bytes. Indices must increase strictly and remain below 64; counts and every raw
transaction commitment agree. Unknown versions, noncanonical offsets, trailing
bytes and duplicate indices reject.

The standalone checker parses the actual transaction and complete ordered input
snapshots independently of compiler/SDK parsing. It reconstructs selection,
correspondence, custody, increments and each public-input byte. Snapshot
CellOutput and data are each bounded to 4,096 bytes before parsing. Host
transport admits direct code dependencies only, rejects duplicate inputs/deps
and binds all witnesses and supplied snapshots in its unchanged check. This
does not authenticate RPC resolution, proofs, deployment trust or consensus.

The CKB adapter derives the result with packed CKB types and compares bytes
with the standalone checker. Native `PreparedCounterSet` locally verifies all
proofs against the immutable parent before returning a complete result. Any
failed attempt invalidates that result. Signing permits only Lock witness
changes within the final transaction budget.

The independent CCC consumer reconstructs the same set and uses generated leaf
SDK ABI v2 witness encoding. It resolves inputs/code, checks genesis/deployment
hashes, finishes fees before proving and freezes previous leaf witnesses while
collecting the next proof. Failure/cancellation clears the completed result.
CCC checks proof encoding/public-input binding; cryptographic verification
occurs in the required all-group node dry-run. It rechecks live inputs/code
before signing and sending. Callers supply an authenticated deployment and
appropriate fee signer; constructor success does not establish that trust.

## Targeted evidence and reproduction

Native creation/update cases contain 1, 2 and 4 independently owned counters.
The maximum has exactly 64 inputs, outputs and direct dependencies and 16,384
serialized bytes. Positive totals are 110,035,523, 220,416,610 and 441,825,731
cycles. Bound-plus-one inputs, outputs, dependencies and transaction bytes
reject at 113. Semantic negatives remove only opaque padding so size cannot
mask their intended rejection boundary.

Rejection codes are 112 for malformed/same-family parent args, 114 for duplicate
or reordered selection and fee-as-successor insertion, 115 for missing
successors, 116 for changed owners, 117 for missing creation lane zero/nonzero
creation, 120 for changed protected capacity/Lock, and 79 for changed raw
transaction commitments or old-proof replay. The checker emits 29 canonical,
selection and public-input mutations for independent TypeScript rejection.
Native/CCC/generated SDK agree on all three positive fixture transactions.

The node test creates live families of sizes 1, 2 and 4, dry-runs two distinct
proof-bearing transactions sharing the family's inputs, commits the first and
rejects the competitor at exact spent-input resolution. The pinned node reports
`Resolve(Unknown(...))` after commitment; the report retains that actual response.
Each case resolves successors, rejects old-proof replay, reproves every leaf and
confirms recovery. The node and diagnostic fixtures use an always-success fee
Lock; owner authorization remains in each proof. Setup is public deterministic
test material and the network is a local integration chain.

Resource replay retains separate lifecycle/parent/child VM stack peaks of
1,568 / 7,152 / 20,208 bytes. Heap peaks and rejected all-group cycle totals
remain unavailable. Independent rejecting-group traces are not summed into an
executed transaction total. Reports retain deployed ELF hashes/sizes, occupied
capacities and witness/full bytes. The new lifecycle is 67,088 bytes; the
unchanged child is 197,384 bytes.

After building the existing exact verifier, focused reproduction is:

```bash
cargo build --locked --release --manifest-path contracts/zk-private-counter/script/Cargo.toml --bin cellscript-counter-set --target riscv64imac-unknown-none-elf
cargo test --locked --release --manifest-path crates/cellscript-zk-counter-client/Cargo.toml --test counter_set
cargo test --locked --release -p cellscript-artifact-checker --test counter_set -- --ignored --nocapture
cargo test --locked --release -p cellscript --test zk_counter_set_resources -- --ignored --nocapture
npm --prefix examples/zk run test:counter-set
npm --prefix examples/zk run check
```

TypeScript requires the generated SDK from the producer's
`target/counter-set/parent-metadata.json`, emitted with the existing
`cellc gen-builder --target typescript` command into `target/counter-set/sdk`.
For the private-counter crate's ignored `node_set` test, set
`CELLSCRIPT_CKB_REPO` to the pinned checkout and `CELLSCRIPT_SET_CKB_RECEIPT` to
a retained receipt authenticating its fresh-built node binary. That receipt
authenticates the node only; the test records new application source, artifacts,
transactions and execution separately. It does not require or run a new gate.
