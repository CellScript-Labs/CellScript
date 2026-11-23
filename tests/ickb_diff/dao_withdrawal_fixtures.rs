fn normalized_dao_withdrawal_fixture_with_header_dep_mode(
    input_since: u64,
    output_capacity: u64,
    failure_mode: Option<&str>,
    header_dep_mode: DaoWithdrawalHeaderDepMode,
) -> Value {
    let withdrawal_data = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::DepositDataInput => Bytes::from(vec![0u8; 8]),
        DaoWithdrawalHeaderDepMode::MalformedInputData => Bytes::from(vec![0x12, 0x06, 0x00, 0x00]),
        DaoWithdrawalHeaderDepMode::LongInputData => dao_long_withdrawal_request_cell_data(),
        _ => Bytes::from(ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK.to_le_bytes().to_vec()),
    };
    let fixture_rate_maximum_capacity = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY,
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => {
            ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY
        }
        _ => ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY,
    };
    let fixture_rate_adjusted_max = failure_mode.is_none()
        && fixture_rate_maximum_capacity != ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY
        && output_capacity == fixture_rate_maximum_capacity;
    let scenario = match failure_mode {
        Some("dao_missing_withdraw_header") => "dao_missing_withdraw_header",
        Some("dao_missing_deposit_header") => "dao_missing_deposit_header",
        Some("dao_deposit_header_index_out_of_bounds") => "dao_deposit_header_index_out_of_bounds",
        Some("dao_over_withdraw_capacity") => "dao_over_withdraw_capacity",
        Some("dao_deposit_rate_adjusted_over_withdraw_capacity") => "dao_deposit_rate_adjusted_over_withdraw_capacity",
        Some("dao_wrong_deposit_accumulated_rate") => "dao_wrong_deposit_accumulated_rate",
        Some("dao_withdraw_rate_adjusted_over_withdraw_capacity") => "dao_withdraw_rate_adjusted_over_withdraw_capacity",
        Some("dao_wrong_withdraw_accumulated_rate") => "dao_wrong_withdraw_accumulated_rate",
        Some("dao_withdrawal_deposit_data_input") => "dao_withdrawal_deposit_data_input",
        Some("dao_withdrawal_malformed_input_data") => "dao_withdrawal_malformed_input_data",
        Some("dao_withdrawal_long_input_data") => "dao_withdrawal_long_input_data",
        Some("dao_missing_witness_input_type") => "dao_missing_witness_input_type",
        Some("dao_empty_witness_input_type") => "dao_empty_witness_input_type",
        Some("dao_short_witness_input_type") => "dao_short_witness_input_type",
        Some("dao_long_witness_input_type") => "dao_long_witness_input_type",
        Some("dao_wrong_deposit_header_index") => "dao_wrong_deposit_header_index",
        Some("dao_wrong_withdraw_committed_header") => "dao_wrong_withdraw_committed_header",
        Some(_) => "dao_immature_withdrawal",
        None if header_dep_mode == DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate && fixture_rate_adjusted_max => {
            "dao_deposit_rate_adjusted_max_withdrawal_capacity"
        }
        None if header_dep_mode == DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate && fixture_rate_adjusted_max => {
            "dao_withdraw_rate_adjusted_max_withdrawal_capacity"
        }
        None if output_capacity == ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY => "dao_max_withdrawal_capacity",
        None => "dao_mature_withdrawal",
    };
    let script_under_test_difference = match failure_mode {
        Some("dao_missing_withdraw_header") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated since-plus-input-header probe"
        }
        Some("dao_missing_deposit_header") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated since-plus-withdraw-header-plus-deposit-header probe"
        }
        Some("dao_deposit_header_index_out_of_bounds") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated since-plus-out-of-bounds-deposit-header probe"
        }
        Some("dao_over_withdraw_capacity") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated DAO occupied-capacity plus rate-compensation upper-bound probe"
        }
        Some("dao_deposit_rate_adjusted_over_withdraw_capacity") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated DAO occupied-capacity plus rate-compensation upper-bound probe"
        }
        Some("dao_wrong_deposit_accumulated_rate") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated DAO occupied-capacity plus rate-compensation upper-bound probe"
        }
        Some("dao_withdraw_rate_adjusted_over_withdraw_capacity") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated DAO occupied-capacity plus rate-compensation upper-bound probe"
        }
        Some("dao_wrong_withdraw_accumulated_rate") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated DAO occupied-capacity plus rate-compensation upper-bound probe"
        }
        None
            if matches!(
                header_dep_mode,
                DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate | DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate
            ) && fixture_rate_adjusted_max =>
        {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated DAO occupied-capacity plus rate-compensation upper-bound probe"
        }
        Some("dao_withdrawal_deposit_data_input") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated withdrawal-data classifier probe"
        }
        Some("dao_withdrawal_malformed_input_data") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated malformed withdrawal-data classifier probe"
        }
        Some("dao_withdrawal_long_input_data") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated long withdrawal-data classifier probe"
        }
        Some("dao_missing_witness_input_type") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated WitnessArgs input_type presence probe"
        }
        Some("dao_empty_witness_input_type") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated WitnessArgs input_type non-empty probe"
        }
        Some("dao_short_witness_input_type") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated WitnessArgs input_type width probe"
        }
        Some("dao_long_witness_input_type") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated WitnessArgs input_type exact-width probe"
        }
        Some("dao_wrong_deposit_header_index") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated since-plus-deposit-header-witness probe"
        }
        Some("dao_wrong_withdraw_committed_header") => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated since-plus-input-header probe"
        }
        None if output_capacity == ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated DAO occupied-capacity plus rate-compensation upper-bound probe"
        }
        _ => "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated since-maturity probe",
    };
    let deposit_accumulated_rate = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
        _ => ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
    };
    let withdraw_accumulated_rate = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
        _ => ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
    };
    let mut output = json!({
        "index": 0,
        "role": "withdrawn_capacity_cell",
        "capacity_shannons": output_capacity,
        "lock": "always_success",
        "type": null,
        "data": "0x"
    });
    if failure_mode == Some("dao_over_withdraw_capacity") {
        output["expected_maximum_capacity_shannons"] = json!(ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY);
        output["overdrawn_by_shannons"] = json!(output_capacity - ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY);
    } else if failure_mode == Some("dao_deposit_rate_adjusted_over_withdraw_capacity")
        || failure_mode == Some("dao_withdraw_rate_adjusted_over_withdraw_capacity")
    {
        output["expected_maximum_capacity_shannons_under_fixture_rate"] = json!(fixture_rate_maximum_capacity);
        output["overdrawn_by_shannons_under_fixture_rate"] = json!(output_capacity - fixture_rate_maximum_capacity);
        output["capacity_boundary"] = json!("fixture_rate_plus_one");
    } else if fixture_rate_adjusted_max {
        output["expected_maximum_capacity_shannons_under_fixture_rate"] = json!(fixture_rate_maximum_capacity);
        output["capacity_boundary"] = json!("fixture_rate_exact_maximum");
    } else if failure_mode.is_none() && output_capacity == ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY {
        output["expected_maximum_capacity_shannons"] = json!(ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY);
        output["capacity_boundary"] = json!("exact_maximum");
    } else if failure_mode == Some("dao_wrong_deposit_accumulated_rate") {
        output["expected_maximum_capacity_shannons_under_fixture_rate"] =
            json!(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY);
        output["overdrawn_by_shannons_under_fixture_rate"] =
            json!(output_capacity - ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_RATE_MAX_OUTPUT_CAPACITY);
    } else if failure_mode == Some("dao_wrong_withdraw_accumulated_rate") {
        output["expected_maximum_capacity_shannons_under_fixture_rate"] =
            json!(ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY);
        output["overdrawn_by_shannons_under_fixture_rate"] =
            json!(output_capacity - ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY);
    }
    if failure_mode == Some("dao_over_withdraw_capacity")
        || failure_mode == Some("dao_deposit_rate_adjusted_over_withdraw_capacity")
        || failure_mode == Some("dao_wrong_deposit_accumulated_rate")
        || failure_mode == Some("dao_withdraw_rate_adjusted_over_withdraw_capacity")
        || failure_mode == Some("dao_wrong_withdraw_accumulated_rate")
        || fixture_rate_adjusted_max
        || (failure_mode.is_none() && output_capacity == ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY)
    {
        output["dao_capacity_compensation"] = json!({
            "formula": "occupied_capacity + ((input_capacity - occupied_capacity) * withdraw_rate / deposit_rate)",
            "input_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
            "input_occupied_capacity_shannons": ORIGINAL_DAO_WITHDRAW_INPUT_OCCUPIED_CAPACITY,
            "withdrawable_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAWABLE_CAPACITY,
            "withdraw_accumulated_rate": withdraw_accumulated_rate,
            "deposit_accumulated_rate": deposit_accumulated_rate,
            "expected_correct_rate_maximum_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY,
            "expected_fixture_rate_maximum_capacity_shannons": fixture_rate_maximum_capacity
        });
    }
    let withdraw_header_role = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => "withdraw_header_wrong_accumulated_rate",
        _ => "withdraw_header",
    };
    let deposit_header_role = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => "deposit_header_wrong_accumulated_rate",
        _ => "deposit_header",
    };
    let header_deps = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::Present
        | DaoWithdrawalHeaderDepMode::DepositDataInput
        | DaoWithdrawalHeaderDepMode::MalformedInputData
        | DaoWithdrawalHeaderDepMode::LongInputData
        | DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds
        | DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate
        | DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate
        | DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex
        | DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader
        | DaoWithdrawalHeaderDepMode::MissingWitnessInputType
        | DaoWithdrawalHeaderDepMode::EmptyWitnessInputType
        | DaoWithdrawalHeaderDepMode::ShortWitnessInputType
        | DaoWithdrawalHeaderDepMode::LongWitnessInputType => json!([
            {
                "index": 0,
                "role": withdraw_header_role,
                "block_number": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
                "accumulated_rate": withdraw_accumulated_rate,
                "epoch": {
                    "number": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                    "index": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                    "length": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH
                }
            },
            {
                "index": 1,
                "role": deposit_header_role,
                "block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
                "accumulated_rate": deposit_accumulated_rate,
                "epoch": {
                    "number": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
                    "index": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
                    "length": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH
                }
            }
        ]),
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader => json!([
            {
                "index": 0,
                "role": "deposit_header",
                "block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
                "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
                "epoch": {
                    "number": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
                    "index": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
                    "length": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH
                }
            }
        ]),
        DaoWithdrawalHeaderDepMode::MissingDepositHeader => json!([
            {
                "index": 0,
                "role": "withdraw_header",
                "block_number": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
                "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
                "epoch": {
                    "number": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                    "index": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                    "length": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH
                }
            }
        ]),
    };
    let witness_header_dep_index = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::Present => 1u64,
        DaoWithdrawalHeaderDepMode::DepositDataInput => 1u64,
        DaoWithdrawalHeaderDepMode::MalformedInputData => 1u64,
        DaoWithdrawalHeaderDepMode::LongInputData => 1u64,
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader => 0u64,
        DaoWithdrawalHeaderDepMode::MissingDepositHeader => 1u64,
        DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds => 2u64,
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => 1u64,
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => 1u64,
        DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex => 0u64,
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => 1u64,
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::LongWitnessInputType => 1u64,
    };
    let witness = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType => json!({
            "index": 0,
            "input_type_present": false
        }),
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType => json!({
            "index": 0,
            "input_type_present": true,
            "input_type_bytes": "0x",
            "input_type_length_bytes": 0
        }),
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType => json!({
            "index": 0,
            "input_type_present": true,
            "input_type_bytes": "0x01",
            "input_type_length_bytes": 1,
            "expected_input_type_length_bytes": 8
        }),
        DaoWithdrawalHeaderDepMode::LongWitnessInputType => json!({
            "index": 0,
            "input_type_present": true,
            "input_type_bytes": "0x010000000000000099",
            "input_type_length_bytes": 9,
            "expected_input_type_length_bytes": 8
        }),
        _ => json!({
            "index": 0,
            "input_type_header_dep_index_le_u64": witness_header_dep_index
        }),
    };
    let linked_header = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::Present => "withdraw_header",
        DaoWithdrawalHeaderDepMode::DepositDataInput => "withdraw_header",
        DaoWithdrawalHeaderDepMode::MalformedInputData => "withdraw_header",
        DaoWithdrawalHeaderDepMode::LongInputData => "withdraw_header",
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader => "missing_withdraw_header",
        DaoWithdrawalHeaderDepMode::MissingDepositHeader => "withdraw_header",
        DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds => "withdraw_header",
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => "withdraw_header",
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => "withdraw_header_wrong_accumulated_rate",
        DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex => "withdraw_header",
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => "deposit_header_as_committed_withdraw_header",
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType => "withdraw_header",
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType => "withdraw_header",
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType => "withdraw_header",
        DaoWithdrawalHeaderDepMode::LongWitnessInputType => "withdraw_header",
    };
    let input_role = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::DepositDataInput => "deposit_data_dao_cell_spent_as_withdrawal",
        DaoWithdrawalHeaderDepMode::MalformedInputData => "malformed_data_dao_cell_spent_as_withdrawal",
        DaoWithdrawalHeaderDepMode::LongInputData => "long_data_dao_cell_spent_as_withdrawal",
        _ => "withdrawing_dao_cell",
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": ["input_0_type"],
        "script_under_test_difference": script_under_test_difference,
        "input_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
        "cell_deps": ["script_under_test"],
        "header_deps": header_deps,
        "witnesses": [witness],
        "inputs": [
            {
                "index": 0,
                "role": input_role,
                "capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&withdrawal_data),
                "deposit_block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
                "linked_header": linked_header,
                "since": format!("0x{input_since:016x}"),
                "since_u64": input_since
            }
        ],
        "outputs": [output],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}

fn normalized_dao_two_input_withdrawal_fixture(
    output_capacity: u64,
    failure_mode: Option<&str>,
    mode: DaoTwoInputWithdrawalMode,
) -> Value {
    let first_input_data = dao_two_input_cell_data(mode, 0);
    let second_input_data = dao_two_input_cell_data(mode, 1);
    let second_input_role = match mode {
        DaoTwoInputWithdrawalMode::SecondDepositDataInput => "deposit_data_dao_cell_spent_as_second_withdrawal",
        DaoTwoInputWithdrawalMode::SecondMalformedInputData => "malformed_data_dao_cell_spent_as_second_withdrawal",
        DaoTwoInputWithdrawalMode::SecondLongInputData => "long_data_dao_cell_spent_as_second_withdrawal",
        _ => "withdrawing_dao_cell",
    };
    let expected_maximum_capacity = match mode {
        DaoTwoInputWithdrawalMode::SameDeposit => ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
        DaoTwoInputWithdrawalMode::MixedDeposit => ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY,
        DaoTwoInputWithdrawalMode::MixedWithdraw => ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY,
        DaoTwoInputWithdrawalMode::MixedBoth => ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        DaoTwoInputWithdrawalMode::SecondDepositDataInput
        | DaoTwoInputWithdrawalMode::SecondMalformedInputData
        | DaoTwoInputWithdrawalMode::SecondLongInputData
        | DaoTwoInputWithdrawalMode::SecondWitnessMissing
        | DaoTwoInputWithdrawalMode::SecondWitnessEmpty
        | DaoTwoInputWithdrawalMode::SecondWitnessShort
        | DaoTwoInputWithdrawalMode::SecondWitnessLong
        | DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex
        | DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex => ORIGINAL_DAO_WITHDRAW_PHASE2_TWO_INPUT_MAX_OUTPUT_CAPACITY,
    };
    let scenario = match (mode, failure_mode) {
        (DaoTwoInputWithdrawalMode::SameDeposit, Some("dao_two_input_over_withdraw_capacity")) => {
            "dao_two_input_over_withdraw_capacity"
        }
        (DaoTwoInputWithdrawalMode::MixedDeposit, Some("dao_two_input_mixed_deposit_rate_over_withdraw_capacity")) => {
            "dao_two_input_mixed_deposit_rate_over_withdraw_capacity"
        }
        (DaoTwoInputWithdrawalMode::MixedWithdraw, Some("dao_two_input_mixed_withdraw_rate_over_withdraw_capacity")) => {
            "dao_two_input_mixed_withdraw_rate_over_withdraw_capacity"
        }
        (DaoTwoInputWithdrawalMode::MixedBoth, Some("dao_two_input_mixed_both_rate_over_withdraw_capacity")) => {
            "dao_two_input_mixed_both_rate_over_withdraw_capacity"
        }
        (DaoTwoInputWithdrawalMode::SecondWitnessMissing, Some("dao_two_input_second_missing_witness_input_type")) => {
            "dao_two_input_second_missing_witness_input_type"
        }
        (DaoTwoInputWithdrawalMode::SecondWitnessEmpty, Some("dao_two_input_second_empty_witness_input_type")) => {
            "dao_two_input_second_empty_witness_input_type"
        }
        (DaoTwoInputWithdrawalMode::SecondWitnessShort, Some("dao_two_input_second_short_witness_input_type")) => {
            "dao_two_input_second_short_witness_input_type"
        }
        (DaoTwoInputWithdrawalMode::SecondWitnessLong, Some("dao_two_input_second_long_witness_input_type")) => {
            "dao_two_input_second_long_witness_input_type"
        }
        (DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex, Some("dao_two_input_second_withdraw_header_witness_index")) => {
            "dao_two_input_second_withdraw_header_witness_index"
        }
        (DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex, Some("dao_two_input_second_oob_witness_index")) => {
            "dao_two_input_second_oob_witness_index"
        }
        (DaoTwoInputWithdrawalMode::SecondDepositDataInput, Some("dao_two_input_second_deposit_data_input")) => {
            "dao_two_input_second_deposit_data_input"
        }
        (DaoTwoInputWithdrawalMode::SecondMalformedInputData, Some("dao_two_input_second_malformed_input_data")) => {
            "dao_two_input_second_malformed_input_data"
        }
        (DaoTwoInputWithdrawalMode::SecondLongInputData, Some("dao_two_input_second_long_input_data")) => {
            "dao_two_input_second_long_input_data"
        }
        (DaoTwoInputWithdrawalMode::MixedDeposit, _) => "dao_two_input_mixed_deposit_rate_max_withdrawal_capacity",
        (DaoTwoInputWithdrawalMode::MixedWithdraw, _) => "dao_two_input_mixed_withdraw_rate_max_withdrawal_capacity",
        (DaoTwoInputWithdrawalMode::MixedBoth, _) => "dao_two_input_mixed_both_rate_max_withdrawal_capacity",
        _ => "dao_two_input_max_withdrawal_capacity",
    };
    let capacity_boundary = match (mode, failure_mode) {
        (DaoTwoInputWithdrawalMode::SameDeposit, Some(_)) => "two_input_exact_maximum_plus_one",
        (DaoTwoInputWithdrawalMode::MixedDeposit, Some(_)) => "two_input_mixed_deposit_rate_exact_maximum_plus_one",
        (DaoTwoInputWithdrawalMode::MixedDeposit, None) => "two_input_mixed_deposit_rate_exact_maximum",
        (DaoTwoInputWithdrawalMode::MixedWithdraw, Some(_)) => "two_input_mixed_withdraw_rate_exact_maximum_plus_one",
        (DaoTwoInputWithdrawalMode::MixedWithdraw, None) => "two_input_mixed_withdraw_rate_exact_maximum",
        (DaoTwoInputWithdrawalMode::MixedBoth, Some(_)) => "two_input_mixed_both_rate_exact_maximum_plus_one",
        (DaoTwoInputWithdrawalMode::MixedBoth, None) => "two_input_mixed_both_rate_exact_maximum",
        (
            DaoTwoInputWithdrawalMode::SecondWitnessMissing
            | DaoTwoInputWithdrawalMode::SecondWitnessEmpty
            | DaoTwoInputWithdrawalMode::SecondWitnessShort
            | DaoTwoInputWithdrawalMode::SecondWitnessLong,
            Some(_),
        ) => "two_input_exact_maximum_with_malformed_second_witness",
        (
            DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex | DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex,
            Some(_),
        ) => "two_input_exact_maximum_with_malformed_second_witness_index",
        (
            DaoTwoInputWithdrawalMode::SecondDepositDataInput
            | DaoTwoInputWithdrawalMode::SecondMalformedInputData
            | DaoTwoInputWithdrawalMode::SecondLongInputData,
            Some(_),
        ) => "two_input_exact_maximum_with_non_withdrawal_second_input_data",
        _ => "two_input_exact_maximum",
    };
    let mut header_deps = vec![
        json!({
            "index": 0,
            "role": "withdraw_header",
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH
            }
        }),
        json!({
            "index": 1,
            "role": "deposit_header",
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH
            }
        }),
    ];
    if matches!(mode, DaoTwoInputWithdrawalMode::MixedDeposit | DaoTwoInputWithdrawalMode::MixedBoth) {
        header_deps.push(json!({
            "index": 2,
            "role": "deposit_header_mixed_rate",
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH
            }
        }));
    }
    if matches!(mode, DaoTwoInputWithdrawalMode::MixedWithdraw | DaoTwoInputWithdrawalMode::MixedBoth) {
        header_deps.push(json!({
            "index": header_deps.len(),
            "role": "withdraw_header_mixed_rate",
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH
            }
        }));
    }
    let mut output = json!({
        "index": 0,
        "role": "withdrawn_capacity_cell",
        "capacity_shannons": output_capacity,
        "lock": "always_success",
        "type": null,
        "data": "0x",
        "capacity_boundary": capacity_boundary,
        "expected_maximum_capacity_shannons": expected_maximum_capacity,
        "dao_capacity_compensation": {
            "formula": "sum(occupied_capacity + ((input_capacity - occupied_capacity) * withdraw_rate / deposit_rate))",
            "input_count": 2,
            "per_input_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
            "per_input_occupied_capacity_shannons": ORIGINAL_DAO_WITHDRAW_INPUT_OCCUPIED_CAPACITY,
            "per_input_withdrawable_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAWABLE_CAPACITY,
            "withdraw_accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
            "second_withdraw_accumulated_rate": if matches!(mode, DaoTwoInputWithdrawalMode::MixedWithdraw | DaoTwoInputWithdrawalMode::MixedBoth) {
                ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE
            } else {
                ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE
            },
            "deposit_accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
            "second_deposit_accumulated_rate": if matches!(mode, DaoTwoInputWithdrawalMode::MixedDeposit | DaoTwoInputWithdrawalMode::MixedBoth) {
                ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE
            } else {
                ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE
            },
            "per_input_maximum_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY
        }
    });
    if output_capacity > expected_maximum_capacity {
        output["overdrawn_by_shannons"] = json!(output_capacity - expected_maximum_capacity);
    }
    let script_under_test_difference = match mode {
        DaoTwoInputWithdrawalMode::SecondWitnessMissing
        | DaoTwoInputWithdrawalMode::SecondWitnessEmpty
        | DaoTwoInputWithdrawalMode::SecondWitnessShort
        | DaoTwoInputWithdrawalMode::SecondWitnessLong => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated two-input WitnessArgs input_type width/presence probe"
        }
        DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex
        | DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated two-input WitnessArgs input_type index probe"
        }
        _ => {
            "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated two-input DAO occupied-capacity plus rate-compensation aggregate probe"
        }
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": ["input_0_type", "input_1_type"],
        "script_under_test_difference": script_under_test_difference,
        "input_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY * 2,
        "cell_deps": ["script_under_test"],
        "header_deps": header_deps,
        "witnesses": [
            {
                "index": 0,
                "input_type_header_dep_index_le_u64": 1
            },
            dao_two_input_second_witness_metadata(mode)
        ],
        "inputs": [
            {
                "index": 0,
                "role": "withdrawing_dao_cell",
                "capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&first_input_data),
                "deposit_block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
                "linked_header": "withdraw_header",
                "since": format!("0x{:016x}", ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE),
                "since_u64": ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE
            },
            {
                "index": 1,
                "role": second_input_role,
                "capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&second_input_data),
                "deposit_block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
                "linked_header": if matches!(
                    mode,
                    DaoTwoInputWithdrawalMode::MixedWithdraw | DaoTwoInputWithdrawalMode::MixedBoth
                ) {
                    "withdraw_header_mixed_rate"
                } else {
                    "withdraw_header"
                },
                "since": format!("0x{:016x}", ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE),
                "since_u64": ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE
            }
        ],
        "outputs": [output],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}

fn normalized_dao_three_input_withdrawal_fixture(
    output_capacity: u64,
    failure_mode: Option<&str>,
    mode: DaoThreeInputWithdrawalMode,
) -> Value {
    let expected_maximum_capacity = match mode {
        DaoThreeInputWithdrawalMode::SameDeposit => ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
        DaoThreeInputWithdrawalMode::MixedDepositSecond => {
            ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY
        }
        DaoThreeInputWithdrawalMode::MixedWithdrawSecond => {
            ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY
        }
        DaoThreeInputWithdrawalMode::MixedBothSecond => ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
        | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird => {
            ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY
        }
        DaoThreeInputWithdrawalMode::MixedDepositThird => {
            ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_DEPOSIT_RATE_MAX_OUTPUT_CAPACITY
        }
        DaoThreeInputWithdrawalMode::MixedWithdrawThird => {
            ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_WITHDRAW_RATE_MAX_OUTPUT_CAPACITY
        }
        DaoThreeInputWithdrawalMode::MixedBothThird => ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MIXED_BOTH_RATE_MAX_OUTPUT_CAPACITY,
        DaoThreeInputWithdrawalMode::SecondWitnessMissing
        | DaoThreeInputWithdrawalMode::SecondWitnessEmpty
        | DaoThreeInputWithdrawalMode::SecondWitnessShort
        | DaoThreeInputWithdrawalMode::SecondWitnessLong
        | DaoThreeInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex
        | DaoThreeInputWithdrawalMode::SecondWitnessOutOfBoundsIndex
        | DaoThreeInputWithdrawalMode::SecondDepositDataInput
        | DaoThreeInputWithdrawalMode::SecondMalformedInputData
        | DaoThreeInputWithdrawalMode::SecondLongInputData
        | DaoThreeInputWithdrawalMode::ThirdWitnessMissing
        | DaoThreeInputWithdrawalMode::ThirdWitnessEmpty
        | DaoThreeInputWithdrawalMode::ThirdWitnessShort
        | DaoThreeInputWithdrawalMode::ThirdWitnessLong
        | DaoThreeInputWithdrawalMode::ThirdWitnessWithdrawHeaderIndex
        | DaoThreeInputWithdrawalMode::ThirdWitnessOutOfBoundsIndex
        | DaoThreeInputWithdrawalMode::ThirdDepositDataInput
        | DaoThreeInputWithdrawalMode::ThirdMalformedInputData
        | DaoThreeInputWithdrawalMode::ThirdLongInputData => ORIGINAL_DAO_WITHDRAW_PHASE2_THREE_INPUT_MAX_OUTPUT_CAPACITY,
    };
    let scenario = match (mode, failure_mode) {
        (DaoThreeInputWithdrawalMode::SameDeposit, Some("dao_three_input_over_withdraw_capacity")) => {
            "dao_three_input_over_withdraw_capacity"
        }
        (DaoThreeInputWithdrawalMode::MixedDepositThird, Some("dao_three_input_mixed_deposit_rate_over_withdraw_capacity")) => {
            "dao_three_input_mixed_deposit_rate_over_withdraw_capacity"
        }
        (DaoThreeInputWithdrawalMode::MixedWithdrawThird, Some("dao_three_input_mixed_withdraw_rate_over_withdraw_capacity")) => {
            "dao_three_input_mixed_withdraw_rate_over_withdraw_capacity"
        }
        (DaoThreeInputWithdrawalMode::MixedBothThird, Some("dao_three_input_mixed_both_rate_over_withdraw_capacity")) => {
            "dao_three_input_mixed_both_rate_over_withdraw_capacity"
        }
        (
            DaoThreeInputWithdrawalMode::MixedDepositSecond,
            Some("dao_three_input_second_mixed_deposit_rate_over_withdraw_capacity"),
        ) => "dao_three_input_second_mixed_deposit_rate_over_withdraw_capacity",
        (
            DaoThreeInputWithdrawalMode::MixedWithdrawSecond,
            Some("dao_three_input_second_mixed_withdraw_rate_over_withdraw_capacity"),
        ) => "dao_three_input_second_mixed_withdraw_rate_over_withdraw_capacity",
        (DaoThreeInputWithdrawalMode::MixedBothSecond, Some("dao_three_input_second_mixed_both_rate_over_withdraw_capacity")) => {
            "dao_three_input_second_mixed_both_rate_over_withdraw_capacity"
        }
        (
            DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird,
            Some("dao_three_input_second_deposit_third_withdraw_rate_over_withdraw_capacity"),
        ) => "dao_three_input_second_deposit_third_withdraw_rate_over_withdraw_capacity",
        (
            DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird,
            Some("dao_three_input_second_withdraw_third_deposit_rate_over_withdraw_capacity"),
        ) => "dao_three_input_second_withdraw_third_deposit_rate_over_withdraw_capacity",
        (DaoThreeInputWithdrawalMode::SecondWitnessMissing, Some("dao_three_input_second_missing_witness_input_type")) => {
            "dao_three_input_second_missing_witness_input_type"
        }
        (DaoThreeInputWithdrawalMode::SecondWitnessEmpty, Some("dao_three_input_second_empty_witness_input_type")) => {
            "dao_three_input_second_empty_witness_input_type"
        }
        (DaoThreeInputWithdrawalMode::SecondWitnessShort, Some("dao_three_input_second_short_witness_input_type")) => {
            "dao_three_input_second_short_witness_input_type"
        }
        (DaoThreeInputWithdrawalMode::SecondWitnessLong, Some("dao_three_input_second_long_witness_input_type")) => {
            "dao_three_input_second_long_witness_input_type"
        }
        (
            DaoThreeInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex,
            Some("dao_three_input_second_withdraw_header_witness_index"),
        ) => "dao_three_input_second_withdraw_header_witness_index",
        (DaoThreeInputWithdrawalMode::SecondWitnessOutOfBoundsIndex, Some("dao_three_input_second_oob_witness_index")) => {
            "dao_three_input_second_oob_witness_index"
        }
        (DaoThreeInputWithdrawalMode::SecondDepositDataInput, Some("dao_three_input_second_deposit_data_input")) => {
            "dao_three_input_second_deposit_data_input"
        }
        (DaoThreeInputWithdrawalMode::SecondMalformedInputData, Some("dao_three_input_second_malformed_input_data")) => {
            "dao_three_input_second_malformed_input_data"
        }
        (DaoThreeInputWithdrawalMode::SecondLongInputData, Some("dao_three_input_second_long_input_data")) => {
            "dao_three_input_second_long_input_data"
        }
        (DaoThreeInputWithdrawalMode::ThirdWitnessMissing, Some("dao_three_input_third_missing_witness_input_type")) => {
            "dao_three_input_third_missing_witness_input_type"
        }
        (DaoThreeInputWithdrawalMode::ThirdWitnessEmpty, Some("dao_three_input_third_empty_witness_input_type")) => {
            "dao_three_input_third_empty_witness_input_type"
        }
        (DaoThreeInputWithdrawalMode::ThirdWitnessShort, Some("dao_three_input_third_short_witness_input_type")) => {
            "dao_three_input_third_short_witness_input_type"
        }
        (DaoThreeInputWithdrawalMode::ThirdWitnessLong, Some("dao_three_input_third_long_witness_input_type")) => {
            "dao_three_input_third_long_witness_input_type"
        }
        (
            DaoThreeInputWithdrawalMode::ThirdWitnessWithdrawHeaderIndex,
            Some("dao_three_input_third_withdraw_header_witness_index"),
        ) => "dao_three_input_third_withdraw_header_witness_index",
        (DaoThreeInputWithdrawalMode::ThirdWitnessOutOfBoundsIndex, Some("dao_three_input_third_oob_witness_index")) => {
            "dao_three_input_third_oob_witness_index"
        }
        (DaoThreeInputWithdrawalMode::ThirdDepositDataInput, Some("dao_three_input_third_deposit_data_input")) => {
            "dao_three_input_third_deposit_data_input"
        }
        (DaoThreeInputWithdrawalMode::ThirdMalformedInputData, Some("dao_three_input_third_malformed_input_data")) => {
            "dao_three_input_third_malformed_input_data"
        }
        (DaoThreeInputWithdrawalMode::ThirdLongInputData, Some("dao_three_input_third_long_input_data")) => {
            "dao_three_input_third_long_input_data"
        }
        (DaoThreeInputWithdrawalMode::MixedDepositThird, _) => "dao_three_input_mixed_deposit_rate_max_withdrawal_capacity",
        (DaoThreeInputWithdrawalMode::MixedWithdrawThird, _) => "dao_three_input_mixed_withdraw_rate_max_withdrawal_capacity",
        (DaoThreeInputWithdrawalMode::MixedBothThird, _) => "dao_three_input_mixed_both_rate_max_withdrawal_capacity",
        (DaoThreeInputWithdrawalMode::MixedDepositSecond, _) => "dao_three_input_second_mixed_deposit_rate_max_withdrawal_capacity",
        (DaoThreeInputWithdrawalMode::MixedWithdrawSecond, _) => "dao_three_input_second_mixed_withdraw_rate_max_withdrawal_capacity",
        (DaoThreeInputWithdrawalMode::MixedBothSecond, _) => "dao_three_input_second_mixed_both_rate_max_withdrawal_capacity",
        (DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird, _) => {
            "dao_three_input_second_deposit_third_withdraw_rate_max_withdrawal_capacity"
        }
        (DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird, _) => {
            "dao_three_input_second_withdraw_third_deposit_rate_max_withdrawal_capacity"
        }
        _ => "dao_three_input_max_withdrawal_capacity",
    };
    let capacity_boundary = match (mode, failure_mode) {
        (DaoThreeInputWithdrawalMode::SameDeposit, Some(_)) => "three_input_exact_maximum_plus_one",
        (DaoThreeInputWithdrawalMode::MixedDepositSecond, Some(_)) => "three_input_second_mixed_deposit_rate_exact_maximum_plus_one",
        (DaoThreeInputWithdrawalMode::MixedDepositSecond, None) => "three_input_second_mixed_deposit_rate_exact_maximum",
        (DaoThreeInputWithdrawalMode::MixedWithdrawSecond, Some(_)) => "three_input_second_mixed_withdraw_rate_exact_maximum_plus_one",
        (DaoThreeInputWithdrawalMode::MixedWithdrawSecond, None) => "three_input_second_mixed_withdraw_rate_exact_maximum",
        (DaoThreeInputWithdrawalMode::MixedBothSecond, Some(_)) => "three_input_second_mixed_both_rate_exact_maximum_plus_one",
        (DaoThreeInputWithdrawalMode::MixedBothSecond, None) => "three_input_second_mixed_both_rate_exact_maximum",
        (DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird, Some(_)) => {
            "three_input_second_deposit_third_withdraw_rate_exact_maximum_plus_one"
        }
        (DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird, None) => {
            "three_input_second_deposit_third_withdraw_rate_exact_maximum"
        }
        (DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird, Some(_)) => {
            "three_input_second_withdraw_third_deposit_rate_exact_maximum_plus_one"
        }
        (DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird, None) => {
            "three_input_second_withdraw_third_deposit_rate_exact_maximum"
        }
        (DaoThreeInputWithdrawalMode::MixedDepositThird, Some(_)) => "three_input_mixed_deposit_rate_exact_maximum_plus_one",
        (DaoThreeInputWithdrawalMode::MixedDepositThird, None) => "three_input_mixed_deposit_rate_exact_maximum",
        (DaoThreeInputWithdrawalMode::MixedWithdrawThird, Some(_)) => "three_input_mixed_withdraw_rate_exact_maximum_plus_one",
        (DaoThreeInputWithdrawalMode::MixedWithdrawThird, None) => "three_input_mixed_withdraw_rate_exact_maximum",
        (DaoThreeInputWithdrawalMode::MixedBothThird, Some(_)) => "three_input_mixed_both_rate_exact_maximum_plus_one",
        (DaoThreeInputWithdrawalMode::MixedBothThird, None) => "three_input_mixed_both_rate_exact_maximum",
        (
            DaoThreeInputWithdrawalMode::SecondWitnessMissing
            | DaoThreeInputWithdrawalMode::SecondWitnessEmpty
            | DaoThreeInputWithdrawalMode::SecondWitnessShort
            | DaoThreeInputWithdrawalMode::SecondWitnessLong,
            Some(_),
        ) => "three_input_exact_maximum_with_malformed_second_witness",
        (
            DaoThreeInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex | DaoThreeInputWithdrawalMode::SecondWitnessOutOfBoundsIndex,
            Some(_),
        ) => "three_input_exact_maximum_with_malformed_second_witness_index",
        (
            DaoThreeInputWithdrawalMode::SecondDepositDataInput
            | DaoThreeInputWithdrawalMode::SecondMalformedInputData
            | DaoThreeInputWithdrawalMode::SecondLongInputData,
            Some(_),
        ) => "three_input_exact_maximum_with_non_withdrawal_second_input_data",
        (
            DaoThreeInputWithdrawalMode::ThirdWitnessMissing
            | DaoThreeInputWithdrawalMode::ThirdWitnessEmpty
            | DaoThreeInputWithdrawalMode::ThirdWitnessShort
            | DaoThreeInputWithdrawalMode::ThirdWitnessLong,
            Some(_),
        ) => "three_input_exact_maximum_with_malformed_third_witness",
        (
            DaoThreeInputWithdrawalMode::ThirdWitnessWithdrawHeaderIndex | DaoThreeInputWithdrawalMode::ThirdWitnessOutOfBoundsIndex,
            Some(_),
        ) => "three_input_exact_maximum_with_malformed_third_witness_index",
        (
            DaoThreeInputWithdrawalMode::ThirdDepositDataInput
            | DaoThreeInputWithdrawalMode::ThirdMalformedInputData
            | DaoThreeInputWithdrawalMode::ThirdLongInputData,
            Some(_),
        ) => "three_input_exact_maximum_with_non_withdrawal_third_input_data",
        _ => "three_input_exact_maximum",
    };
    let mut header_deps = vec![
        json!({
            "index": 0,
            "role": "withdraw_header",
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH
            }
        }),
        json!({
            "index": 1,
            "role": "deposit_header",
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH
            }
        }),
    ];
    if matches!(
        mode,
        DaoThreeInputWithdrawalMode::MixedDepositSecond
            | DaoThreeInputWithdrawalMode::MixedBothSecond
            | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            | DaoThreeInputWithdrawalMode::MixedDepositThird
            | DaoThreeInputWithdrawalMode::MixedBothThird
    ) {
        header_deps.push(json!({
            "index": 2,
            "role": if matches!(
                mode,
                DaoThreeInputWithdrawalMode::MixedDepositSecond
                    | DaoThreeInputWithdrawalMode::MixedBothSecond
                    | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            ) {
                "deposit_header_second_mixed_rate"
            } else {
                "deposit_header_mixed_rate"
            },
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH
            }
        }));
    }
    if matches!(
        mode,
        DaoThreeInputWithdrawalMode::MixedWithdrawSecond
            | DaoThreeInputWithdrawalMode::MixedBothSecond
            | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedBothThird
    ) {
        header_deps.push(json!({
            "index": header_deps.len(),
            "role": if matches!(
                mode,
                DaoThreeInputWithdrawalMode::MixedWithdrawSecond
                    | DaoThreeInputWithdrawalMode::MixedBothSecond
                    | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            ) {
                "withdraw_header_second_mixed_rate"
            } else {
                "withdraw_header_mixed_rate"
            },
            "block_number": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
            "accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
            "epoch": {
                "number": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                "index": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                "length": ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH
            }
        }));
    }
    let mut output = json!({
        "index": 0,
        "role": "withdrawn_capacity_cell",
        "capacity_shannons": output_capacity,
        "lock": "always_success",
        "type": null,
        "data": "0x",
        "capacity_boundary": capacity_boundary,
        "expected_maximum_capacity_shannons": expected_maximum_capacity,
        "dao_capacity_compensation": {
            "formula": "sum(occupied_capacity + ((input_capacity - occupied_capacity) * withdraw_rate / deposit_rate))",
            "input_count": 3,
            "per_input_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
            "per_input_occupied_capacity_shannons": ORIGINAL_DAO_WITHDRAW_INPUT_OCCUPIED_CAPACITY,
            "per_input_withdrawable_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAWABLE_CAPACITY,
            "withdraw_accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
            "second_withdraw_accumulated_rate": if matches!(
                mode,
                DaoThreeInputWithdrawalMode::MixedWithdrawSecond
                    | DaoThreeInputWithdrawalMode::MixedBothSecond
                    | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            ) {
                ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE
            } else {
                ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE
            },
            "third_withdraw_accumulated_rate": if matches!(
                mode,
                DaoThreeInputWithdrawalMode::MixedWithdrawThird
                    | DaoThreeInputWithdrawalMode::MixedBothThird
                    | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            ) {
                ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE
            } else {
                ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE
            },
            "deposit_accumulated_rate": ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
            "second_deposit_accumulated_rate": if matches!(
                mode,
                DaoThreeInputWithdrawalMode::MixedDepositSecond
                    | DaoThreeInputWithdrawalMode::MixedBothSecond
                    | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            ) {
                ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE
            } else {
                ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE
            },
            "third_deposit_accumulated_rate": if matches!(
                mode,
                DaoThreeInputWithdrawalMode::MixedDepositThird
                    | DaoThreeInputWithdrawalMode::MixedBothThird
                    | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            ) {
                ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE
            } else {
                ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE
            },
            "per_input_maximum_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE2_MAX_OUTPUT_CAPACITY
        }
    });
    if output_capacity > expected_maximum_capacity {
        output["overdrawn_by_shannons"] = json!(output_capacity - expected_maximum_capacity);
    }
    let inputs: Vec<Value> = (0..3)
        .map(|index| {
            let linked_header = match (index, mode) {
                (
                    1,
                    DaoThreeInputWithdrawalMode::MixedWithdrawSecond
                    | DaoThreeInputWithdrawalMode::MixedBothSecond
                    | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird,
                ) => "withdraw_header_second_mixed_rate",
                (
                    2,
                    DaoThreeInputWithdrawalMode::MixedWithdrawThird
                    | DaoThreeInputWithdrawalMode::MixedBothThird
                    | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird,
                ) => "withdraw_header_mixed_rate",
                _ => "withdraw_header",
            };
            let data = dao_three_input_cell_data(mode, index);
            let role = if index == 1 {
                match mode {
                    DaoThreeInputWithdrawalMode::SecondDepositDataInput => "deposit_data_dao_cell_spent_as_second_withdrawal",
                    DaoThreeInputWithdrawalMode::SecondMalformedInputData => "malformed_data_dao_cell_spent_as_second_withdrawal",
                    DaoThreeInputWithdrawalMode::SecondLongInputData => "long_data_dao_cell_spent_as_second_withdrawal",
                    _ => "withdrawing_dao_cell",
                }
            } else if index == 2 {
                match mode {
                    DaoThreeInputWithdrawalMode::ThirdDepositDataInput => "deposit_data_dao_cell_spent_as_third_withdrawal",
                    DaoThreeInputWithdrawalMode::ThirdMalformedInputData => "malformed_data_dao_cell_spent_as_third_withdrawal",
                    DaoThreeInputWithdrawalMode::ThirdLongInputData => "long_data_dao_cell_spent_as_third_withdrawal",
                    _ => "withdrawing_dao_cell",
                }
            } else {
                "withdrawing_dao_cell"
            };
            json!({
                "index": index,
                "role": role,
                "capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&data),
                "deposit_block_number": ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
                "linked_header": linked_header,
                "since": format!("0x{:016x}", ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE),
                "since_u64": ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE
            })
        })
        .collect();
    let witnesses: Vec<Value> = (0..3).map(|index| dao_three_input_witness_metadata(mode, index)).collect();
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": ["input_0_type", "input_1_type", "input_2_type"],
        "script_under_test_difference": "only code cell and input type script hash differ; original side uses the unmodified DAO ELF and CellScript side uses a generated three-input DAO occupied-capacity plus rate-compensation aggregate probe",
        "input_capacity_shannons": ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY * 3,
        "cell_deps": ["script_under_test"],
        "header_deps": header_deps,
        "witnesses": witnesses,
        "inputs": inputs,
        "outputs": [output],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}
