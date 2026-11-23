fn dao_withdrawal_differential_execution(input_since: u64, output_capacity: u64, failure_mode: Option<&str>) -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        input_since,
        output_capacity,
        failure_mode,
        DaoWithdrawalHeaderDepMode::Present,
        DAO_WITHDRAWAL_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_missing_withdraw_header_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_missing_withdraw_header"),
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader,
        DAO_WITHDRAWAL_HEADER_LINEAGE_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_HEADER_LINEAGE_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_max_capacity_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY,
        None,
        DaoWithdrawalHeaderDepMode::Present,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_two_input_withdrawal_max_capacity_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        None,
        DaoTwoInputWithdrawalMode::SameDeposit,
    )
}

fn dao_two_input_withdrawal_over_capacity_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_OVER_OUTPUT_CAPACITY,
        Some("dao_two_input_over_withdraw_capacity"),
        DaoTwoInputWithdrawalMode::SameDeposit,
    )
}

fn dao_two_input_mixed_deposit_rate_max_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoTwoInputWithdrawalMode::MixedDeposit,
    )
}

fn dao_two_input_mixed_deposit_rate_over_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_two_input_mixed_deposit_rate_over_withdraw_capacity"),
        DaoTwoInputWithdrawalMode::MixedDeposit,
    )
}

fn dao_two_input_mixed_withdraw_rate_max_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoTwoInputWithdrawalMode::MixedWithdraw,
    )
}

fn dao_two_input_mixed_withdraw_rate_over_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_two_input_mixed_withdraw_rate_over_withdraw_capacity"),
        DaoTwoInputWithdrawalMode::MixedWithdraw,
    )
}

fn dao_two_input_mixed_both_rate_max_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoTwoInputWithdrawalMode::MixedBoth,
    )
}

fn dao_two_input_mixed_both_rate_over_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_two_input_mixed_both_rate_over_withdraw_capacity"),
        DaoTwoInputWithdrawalMode::MixedBoth,
    )
}

fn dao_two_input_second_missing_witness_input_type_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_missing_witness_input_type"),
        DaoTwoInputWithdrawalMode::SecondWitnessMissing,
    )
}

fn dao_two_input_second_empty_witness_input_type_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_empty_witness_input_type"),
        DaoTwoInputWithdrawalMode::SecondWitnessEmpty,
    )
}

fn dao_two_input_second_short_witness_input_type_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_short_witness_input_type"),
        DaoTwoInputWithdrawalMode::SecondWitnessShort,
    )
}

fn dao_two_input_second_long_witness_input_type_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_long_witness_input_type"),
        DaoTwoInputWithdrawalMode::SecondWitnessLong,
    )
}

fn dao_two_input_second_withdraw_header_witness_index_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_withdraw_header_witness_index"),
        DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex,
    )
}

fn dao_two_input_second_oob_witness_index_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_oob_witness_index"),
        DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex,
    )
}

fn dao_two_input_second_deposit_data_input_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_deposit_data_input"),
        DaoTwoInputWithdrawalMode::SecondDepositDataInput,
    )
}

fn dao_two_input_second_malformed_input_data_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_malformed_input_data"),
        DaoTwoInputWithdrawalMode::SecondMalformedInputData,
    )
}

fn dao_two_input_second_long_input_data_differential_execution() -> Value {
    dao_two_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_two_input_second_long_input_data"),
        DaoTwoInputWithdrawalMode::SecondLongInputData,
    )
}

fn dao_three_input_withdrawal_max_capacity_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::SameDeposit,
    )
}

fn dao_three_input_withdrawal_over_capacity_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::SameDeposit,
    )
}

fn dao_three_input_mixed_deposit_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedDepositThird,
    )
}

fn dao_three_input_mixed_deposit_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_mixed_deposit_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedDepositThird,
    )
}

fn dao_three_input_mixed_withdraw_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedWithdrawThird,
    )
}

fn dao_three_input_mixed_withdraw_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_mixed_withdraw_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedWithdrawThird,
    )
}

fn dao_three_input_mixed_both_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedBothThird,
    )
}

fn dao_three_input_mixed_both_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_mixed_both_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedBothThird,
    )
}

fn dao_three_input_second_mixed_deposit_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedDepositSecond,
    )
}

fn dao_three_input_second_mixed_deposit_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_second_mixed_deposit_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedDepositSecond,
    )
}

fn dao_three_input_second_mixed_withdraw_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedWithdrawSecond,
    )
}

fn dao_three_input_second_mixed_withdraw_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_second_mixed_withdraw_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedWithdrawSecond,
    )
}

fn dao_three_input_second_mixed_both_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedBothSecond,
    )
}

fn dao_three_input_second_mixed_both_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_second_mixed_both_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedBothSecond,
    )
}

fn dao_three_input_second_deposit_third_withdraw_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird,
    )
}

fn dao_three_input_second_deposit_third_withdraw_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_second_deposit_third_withdraw_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird,
    )
}

fn dao_three_input_second_withdraw_third_deposit_rate_max_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird,
    )
}

fn dao_three_input_second_withdraw_third_deposit_rate_over_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_three_input_second_withdraw_third_deposit_rate_over_withdraw_capacity"),
        DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird,
    )
}

fn dao_three_input_second_missing_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_missing_witness_input_type"),
        DaoThreeInputWithdrawalMode::SecondWitnessMissing,
    )
}

fn dao_three_input_second_empty_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_empty_witness_input_type"),
        DaoThreeInputWithdrawalMode::SecondWitnessEmpty,
    )
}

fn dao_three_input_second_short_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_short_witness_input_type"),
        DaoThreeInputWithdrawalMode::SecondWitnessShort,
    )
}

fn dao_three_input_second_long_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_long_witness_input_type"),
        DaoThreeInputWithdrawalMode::SecondWitnessLong,
    )
}

fn dao_three_input_second_withdraw_header_witness_index_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_withdraw_header_witness_index"),
        DaoThreeInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex,
    )
}

fn dao_three_input_second_oob_witness_index_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_oob_witness_index"),
        DaoThreeInputWithdrawalMode::SecondWitnessOutOfBoundsIndex,
    )
}

fn dao_three_input_second_deposit_data_input_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_deposit_data_input"),
        DaoThreeInputWithdrawalMode::SecondDepositDataInput,
    )
}

fn dao_three_input_second_malformed_input_data_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_malformed_input_data"),
        DaoThreeInputWithdrawalMode::SecondMalformedInputData,
    )
}

fn dao_three_input_second_long_input_data_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_second_long_input_data"),
        DaoThreeInputWithdrawalMode::SecondLongInputData,
    )
}

fn dao_three_input_third_missing_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_missing_witness_input_type"),
        DaoThreeInputWithdrawalMode::ThirdWitnessMissing,
    )
}

fn dao_three_input_third_empty_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_empty_witness_input_type"),
        DaoThreeInputWithdrawalMode::ThirdWitnessEmpty,
    )
}

fn dao_three_input_third_short_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_short_witness_input_type"),
        DaoThreeInputWithdrawalMode::ThirdWitnessShort,
    )
}

fn dao_three_input_third_long_witness_input_type_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_long_witness_input_type"),
        DaoThreeInputWithdrawalMode::ThirdWitnessLong,
    )
}

fn dao_three_input_third_withdraw_header_witness_index_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_withdraw_header_witness_index"),
        DaoThreeInputWithdrawalMode::ThirdWitnessWithdrawHeaderIndex,
    )
}

fn dao_three_input_third_oob_witness_index_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_oob_witness_index"),
        DaoThreeInputWithdrawalMode::ThirdWitnessOutOfBoundsIndex,
    )
}

fn dao_three_input_third_deposit_data_input_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_deposit_data_input"),
        DaoThreeInputWithdrawalMode::ThirdDepositDataInput,
    )
}

fn dao_three_input_third_malformed_input_data_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_malformed_input_data"),
        DaoThreeInputWithdrawalMode::ThirdMalformedInputData,
    )
}

fn dao_three_input_third_long_input_data_differential_execution() -> Value {
    dao_three_input_withdrawal_differential_execution(
        ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        Some("dao_three_input_third_long_input_data"),
        DaoThreeInputWithdrawalMode::ThirdLongInputData,
    )
}

fn dao_two_input_withdrawal_differential_execution(
    output_capacity: u64,
    failure_mode: Option<&str>,
    mode: DaoTwoInputWithdrawalMode,
) -> Value {
    let original_dao_elf = load_original_ickb_binary("dao");
    let original_dao_binary_sha256 = sha256_prefixed(&original_dao_elf);
    let original = run_original_dao_two_input_withdrawal(output_capacity, mode);
    let (cellscript, cellscript_elf) = run_cellscript_dao_two_input_withdrawal(output_capacity, mode);

    assert_eq!(
        original.status, cellscript.status,
        "DAO two-input withdrawal differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original DAO two-input withdrawal status");
    assert_eq!(cellscript.status, expected_status, "CellScript DAO two-input withdrawal status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_dao_two_input_withdrawal_fixture(output_capacity, failure_mode, mode);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_dao_binary_sha256,
        "original_dao_binary_sha256": original_dao_binary_sha256,
        "original_ickb_binary_patched": false,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "ckb_vm_or_testtool_version": CKB_TESTTOOL_VERSION,
        "original_ickb_exit_code": original.exit_code,
        "cellscript_exit_code": cellscript.exit_code,
        "original_ickb_status": original.status,
        "cellscript_status": cellscript.status,
        "statuses_match": true,
        "original_cycles": original.cycles,
        "cellscript_cycles": cellscript.cycles,
        "tx_size_bytes": original.tx_size_bytes,
        "tx_size_bytes_by_side": {
            "original": original.tx_size_bytes,
            "cellscript": cellscript.tx_size_bytes
        },
        "occupied_capacity_shannons": original.occupied_capacity_shannons,
        "fee_shannons": original.fee_shannons,
        "failure_mode": failure_mode,
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn dao_three_input_withdrawal_differential_execution(
    output_capacity: u64,
    failure_mode: Option<&str>,
    mode: DaoThreeInputWithdrawalMode,
) -> Value {
    let original_dao_elf = load_original_ickb_binary("dao");
    let original_dao_binary_sha256 = sha256_prefixed(&original_dao_elf);
    let original = run_original_dao_three_input_withdrawal(output_capacity, mode);
    let (cellscript, cellscript_elf) = run_cellscript_dao_three_input_withdrawal(output_capacity, mode);

    assert_eq!(
        original.status, cellscript.status,
        "DAO three-input withdrawal differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original DAO three-input withdrawal status");
    assert_eq!(cellscript.status, expected_status, "CellScript DAO three-input withdrawal status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_dao_three_input_withdrawal_fixture(output_capacity, failure_mode, mode);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_dao_binary_sha256,
        "original_dao_binary_sha256": original_dao_binary_sha256,
        "original_ickb_binary_patched": false,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "ckb_vm_or_testtool_version": CKB_TESTTOOL_VERSION,
        "original_ickb_exit_code": original.exit_code,
        "cellscript_exit_code": cellscript.exit_code,
        "original_ickb_status": original.status,
        "cellscript_status": cellscript.status,
        "statuses_match": true,
        "original_cycles": original.cycles,
        "cellscript_cycles": cellscript.cycles,
        "tx_size_bytes": original.tx_size_bytes,
        "tx_size_bytes_by_side": {
            "original": original.tx_size_bytes,
            "cellscript": cellscript.tx_size_bytes
        },
        "occupied_capacity_shannons": original.occupied_capacity_shannons,
        "fee_shannons": original.fee_shannons,
        "failure_mode": failure_mode,
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn dao_withdrawal_wrong_deposit_rate_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY,
        Some("dao_wrong_deposit_accumulated_rate"),
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_deposit_rate_adjusted_max_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_deposit_rate_adjusted_over_capacity_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_deposit_rate_adjusted_over_withdraw_capacity"),
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_wrong_withdraw_rate_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY,
        Some("dao_wrong_withdraw_accumulated_rate"),
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_withdraw_rate_adjusted_max_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY,
        None,
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_withdraw_rate_adjusted_over_capacity_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_OVER_OUTPUT_CAPACITY,
        Some("dao_withdraw_rate_adjusted_over_withdraw_capacity"),
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_over_capacity_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OVER_OUTPUT_CAPACITY,
        Some("dao_over_withdraw_capacity"),
        DaoWithdrawalHeaderDepMode::Present,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_wrong_deposit_header_index_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_wrong_deposit_header_index"),
        DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex,
        DAO_WITHDRAWAL_DEPOSIT_HEADER_WITNESS_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_DEPOSIT_HEADER_WITNESS_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_wrong_withdraw_committed_header_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_wrong_withdraw_committed_header"),
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader,
        DAO_WITHDRAWAL_HEADER_LINEAGE_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_HEADER_LINEAGE_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_missing_deposit_header_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_missing_deposit_header"),
        DaoWithdrawalHeaderDepMode::MissingDepositHeader,
        DAO_WITHDRAWAL_DEPOSIT_HEADER_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_DEPOSIT_HEADER_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_deposit_header_index_oob_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_deposit_header_index_out_of_bounds"),
        DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds,
        DAO_WITHDRAWAL_DEPOSIT_HEADER_OOB_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_DEPOSIT_HEADER_OOB_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_deposit_data_input_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_withdrawal_deposit_data_input"),
        DaoWithdrawalHeaderDepMode::DepositDataInput,
        DAO_WITHDRAWAL_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_malformed_input_data_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_withdrawal_malformed_input_data"),
        DaoWithdrawalHeaderDepMode::MalformedInputData,
        DAO_WITHDRAWAL_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_long_input_data_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_withdrawal_long_input_data"),
        DaoWithdrawalHeaderDepMode::LongInputData,
        DAO_WITHDRAWAL_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_missing_witness_input_type_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_missing_witness_input_type"),
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_empty_witness_input_type_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_empty_witness_input_type"),
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_short_witness_input_type_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_short_witness_input_type"),
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_WIDTH_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_WIDTH_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_long_witness_input_type_differential_execution() -> Value {
    dao_withdrawal_differential_execution_with_cellscript_probe(
        ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY,
        Some("dao_long_witness_input_type"),
        DaoWithdrawalHeaderDepMode::LongWitnessInputType,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_WIDTH_CELLSCRIPT_PROGRAM,
        DAO_WITHDRAWAL_WITNESS_INPUT_TYPE_WIDTH_CELLSCRIPT_ACTION,
    )
}

fn dao_withdrawal_differential_execution_with_cellscript_probe(
    input_since: u64,
    output_capacity: u64,
    failure_mode: Option<&str>,
    header_dep_mode: DaoWithdrawalHeaderDepMode,
    cellscript_program: &str,
    cellscript_action: &str,
) -> Value {
    let original_dao_elf = load_original_ickb_binary("dao");
    let original_dao_binary_sha256 = sha256_prefixed(&original_dao_elf);
    let original = run_original_dao_withdrawal_with_header_dep_mode(input_since, output_capacity, header_dep_mode);
    let (cellscript, cellscript_elf) = run_cellscript_dao_withdrawal_with_program(
        input_since,
        output_capacity,
        header_dep_mode,
        cellscript_program,
        cellscript_action,
    );

    assert_eq!(
        original.status, cellscript.status,
        "DAO withdrawal differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original DAO withdrawal status");
    assert_eq!(cellscript.status, expected_status, "CellScript DAO withdrawal status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture =
        normalized_dao_withdrawal_fixture_with_header_dep_mode(input_since, output_capacity, failure_mode, header_dep_mode);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_dao_binary_sha256,
        "original_dao_binary_sha256": original_dao_binary_sha256,
        "original_ickb_binary_patched": false,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "ckb_vm_or_testtool_version": CKB_TESTTOOL_VERSION,
        "original_ickb_exit_code": original.exit_code,
        "cellscript_exit_code": cellscript.exit_code,
        "original_ickb_status": original.status,
        "cellscript_status": cellscript.status,
        "statuses_match": true,
        "original_cycles": original.cycles,
        "cellscript_cycles": cellscript.cycles,
        "tx_size_bytes": original.tx_size_bytes,
        "tx_size_bytes_by_side": {
            "original": original.tx_size_bytes,
            "cellscript": cellscript.tx_size_bytes
        },
        "occupied_capacity_shannons": original.occupied_capacity_shannons,
        "fee_shannons": original.fee_shannons,
        "failure_mode": failure_mode,
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}
