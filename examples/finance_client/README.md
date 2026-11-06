# Bounded finance client

These application helpers use the ordinary protocol packages in
[`../finance`](../finance/README.md). They are public source examples, with
generated Cell-data and entry-witness encoders, checked ProtocolBundle
construction, the existing CKB adapter and upstream secp signing. They do not
replace those encoders, the on-chain verifiers or node validation.

`quote.rs` owns the admitted numerical domain and pure quotations:

- `initialise`, `deposit` and `withdraw` account for the permanently locked
  1000-share minimum, proportional mint/burn and unused asset refunds.
- `swap_exact_in` and `swap_exact_out` handle both directions. Exact output
  returns only the desired amount, caps actual input and identifies the unused
  input and output rounding retained by the pool.
- `route_exact_in` and `route_exact_out` accept exactly two `RouteLeg`s. Each
  includes its full pool and asset identities, direction and resolved state.
  Duplicate pools and disconnected asset paths reject. Intermediate amounts
  agree exactly; exact output is quoted backward from the last pool.
- `order_entitlement` and `fill_order` use the cumulative ceiling rule. An
  overpayment is a donation, not credit against a subsequent fill. Zero
  entitlement increments reject, including terminal dust.

Quotation is based on supplied state. Resolve the actual live predecessors and
complete Script identities before construction, and bind the final signed
transaction to the immutable user intent. The ordinary Type/Lock groups enforce
their own pricing, custody, supply and recipient rules against that transaction.

`recovery.rs` checks actual resolved owner assets, recipients, net debits, fees
and capacity against the immutable intent. `journal.rs` records a durable
operation identity and exact unsigned/signed transaction bytes. Resuming an
operation whose submission began permits observation of the original hash;
it does not create another payment. The [recovery contract](../../docs/CELLSCRIPT_FINANCE_RECOVERY.md)
defines the state machine and its concurrency and reorganisation boundaries.

The runnable public SDK example and exact local-node commands are in
[`../ckb-sdk-builder`](../ckb-sdk-builder/README.md#disposable-financial-node-example).
It pins the node source/binary, resolves genuine deployed CellDeps, constructs
the formal bundle, signs with the upstream SDK, preflights and actually submits
transactions to its disposable node. Its report separates client fault
injection, node observations and confirmation depth.

Focused independent checks from the repository root are:

```bash
cargo test --locked -p cellscript --test finance_math -- --nocapture
cargo test --locked -p cellscript --test finance_quotes -- --nocapture
cargo test --locked -p cellscript --test finance_protocol -- --nocapture
cargo test --locked -p cellscript --test finance_orders -- --nocapture
```

`finance_quotes` uses an independently written Node BigInt model; exact-output
input is also found by monotone integer search. `finance_protocol` and
`finance_orders` execute full transactions with real asset policies and
standard signatures, including hostile builder cases. Generated evidence lives
under `target/finance-protocol-evidence/`. On 2026-11-07 the maintainer cancelled
the mandatory named outside-developer reproduction/report/transaction-evidence
requirement in #55/#60. External reproduction is optional; these internal runs
do not claim it. Independent security review and other requirements remain.
No gates or CI were requested for this work; their acceptance remains pending.
