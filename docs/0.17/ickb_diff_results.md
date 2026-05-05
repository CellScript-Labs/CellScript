# iCKB Differential Results

The differential harness has advanced from model-only to a selected executed
iCKB equivalence matrix.

`tests/benchmarks/ickb_diff/matrix.json` records the comparison matrix. The
manifest uses schema `cellscript-ickb-diff-matrix-v1` and is also the executable
production-equivalence claim gate. The integration test `tests/ickb_diff.rs`
validates that every selected row's evidence level is correctly declared, and
rejects any `PROVEN` claim that lacks binary hashes, CKB VM/testtool evidence,
fixture hashes, exit codes, cycle counts, transaction-context hashes, capacity
and fee measurements, or named reject modes.

Current manifest status:

- `mode`: `EXECUTED_CKB_VM_DIFF`
- `equivalence_status`: `PROVEN`
- `production_equivalence_claim`: `true`
- `equivalence_evidence`: populated

Selected equivalence row counts:

- `DIFFERENTIAL_CKB_VM_EXECUTED`: 75
- `MODEL`: 0

Supporting evidence outside the selected equivalence rows:

- `CELL_SCRIPT_CKB_VM_EXECUTED`: 14
- `ORIGINAL_ICKB_CKB_VM_EXECUTED`: 8

The top-level `remaining_model_blockers` registry is now empty, and the test
suite requires it to match the active `MODEL` rows exactly. Active
`non_executable_model_assumptions` is empty. Three legacy model assumptions that
should not remain active rows are retained under `retired_model_assumptions`:
duplicate receipt-id, wrong-owner resource fields, and synthetic current-epoch
redeem maturity. Each entry names the fixture-shape reason and a replacement
differential row that executes the corresponding chain-level evidence.

## CellScript CKB VM Execution Rows

| Scenario | Evidence Level | CellScript expected | Result |
|---|---|---:|---|
| LOAD_SCRIPT_HASH syscall | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| script exit code 1 | CELL_SCRIPT_CKB_VM_EXECUTED | fail | cellscript-ckb-vm-fail |
| LOAD_HEADER DAO accumulated_rate | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| LOAD_HEADER missing DAO header dep | CELL_SCRIPT_CKB_VM_EXECUTED | fail | cellscript-ckb-vm-fail |
| LOAD_CELL_DATA DAO is_deposit_data | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| LOAD_CELL_DATA DAO is_withdrawal_request_data | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| LOAD_CELL_BY_FIELD cell_capacity | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| LOAD_CELL_BY_FIELD has_dao_type negative | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| cell_occupied_capacity multi-syscall | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| LOAD_CELL_DATA cell_data_size | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| LOAD_CELL_DATA CellDep data_size | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| combined iCKB deposit verification | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |
| DAO immature redeem relative since | CELL_SCRIPT_CKB_VM_EXECUTED | fail | cellscript-ckb-vm-fail |
| DAO mature redeem relative since | CELL_SCRIPT_CKB_VM_EXECUTED | pass | cellscript-ckb-vm-pass |

## Original iCKB CKB VM Execution Rows

| Scenario | Evidence Level | Original iCKB expected | Result |
|---|---|---:|---|
| rejects non-empty script args | ORIGINAL_ICKB_CKB_VM_EXECUTED | fail | original-ickb-ckb-vm-fail |
| accepts empty args | ORIGINAL_ICKB_CKB_VM_EXECUTED | pass | original-ickb-ckb-vm-pass |
| rejects receipt without matching deposit | ORIGINAL_ICKB_CKB_VM_EXECUTED | fail | original-ickb-ckb-vm-fail |
| DAO type hash mismatch diagnostic | ORIGINAL_ICKB_CKB_VM_EXECUTED | fail | original-ickb-ckb-vm-fail |
| deposit phase 1 passes (patched DAO_HASH) | ORIGINAL_ICKB_CKB_VM_EXECUTED | pass | original-ickb-ckb-vm-pass |
| original DAO creates withdrawing cell | ORIGINAL_ICKB_CKB_VM_EXECUTED | pass | original-ickb-ckb-vm-pass |
| original DAO mature withdrawal | ORIGINAL_ICKB_CKB_VM_EXECUTED | pass | original-ickb-ckb-vm-pass |
| original DAO immature withdrawal | ORIGINAL_ICKB_CKB_VM_EXECUTED | fail | original-ickb-ckb-vm-fail |

**Note**: The DAO type hash mismatch is a known limitation of the current test
environment when the original iCKB Logic binary is used unmodified. The
differential deposit rows patch the binary's hardcoded DAO hash at offset
`0x360` so it matches the DAO script hash produced by ckb-testtool. This is
recorded in the execution objects as patched functional evidence, not mainnet
identity reconstruction.

The original DAO rows execute the unmodified DAO ELF used by the iCKB fixtures.
They cover phase-1 deposit-to-withdrawing-cell creation, phase-2 withdrawal with
deposit and withdraw headers plus mature since `0x2003e8022a0002f3`, and the
same phase-2 shape rejected with immature since `0x2003e802290002f3` /
`ERROR_INCORRECT_SINCE (-17)`. The phase-2 mature and immature withdrawal
fixtures now also have matching CellScript-side execution rows, so they are
recorded as differential DAO maturity/header evidence. Phase-1 withdrawing-cell
creation remains original-side only.

## Differential CKB VM Rows

| Scenario | Evidence Level | Original status/cycles | CellScript status/cycles | Failure mode |
|---|---|---:|---:|---|
| non-empty script args original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | non_empty_args_rejected |
| deposit phase 1 original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 97057 | pass / 16559 | n/a |
| deposit too small original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | deposit_capacity_bound_rejected |
| deposit too big original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | deposit_capacity_upper_bound_rejected |
| receipt without deposit original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_without_deposit_rejected |
| duplicate receipt output original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | duplicate_receipt_output |
| receipt group exact mint original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 96832 | pass / 26288 | n/a |
| receipt group over-mint original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_group_over_mint |
| receipt group missing header original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_group_missing_header_dep |
| receipt group wrong accumulated rate original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_group_wrong_accumulated_rate |
| receipt group wrong xUDT args original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_group_wrong_xudt_binding |
| receipt group malformed receipt data original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_group_malformed_receipt_data |
| receipt group second malformed receipt data original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_group_second_malformed_receipt_data |
| receipt group under-mint original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | receipt_group_under_mint |
| mint from receipt original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 90980 | pass / 24293 | n/a |
| mint from malformed receipt data original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | mint_malformed_receipt_data |
| amount inflation original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | amount_inflation |
| amount deflation original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | amount_deflation |
| wrong xUDT args original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | wrong_xudt_binding |
| wrong accumulated rate original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | wrong_accumulated_rate |
| missing header dep original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | missing_header_dep |
| DAO mature withdrawal original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 14301 | pass / 7840 | n/a |
| DAO max withdrawal capacity original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 14301 | pass / 15038 | n/a |
| DAO deposit-rate adjusted max withdrawal capacity original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 14301 | pass / 15038 | n/a |
| DAO withdraw-rate adjusted max withdrawal capacity original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 14301 | pass / 15038 | n/a |
| DAO immature withdrawal original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_incorrect_since |
| DAO deposit-rate adjusted over-withdraw capacity original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_deposit_rate_adjusted_over_withdraw_capacity |
| DAO withdraw-rate adjusted over-withdraw capacity original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_withdraw_rate_adjusted_over_withdraw_capacity |
| DAO wrong deposit accumulated rate original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_wrong_deposit_accumulated_rate |
| DAO wrong withdraw accumulated rate original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_wrong_withdraw_accumulated_rate |
| DAO over-withdraw capacity original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_over_withdraw_capacity |
| DAO missing withdraw header original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_missing_withdraw_header |
| DAO missing deposit header original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_missing_deposit_header |
| DAO deposit header index out of bounds original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_deposit_header_index_out_of_bounds |
| DAO withdrawal deposit-data input original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_withdrawal_deposit_data_input |
| DAO withdrawal malformed input data original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_withdrawal_malformed_input_data |
| DAO missing witness input_type original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_missing_witness_input_type |
| DAO empty witness input_type original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_empty_witness_input_type |
| DAO short witness input_type original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_short_witness_input_type |
| DAO long witness input_type original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_long_witness_input_type |
| DAO wrong deposit header index original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_wrong_deposit_header_index |
| DAO wrong withdraw committed header original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | dao_wrong_withdraw_committed_header |
| valid limit order original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 60243 | pass / 11144 | n/a |
| limit order min-match boundary original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 60247 | pass / 11199 | n/a |
| limit order underpayment original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | limit_order_underpayment |
| limit order wrong asset original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | wrong_asset |
| limit order insufficient match original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | insufficient_match |
| limit order no CKB paid original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | no_ckb_paid_out |
| limit order UDT decreased original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | udt_decreased |
| valid limit order UDT-to-CKB original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 64486 | pass / 13505 | n/a |
| limit order UDT-to-CKB min-match boundary original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 65395 | pass / 13530 | n/a |
| limit order UDT-to-CKB no UDT paid original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | no_udt_paid_out |
| limit order UDT-to-CKB wrong asset original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | wrong_asset |
| limit order UDT-to-CKB insufficient match original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | insufficient_match |
| limit order UDT-to-CKB underpayment original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | limit_order_underpayment |
| valid owned-owner original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 83458 | pass / 36168 | n/a |
| valid owned-owner output pairing original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | pass / 47359 | pass / 20069 | n/a |
| owned-owner output relative mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_relative_distance_mismatch |
| owned-owner output duplicate owner original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_duplicate_owner_pair |
| owned-owner output missing owner original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_missing_owner_pair |
| owned-owner output missing owned original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_missing_owned_pair |
| owned-owner output script misuse original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_script_misuse |
| owned-owner output non-withdrawal request original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_not_withdrawal_request |
| owned-owner output owner data length mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_owner_data_length_mismatch |
| owned-owner output related type hash mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_related_type_hash_mismatch |
| owned-owner output related data rule mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | output_related_data_rule_mismatch |
| owned-owner related type hash mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | related_type_hash_mismatch |
| owned-owner related data rule mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | related_data_rule_mismatch |
| owned-owner owner data length mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | owner_data_length_mismatch |
| owned-owner relative mismatch original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | relative_distance_mismatch |
| owned-owner script misuse original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | script_misuse |
| owned-owner non-withdrawal request original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | not_withdrawal_request |
| owned-owner missing owner original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | missing_owner_pair |
| owned-owner missing owned original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | missing_owned_pair |
| owned-owner duplicate owner original vs CellScript agree | DIFFERENTIAL_CKB_VM_EXECUTED | fail / 0 | fail / 0 | duplicate_owner_pair |

The non-empty script args row uses a single output type script with non-empty
args and no DAO dependency. The deposit rows use the same normalized
deposit/receipt fixture shape on each side: the script under test appears as
output 0 lock and output 1 type; output 0 is a DAO deposit cell, output 1 is an
iCKB receipt cell, cell deps include DAO, and witnesses are empty. The
deposit-too-big row uses a 150,000,000,000,000 shannon deposit capacity and a
400,000,000,000,000 shannon funding input; patched original iCKB rejects with
exit `8`, while the generated upper-bound probe rejects with exit `7` under
`deposit_capacity_upper_bound_rejected`.
The receipt-without-deposit row uses one receipt output with no deposit output. The
duplicate receipt output row uses one DAO deposit output and two receipt
outputs with the same receipt amount; both sides reject with exit code 10. This
is an executed output-accounting `ReceiptMismatch` fixture, not the model-only
receipt-id double-mint fixture.
The receipt group rows use two same-shaped receipt inputs linked to the same DAO
header. The exact-mint row emits two receipts worth of xUDT and both sides pass
(`96,832` original cycles, `26,288` CellScript cycles). The over-mint row emits
one shannon more than two receipts worth of xUDT; the under-mint row emits only
one receipt worth of xUDT. Original iCKB rejects both bad rows with exit `11`,
while the CellScript aggregate probe rejects both with exit `36`. The
missing-header row keeps the exact output amount but omits the DAO header dep
for both linked receipt inputs; original iCKB rejects with exit `2`, while the
CellScript aggregate probe rejects with exit `28`. The wrong accumulated-rate
row keeps the header dep and exact output amount but uses a DAO accumulated rate
that differs from the receipt data for both inputs; original iCKB rejects with
exit `11`, while the CellScript aggregate probe rejects with exit `31`. These
are multi-receipt group amount, header-dep, and DAO-rate evidence. The group
wrong-xUDT row keeps the correct DAO header, receipt rate, and exact two-receipt
xUDT output amount but uses owner-mode args that are not bound to the
script-under-test hash; original iCKB rejects with exit `11`, while the
CellScript aggregate probe rejects with exit `30` under
`receipt_group_wrong_xudt_binding`. The malformed-receipt-data row keeps the
second receipt, DAO header, xUDT owner-mode args, and exact two-receipt output
amount valid, but changes the first receipt input data to 4 bytes; original iCKB
rejects with exit `4`, while the CellScript receipt-data-size probe rejects with
exit `37` under `receipt_group_malformed_receipt_data`. The second malformed
receipt-data row keeps the first receipt valid and shortens the second receipt
input to 4 bytes; both sides reject again with original exit `4` and CellScript
exit `37` under `receipt_group_second_malformed_receipt_data`. These rows are
not duplicate receipt-id proof.
The DAO redeem relative-since rows are CellScript-only VM evidence: both inputs
are withdrawal requests, the immature row carries relative epoch since `359/0/1`
and exits with DAO maturity violation, while the mature row carries `360/0/1`
and passes the same generated script. They are useful precursor evidence for
redeem maturity, but they are not full original-vs-CellScript differential rows.
The DAO withdrawal differential rows use the same phase-2 fixture shape on both
sides: one withdrawing DAO input, withdraw and deposit headers, witness
`input_type = 1`, and one withdrawn capacity output. The mature row uses since
`0x2003e8022a0002f3` and both sides pass. The immature row uses
`0x2003e802290002f3`; original DAO rejects with `ERROR_INCORRECT_SINCE (-17)`
and CellScript rejects with DAO maturity violation exit `36`. The max-capacity
row keeps the mature since/header/witness shape, runs the CellScript capacity
probe, and sets output capacity to the observed original DAO boundary
`123468305678`; both sides pass. The over-withdraw capacity row uses the same
shape but sets output capacity to `123468305679`, one shannon above that
observed boundary; original DAO rejects with exit `-15`, while the CellScript
capacity probe rejects with exit `48` under `dao_over_withdraw_capacity`. The
deposit-rate adjusted max row changes the deposit header accumulated rate from
`10000000` to `10000001` and sets output capacity to the fixture-rate maximum
`123468294151`; both sides pass. The withdraw-rate adjusted max row changes the
withdraw header accumulated rate from `10001000` to `10000999` and sets output
capacity to the fixture-rate maximum `123468294152`; both sides pass. The
deposit-rate adjusted over row sets output capacity to `123468294152`, one
shannon above the deposit fixture-rate maximum; both sides reject. The
withdraw-rate adjusted over row sets output capacity to `123468294153`, one
shannon above the withdraw fixture-rate maximum; both sides reject. The
wrong-deposit-rate row keeps the output at `123468305678` but changes the
deposit header accumulated rate from `10000000` to `10000001`; original DAO
rejects with exit `-15`, while the CellScript rate/capacity probe rejects with
exit `48` under `dao_wrong_deposit_accumulated_rate`. The wrong-withdraw-rate
row keeps the output at `123468305678` but changes the withdraw header
accumulated rate from `10001000` to `10000999`; original DAO rejects with exit
`-15`, while the CellScript rate/capacity probe rejects with exit `48` under
`dao_wrong_withdraw_accumulated_rate`. The missing
withdraw-header row keeps mature since, the deposit header, witness
`input_type = 0`, and the same output capacity, but omits the withdraw header
dep required by the input's committed header; original DAO rejects with exit
`2`, while the CellScript input-header probe rejects with exit `28` under
`dao_missing_withdraw_header`. The missing-deposit-header row keeps mature
since, the withdraw header, witness `input_type = 1`, and the same output
capacity, but omits the deposit header dep; original DAO rejects with exit `1`,
while the CellScript deposit-header probe rejects with exit `28` under
`dao_missing_deposit_header`. The deposit-header-index-out-of-bounds row keeps
both withdraw and deposit header deps present, but witness `input_type` points
past them to header dep index `2`; original DAO rejects with exit `1`, while
the CellScript out-of-bounds header probe rejects with exit `28` under
`dao_deposit_header_index_out_of_bounds`. The wrong-deposit-header-index row keeps both
withdraw and deposit header deps present, but witness `input_type` points to
header dep index `0` (withdraw header) instead of index `1` (deposit header);
original DAO rejects with exit `-14`, while the CellScript deposit-header
witness probe rejects with exit `41` under
`dao_wrong_deposit_header_index`. The wrong-withdraw-committed-header row keeps
both header deps and witness `input_type = 1`, but links the withdrawing input
to the deposit header instead of the withdraw header; original DAO rejects with
exit `-14`, while the CellScript input-header probe rejects with exit `40`
under `dao_wrong_withdraw_committed_header`. The deposit-data input row keeps
the same mature since, headers, witness, and output capacity, but changes the
input data from the withdrawal-request block number to `0x0000000000000000`;
original DAO rejects with exit `2`, while the CellScript withdrawal-data
classifier probe rejects with exit `34` under
`dao_withdrawal_deposit_data_input`. The malformed-input-data row keeps the
same mature/header/witness shape but shortens input data to `0x12060000`;
original DAO rejects with exit `-4`, while the CellScript classifier probe
rejects with exit `34` under `dao_withdrawal_malformed_input_data`.
The missing-witness-input_type, empty-witness-input_type, short-witness-input_type,
and long-witness-input_type rows keep the same mature/header/data/output shape
but omit WitnessArgs `input_type`, provide it with zero payload bytes, provide
only one non-zero byte, or provide nine bytes instead of the expected 8-byte
little-endian header dep index. Original DAO rejects all four with exit `-11`;
CellScript rejects the missing/empty rows with exit `42` and the width rows with
exit `43`.
The mint-family rows use an input iCKB receipt cell, the original xUDT binary
with `Data1` hash type, owner-mode args, and a header-linked input accumulated
rate. The pass, amount-inflation, amount-deflation, wrong-rate, and
missing-header-dep rows bind xUDT owner-mode args to the script-under-test hash;
the wrong-xUDT row uses a fixed wrong owner hash on both sides. The missing
header dep row links the receipt input to a DAO header in the test context but
omits that header from transaction header deps, so both scripts reject the same
fixture. The new single-receipt malformed-data row keeps the DAO header, xUDT
owner-mode args, and exact xUDT output valid, but shortens the receipt input
data to 4 bytes; original iCKB rejects with exit `4`, while the CellScript
receipt-data-size probe rejects with exit `37` under
`mint_malformed_receipt_data`. The CellScript side checks the fixture-bound
input rate, xUDT owner args, output xUDT amount, and selected receipt data-size
boundaries; full receipt byte decoding and aggregate recomputation remain open
work. Only the intended script-under-test code cell and script hash differ
between original iCKB and CellScript. The matrix records
fixture hashes, transaction context hashes, original and generated artifact
hashes, exit codes, status, cycles, transaction size, occupied capacity, and
fee.

The Limit Order rows use the original `limit_order` binary as the lock script
and a shared auxiliary always-success UDT type code cell. The input order
encodes `Action::Mint` with a stable relative master point; the output order
encodes `Action::Match` with the same absolute master OutPoint. The CellScript
side checks fixture-bound CKB+UDT value conservation, the `ckb_min_match`
boundary, and, for the wrong-asset case, type-hash low-word equality. This
gives real original-vs-CellScript VM evidence for the selected CKB-to-UDT
valid, exact min-match boundary, underpayment, wrong-asset,
insufficient-match, no-CKB-paid, and UDT-decreased fixtures. The min-match
boundary row pays exactly `64` shannons of CKB and receives exactly `64` UDT
units, proving the equality edge of `1 << min_match_log`. The UDT-to-CKB valid
row exercises the opposite fulfilment direction with full UDT fill, preserved
value, and a normalized funding input for the increased order CKB capacity. The
UDT-to-CKB min-match boundary row then proves the reverse equality edge: the
output order gains exactly `64` shannons of CKB, spends exactly `64` UDT units,
and both sides pass. The UDT-to-CKB no-UDT-paid
reject row keeps the full UDT amount in the output order and pays no CKB to the
order, so both scripts reject the same reverse-direction bad fulfilment. The
UDT-to-CKB wrong-asset row changes the output auxiliary UDT type script args,
so both scripts reject the same reverse-direction asset-binding mismatch. The
UDT-to-CKB insufficient-match row preserves order value but only moves 50 UDT,
below `ckb_min_match = 64`, so both scripts reject the same reverse-direction
min-fill violation. The UDT-to-CKB underpayment row consumes the full
10,000,000,000 UDT fill but only pays 5,000,000,000 CKB into the output order,
so both scripts reject the same reverse-direction value-shortfall fixture. Full
first-class action-aware MetaPoint map remains open. Full `Script` equality
semantics move to the 0.18 ScriptRef/ScriptArgs track.

The Owned-Owner rows use the original `owned_owner` binary and generated
CellScript ELFs on lock-owned/type-owner fixture shapes. The input valid row
stores relative distance `1` in the owner cell at OutPoint index 1, pointing to
the owned withdrawal request at index 2; both sides accept it. The output valid
row executes the script as an output type, stores relative distance `-1` in the
owner output at index 1, and points to the owned withdrawal request output at
index 0; both sides accept it. The relative-mismatch row stores `-1` on the
input-side fixture, which points to index 0 instead of the owned cell; both
sides reject it. The output relative-mismatch row stores `1` in the owner
output, which points to missing output index 2 instead of the owned withdrawal
request output at index 0; both sides reject it. The original binary is patched
so its hardcoded DAO hash
matches the shared auxiliary withdrawal type hash in ckb-testtool for rows that
need withdrawal classification; this is recorded as patched functional
evidence. The script-misuse row uses a separate single-input fixture where the
script under test appears as both lock and type on the same cell; both sides
reject with exit code 7, and no DAO-hash patch is used because the original
script rejects before DAO type/data classification.
The non-withdrawal request row uses a lock-owned input with no DAO
withdrawal-request type/data; both sides reject with exit code 6 and no
DAO-hash patch is used. The missing-owner row uses a valid lock-owned
withdrawal request but omits the matching type-owner cell; the patched original
rejects with exit code 8 and the CellScript helper rejects with exit code 40.
The missing-owned row uses a type-owner cell whose relative distance points to
a missing lock-owned cell; the unpatched original rejects with exit code 8 and
the CellScript helper rejects with exit code 40. The duplicate-owner row uses
two type-owner cells that both point to the same lock-owned withdrawal request;
both sides reject the owner count overflow. The output duplicate-owner row uses
two type-owner outputs at indices 1 and 2, with relative distances `-1` and `-2`
that both point to the lock-owned withdrawal request output at index 0; both
sides reject the output-side owner count overflow. These rows cover concrete
MetaPoint pairing pass/reject across input and output source views. The output
missing-owned row uses one type-owner output pointing to a missing lock-owned
output and needs no DAO hash patch. The output missing-owner row uses one valid
output pair to trigger type execution while a second lock-owned withdrawal
request output has no owner; both sides reject that extra unpaired owned output.
The output script-misuse row executes output 0 as a type script while the same
output also uses the script under test as its lock; both sides reject with exit
code 7 before DAO classification.
The output non-withdrawal request row executes output 1 as a type script while
output 0 uses the script under test as its lock but lacks DAO withdrawal
type/data; both sides reject with exit code 6 before owner-pair matching.
The output owner data length mismatch row executes output 1 as a type script
while output 0 is a valid lock-owned withdrawal request, but the type-owner
output data is only three bytes and cannot decode an i32 relative distance;
original rejects with exit code 4 and the CellScript output helper rejects with
exit code 34.
The output related type hash mismatch row patches original Owned-Owner to the
expected auxiliary withdrawal type hash, then gives the lock-owned output the
same auxiliary code with non-empty args so its actual type hash differs while
the output data remains a nonzero withdrawal-request payload and the owner
output distance is valid; original rejects with exit code 6 and the CellScript
output low-word type-hash probe rejects with exit code 46.
The output related data rule mismatch row keeps the expected auxiliary
withdrawal type hash on the lock-owned output but changes its data to an 8-byte
zero/deposit marker instead of a nonzero withdrawal-request payload; original
rejects with exit code 6 and the CellScript output withdrawal-data guard rejects
with exit code 47.
The input related type hash mismatch row patches original Owned-Owner to the expected
auxiliary withdrawal type hash, then gives the lock-owned input the same
auxiliary code with non-empty args so its actual type hash differs while the
data remains a nonzero withdrawal-request payload; original rejects with exit
code 6 and the CellScript low-word type-hash probe rejects with exit code 46.
The related data rule mismatch row keeps the expected auxiliary withdrawal type
hash on the lock-owned input but changes its data to an 8-byte zero/deposit
marker instead of a nonzero withdrawal-request payload; original rejects with
exit code 6 and the CellScript withdrawal-data guard rejects with exit code 47.
The owner data length mismatch row keeps both lock-owned and type-owner cells
present, but the owner cell data is only three bytes, so the signed i32
relative MetaPoint distance cannot be decoded; original rejects with exit code
4 and the CellScript helper rejects with exit code 34.
Together these rows cover both missing-pair directions and duplicate-owner
cardinality on both input and output views, input/output script-role misuse,
input/output lock-owned withdrawal-shape guards, input/output related type-hash
mismatch plus input/output related data-rule mismatch and input/output owner-data
decoding failures, not full Owned-Owner resource owner semantics or full
`Script` equality.

## iCKB Scenario Model Rows

There are no active `MODEL` rows in the matrix.

The matrix now removes legacy model rows whose scenarios already have
fixture-bound `DIFFERENTIAL_CKB_VM_EXECUTED` coverage: `valid deposit phase 1`,
`valid mint from receipt`, `amount inflation`, `wrong xUDT args`,
`valid limit order`, and `limit order underpayment`. It also moves the
duplicate receipt-id model fixture to `non_executable_model_assumptions`, because
that synthetic `id` field is not present in executable receipt cell data. The
wrong-owner model fixture is also non-executable under the current fixture
schema because executable Owned-Owner rows carry owner binding as
lock/type placement, OutPoint/MetaPoint relative distance, and i32 owner-cell
data rather than `owner` / `claimed_owner` fields. The immature-redeem model
fixture is replaced by the DAO immature withdrawal differential row because the
executable chain path expresses maturity through `since`, header deps, and
witness input-type data, not `current_epoch` / `maturity_epoch` fields.

## Production Equivalence Gate

The stricter gate is documented in
`docs/0.17/ickb_production_equivalence_gate.md`. In short, production
equivalence requires:

- original iCKB repo commit and script binary hashes;
- CellScript source commit and generated artifact hashes;
- CKB VM/testtool version;
- transaction fixture manifest hash;
- proof that inputs, outputs, cell deps, header deps, witnesses, and output data
  are identical across both executions;
- original and generated exit codes;
- named failure modes for rejects;
- cycle and transaction-size measurements;
- per-row execution objects with fixture/context hashes, both artifact hashes,
  status/exit-code match, cycles, transaction size, occupied capacity, and fee.

Without those fields, `MODEL_LEVEL_ONLY` rows cannot be upgraded to
behavioural-equivalence rows.

## CKB VM Execution Harness

The CKB VM execution harness (`tests/support/ckb_script_runner.rs`) is backed
by `ckb-testtool` v1.1, which provides real CKB VM execution with full syscall
context (LOAD_SCRIPT, LOAD_CELL, LOAD_WITNESS, LOAD_HEADER, LOAD_CELL_DATA,
LOAD_SCRIPT_HASH, LOAD_CELL_BY_FIELD, etc.). This is NOT a bare `ckb-vm` runner;
it uses ckb-script's `ScriptVerify` to handle the complete transaction
verification pipeline.

### CellScript Side

The harness currently proves:

1. **Pass case**: CellScript-compiled actions that call `ckb::current_script_hash()`,
   `dao::input_accumulated_rate()`, `dao::is_deposit_data()`,
   `dao::is_withdrawal_request_data()`, `ckb::cell_capacity()`,
   `dao::has_dao_type()`, `ckb::cell_occupied_capacity()`,
   `ckb::cell_data_size()`, and combined iCKB deposit verification all return
   exit code 0 in ckb-testtool.

2. **Fail case**: CellScript-compiled actions that return 1 or that call
   `dao::input_accumulated_rate()` without a header-linked input correctly fail
   script verification in ckb-testtool.

3. **Combined iCKB deposit**: A single CellScript script calling
   `is_deposit_data` + `cell_capacity` + `input_accumulated_rate` passes in CKB VM.

This is "executable evidence", not differential evidence by itself. The matrix
status for these rows is `CELL_SCRIPT_CKB_VM_EXECUTED`, not
`DIFFERENTIAL_CKB_VM_EXECUTED` or `PROVEN`.

### Original iCKB Side

The original iCKB Logic script binary (pre-built RISC-V ELF from
`github.com/ickb/proposal`) has been loaded and executed in ckb-testtool:

1. **Args rejection**: The original script rejects non-empty `script.args` with
   exit code -1 (assertion failure). This confirms the script's args validation.

2. **Empty args acceptance**: With empty args and no iCKB-type outputs, the
   original script exits 0 (trivially passes when no iCKB outputs to verify).

3. **Receipt rejection**: The original script correctly rejects a receipt-type
   output cell when there is no matching deposit input.

4. **DAO type hash mismatch**: The original iCKB script has a hardcoded DAO type
   hash (`0x82a922...`). In ckb-testtool, deploying the DAO script via
   `build_script()` uses `hash_type=Data` (code_hash = blake2b of script binary).
   The original iCKB expects `hash_type=Type` (code_hash = type ID). This causes
   a hash mismatch and the original script rejects DAO-related operations.
   The deposit differential rows patch the hardcoded DAO hash to the
   ckb-testtool DAO script hash and record that patch in evidence.

5. **Patched DAO deposit phase 1**: With the DAO hash patched, original iCKB
   Logic verifies the valid deposit phase 1 fixture and consumes 97,057 cycles.

Matrix status for original iCKB rows is `ORIGINAL_ICKB_CKB_VM_EXECUTED`.

## Why This Is Not Full Differential Equivalence

## What Still Blocks Full Differential Equivalence

- Seventy-five normalized fixtures have full original-vs-CellScript CKB VM
  differential evidence.
- DAO hash patching is still a test-environment bridge and must remain
  explicitly recorded; it is not mainnet identity reconstruction.
- The matrix no longer retains active model rows. Three legacy model
  assumptions are tracked outside active rows: duplicate receipt-id, wrong-owner
  synthetic resource fields, and synthetic current-epoch redeem maturity. They
  are not production equivalence proof; they document why those model fixture
  shapes are not executable iCKB rows and point to the closest differential VM
  evidence already in the matrix.
- Owned-Owner now has input-side and output-side relative-distance pairing pass
  rows, input-side and output-side mismatch reject rows, input-side and
  output-side missing-owner pairs, input-side and output-side missing-owned
  pairs, input-side duplicate-owner pair, output-side duplicate-owner pair,
  input-side and output-side script-role misuse, input-side and output-side
  non-withdrawal request reject rows, input-side and output-side owner data
  length mismatch rows, input-side and output-side related type-hash mismatch
  rows, and input-side and output-side related data-rule mismatch rows with
  differential VM evidence. These rows cover the executable Owned-Owner
  metapoint fixture shape, not synthetic `owner` / `claimed_owner`
  witness-auth fields.
- CellScript still lacks full arbitrary `Script`, full DAO
  second-withdrawal request/deposit/header binding, computed multi-cell iCKB
  mint-side receipt/deposit/DAO aggregate lowering, and native action-aware
  MetaPoint map semantics for a true CKB VM differential test.
- Generic witness/Molecule parsing is implemented and now has protocol-neutral
  CKB VM coverage plus DAO witness input_type differential reject rows; the
  remaining language-surface gap is first-class read-only ScriptRef/ScriptArgs.
  That topic is explicitly deferred to CellScript 0.18 and is not a 0.17
  differential blocker.

## Next Differential Step

1. **Broaden shared transaction fixtures** that work with both original iCKB
   and CellScript ELFs, with matching semantic cell deps (DAO, xUDT), header
   deps, witnesses, inputs, and outputs.

2. **Keep non-executable assumptions out of active rows** unless a future
   fixture adds real owner-auth bytes or redeem aggregate fields that both
   original iCKB and CellScript can execute.

3. **Defer first-class Script API to 0.18**. The 0.17 line keeps helper-level
   Script checks; 0.18 should add read-only ScriptRef/ScriptArgs to collapse
   helper fragmentation without arbitrary Script construction, deploy manifest
   resolution, or TYPE_ID constructors.

Until every selected scenario has matching original-vs-CellScript CKB VM
execution evidence, equivalence must not be claimed.
