# Financial operation resource evidence

`tests/finance_costs.rs` measures the bounded financial fixtures separately from
the existing compiler cost baselines. Its inputs are the exact v2 transaction
archives described in [the replay contract](CELLSCRIPT_FINANCE_ARCHIVE_REPLAY.md).
No application compilation, transaction repair, witness rewrite or substituted
verifier participates in cost replay.

The default portfolio selects pool initialisation, deposit, both swaps and
redemption; order operations and hostile cases; C1; C2; both C3 amount modes;
and numerical/fee/capacity boundaries. C2/C3 maximum fixtures include two
participants, four allocation rows where applicable, and 32 inputs/32 outputs.
These are measured fixture ceilings. They are not a proof of a universal cost
bound for every possible transaction containing unrelated third-party Scripts.

## Metrics and authority

Every selected case first executes through ordinary
`ckb-testtool::Context::verify_tx`. Accepted cycles must match its producer
archive. The pinned verifier's actual group iterator supplies the independent
replay order and complete Script identities. Existing `cost_measurement` and
`cost_trace` owners provide the scheduler and instruction observer.

- For an accepted transaction, the sum of real group cycles must equal the
  ordinary complete-transaction cycle count.
- For a rejected transaction, the ordinary result must be a policy
  `ValidationFailure` in its expected group. The ordinary verifier exposes no
  rejected-transaction cycle count. The report separately records cycles from
  replaying groups in verifier order through the first failure, and the sum of
  all independent group replays, including groups after that failure. The latter
  is not the cost charged by an ordinary rejected transaction.
- Each guest group's instruction replay must agree with ordinary group
  execution on exit and cycles. Stack is observed relative to entry SP for
  each VM generation. The operation reports the maximum per VM, rather than
  adding independent group stacks. The standard secp verifier can dominate
  this maximum; per-group traces retain the compiler-generated verifier's
  separate observation.
- Built-in Type ID uses its actual native `verify_single` path. It has no guest
  VM or guest stack. Native failure cycles are explicitly unavailable when the
  API does not return them; they are never replaced with zero.
- ELF sizes, declared frame sizes, packed transaction/witness sizes, occupied
  output capacity and the actual capacity difference paid as fee come from the
  archived program/transaction bytes. A declared frame is not an observed or
  whole-call-chain stack bound. The latter is marked uncomputed in this owner.
- Diagnostic host elapsed time includes case restoration, ordinary verification,
  group replays and trace writes. It excludes the initial archive/source/bundle
  validation. It is not discovery, quote, construction, signing, RPC, node
  acceptance or confirmation latency. The public node example records those
  operations separately. Host OS/architecture, available parallelism, toolchain
  contract, Cargo lock hash and archive/program/transaction identities accompany
  the report.

Local stale-input observations do not enter the VM sample set. They are retained
under their own observation identities. Node acceptance remains separate from
these measurements. External reproduction is optional under the maintainer's
2026-11-07 decision and is not established by these measurements.

## Explicit commands

Produce fresh archives with the focused `finance_protocol`, `finance_orders`
and `finance_allocations` integration-test owners. Then run the pilot:

```sh
cargo test --locked -p cellscript --test finance_costs full_financial_operation_resource_pilot -- --ignored --nocapture
```

`CELLSCRIPT_FINANCE_COST_SCOPE` can select a comma-separated subset of
`pool,orders,c1,c2,c3,pool-boundaries`, plus the separately archived
`pool-admission`, `c1-layout`, `pool-math-direct`, `pool-math-relation` and
`c3-mode-boundaries` additions. Reports state the requested namespaces
and whether all six original namespaces were measured; a subset cannot stand
for the portfolio.
Reports and per-group traces live under
`target/finance-operation-costs/pilot/<scope>/`.

After an explicit pilot-based freeze,
`tests/fixtures/finance_operation_vm_limits.json` holds the per-namespace
ceilings and exact selected case multiplicities. Acceptance re-executes the
archives and checks coverage, cycles, observed stack, ELF/frame sizes,
transaction/witness sizes, occupied capacity and fee against those fixed limits:

```sh
cargo test --locked -p cellscript --test finance_costs full_financial_operation_vm_resource_acceptance -- --ignored --nocapture
```

The acceptance owner never derives or widens limits from its current run.
Missing cases, unavailable required measurements and exceeded ceilings fail.
It records the limits-file hash under
`target/finance-operation-costs/vm-acceptance/<scope>/`.

The 2026-11-06 freeze covers 107 selected cases from all six namespaces. Its
pilot-derived ceilings use 25% headroom and explicit integer rounding; fees keep
their exact measured ceiling. The independent acceptance rerun passed against
the fixed [limits file](../tests/fixtures/finance_operation_vm_limits.json).

| Namespace | Selected cases | All-group cycle ceiling | Transaction-byte ceiling |
| --- | ---: | ---: | ---: |
| Pool | 6 | 6,046,000 | 4,224 |
| Orders | 15 | 3,711,000 | 3,072 |
| C1 | 12 | 6,652,000 | 6,016 |
| C2 | 24 | 7,710,000 | 10,112 |
| C3 | 27 | 18,321,000 | 10,880 |
| Pool boundaries | 23 | 3,890,000 | 5,632 |

The maximum observed guest stack in these samples was 1,122,400 bytes, from
the upstream secp verifier; the frozen per-VM ceiling is 1,404,928 bytes.
Compiler-generated groups retain their own smaller observations in the traces.
All-group ceilings include independent replays after an actual first rejection,
as explained above; they are not presented as ordinary rejected-tx cycle counts.

The acceptance re-audit added direct admission, numerical, order and C4 cases.
The original 107-case limits file remains byte-for-byte unchanged. An
[additive freeze](../tests/fixtures/finance_operation_vm_extra_limits.json)
owns only the new cases and pins that original file's hash. The checker applies
the original ceilings and required multiplicities to every original case;
an extension cannot replace them, hide missing cases or accept duplicates.

| Additional namespace cases | New cases | All-group cycle ceiling | Transaction-byte ceiling |
| --- | ---: | ---: | ---: |
| Orders | 18 | 5,799,000 | 3,712 |
| C2 layout variants | 6 | 9,344,000 | 10,112 |
| Pool admission and numerical boundaries | 11 | 3,924,000 | 3,968 |
| C1 layout and participant boundaries | 12 | 16,519,000 | 11,008 |
| Complete direct arithmetic operation | 3 | 3,842,000 | 3,968 |
| Complete quotient/remainder relation operation | 5 | 3,796,000 | 3,968 |
| C3 substituted amount modes | 2 | 6,563,000 | 6,144 |

These 57 additional cases passed independent acceptance after their explicit
pilot-derived freeze. The fresh orders/C2 checks also revalidated their 39
original selected cases against the unchanged original ceilings. C1's larger
sample exercises 32 separately resolved inputs and 32 outputs; it does not
change the smaller original C1 case budget. Exact source/archive and limits
identities are in the corresponding `orders-c2-pool-admission` and `c1-layout`
acceptance reports. A final combined run re-executed all 164 selected cases
across all eleven namespaces against the final limits-file identities and
passed. The original 107-case file and every numerical ceiling from the earlier
additive freeze remain unchanged; the later high-u128 tests only extend the
required math case inventory. The initial additive file is retained in
`tests/fixtures/finance_resource_limits_history/`.
Native Type ID failure cases whose cycles are unavailable
remain byte-replay evidence and are excluded from this numerical cost scope.

The additional snapshots can be checked without rerunning unrelated scopes:

```sh
CELLSCRIPT_FINANCE_COST_SCOPE=orders,c2,pool-admission cargo test --locked -p cellscript --test finance_costs full_financial_operation_vm_resource_acceptance -- --ignored --nocapture
CELLSCRIPT_FINANCE_COST_SCOPE=c1-layout cargo test --locked -p cellscript --test finance_costs full_financial_operation_vm_resource_acceptance -- --ignored --nocapture
CELLSCRIPT_FINANCE_COST_SCOPE=pool,orders,c1,c2,c3,pool-boundaries,c1-layout,pool-admission,pool-math-direct,pool-math-relation,c3-mode-boundaries cargo test --locked -p cellscript --test finance_costs full_financial_operation_vm_resource_acceptance -- --ignored --nocapture
```

Passing these local VM case ceilings does not set
`frozen_financial_budgets=true`: full host/node timing and contention budgets,
review disposition and the programme's other acceptance requirements need their
own evidence. No gates or CI are invoked by these focused commands.

## Measured node experiment policy

The public example accepts `--resource-limits` with
[`finance-node-resource-limits.json`](../examples/ckb-sdk-builder/finance-node-resource-limits.json).
It validates the exact reference/second profile parameters before starting a
node, then checks the expected case multiplicities, available complete-tx RPC
cycle maxima, bytes, capacity, fees and measured timing events. Missing values
stay nullable; a required missing value or exceeded ceiling fails. Runtime
does not rewrite the limits file.

The final reference experiment passed all 68 business cases and its v2 measured
resource scope, including 25 pure-quote and 60 packing/build observations. Each
of the pool and allocation contention experiments actually submits two signed
candidates serially: one commits and the unchanged stale candidate receives
the node's `-301` unresolved-OutPoint error. Neither candidate is rebuilt or
re-signed as another economic operation. These windows are not simultaneous
load tests or estimates of public-network capacity.

Quote timing covers the public pure quotation closure. Build timing covers
capacity balancing, resolved action construction and packed materialisation;
data/witness pre-encoding and later raw-transaction mutations are excluded.
Artifact/live resolution is an aggregate stage. This seeded, exact-deployment
profile has no dynamic indexer discovery stage. Node-observed guest stack and
memory remain unavailable; the separate local VM observer supplies its own
stack evidence.

The [decision history](../examples/ckb-sdk-builder/resource-limits-history/decision-v2.json)
retains the v1 freeze and a real failed validation. Two journal sync timings and
a pool window exceeded v1. The explicit v2 policy changes only the additive
journal/window allowance to one second; it is an experimental acceptance
ceiling, not an enforced timeout or statistical tail guarantee. 1,594 other
existing non-null ceilings and all quote/build ceilings remain unchanged.
New actual stale-send and available RPC-summary metrics have separate origins.

The second v2 pilot still exceeded three retained ceilings in its maximum
allocation case:

| Stage | Observed ms | Ceiling ms |
| --- | ---: | ---: |
| SDK signing and receipt | 178.806495 | 141 |
| Signed execution RPC and receipt | 109.703531 | 94 |
| Submission RPC and receipt | 93.071460 | 92 |

Those failures remain recorded. A successful reference validation does not
establish that both profiles always meet the timing policy. Host/cache/load
conditions, samples and exact executable/source identities belong with every
report; no percentile or latency SLA follows from these runs.

## Frozen full-operation and recovery experiments

The current runner has separate
[full-operation](../examples/ckb-sdk-builder/finance-node-whole-operation-limits.json)
and [recovery](../examples/ckb-sdk-builder/finance-node-recovery-limits.json)
namespaces. These retain the old v2 policy and its failed observations rather
than widening it. The new limits pin exact profile, compiler snapshot, running
application/checker, compiled application sources, financial ELFs, toolchain/SDK
inputs and ordered case/metric/availability inventory. A fresh run cannot derive
its own limits. The [decision history](../examples/ckb-sdk-builder/resource-limits-history/whole-operation-decision-v1.json)
preserves pilot identities and the identity-only correction of a reporting
scope label; that correction changed no numeric ceiling.

Both independent final validations passed their frozen measured scopes:

| Final run directory under `target/finance-node/` | Records | Budget samples | Wall time ms |
| --- | ---: | ---: | ---: |
| `complete-c4-final-validation-v1` | 78 | 26 full financial operations | 374,871.665645 |
| `operations-final-validation-v1` | 39 | 10 recovery experiment operations | 221,824.216152 |

The C4 run includes four real pairs of canonical/permuted preflights, one
selected commit per pair, complete third-party tail refunds, C2 at 32/32, and
three correctly signed C1/C3 one-shannon fee-overrun rejections at the expected
wrapper. Its full-operation clocks include canonical data/witness encoding,
funding/quote/construction, adapter checks, signing and actual inclusion or
intended failure. The samples also include the simple pool/order lifecycle
and existing composite late rejections. Nested phase times overlap.

The recovery run repeats one serialized contention pattern three times:
two actual signed submissions, one commit and one stale `-301` conflict per
window, with no new economic retry. Actual windows were 15,605.955701,
16,554.644283 and 16,906.006303 ms. Their minimum/median/maximum describe these
three samples; successful throughput over their combined active windows was
0.0611413796 commits/second. This is not concurrent load or network capacity.
The disjoint-pool control commits both transactions; a genuine same-order
fill/cancel pair commits once and rejects the unchanged stale candidate.

Further real operations re-resolve the updated pool successor and rebuild
within the same immutable intent, change funding and fee by one shannon,
reject the old signature and sign/commit the rebuilt candidate. Controlled
upstream wallet callbacks reject/cancel without sending. A TCP proxy forwards
the original transaction to the actual node, then holds/drops the response
past the SDK's 100 ms deadline. After restart the observer follows that same
hash through actual private-node `truncate`, disappearance and re-inclusion
in another block. The private driver rebroadcasts identical bytes; the
observer never obtains signing/replacement rights.

Both final reports retain `compiler_snapshot_unchanged=true`, the running
application identity and matching compiled application source hashes. Static
identity admission uses no indexer. Missing rejected-RPC cycle totals and node
VM stack/memory remain explicitly unavailable; the separate VM tracer owns its
stack evidence. Passing these finite experimental budgets does not establish outside
reproduction, a security review, a latency SLA or a public-deployment receipt.
