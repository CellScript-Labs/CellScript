// Executed differential cases share one inventory with explicit candidate export.

macro_rules! measured_cases {
    ($(fn $name:ident() $body:block)*) => {
        $(#[test] fn $name() $body)*
        fn run_all_measured_cases() {
            for case in [$($name as fn(),)*] { case(); }
        }
    };
}

measured_cases! {
fn differential_deposit_phase1_original_and_cellscript_agree() {
    let execution = deposit_phase1_differential_execution(VALID_DEPOSIT_PHASE1_CAPACITY, None);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(DEPOSIT_PHASE1_DIFF_SCENARIO, &execution);
}

fn differential_non_empty_args_both_reject() {
    let execution = non_empty_args_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(NON_EMPTY_ARGS_DIFF_SCENARIO, &execution);
}

fn differential_receipt_without_deposit_both_reject() {
    let execution = receipt_without_deposit_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_WITHOUT_DEPOSIT_DIFF_SCENARIO, &execution);
}

fn differential_duplicate_receipt_output_both_reject() {
    let execution = duplicate_receipt_output_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DUPLICATE_RECEIPT_OUTPUT_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_under_mint_both_reject() {
    let execution = receipt_group_under_mint_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_GROUP_UNDER_MINT_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_exact_mint_both_accept() {
    let execution = receipt_group_exact_mint_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let inputs = execution["normalized_fixture"]["inputs"].as_array().expect("receipt group inputs");
    assert_eq!(inputs[0]["data"], inputs[1]["data"], "executable receipt data has no receipt-id discriminator");
    assert!(inputs[0].get("receipt_id").is_none(), "normalized executable receipt input must not invent receipt_id");
    assert!(inputs[1].get("receipt_id").is_none(), "normalized executable receipt input must not invent receipt_id");
    assert_matrix_execution_matches(RECEIPT_GROUP_EXACT_MINT_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_mixed_quantities_both_accept() {
    let execution = receipt_group_mixed_quantities_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(RECEIPT_GROUP_MIXED_QUANTITIES_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_zero_first_quantity_both_accept() {
    let execution = receipt_group_zero_first_quantity_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let inputs = execution["normalized_fixture"]["inputs"].as_array().expect("receipt group inputs");
    assert_eq!(inputs[0]["receipt_quantity"], 0);
    assert_eq!(inputs[1]["receipt_quantity"], 1);
    assert_matrix_execution_matches(RECEIPT_GROUP_ZERO_FIRST_QUANTITY_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_quantity_zero_both_accept() {
    let execution = receipt_group_quantity_zero_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    let inputs = fixture["inputs"].as_array().expect("receipt group inputs");
    assert_eq!(inputs[0]["receipt_quantity"], 0);
    assert_eq!(inputs[1]["receipt_quantity"], 0);
    let output = &fixture["outputs"][0];
    assert_eq!(output["xudt_amount_low_u64"].as_u64(), Some(0));
    assert_eq!(output["expected_xudt_amount_low_u64"].as_u64(), Some(0));
    assert_matrix_execution_matches(RECEIPT_GROUP_QUANTITY_ZERO_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_quantity_two_both_accept() {
    let execution = receipt_group_quantity_two_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    let inputs = fixture["inputs"].as_array().expect("receipt group inputs");
    assert_eq!(inputs[0]["receipt_quantity"], 2);
    assert_eq!(inputs[1]["receipt_quantity"], 2);
    let output = &fixture["outputs"][0];
    assert_eq!(output["xudt_amount_low_u64"].as_u64(), Some((MINT_RECEIPT_QUANTITY_TWO_OUTPUT_AMOUNT * 2) as u64));
    assert_eq!(output["expected_xudt_amount_low_u64"].as_u64(), Some((MINT_RECEIPT_QUANTITY_TWO_OUTPUT_AMOUNT * 2) as u64));
    assert_matrix_execution_matches(RECEIPT_GROUP_QUANTITY_TWO_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_long_receipt_data_both_accept() {
    let execution = receipt_group_long_receipt_data_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let inputs = execution["normalized_fixture"]["inputs"].as_array().expect("receipt group inputs");
    assert_eq!(inputs[0]["data"].as_str().expect("first receipt data").len(), 28);
    assert_eq!(inputs[1]["data"].as_str().expect("second receipt data").len(), 28);
    assert_matrix_execution_matches(RECEIPT_GROUP_LONG_RECEIPT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_over_mint_both_reject() {
    let execution = receipt_group_over_mint_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_GROUP_OVER_MINT_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_amount_high_nonzero_both_reject() {
    let execution = receipt_group_amount_high_nonzero_differential_execution();
    assert_eq!(execution["failure_mode"], "receipt_group_amount_high_nonzero");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let output = &execution["normalized_fixture"]["outputs"][0];
    assert_eq!(output["xudt_amount_low_u64"].as_u64(), Some((MINT_RECEIPT_OUTPUT_AMOUNT * 2) as u64));
    assert_eq!(output["xudt_amount_high_u64"].as_u64(), Some(1));
    assert_eq!(output["expected_xudt_amount_high_u64"].as_u64(), Some(0));
    assert_matrix_execution_matches(RECEIPT_GROUP_AMOUNT_HIGH_NONZERO_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_missing_header_both_reject() {
    let execution = receipt_group_missing_header_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_GROUP_MISSING_HEADER_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_wrong_accumulated_rate_both_reject() {
    let execution = receipt_group_wrong_accumulated_rate_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_GROUP_WRONG_RATE_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_wrong_xudt_args_both_reject() {
    let execution = receipt_group_wrong_xudt_args_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_GROUP_WRONG_XUDT_ARGS_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_malformed_receipt_data_both_reject() {
    let execution = receipt_group_malformed_receipt_data_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_GROUP_MALFORMED_RECEIPT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_second_malformed_receipt_data_both_reject() {
    let execution = receipt_group_second_malformed_receipt_data_differential_execution();
    assert_eq!(execution["failure_mode"], "receipt_group_second_malformed_receipt_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(RECEIPT_GROUP_SECOND_MALFORMED_RECEIPT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_receipt_group_missing_second_input_both_reject() {
    let execution = receipt_group_missing_second_input_differential_execution();
    assert_eq!(execution["failure_mode"], "receipt_group_missing_second_input");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let inputs = execution["normalized_fixture"]["inputs"].as_array().expect("receipt group inputs");
    assert_eq!(inputs.len(), 1, "fixture must exercise a missing second receipt input");
    assert_matrix_execution_matches(RECEIPT_GROUP_MISSING_SECOND_INPUT_DIFF_SCENARIO, &execution);
}

fn differential_deposit_too_small_both_reject() {
    let execution = deposit_phase1_differential_execution(TINY_DEPOSIT_PHASE1_CAPACITY, Some("deposit_capacity_bound_rejected"));
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DEPOSIT_TOO_SMALL_DIFF_SCENARIO, &execution);
}

fn differential_deposit_too_big_both_reject() {
    let execution = deposit_phase1_upper_bound_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DEPOSIT_TOO_BIG_DIFF_SCENARIO, &execution);
}

fn differential_deposit_receipt_amount_mismatch_both_reject() {
    let execution = deposit_phase1_receipt_amount_mismatch_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_receipt_amount_mismatch");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_receipt_amount_mismatch");
    assert_eq!(fixture["outputs"][1]["receipt_quantity"], 1);
    assert_eq!(
        fixture["outputs"][1]["receipt_deposit_amount_shannons"],
        deposit_phase1_unoccupied_capacity(VALID_DEPOSIT_PHASE1_CAPACITY) + 1
    );
    assert_matrix_execution_matches(DEPOSIT_RECEIPT_AMOUNT_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_deposit_receipt_quantity_zero_both_reject() {
    let execution = deposit_phase1_receipt_quantity_zero_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_receipt_quantity_zero");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_receipt_quantity_zero");
    assert_eq!(fixture["outputs"][1]["receipt_quantity"], 0);
    assert_eq!(
        fixture["outputs"][1]["receipt_deposit_amount_shannons"],
        deposit_phase1_unoccupied_capacity(VALID_DEPOSIT_PHASE1_CAPACITY)
    );
    assert_matrix_execution_matches(DEPOSIT_RECEIPT_QUANTITY_ZERO_DIFF_SCENARIO, &execution);
}

fn differential_deposit_receipt_quantity_mismatch_both_reject() {
    let execution = deposit_phase1_receipt_quantity_mismatch_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_receipt_quantity_mismatch");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_receipt_quantity_mismatch");
    assert_eq!(fixture["outputs"][1]["receipt_quantity"], 2);
    assert_eq!(
        fixture["outputs"][1]["receipt_deposit_amount_shannons"],
        deposit_phase1_unoccupied_capacity(VALID_DEPOSIT_PHASE1_CAPACITY)
    );
    assert_matrix_execution_matches(DEPOSIT_RECEIPT_QUANTITY_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_deposit_receipt_short_data_both_reject() {
    let execution = deposit_phase1_receipt_short_data_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_receipt_short_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_receipt_short_data");
    assert_eq!(fixture["outputs"][1]["receipt_data_length_bytes"], 4);
    assert_matrix_execution_matches(DEPOSIT_RECEIPT_SHORT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_deposit_receipt_long_data_both_accept() {
    let execution = deposit_phase1_receipt_long_data_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_receipt_long_data");
    assert_eq!(fixture["outputs"][1]["receipt_data_length_bytes"], 13);
    assert_matrix_execution_matches(DEPOSIT_RECEIPT_LONG_DATA_DIFF_SCENARIO, &execution);
}

fn differential_deposit_missing_dao_type_both_reject() {
    let execution = deposit_phase1_missing_dao_type_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_missing_dao_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_missing_dao_type");
    assert_eq!(fixture["outputs"][0]["type"], Value::Null);
    assert_matrix_execution_matches(DEPOSIT_MISSING_DAO_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_deposit_wrong_dao_type_both_reject() {
    let execution = deposit_phase1_wrong_dao_type_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_wrong_dao_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_wrong_dao_type");
    assert_eq!(fixture["outputs"][0]["type"], "always_success_wrong_dao_type");
    assert_matrix_execution_matches(DEPOSIT_WRONG_DAO_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_deposit_wrong_lock_both_reject() {
    let execution = deposit_phase1_wrong_lock_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_wrong_ickb_lock");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_wrong_ickb_lock");
    assert_eq!(fixture["outputs"][0]["lock"], "always_success_wrong_deposit_lock");
    assert_matrix_execution_matches(DEPOSIT_WRONG_LOCK_DIFF_SCENARIO, &execution);
}

fn differential_deposit_short_data_both_reject() {
    let execution = deposit_phase1_short_data_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_short_dao_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_short_dao_data");
    assert_eq!(fixture["outputs"][0]["deposit_data_length_bytes"], 4);
    assert_matrix_execution_matches(DEPOSIT_SHORT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_deposit_nonzero_data_both_reject() {
    let execution = deposit_phase1_nonzero_data_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_nonzero_dao_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_nonzero_dao_data");
    assert_eq!(fixture["outputs"][0]["data"], "0x0100000000000000");
    assert_matrix_execution_matches(DEPOSIT_NONZERO_DATA_DIFF_SCENARIO, &execution);
}

fn differential_deposit_long_data_both_reject() {
    let execution = deposit_phase1_long_data_differential_execution();
    assert_eq!(execution["failure_mode"], "deposit_long_dao_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "deposit_long_dao_data");
    assert_eq!(fixture["outputs"][0]["deposit_data_length_bytes"], 9);
    assert_matrix_execution_matches(DEPOSIT_LONG_DATA_DIFF_SCENARIO, &execution);
}

fn differential_mint_from_receipt_both_accept() {
    let execution = mint_from_receipt_differential_execution(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintXudtBinding::ScriptUnderTest,
    );
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(MINT_FROM_RECEIPT_DIFF_SCENARIO, &execution);
}

fn differential_mint_from_quantity_two_receipt_both_accept() {
    let execution = mint_from_receipt_quantity_two_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(MINT_FROM_RECEIPT_QUANTITY_TWO_DIFF_SCENARIO, &execution);
}

fn differential_mint_from_quantity_zero_receipt_both_accept() {
    let execution = mint_from_receipt_quantity_zero_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let inputs = execution["normalized_fixture"]["inputs"].as_array().expect("mint receipt inputs");
    assert_eq!(inputs[0]["receipt_quantity"], 0);
    assert_matrix_execution_matches(MINT_FROM_RECEIPT_QUANTITY_ZERO_DIFF_SCENARIO, &execution);
}

fn differential_mint_from_long_receipt_data_both_accept() {
    let execution = mint_from_receipt_long_data_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let inputs = execution["normalized_fixture"]["inputs"].as_array().expect("mint receipt inputs");
    assert_eq!(inputs[0]["data"].as_str().expect("receipt data").len(), 28);
    assert_matrix_execution_matches(MINT_FROM_RECEIPT_LONG_DATA_DIFF_SCENARIO, &execution);
}

fn differential_mint_from_malformed_receipt_data_both_reject() {
    let execution = mint_from_receipt_malformed_receipt_data_differential_execution();
    assert_matrix_execution_matches(MINT_FROM_RECEIPT_MALFORMED_RECEIPT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_amount_inflation_both_reject() {
    let execution = mint_from_receipt_differential_execution(
        MINT_RECEIPT_OUTPUT_AMOUNT + 1,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("amount_inflation"),
        MintXudtBinding::ScriptUnderTest,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(AMOUNT_INFLATION_DIFF_SCENARIO, &execution);
}

fn differential_amount_high_nonzero_both_reject() {
    let execution = mint_from_receipt_high_word_differential_execution();
    assert_eq!(execution["failure_mode"], "amount_high_nonzero");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let output = &execution["normalized_fixture"]["outputs"][0];
    assert_eq!(output["xudt_amount_low_u64"], MINT_RECEIPT_OUTPUT_AMOUNT as u64);
    assert_eq!(output["xudt_amount_high_u64"], 1);
    assert_matrix_execution_matches(AMOUNT_HIGH_NONZERO_DIFF_SCENARIO, &execution);
}

fn differential_amount_deflation_both_reject() {
    let execution = mint_from_receipt_differential_execution(
        MINT_RECEIPT_OUTPUT_AMOUNT - 1,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("amount_deflation"),
        MintXudtBinding::ScriptUnderTest,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(AMOUNT_DEFLATION_DIFF_SCENARIO, &execution);
}

fn differential_wrong_xudt_args_both_reject() {
    let execution = mint_from_receipt_differential_execution(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("wrong_xudt_binding"),
        MintXudtBinding::WrongOwnerHash,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(WRONG_XUDT_ARGS_DIFF_SCENARIO, &execution);
}

fn differential_wrong_accumulated_rate_both_reject() {
    let execution = mint_from_receipt_differential_execution(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        WRONG_MINT_RECEIPT_ACCUMULATED_RATE,
        Some("wrong_accumulated_rate"),
        MintXudtBinding::ScriptUnderTest,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(WRONG_ACCUMULATED_RATE_DIFF_SCENARIO, &execution);
}

fn differential_missing_header_dep_both_reject() {
    let execution = mint_from_receipt_differential_execution_with_header_dep(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("missing_header_dep"),
        MintXudtBinding::ScriptUnderTest,
        MintHeaderDepMode::Omitted,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(MISSING_HEADER_DEP_DIFF_SCENARIO, &execution);
}

fn differential_dao_mature_withdrawal_both_accept() {
    let execution = dao_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        None,
    );
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(DAO_MATURE_WITHDRAWAL_DIFF_SCENARIO, &execution);
}

fn differential_dao_immature_withdrawal_both_reject() {
    let execution = dao_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_IMMATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_IMMATURE_OUTPUT_CAPACITY,
        Some("dao_incorrect_since"),
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_immature_withdrawal");
    assert_eq!(fixture["failure_mode"], "dao_incorrect_since");
    assert!(fixture.get("current_epoch").is_none(), "executable DAO fixture must not invent model current_epoch");
    assert!(fixture.get("maturity_epoch").is_none(), "executable DAO fixture must not invent model maturity_epoch");
    let input = &fixture["inputs"].as_array().expect("DAO withdrawal inputs")[0];
    assert_eq!(input["since_u64"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_IMMATURE_SINCE));
    assert!(input.get("current_epoch").is_none(), "DAO input must express maturity through since, not model current_epoch");
    assert!(input.get("maturity_epoch").is_none(), "DAO input must express maturity through since, not model maturity_epoch");
    assert_matrix_execution_matches(DAO_IMMATURE_WITHDRAWAL_DIFF_SCENARIO, &execution);
}

fn differential_dao_max_withdrawal_capacity_both_accept() {
    let execution = dao_withdrawal_max_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_max_withdrawal_capacity");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY));
    assert_eq!(output["expected_maximum_capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY));
    assert_eq!(output["capacity_boundary"], "exact_maximum");
    assert_matrix_execution_matches(DAO_MAX_WITHDRAWAL_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_max_withdrawal_capacity_both_accept() {
    let execution = dao_two_input_withdrawal_max_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_max_withdrawal_capacity");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs.len(), 2, "fixture must spend two DAO withdrawal inputs");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY));
    assert_eq!(
        output["expected_maximum_capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum");
    assert_matrix_execution_matches(DAO_TWO_INPUT_MAX_WITHDRAWAL_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_over_withdrawal_capacity_both_reject() {
    let execution = dao_two_input_withdrawal_over_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_over_withdraw_capacity");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs.len(), 2, "fixture must spend two DAO withdrawal inputs");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_OVER_OUTPUT_CAPACITY));
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_TWO_INPUT_OVER_WITHDRAWAL_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_mixed_deposit_rate_max_both_accept() {
    let execution = dao_two_input_mixed_deposit_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_mixed_deposit_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps.len(), 3, "mixed-rate fixture must expose two deposit headers");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    assert_eq!(header_deps[2]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE));
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "two_input_mixed_deposit_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_mixed_deposit_rate_over_both_reject() {
    let execution = dao_two_input_mixed_deposit_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_mixed_deposit_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_mixed_deposit_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "two_input_mixed_deposit_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_mixed_withdraw_rate_max_both_accept() {
    let execution = dao_two_input_mixed_withdraw_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_mixed_withdraw_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps.len(), 3, "mixed withdraw-rate fixture must expose two withdraw headers");
    assert_eq!(header_deps[2]["role"], "withdraw_header_mixed_rate");
    assert_eq!(header_deps[2]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE));
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "two_input_mixed_withdraw_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_mixed_withdraw_rate_over_both_reject() {
    let execution = dao_two_input_mixed_withdraw_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_mixed_withdraw_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_mixed_withdraw_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "withdraw_header_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "two_input_mixed_withdraw_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_mixed_both_rate_max_both_accept() {
    let execution = dao_two_input_mixed_both_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_mixed_both_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_mixed_rate");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["linked_header"], "withdraw_header_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY));
    assert_eq!(output["capacity_boundary"], "two_input_mixed_both_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_TWO_INPUT_MIXED_BOTH_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_mixed_both_rate_over_both_reject() {
    let execution = dao_two_input_mixed_both_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_mixed_both_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_mixed_both_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "two_input_mixed_both_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_TWO_INPUT_MIXED_BOTH_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_missing_witness_input_type_both_reject() {
    let execution = dao_two_input_second_missing_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_missing_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_missing_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_present"], false);
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum_with_malformed_second_witness");
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_empty_witness_input_type_both_reject() {
    let execution = dao_two_input_second_empty_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_empty_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_empty_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_present"], true);
    assert_eq!(witnesses[1]["input_type_length_bytes"].as_u64(), Some(0));
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_short_witness_input_type_both_reject() {
    let execution = dao_two_input_second_short_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_short_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_short_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_present"], true);
    assert_eq!(witnesses[1]["input_type_length_bytes"].as_u64(), Some(1));
    assert_eq!(witnesses[1]["expected_input_type_length_bytes"].as_u64(), Some(8));
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_long_witness_input_type_both_reject() {
    let execution = dao_two_input_second_long_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_long_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_long_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_present"], true);
    assert_eq!(witnesses[1]["input_type_length_bytes"].as_u64(), Some(9));
    assert_eq!(witnesses[1]["expected_input_type_length_bytes"].as_u64(), Some(8));
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_withdraw_header_witness_index_both_reject() {
    let execution = dao_two_input_second_withdraw_header_witness_index_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_withdraw_header_witness_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_withdraw_header_witness_index");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(0));
    assert_eq!(witnesses[1]["expected_input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[1]["witness_index_role"], "withdraw_header_instead_of_deposit_header");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum_with_malformed_second_witness_index");
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_WITHDRAW_HEADER_WITNESS_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_oob_witness_index_both_reject() {
    let execution = dao_two_input_second_oob_witness_index_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_oob_witness_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_oob_witness_index");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    assert_eq!(witnesses[1]["expected_input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[1]["witness_index_role"], "out_of_bounds_header_dep_index");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum_with_malformed_second_witness_index");
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_OOB_WITNESS_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_deposit_data_input_both_reject() {
    let execution = dao_two_input_second_deposit_data_input_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_deposit_data_input");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_deposit_data_input");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["role"], "deposit_data_dao_cell_spent_as_second_withdrawal");
    assert_eq!(inputs[1]["data"], hex_prefixed(&dao_deposit_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum_with_non_withdrawal_second_input_data");
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_DEPOSIT_DATA_INPUT_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_malformed_input_data_both_reject() {
    let execution = dao_two_input_second_malformed_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_malformed_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_malformed_input_data");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["role"], "malformed_data_dao_cell_spent_as_second_withdrawal");
    assert_eq!(inputs[1]["data"], hex_prefixed(&dao_malformed_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum_with_non_withdrawal_second_input_data");
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_MALFORMED_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_two_input_second_long_input_data_both_reject() {
    let execution = dao_two_input_second_long_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_two_input_second_long_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_two_input_second_long_input_data");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["role"], "long_data_dao_cell_spent_as_second_withdrawal");
    assert_eq!(inputs[1]["data"], hex_prefixed(&dao_long_withdrawal_request_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "two_input_exact_maximum_with_non_withdrawal_second_input_data");
    assert_matrix_execution_matches(DAO_TWO_INPUT_SECOND_LONG_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_max_withdrawal_capacity_both_accept() {
    let execution = dao_three_input_withdrawal_max_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_max_withdrawal_capacity");
    assert_eq!(fixture["input_capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY * 3));
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs.len(), 3);
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses.len(), 3);
    for witness in witnesses {
        assert_eq!(witness["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    }
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY));
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_MAX_WITHDRAWAL_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_over_withdrawal_capacity_both_reject() {
    let execution = dao_three_input_withdrawal_over_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_over_withdraw_capacity");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_OVER_OUTPUT_CAPACITY));
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_OVER_WITHDRAWAL_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_mixed_deposit_rate_max_both_accept() {
    let execution = dao_three_input_mixed_deposit_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_mixed_deposit_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    assert_eq!(header_deps[2]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE));
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[0]["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[2]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_mixed_deposit_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_mixed_deposit_rate_over_both_reject() {
    let execution = dao_three_input_mixed_deposit_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_mixed_deposit_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_mixed_deposit_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_mixed_deposit_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_mixed_withdraw_rate_max_both_accept() {
    let execution = dao_three_input_mixed_withdraw_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_mixed_withdraw_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "withdraw_header_mixed_rate");
    assert_eq!(header_deps[2]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE));
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[2]["linked_header"], "withdraw_header_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    for witness in witnesses {
        assert_eq!(witness["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    }
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_mixed_withdraw_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_mixed_withdraw_rate_over_both_reject() {
    let execution = dao_three_input_mixed_withdraw_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_mixed_withdraw_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_mixed_withdraw_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "withdraw_header_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_mixed_withdraw_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_mixed_both_rate_max_both_accept() {
    let execution = dao_three_input_mixed_both_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_mixed_both_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_mixed_rate");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[2]["linked_header"], "withdraw_header_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[2]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_mixed_both_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_MIXED_BOTH_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_mixed_both_rate_over_both_reject() {
    let execution = dao_three_input_mixed_both_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_mixed_both_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_mixed_both_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_mixed_both_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_MIXED_BOTH_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_mixed_deposit_rate_max_both_accept() {
    let execution = dao_three_input_second_mixed_deposit_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_mixed_deposit_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_second_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[0]["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    assert_eq!(witnesses[2]["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_second_mixed_deposit_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_mixed_deposit_rate_over_both_reject() {
    let execution = dao_three_input_second_mixed_deposit_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_mixed_deposit_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_mixed_deposit_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_second_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_second_mixed_deposit_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_mixed_withdraw_rate_max_both_accept() {
    let execution = dao_three_input_second_mixed_withdraw_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_mixed_withdraw_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "withdraw_header_second_mixed_rate");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["linked_header"], "withdraw_header_second_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    for witness in witnesses {
        assert_eq!(witness["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    }
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_second_mixed_withdraw_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_mixed_withdraw_rate_over_both_reject() {
    let execution = dao_three_input_second_mixed_withdraw_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_mixed_withdraw_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_mixed_withdraw_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "withdraw_header_second_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_second_mixed_withdraw_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_mixed_both_rate_max_both_accept() {
    let execution = dao_three_input_second_mixed_both_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_mixed_both_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_second_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_second_mixed_rate");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["linked_header"], "withdraw_header_second_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_second_mixed_both_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_mixed_both_rate_over_both_reject() {
    let execution = dao_three_input_second_mixed_both_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_mixed_both_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_mixed_both_rate_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_second_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_second_mixed_rate");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_second_mixed_both_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_deposit_third_withdraw_rate_max_both_accept() {
    let execution = dao_three_input_second_deposit_third_withdraw_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_deposit_third_withdraw_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_second_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_mixed_rate");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[2]["linked_header"], "withdraw_header_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    assert_eq!(witnesses[2]["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_second_deposit_third_withdraw_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_deposit_third_withdraw_rate_over_both_reject() {
    let execution = dao_three_input_second_deposit_third_withdraw_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_deposit_third_withdraw_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_deposit_third_withdraw_rate_over_withdraw_capacity");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_second_deposit_third_withdraw_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_withdraw_third_deposit_rate_max_both_accept() {
    let execution = dao_three_input_second_withdraw_third_deposit_rate_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_withdraw_third_deposit_rate_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[2]["role"], "deposit_header_mixed_rate");
    assert_eq!(header_deps[3]["role"], "withdraw_header_second_mixed_rate");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["linked_header"], "withdraw_header_second_mixed_rate");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[2]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_eq!(output["capacity_boundary"], "three_input_second_withdraw_third_deposit_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_withdraw_third_deposit_rate_over_both_reject() {
    let execution = dao_three_input_second_withdraw_third_deposit_rate_over_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_withdraw_third_deposit_rate_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_withdraw_third_deposit_rate_over_withdraw_capacity");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["capacity_shannons"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY)
    );
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "three_input_second_withdraw_third_deposit_rate_exact_maximum_plus_one");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_missing_witness_input_type_both_reject() {
    let execution = dao_three_input_second_missing_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_missing_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_missing_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["witness_input_type_shape"], "missing");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_malformed_second_witness");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_empty_witness_input_type_both_reject() {
    let execution = dao_three_input_second_empty_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_empty_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_empty_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["witness_input_type_shape"], "empty");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_short_witness_input_type_both_reject() {
    let execution = dao_three_input_second_short_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_short_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_short_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["witness_input_type_shape"], "short_1_byte");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_long_witness_input_type_both_reject() {
    let execution = dao_three_input_second_long_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_long_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_long_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["witness_input_type_shape"], "long_9_bytes");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_withdraw_header_witness_index_both_reject() {
    let execution = dao_three_input_second_withdraw_header_witness_index_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_withdraw_header_witness_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_withdraw_header_witness_index");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(0));
    assert_eq!(witnesses[1]["expected_input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[1]["witness_index_role"], "withdraw_header_instead_of_deposit_header");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_malformed_second_witness_index");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_WITHDRAW_HEADER_WITNESS_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_oob_witness_index_both_reject() {
    let execution = dao_three_input_second_oob_witness_index_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_oob_witness_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_oob_witness_index");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[1]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    assert_eq!(witnesses[1]["expected_input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[1]["witness_index_role"], "out_of_bounds_header_dep_index");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_malformed_second_witness_index");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_OOB_WITNESS_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_deposit_data_input_both_reject() {
    let execution = dao_three_input_second_deposit_data_input_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_deposit_data_input");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_deposit_data_input");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["role"], "deposit_data_dao_cell_spent_as_second_withdrawal");
    assert_eq!(inputs[1]["data"], hex_prefixed(&dao_deposit_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_non_withdrawal_second_input_data");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_DEPOSIT_DATA_INPUT_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_malformed_input_data_both_reject() {
    let execution = dao_three_input_second_malformed_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_malformed_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_malformed_input_data");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["role"], "malformed_data_dao_cell_spent_as_second_withdrawal");
    assert_eq!(inputs[1]["data"], hex_prefixed(&dao_malformed_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_non_withdrawal_second_input_data");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_MALFORMED_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_second_long_input_data_both_reject() {
    let execution = dao_three_input_second_long_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_second_long_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_second_long_input_data");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[1]["role"], "long_data_dao_cell_spent_as_second_withdrawal");
    assert_eq!(inputs[1]["data"], hex_prefixed(&dao_long_withdrawal_request_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_non_withdrawal_second_input_data");
    assert_matrix_execution_matches(DAO_THREE_INPUT_SECOND_LONG_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_missing_witness_input_type_both_reject() {
    let execution = dao_three_input_third_missing_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_missing_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_missing_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[2]["witness_input_type_shape"], "missing");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_malformed_third_witness");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_empty_witness_input_type_both_reject() {
    let execution = dao_three_input_third_empty_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_empty_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_empty_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[2]["witness_input_type_shape"], "empty");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_short_witness_input_type_both_reject() {
    let execution = dao_three_input_third_short_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_short_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_short_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[2]["witness_input_type_shape"], "short_1_byte");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_long_witness_input_type_both_reject() {
    let execution = dao_three_input_third_long_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_long_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_long_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[2]["witness_input_type_shape"], "long_9_bytes");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_withdraw_header_witness_index_both_reject() {
    let execution = dao_three_input_third_withdraw_header_witness_index_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_withdraw_header_witness_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_withdraw_header_witness_index");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[2]["input_type_header_dep_index_le_u64"].as_u64(), Some(0));
    assert_eq!(witnesses[2]["expected_input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[2]["witness_index_role"], "withdraw_header_instead_of_deposit_header");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_malformed_third_witness_index");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_WITHDRAW_HEADER_WITNESS_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_oob_witness_index_both_reject() {
    let execution = dao_three_input_third_oob_witness_index_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_oob_witness_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_oob_witness_index");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[2]["input_type_header_dep_index_le_u64"].as_u64(), Some(2));
    assert_eq!(witnesses[2]["expected_input_type_header_dep_index_le_u64"].as_u64(), Some(1));
    assert_eq!(witnesses[2]["witness_index_role"], "out_of_bounds_header_dep_index");
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_malformed_third_witness_index");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_OOB_WITNESS_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_deposit_data_input_both_reject() {
    let execution = dao_three_input_third_deposit_data_input_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_deposit_data_input");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_deposit_data_input");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[2]["role"], "deposit_data_dao_cell_spent_as_third_withdrawal");
    assert_eq!(inputs[2]["data"], hex_prefixed(&dao_deposit_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_non_withdrawal_third_input_data");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_DEPOSIT_DATA_INPUT_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_malformed_input_data_both_reject() {
    let execution = dao_three_input_third_malformed_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_malformed_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_malformed_input_data");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[2]["role"], "malformed_data_dao_cell_spent_as_third_withdrawal");
    assert_eq!(inputs[2]["data"], hex_prefixed(&dao_malformed_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_non_withdrawal_third_input_data");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_MALFORMED_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_three_input_third_long_input_data_both_reject() {
    let execution = dao_three_input_third_long_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_three_input_third_long_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_three_input_third_long_input_data");
    let inputs = fixture["inputs"].as_array().expect("DAO withdrawal inputs");
    assert_eq!(inputs[2]["role"], "long_data_dao_cell_spent_as_third_withdrawal");
    assert_eq!(inputs[2]["data"], hex_prefixed(&dao_long_withdrawal_request_cell_data()));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_boundary"], "three_input_exact_maximum_with_non_withdrawal_third_input_data");
    assert_matrix_execution_matches(DAO_THREE_INPUT_THIRD_LONG_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_wrong_deposit_rate_both_reject() {
    let execution = dao_withdrawal_wrong_deposit_rate_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_wrong_deposit_accumulated_rate");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_wrong_deposit_accumulated_rate");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[1]["role"], "deposit_header_wrong_accumulated_rate");
    assert_eq!(header_deps[1]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE));
    assert_matrix_execution_matches(DAO_WRONG_DEPOSIT_RATE_DIFF_SCENARIO, &execution);
}

fn differential_dao_deposit_rate_adjusted_max_both_accept() {
    let execution = dao_withdrawal_deposit_rate_adjusted_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_deposit_rate_adjusted_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[1]["role"], "deposit_header_wrong_accumulated_rate");
    assert_eq!(header_deps[1]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY));
    assert_eq!(output["capacity_boundary"], "fixture_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_DEPOSIT_RATE_ADJUSTED_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_deposit_rate_adjusted_over_capacity_both_reject() {
    let execution = dao_withdrawal_deposit_rate_adjusted_over_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_deposit_rate_adjusted_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_deposit_rate_adjusted_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[1]["role"], "deposit_header_wrong_accumulated_rate");
    assert_eq!(header_deps[1]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_OVER_OUTPUT_CAPACITY));
    assert_eq!(output["overdrawn_by_shannons_under_fixture_rate"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "fixture_rate_plus_one");
    assert_matrix_execution_matches(DAO_DEPOSIT_RATE_ADJUSTED_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_wrong_withdraw_rate_both_reject() {
    let execution = dao_withdrawal_wrong_withdraw_rate_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_wrong_withdraw_accumulated_rate");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_wrong_withdraw_accumulated_rate");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[0]["role"], "withdraw_header_wrong_accumulated_rate");
    assert_eq!(header_deps[0]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(
        output["expected_maximum_capacity_shannons_under_fixture_rate"].as_u64(),
        Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY)
    );
    assert_matrix_execution_matches(DAO_WRONG_WITHDRAW_RATE_DIFF_SCENARIO, &execution);
}

fn differential_dao_withdraw_rate_adjusted_max_both_accept() {
    let execution = dao_withdrawal_withdraw_rate_adjusted_max_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_withdraw_rate_adjusted_max_withdrawal_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[0]["role"], "withdraw_header_wrong_accumulated_rate");
    assert_eq!(header_deps[0]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY));
    assert_eq!(output["capacity_boundary"], "fixture_rate_exact_maximum");
    assert_matrix_execution_matches(DAO_WITHDRAW_RATE_ADJUSTED_MAX_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_withdraw_rate_adjusted_over_capacity_both_reject() {
    let execution = dao_withdrawal_withdraw_rate_adjusted_over_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_withdraw_rate_adjusted_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_withdraw_rate_adjusted_over_withdraw_capacity");
    let header_deps = fixture["header_deps"].as_array().expect("DAO withdrawal header deps");
    assert_eq!(header_deps[0]["role"], "withdraw_header_wrong_accumulated_rate");
    assert_eq!(header_deps[0]["accumulated_rate"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE));
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY));
    assert_eq!(output["overdrawn_by_shannons_under_fixture_rate"].as_u64(), Some(1));
    assert_eq!(output["capacity_boundary"], "fixture_rate_plus_one");
    assert_matrix_execution_matches(DAO_WITHDRAW_RATE_ADJUSTED_OVER_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_over_withdraw_capacity_both_reject() {
    let execution = dao_withdrawal_over_capacity_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_over_withdraw_capacity");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    let output = &fixture["outputs"].as_array().expect("DAO withdrawal outputs")[0];
    assert_eq!(output["capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_OVER_OUTPUT_CAPACITY));
    assert_eq!(output["expected_maximum_capacity_shannons"].as_u64(), Some(ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY));
    assert_eq!(output["overdrawn_by_shannons"].as_u64(), Some(1));
    assert_matrix_execution_matches(DAO_OVER_WITHDRAW_CAPACITY_DIFF_SCENARIO, &execution);
}

fn differential_dao_missing_withdraw_header_both_reject() {
    let execution = dao_withdrawal_missing_withdraw_header_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DAO_MISSING_WITHDRAW_HEADER_DIFF_SCENARIO, &execution);
}

fn differential_dao_wrong_deposit_header_index_both_reject() {
    let execution = dao_withdrawal_wrong_deposit_header_index_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DAO_WRONG_DEPOSIT_HEADER_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_dao_wrong_withdraw_committed_header_both_reject() {
    let execution = dao_withdrawal_wrong_withdraw_committed_header_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DAO_WRONG_WITHDRAW_COMMITTED_HEADER_DIFF_SCENARIO, &execution);
}

fn differential_dao_missing_deposit_header_both_reject() {
    let execution = dao_withdrawal_missing_deposit_header_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DAO_MISSING_DEPOSIT_HEADER_DIFF_SCENARIO, &execution);
}

fn differential_dao_deposit_header_index_oob_both_reject() {
    let execution = dao_withdrawal_deposit_header_index_oob_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DAO_DEPOSIT_HEADER_INDEX_OOB_DIFF_SCENARIO, &execution);
}

fn differential_dao_withdrawal_deposit_data_input_both_reject() {
    let execution = dao_withdrawal_deposit_data_input_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_withdrawal_deposit_data_input");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DAO_WITHDRAWAL_DEPOSIT_DATA_INPUT_DIFF_SCENARIO, &execution);
}

fn differential_dao_withdrawal_malformed_input_data_both_reject() {
    let execution = dao_withdrawal_malformed_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_withdrawal_malformed_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(DAO_WITHDRAWAL_MALFORMED_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_withdrawal_long_input_data_both_reject() {
    let execution = dao_withdrawal_long_input_data_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_withdrawal_long_input_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_withdrawal_long_input_data");
    let inputs = fixture["inputs"].as_array().expect("DAO inputs");
    assert_eq!(inputs[0]["data"], hex_prefixed(&dao_long_withdrawal_request_cell_data()));
    assert_matrix_execution_matches(DAO_WITHDRAWAL_LONG_INPUT_DATA_DIFF_SCENARIO, &execution);
}

fn differential_dao_missing_witness_input_type_both_reject() {
    let execution = dao_withdrawal_missing_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_missing_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_missing_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[0]["input_type_present"], false);
    assert_matrix_execution_matches(DAO_MISSING_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_empty_witness_input_type_both_reject() {
    let execution = dao_withdrawal_empty_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_empty_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_empty_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[0]["input_type_present"], true);
    assert_eq!(witnesses[0]["input_type_length_bytes"].as_u64(), Some(0));
    assert_matrix_execution_matches(DAO_EMPTY_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_short_witness_input_type_both_reject() {
    let execution = dao_withdrawal_short_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_short_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_short_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[0]["input_type_present"], true);
    assert_eq!(witnesses[0]["input_type_length_bytes"].as_u64(), Some(1));
    assert_eq!(witnesses[0]["expected_input_type_length_bytes"].as_u64(), Some(8));
    assert_matrix_execution_matches(DAO_SHORT_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_dao_long_witness_input_type_both_reject() {
    let execution = dao_withdrawal_long_witness_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "dao_long_witness_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "dao_long_witness_input_type");
    let witnesses = fixture["witnesses"].as_array().expect("DAO withdrawal witnesses");
    assert_eq!(witnesses[0]["input_type_present"], true);
    assert_eq!(witnesses[0]["input_type_length_bytes"].as_u64(), Some(9));
    assert_eq!(witnesses[0]["expected_input_type_length_bytes"].as_u64(), Some(8));
    assert_matrix_execution_matches(DAO_LONG_WITNESS_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_valid_limit_order_both_accept() {
    let execution = limit_order_differential_execution(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        None,
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(LIMIT_ORDER_VALID_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_min_match_boundary_both_accept() {
    let execution = limit_order_min_match_boundary_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(LIMIT_ORDER_MIN_MATCH_BOUNDARY_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_underpayment_both_reject() {
    let execution = limit_order_differential_execution(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_UNDERPAYMENT_OUTPUT_UDT_AMOUNT,
        Some("limit_order_underpayment"),
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_UNDERPAYMENT_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_wrong_asset_both_reject() {
    let execution = limit_order_differential_execution(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_WRONG_ASSET_OUTPUT_UDT_AMOUNT,
        Some("wrong_asset"),
        LimitOrderAssetBinding::DifferentAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_WRONG_ASSET_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_insufficient_match_both_reject() {
    let execution = limit_order_differential_execution(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_INSUFFICIENT_MATCH_OUTPUT_CAPACITY,
        LIMIT_ORDER_INSUFFICIENT_MATCH_OUTPUT_UDT_AMOUNT,
        Some("insufficient_match"),
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_INSUFFICIENT_MATCH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_no_ckb_paid_both_reject() {
    let execution = limit_order_differential_execution(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_NO_CKB_PAID_OUTPUT_CAPACITY,
        LIMIT_ORDER_NO_CKB_PAID_OUTPUT_UDT_AMOUNT,
        Some("no_ckb_paid_out"),
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_NO_CKB_PAID_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_decreased_both_reject() {
    let execution = limit_order_differential_execution(
        LIMIT_ORDER_UDT_DECREASED_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_UDT_DECREASED_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_DECREASED_OUTPUT_UDT_AMOUNT,
        Some("udt_decreased"),
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_DECREASED_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_wrong_master_tx_hash_both_reject() {
    let execution = limit_order_wrong_master_tx_hash_differential_execution();
    assert_eq!(execution["failure_mode"], "wrong_master_tx_hash");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_wrong_master_tx_hash");
    assert_eq!(fixture["outputs"][0]["master_out_point"]["tx_hash"], hex_prefixed(&LIMIT_ORDER_WRONG_MASTER_TX_HASH));
    assert_matrix_execution_matches(LIMIT_ORDER_WRONG_MASTER_TX_HASH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_wrong_master_index_both_reject() {
    let execution = limit_order_wrong_master_index_differential_execution();
    assert_eq!(execution["failure_mode"], "wrong_master_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_wrong_master_index");
    assert_eq!(fixture["outputs"][0]["master_out_point"]["index"], 1);
    assert_matrix_execution_matches(LIMIT_ORDER_WRONG_MASTER_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_output_mint_action_both_reject() {
    let execution = limit_order_output_mint_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_output_mint_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_output_mint_action");
    assert_eq!(fixture["outputs"][0]["order_action"], "Mint");
    assert_matrix_execution_matches(LIMIT_ORDER_OUTPUT_MINT_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_output_invalid_action_both_reject() {
    let execution = limit_order_output_invalid_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_output_invalid_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_output_invalid_action");
    assert_eq!(fixture["outputs"][0]["order_action"], "Invalid");
    assert_matrix_execution_matches(LIMIT_ORDER_OUTPUT_INVALID_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_output_short_action_both_reject() {
    let execution = limit_order_output_short_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_output_short_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_output_short_action");
    assert_eq!(fixture["outputs"][0]["data"].as_str().expect("output data hex").len(), 2 + 16 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_OUTPUT_SHORT_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_output_short_master_both_reject() {
    let execution = limit_order_output_short_master_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_output_short_master_out_point");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_output_short_master_out_point");
    assert_eq!(fixture["outputs"][0]["order_action"], "Match");
    assert_eq!(fixture["outputs"][0]["data"].as_str().expect("output data hex").len(), 2 + 28 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_OUTPUT_SHORT_MASTER_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_output_long_data_both_reject() {
    let execution = limit_order_output_long_data_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_output_long_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_output_long_data");
    assert_eq!(fixture["outputs"][0]["order_action"], "Match");
    let data = fixture["outputs"][0]["data"].as_str().expect("output data hex");
    assert_eq!(data.len(), 2 + 90 * 2);
    assert!(data.ends_with("99"));
    assert_matrix_execution_matches(LIMIT_ORDER_OUTPUT_LONG_DATA_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_input_invalid_action_both_reject() {
    let execution = limit_order_input_invalid_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_input_invalid_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_input_invalid_action");
    assert_eq!(fixture["inputs"][0]["order_action"], "Invalid");
    assert_matrix_execution_matches(LIMIT_ORDER_INPUT_INVALID_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_input_short_action_both_reject() {
    let execution = limit_order_input_short_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_input_short_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_input_short_action");
    assert_eq!(fixture["inputs"][0]["data"].as_str().expect("input data hex").len(), 2 + 16 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_INPUT_SHORT_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_input_short_master_both_reject() {
    let execution = limit_order_input_short_master_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_input_short_master_out_point");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_input_short_master_out_point");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert_eq!(fixture["inputs"][0]["data"].as_str().expect("input data hex").len(), 2 + 28 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_INPUT_SHORT_MASTER_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_input_long_data_both_reject() {
    let execution = limit_order_input_long_data_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_input_long_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_input_long_data");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    let data = fixture["inputs"][0]["data"].as_str().expect("input data hex");
    assert_eq!(data.len(), 2 + 90 * 2);
    assert!(data.ends_with("99"));
    assert_matrix_execution_matches(LIMIT_ORDER_INPUT_LONG_DATA_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_input_absolute_match_both_accept() {
    let execution = limit_order_input_absolute_match_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_input_absolute_match");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert_matrix_execution_matches(LIMIT_ORDER_INPUT_ABSOLUTE_MATCH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_input_wrong_master_tx_hash_both_reject() {
    let execution = limit_order_input_wrong_master_tx_hash_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_input_wrong_master_tx_hash");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_input_wrong_master_tx_hash");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert!(fixture["inputs"][0]["data"].as_str().expect("input data").contains("787878"));
    assert_matrix_execution_matches(LIMIT_ORDER_INPUT_WRONG_MASTER_TX_HASH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_input_wrong_master_index_both_reject() {
    let execution = limit_order_input_wrong_master_index_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_input_wrong_master_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_input_wrong_master_index");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert_matrix_execution_matches(LIMIT_ORDER_INPUT_WRONG_MASTER_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_missing_matching_output_both_reject() {
    let execution = limit_order_missing_matching_output_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_missing_matching_output");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_missing_matching_output");
    assert_eq!(fixture["script_under_test_roles"].as_array().expect("roles").len(), 1);
    assert_eq!(fixture["outputs"][0]["lock"], "always_success");
    assert_matrix_execution_matches(LIMIT_ORDER_MISSING_MATCHING_OUTPUT_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_duplicate_matching_output_both_reject() {
    let execution = limit_order_duplicate_matching_output_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_duplicate_matching_output");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_duplicate_matching_output");
    assert_eq!(fixture["outputs"].as_array().expect("outputs").len(), 2);
    assert_eq!(fixture["outputs"][0]["lock"], "script_under_test");
    assert_eq!(fixture["outputs"][1]["lock"], "script_under_test");
    assert_matrix_execution_matches(LIMIT_ORDER_DUPLICATE_MATCHING_OUTPUT_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_missing_input_type_both_reject() {
    let execution = limit_order_missing_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_missing_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_missing_input_type");
    assert_eq!(fixture["inputs"][0]["type"], Value::Null);
    assert_eq!(fixture["outputs"][0]["type"], "auxiliary_udt_type");
    assert_matrix_execution_matches(LIMIT_ORDER_MISSING_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_missing_output_type_both_reject() {
    let execution = limit_order_missing_output_type_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_missing_output_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_missing_output_type");
    assert_eq!(fixture["inputs"][0]["type"], "auxiliary_udt_type");
    assert_eq!(fixture["outputs"][0]["type"], Value::Null);
    assert_matrix_execution_matches(LIMIT_ORDER_MISSING_OUTPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_valid_limit_order_udt_to_ckb_both_accept() {
    let execution = limit_order_udt_to_ckb_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_VALID_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_min_match_boundary_both_accept() {
    let execution = limit_order_udt_to_ckb_min_match_boundary_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_MIN_MATCH_BOUNDARY_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_no_udt_paid_both_reject() {
    let execution = limit_order_udt_to_ckb_differential_execution_with_params(
        LIMIT_ORDER_UDT_TO_CKB_NO_UDT_PAID_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_NO_UDT_PAID_OUTPUT_UDT_AMOUNT,
        Some("no_udt_paid_out"),
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_NO_UDT_PAID_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_wrong_asset_both_reject() {
    let execution = limit_order_udt_to_ckb_differential_execution_with_params(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        Some("wrong_asset"),
        LimitOrderAssetBinding::DifferentAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_WRONG_ASSET_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_insufficient_match_both_reject() {
    let execution = limit_order_udt_to_ckb_differential_execution_with_params(
        LIMIT_ORDER_UDT_TO_CKB_INSUFFICIENT_MATCH_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_INSUFFICIENT_MATCH_OUTPUT_UDT_AMOUNT,
        Some("insufficient_match"),
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INSUFFICIENT_MATCH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_underpayment_both_reject() {
    let execution = limit_order_udt_to_ckb_differential_execution_with_params(
        LIMIT_ORDER_UDT_TO_CKB_UNDERPAYMENT_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_UNDERPAYMENT_OUTPUT_UDT_AMOUNT,
        Some("limit_order_underpayment"),
        LimitOrderAssetBinding::SameAuxiliaryType,
    );
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_UNDERPAYMENT_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_wrong_master_tx_hash_both_reject() {
    let execution = limit_order_udt_to_ckb_wrong_master_tx_hash_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_wrong_master_tx_hash");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_wrong_master_tx_hash");
    assert_eq!(fixture["outputs"][0]["master_out_point"]["tx_hash"], hex_prefixed(&LIMIT_ORDER_WRONG_MASTER_TX_HASH));
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_WRONG_MASTER_TX_HASH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_wrong_master_index_both_reject() {
    let execution = limit_order_udt_to_ckb_wrong_master_index_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_wrong_master_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_wrong_master_index");
    assert_eq!(fixture["outputs"][0]["master_out_point"]["index"], 1);
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_WRONG_MASTER_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_output_mint_action_both_reject() {
    let execution = limit_order_udt_to_ckb_output_mint_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_output_mint_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_output_mint_action");
    assert_eq!(fixture["outputs"][0]["order_action"], "Mint");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_MINT_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_output_invalid_action_both_reject() {
    let execution = limit_order_udt_to_ckb_output_invalid_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_output_invalid_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_output_invalid_action");
    assert_eq!(fixture["outputs"][0]["order_action"], "Invalid");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_INVALID_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_output_short_action_both_reject() {
    let execution = limit_order_udt_to_ckb_output_short_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_output_short_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_output_short_action");
    assert_eq!(fixture["outputs"][0]["data"].as_str().expect("output data hex").len(), 2 + 16 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_SHORT_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_output_short_master_both_reject() {
    let execution = limit_order_udt_to_ckb_output_short_master_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_output_short_master_out_point");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_output_short_master_out_point");
    assert_eq!(fixture["outputs"][0]["order_action"], "Match");
    assert_eq!(fixture["outputs"][0]["data"].as_str().expect("output data hex").len(), 2 + 28 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_SHORT_MASTER_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_output_long_data_both_reject() {
    let execution = limit_order_udt_to_ckb_output_long_data_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_output_long_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_output_long_data");
    assert_eq!(fixture["outputs"][0]["order_action"], "Match");
    let data = fixture["outputs"][0]["data"].as_str().expect("output data hex");
    assert_eq!(data.len(), 2 + 90 * 2);
    assert!(data.ends_with("99"));
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_LONG_DATA_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_input_invalid_action_both_reject() {
    let execution = limit_order_udt_to_ckb_input_invalid_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_input_invalid_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_input_invalid_action");
    assert_eq!(fixture["inputs"][0]["order_action"], "Invalid");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INPUT_INVALID_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_input_short_action_both_reject() {
    let execution = limit_order_udt_to_ckb_input_short_action_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_input_short_action");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_input_short_action");
    assert_eq!(fixture["inputs"][0]["data"].as_str().expect("input data hex").len(), 2 + 16 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INPUT_SHORT_ACTION_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_input_short_master_both_reject() {
    let execution = limit_order_udt_to_ckb_input_short_master_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_input_short_master_out_point");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_input_short_master_out_point");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert_eq!(fixture["inputs"][0]["data"].as_str().expect("input data hex").len(), 2 + 28 * 2);
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INPUT_SHORT_MASTER_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_input_long_data_both_reject() {
    let execution = limit_order_udt_to_ckb_input_long_data_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_input_long_data");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_input_long_data");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    let data = fixture["inputs"][0]["data"].as_str().expect("input data hex");
    assert_eq!(data.len(), 2 + 90 * 2);
    assert!(data.ends_with("99"));
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INPUT_LONG_DATA_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_input_absolute_match_both_accept() {
    let execution = limit_order_udt_to_ckb_input_absolute_match_differential_execution();
    assert_eq!(execution["failure_mode"], Value::Null);
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_input_absolute_match");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INPUT_ABSOLUTE_MATCH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_input_wrong_master_tx_hash_both_reject() {
    let execution = limit_order_udt_to_ckb_input_wrong_master_tx_hash_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_input_wrong_master_tx_hash");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_input_wrong_master_tx_hash");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert!(fixture["inputs"][0]["data"].as_str().expect("input data").contains("787878"));
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INPUT_WRONG_MASTER_TX_HASH_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_input_wrong_master_index_both_reject() {
    let execution = limit_order_udt_to_ckb_input_wrong_master_index_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_input_wrong_master_index");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_input_wrong_master_index");
    assert_eq!(fixture["inputs"][0]["order_action"], "Match");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_INPUT_WRONG_MASTER_INDEX_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_missing_matching_output_both_reject() {
    let execution = limit_order_udt_to_ckb_missing_matching_output_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_missing_matching_output");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_missing_matching_output");
    assert_eq!(fixture["script_under_test_roles"].as_array().expect("roles").len(), 1);
    assert_eq!(fixture["outputs"][0]["lock"], "always_success");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_MISSING_MATCHING_OUTPUT_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_duplicate_matching_output_both_reject() {
    let execution = limit_order_udt_to_ckb_duplicate_matching_output_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_duplicate_matching_output");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_duplicate_matching_output");
    assert_eq!(fixture["outputs"].as_array().expect("outputs").len(), 2);
    assert_eq!(fixture["outputs"][0]["lock"], "script_under_test");
    assert_eq!(fixture["outputs"][1]["lock"], "script_under_test");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_MATCHING_OUTPUT_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_missing_input_type_both_reject() {
    let execution = limit_order_udt_to_ckb_missing_input_type_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_missing_input_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_missing_input_type");
    assert_eq!(fixture["inputs"][0]["type"], Value::Null);
    assert_eq!(fixture["outputs"][0]["type"], "auxiliary_udt_type");
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_MISSING_INPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_limit_order_udt_to_ckb_missing_output_type_both_reject() {
    let execution = limit_order_udt_to_ckb_missing_output_type_differential_execution();
    assert_eq!(execution["failure_mode"], "limit_order_udt_to_ckb_missing_output_type");
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    let fixture = &execution["normalized_fixture"];
    assert_eq!(fixture["scenario"], "limit_order_udt_to_ckb_missing_output_type");
    assert_eq!(fixture["inputs"][0]["type"], "auxiliary_udt_type");
    assert_eq!(fixture["outputs"][0]["type"], Value::Null);
    assert_matrix_execution_matches(LIMIT_ORDER_UDT_TO_CKB_MISSING_OUTPUT_TYPE_DIFF_SCENARIO, &execution);
}

fn differential_valid_owned_owner_both_accept() {
    let execution = owned_owner_valid_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    let fixture = &execution["normalized_fixture"];
    for collection in ["inputs", "outputs"] {
        for cell in fixture[collection].as_array().expect(collection) {
            assert!(cell.get("owner").is_none(), "executable Owned-Owner fixture must not invent model owner field");
            assert!(cell.get("claimed_owner").is_none(), "executable Owned-Owner fixture must not invent model claimed_owner field");
        }
    }
    let owner_cell = fixture["inputs"]
        .as_array()
        .expect("owned-owner inputs")
        .iter()
        .find(|cell| cell["role"] == "owner_cell")
        .expect("owner cell");
    assert_eq!(owner_cell["data"], "0x01000000");
    assert_eq!(owner_cell["owner_relative_distance_i32"].as_i64(), Some(i64::from(OWNED_OWNER_VALID_DISTANCE)));
    assert_matrix_execution_matches(OWNED_OWNER_VALID_DIFF_SCENARIO, &execution);
}

fn differential_valid_owned_owner_output_pair_both_accept() {
    let execution = owned_owner_output_valid_differential_execution();
    assert_eq!(execution["original_ickb_status"], "pass");
    assert_eq!(execution["cellscript_status"], "pass");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_VALID_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_relative_mismatch_both_reject() {
    let execution = owned_owner_output_relative_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_RELATIVE_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_duplicate_owner_both_reject() {
    let execution = owned_owner_output_duplicate_owner_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_DUPLICATE_OWNER_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_missing_owner_both_reject() {
    let execution = owned_owner_output_missing_owner_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_MISSING_OWNER_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_missing_owned_both_reject() {
    let execution = owned_owner_output_missing_owned_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_MISSING_OWNED_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_script_misuse_both_reject() {
    let execution = owned_owner_output_script_misuse_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_SCRIPT_MISUSE_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_not_withdrawal_both_reject() {
    let execution = owned_owner_output_not_withdrawal_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_NOT_WITHDRAWAL_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_owner_data_length_mismatch_both_reject() {
    let execution = owned_owner_output_owner_data_length_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_OWNER_DATA_LENGTH_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_related_type_hash_mismatch_both_reject() {
    let execution = owned_owner_output_related_type_hash_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_RELATED_TYPE_HASH_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_output_related_data_rule_mismatch_both_reject() {
    let execution = owned_owner_output_related_data_rule_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OUTPUT_RELATED_DATA_RULE_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_related_type_hash_mismatch_both_reject() {
    let execution = owned_owner_related_type_hash_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_RELATED_TYPE_HASH_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_related_data_rule_mismatch_both_reject() {
    let execution = owned_owner_related_data_rule_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_RELATED_DATA_RULE_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_owner_data_length_mismatch_both_reject() {
    let execution = owned_owner_owner_data_length_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_OWNER_DATA_LENGTH_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_relative_mismatch_both_reject() {
    let execution = owned_owner_relative_mismatch_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_RELATIVE_MISMATCH_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_script_misuse_both_reject() {
    let execution = owned_owner_script_misuse_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_SCRIPT_MISUSE_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_not_withdrawal_both_reject() {
    let execution = owned_owner_not_withdrawal_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_NOT_WITHDRAWAL_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_missing_owner_both_reject() {
    let execution = owned_owner_missing_owner_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_MISSING_OWNER_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_missing_owned_both_reject() {
    let execution = owned_owner_missing_owned_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_MISSING_OWNED_DIFF_SCENARIO, &execution);
}

fn differential_owned_owner_duplicate_owner_both_reject() {
    let execution = owned_owner_duplicate_owner_differential_execution();
    assert_eq!(execution["original_ickb_status"], "fail");
    assert_eq!(execution["cellscript_status"], "fail");
    assert_matrix_execution_matches(OWNED_OWNER_DUPLICATE_OWNER_DIFF_SCENARIO, &execution);
}
}
