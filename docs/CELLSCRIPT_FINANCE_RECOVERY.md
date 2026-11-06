# Financial client recovery

The [FIN-09 work item](https://github.com/CellScript-Labs/CellScript/issues/59)
uses the existing ProtocolBundle transaction, live-resolution, signing and
submission interfaces. The public Rust `cellscript_ckb_adapter::recovery`
module adds bounded client policy over those interfaces. It performs no RPC,
signing, transaction serialization or financial authorization.

`examples/finance_client/recovery.rs` is the application-owned reference client.
Its source is compiled by the adapter's focused host tests and the
[public financial node example](../examples/ckb-sdk-builder/README.md#disposable-financial-node-example).
It is not a second serializer or signer or a separately published finance package.
Host tests, actual node execution and optional independent developer
reproduction provide different evidence; complete acceptance retains those
distinctions.

## Immutable intent and real candidate Cells

`RecoveryIntent` fixes the chain/genesis, application intent commitment, complete
Lock/Type Script subjects, maximum transaction fee and maximum occupied output
capacity. A subject includes its code hash, hash type and complete arguments;
a ticker, package name or bare code hash is insufficient.

The finance client owns a nonzero operation ID, operation, owner, recipient, each debit asset and
maximum spend, each credit asset and minimum receipt, selected pool/order Script
identities, fee ceiling and owner capacity-debit ceiling. Its local v1 intent
commitment includes the exact values and amount encoding. It supports explicit
canonical eight-byte `u64`, sixteen-byte `u128`, and the finance protocol's
eighty-byte `ShareData` profile. The LP profile fixes its complete pool hash and
controller code hash, checks all sixty-four configuration bytes, and requires
the full LP Script's thirty-two argument bytes to equal the CKB hash of that
tail. It does not treat arbitrary longer payloads as amount prefixes. Other
token layouts/extensions require their own application decoder.

An intent declares at least one debit or credit limit. Placing an escrow order
can have only a debit; cancellation can have only a credit from the escrow.
An empty debit list still grants no authority to consume colored owner Cells.
This permits one-sided actions without weakening explicit spend limits.

Every `FinanceRecovery::prepare` reads the actual packed transaction outputs and
resolved input Cells. It checks each input OutPoint, full asset/recipient Script
identity, checked aggregate amounts, net owner debit, net recipient receipt,
actual transaction fee and net owner capacity debit. Every owner asset input
requires an explicit debit limit: a recipient credit grants no permission to
spend that asset from the owner. A credit asset consolidated without owner loss
can declare a zero debit limit. The application then gives the adapter exactly those same resolved
input Cells; a separately supplied quote or another input observation cannot
replace them.

Before counting owner output capacity as a refund, every colored owner output
must match a complete asset Script explicitly declared in a debit or credit
bound, and pass that bound's exact amount codec. This check also applies when
the credit's intended recipient differs from the owner. An undeclared Type
cannot turn unusable capacity into a reported owner refund. Untyped outputs
remain governed by the owner's exact Lock, including when they contain data.

The adapter rechecks canonical live-resolution and dependency bindings through
`protocol_bundle_live_resolution_evidence` and
`protocol_bundle_ready_to_sign_evidence`. It recomputes output and occupied
capacity from actual candidate bytes. Its opaque intent commitment alone does
not prove any financial predicate; the application checks remain mandatory.
Executing Lock/Type Scripts must independently enforce custody, asset supply,
pool/order transitions and economic correctness against a hostile builder.

## Attempt state and failure handling

The Rust recovery policy fixes maximum unsigned rebuilds, maximum original-
transaction observations and required confirmation depth. No library method
starts a polling loop or sleeps. The caller supplies bounded RPC/backoff and
cancellation scheduling; unavailable observations retain their reason.

| Situation | Client action |
| --- | --- |
| Discovery unavailable | Record `DiscoveryUnavailable`; retain the original intent. No transaction is assumed rejected. |
| Unsigned input/quote stale | Mark `RebuildRequired`; refresh Cells/dependencies, rebuild within identical intent, repeat preflight and sign the new bytes. |
| Unsigned RPC unavailable | Retain the prepared attempt and the unavailable classification. |
| Unsigned preflight/signer rejection | Record rejection separately from RPC failure. |
| Signer cancelled before signed bytes bind | Stop as `Cancelled`. |
| Signed attempt | Preserve the original raw and signed-serialization identities. Replacing the candidate is prohibited. |
| Submission begins | Validate the exact signed/tx-pool receipt chain **before** the network call. |
| Submission RPC times out | Mark `SubmissionUnknown`; query the original hash. Never refresh state and execute a second economic operation automatically. |
| Original transaction pending or unknown | Continue bounded observation of that same hash; neither status authorizes replacement. |
| Signed execution, tx-pool or transaction-status rejection | Retain the explicit rejecting stage and reason. No automatic new economic attempt follows. |
| Observation unavailable | Retain `ObservationUnavailable`; do not turn it into rejection, non-inclusion or a reorg. |
| Included or required depth observed | Bind the existing submission/confirmation receipts, network and original required depth. Confirmation remains a bounded chain observation. |
| Inclusion disappears or changes | Record `Reorged`, count the change and continue following the original transaction. Never duplicate its payout. |
| Cancellation after signed bytes exist | Record cancellation requested and continue original-transaction observation. Client cancellation cannot revoke already signed/submitted bytes. |
| Observation/rebuild bound exhausted | Stop automatic work with the unresolved original identity retained; caller intervention is required. |

Changing an asset, participant, recipient, operation or limit requires a new
explicitly authorized intent. The existing tracker cannot silently adopt it.
A rejected or disappeared signed attempt does not automatically authorize a new
economic operation: the client must reconcile original state and obtain the
necessary caller decision separately.

## Durable original-attempt journal

Applications start with `FinanceRecovery::start_or_resume`, passing the persistent
journal and the expected original intent/network/policy. A new operation returns
`FinanceRecoverySession::Fresh`; an already recorded operation returns
`FinanceRecoverySession::ObservationOnly`. The latter branch must not request
a signature and exposes no prepare/sign/submit API. A partial, corrupt or
intent/network-mismatched existing record returns an error instead of a fresh
client. Both the ordinary restart test and separate-process test use this entry.

`FinanceRecovery::begin_submission` requires a `FinanceJournal`. Before it
advances to `Submitting`, it obtains an exact validated snapshot from the
adapter and durably records the full financial intent, resolved input Cells,
original unsigned/signed Molecule transactions, exact byte hashes, attempt
number, network and fixed recovery policy. The caller may invoke the existing
adapter RPC submission method only after this method succeeds. Persistence
failure leaves the client in `Signed`; no submission phase is granted.

The journal writes a same-directory temporary, synchronizes its file, installs
it atomically without replacement, then synchronizes the directory. A separate
write-once reference pins the payload and original transaction identities.
Files use private permissions on Unix. This requires a filesystem supporting
hard links and file/directory synchronization; failures are explicit. Partial
temporaries are never recovered as submission records. A crash before the
reference completes blocks recovery; retrying persistence of the exact original
record can safely finish that installation.

One operation ID permits one original signed submission in this store.
Concurrent or restarted clients cannot overwrite it with different transaction
bytes, including after timeout/NotFound. Each separately authorized user command
must have its own operation ID; reusing an old ID does not authorize a repeat.
The storage key fixes that ID, rather than the mutable contents of a newly
supplied quote, chain or limits. Those values are separately fixed by the full
intent binding in the reference. Changing them cannot select another file and
bypass the original operation's write-once record.
This local ID is not a consensus nonce or new signing protocol.

On restart, `FinanceJournal::restore_observation_only` requires the expected
financial intent, chain/genesis, attempt and policy. Closed, versioned schemas,
the durable reference, payload hash, canonical amounts/Script/Molecule encodings
and all original transaction hashes must agree. The stored financial constraints
are reconstructed and checked against actual retained Cells/outputs again.
No checked live/signing/tx-pool receipt is deserialized or recreated from an
asserted JSON flag.

The result is the adapter's separate `ObservationOnlyRecovery` type. It can
follow only the original transaction on its original chain and calculate bounded
confirmation-depth observations. It has no prepare, sign, submit or replacement
method. Its initial state is `SubmissionUnknown`, including when the former
process died between persistence and the network call. Unknown, unavailable or
rejected observations never restore economic execution authority. An inclusion
change remains observable after depth was previously observed.

The journal contains public transaction/Cell data and signatures, never private
keys. It assumes the application-controlled local journal directory is trusted;
integrity hashes do not authenticate a filesystem controlled by an attacker.
It is not a distributed idempotency service. Observation bounds apply to each
loaded observer session; restarting a session does not regain submission or
economic retry rights. The existing RPC adapter remains responsible for actual
node calls and canonical-chain rechecks.

The low-level `FinanceRecovery::new` constructor remains available for explicit
in-memory use. It is not a global signing fence: two initially concurrent fresh
clients can request signatures before either records a submission. The write-once
journal prevents a different transaction from crossing the send boundary, and
the default restart factory suppresses new signing for an existing record.
Applications requiring global exclusion before the first signature must also
serialize initial signer access; this policy does not claim distributed locking.

## Focused evidence

The owning host tests are `protocol_bundle::tests::recovery` in the adapter.
They use actual packed candidates, the existing canonical materializer and
adapter receipt functions, with synthetic node observations. They cover changed
amounts, full asset arguments, recipient and fee; same-input binding; deterministic
rebuilds and new signatures; retry bounds; cancelled signing; submission timeout
followed by unknown/pending/committed outcomes; RPC unavailability; rejection;
confirmation-depth changes and reorg observations.
Real filesystem tests cover partial records, failed persistence, write-once
conflicts and strict schema/chain/intent/attempt checks. A separately spawned
test process restores the journal without inherited tracker/receipt objects and
can only observe the original transaction; its node observations remain synthetic.
The separately selected `real_pool` smoke consumes a fresh authentic pool report,
checks original raw/serialized identities, accepts actual LP mint/transfer/withdraw
Cells and rejects altered LP tails, amounts and recipients. It establishes client
codec integration with those retained transactions, not a new execution or quote
correctness proof. It requires the owning `finance_protocol` report to exist.

These tests are client-policy evidence. They do not supply genuine financial
Script execution, pinned-node contention/latency measurements, frozen financial
operation budgets or security review. The maintainer cancelled mandatory
FIN-05/FIN-10 outside-developer reproduction on 2026-11-07; any such exercise is
optional and is not claimed by these tests. Existing compiler cost baselines
are unchanged. The maintainer
requested this work without gates or CI; no such pass is claimed here.

The separately runnable `finance_node` example uses these same application APIs
with genuine enforcing assets, ordinary compiled pool/reserve/LP verifiers,
upstream secp256k1 signatures, actual RPC admission, and node commitment. It
persists before a real send, deliberately discards that successful send's
acknowledgement, restores the journal's observation-only branch, and follows
the original pending/committed transaction and required depth. The injection
does not establish that an organic RPC timeout occurred. A competing candidate
first passes actual node preflight and then receives a stale-input rejection
after the original commits; that is separate from a financial Script rejection.
Its report retains full transactions and exact observations. The public command,
second profile, bootstrap assumptions and remaining acceptance scope are in the
example README. The complete lane also exercises escrowed orders, two pool routes
in both amount modes, direct order/pool settlement with no taker B prefunding,
and authenticated shared-output allocation with actual 32-input/32-output
commitment and stale-plan whole-Cell refunds. Intent funding with one custom
Script group uses the ordinary action adapter and candidate checks, with no
claim of a ProtocolBundle or durable recovery journal for that funding step.
Running these as an implementer does not establish optional outside-developer
reproduction, freeze all financial budgets or complete the full adversarial
composition matrix.
Exact source hashes and report cases define the evidence of each run.

The separate public `--lane operations` experiment adds actual same-order
fill/cancel contention, two commits with disjoint pool/wallet inputs, and an
unsigned rebuild against a genuinely committed successor. The rebuild changes
funding and fee within the original immutable limit, rejects an old signature
binding and signs the rebuilt bytes through the upstream SDK. Three sequential
instances of the same pool-contention pattern retain individual window samples,
minimum/median/maximum and commits per active test-window second. They are not
simultaneous traffic or a sustainable network-capacity result.

Controlled wallet callbacks reject or cancel through the real SDK sighash
adapter on actual resolved candidates; these are injected callback failures,
not a claim about a physical wallet or a human cancellation. A local HTTP proxy
also forwards a real send, retains the actual successful node response and
withholds it beyond the SDK's explicit deadline. The resulting SDK timeout is
checked as a transport timeout. Restart still yields only the original-hash
observer. Actual private-node `truncate` clears its inclusion/tx pool; the
observer records `Reorged`, and observes changed inclusion of the same signed
original after the private driver rebroadcasts identical bytes. The observer
itself has no rebroadcast API. No new payment or signature is created by this
recovery experiment.

Earlier reports sampled the active compiler binary path only at completion,
which cannot establish continuous CLI identity during concurrent builds. Their
archived ELF/metadata/transaction bytes retain their narrower execution facts.
New runs create a private compiler snapshot before any compiler subprocess and
check its hash afterward; all lock/builder subprocesses use that snapshot.
New whole-operation measurements include the compiler-owned data/witness
encoding before packing, with separate unavailable reasons and overlapping
phase scopes. Acceptance of their new frozen namespace must be read from its
own independent validation, not inferred from the historical v2 pass.

The final source's two new frozen namespaces passed independent fresh runs:
`complete-c4-final-validation-v1` retained 78 case records and 26 whole-operation
samples; `operations-final-validation-v1` retained 39 records and 10 samples.
Both report `passed-measured-scope` with no violations. The recovery run's
three same-shape windows actually sent six candidates: three commitments,
three stale conflicts and no new economic retries within those windows.
The separate unsigned-state experiment executed one authorised rebuild.
The disjoint control committed both transactions, and the actual callback
rejection/cancellation attempts sent none.

The [freeze and validation history](../examples/ckb-sdk-builder/resource-limits-history/whole-operation-decision-v1.json)
binds both reports, compiler snapshots, running application/checker identities,
fixed limits and unavailable measurements. It preserves the earlier scope-label
correction without changing any case ceiling. Historical second-profile timing
failures, gate evidence and independent security review retain their separate
dispositions. Outside-developer reproduction is optional under the maintainer's
2026-11-07 decision and is not a remaining acceptance blocker.
