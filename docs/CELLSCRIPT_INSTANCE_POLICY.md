# Constrained Script instances

Status: bounded technical implementation accepted for
[#80](https://github.com/CellScript-Labs/CellScript/issues/80), owned by Codex
under the maintainer's 2026-10-17 implementation request. The versioned profile,
compiler/checker enforcement, complete transaction transport, non-financial
application, two-pool slice and resource corpus have executable evidence.
The retained [acceptance record](reports/0.32/instance-policy-acceptance/README.md)
identifies the signed source snapshot and the completed `dev`, `ci` and
`backend` runs, including their exact source deltas and earlier failed attempts.
Release, public deployment and independent human audit remain separate.
The issue-specific author self-review is recorded below; no earlier #28/#29
human-review waiver is inherited.

## Problem and ownership

The accepted [open handle](CELLSCRIPT_OPEN_HANDLE_DESIGN.md) member commits a
complete Script hash, including concrete args. Two deployments of the same
code with different issuers or pool configurations therefore require two
members. A finite immutable set cannot enumerate all future configurations.
Changing the interpretation of that member hash would silently invalidate the
existing exact-instance authorization boundary.

The new profile authenticates a separate args policy around an unchanged
implementation selection. The original selection retains its template Script,
source receipt, exact code bytes, OutPoint, target, network, version floor and
Type-ID history. Its template Script does not become a wildcard. Only the new
outer policy can authorize a separately identified concrete instance.

A non-financial example is a document-attestation observer: approved code may
be configured with an issuer equal to the observer's committed issuer, an expiry within
a permitted interval and an explicitly variable instance nonce. Different
nonces remain different instances. Policy membership alone does not create an
attestation, establish uniqueness, grant document ownership or prove its Script succeeds.

The smallest owning layers are the existing policy/checker, handle runtime and
ProtocolBundle. No application-name dispatch, allocator or general IPC service
is needed. SSRI discovery and additional argv/IPC codecs remain separate work.

## Independent policy dimensions

| Implementation selection | Instance admission | Initial contract |
| --- | --- | --- |
| Exact | Exact | Existing receipt selection plus exact full Script |
| Compatible | Exact | Existing directional compatibility plus exact full Script |
| Exact | Constrained | One selected implementation, bounded authenticated args |
| Compatible | Constrained | Selected compatible implementation, the same bounded args policy |

All four combinations are defined for Lock/Type participants. Existing exact
spawned-verifier selection remains supported. Constrained spawned-verifier args
are rejected: the accepted parameterless child inherits the consumer context,
and has no independently selected Script/group whose args can be observed.
Admitting a new child request ABI requires its own explicit contract; a Script
args policy cannot introduce one implicitly.

## Canonical wire contract

The independently parsed [Molecule layout](../crates/cellscript-artifact-checker/src/instance_policy.mol)
uses new magics and domains. Old 202-byte exact handles and 656-byte open
selections remain byte-for-byte unchanged and cannot decode the new format.
All lengths/counts are checked before allocation/traversal. Unused bytes are
zero; unknown tags, alternate padding, truncation and trailing bytes reject.

| Record | Bytes | Content |
| --- | ---: | --- |
| Policy `CSINPv1\0` | 848 | 80-byte header plus sixteen 48-byte constraint slots |
| Consumer args `CSINCv1\0` | 208 | Policy root, concrete subject Script hash, configuration length and 128-byte padded configuration |
| Selection `CSINWv1\0` | 1768 | Magic, policy, unchanged 656-byte implementation selection, 256-byte padded concrete args |

The policy header contains instance mode, constraint count, exact args/config
lengths, the implementation policy root and an independently recomputed args
schema hash. Its root is CKB BLAKE2b-256 over
`cellscript-instance-policy-v1\0 || policy_bytes`. The schema hash uses
`cellscript-instance-args-schema-v1\0`, a fixed eight-byte header (u16 args
length, u8 field count, five zero bytes) and sixteen eight-byte field slots
(u16 offset, u16 length, u8 kind, three zero bytes). Kind is fixed bytes (0)
or unsigned integer (1). Unused slots are zero. Schema identity describes
layout; permission predicates/values additionally enter the policy identity.

Constraints form an ordered complete partition of at most 256 args bytes,
with at most sixteen fields. Gaps, overlap, zero-width fields and out-of-range
offsets reject. Each row stores a tag, a zero byte, u16 length, u16 offset,
u16 config offset, eight zero bytes and a 32-byte value area:

| Tag | Predicate | Bounds and representation |
| --- | --- | --- |
| 0 | Exact bytes | 1–32 bytes, padded with zeros |
| 1 | Unsigned inclusive range | 1/2/4/8-byte LE scalar; u64 minimum and maximum, remaining value bytes zero; endpoints must fit the declared width |
| 2 | Equal committed consumer configuration | Nonempty in-range slice of the 0–128-byte consumer configuration; value bytes zero |
| 3 | Explicitly variable bytes | Nonempty bounded field; value bytes zero |

Config offset is zero except for tag 2. Exact-instance mode admits only tag 0
and additionally requires the resulting complete Script hash to equal the
original implementation member's Script hash. Empty args are representable by
an empty partition. This is fixed-layout admission, not variable-length fields,
arbitrary predicates, nested codecs or unrestricted allocation.

The selected concrete Script is canonically reconstructed from the member's
code hash/hash type and the actual args. Its hash must equal the subject in
the consumer's committed args in both modes. Replacing an authorized instance
A with B therefore rejects even when both independently satisfy the policy.

The [independent CCC reference encoder](../crates/cellscript-artifact-checker/tests/fixtures/instance_policy_vectors.cjs)
uses pinned `@ckb-ccc/core` 1.23.0 Molecule structs/arrays, canonical Script
serialization and CKB hashing without importing compiler/checker encoders.
Its [fourteen frozen vectors](../crates/cellscript-artifact-checker/tests/fixtures/instance_policy_vectors.json)
cover all four implementation/instance-mode combinations, empty args, maximum
args/config/constraint layouts, all four unsigned widths and a non-palindromic
mixed policy containing every predicate tag. Rust compares the complete policy,
schema/root, consumer, old implementation selection, new selection and concrete
Script bytes, and verifies that old/new readers reject the other's format.
CI independently regenerates and compares the vectors using the existing CCC pin.
Their identities are synthetic byte-codec evidence, not admitted code receipts.

## Authority, runtime and transaction obligations

The consumer root/configuration and selected participant args have different
locations and meanings. The new consumer layout is separate from the existing
exact 32-byte-root path. Native admission must derive it from an actual input
Script, and the machine helper must load it from the current Script. A witness
cannot provide its own expected root, subject or configuration.

The machine helper must authenticate the policy, enforce the canonical decoder
and complete args predicates, check the real direct code dependency and observe
the selected complete Lock/Type Script on the declared transaction/group source
and index. Reconstructing a witness Script without that observation is
insufficient. The standalone checker must independently establish those machine
checks and their rejecting paths after enclosing hashes are rebound.

Direct O0–O3 VM regressions exercise the new helper with global input/output
and group-relative input/output views. Lock-role cases place the observed Cell
at global index 1 and group index 0, so aliasing those scopes cannot pass.
Type-role cases accept the foreign global participant and reject group views
that actually select the consumer's Type Script. Each view also rejects an
out-of-range index and substitution of a different, independently policy-valid
instance. These isolated observation tests use an always-success peer; actual
peer authority and rejection are covered by the application tests below.

The native `freeze_instance_policy` path retains the original private
source/implementation proof, independently checks a new fixed-policy receipt
for the concrete Script at the exact original deployment, and binds a new
source receipt. An already checked Type-ID history can only be rebound to
that same target, executable, Type identity and code OutPoint. Private
selection/dependency/group records bind the policy, concrete instance and
actual transaction snapshots; rechecks reject changed sources and bytes.
Their supplied consumer args still need authentication by ProtocolBundle and
the machine helper. These host APIs alone do not prove that authorization.

The selected instance, policy, role,
source/index, schema, cardinality, dependency and witness must survive generated
builder/CLI/SDK materialization with identical final bytes before signing.
Every required Script group must execute, including peers that reject despite
successful membership. Foreign observation remains read-only authority.

The same schema or allowed issuer does not establish pool identity,
authentic genesis, asset custody or lifecycle correspondence. Those remain
application predicates. The initial pool evidence must use two separately
configured instances of one implementation without a universal consumed
coordinator, with valid signatures on otherwise-valid substitution negatives.

## Source, machine and transport profile

`InstanceScriptHandle<I>` is a separate nominal imported-module handle, with a
fixed 1768-byte witness representation. The helper
`ckb::require_instance_script_participant_from_args(dep, handle, subject)` accepts
a `CellDepView` and an `InputView<T>` or `OutputView<T>`. It loads the current
Script's 208-byte consumer args, authenticates the outer policy, checks the
unchanged implementation membership and direct dependency, then checks the
actual selected Cell's complete Script and args. Malformed or disallowed
values terminate with `OpenHandleInvalid` (85). Observation never executes an
output Lock or grants authority to spend a foreign Cell.

The entry decoder profile is `cellscript-fixed-instance-parameter-decoder-v1`:
one instance handle, exactly 1776 CSARG payload bytes including the eight-byte
envelope. The old fixed-parameter decoder retains its 1536-byte aggregate
limit. Multiple instance handles or additional parameters are outside this
admitted entry profile. ProtocolBundle retains the original binding/count,
fixed Cell-schema and source/index limits from the open-participant contract.

The standalone machine checker freezes the executable shape of the instance
helper and its five transitive runtime dependencies. It normalizes only checked
call relocations to unique callee identities; all other machine words remain
bound. This is a closed runtime profile, not general source/ELF equivalence.
Updating the emitter requires review of the changed instructions and a new
reviewed profile, never automatic digest refresh on a failed check. Rehashed
ELF/source-map mutants cover conditionals, loads, constants and transitive
helpers independently of the producer's enclosing hashes.

`check_protocol_bundle_with_instance_handles` accepts private instance policies
and the complete transaction/snapshots. Its `open_bindings` field can retain
ordinary open participants in the same transaction. The CLI uses the existing
`cellc protocol bundle check ... --open-admission ...` command with the new
request schema `cellscript-protocol-instance-admission-input-v1`. An instance
binding adds `instance_policy` (canonical 848-byte lowercase `0x` hex) to the
existing source-policy fields and carries a 1768-byte `selection`. The new
request schema requires at least one such binding; the old schema rejects it.

The resolved record uses `cellscript-protocol-instance-handle-v1` and an
`instance` record with schema `cellscript-protocol-instance-admission-v1`.
That record preserves the implementation receipt separately from the concrete
instance receipt, plus the instance source policy/selection identities, args
schema, policy root, mode and actual consumer args. Old records omit `instance`.

Generated TypeScript exports `encodeInstanceHandleWitness` and
`instanceHandleWitnessFromCheckedBundle`; the existing open-handle helpers
continue to accept only their original format. These are serialization and
checked-report transport helpers, not independent private-receipt constructors.
The SDK adapter rechecks canonical constraints, actual consumer configuration,
complete participant Script, dependencies, groups and final witness bytes.
Signing preparation still binds the complete materialized transaction.

### Isolated maximum-args measurements

`instance_runtime_empty_maximum_scalar_widths_and_late_rejection_are_bounded`
executes all four implementation/instance-mode combinations at O0–O3, empty
args, every unsigned width at zero and its maximum, all sixteen constraint
slots, 256-byte args, 128-byte consumer configuration and a 257-byte actual
participant rejection. Its late-invalid case changes the last byte of the
sixteenth exact-byte constraint and recomputes the concrete subject, so the
failure reaches the final field predicate rather than failing identity first.

The diagnostic instruction replay agrees with the ordinary group verifier on
verdict and cycles. These measurements isolate the admitting Type group; the
peer is an always-success fixture and is not application acceptance evidence.

| Optimization | Valid maximum group cycles | Late-invalid group cycles | Observed stack bytes | ELF bytes | CSARG payload bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| O0 | 198615 | 189380 | 9808 | 16376 | 1776 |
| O1–O3 | 198614 | 189379 | 9808 | 16376 | 1776 |

The fixture ceilings are 205000 group cycles, 10240 observed stack bytes and
16384 ELF bytes. Observed stack is the traced stack-pointer high-water mark,
not total VM memory. These are bounded fixture measurements, not a proof that
this constraint mix maximizes every permitted policy's cost.

The expanded boundary corpus also selects the final member of a 32-member set
and scans all 64 direct dependencies. It exercises sixteen exact-byte fields,
sixteen configuration comparisons, sixteen u64 fields, sixteen explicit variable
fields, and fifteen unsigned fields of each 1/2/4/8-byte width followed by a
variable tail. Args reach 256 bytes where representable, configuration reaches
128 bytes, and the last byte of the final exact/configuration field supplies
late-rejection cases. Sixteen u64 fields occupy only 128 args bytes; the report
does not mislabel that shape as a 256-byte scalar layout.

| Combined boundary observation | O0 | O1–O3 |
| --- | ---: | ---: |
| Largest successful group cycles among measured cases | 271812 | 271811 |
| Last-field configuration rejection cycles | 261927 | 261926 |
| Observed stack bytes | 9808 | 9808 |
| Guest pages initialized/read/written, bytes | 32768 | 32768 |
| ELF file / PT_LOAD memory bytes | 16376 / 16084 | 16376 / 16084 |
| Largest isolated transaction bytes | 4686 | 4686 |

The combined fixture freezes ceilings of 275000 group cycles, 10240 stack bytes,
32768 touched guest-page bytes, 16384 ELF bytes and 4800 transaction bytes.
The unchanged witness payload is 1776 bytes. The VM's separate address-space
ceiling is 4194304 bytes. Guest-page accounting includes ELF/argv initialization,
instruction/data reads, writes and syscall buffers through a memory wrapper;
repeated touches count once. It does not measure host allocator overhead/RSS or
identify allocations as application heap. Ordinary and instrumented scheduler
verdicts/cycles must agree. A focused crossing/initialization test and the real
parent/child byte-context suite validate the added measurement path.

The 88 case records and diagnostic traces are retained at
`target/instance-policy-evidence/resources/report.json`. These are explicitly
named boundary measurements rather than a mathematical maximum over every
possible mixed policy. Complete-transaction custody, signatures, occupied
capacity and application budgets are recorded separately below.

## Evolution and threat model

The outer root commits the full implementation root, including sequence/floor,
active/yanked members and exact/compatible mode. A later Registry edit cannot
change an old immutable snapshot. Policy widening, different constraints,
changed schemas and switching instance mode produce different roots. An
application must separately authorize any root/config/subject transition.
Source compatibility never supplies that authority. Type-hash code selection
continues to require exact current bytes/OutPoint and independently checked
history; merely preserving history bytes is not validation of that history.

| Attempt | Required rejecting boundary |
| --- | --- |
| Old bytes relabelled as a new policy or constrained selection used as exact | Distinct lengths/magics/domains and mode rules |
| Supplied expected root, owner or instance chosen by the same witness | Actual current/input Script args and checked accessors |
| Omitted field constraints, malformed scalar, overflow or padding aliases | Bounded canonical partition/decoder |
| Policy widening, stale implementation, yanked/downgraded candidate | Root plus existing source/version/sequence checks |
| Same interface but wrong code, network, deployment or Type history | Original independent receipt and actual dependency binding |
| A/B state, reserve, owner or subject substitution | Concrete subject plus application genesis/custody/lifecycle predicates |
| Peer missing or failing despite valid admission | Complete required group derivation and actual all-group execution |
| Changed witness/dependency/instance after checking or signing | Final-byte materialization and signature checks |
| Advertised checks removed from rehashed ELF/sidecars | Independent machine checker mutations |

### Implementation review record

On 2026-10-17 Codex performed an author self-review of this #80 implementation:
the independent host codec against the runtime decoder, stack-buffer extents
and scratch-register lifetimes, policy/configuration/subject authority,
normalized-call machine fingerprints, private receipt and Type-ID history
composition, ProtocolBundle/adapter downgrade checks, and the document/pool
genesis, custody and all-group failure fixtures. The review preserves the
separation between membership, concrete identity, application authority and
actual execution. Generated TypeScript transports checked evidence; it is not
presented as an independent receipt verifier.

This is the issue-specific review disposition for the local implementation:
author self-review completed, independent human audit not performed, and no
external-review waiver claimed or inherited. The executable checker, adversarial
tests and remaining unified gates are separate technical evidence, not a
substitute claim that a human audit occurred. The bounded profile remains
experimental; publication, deployment and any later stable/release acceptance
remain separate decisions.

## Executed non-financial lifecycle

`instance_document::issued_document_instances_require_real_issuer_custody_and_peer_execution`
in [the native source-policy suite](../tests/support/instance_document.rs) uses
a [document Type validator](../tests/fixtures/instance_policy/document.cell.in)
and an [observer Lock](../tests/fixtures/instance_policy/document_observer.cell.in).
The approved document implementation pins the complete issuer Lock hash; the
test renders that source from a real secp256k1 signing identity before compiling
and independently checking its receipt. This is a bounded issuer deployment,
not a claim that variable issuer authority has been added to Script args.

Document args contain a 16-byte namespace, an LE64 category and an LE64 serial.
The 64-byte data repeats those 32 args bytes followed by a full document digest.
The observer's authenticated configuration fixes the namespace; its policy
admits categories 1 through 5 and nonzero serials. Each observer still commits
the complete concrete document Script hash. Serial values are issuer-assigned
instance coordinates, not a globally enforced unique-NFT or Type-ID promise.

Only ordinary issuer and attacker funding Cells are seeded. Three documents
and their observer Cells are outputs of verified signed issuance transactions.
The Type validator checks issuer authorization, exact data size, args/data
correspondence and issuer custody at creation. It permits retirement, rejects
successor outputs and verifies the caller's complete expected document digest.
The observer has only a foreign read view; it acquires no document lifecycle
authority. Local live-Cell and capacity bookkeeping supplement the real
all-group VM checks without claiming full-node admission or inclusion.

At O0–O3, two independently configured documents pass native instance/dependency
admission and retire successfully. A third document is genuinely issued in
category 6: its issuer permits it, but both native admission and the observer
Lock reject it. Other negatives retain otherwise-valid signatures and reject
at the intended group: unauthorized issuance, wrong custody, mismatched
configuration or malformed data at the document Type; a changed final digest
byte or forbidden successor at the selected peer Type; missing participants
and policy-valid B substituted for observer A at the observer Lock.

The complete successful transactions use 2,354 serialized bytes at most across
the four optimization levels. Each retained report records exact cycles for
its signed transactions; the fixture enforces 2,000,000-cycle and 2,400-byte
ceilings. These are application observations,
not the global maximum-policy resource claim. The earlier isolated issuer/
expiry/nonce fixture separately exercises wider argument constraints; it is
not used as evidence of this document's issuance or custody.

The fixture retains each optimization level under
`target/instance-policy-evidence/document/O{0,1,2,3}`: rendered source, complete
compiler bundles and source contexts, implementation creation, canonical policy,
dependency bytes and a report containing all sixteen accepted/rejected signed
transactions with actual input snapshots. Ordinary funding OutPoints are fixed
as well as deployed dependencies. The retained transaction freezes its exact
source-policy identity and signature; a newly frozen source snapshot may change
that identity and signature-dependent cycles even when the executable is unchanged.
These are local test artifacts, not a published deployment record.

## Executed two-pool application slice

`instance_pool::two_pools_issue_assets_authenticate_genesis_and_swap_independently_and_together`
in [the pool fixture](../tests/support/instance_pool.rs) executes two instances
of the same deployed [pool Lock](../tests/fixtures/instance_policy/pool.cell.in)
at O0–O3. The two real test-asset Type identities share an enforcing
[asset implementation](../tests/fixtures/instance_policy/pool_asset.cell.in)
but bind different complete issuer Lock hashes. Issuance requires the issuer's
actual secp signature; subsequent groups bind all amounts to actual Cell data
and preserve their sum. These are explicitly bounded test assets, not sUDT/xUDT.

Each pool has an independent built-in Type ID, an owner, a nonce and immutable
fee (30 or 100 basis points). Its 200-byte state contains five complete hashes
(Type ID Script, ordered asset A and B Scripts, owner Lock and network domain),
fee and nonce, followed by reserves and revision. The 32-byte controller args
commit the first 176 bytes. A separately funded untyped bootstrap Lock validates
initial state/configuration/custody; the built-in Type ID independently checks
the first-input/output-index creation anchor. No pool state or reserves are
preseeded, and no global coordinator is spent. Same-pair instances are allowed:
Type ID establishes per-instance uniqueness, not a unique pool for each pair.

The [reserve Lock](../tests/fixtures/instance_policy/pool_reserve.cell.in)
commits the complete pool, controller and asset identities and requires the
matching pool input and successor. The pool binds both real reserve amounts,
preserves configuration, requires its owner's signature, advances revision and
checks an A-to-B constant-product quote with the configured retained fee.
Asset groups enforce conservation across reserves and wallets. Extra
reserve-like deposits grant no shares or accounting credit in this slice.
Balances are bounded by 1,000,000,000, fees by 1,000 basis points and admitted
input revisions by 1,000,000; intermediate arithmetic remains within u64.
Asset groups allow at most four inputs/outputs and custody scans at most sixteen
transaction inputs/outputs. A pool transition uses six input/output roles:
state, reserve A, reserve B, owner funding, wallet A and wallet B.

The instance policy admits an explicit opaque 32-byte commitment field; fee,
owner, network and lifecycle validation belong to the pool implementation.
It does not advertise a range predicate over hidden fee bytes. Each
[observer](../tests/fixtures/instance_policy/pool_observer.cell.in) still commits
one concrete pool Lock hash. Native source/instance/dependency/group admission
checks the full signed transaction. Both pools swap separately and then together,
with two independently bound observers in the combined transaction.

Nineteen cases per optimization level retain ten successful transactions and
nine rejecting transactions: forged genesis anchors, policy-valid B replacing
A, B's reserves/owner replacing A's roles, fee configuration replacement,
reserve spending without pool state and an asset peer that rejects inflation
despite valid instance admission. Negatives rebuild actual asset witnesses and
holder signatures. A cross-pool reserve substitution can reject at the pool
Lock or the substituted reserve Lock depending on group execution order; both
are explicit enforcing boundaries, and unrelated failures are not accepted.

The combined transaction is 8,444 serialized bytes. The fixture enforces a
7,000,000-cycle successful-transaction ceiling and a 9,000-byte combined limit;
these are bounded observations, not global policy maxima. Each transaction pays
100,000,000 shannons, checks capacity balance and occupied output capacity, and
uses a local live set. Full source/artifact/dependency/input/transaction records
and four native group receipts are retained under
`target/instance-policy-evidence/pools/O{0,1,2,3}`. A fresh VM then replays the
retained bytes, checking accepted cycles and intended rejecting groups without
recompilation or resigning.

This slice supplies #80's authentic instance/custody exercise. It does not close
#52/#61: LP issuance/redemption, concentrated-liquidity accounting, arbitrary
asset modes, canonical pair discovery, generated financial builders, competing
node submissions and public deployment are outside this fixture. Existing
ProtocolBundle/CLI/TypeScript/SDK instance parity is tested separately; no
financial-builder parity is inferred from these Rust-constructed pool transactions.

## Acceptance ledger

The following evidence closes the bounded technical scope of #80. The linked
acceptance record retains full gate provenance and reproducible application bytes.

| Requirement | Accepted evidence / scope boundary |
| --- | --- |
| Versioned design, owner, bounds and threat model | This bounded profile and threat matrix; Codex ownership and issue-specific author self-review recorded above, with no independent human audit or inherited waiver claim |
| Independent canonical wire, four selection combinations, A/B subject separation | Seven standalone checker tests including fourteen frozen independently generated CCC/Molecule vectors; host-only |
| Preserve #11/#28 formats, roots and vectors | Existing exact/open suites; open wire regressions unchanged |
| Private source/receipt and actual instance/dependency binding | `tests/frozen_interface.rs` checks O0–O3, exact/compatible implementation policies, SDK Script serialization, distinct A/B receipts/groups, dependency/witness/source replacement and exact Type-ID history reuse; ProtocolBundle binds consumer args to actual input snapshots |
| Actual args/root machine checks and rehashed mutations | `tests/exact_script_handles.rs` executes bounded args/config/root, all four global/group input/output views, actual Lock/Type fields and A/B negatives at O0–O3; `tests/artifact_checker.rs` rejects over 200 rehashed instruction changes and transitive-helper mutations |
| Source/type/lowering/ProofPlan/metadata/editor/WASM parity | Formatter round-trip, native CLI/LSP cross-profile rejection, completion and browser multi-source nominal-type regressions pass; full integration gates passed with source deltas recorded in the acceptance record |
| Native/CLI/generated-builder/SDK byte parity and signing negatives | `instance_protocol_admission_binds_private_receipts_to_complete_transaction_bytes` passes O0–O3 native/CLI/Node/SDK parity, real secp signing, actual all-group success, signed peer rejection, post-signing witness replacement, schema downgrade and rebound report mutations; unchanged open-path regression also passes |
| Fixed verifier allowlist and actual child-failure regressions | The unchanged `open_lock_and_verifier_profiles_materialize_and_execute_identical_transactions` passes at O0–O3 for Lock/Type consumers, including actual SPAWN/WAIT, child-predicate failure, machine failure-path mutations and native/CLI/generated-builder/adapter parity; full gate revalidation passed |
| Non-financial instance example with real VM enforcement | O0–O3 document issuance and retirement use actual secp signatures, native source/instance/dependency receipts and complete VM group execution; category-policy, issuer, custody, configuration, lifecycle, missing-peer, rejecting-peer and A/B substitutions reject at their intended boundaries |
| Two real pool instances, authentic genesis/custody, all-group A/B negatives | O0–O3 bounded pool slice above: real issuance, Type-ID bootstrap, owner/custody/asset groups, separate and combined swaps, native instance/dependency/group admission and targeted A/B negatives; broader #52/#61 financial scope remains separate |
| Maximum/late-invalid cycles, stack/memory, ELF, witness/tx/capacity | 88 named boundary observations combine all 32 members/64 direct dependencies with maximum predicate shapes; cycles, observed stack, touched guest pages, ELF/load segments and transaction bounds above; real document/pool cases separately check capacity and application budgets |
| Reference vectors, retained source/artifact/transaction evidence and gates | Independent CCC vectors and replayable application/resource evidence retained; final hashed inventory, application bytes and dev/backend/ci provenance are retained in the acceptance record |

#50/#51 are independently tracked. On 2026-10-17 the three `vec_constructor`
tests and four `bounded_byte_context` tests passed again, including CLI/LSP
diagnostic recovery, matched real parent/child execution and the paired
AgoraSeal template/generated-source check. The application adoption is the
repository's [vendored vote package](../examples/bounded-byte-context/README.md):
both source forms and manifest forms are updated together. The separate upstream
AgoraSeal checkout remains untouched; its broader application work was paused
by the maintainer and is not silently resumed by this compiler task.

The full `dev` replay passed on 2026-10-17 after isolating document templates as
`.cell.in` and refreshing the business-corpus inventory through its native
generator. Full `ci` passed in the isolated acceptance worktree. The CI root
test binary predates the test-only view/role regression in `f55508b5`; that
regression passed separately, and the final full `backend` run includes it.
Compiler, checker and application implementation did not change across that delta.

The final `backend` run uses clean signed source
`f55508b572aa56c9c7298766bda28f089d57fe14`, `RUST_TEST_THREADS=1` and the
required clean CKB revision `f7fa4436737756f97a24e254f22c13a36316ecea`.
The earlier concurrent-fixture and mismatched-CKB attempts remain recorded as
failures. A subsequent infrastructure change makes the same serialization
explicit in the backend command; the final documentation/evidence commit does
not change runtime semantics or rebind the clean-source receipt to a new commit.
The final commit also passes the ordinary `commit` gate.

Pinned submodules in the isolated worktree exclude unrelated maintainer edits.
The CKB stateful and counter-node receipts cover their named existing fixtures;
the document/two-pool slice remains local all-group CKB-VM evidence. These gate
results establish technical integration acceptance, not a release or public
network deployment.
