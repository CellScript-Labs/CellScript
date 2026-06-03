# Audit Status

## Current Status

NovaSeal Agreement Profile v0 is a reviewable CKB-native agreement skeleton with
audited terminal-path structure, local transaction-shape evidence, resolved
transaction verifier evidence, live devnet lifecycle evidence, and fixed-width
wallet signing vectors. It now also has a deterministic public profile
certification gate exposed through `cellc certify --plugin novaseal-profile-v0`.

## Latest Results

| Command | Result |
| --- | --- |
| `cellc check --target-profile ckb` | passed |
| `cellc audit-bundle --target-profile ckb --json` | passed |
| `cellc explain-assumptions --target-profile ckb` | passed |
| `cellc check --target-profile ckb --primitive-strict 0.16` | passed |
| `cellc src/nova_agreement_lifecycle_type.cell --target riscv64-asm --target-profile ckb --entry-action nova_agreement_lifecycle` | passed |
| `python3 scripts/nova_agreement_tx_shape_harness.py --pretty` | passed; 8/8 expectations matched |
| `cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_tx_harness -- --pretty` | passed; 20/20 script-layer and node-verifier expectations matched |
| `scripts/novaseal_agreement_devnet_stateful_live.py --pretty --ckb-repo ../ckb --ckb-bin ../ckb/target/debug/ckb` | passed; originate -> repay, originate -> claim, and live negative dry-runs |
| `python3 ../../../scripts/novaseal_wallet_signing_vectors.py --pretty` | passed; includes 3 Agreement vectors |
| `../../../target/debug/cellc certify --plugin novaseal-profile-v0 --json` | passed local Rust compiler-hosted production-prep and profile certification gates, including `agreement_profile_public_ecosystem_certification_v0`; external production attestations still required |

Generated audit surface:

- actions: 3
- locks: 0
- source units: 3
- ProofPlan records: 170
- builder assumptions: 78
- runtime gaps: 0

Primitive-strict 0.16 is clean for the current Agreement Profile bundle. Current
executable evidence is the resolved transaction harness plus the live devnet
lifecycle runner.

## Commands

```bash
../../../target/debug/cellc check --target-profile ckb
../../../target/debug/cellc audit-bundle --target-profile ckb --json
../../../target/debug/cellc explain-assumptions --target-profile ckb
../../../target/debug/cellc check --target-profile ckb --primitive-strict 0.16
python3 scripts/nova_agreement_tx_shape_harness.py --pretty
../../../target/debug/cellc src/nova_agreement_type.cell --target riscv64-elf --target-profile ckb --entry-action originate_agreement -o target/nova-agreement-originate-action.elf
../../../target/debug/cellc src/nova_agreement_type.cell --target riscv64-elf --target-profile ckb --entry-action repay_before_expiry -o target/nova-agreement-repay-action.elf
../../../target/debug/cellc src/nova_agreement_type.cell --target riscv64-elf --target-profile ckb --entry-action claim_after_expiry -o target/nova-agreement-claim-action.elf
../../../target/debug/cellc harness/ckb_vm/always_success_lock.cell --target riscv64-elf --target-profile ckb --entry-lock always_success -o target/nova-agreement-always-success-lock.elf
cargo run --manifest-path harness/ckb_vm/Cargo.toml --bin novaseal_agreement_tx_harness -- --pretty
python3 ../../../scripts/novaseal_agreement_devnet_stateful_live.py --pretty --ckb-repo /path/to/ckb --ckb-bin /path/to/ckb/target/debug/ckb
python3 ../../../scripts/novaseal_wallet_signing_vectors.py --pretty
../../../target/debug/cellc certify --plugin novaseal-profile-v0 --json
```

## Claim Classification

| Claim | Status | Classification |
| --- | --- | --- |
| Package is separate from NovaSeal core | implemented | source-guard-present |
| CKB/CKB only | implemented | source-guard-present |
| Origination guards | implemented | source-guard-present |
| Repay before expiry | implemented | source-guard-present |
| Claim after expiry | implemented | source-guard-present |
| Stable lifecycle type-script identity | implemented | source-guard-present |
| Receipt output materialization | implemented | resolved-transaction-covered + live-devnet-covered |
| Terminal AgreementCell resource transition soundness | implemented | source + generated strict + resolved transaction + live devnet |
| Executable fixture shape harness | implemented | local-transaction-shape-covered |
| Legacy per-action CKB VM fixture harness | superseded | legacy-action-harness-superseded |
| Resolved transaction verifier harness | implemented | resolved-transaction-covered |
| Native CKB occupied-capacity rejection | implemented | resolved-transaction-covered |
| Native CKB payout output binding | implemented | resolved-transaction-covered + live-devnet-covered |
| Terms hash output binding | implemented | resolved-transaction-covered |
| Receipt hash output binding | implemented | resolved-transaction-covered |
| Fixed-width wallet signing vectors | implemented | production-gate-covered |
| Public ecosystem profile certification | implemented | compiler-certification-covered |
| BTC collateral support | out of scope | not implemented |

## Fixture Honesty

The local harness executes the builder-visible transaction shapes for
origination, repayment, default claim, time rejects, party rejects,
under-capacity reject, and wrong-settlement reject.

The legacy action CKB VM harness is no longer part of the current pass/fail
claim because the Agreement surface moved to signed-intent witness shapes and a
single lifecycle type-script entry. The resolved transaction harness and live
devnet lifecycle runner are the current executable evidence.

The resolved transaction harness constructs deterministic CKB transactions,
loads action code and a local always-success lock through CellDeps, and runs both
`ckb-script` and `ckb-verification`. It covers the same terminal-path cases plus
the transaction-layer under-capacity reject. The wrong-settlement fixture is now
resolved-transaction-covered through a typed `NativeCkbPayoutV0` output mismatch.
The harness now fails unless every fixture file is covered by resolved
transaction evidence.

## Receipt Honesty

Receipts are materialized as outputs. The `receipt_hash`/`latest_receipt_hash` value is
carried through state and receipt fields, and receipt output mismatches are
covered by resolved transaction evidence plus the live devnet lifecycle runner.
Fixed-width wallet signing vectors are generated by the root
`scripts/novaseal_wallet_signing_vectors.py` and checked by the compiler-hosted
certification gate through the Rust-generated NovaSeal certification report.
Public/shared CellDep attestation and external BIP340 TCB review remain
production blockers.

## Production Statement Boundary

The current local certification result is sufficient to say that the checked-in
Agreement package satisfies NovaSeal profile certification requirements for the
local evidence set. It is not sufficient to say that a public mainnet deployment
is production ready until the two external attestations named by
`target/novaseal-production-gates.json` are present and
`cellc certify --plugin novaseal-profile-v0 --require-production` passes.
