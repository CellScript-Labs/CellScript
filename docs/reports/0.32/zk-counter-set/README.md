# Bounded counter-set evidence

Issue #44 implements `cellscript-zk-counter-set-v1`: one transaction updates
1–4 independently owned counter Cells using one unchanged exact proof per
state. The [contract](../../../CELLSCRIPT_ZK_COUNTER_SET.md) fixes complete
transaction-wide membership, canonical ordering, custody, creation and conflict
rules. This is a conjunction of proofs, not aggregation or a new circuit.

Codex is the implementer and boundary reviewer. No independent human review,
production setup or public deployment is claimed. Following the maintainer's
2026-10-12 instruction, no CI or gate script was run. Direct focused tests,
clippy, formatting and shell syntax checks are recorded separately. Future
gate/walkthrough wiring was updated; wiring is not evidence that a gate ran.

## Execution and resources

| Protected inputs / successors / proofs | All-group cycles | Full transaction bytes | Witness-vector bytes |
| --- | ---: | ---: | ---: |
| 1 / 1 / 1 | 110,035,523 | 1,067 | 378 |
| 2 / 2 / 2 | 220,416,610 | 1,740 | 744 |
| 4 / 4 / 4 | 441,825,731 | 16,384 | 4,744 |

The last row reaches 64 inputs, 64 outputs and 64 direct dependencies. Each
required Type group and the fee Lock accepts the same transaction. The resource
replay's complete fixture hash is checked against `fixtures.json.gz`; actual
proof/transaction bytes determine these totals. New runs may produce different
fixture deployment coordinates and must retain their own identities and costs.

Each Type group executes lifecycle, parent and child VM generations. Their
observed stack peaks are 1,568, 7,152 and 20,208 bytes respectively, not a sum.
The frozen limits are 500 million total cycles, 250 million per Type group,
32,768 stack bytes per generation and 16,384 serialized transaction bytes.
The lifecycle ELF is 67,088 bytes, the child 197,384 and parent 14,560.
Reports retain all deployed code/data Cell hashes, capacities and occupied
capacities. Heap peaks are unavailable rather than zero.

Changing a fee output after proving reaches late cryptographic rejection 79.
For each 1/2/4 case, every group is independently traced and its authoritative
cycles agree with instruction replay. Successful group totals equal the
all-group oracle. Rejected all-group totals are unavailable; independently
traced rejecting groups are not summed into a fictional executed total.

## Negative boundaries and consumers

Native tests cover input/output/dependency/byte maxima plus one; missing,
duplicate and reordered protected states; fee-as-protected-successor insertion;
same-family parent substitution; changed owner, Lock and capacity; creation
without lane zero/nonzero creation; post-proof raw mutation and successor replay.
Semantic negatives trim only opaque padding to avoid hiding their intended
boundary behind the maximum transaction size. Exact rejection codes are retained.

The standalone checker independently parses complete transaction/input snapshots
and emits 29 rebound selection/public-input mutations. The CCC consumer rejects
all of them and agrees byte-for-byte with native set statements, public inputs
and generated SDK witnesses for all positive cases. Native proof attachment
requires every proof and cryptographically verifies it. CCC clears incomplete
or cancelled proof attempts and freezes raw transaction/non-Lock witnesses;
its required node dry-run supplies cryptographic execution. Neither a snapshot
checker nor a constructor authenticates external deployment trust.

The live node test uses the pinned CKB 0.207.0 revision
`f7fa4436737756f97a24e254f22c13a36316ecea` and the exact retained fresh-build
binary commitment. Its old receipt authenticates that binary only, not the new
application. In all 1/2/4 cases, two competing transactions first pass dry-run;
after the first commits, the second rejects on its exact previously live
protected OutPoint. The actual response is `Resolve(Unknown(...))`. Resolved
successors reject old-proof replay, then accept fresh complete proofs and commit.
No nullifier or simulator-only conflict claim is substituted for this evidence.

Node and VM fixtures use an always-success fee Lock; ownership authorization is
the counter proof. Setups/owner witnesses are deterministic public test fixtures.
These reports establish local integration-chain behavior, not mainnet/testnet
deployment or secret setup trust.

## Reproduction and source

`manifest.json` binds raw/stored SHA-256 and sizes of deterministic gzip archives
and exact source files. The source capsule reconstructs changed source on signed
base `16b9e58394aa182f071303e6b5ab15dc94d304f4`. The isolated replay uses that
source without the main workspace's unrelated changes; its report identifies
the capsule. It is an extracted source reconstruction; publication verifies
that the committed implementation matches its exact source-file commitments.
The capsule records its pre-publication state separately from Git signatures.

A subsequent clean detached checkout of signed implementation commit
`98375e0a48daf8f9523fd033c59b9e8f217b5b9d` passed the complete native 1/2/4-cell
positive/negative suite, with no tracked changes before or after execution.
All 35 capsule entries match that committed source. `clean-commit-replay.json.gz`
and its retained log/complete fixtures identify this final clean-checkout
acceptance separately from the earlier extracted-source replay. This is a
focused test, not a CI/backend/dev gate. The tested child/lifecycle artifacts
match the retained exact bytes and reproducibility records above.

Two fresh target directories, one using a copied source tree at another path,
produce byte-identical lifecycle ELFs matching the exact tested binary:
SHA-256 `e85fc75b94426747cafdbf1328fb84cfc5524177d18350db6aa8ff774170e409`,
CKB data hash `b0ace9e2da48d6e4a42927f01ddbaf701936bd9f67338d9c470b4eeb22143f4e`.
The original compiler, exact circuit and child verifier are unchanged.

The contract lists focused reproduction commands and the node environment
inputs. Full node transactions and test fixtures are retained; proving keys and
runtime private setup material are not committed. The separate
[cost corpus](../zk-costs/README.md) measures complete 1/2/4 proof requests and
records its own pilot, frozen limits, acceptance and finite decisions.

Initial attempts failed: a node assertion expected `Dead`
instead of the pinned node's post-commit `Unknown`; a maximum-size semantic
negative first reached the byte limit; and an isolated snapshot under `target/`
lacked the required sibling SDK path. The tests were corrected at their stated
boundaries and the snapshot moved beside the original checkout. Only passing
replays are treated as acceptance evidence.
