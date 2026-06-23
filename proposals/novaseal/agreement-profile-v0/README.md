# NovaSeal Agreement Profile v0

**Status**: production-ready CKB-native Agreement source package with audited terminal-path
structure, local transaction-shape evidence, resolved transaction verifier
evidence, live devnet lifecycle evidence, fixed-width wallet signing vectors,
wallet/lock digest alignment, and local production-gate checks. Public/shared
CellDep attestation and external BIP340 TCB review remain public/mainnet
deployment evidence; BTC SPV and RWA legal/registry evidence belong to the
profiles that use those external facts.

**Roadmap position**: this package is the current NovaSeal **v0.2 Agreement
Profile** stage. The package/schema name remains `agreement-profile-v0` because
it is the first version of this profile, not because the roadmap stage is v0.

This package is inspired by Matt's Minimum Viable Borrowing idea, but it is not
a general lending protocol. It models a Cell-native financial agreement with
pre-agreed terms and deterministic terminal paths.

## Boundary

NovaSeal core stays thin; profiles carry meaning.

This package is separate from `../v0-mvp-skeleton/`. It does not modify the
NovaSeal core state type, BTC verifier surface, or generic verifier registry.
It is allowed to call itself a NovaSeal profile only because `Cell.toml`
declares `conforms_to = "NovaSealCanonicalV0"`, pins `canonical_schema_hash`,
and the deterministic production and devnet acceptance gates verify that
declaration against the canonical schema hash, manifest and source surface.

Implemented in this slice:

| Area | Status | Classification |
| --- | --- | --- |
| Separate Agreement Profile package | implemented | source-guard-present |
| CKB/CKB terms only | implemented | source-guard-present |
| `originate_agreement` | compiles | source-guard-present |
| `repay_before_expiry` | compiles | source-guard-present |
| `claim_after_expiry` | compiles | source-guard-present |
| `nova_agreement_lifecycle` stable type entry | compiles | source-guard-present |
| Receipt output materialization | implemented | resolved-transaction-covered + live-devnet-covered |
| Primitive-strict 0.16 | passes | generated-strict-covered |
| Fixture shape harness | implemented | local-transaction-shape-covered |
| Legacy per-action CKB VM harness | superseded | legacy-action-harness-superseded |
| Resolved transaction harness | implemented | resolved-transaction-covered |
| Native CKB occupied-capacity rejection | implemented | resolved-transaction-covered |
| Native CKB payout output binding | implemented | resolved-transaction-covered + live-devnet-covered |
| Terms hash output binding | implemented | resolved-transaction-covered |
| Receipt hash output binding | implemented | resolved-transaction-covered |
| Checked terminal arithmetic | implemented | source-guard-present + local-arithmetic-boundary-covered + production-gate-covered |
| Fixed-width wallet signing vectors | implemented | production-gate-covered |
| Wallet/lock digest alignment | implemented | production-gate-covered |
| BTC UTXO mirror / SPV / OP_RETURN | out of scope | not implemented |

Do not call this "trustless borrowing". Better names:

- Agreement Profile
- Cell-native financial agreement
- pre-agreed terminal contract
- handshake / option-like agreement
- oracle-free BTCFi primitive

## Actions

`originate_agreement`

- creates an `Active` agreement from pre-agreed CKB/CKB terms
- checks positive collateral/principal
- checks start/expiry window
- checks borrower-originated actor hash
- creates a typed borrower principal payout output
- creates a `NovaAgreementReceiptV0`

`repay_before_expiry`

- consumes an `Active` agreement
- requires `now <= expiry`
- requires borrower actor hash
- creates a terminal `Repaid` agreement
- creates typed lender repayment and borrower collateral-return payout outputs
- creates a receipt with checked `terminal_amount = principal + fixed_fee`

`claim_after_expiry`

- consumes an `Active` agreement
- requires `now > expiry`
- requires lender actor hash
- creates a terminal `Defaulted` agreement
- creates a typed lender default-claim payout output
- creates a receipt with `terminal_amount = collateral`

The default path intentionally does not add `fixed_fee` on top of collateral.
The agreement cell only models locked collateral, so default claim must not
imply extra CKB is minted or supplied by the claimant.

## Receipt Semantics

Receipts are runtime-relevant because the actions materialize
`NovaAgreementReceiptV0` outputs. The actions also update `latest_receipt_hash` from
the witness `receipt_hash`, and the materialized receipt output must carry the
same latest receipt hash.

Native CKB settlement intent is materialized as `NativeCkbPayoutV0` outputs.
The local transaction harness also checks that the CKB capacity/value shape
matches those typed payout amounts. Fixed-width wallet signing vectors for the
Agreement terminal intents are generated in
`/home/arthur/a19q3/CellScript/target/novaseal-wallet-signing-vectors.json`.

Authority identifiers are signature and display identifiers in this profile.
They are not a general payout-recipient schema: payout routing is committed by
`payout_commitment_hash`, typed payout outputs, and the transaction output shape.

## Commands

```bash
/home/arthur/a19q3/CellScript/target/debug/cellc check --target-profile ckb
/home/arthur/a19q3/CellScript/target/debug/cellc audit-bundle --target-profile ckb --json
/home/arthur/a19q3/CellScript/target/debug/cellc explain-assumptions --target-profile ckb
/home/arthur/a19q3/CellScript/target/debug/cellc check --target-profile ckb --primitive-strict 0.16
python3 scripts/nova_agreement_tx_shape_harness.py --pretty
/home/arthur/a19q3/CellScript/target/debug/cellc src/nova_agreement_type.cell --target riscv64-elf --target-profile ckb --entry-action originate_agreement -o target/nova-agreement-originate-action.elf
/home/arthur/a19q3/CellScript/target/debug/cellc src/nova_agreement_type.cell --target riscv64-elf --target-profile ckb --entry-action repay_before_expiry -o target/nova-agreement-repay-action.elf
/home/arthur/a19q3/CellScript/target/debug/cellc src/nova_agreement_type.cell --target riscv64-elf --target-profile ckb --entry-action claim_after_expiry -o target/nova-agreement-claim-action.elf
/home/arthur/a19q3/CellScript/target/debug/cellc src/nova_agreement_lifecycle_type.cell --target riscv64-elf --target-profile ckb --entry-action nova_agreement_lifecycle -o target/nova-agreement-lifecycle-type.elf
/home/arthur/a19q3/CellScript/target/debug/cellc harness/ckb_vm/always_success_lock.cell --target riscv64-elf --target-profile ckb --entry-lock always_success -o target/nova-agreement-always-success-lock.elf
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_tx_harness -- --pretty
python3 /home/arthur/a19q3/CellScript/scripts/novaseal_wallet_signing_vectors.py --pretty
/home/arthur/a19q3/CellScript/target/debug/cellc certify --plugin novaseal-profile-v0 --json
/home/arthur/a19q3/CellScript/scripts/novaseal_devnet_stateful_acceptance.sh --pretty --report-only
```

Latest local result: non-strict and primitive-strict CellScript commands pass.
The generated audit bundle reports 3 actions, 0 locks, 3 source units, 170
ProofPlan records, 78 builder assumptions, and zero runtime gaps. The local
transaction-shape harness reports 12/12 fixture expectations matched, including
four `principal + fixed_fee` and nonce max/max-1 arithmetic boundary cases. The resolved
transaction harness reports 20/20 script-layer expectations matched and 20/20
node-verifier expectations matched. The older per-action CKB VM harness is a
legacy ABI check for the pre-signed-intent action witness shape and is not part
of the current live stateful release gate.
The devnet stateful gate now reports zero lifecycle blockers and full live
stateful acceptance. The Agreement live runner deploys the BIP340 runtime
verifier and `src/nova_agreement_lifecycle_type.cell:nova_agreement_lifecycle`
as live CellDeps, submits originate -> repay and originate -> claim paths,
dry-runs wrong signer, non-CKB asset, payout capacity, payout lock args, wrong
payout amount, and early-claim negatives without consuming state, then verifies
the consumed active inputs are dead plus the closed agreement, payout, and
receipt outputs are live. The current aggregate status is `passed`. See
[docs/DEVNET_STATEFUL_ACCEPTANCE.md](docs/DEVNET_STATEFUL_ACCEPTANCE.md).

## Harness Boundary

`scripts/nova_agreement_tx_shape_harness.py` checks builder-visible CKB output
amount, occupied-capacity shapes, and local terminal-arithmetic boundary cases
for repayment amount and nonce increment overflow.

`harness/ckb_vm` contains a legacy per-action `ckb-vm` runner for the older
action witness ABI. The current release gate relies on the lifecycle type-script
runner plus the resolved transaction harness below.

`harness/ckb_vm` also contains `novaseal_agreement_tx_harness`, which constructs
deterministic resolved CKB transactions and runs both `ckb-script` and
`ckb-verification` over them. It uses a local always-success lock only so the
terminal input transactions can reach the Agreement Profile type/action script.
All fixture files are now covered by the resolved transaction harness.

These harnesses remain local verifier evidence, distinct from the live devnet
runner. Local devnet deployment pinning, wallet vectors, and wallet/lock digest
alignment are now gate-checked.
Public/shared CellDep publication, public BTC SPV evidence for BTC-facing
profiles, and external BIP340 TCB review remain production attestations or
external evidence, not local facts.

## Honest Next Slice

The next conservative slice should replace the local always-success lock with
real borrower/lender authority locks, then add public/shared CellDep
attestation, public BTC SPV evidence, RWA legal/registry review evidence, and
external BIP340 TCB review. Only after that should we consider expanding BTC
authority hooks or iCKB/xUDT variants.
