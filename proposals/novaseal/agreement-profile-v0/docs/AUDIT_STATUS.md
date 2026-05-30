# Audit Status

## Current Status

NovaSeal Agreement Profile v0 is a reviewable CKB-native agreement skeleton with
audited terminal-path structure, local transaction-shape evidence, action VM
evidence, and resolved transaction verifier evidence.

## Latest Results

| Command | Result |
| --- | --- |
| `cellc check --target-profile ckb` | passed |
| `cellc audit-bundle --target-profile ckb --json` | passed |
| `cellc explain-assumptions --target-profile ckb` | passed; ProofPlan soundness passed |
| `cellc check --target-profile ckb --primitive-strict 0.16` | passed |
| `python3 scripts/nova_agreement_tx_shape_harness.py --pretty` | passed; 8/8 expectations matched |
| `cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_ckb_vm_harness -- --pretty` | passed; 11/11 expectations matched |
| `cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_tx_harness -- --pretty` | passed; 12/12 script-layer and node-verifier expectations matched |

Generated audit surface:

- actions: 3
- locks: 0
- source units: 2
- ProofPlan records: 63
- builder assumptions: 17

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
/home/arthur/a19q3/CellScript/target/debug/cellc harness/ckb_vm/always_success_lock.cell --target riscv64-elf --target-profile ckb --entry-lock always_success -o target/nova-agreement-always-success-lock.elf
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_ckb_vm_harness -- --pretty
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_tx_harness -- --pretty
```

## Claim Classification

| Claim | Status | Classification |
| --- | --- | --- |
| Package is separate from NovaSeal core | implemented | source-guard-present |
| CKB/CKB only | implemented | source-guard-present |
| Origination guards | implemented | source-guard-present |
| Repay before expiry | implemented | source-guard-present |
| Claim after expiry | implemented | source-guard-present |
| Receipt output materialization | implemented | generated-audit-covered |
| Terminal AgreementCell resource transition soundness | implemented | generated-audit-covered |
| Executable fixture shape harness | implemented | local-transaction-shape-covered |
| Action CKB VM fixture harness | implemented | action-ckb-vm-covered |
| Resolved transaction verifier harness | implemented | resolved-transaction-covered |
| Native CKB occupied-capacity rejection | implemented | resolved-transaction-covered |
| Native CKB payout-cell settlement binding | local model only | local-transaction-shape-covered |
| Canonical terms hash | missing | blocked |
| Canonical receipt hash | missing | blocked |
| BTC collateral support | out of scope | not implemented |

## Fixture Honesty

The local harness executes the builder-visible transaction shapes for
origination, repayment, default claim, time rejects, party rejects,
under-capacity reject, and wrong-settlement reject. The hash preimage fixtures
remain blocked because canonical terms/receipt hashing is not implemented.

The action CKB VM harness executes the three compiled action ELFs and covers
origination, repayment, default claim, time rejects, party rejects, nonce
mismatch, receipt-root mismatch, and preserved-field mutation. It deliberately
does not claim capacity/payout-cell settlement or canonical hash preimage
coverage.

The resolved transaction harness constructs deterministic CKB transactions,
loads action code and a local always-success lock through CellDeps, and runs both
`ckb-script` and `ckb-verification`. It covers the same terminal-path cases plus
the transaction-layer under-capacity reject. The wrong-settlement fixture remains
local-shape-only because this profile does not yet bind payout cells on-chain.
The harness fails if the not-executed fixture partition differs from the known
blocker set: canonical terms hash, canonical receipt hash, and payout-cell
settlement binding.

## Receipt Honesty

Receipts are materialized as outputs. The `receipt_hash`/`receipt_root` value is
carried through state and receipt fields. The profile does not yet compute the
canonical receipt hash preimage on-chain.
