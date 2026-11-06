# Two-pool route Lock

This ordinary application package protects an explicitly funded X-only intent
through two different X/Y and Y/Z pools. It has no order dependency. The pools,
reserve Locks, asset Types, Type IDs and real standard secp signer still execute
their own checks in the same final transaction.

The profile admits two pools, each used once, with three distinct full asset
Script hashes. Inputs are pool A's state/reserves at 0–2, pool B's at 3–5,
intent-owned X at 6, intent-owned capacity at 7, the 328-byte descriptor at 8,
and its real standard authorizer at 9. Outputs are both pool blocks at 0–5,
intent-owned X change at 6, intent-owned Z at 7, capacity change at 8, the
identical descriptor at 9 and authorizer capacity at 10. Up to 32 inputs and
outputs may include unrelated trailing Cells; intermediate Y is admitted only
in the two authentic reserve positions. Moving a required prefix role is not
an admitted permutation.

Entry arguments are `(mode, intent_index, authorizer_index)`. Mode 0 spends
exactly the descriptor's maximum X and enforces its minimum Z. Mode 1 receives
exactly its desired Z and enforces maximum X and minimum Z. Both bind the
actual first pool's Y decrease to the second pool's Y increase. Pool modes 5
and 7 respectively validate forward floor pricing and backward ceiling
pricing. Any integer surplus in an exact-output quote remains in reserves.
There is no transaction-local consumption of a newly created output and no
pre-funded user Y substitute for the intermediate flow.

The descriptor commits both full pool identities/controllers, all three full
asset identities, the authorizer, amounts, maximum fee, maximum capacity debit
and nonce. Its complete data hash is the Lock argument. The authorizer uses
the bundled `ckb-system-scripts` v0.6.0 secp executable under `data2`; this is a
test-profile identity, not a claim about a public network deployment. A real
authorizer capacity input and signature are required on every operation.
Mode 2 returns all held X, Z and capacity to that authorizer, including after
the chosen pool predecessor becomes stale. It does not execute a route.
Capacity credit counts only empty untyped outputs or admitted sixteen-byte X/Z
wallets under that complete authorizer Lock. A foreign Type that prevents later
spending cannot stand for a refundable capacity balance. Unwrap permits a loss
of at most the actual transaction fee, rather than the unused fee allowance.
The route Lock independently recomputes both floor legs for exact input and
both backward ceiling legs for exact output, so peer witness mode changes
cannot donate quoted output or consume extra input while retaining the same
terminal limits.

`tests/support/finance_route.rs`, registered by `tests/finance_protocol.rs`,
compares the public Rust quotations with independent Node BigInt calculations
and verifies actual complete transactions with generated data/witness encoding
and formal ProtocolBundle construction. Reports distinguish local VM evidence,
node acceptance, frozen budgets and optional external reproduction. This
package does not by itself establish the latter three.
