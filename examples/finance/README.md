# Bounded financial protocol packages

These are ordinary author-defined CellScript packages implementing the
[selected financial profile](../../docs/CELLSCRIPT_FINANCE_PROFILE.md). They
have no financial compiler intrinsics, administrative reserve key or implicit
cross-contract calls. They are implementation candidates, not accepted,
audited or publicly deployed financial products.

The [C1 settlement intent](settlement/README.md) and
[two-pool route](route/README.md) packages describe their own executing
compositors and admitted net-flow contracts.

## Freeze and deployment dependencies

Compile `asset`, `reserve`, `lp`, `minimum` and `escrow` first. Materialise a separate copy
of `deployment/src/lib.cell` containing their exact CKB ELF data hashes and the
selected chain/network domain, then lock and freeze `pool` with those ordinary
source-package dependencies and `../finance_math`. The tracked all-zero
deployment inputs fail closed for real asset and custody Scripts. Retain the
materialised source snapshot, `Cell.lock`, compiler/checker identities, ELF,
metadata, lowering/source-map records and actual deployed CellDeps together.
The dependency keys are the actual `finance_deployment` and `finance_math`
package names; there is no implicit rename of their package coordinates.

`finance_deployment` exposes seven `public fn() -> Hash` source inputs:
`asset_code_hash`, `reserve_code_hash`, `lp_code_hash`, `minimum_code_hash`, and
`network_domain`, plus `escrow_code_hash` and `allocation_code_hash` for the
order/allocation packages. Configuration is source input to freezing, not a builder
override after executable identities have been selected. All application code
uses immutable `data2` identities. The pool state uses the built-in Type ID;
there is no application upgrade or LP-mint escape.

The dependency order has no hash cycle. LP Type args commit the pool Type ID and
the final pool **code** hash, rather than its configuration-dependent controller
Lock hash. Pool controller args then commit the complete configuration,
including the resulting complete LP Type identity. Reserve args bind the full
pool, controller and asset Script hashes.

## Data and witness contract

Use the compiler-generated fixed Cell-data encoders and entry-witness encoders.
The table below describes their packed bytes; it is not a second manual codec.
All quantities occupy 16 little-endian bytes (`u128`) and must be at most
`1_000_000_000_000`. Their upper 64 bits therefore must be zero. At most 32
transaction inputs and 32 outputs are inspected.

| Package/resource | Fields and byte offsets | Script args |
| --- | --- | --- |
| `asset/AssetData` | `amount:u128` at 0; plain wallet data is exactly 16 bytes; `CustodyData` additionally admits the 96-byte subject/controller/asset tail (112 total) | Complete issuer Lock hash, exactly 32 bytes |
| `reserve/ReserveData` | `amount:u128` at 0; `pool`, `controller`, `asset` hashes at 16, 48, 80; total 112 | CKB hash of the 96-byte tail |
| `lp/ShareData` | `amount:u128` at 0; `pool`, `pool_code` hashes at 16, 48; total 80 | CKB hash of the 64-byte tail |
| `pool/PoolState` | `pool`, `asset_a`, `asset_b`, `lp`, `minimum_lock`, `network` hashes at 0,32,64,96,128,160; `fee:u64` at 192; `nonce:u64` at 200; `reserve_a`, `reserve_b`, `total_lp:u128` at 208,224,240; `revision:u64` at 256; total 264 | CKB hash of the first 208 bytes |

`asset` and `lp` expose ordinary `action verify()` entries. `reserve` and
`minimum` expose `lock verify()` entries. They have no payload parameters;
their generated empty entry payload is still placed using the normal
WitnessArgs ABI. `minimum` always rejects a spend, including destruction and
attempted capacity recovery.

The pool exposes this ordinary Lock entry:

```cell
verify(
    mode: u64, input_base: u64, output_base: u64,
    a_max: u128, b_max: u128, shares: u128,
    min_a: u128, min_b: u128, owner: Hash,
) -> bool
```

All parameters are witnesses encoded by the generated ABI. `owner` is the full
Lock Script hash of the authenticated funding and user asset/LP inputs, not a
pool administrator. Any owner with the admitted assets may operate the pool.

| Mode | Meaning of quantity parameters |
| --- | --- |
| 0, initialise | `a_max`, `b_max` are initial reserves; `shares` is minimum provider LP; `min_a=min_b=0` |
| 1, A→B | Input is `a_max>0`; output must meet `min_b`; `b_max=shares=min_a=0` |
| 2, B→A | Input is `b_max>0`; output must meet `min_a`; `a_max=shares=min_b=0` |
| 3, deposit | `a_max`, `b_max` are deposit maxima; `shares` is minimum LP received; `min_a=min_b=0` |
| 4, withdraw | `shares>0` is authentic LP burned; outputs meet `min_a`, `min_b`; `a_max=b_max=0` |

The same ordinary Lock also has composable reserve-leg modes. They retain all
state, configuration, actual reserve, predecessor-sibling, capacity and supply
checks, but consume/create only the three state/reserve roles. `owner` must be
the zero Hash and `shares=0`; these modes do not claim ownership of a user's
wallet or select its recipient. Genuine asset Type groups conserve the final
transaction, and an executing user/compositor Lock enforces chosen peers,
destinations and intent limits.

| Mode | Exact reserve-leg contract |
| --- | --- |
| 5, A→B exact input | `a_max>0` is exact A input; `b_max=min_a=0`; `min_b` bounds B received |
| 6, B→A exact input | `b_max>0` is exact B input; `a_max=min_b=0`; `min_a` bounds A received |
| 7, A→B exact output | `b_max>0` is exact desired B output; `a_max>0` bounds A input; both `min` fields are zero |
| 8, B→A exact output | `a_max>0` is exact desired A output; `b_max>0` bounds B input; both `min` fields are zero |

For an exact output `b` from reserves `x,y`, the required A input is exactly
`ceil(x*b*10000/((y-b)*(10000-fee)))`. It must fit the admitted reserve bound and
the user's maximum. Only the desired B amount leaves reserves. Any integer
surplus implied by that input stays in the pool; a larger exact-input quote
cannot be substituted for exact-output semantics. The reverse direction is
symmetric. This permits authentic reserve-to-reserve intermediate flow between
two different pools without manufacturing an intermediate input from an output
of the same transaction or requiring the trader to pre-own the bridge asset.

The immutable fee numerator is 0–1000 with denominator 10000. The swap uses
`floor(y*a*(10000-fee)/(x*10000+a*(10000-fee)))`, so the fee is never rounded
first. Deposits mint `s=min(floor(a_max*L/x),floor(b_max*L/y))` shares and use
`ceil(s*x/L)`, `ceil(s*y/L)` actual assets. Refunds are checked against actual
input amounts and owner outputs. Withdrawals use floor-rounded pro-rata amounts
and cannot reduce supply below the permanently locked 1000 shares.

## Canonical transaction roles

Initialisation first creates a separately funded, untyped bootstrap Cell under
the final controller Lock, with the exact proposed 264-byte state data. Its
spend executes that Lock. The genesis transaction uses input 0 for signed owner
funding, input 1 for bootstrap, and inputs 2/3 for owner assets A/B. Type ID is
anchored to this actual first input and state output index 0. Witness bases are
`input_base=1`, `output_base=0`.

For updates, `input_base` identifies state at +0, actual reserves A/B at +1/+2,
and signed owner funding at +3. Swaps and deposits additionally use owner asset
A/B inputs at +4/+5. Withdrawal instead uses the owner's LP input at +4.

Every operation creates state and reserves at `output_base +0/+1/+2` and owner
asset outputs at +3/+4. The remaining roles are:

| Operation | Further outputs relative to `output_base` |
| --- | --- |
| Initialise | Provider LP +5, locked minimum LP +6, owner fee/change +7 |
| Swap | Owner fee/change +5 |
| Deposit | Provider LP +5, owner fee/change +6 |
| Withdraw part of an LP Cell | Owner LP remainder +5, owner fee/change +6 |
| Withdraw an entire LP Cell | Owner fee/change +5; no zero LP Cell is created |

Fee/change outputs are untyped, empty and locked to the owner. State and reserve
capacity is preserved on updates. Asset wallet refunds preserve their input
capacity. A partial LP remainder preserves its input capacity; full LP
consumption requires the owner's fee/change output to contain at least the old
LP Cell capacity. That refundable capacity cannot pay transaction fees. Funding
pays newly occupied capacity and transaction fees. New minimum-share capacity
is permanently locked together with those shares. The owner signs the exact
final transaction, including its fee and change.

Additional unrelated Cells are permitted within the 32/32 bound. They cannot
replace a canonical role, count as a refund or satisfy another obligation. A
composed transaction may place separate pool role blocks at different bases;
each still executes its own Lock/Type groups against the same transaction.

## Inventory, supply and hostile-builder checks

Reserve identity is stronger than matching data or Lock args. On every update,
each reserve input must have the exact same previous transaction hash as the
state input and the state output index +1/+2. State successors always create
the next such sibling pair. An unsolicited equal-looking reserve from another
transaction therefore grants no inventory, LP or synchronisation credit. Such
donations have no recovery path in this profile; including one as an extra
spend duplicates its custody Lock group and rejects.

The LP Type checks the entire group. Without a matching pool it requires exact
input/output sum equality for transfers and splits/merges. If the pool is
present it always checks
`LP_outputs + previous_supply == LP_inputs + next_supply`, even when LP group
input/output sums happen to be equal. Extra compensating LP Cells cannot hide a
missing mint or burn. Genesis additionally requires the authentic bootstrap
Lock and unique Type ID output. A positive standalone LP destruction with no LP
outputs still runs the input Type group and rejects.

Required full-transaction negatives include correctly signed reserve-only
spending, omitted state, substituted full Script identities, duplicate roles,
same-amount unsolicited reserve substitution, incorrect predecessor sibling,
forged LP, missing or offsetting LP mint/burn, standalone LP destruction with
valid holder authorisation, locked-minimum spending, wrong owner/refund,
incorrect retained-fee rounding, failed slippage, zero-share deposits,
out-of-domain amounts and changed state/reserve capacity. Record the actual
rejecting Script group rather than accepting an unrelated signature failure.

Local CKB-VM execution and a host-maintained predecessor set do not establish
node admission, live-cell contention, finality or human security review. Those
remain separate evidence in the financial profile. External reproduction is
optional under the maintainer's 2026-11-07 decision and is not claimed here.

## Persisted order and actual escrow

The independent `order` and `escrow` packages implement the selected cumulative
pricing policy without maker participation in a fill. The order's built-in
Type ID anchors its unique state. A separately executed bootstrap Lock checks
actual initial escrow and the maker's signed funding/asset inputs. Cancellation
requires a real input under the committed maker Lock; witness naming alone
grants no cancellation authority.

`OrderState` has seven hashes (`order`, `sold_asset`, `requested_asset`, `maker`,
`proceeds`, `remainder`, `network`) at 0,32,64,96,128,160,192; positive
`numerator`, `denominator`, `total_sold:u128` at 224,240,256; `nonce:u64` at 272;
`filled`, `entitlement:u128` at 280,296; `revision:u64` at 312. Total length is
320 bytes. Controller args commit the first 280 immutable bytes. Each quantity
is at most `1_000_000_000_000`, including total maker entitlement
`ceil(total_sold*numerator/denominator)`; a greater total rejects at genesis.
`EscrowData` is 112 bytes: `amount:u128` plus `order`, `controller`, `asset`
hashes, and args commit the 96-byte tail. Its authentic input must be the
preceding state transaction's exact +1 sibling.

The ordinary order Lock entry is
`verify(mode:u64,input_base:u64,output_base:u64,sold:u128,paid:u128,taker:Hash,requested_issuer:Hash)`.
The last witness reconstructs the exact admitted requested-asset Script from
the frozen test-asset code hash, `data2` and its issuer args. Its full hash must
equal the immutable requested identity in every mode, including genesis; the
witness can never select a different requested asset.
Modes 0/1/2/3 mean initialise/partial fill/final fill/cancel. On initialise and
cancel, `sold=paid=0` and `taker` must equal the committed maker. On a fill,
`sold>=1`, `paid>=1`, and the actual maker proceeds must cover
`ceil((filled+sold)*N/D)-entitlement`. This increment must be positive even on a
final fill. Actual overpayment goes to proceeds as a donation and is never
added to the stored entitlement or credited against later fills. A final dust
remainder without an admissible positive-payment fill stays escrowed until
maker cancellation. There is no expiry or implicit clock.

| Operation | Canonical inputs | Canonical outputs |
| --- | --- | --- |
| Initialise (`ib=1,ob=0`) | Maker funding 0, bootstrap 1, maker sold-asset wallet 2 | State 0, escrow 1, maker sold-asset refund 2, maker fee/change 3 |
| Partial fill | State `ib+0`, escrow +1, taker funding +2, taker requested-asset wallet +3 | Successor state `ob+0`, escrow +1, taker sold asset +2, proceeds payment +3, taker requested-asset refund +4, taker fee/change +5 |
| Final fill | Same as partial fill | Taker sold asset `ob+0`, proceeds payment +1, taker requested-asset refund +2, maker capacity refund +3, taker fee/change +4; no state/escrow successor |
| Cancel | State `ib+0`, escrow +1, maker funding +2 | Actual remaining sold asset to committed remainder recipient `ob+0`, maker fee/change +1; no state/escrow successor |

Partial fills preserve state, escrow and refunded requested-wallet capacity.
Final fills return exactly the old state plus escrow capacity in the separate
untyped empty maker-refund output. Cancellation preserves escrow capacity in
the returned sold-asset Cell and refunds at least the old state capacity to the
maker's fee/change output. The taker funds new payment/receive Cells and fees;
maker funding pays cancellation fees. Every recipient is a full committed or
authenticated Lock hash, and every asset is a full configured Type identity.

These basic distinct-output modes admit one order controller per transaction.
That restriction prevents two orders from claiming the same payment through
overlapping role blocks. It is not the two-order shared-output allocation mode;
that mode requires its own complete participating-order and aggregate-credit
contract and execution evidence before admission. A pool may independently
participate in the same final transaction under its own role block.

Order modes 4 and 5 are direct-funded partial/final fills using the same seven
witness parameters. They consume `state, escrow, actor funding` at
`input_base +0/+1/+2`; actor funding is an actual empty, untyped input under the
complete `taker` Lock. No requested-asset wallet input or artificial sold-asset
wallet output is required:

| Direct-funded mode | Outputs relative to `output_base` |
| --- | --- |
| 4, partial | State +0, escrow +1, authentic maker proceeds +2, actor fee/change +3 |
| 5, final | Authentic maker proceeds +0, exact maker state/escrow capacity refund +1, actor fee/change +2 |

The maker's authentic payment still equals `paid` and covers the positive
cumulative increment; immutable terms, current escrow, successor lineage,
requested-asset profile and capacity ownership remain enforced. The sold asset
may flow directly from escrow to a pool reserve, while the requested asset may
come directly from that pool. Actual token Type groups enforce conservation.
The executing actor/compositor Lock owns the buyer's chosen pool/order and net
asset-flow constraints; merely using these order/pool modes does not itself
admit a C1 settlement intent. One order controller remains the basic limit;
shared-payment multi-order settlement needs a separate complete allocation rule.

Order hostile cases must include underpayment with valid taker signatures,
zero-payment increments including terminal dust, excessive fill, changed price,
forged entitlement/remainder, wrong asset/recipient, absent or unsolicited
escrow, stale predecessor, unauthorised cancellation, missing capacity refund
and overlapping multi-order payment claims. Archive the actual rejecting
order/escrow/asset/holder group and keep local stale-input bookkeeping distinct
from node rejection evidence.

## Complete shared-payment allocation

`allocation` and order mode 6 implement the bounded shared-payment profile.
There are one or two actual persistent orders and at most four credit rows.
Every order independently checks the common participant set, authenticated
configuration and complete aggregate payment table; its own ordinary order
policy additionally checks its price, escrow and successor. The Allocation Lock
checks all participants' actual escrow, cumulative entitlement and successors,
as well as real authorisation. This separation keeps the closed executable
projections within their existing budgets.

The one consumed `AllocationData` descriptor is global input 0, untyped and
exactly 416 bytes. Its Lock args are exactly the full data hash. The immutable
header consists of `authorizer`, `allowed_order_code`, `network` hashes at
0,32,64; `auth_input`, `change_output`, `order_count`, `credit_count:u64` at
96,104,112,120; and `order0`, `order1` hashes at 128,160. Four flat rows start at
192, each 56 bytes: complete order Type hash, global output index `u64`, credit
`u128`. Use the generated `AllocationData` codec; no partial-width typed view or
separate offset encoder is admitted.

The participant list matches the actual inputs using the current Order code,
`data2`, full built-in Type ID and exact args shape, in global-input order.
Duplicate, omitted or foreign members reject. Credit rows are strictly ordered
by participant ordinal then output index; repeated `(order, output)` keys
reject. Unused participant/row fields are all zero. Each row derives its full
requested-asset Type and proceeds Lock from that actual order's authenticated
immutable terms, and matches both against the real payment output. Every row
referencing one global output contributes to the same aggregate; total credited
units cannot exceed that output's actual amount. Each order's total credited
units must cover its positive cumulative entitlement increment. Overpayment is
a donation and cannot alter later entitlement.

Mode 6 keeps the existing seven order witness parameters. Input roles are
state/escrow/empty allocator funding at `input_base +0/+1/+2`; output roles are
the authentic successor state/escrow at `output_base +0/+1`. `taker` must equal
the complete Allocation Lock hash of actual input 0, whose code/hash type/args
are independently verified. `paid` equals this order's aggregate table credit,
not the complete shared output amount. The mode remains a partial fill;
ordinary single-order final fill/cancellation retain their separate contracts.

Allocation `verify(operation:u64,auth_index:u64)` has two explicit operations:

- `0` executes the complete table. The witness index equals header `auth_input`.
  That input must execute the profile's pinned upstream
  `secp256k1-blake160-sighash-all` code
  `709f3fda12f561cfacf92273c57a98fede188a3f1a59b1f888d113f9cce08649`,
  `data2`, exactly 20 args bytes, and the complete committed authorizer Lock
  identity. The designated empty, untyped authorizer change preserves at least
  the descriptor capacity. This is the exact code used by the local upstream
  system-script dependency; it is not a public-network deployment claim.
- `1` cancels a stale/abandoned funded intent with a real authorizer input at
  the witness index. Every Cell in this Allocation Lock group is returned at
  its corresponding global output ordinal with unchanged full Type, data and
  capacity, under the authorizer Lock. Separate standard funding pays fees.
  No order peer or successful payment is inferred from this recovery operation.
  An accompanying order still independently verifies the full allocation table.

Compile the allocator after materialising the actual asset/escrow identities,
then materialise its exact code hash into the deployment package and refreeze.
The test requires the allocator ELF bytes to stay unchanged by its unused
self-hash input. Only then compile the order package. `allowed_order_code` is
data supplied after the order executable exists; every real order checks it
against its own executing code, avoiding a static deployment hash cycle.

The full-transaction C2 owner is
[`tests/finance_allocations.rs`](../../tests/finance_allocations.rs). Its admitted
maximum executes two orders with all four rows, two authentic shared payment
outputs and 32 transaction inputs/outputs. It also retains genuine rebound
tables, aggregate underpayment, omitted/duplicate members, wrong asset and
recipient, nonzero padding, fifth-row/33-input rejection, stale intent recovery
and altered type/data/capacity refunds. Local stale-input observations remain
separate from VM accepted/rejected transactions and node evidence.
