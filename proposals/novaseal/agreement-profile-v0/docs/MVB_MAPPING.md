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
output capacities and modeled settlement amounts, a resolved
`ckb-verification` transaction harness over deterministic in-memory
transactions, and a live local devnet lifecycle runner. Payout intent is bound
by typed `NativeCkbPayoutV0` outputs; the resolved transaction harness covers
wrong settlement amount rejection, and the live runner covers originate ->
repay, originate -> claim, and negative dry-runs for wrong signer, non-CKB
asset kind, payout capacity, payout lock args, wrong payout amount, and early
claim.

It is still not production deployment evidence: public/shared CellDep pinning,
production locks, primitive-strict generated ProofPlan closure, and full
Molecule/wallet signing vectors remain future work.
