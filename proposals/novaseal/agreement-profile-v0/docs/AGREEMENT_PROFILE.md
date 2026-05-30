# Agreement Profile

NovaSeal Agreement Profile v0 is a profile package, not a change to NovaSeal
core. It gives financial meaning to an otherwise thin NovaSeal-style state
transition discipline.

## Design Motto

Core stays thin; profiles carry meaning.

NovaSeal core should remain focused on authority, typed intents, CKB Cell
transitions, nonce/expiry, policy hashes, and ProofReceipts. Agreement semantics
belong here.

## v0 Shape

The first slice models CKB-native agreements only:

- collateral asset: CKB
- principal asset: CKB
- fee: fixed fee
- terminal paths: originate, repay before expiry, claim after expiry
- no price feed
- no margin call
- no dynamic liquidation

Actor hashes are explicit fields and guards in this slice. Cryptographic locks
and BTC authority hooks are future profile slices.

The default claim path pays the locked collateral only. The fixed fee is a
repayment-path amount; adding it to the default claim would imply extra CKB
outside the locked agreement cell.

## Why This Is Not Ordinary Lending

Phroi's critique is respected: without oracle/margin-call machinery, this is not
ordinary overcollateralized DeFi lending. It is a priced terminal-rights
agreement. If the market makes one terminal path attractive, the party with that
right will exercise it.

That is the point: the agreement is digitally native because its terminal paths
are deterministic.

## Local Shape Harness

`scripts/nova_agreement_tx_shape_harness.py` checks the builder-visible output
shape for the CKB/CKB profile: occupied-capacity floors, principal payout,
repayment amount, collateral return, default collateral claim, time rejects,
party rejects, and wrong-settlement rejects.

`harness/ckb_vm` executes the compiled `originate_agreement`,
`repay_before_expiry`, and `claim_after_expiry` action ELFs in `ckb-vm`. It
covers the action/type-script layer for time guards, party guards, nonce
increments, receipt-root binding, receipt output fields, and preserved-field
checks.

`novaseal_agreement_tx_harness` constructs deterministic resolved transactions
and runs them through `ckb-script` plus the CKB non-contextual/contextual
verification stack. It uses a local always-success lock so that terminal input
transactions can reach the Agreement Profile type/action script.

These are still local evidence layers. They do not replace live-chain deployment
evidence, real CellDep liveness, production authority locks, concrete
payout-cell binding, or canonical hash preimage checks.
