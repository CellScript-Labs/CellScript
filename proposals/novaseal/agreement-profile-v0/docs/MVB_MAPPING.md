# MVB Mapping

Matt's MVB framing maps cleanly to a Cell-native agreement:

| MVB idea | Agreement Profile v0 |
| --- | --- |
| Borrower locks collateral | `NovaAgreementCellV0.collateral_amount` plus local transaction-shape harness |
| Lender provides principal | `principal_amount` plus local transaction-shape harness |
| Pre-agreed terms | `NovaAgreementTermsV0` |
| Fixed term | `expiry_timepoint` |
| Repay before expiry | `repay_before_expiry` |
| Expired/default claim | `claim_after_expiry` |
| No oracle | no oracle fields or hooks |
| No margin call | no margin-call path |
| Receipts | `NovaAgreementReceiptV0` output |

## Important Reduction

MVB discussion touched CKB/iCKB and later broader market use cases. This first
slice intentionally reduces the scope to CKB/CKB so no external price ratio is
needed.

The current package includes a local transaction-shape harness for native CKB
output capacities and modeled settlement amounts, an action CKB VM harness for
the compiled CellScript terminal-path guards, and a resolved
`ckb-verification` transaction harness over deterministic in-memory
transactions. Payout-cell settlement remains local-shape-only until the
CellScript profile binds concrete payout outputs on-chain.

It is still not live deployment evidence: real CellDep liveness, mempool/miner
acceptance, production locks, canonical terms hashing, and canonical receipt
hashing remain future work.
