const REQUIRED_EQUIVALENCE_EVIDENCE: [&str; 17] = [
    "reviewed_ickb_contracts_commit",
    "ickb_contracts_audit_suite_commit",
    "ickb_contracts_audit_report",
    "original_ickb_repo_commit",
    "original_ickb_script_binary_sha256",
    "cellscript_source_commit",
    "generated_cellscript_artifact_sha256",
    "ckb_vm_or_testtool_version",
    "transaction_fixture_manifest_sha256",
    "identical_inputs_outputs_cell_deps_header_deps_witnesses",
    "original_and_cellscript_exit_codes",
    "named_failure_mode_for_rejects",
    "cycle_and_tx_size_measurements",
    "per_row_execution_objects",
    "pass_fail_status_matches",
    "transaction_context_hashes",
    "capacity_and_fee_measurements",
];

const REMAINING_MODEL_BLOCKERS: [(&str, &str); 0] = [];

const RETIRED_MODEL_ASSUMPTIONS: [(&str, &str, &str); 3] = [
    ("duplicate receipt", "duplicate_receipt", RECEIPT_GROUP_EXACT_MINT_DIFF_SCENARIO),
    ("wrong owner", "wrong_owner", OWNED_OWNER_VALID_DIFF_SCENARIO),
    ("immature redeem", "immature_redeem", DAO_IMMATURE_WITHDRAWAL_DIFF_SCENARIO),
];

const VM_HARNESS_WITNESS_ARGS_PROGRAM: &str = r#"
module vm_harness_witness_args;

action test_witness_args() -> u64 {

        let view = source::input(0);
        let size = witness::size(view);
        ckb::require_witness_size_at_least(view, 16);
        let raw = witness::bounded_raw(view, 16);
        if (size != 16) {
            return 1;
        }
        if (witness::u32_le(raw, 0) != 16) {
            return 2;
        }
        if (witness::u32_le(raw, 4) != 16) || (witness::u32_le(raw, 8) != 16) || (witness::u32_le(raw, 12) != 16) {
            return 3;
        }
        return 0;
}
"#;

const VM_HARNESS_WITNESS_ARGS_ACTION: &str = "test_witness_args";

const VM_HARNESS_WITNESS_SIZE_TOO_SMALL_PROGRAM: &str = r#"
module vm_harness_witness_size_too_small;

action test_witness_size_too_small() -> u64 {

        let view = source::input(0);
        ckb::require_witness_size_at_least(view, 17);
        return 0;
}
"#;

const VM_HARNESS_WITNESS_SIZE_TOO_SMALL_ACTION: &str = "test_witness_size_too_small";

const VM_HARNESS_WITNESS_SHORT_LOCK_PROGRAM: &str = r#"
module vm_harness_witness_short_lock;

action test_witness_short_lock_rejected() -> u64 {

        let view = source::input(0);
        let lock_field = witness::lock_exact32(view);
        if lock_field != Hash::zero().0 {
            return 1;
        }
        return 0;
}
"#;

const VM_HARNESS_WITNESS_SHORT_LOCK_ACTION: &str = "test_witness_short_lock_rejected";

const VM_HARNESS_WITNESS_TYPED_FIELDS_PROGRAM: &str = r#"
module vm_harness_witness_typed_fields;

action test_witness_typed_fields() -> u64 {

        let witness_args = witness::args(0);
        let lock_field = witness::lock_exact32(witness_args);
        let input_type = witness::input_type_exact32(witness_args);
        let output_type = witness::output_type_exact32(witness_args);
        if lock_field == Hash::zero().0 {
            return 1;
        }
        if input_type == Hash::zero().0 {
            return 2;
        }
        if output_type == Hash::zero().0 {
            return 3;
        }
        if lock_field == input_type {
            return 4;
        }
        if input_type == output_type {
            return 5;
        }
        if lock_field == output_type {
            return 6;
        }
        return 0;
}
"#;

const VM_HARNESS_WITNESS_TYPED_FIELDS_ACTION: &str = "test_witness_typed_fields";

const VM_HARNESS_WITNESS_TYPED_LOCK_LARGE_SIBLING_PROGRAM: &str = r#"
module vm_harness_witness_typed_lock_large_sibling;

action test_witness_typed_lock_large_sibling() -> u64 {

        let witness_args = witness::args(0);
        if witness::lock_exact32(witness_args) == Hash::zero().0 {
            return 1;
        }
        return 0;
}
"#;

const VM_HARNESS_WITNESS_TYPED_LOCK_LARGE_SIBLING_ACTION: &str = "test_witness_typed_lock_large_sibling";

const VM_HARNESS_WITNESS_TYPED_SHORT_LOCK_PROGRAM: &str = r#"
module vm_harness_witness_typed_short_lock;

action test_witness_typed_short_lock() -> u64 {

        let witness_args = witness::args(0);
        let lock_field = witness::lock_exact32(witness_args);
        if lock_field == Hash::zero().0 {
            return 1;
        }
        return 0;
}
"#;

const VM_HARNESS_WITNESS_TYPED_SHORT_LOCK_ACTION: &str = "test_witness_typed_short_lock";

const VM_HARNESS_WITNESS_MALFORMED_PROGRAM: &str = r#"
module vm_harness_witness_malformed;

action test_witness_malformed() -> u64 {

        let view = source::input(0);
        let lock_field = witness::lock_exact32(view);
        if lock_field == Hash::zero().0 {
            return 1;
        }
        return 0;
}
"#;

const VM_HARNESS_WITNESS_MALFORMED_ACTION: &str = "test_witness_malformed";

const DEPOSIT_PHASE1_DIFF_SCENARIO: &str = "differential: deposit phase 1 original vs CellScript agree";
const DEPOSIT_TOO_SMALL_DIFF_SCENARIO: &str = "differential: deposit too small original vs CellScript agree";
const DEPOSIT_TOO_BIG_DIFF_SCENARIO: &str = "differential: deposit too big original vs CellScript agree";
const DEPOSIT_RECEIPT_AMOUNT_MISMATCH_DIFF_SCENARIO: &str =
    "differential: deposit receipt amount mismatch original vs CellScript agree";
const DEPOSIT_RECEIPT_QUANTITY_ZERO_DIFF_SCENARIO: &str = "differential: deposit receipt quantity zero original vs CellScript agree";
const DEPOSIT_RECEIPT_QUANTITY_MISMATCH_DIFF_SCENARIO: &str =
    "differential: deposit receipt quantity mismatch original vs CellScript agree";
const DEPOSIT_RECEIPT_SHORT_DATA_DIFF_SCENARIO: &str = "differential: deposit receipt short data original vs CellScript agree";
const DEPOSIT_RECEIPT_LONG_DATA_DIFF_SCENARIO: &str = "differential: deposit receipt long data original vs CellScript agree";
const DEPOSIT_MISSING_DAO_TYPE_DIFF_SCENARIO: &str = "differential: deposit missing DAO type original vs CellScript agree";
const DEPOSIT_WRONG_DAO_TYPE_DIFF_SCENARIO: &str = "differential: deposit wrong DAO type original vs CellScript agree";
const DEPOSIT_WRONG_LOCK_DIFF_SCENARIO: &str = "differential: deposit wrong iCKB lock original vs CellScript agree";
const DEPOSIT_SHORT_DATA_DIFF_SCENARIO: &str = "differential: deposit short DAO data original vs CellScript agree";
const DEPOSIT_NONZERO_DATA_DIFF_SCENARIO: &str = "differential: deposit nonzero DAO data original vs CellScript agree";
const DEPOSIT_LONG_DATA_DIFF_SCENARIO: &str = "differential: deposit long DAO data original vs CellScript agree";
const DUPLICATE_RECEIPT_OUTPUT_DIFF_SCENARIO: &str = "differential: duplicate receipt output original vs CellScript agree";
const RECEIPT_GROUP_EXACT_MINT_DIFF_SCENARIO: &str = "differential: receipt group exact mint original vs CellScript agree";
const RECEIPT_GROUP_MISSING_HEADER_DIFF_SCENARIO: &str = "differential: receipt group missing header original vs CellScript agree";
const RECEIPT_GROUP_OVER_MINT_DIFF_SCENARIO: &str = "differential: receipt group over-mint original vs CellScript agree";
const RECEIPT_GROUP_UNDER_MINT_DIFF_SCENARIO: &str = "differential: receipt group under-mint original vs CellScript agree";
const RECEIPT_GROUP_WRONG_RATE_DIFF_SCENARIO: &str = "differential: receipt group wrong accumulated rate original vs CellScript agree";
const RECEIPT_GROUP_WRONG_XUDT_ARGS_DIFF_SCENARIO: &str = "differential: receipt group wrong xUDT args original vs CellScript agree";
const RECEIPT_GROUP_MALFORMED_RECEIPT_DATA_DIFF_SCENARIO: &str =
    "differential: receipt group malformed receipt data original vs CellScript agree";
const RECEIPT_GROUP_SECOND_MALFORMED_RECEIPT_DATA_DIFF_SCENARIO: &str =
    "differential: receipt group second malformed receipt data original vs CellScript agree";
const RECEIPT_GROUP_MISSING_SECOND_INPUT_DIFF_SCENARIO: &str =
    "differential: receipt group missing second input original vs CellScript agree";
const NON_EMPTY_ARGS_DIFF_SCENARIO: &str = "differential: non-empty script args original vs CellScript agree";
const MINT_FROM_RECEIPT_DIFF_SCENARIO: &str = "differential: mint from receipt original vs CellScript agree";
const MINT_FROM_RECEIPT_QUANTITY_ZERO_DIFF_SCENARIO: &str =
    "differential: mint from zero-quantity receipt original vs CellScript agree";
const MINT_FROM_RECEIPT_QUANTITY_TWO_DIFF_SCENARIO: &str = "differential: mint from quantity-two receipt original vs CellScript agree";
const MINT_FROM_RECEIPT_LONG_DATA_DIFF_SCENARIO: &str = "differential: mint from long receipt data original vs CellScript agree";
const MINT_FROM_RECEIPT_MALFORMED_RECEIPT_DATA_DIFF_SCENARIO: &str =
    "differential: mint from malformed receipt data original vs CellScript agree";
const RECEIPT_GROUP_ZERO_FIRST_QUANTITY_DIFF_SCENARIO: &str =
    "differential: receipt group zero first quantity original vs CellScript agree";
const RECEIPT_GROUP_QUANTITY_ZERO_DIFF_SCENARIO: &str =
    "differential: receipt group zero-quantity receipts original vs CellScript agree";
const RECEIPT_GROUP_QUANTITY_TWO_DIFF_SCENARIO: &str =
    "differential: receipt group quantity-two receipts original vs CellScript agree";
const RECEIPT_GROUP_MIXED_QUANTITIES_DIFF_SCENARIO: &str = "differential: receipt group mixed quantities original vs CellScript agree";
const RECEIPT_GROUP_LONG_RECEIPT_DATA_DIFF_SCENARIO: &str =
    "differential: receipt group long receipt data original vs CellScript agree";
const RECEIPT_GROUP_AMOUNT_HIGH_NONZERO_DIFF_SCENARIO: &str =
    "differential: receipt group xUDT high amount word nonzero original vs CellScript agree";
const AMOUNT_INFLATION_DIFF_SCENARIO: &str = "differential: amount inflation original vs CellScript agree";
const AMOUNT_HIGH_NONZERO_DIFF_SCENARIO: &str = "differential: xUDT high amount word nonzero original vs CellScript agree";
const AMOUNT_DEFLATION_DIFF_SCENARIO: &str = "differential: amount deflation original vs CellScript agree";
const WRONG_XUDT_ARGS_DIFF_SCENARIO: &str = "differential: wrong xUDT args original vs CellScript agree";
const WRONG_ACCUMULATED_RATE_DIFF_SCENARIO: &str = "differential: wrong accumulated rate original vs CellScript agree";
const MISSING_HEADER_DEP_DIFF_SCENARIO: &str = "differential: missing header dep original vs CellScript agree";
const DAO_MATURE_WITHDRAWAL_DIFF_SCENARIO: &str = "differential: DAO mature withdrawal original vs CellScript agree";
const DAO_IMMATURE_WITHDRAWAL_DIFF_SCENARIO: &str = "differential: DAO immature withdrawal original vs CellScript agree";
const DAO_MAX_WITHDRAWAL_CAPACITY_DIFF_SCENARIO: &str = "differential: DAO max withdrawal capacity original vs CellScript agree";
const DAO_TWO_INPUT_MAX_WITHDRAWAL_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input max withdrawal capacity original vs CellScript agree";
const DAO_TWO_INPUT_OVER_WITHDRAWAL_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input over-withdraw capacity original vs CellScript agree";
const DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input mixed deposit-rate max withdrawal capacity original vs CellScript agree";
const DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input mixed deposit-rate over-withdraw capacity original vs CellScript agree";
const DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input mixed withdraw-rate max withdrawal capacity original vs CellScript agree";
const DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input mixed withdraw-rate over-withdraw capacity original vs CellScript agree";
const DAO_TWO_INPUT_MIXED_BOTH_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input mixed deposit+withdraw-rate max withdrawal capacity original vs CellScript agree";
const DAO_TWO_INPUT_MIXED_BOTH_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO two-input mixed deposit+withdraw-rate over-withdraw capacity original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO two-input second missing witness input_type original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO two-input second empty witness input_type original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO two-input second short witness input_type original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO two-input second long witness input_type original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_WITHDRAW_HEADER_WITNESS_INDEX_DIFF_SCENARIO: &str =
    "differential: DAO two-input second withdraw-header witness index original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_OOB_WITNESS_INDEX_DIFF_SCENARIO: &str =
    "differential: DAO two-input second out-of-bounds witness index original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_DEPOSIT_DATA_INPUT_DIFF_SCENARIO: &str =
    "differential: DAO two-input second deposit-data input original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_MALFORMED_INPUT_DATA_DIFF_SCENARIO: &str =
    "differential: DAO two-input second malformed input data original vs CellScript agree";
const DAO_TWO_INPUT_SECOND_LONG_INPUT_DATA_DIFF_SCENARIO: &str =
    "differential: DAO two-input second long input data original vs CellScript agree";
const DAO_THREE_INPUT_MAX_WITHDRAWAL_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_OVER_WITHDRAWAL_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input mixed deposit-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input mixed deposit-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input mixed withdraw-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input mixed withdraw-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_MIXED_BOTH_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input mixed deposit+withdraw-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_MIXED_BOTH_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input mixed deposit+withdraw-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed deposit-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed deposit-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed withdraw-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed withdraw-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed deposit+withdraw-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed deposit+withdraw-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed deposit-rate plus third mixed withdraw-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed deposit-rate plus third mixed withdraw-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed withdraw-rate plus third mixed deposit-rate max withdrawal capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO three-input second mixed withdraw-rate plus third mixed deposit-rate over-withdraw capacity original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input second missing witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input second empty witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input second short witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input second long witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_WITHDRAW_HEADER_WITNESS_INDEX_DIFF_SCENARIO: &str =
    "differential: DAO three-input second withdraw-header witness index original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_OOB_WITNESS_INDEX_DIFF_SCENARIO: &str =
    "differential: DAO three-input second out-of-bounds witness index original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_DEPOSIT_DATA_INPUT_DIFF_SCENARIO: &str =
    "differential: DAO three-input second deposit-data input original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_MALFORMED_INPUT_DATA_DIFF_SCENARIO: &str =
    "differential: DAO three-input second malformed input data original vs CellScript agree";
const DAO_THREE_INPUT_SECOND_LONG_INPUT_DATA_DIFF_SCENARIO: &str =
    "differential: DAO three-input second long input data original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input third missing witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input third empty witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input third short witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: DAO three-input third long witness input_type original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_WITHDRAW_HEADER_WITNESS_INDEX_DIFF_SCENARIO: &str =
    "differential: DAO three-input third withdraw-header witness index original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_OOB_WITNESS_INDEX_DIFF_SCENARIO: &str =
    "differential: DAO three-input third out-of-bounds witness index original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_DEPOSIT_DATA_INPUT_DIFF_SCENARIO: &str =
    "differential: DAO three-input third deposit-data input original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_MALFORMED_INPUT_DATA_DIFF_SCENARIO: &str =
    "differential: DAO three-input third malformed input data original vs CellScript agree";
const DAO_THREE_INPUT_THIRD_LONG_INPUT_DATA_DIFF_SCENARIO: &str =
    "differential: DAO three-input third long input data original vs CellScript agree";
const DAO_DEPOSIT_RATE_ADJUSTED_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO deposit-rate adjusted max withdrawal capacity original vs CellScript agree";
const DAO_DEPOSIT_RATE_ADJUSTED_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO deposit-rate adjusted over-withdraw capacity original vs CellScript agree";
const DAO_WRONG_DEPOSIT_RATE_DIFF_SCENARIO: &str = "differential: DAO wrong deposit accumulated rate original vs CellScript agree";
const DAO_WITHDRAW_RATE_ADJUSTED_MAX_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO withdraw-rate adjusted max withdrawal capacity original vs CellScript agree";
const DAO_WITHDRAW_RATE_ADJUSTED_OVER_CAPACITY_DIFF_SCENARIO: &str =
    "differential: DAO withdraw-rate adjusted over-withdraw capacity original vs CellScript agree";
const DAO_WRONG_WITHDRAW_RATE_DIFF_SCENARIO: &str = "differential: DAO wrong withdraw accumulated rate original vs CellScript agree";
const DAO_OVER_WITHDRAW_CAPACITY_DIFF_SCENARIO: &str = "differential: DAO over-withdraw capacity original vs CellScript agree";
const DAO_MISSING_WITHDRAW_HEADER_DIFF_SCENARIO: &str = "differential: DAO missing withdraw header original vs CellScript agree";
const DAO_MISSING_DEPOSIT_HEADER_DIFF_SCENARIO: &str = "differential: DAO missing deposit header original vs CellScript agree";
const DAO_DEPOSIT_HEADER_INDEX_OOB_DIFF_SCENARIO: &str =
    "differential: DAO deposit header index out of bounds original vs CellScript agree";
const DAO_WITHDRAWAL_DEPOSIT_DATA_INPUT_DIFF_SCENARIO: &str =
    "differential: DAO withdrawal deposit-data input original vs CellScript agree";
const DAO_WITHDRAWAL_MALFORMED_INPUT_DATA_DIFF_SCENARIO: &str =
    "differential: DAO withdrawal malformed input data original vs CellScript agree";
const DAO_WITHDRAWAL_LONG_INPUT_DATA_DIFF_SCENARIO: &str = "differential: DAO withdrawal long input data original vs CellScript agree";
const DAO_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str = "differential: DAO missing witness input_type original vs CellScript agree";
const DAO_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str = "differential: DAO empty witness input_type original vs CellScript agree";
const DAO_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str = "differential: DAO short witness input_type original vs CellScript agree";
const DAO_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO: &str = "differential: DAO long witness input_type original vs CellScript agree";
const DAO_WRONG_DEPOSIT_HEADER_INDEX_DIFF_SCENARIO: &str = "differential: DAO wrong deposit header index original vs CellScript agree";
const DAO_WRONG_WITHDRAW_COMMITTED_HEADER_DIFF_SCENARIO: &str =
    "differential: DAO wrong withdraw committed header original vs CellScript agree";
const LIMIT_ORDER_VALID_DIFF_SCENARIO: &str = "differential: valid limit order original vs CellScript agree";
const LIMIT_ORDER_MIN_MATCH_BOUNDARY_DIFF_SCENARIO: &str = "differential: limit order min-match boundary original vs CellScript agree";
const LIMIT_ORDER_UNDERPAYMENT_DIFF_SCENARIO: &str = "differential: limit order underpayment original vs CellScript agree";
const LIMIT_ORDER_WRONG_ASSET_DIFF_SCENARIO: &str = "differential: limit order wrong asset original vs CellScript agree";
const LIMIT_ORDER_INSUFFICIENT_MATCH_DIFF_SCENARIO: &str = "differential: limit order insufficient match original vs CellScript agree";
const LIMIT_ORDER_NO_CKB_PAID_DIFF_SCENARIO: &str = "differential: limit order no CKB paid original vs CellScript agree";
const LIMIT_ORDER_UDT_DECREASED_DIFF_SCENARIO: &str = "differential: limit order UDT decreased original vs CellScript agree";
const LIMIT_ORDER_WRONG_MASTER_TX_HASH_DIFF_SCENARIO: &str =
    "differential: limit order wrong master tx hash original vs CellScript agree";
const LIMIT_ORDER_WRONG_MASTER_INDEX_DIFF_SCENARIO: &str = "differential: limit order wrong master index original vs CellScript agree";
const LIMIT_ORDER_OUTPUT_MINT_ACTION_DIFF_SCENARIO: &str = "differential: limit order output mint action original vs CellScript agree";
const LIMIT_ORDER_OUTPUT_INVALID_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order output invalid action original vs CellScript agree";
const LIMIT_ORDER_OUTPUT_SHORT_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order output short action original vs CellScript agree";
const LIMIT_ORDER_OUTPUT_SHORT_MASTER_DIFF_SCENARIO: &str =
    "differential: limit order output short master OutPoint original vs CellScript agree";
const LIMIT_ORDER_OUTPUT_LONG_DATA_DIFF_SCENARIO: &str =
    "differential: limit order output long trailing data original vs CellScript agree";
const LIMIT_ORDER_INPUT_INVALID_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order input invalid action original vs CellScript agree";
const LIMIT_ORDER_INPUT_SHORT_ACTION_DIFF_SCENARIO: &str = "differential: limit order input short action original vs CellScript agree";
const LIMIT_ORDER_INPUT_SHORT_MASTER_DIFF_SCENARIO: &str =
    "differential: limit order input short master OutPoint original vs CellScript agree";
const LIMIT_ORDER_INPUT_LONG_DATA_DIFF_SCENARIO: &str =
    "differential: limit order input long trailing data original vs CellScript agree";
const LIMIT_ORDER_INPUT_ABSOLUTE_MATCH_DIFF_SCENARIO: &str =
    "differential: limit order input absolute match original vs CellScript agree";
const LIMIT_ORDER_INPUT_WRONG_MASTER_TX_HASH_DIFF_SCENARIO: &str =
    "differential: limit order input wrong master tx hash original vs CellScript agree";
const LIMIT_ORDER_INPUT_WRONG_MASTER_INDEX_DIFF_SCENARIO: &str =
    "differential: limit order input wrong master index original vs CellScript agree";
const LIMIT_ORDER_MISSING_MATCHING_OUTPUT_DIFF_SCENARIO: &str =
    "differential: limit order missing matching output original vs CellScript agree";
const LIMIT_ORDER_DUPLICATE_MATCHING_OUTPUT_DIFF_SCENARIO: &str =
    "differential: limit order duplicate matching output original vs CellScript agree";
const LIMIT_ORDER_MISSING_INPUT_TYPE_DIFF_SCENARIO: &str = "differential: limit order missing input type original vs CellScript agree";
const LIMIT_ORDER_MISSING_OUTPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: limit order missing output type original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_VALID_DIFF_SCENARIO: &str = "differential: valid limit order UDT-to-CKB original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_MIN_MATCH_BOUNDARY_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB min-match boundary original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_NO_UDT_PAID_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB no UDT paid original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_WRONG_ASSET_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB wrong asset original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INSUFFICIENT_MATCH_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB insufficient match original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_UNDERPAYMENT_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB underpayment original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_WRONG_MASTER_TX_HASH_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB wrong master tx hash original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_WRONG_MASTER_INDEX_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB wrong master index original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_OUTPUT_MINT_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB output mint action original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_OUTPUT_INVALID_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB output invalid action original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_OUTPUT_SHORT_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB output short action original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_OUTPUT_SHORT_MASTER_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB output short master OutPoint original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_OUTPUT_LONG_DATA_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB output long trailing data original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INPUT_INVALID_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB input invalid action original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INPUT_SHORT_ACTION_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB input short action original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INPUT_SHORT_MASTER_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB input short master OutPoint original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INPUT_LONG_DATA_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB input long trailing data original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INPUT_ABSOLUTE_MATCH_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB input absolute match original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INPUT_WRONG_MASTER_TX_HASH_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB input wrong master tx hash original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_INPUT_WRONG_MASTER_INDEX_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB input wrong master index original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_MISSING_MATCHING_OUTPUT_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB missing matching output original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_MATCHING_OUTPUT_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB duplicate matching output original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_MISSING_INPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB missing input type original vs CellScript agree";
const LIMIT_ORDER_UDT_TO_CKB_MISSING_OUTPUT_TYPE_DIFF_SCENARIO: &str =
    "differential: limit order UDT-to-CKB missing output type original vs CellScript agree";
const OWNED_OWNER_VALID_DIFF_SCENARIO: &str = "differential: valid owned-owner original vs CellScript agree";
const OWNED_OWNER_OUTPUT_VALID_DIFF_SCENARIO: &str = "differential: valid owned-owner output pairing original vs CellScript agree";
const OWNED_OWNER_OUTPUT_RELATIVE_MISMATCH_DIFF_SCENARIO: &str =
    "differential: owned-owner output relative mismatch original vs CellScript agree";
const OWNED_OWNER_OUTPUT_DUPLICATE_OWNER_DIFF_SCENARIO: &str =
    "differential: owned-owner output duplicate owner original vs CellScript agree";
const OWNED_OWNER_OUTPUT_MISSING_OWNER_DIFF_SCENARIO: &str =
    "differential: owned-owner output missing owner original vs CellScript agree";
const OWNED_OWNER_OUTPUT_MISSING_OWNED_DIFF_SCENARIO: &str =
    "differential: owned-owner output missing owned original vs CellScript agree";
const OWNED_OWNER_OUTPUT_SCRIPT_MISUSE_DIFF_SCENARIO: &str =
    "differential: owned-owner output script misuse original vs CellScript agree";
const OWNED_OWNER_OUTPUT_NOT_WITHDRAWAL_DIFF_SCENARIO: &str =
    "differential: owned-owner output non-withdrawal request original vs CellScript agree";
const OWNED_OWNER_OUTPUT_OWNER_DATA_LENGTH_MISMATCH_DIFF_SCENARIO: &str =
    "differential: owned-owner output owner data length mismatch original vs CellScript agree";
const OWNED_OWNER_OUTPUT_RELATED_TYPE_HASH_MISMATCH_DIFF_SCENARIO: &str =
    "differential: owned-owner output related type hash mismatch original vs CellScript agree";
const OWNED_OWNER_OUTPUT_RELATED_DATA_RULE_MISMATCH_DIFF_SCENARIO: &str =
    "differential: owned-owner output related data rule mismatch original vs CellScript agree";
const OWNED_OWNER_RELATED_TYPE_HASH_MISMATCH_DIFF_SCENARIO: &str =
    "differential: owned-owner related type hash mismatch original vs CellScript agree";
const OWNED_OWNER_RELATED_DATA_RULE_MISMATCH_DIFF_SCENARIO: &str =
    "differential: owned-owner related data rule mismatch original vs CellScript agree";
const OWNED_OWNER_OWNER_DATA_LENGTH_MISMATCH_DIFF_SCENARIO: &str =
    "differential: owned-owner owner data length mismatch original vs CellScript agree";
const OWNED_OWNER_RELATIVE_MISMATCH_DIFF_SCENARIO: &str = "differential: owned-owner relative mismatch original vs CellScript agree";
const OWNED_OWNER_SCRIPT_MISUSE_DIFF_SCENARIO: &str = "differential: owned-owner script misuse original vs CellScript agree";
const OWNED_OWNER_NOT_WITHDRAWAL_DIFF_SCENARIO: &str = "differential: owned-owner non-withdrawal request original vs CellScript agree";
const OWNED_OWNER_MISSING_OWNER_DIFF_SCENARIO: &str = "differential: owned-owner missing owner original vs CellScript agree";
const OWNED_OWNER_MISSING_OWNED_DIFF_SCENARIO: &str = "differential: owned-owner missing owned original vs CellScript agree";
const OWNED_OWNER_DUPLICATE_OWNER_DIFF_SCENARIO: &str = "differential: owned-owner duplicate owner original vs CellScript agree";
const CKB_TESTTOOL_VERSION: &str = "ckb-testtool 1.1";
const DEPOSIT_PHASE1_INPUT_CAPACITY: u64 = 1_000_000_000_000;
const VALID_DEPOSIT_PHASE1_CAPACITY: u64 = 400_000_000_000;
const TINY_DEPOSIT_PHASE1_CAPACITY: u64 = 10_000_000_000;
const HUGE_DEPOSIT_PHASE1_CAPACITY: u64 = 150_000_000_000_000;
const HUGE_DEPOSIT_PHASE1_INPUT_CAPACITY: u64 = 400_000_000_000_000;
const DUPLICATE_RECEIPT_OUTPUT_CAPACITY: u64 = 200_000_000_000;
const ICKB_MIN_DEPOSIT_CAPACITY: u64 = 100_000_000_000;
const DEPOSIT_PHASE1_MAX_CYCLES: u64 = 50_000_000;
const NON_EMPTY_ARGS_INPUT_CAPACITY: u64 = 100_000_000_000;
const NON_EMPTY_ARGS_OUTPUT_CAPACITY: u64 = 100_000_000_000;
const NON_EMPTY_ARGS_MAX_CYCLES: u64 = 10_000_000;
const MINT_RECEIPT_INPUT_CAPACITY: u64 = 100_000_000_000;
const MINT_XUDT_OUTPUT_CAPACITY: u64 = 100_000_000_000;
const MINT_RECEIPT_QUANTITY: u32 = 1;
const MINT_RECEIPT_DEPOSIT_AMOUNT: u64 = 10_000_000_000_000;
const MINT_RECEIPT_ACCUMULATED_RATE: u64 = 10_000_000_000_000_000;
const WRONG_MINT_RECEIPT_ACCUMULATED_RATE: u64 = 20_000_000_000_000_000;
const MINT_RECEIPT_OUTPUT_AMOUNT: u128 = 10_000_000_000_000;
const MINT_RECEIPT_QUANTITY_ZERO_OUTPUT_AMOUNT: u128 = 0;
const MINT_RECEIPT_QUANTITY_TWO_OUTPUT_AMOUNT: u128 = MINT_RECEIPT_OUTPUT_AMOUNT * 2;
const MINT_RECEIPT_HIGH_WORD_OUTPUT_AMOUNT: u128 = MINT_RECEIPT_OUTPUT_AMOUNT + (1u128 << 64);
const MINT_RECEIPT_MIXED_SECOND_QUANTITY: u32 = 2;
const MINT_RECEIPT_MIXED_SECOND_DEPOSIT_AMOUNT: u64 = 9_000_000_000_000;
const MINT_RECEIPT_MIXED_GROUP_OUTPUT_AMOUNT: u128 =
    MINT_RECEIPT_OUTPUT_AMOUNT + (MINT_RECEIPT_MIXED_SECOND_QUANTITY as u128 * MINT_RECEIPT_MIXED_SECOND_DEPOSIT_AMOUNT as u128);
const XUDT_OWNER_MODE_TYPE_FLAGS: u32 = 2_147_483_648;
const WRONG_XUDT_OWNER_HASH: [u8; 32] = [0x42; 32];
const MINT_FROM_RECEIPT_MAX_CYCLES: u64 = 50_000_000;
const LIMIT_ORDER_INPUT_CAPACITY: u64 = 100_000_000_000;
const LIMIT_ORDER_OUTPUT_CAPACITY: u64 = 90_000_000_000;
const LIMIT_ORDER_DUPLICATE_FIRST_OUTPUT_CAPACITY: u64 = 50_000_000_000;
const LIMIT_ORDER_DUPLICATE_SECOND_OUTPUT_CAPACITY: u64 = 40_000_000_000;
const LIMIT_ORDER_MIN_MATCH_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY - (1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG);
const LIMIT_ORDER_INSUFFICIENT_MATCH_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY - 50;
const LIMIT_ORDER_NO_CKB_PAID_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY;
const LIMIT_ORDER_UDT_DECREASED_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY - (1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG);
const LIMIT_ORDER_INPUT_UDT_AMOUNT: u128 = 0;
const LIMIT_ORDER_UDT_DECREASED_INPUT_UDT_AMOUNT: u128 = 10;
const LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT: u128 = 10_000_000_000;
const LIMIT_ORDER_DUPLICATE_FIRST_OUTPUT_UDT_AMOUNT: u128 = 50_000_000_000;
const LIMIT_ORDER_DUPLICATE_SECOND_OUTPUT_UDT_AMOUNT: u128 = 60_000_000_000;
const LIMIT_ORDER_MIN_MATCH_OUTPUT_UDT_AMOUNT: u128 = 1u128 << LIMIT_ORDER_CKB_MIN_MATCH_LOG;
const LIMIT_ORDER_UNDERPAYMENT_OUTPUT_UDT_AMOUNT: u128 = 5_000_000_000;
const LIMIT_ORDER_WRONG_ASSET_OUTPUT_UDT_AMOUNT: u128 = 10_000_000_000;
const LIMIT_ORDER_INSUFFICIENT_MATCH_OUTPUT_UDT_AMOUNT: u128 = 50;
const LIMIT_ORDER_NO_CKB_PAID_OUTPUT_UDT_AMOUNT: u128 = 0;
const LIMIT_ORDER_UDT_DECREASED_OUTPUT_UDT_AMOUNT: u128 = 0;
const LIMIT_ORDER_CKB_TO_UDT_MUL: u64 = 1;
const LIMIT_ORDER_UDT_TO_CKB_MUL: u64 = 1;
const LIMIT_ORDER_CKB_MIN_MATCH_LOG: u8 = 6;
const LIMIT_ORDER_MAX_CYCLES: u64 = 50_000_000;
const LIMIT_ORDER_MASTER_TX_HASH: [u8; 32] = [0x77; 32];
const LIMIT_ORDER_WRONG_MASTER_TX_HASH: [u8; 32] = [0x78; 32];
const LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY: u64 = 10_000_000_000;
const LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_FUNDING_CAPACITY: u64 = 110_000_000_000;
const LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT: u128 = 10_000_000_000;
const LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY + 10_000_000_000;
const LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT: u128 = 0;
const LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_FIRST_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY;
const LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_SECOND_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_OUTPUT_CAPACITY;
const LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_OUTPUT_UDT_AMOUNT: u128 = 0;
const LIMIT_ORDER_UDT_TO_CKB_MIN_MATCH_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY + (1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG);
const LIMIT_ORDER_UDT_TO_CKB_MIN_MATCH_OUTPUT_UDT_AMOUNT: u128 =
    LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT - (1u128 << LIMIT_ORDER_CKB_MIN_MATCH_LOG);
const LIMIT_ORDER_UDT_TO_CKB_NO_UDT_PAID_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY;
const LIMIT_ORDER_UDT_TO_CKB_NO_UDT_PAID_OUTPUT_UDT_AMOUNT: u128 = LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT;
const LIMIT_ORDER_UDT_TO_CKB_INSUFFICIENT_MATCH_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY + 50;
const LIMIT_ORDER_UDT_TO_CKB_INSUFFICIENT_MATCH_OUTPUT_UDT_AMOUNT: u128 = LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT - 50;
const LIMIT_ORDER_UDT_TO_CKB_UNDERPAYMENT_OUTPUT_CAPACITY: u64 = LIMIT_ORDER_INPUT_CAPACITY + 5_000_000_000;
const LIMIT_ORDER_UDT_TO_CKB_UNDERPAYMENT_OUTPUT_UDT_AMOUNT: u128 = 0;
const OWNED_OWNER_INPUT_CAPACITY: u64 = 100_000_000_000;
const OWNED_OWNER_OUTPUT_CAPACITY: u64 = 200_000_000_000;
const OWNED_OWNER_MAX_CYCLES: u64 = 50_000_000;
const OWNED_OWNER_TX_HASH: [u8; 32] = [0x88; 32];
const OWNED_OWNER_OWNER_OUT_POINT_INDEX: u32 = 1;
const OWNED_OWNER_OWNED_OUT_POINT_INDEX: u32 = 2;
const OWNED_OWNER_VALID_DISTANCE: i32 = 1;
const OWNED_OWNER_MISMATCH_DISTANCE: i32 = -1;
const OWNED_OWNER_SCRIPT_MISUSE_OUT_POINT_INDEX: u32 = 3;
const OWNED_OWNER_NOT_WITHDRAWAL_OUT_POINT_INDEX: u32 = 4;
const OWNED_OWNER_MISSING_OWNER_OUT_POINT_INDEX: u32 = 5;
const OWNED_OWNER_MISSING_OWNED_OUT_POINT_INDEX: u32 = 6;
const OWNED_OWNER_DUPLICATE_OWNER_OUT_POINT_INDEX: u32 = 0;
const OWNED_OWNER_DUPLICATE_OWNER_DISTANCE: i32 = 2;
const OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX: u32 = 7;
const OWNED_OWNER_RELATED_TYPE_HASH_MISMATCH_OUT_POINT_INDEX: u32 = 8;
const OWNED_OWNER_RELATED_DATA_RULE_MISMATCH_OUT_POINT_INDEX: u32 = 9;
const OWNED_OWNER_OWNER_DATA_LENGTH_MISMATCH_OUT_POINT_INDEX: u32 = 10;
const OWNED_OWNER_OUTPUT_OWNER_DISTANCE: i32 = -1;
const OWNED_OWNER_OUTPUT_MISMATCH_DISTANCE: i32 = 1;
const OWNED_OWNER_OUTPUT_DUPLICATE_OWNER_DISTANCE: i32 = -2;
const DEPOSIT_PHASE1_CELLSCRIPT_ACTION: &str = "test_deposit_phase1";
const DEPOSIT_PHASE1_CELLSCRIPT_PROGRAM: &str = r#"
module differential_deposit_phase1;

action test_deposit_phase1() -> u64 {

        let deposit = source::output(0);
        let receipt = source::group_output(0);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(deposit, current_script_hash);
        let expected_dao_type = __EXPECTED_DAO_TYPE_SCRIPT__;
        script::require_cell_type_matches(deposit, expected_dao_type);
        let is_deposit = dao::is_deposit_data(deposit);
        if !is_deposit {
            return 11;
        }
        let capacity = ckb::cell_capacity(deposit);
        if capacity < 100000000000 {
            return 6;
        }
        let receipt_size = ckb::cell_data_size(receipt);
        if receipt_size < 12 {
            return 9;
        }
        let receipt_quantity = ckb::cell_data_u32_le(receipt, 0);
        if receipt_quantity != 1 {
            return 12;
        }
        let expected_unoccupied_capacity = capacity - 8200000000;
        let receipt_deposit_amount = ckb::cell_data_u64_le(receipt, 4);
        if receipt_deposit_amount != expected_unoccupied_capacity {
            return 13;
        }
        return 0;
}
"#;
const DEPOSIT_PHASE1_UPPER_BOUND_CELLSCRIPT_ACTION: &str = "test_deposit_phase1_upper_bound";
const DEPOSIT_PHASE1_UPPER_BOUND_CELLSCRIPT_PROGRAM: &str = r#"
module differential_deposit_phase1_upper_bound;

action test_deposit_phase1_upper_bound() -> u64 {

        let deposit = source::output(0);
        let receipt = source::group_output(0);
        let current_script_hash: Hash = ckb::current_script_hash();
        ckb::require_cell_lock_hash(deposit, current_script_hash);
        let expected_dao_type = __EXPECTED_DAO_TYPE_SCRIPT__;
        script::require_cell_type_matches(deposit, expected_dao_type);
        let is_deposit = dao::is_deposit_data(deposit);
        if !is_deposit {
            return 11;
        }
        let capacity = ckb::cell_capacity(deposit);
        if capacity < 100000000000 {
            return 6;
        }
        if capacity > 100000000000000 {
            return 7;
        }
        let receipt_size = ckb::cell_data_size(receipt);
        if receipt_size < 12 {
            return 9;
        }
        let receipt_quantity = ckb::cell_data_u32_le(receipt, 0);
        if receipt_quantity != 1 {
            return 12;
        }
        let expected_unoccupied_capacity = capacity - 8200000000;
        let receipt_deposit_amount = ckb::cell_data_u64_le(receipt, 4);
        if receipt_deposit_amount != expected_unoccupied_capacity {
            return 13;
        }
        return 0;
}
"#;
const NON_EMPTY_ARGS_CELLSCRIPT_ACTION: &str = "test_non_empty_args";
const NON_EMPTY_ARGS_CELLSCRIPT_PROGRAM: &str = r#"
module diff_non_empty_args;

action test_non_empty_args() -> u64 {

        ckb::require_cell_type_args_empty(source::output(0));
        return 0;
}
"#;
const MINT_FROM_RECEIPT_CELLSCRIPT_ACTION: &str = "test_mint_from_receipt";
const MINT_FROM_RECEIPT_CELLSCRIPT_PROGRAM: &str = r#"
module differential_mint_from_receipt;

action test_mint_from_receipt() -> u64 {

        ckb::require_current_script_args_empty();
        let receipt_input = source::group_input(0);
        let input_rate = dao::input_accumulated_rate(receipt_input);
        if input_rate != 10000000000000000 {
            return 31;
        }
        let receipt_quantity = ckb::cell_data_u32_le(receipt_input, 0);
        let receipt_deposit_amount = ckb::cell_data_u64_le(receipt_input, 4);
        let expected_minted = receipt_quantity * receipt_deposit_amount;
        let xudt_output = source::output(0);
        xudt::require_owner_mode_type_args_current_script(xudt_output, 2147483648);
        let minted_low = xudt::amount_low(xudt_output);
        let minted_high = xudt::amount_high(xudt_output);
        if minted_low != expected_minted {
            return 32;
        }
        if minted_high != 0 {
            return 33;
        }
        return 0;
}
"#;
const MINT_FROM_RECEIPT_RECEIPT_DATA_SIZE_CELLSCRIPT_ACTION: &str = "test_mint_from_receipt_receipt_data_size";
const MINT_FROM_RECEIPT_RECEIPT_DATA_SIZE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_mint_from_receipt_receipt_data_size;

action test_mint_from_receipt_receipt_data_size() -> u64 {

        ckb::require_current_script_args_empty();
        let receipt_input = source::group_input(0);
        let receipt_size = ckb::cell_data_size(receipt_input);
        if receipt_size < 12 {
            return 37;
        }
        let input_rate = dao::input_accumulated_rate(receipt_input);
        if input_rate != 10000000000000000 {
            return 31;
        }
        let receipt_quantity = ckb::cell_data_u32_le(receipt_input, 0);
        let receipt_deposit_amount = ckb::cell_data_u64_le(receipt_input, 4);
        let expected_minted = receipt_quantity * receipt_deposit_amount;
        let xudt_output = source::output(0);
        xudt::require_owner_mode_type_args_current_script(xudt_output, 2147483648);
        let minted_low = xudt::amount_low(xudt_output);
        let minted_high = xudt::amount_high(xudt_output);
        if minted_low != expected_minted {
            return 32;
        }
        if minted_high != 0 {
            return 33;
        }
        return 0;
}
"#;
const RECEIPT_GROUP_UNDER_MINT_CELLSCRIPT_ACTION: &str = "test_receipt_group_under_mint";
const RECEIPT_GROUP_UNDER_MINT_CELLSCRIPT_PROGRAM: &str = r#"
module differential_receipt_group_under_mint;

action test_receipt_group_under_mint() -> u64 {

        ckb::require_current_script_args_empty();
        let first_receipt_input = source::group_input(0);
        let first_input_rate = dao::input_accumulated_rate(first_receipt_input);
        if first_input_rate != 10000000000000000 {
            return 31;
        }
        let first_receipt_quantity = ckb::cell_data_u32_le(first_receipt_input, 0);
        let first_receipt_deposit_amount = ckb::cell_data_u64_le(first_receipt_input, 4);
        let second_receipt_input = source::group_input(1);
        let second_input_rate = dao::input_accumulated_rate(second_receipt_input);
        if second_input_rate != 10000000000000000 {
            return 31;
        }
        let second_receipt_quantity = ckb::cell_data_u32_le(second_receipt_input, 0);
        let second_receipt_deposit_amount = ckb::cell_data_u64_le(second_receipt_input, 4);
        let first_expected_minted = first_receipt_quantity * first_receipt_deposit_amount;
        let second_expected_minted = second_receipt_quantity * second_receipt_deposit_amount;
        let expected_minted = first_expected_minted + second_expected_minted;
        let xudt_output = source::output(0);
        xudt::require_owner_mode_type_args_current_script(xudt_output, 2147483648);
        let minted_low = xudt::amount_low(xudt_output);
        let minted_high = xudt::amount_high(xudt_output);
        if minted_low != expected_minted {
            return 36;
        }
        if minted_high != 0 {
            return 33;
        }
        return 0;
}
"#;
const RECEIPT_GROUP_RECEIPT_DATA_SIZE_CELLSCRIPT_ACTION: &str = "test_receipt_group_receipt_data_size";
const RECEIPT_GROUP_RECEIPT_DATA_SIZE_CELLSCRIPT_PROGRAM: &str = r#"
module differential_receipt_group_receipt_data_size;

action test_receipt_group_receipt_data_size() -> u64 {

        ckb::require_current_script_args_empty();
        let first_receipt_input = source::group_input(0);
        let first_receipt_size = ckb::cell_data_size(first_receipt_input);
        if first_receipt_size < 12 {
            return 37;
        }
        let first_input_rate = dao::input_accumulated_rate(first_receipt_input);
        if first_input_rate != 10000000000000000 {
            return 31;
        }
        let first_receipt_quantity = ckb::cell_data_u32_le(first_receipt_input, 0);
        let first_receipt_deposit_amount = ckb::cell_data_u64_le(first_receipt_input, 4);
        let second_receipt_input = source::group_input(1);
        let second_receipt_size = ckb::cell_data_size(second_receipt_input);
        if second_receipt_size < 12 {
            return 38;
        }
        let second_input_rate = dao::input_accumulated_rate(second_receipt_input);
        if second_input_rate != 10000000000000000 {
            return 31;
        }
        let second_receipt_quantity = ckb::cell_data_u32_le(second_receipt_input, 0);
        let second_receipt_deposit_amount = ckb::cell_data_u64_le(second_receipt_input, 4);
        let first_expected_minted = first_receipt_quantity * first_receipt_deposit_amount;
        let second_expected_minted = second_receipt_quantity * second_receipt_deposit_amount;
        let expected_minted = first_expected_minted + second_expected_minted;
        let xudt_output = source::output(0);
        xudt::require_owner_mode_type_args_current_script(xudt_output, 2147483648);
        let minted_low = xudt::amount_low(xudt_output);
        let minted_high = xudt::amount_high(xudt_output);
        if minted_low != expected_minted {
            return 36;
        }
        if minted_high != 0 {
            return 33;
        }
        return 0;
}
"#;
const RECEIPT_WITHOUT_DEPOSIT_DIFF_SCENARIO: &str = "differential: receipt without deposit original vs CellScript agree";
const RECEIPT_WITHOUT_DEPOSIT_INPUT_CAPACITY: u64 = 200_000_000_000;
const RECEIPT_WITHOUT_DEPOSIT_OUTPUT_CAPACITY: u64 = 100_000_000_000;
const RECEIPT_WITHOUT_DEPOSIT_MAX_CYCLES: u64 = 10_000_000;
const RECEIPT_WITHOUT_DEPOSIT_CELLSCRIPT_ACTION: &str = "test_receipt_needs_deposit";
const RECEIPT_WITHOUT_DEPOSIT_CELLSCRIPT_PROGRAM: &str = r#"
module differential_receipt_without_deposit;

action test_receipt_needs_deposit() -> u64 {

        let receipt = source::group_output(0);
        let receipt_size = ckb::cell_data_size(receipt);
        if receipt_size == 0 {
            return 9;
        }
        return 10;
}
"#;
const DUPLICATE_RECEIPT_OUTPUT_CELLSCRIPT_ACTION: &str = "test_duplicate_receipt_output";
const DUPLICATE_RECEIPT_OUTPUT_CELLSCRIPT_PROGRAM: &str = r#"
module differential_duplicate_receipt_output;

action test_duplicate_receipt_output() -> u64 {

        ckb::require_current_script_args_empty();
        let deposit = source::output(0);
        let is_deposit = dao::is_deposit_data(deposit);
        if !is_deposit {
            return 11;
        }
        let first_receipt = source::group_output(0);
        let first_receipt_size = ckb::cell_data_size(first_receipt);
        if first_receipt_size == 0 {
            return 9;
        }
        let second_receipt = source::group_output(1);
        let second_receipt_size = ckb::cell_data_size(second_receipt);
        if second_receipt_size == 0 {
            return 9;
        }
        return 10;
}
"#;
const IMMATURE_REDEEM_CELLSCRIPT_ACTION: &str = "test_immature_redeem_since";
const IMMATURE_REDEEM_REQUIRED_EPOCH: u64 = 360;
const IMMATURE_REDEEM_INPUT_EPOCH: u64 = 359;
const MATURE_REDEEM_INPUT_EPOCH: u64 = 360;
const IMMATURE_REDEEM_CAPACITY: u64 = 102_000_000_000;
const IMMATURE_REDEEM_MAX_CYCLES: u64 = 10_000_000;
const ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY: u64 = 123_456_780_000;
const ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK: u64 = 1554;
const ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE: u64 = 10_000_000;
const ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE: u64 = ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER: u64 = 35;
const ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX: u64 = 554;
const ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH: u64 = 1000;
const ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK: u64 = 2_000_610;
const ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE: u64 = 10_001_000;
const ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE: u64 = 10_000_999;
const ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER: u64 = 575;
const ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX: u64 = 610;
const ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH: u64 = 1100;
const ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE: u64 = 0x2003e8022a0002f3;
const ORIGINAL_DAO_WITHDRAW_PHASE2_IMMATURE_SINCE: u64 = 0x2003e802290002f3;
const ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY: u64 = 123_468_105_678;
const ORIGINAL_DAO_WITHDRAW_INPUT_OCCUPIED_CAPACITY: u64 = 8_200_000_000;
const ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAWABLE_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY - ORIGINAL_DAO_WITHDRAW_INPUT_OCCUPIED_CAPACITY;
const ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY: u64 = 123_468_305_678;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY: u64 = ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY * 2;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY: u64 = ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY * 3;
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY: u64 = 123_468_294_151;
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY: u64 =
    (ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY * 2) + ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY;
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY + ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY: u64 = 123_468_294_152;
const ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY: u64 =
    (ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY * 2) + ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY;
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_BOTH_WRONG_RATE_MAX_OUTPUT_CAPACITY: u64 = ORIGINAL_DAO_WITHDRAW_INPUT_OCCUPIED_CAPACITY
    + ((ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAWABLE_CAPACITY * ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE)
        / ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE);
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY: u64 =
    (ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY * 2) + ORIGINAL_DAO_WITHDRAW_PHASE2_BOTH_WRONG_RATE_MAX_OUTPUT_CAPACITY;
const ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY + ORIGINAL_DAO_WITHDRAW_PHASE2_BOTH_WRONG_RATE_MAX_OUTPUT_CAPACITY;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY + ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY;
const ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY: u64 =
    ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_OVER_OUTPUT_CAPACITY: u64 = ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY + 1;
const ORIGINAL_DAO_WITHDRAW_PHASE2_IMMATURE_OUTPUT_CAPACITY: u64 = 123_468_045_678;
const ORIGINAL_DAO_MAX_CYCLES: u64 = 50_000_000;
const DAO_WITHDRAWAL_CELLSCRIPT_ACTION: &str = "test_dao_withdrawal_since";
const DAO_WITHDRAWAL_CELLSCRIPT_PROGRAM: &str = r#"
module differential_dao_withdrawal;

action test_dao_withdrawal_since() -> u64 {

        ckb::require_current_script_args_empty();
        let input = source::group_input(0);
        let is_withdrawal = dao::is_withdrawal_request_data(input);
        if !is_withdrawal {
            return 34;
        }
        dao::require_input_since_at_least(input, 2306942530136048371);
        return 0;
}
"#;

include!("dao_withdrawal_programs.rs");
include!("dao_rate_programs.rs");
include!("dao_witness_programs.rs");
include!("order_and_owner_programs.rs");
