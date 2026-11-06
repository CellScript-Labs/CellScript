# Direct order/pool settlement intent

This ordinary application Lock encumbers the taker's assets and funding. It
requires the authentic selected pool and order, a real authorizer Lock input,
matched pool-B withdrawal and maker-B payment, unchanged LP supply, the chosen
maximum A input, minimum net A profit, fee and capacity limits. Removing a peer
or bypassing a builder cannot remove the intent Lock from its actual inputs.
The pool and order independently enforce their own reserves, escrow, arithmetic
and lifecycle. No compiler action name is special.

`SettlementIntent` is a fixed 280-byte source schema. Its seven complete Hash
fields occupy 0..224, maximum input/minimum profit are LE u128 at 224/240,
maximum fee/capacity debit are LE u64 at 256/264, and nonce is LE u64 at 272.
Lock args are exactly the 32-byte CKB hash of the complete intent Cell data.
The nonce is part of the commitment; transaction input liveness and a fresh
authorizer signature protect subsequent executions.

The first C1 profile is direct-funded A-for-B plus a partial A-for-B order fill.
No pre-existing taker B inventory is required. Canonical inputs are pool state
and two reserves (0..2), order state/escrow/taker funding (3..5), taker A (6),
intent (7), and a real authorizer capacity Cell (8). Outputs are pool state and
reserves (0..2), order successor/escrow/maker B payment/taker funding change
(3..6), taker A change including purchased order assets (7), unchanged intent
(8), and authorizer capacity refund (9). Extra unrelated Cells stay outside
these blocks and cannot change participant accounting. All transaction scans
are bounded by 32.

`tests/support/finance_settlement_layout.rs` exercises C4 at 32 inputs and
32 outputs with genuine separately signed trailing capacity Cells. Their full
capacity returns to their owner. The canonical candidate executes a complete
VM preflight without consuming its predecessors; a different trailing
input/output permutation uses that same predecessor set and commits once.
The fixed prefix is preserved. Moving its funding/asset roles, referring to
a trailing Cell as the intent, or exceeding either cardinality rejects.

The same suite replaces an omitted pool or order with honest signed external
funding, substitutes a separately initialized authentic pool, pays a genuine
third asset while conserving both Types, and preserves asset totals around a
reserve mismatch. Each rejects at the recorded protocol Script boundary.
An extra maker payment outside the obligation output cannot repair a short
payment in that output.

The public builder uses pool mode 5 and order mode 4 for direct reserve/escrow effects. This Lock
then enforces that their B amounts are the same actual payment and that the
taker's real net A result honours the immutable intent. Maker surplus payment
retains the order's documented donation semantics.

Entry arguments are `(mode: u64, intent_index: u64, authorizer_index: u64)`.
Mode 0 uses indices 7/8. Mode 1 permits authorizer-approved unwrapping: every
encumbered A/B amount and capacity less the actual transaction fee returns to the
authorizer, no output remains under this intent Lock, and no pool/order action
is needed. It prevents preparatory funding or residual funds becoming trapped.
The maximum fee bounds that actual fee; unused allowance cannot be redirected
to another owner. All authorizer outputs must be empty and untyped or valid
sixteen-byte wallets of the full admitted A/B Type identities. A foreign Type
that prevents future spending cannot count as a capacity refund. The admitted
authorizer is the exact bundled upstream secp executable under `data2`, with
its full owner Lock committed by the intent.

Source implementation is not execution acceptance. Full VM, generated-builder,
node and adversarial records name the exact tested source and compiled
identities before those evidence tiers are claimed. External reproduction is
optional under the maintainer's 2026-11-07 decision and is not claimed here.

After the C1 archives exist, `tests/finance_settlement_model.rs` runs the
independent BigInt model over their original resolved input/output bytes.
It checks the pool's fee and rounding, cumulative order entitlement, escrow
delta, full participant/asset identities, actual maker payment, user gain,
fee and capacity limits. Witness-only rejection remains separate from valid
economics. This local model and VM preflight do not establish node commitment
or optional outside-developer reproduction.
