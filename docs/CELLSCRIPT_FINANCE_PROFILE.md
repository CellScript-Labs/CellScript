# Bounded financial protocol profile

Status: implementing; no financial acceptance, external reproduction or public
deployment is claimed. This record owns the initial application profile for
[#43](https://github.com/CellScript-Labs/CellScript/issues/43) and
[#52–#60](https://github.com/CellScript-Labs/CellScript/issues/52).
The [public roadmap](https://talk.nervos.org/t/cellscript-roadmap-expanding-the-frontier-of-ckb-native-defi/10936)
defines the architectural boundary. This work is on
`0.35`, based on `fc94b122`; the existing uncommitted
immediate-planner change under #99 is retained separately in that working tree.

## Ownership and acceptance

Codex implements the protocol packages, mathematical model, SDK integration and
local evidence under the maintainer's instruction. Separate automated reviewers
inspect arithmetic, protocol and client boundaries; that is implementation-team
review, not an outside-developer exercise or an independent human security audit.
On 2026-11-07 the maintainer cancelled the mandatory #55/#60 requirement for a
named developer outside the implementation team to provide clean-checkout
reproduction, a report and transaction evidence. External reproduction is now
optional and does not block programme acceptance. No such reproduction is
claimed. This decision does not waive independent human security review or
other evidence requirements. An independent human reviewer remains unassigned;
the required review disposition and actual results remain separate.

The maintainer requested no gates or CI for this work. Focused checks and their
exact source state are recorded individually. This instruction is not a passing
gate receipt and does not authorise a release or public deployment. Other
outstanding evidence requirements remain in force; the external-reproduction
change above is the separately stated maintainer decision.

## M0 evidence baseline

- `tests/support/instance_pool.rs` supplies real test-asset issuance, built-in
  Type ID, reserve custody, secp authorisation and complete transaction VM
  execution. Its actual default/diagnostic loop is two configurations; older
  prose saying O0–O3 does not establish four separately executed modes.
- That fixture has no LP supply lifecycle and constructs transactions and some
  witnesses directly. It does not close #52, #54, #55 or #60.
- `examples/amm_pool` and `tests/order_amm_scenarios.rs` contain useful formula
  and layout examples, not the required authentic custody/LP/order programme.
- Existing ProtocolBundle, generated entry encoders, formal CKB adapter and
  standard secp signing remain the construction path. No second serializer or
  finance-specific compiler branch is introduced.
- `ckb-testtool::Context::verify_tx` executes all required Script groups. A
  host-maintained live set proves local predecessor bookkeeping only; actual
  node live-cell, submission, contention and reorganisation observations stay
  separate requirements.

## Decisions for implementation

The following logical/numeric contracts are selected before protocol source
implementation. Transaction layouts and exact compiled identities belong to the
owning package and replay evidence. Measured case ceilings and their pilot,
freeze and validation identities are recorded in
[the resource record](CELLSCRIPT_FINANCE_RESOURCES.md); a logical bound below
is not a measured cycle budget.

| Decision | Selected contract |
| --- | --- |
| D01: assets | Two distinct enforcing test-asset Type Script identities, with amounts encoded as little-endian `u128`. These are labelled test assets. sUDT and all xUDT extension/owner modes are unsupported in this first profile; neither implementation nor issuer safety is inferred from compatibility. |
| D02: custody | Separate reserve asset Cells. Every reserve spend requires exactly one authentic matching pool predecessor and successor; every pool operation binds both actual reserves. Unmatched/duplicated custody groups reject. |
| D03: pool identity | Built-in Type ID anchored to the actual creation input/output. Configuration commits the ordered distinct asset identities, LP identity and custody policy. Same-pair pools are permitted only with different creation anchors and LP identities. |
| D04: shares | A dedicated LP Type Script enforces complete-group conservation for transfer/split/merge. Every supply change requires the matching pool transition and exact supply delta. Standalone destruction, including no LP output, rejects. |
| D05: numbers | Amount, reserve and total-share fields use `u128`, each at most `1_000_000_000_000`. Check bounds before arithmetic. At most 32 transaction inputs/outputs are inspected; checked aggregates fit `u128`. Wider mathematical helper contracts have their own explicit preconditions. |
| D06: fees | Immutable numerator `0..=1000`, denominator `10000`; fee remains in reserves. Swap quotation uses the exact scaled-input formula below, rather than rounding the fee first. No admin extraction or dynamic rate. |
| D07: minimum shares | Initialise `L=floor_sqrt(a*b)` with `L>1000`. Exactly 1000 authentic shares remain under a non-spendable Lock. They have no redemption, destruction or upgrade escape. `total_lp` includes them. |
| D08: deposits | Compute proportional shares from both maxima, use ceiling-rounded proportional amounts, and return unused user assets to the authenticated owner. Reject zero shares, exceeded limits and missing/reforged refunds. |
| D09a: zero payment | A fill whose cumulative entitlement increment is zero rejects, including a final fill. A client cannot silently replace this with a free fill or a one-unit donation. |
| D09b: minimum fill | Sold increment is at least one base unit and within remaining escrow; payment increment is at least one requested-asset unit and covers cumulative entitlement. No ticker/decimal interpretation. |
| D09c: final dust | No special final-fill exception. A remainder that cannot satisfy the ordinary rule stays escrowed until an admissible fill or maker-authorised cancellation. Cancellation returns actual remaining assets and refundable capacity. |
| D09d: surplus | Overpayment is a donation to the maker, never credit against a future fill. Store cumulative sold/entitlement separately from actual donated amounts; future obligations use the entitlement formula. |
| D10: time | No on-chain expiry in this profile. Client deadlines and observation timestamps are client policy, not consensus-visible current time. |
| D11: allocation | C1 uses distinct outputs per obligation. C2 uses one canonical bounded allocation table authenticated against the complete participating order set; every participant checks aggregate credits against actual paid outputs. |
| D12: composition | Initial logical shape: at most two distinct pools, each used once, at most two orders and four payment allocations; transaction scans are bounded by 32 inputs/outputs. Freeze measured byte/cycle/stack budgets after pilots and before final acceptance. |
| D13: upgrades | Exact executable/dependency identities, immutable configuration and no administrative reserve-spend or LP-mint path. Upgrades require a separately specified profile. |
| D14: capacity | State/reserve capacity is preserved on update. User/funding capacity, LP/order refundable capacity, change and transaction fees have explicit owners. Funding pays fees; token amounts and occupied capacity are separate quantities. Native CKB trading is unsupported. |
| D15: surplus custody | Only the state predecessor's exact same-transaction +1/+2 reserve OutPoints are recognised, using the full transaction hash and output indexes. Equal-looking unsolicited Cells cannot replace them. Extra reserve-like Cells grant no LP or deposit credit; their presence/spending must not bypass the matching transition or change recognised balances. No implicit indexer-driven synchronisation. |

The bound on normal financial inputs makes the largest direct swap numerator
`reserve * input * (10000-fee)` at most `10^28`, below `2^128`. Initial products,
deposit/withdrawal products and cumulative order-price products are at most
`10^24`. Denominators and checked totals are separately bounded before use.
Reject intermediate overflow even when a later quotient would fit.

## Pool and order equations

All formulae below operate on mathematical nonnegative integers. Runtime
arithmetic must prove or check the stated representation and bounds.

- Initialise: `L=floor_sqrt(a*b)`; provider gets `L-1000` shares and the locked
  minimum gets 1000. Actual assets fund both reserves.
- Deposit: `s=min(floor(a_max*L/x), floor(b_max*L/y))`,
  `used_a=ceil(s*x/L)`, `used_b=ceil(s*y/L)`; require positive `s`, user minimum
  shares and `used` within maxima. Both unused differences are explicit refunds.
- Withdraw: `a=floor(s*x/L)`, `b=floor(s*y/L)`; burn exactly the authentic
  circulating shares, honour recipient and minimums, and preserve locked shares.
- A-to-B swap: `n=a*(10000-fee)`,
  `b=floor(y*n/(x*10000+n))`, `x'=x+a`, `y'=y-b`. Require positive input/output,
  admitted final bounds, correct recipient and user minimum. B-to-A is symmetric.
- Exact B output: `a=ceil(x*b*10000/((y-b)*(10000-fee)))`, with
  `0<b<y`, positive bounded actual input and the user's maximum. Only `b`
  leaves reserves; any integer quote surplus stays in the pool. The reverse
  direction is symmetric. A minimum-output swap is not an exact-output swap.
- Order entitlement: `E(s)=ceil(s*N/D)` for immutable positive bounded `N,D`.
  A fill from cumulative sold `s` to `s+t` owes `E(s+t)-E(s)` and must pass
  D09a–D09d. Actual escrow, proceeds, immutable terms and unique predecessor
  lineage are checked by executing Scripts. Maker total entitlement and each
  taker's permitted spend/output are tested separately.

Pure X/Y→Y/Z routing consumes two prior pool states and creates their final
successors in the same transaction. Intermediate Y amounts agree exactly;
there is no attempt to consume an output created by that same transaction.
This track does not wait for order settlement. Shared order-payment allocation
does require authentic order settlement and complete aggregate credit checks.

## Implementation and evidence map

| Issue | Required work | Current disposition |
| --- | --- | --- |
| #52 | Asset/pool/LP identities, custody, supply and first continuous slice | Ordinary packages and complete-transaction custody/supply negatives implemented; full acceptance pending |
| #53 | Checked arithmetic, independent BigInt model, builder/model/VM agreement | Wide-math VM, full signed direct/relation swap comparison and high-u128 narrowing negatives passed; exact-output/routes also independently compared |
| #54 | Full AMM/LP lifecycle and generated construction | Complete VM lifecycle and two actual pinned-node basic profiles passed; budgets/review pending |
| #55 | Different admitted asset pair through public interfaces; external exercise optional | Implementation-team profiles recorded; mandatory external reproduction cancelled on 2026-11-07 |
| #56 | Authentic escrow, cumulative partial fills, final fill/cancel and hostile cases | Ordinary packages, node lifecycle, genuine wrong-asset/refund/forged-state negatives and one-unit/maximum-number VM boundaries passed; full review acceptance pending |
| #57 | C1 one-order/one-pool final transaction with distinct payments | Complete VM/Bundle transaction and actual node settlement/unwrap passed; gross-input/net-gain/recipient/remainder negatives passed; full budget/review acceptance pending |
| #58 | C2 allocation, independent C3 routing and C4 layouts/unrelated Cells | C2 and C3 full-transaction paths passed on VM and actual node, including four credits and 32/32 cardinalities; scope-specific review acceptance pending |
| #59 | Immutable-intent recovery, unknown submission observation and node contention/costs | Recovery and actual submissions passed; 164 selected VM cases passed the original plus additive budgets; historical node v2 scope and its second-profile timing violations remain explicit |
| #60 | Complete M0–M6 acceptance map; outside reproduction optional | Local evidence recorded; remaining review/gate requirements retain their own disposition |

Every accepted execution must retain source/compiler/checker/ELF/CellDep and
raw/serialized transaction identities, resolved Cells, predecessor lineage,
actual expected/rejecting Script boundary and capacity balance. Sign malformed
economic transactions correctly so negatives reach the intended policy.
Positive builder preflight does not replace raw hostile full-transaction tests.

Pilot reports distinguish complete-transaction cycles, per-group observations,
ELF/witness/transaction bytes, static and observed stack, occupied/refundable
capacity, host construction time and unavailable measurements. Freeze budgets
before final runs; preserve existing compiler baselines and failed results.
Node admission/confirmation, security review and public deployment each need
their own actual evidence. Optional outside usability reproduction, if reported,
remains a distinct evidence layer.

## Current executable shapes

The [ordinary packages](../examples/finance/README.md) and
[public client helpers](../examples/finance_client/README.md) own their complete
layout and encoding contracts. C1 currently admits a direct-funded partial
order fill and A-to-B pool leg with distinct maker payment. The taker funds A
and capacity, owns no B input, and receives the actual net A gain. The executing
intent Lock binds its selected pool/order, maximum gross A input, minimum net
A gain, fee/capacity limits, retained intent and real upstream secp authorizer.
The escrow, order, pool, reserves, assets and Type IDs execute independently.

C3 admits exactly two distinct X/Y and Y/Z pools, each used once. Its two
amount modes enforce actual X spend and Z receipt, matching Y reserve deltas,
and unchanged LP supply. No external Y wallet may be interposed. The fixed
role prefix and unrelated trailing Cell policy are explicit; accepted trailing
permutations do not authorise moving a required prefix role. A separate
authorizer-approved unwrap recovers residual intent funds after an operation
or a stale predecessor. It is not a successful swap.

The two successful basic node profiles used different actual issuers/owners,
fees, nonces and reserves. They cover real issuance, pool/LP initialisation,
bidirectional swaps and partial/all circulating LP redemption, leaving the
1000 minimum. Two candidates on the same predecessor were independently
preflighted; committing one caused the other to reject for the stale input.
The unknown-outcome exercise deliberately discarded an acknowledgement after
submission and recovered by observing the original transaction hash. This is
client fault injection, not evidence of an actual network outage or reorg.
Reports retain node/compiler/source/executable/transaction identities and
actual confirmation observations. These runs were by the implementation team.

Later node profiles also execute two makers' cumulative partial/final/cancel
order paths and both C3 amount modes. A further real C1 path spends 50 A into a
20,000 A / 100,000 B pool, sends its actual 248 B to the maker for 100 A from
escrow, and returns 250 A to the taker that supplied only 200 A. Authorised
unwrap and maker cancellation of the residual 900 A were committed as separate
transactions. C2's first shared-payment VM path credits one actual payment of
12 exactly once as 5 plus 7. The complete node lane also executes four credits
against two payments of 6 at 32 inputs/outputs, and authoriser-approved refund
of a stale funded plan with original type/data/capacity preserved.

Eleven independent archived snapshots retain 282 real VM cases plus two separate
local stale observations. All 282 cases replayed from exact bytes into fresh
Contexts. Independent shared-allocation economics covers 21 actual archived
cases; the separate combined C1 model covers 21 actual cases, including
equivalent 32/32 trailing layouts against the same predecessors. The original
107 selected VM budgets are preserved, with 57 newly measured cases under a
separate additive freeze. The historical reference node run passes 68 business cases and the measured
v2 resource scope. Its two contention windows each include two actual signed
submissions, one commitment and one stale rejection, with zero new economic
retries. The second node pilot still has three timing-ceiling violations;
neither its business correctness nor a successful reference timing run erases
those failures.

That historical node runner used an active `cellc` path: its final path hash
cannot establish that every subprocess used one immutable CLI executable
during concurrent builds. Its archived artifact/Cell/transaction observations
remain evidence of their actual bytes. The new operation experiments snapshot
the compiler before any subprocess and verify that snapshot throughout;
their independently frozen measurements and validation are separate records.

The final `complete-c4-final-validation-v1` run passed 78 records and 26
full-operation budget samples; `operations-final-validation-v1` passed 39
records and ten recovery-experiment samples. Both used the unchanged compiler
snapshot and matching compiled application sources, and both reported no
frozen-budget violations. C1/C2/C3 each execute real unrelated tail permutations
with whole-cell refunds and only one selected commit. Recovery adds repeated
pool competition, a disjoint-pool control, actual fill/cancel contention,
updated-state rebuilding, wallet callback failure, genuine SDK timeout and
actual private-node reorganisation observations. Their scopes and immutable
report identities belong to the [resource record](CELLSCRIPT_FINANCE_RESOURCES.md)
and its decision history; these remain implementation-team experiments.

[Offline archive replay](CELLSCRIPT_FINANCE_ARCHIVE_REPLAY.md) restores exact
resolved Cells into a fresh Context without recompiling application source.
[Operation resource evidence](CELLSCRIPT_FINANCE_RESOURCES.md) separates ordinary
transaction cycles, independently replayed rejecting-group costs, observed
guest stack and native Type ID execution. These owners do not substitute for
node observations or outside reproduction.

## Admission of a new identity

The known pool/order identities in these fixtures and public-node examples come
from their actually executed bootstrap/genesis transactions, with complete
predecessor lineage retained. A new identity must be admitted through that
creation history before a client treats it as this profile's pool or order.
The public example retains the identities it creates itself; discovery of
arbitrary third-party pools is outside this client portfolio.

Built-in Type ID alone proves uniqueness. An output Lock does not execute on
creation, so an attacker can fund a fresh Type ID state and reserve/escrow
siblings without running this profile's initialisation Lock. Such a new pool
can contain nominal supply without authentic issued LP/minimum shares; a new
order can claim prior filled state. This does not reproduce a previously
admitted full Type ID or authorise spending its existing reserves. It is still
insufficient creation evidence. Neither a matching current data layout nor a
current artifact receipt proves that past bootstrap execution occurred.
Clients must retain/check actual admitted genesis history instead of promoting
an arbitrary live Type ID into a trusted protocol instance.

## General prerequisite defects

The financial work exposed two general independent-checker admission gaps.
[#100](https://github.com/CellScript-Labs/CellScript/issues/100) binds complete
Script-hash/memory-hash helper bodies, private output regions and actual caller
ABI reads. [#101](https://github.com/CellScript-Labs/CellScript/issues/101)
establishes the complete physical scalar-slot layout for a bounded ordinary
forwarding Lock when dead parameter storage is reused before its exact-read
cache. Both have non-financial Document fixtures, independently decoded machine
contracts and rebound mutations. Their bounded proofs and focused checks do
not grant finance-specific admission or passing backend/integration gates.

## Normative references

- [Cell Model](https://docs.nervos.org/docs/ckb-fundamentals/cell-model).
- [sUDT specification](https://github.com/nervosnetwork/rfcs/blob/master/rfcs/0025-simple-udt/0025-simple-udt.md): its burn-permitting transfer rule cannot establish the stronger LP invariant.
- [xUDT specification](https://docs.nervos.org/docs/ecosystem-scripts/xudt): flags, owner modes and extensions require explicit admission; none are silently enabled here.
- [Instance-policy evidence boundary](CELLSCRIPT_INSTANCE_POLICY.md#executed-two-pool-application-slice).
- [ProtocolBundle](CELLSCRIPT_PROTOCOL_BUNDLE.md).
- [Gate policy](CELLSCRIPT_GATE_POLICY.md).
