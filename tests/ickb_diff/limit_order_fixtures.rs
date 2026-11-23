fn normalized_limit_order_fixture_with_scenario(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    options: LimitOrderScenarioOptions,
) -> Value {
    let input_data = limit_order_input_data_for_mode(input_udt_amount, options.input_data_mode);
    let output_data = limit_order_output_data_for_mode(output_udt_amount, options.master_binding, options.output_data_mode);
    let output_auxiliary_type_args = match options.asset_binding {
        LimitOrderAssetBinding::SameAuxiliaryType => "0x",
        LimitOrderAssetBinding::DifferentAuxiliaryType => "0x01",
    };
    let scenario = match options.failure_mode {
        Some("limit_order_underpayment") => "limit_order_underpayment",
        Some("wrong_asset") => "limit_order_wrong_asset",
        Some("insufficient_match") => "limit_order_insufficient_match",
        Some("no_ckb_paid_out") => "limit_order_no_ckb_paid_out",
        Some("udt_decreased") => "limit_order_udt_decreased",
        Some("wrong_master_tx_hash") => "limit_order_wrong_master_tx_hash",
        Some("wrong_master_index") => "limit_order_wrong_master_index",
        Some("limit_order_output_mint_action") => "limit_order_output_mint_action",
        Some("limit_order_output_invalid_action") => "limit_order_output_invalid_action",
        Some("limit_order_output_short_action") => "limit_order_output_short_action",
        Some("limit_order_output_short_master_out_point") => "limit_order_output_short_master_out_point",
        Some("limit_order_output_long_data") => "limit_order_output_long_data",
        Some("limit_order_input_invalid_action") => "limit_order_input_invalid_action",
        Some("limit_order_input_short_action") => "limit_order_input_short_action",
        Some("limit_order_input_short_master_out_point") => "limit_order_input_short_master_out_point",
        Some("limit_order_input_long_data") => "limit_order_input_long_data",
        Some("limit_order_input_wrong_master_tx_hash") => "limit_order_input_wrong_master_tx_hash",
        Some("limit_order_input_wrong_master_index") => "limit_order_input_wrong_master_index",
        None if options.pass_scenario == Some("limit_order_input_absolute_match") => "limit_order_input_absolute_match",
        Some(_) => "limit_order_reject",
        None if options.pass_scenario == Some("limit_order_min_match_boundary") => "limit_order_min_match_boundary",
        None => "valid_limit_order",
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": ["input_0_lock", "output_0_lock"],
        "script_under_test_difference": "only the Limit Order owner lock script code cell and script hashes differ; both sides use the same auxiliary always-success UDT type script code",
        "asset_binding": match options.asset_binding {
            LimitOrderAssetBinding::SameAuxiliaryType => "same_auxiliary_type_hash",
            LimitOrderAssetBinding::DifferentAuxiliaryType => "different_auxiliary_type_hash"
        },
        "input_capacity_shannons": LIMIT_ORDER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_type"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "limit_order",
                "capacity_shannons": LIMIT_ORDER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&input_data),
                "order_action": options.input_data_mode.order_action(),
                "master_distance_i32": 0,
                "udt_amount": input_udt_amount as u64,
                "ckb_to_udt_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "udt_to_ckb_ratio": null,
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "limit_order",
                "capacity_shannons": output_capacity,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": output_auxiliary_type_args,
                "data": hex_prefixed(&output_data),
                "order_action": options.output_data_mode.order_action(),
                "master_out_point": {
                    "tx_hash": hex_prefixed(options.master_binding.master_tx_hash()),
                    "index": options.master_binding.master_index()
                },
                "udt_amount": output_udt_amount as u64,
                "ckb_to_udt_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "udt_to_ckb_ratio": null,
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }
        ],
        "expected_status": if options.failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": options.failure_mode
    })
}

fn normalized_limit_order_cell_shape_fixture(shape: LimitOrderCellShape) -> Value {
    let input_data = limit_order_mint_data(LIMIT_ORDER_INPUT_UDT_AMOUNT, 0);
    let matching_data = limit_order_match_data(LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
    let first_duplicate_data = limit_order_match_data(LIMIT_ORDER_DUPLICATE_FIRST_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
    let second_duplicate_data = limit_order_match_data(LIMIT_ORDER_DUPLICATE_SECOND_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
    let outputs = match shape {
        LimitOrderCellShape::MissingMatchingOutput => vec![json!({
            "index": 0,
            "role": "non_matching_limit_order_candidate",
            "capacity_shannons": LIMIT_ORDER_OUTPUT_CAPACITY,
            "lock": "always_success",
            "type": "auxiliary_udt_type",
            "auxiliary_type_args": "0x",
            "data": hex_prefixed(&matching_data),
            "order_action": "Match",
            "master_out_point": {
                "tx_hash": hex_prefixed(&LIMIT_ORDER_MASTER_TX_HASH),
                "index": 0
            },
            "udt_amount": LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT as u64,
            "ckb_to_udt_ratio": {
                "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
            },
            "udt_to_ckb_ratio": null,
            "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
        })],
        LimitOrderCellShape::DuplicateMatchingOutputs => vec![
            json!({
                "index": 0,
                "role": "duplicate_matching_limit_order",
                "capacity_shannons": LIMIT_ORDER_DUPLICATE_FIRST_OUTPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&first_duplicate_data),
                "order_action": "Match",
                "master_out_point": {
                    "tx_hash": hex_prefixed(&LIMIT_ORDER_MASTER_TX_HASH),
                    "index": 0
                },
                "udt_amount": LIMIT_ORDER_DUPLICATE_FIRST_OUTPUT_UDT_AMOUNT as u64,
                "ckb_to_udt_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "udt_to_ckb_ratio": null,
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }),
            json!({
                "index": 1,
                "role": "duplicate_matching_limit_order",
                "capacity_shannons": LIMIT_ORDER_DUPLICATE_SECOND_OUTPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&second_duplicate_data),
                "order_action": "Match",
                "master_out_point": {
                    "tx_hash": hex_prefixed(&LIMIT_ORDER_MASTER_TX_HASH),
                    "index": 0
                },
                "udt_amount": LIMIT_ORDER_DUPLICATE_SECOND_OUTPUT_UDT_AMOUNT as u64,
                "ckb_to_udt_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "udt_to_ckb_ratio": null,
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }),
        ],
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": shape.scenario(false),
        "script_under_test_roles": match shape {
            LimitOrderCellShape::MissingMatchingOutput => vec!["input_0_lock"],
            LimitOrderCellShape::DuplicateMatchingOutputs => vec!["input_0_lock", "output_0_lock", "output_1_lock"],
        },
        "script_under_test_difference": "only the Limit Order owner lock script code cell and script hashes differ; both sides use the same auxiliary always-success UDT type script code",
        "asset_binding": "same_auxiliary_type_hash",
        "input_capacity_shannons": LIMIT_ORDER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_type"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "limit_order",
                "capacity_shannons": LIMIT_ORDER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&input_data),
                "order_action": "Mint",
                "master_distance_i32": 0,
                "udt_amount": LIMIT_ORDER_INPUT_UDT_AMOUNT as u64,
                "ckb_to_udt_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "udt_to_ckb_ratio": null,
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }
        ],
        "outputs": outputs,
        "expected_status": "fail",
        "failure_mode": shape.failure_mode(false)
    })
}

fn normalized_limit_order_type_shape_fixture(shape: LimitOrderTypeShape) -> Value {
    let mut fixture = normalized_limit_order_fixture_with_scenario(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some(shape.failure_mode(false)),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::Match,
        ),
    );
    fixture["scenario"] = json!(shape.scenario(false));
    fixture["failure_mode"] = json!(shape.failure_mode(false));
    match shape {
        LimitOrderTypeShape::MissingInputAuxiliaryType => {
            fixture["inputs"][0]["type"] = Value::Null;
            fixture["inputs"][0]["auxiliary_type_args"] = Value::Null;
        }
        LimitOrderTypeShape::MissingOutputAuxiliaryType => {
            fixture["outputs"][0]["type"] = Value::Null;
            fixture["outputs"][0]["auxiliary_type_args"] = Value::Null;
        }
    }
    fixture
}

fn normalized_limit_order_udt_to_ckb_fixture(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    options: LimitOrderScenarioOptions,
) -> Value {
    let input_data = limit_order_udt_to_ckb_input_data_for_mode(input_udt_amount, options.input_data_mode);
    let output_data = limit_order_udt_to_ckb_output_data_for_mode(output_udt_amount, options.master_binding, options.output_data_mode);
    let output_auxiliary_type_args = match options.asset_binding {
        LimitOrderAssetBinding::SameAuxiliaryType => "0x",
        LimitOrderAssetBinding::DifferentAuxiliaryType => "0x01",
    };
    let scenario = match options.failure_mode {
        Some("limit_order_underpayment") => "limit_order_udt_to_ckb_underpayment",
        Some("insufficient_match") => "limit_order_udt_to_ckb_insufficient_match",
        Some("no_udt_paid_out") => "limit_order_udt_to_ckb_no_udt_paid_out",
        Some("wrong_asset") => "limit_order_udt_to_ckb_wrong_asset",
        Some("limit_order_udt_to_ckb_wrong_master_tx_hash") => "limit_order_udt_to_ckb_wrong_master_tx_hash",
        Some("limit_order_udt_to_ckb_wrong_master_index") => "limit_order_udt_to_ckb_wrong_master_index",
        Some("limit_order_udt_to_ckb_output_mint_action") => "limit_order_udt_to_ckb_output_mint_action",
        Some("limit_order_udt_to_ckb_output_invalid_action") => "limit_order_udt_to_ckb_output_invalid_action",
        Some("limit_order_udt_to_ckb_output_short_action") => "limit_order_udt_to_ckb_output_short_action",
        Some("limit_order_udt_to_ckb_output_short_master_out_point") => "limit_order_udt_to_ckb_output_short_master_out_point",
        Some("limit_order_udt_to_ckb_output_long_data") => "limit_order_udt_to_ckb_output_long_data",
        Some("limit_order_udt_to_ckb_input_invalid_action") => "limit_order_udt_to_ckb_input_invalid_action",
        Some("limit_order_udt_to_ckb_input_short_action") => "limit_order_udt_to_ckb_input_short_action",
        Some("limit_order_udt_to_ckb_input_short_master_out_point") => "limit_order_udt_to_ckb_input_short_master_out_point",
        Some("limit_order_udt_to_ckb_input_long_data") => "limit_order_udt_to_ckb_input_long_data",
        Some("limit_order_udt_to_ckb_input_wrong_master_tx_hash") => "limit_order_udt_to_ckb_input_wrong_master_tx_hash",
        Some("limit_order_udt_to_ckb_input_wrong_master_index") => "limit_order_udt_to_ckb_input_wrong_master_index",
        None if options.pass_scenario == Some("limit_order_udt_to_ckb_input_absolute_match") => {
            "limit_order_udt_to_ckb_input_absolute_match"
        }
        Some(_) => "limit_order_udt_to_ckb_reject",
        None if options.pass_scenario == Some("limit_order_udt_to_ckb_min_match_boundary") => {
            "limit_order_udt_to_ckb_min_match_boundary"
        }
        None => "valid_limit_order_udt_to_ckb",
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": ["input_0_lock", "output_0_lock"],
        "script_under_test_difference": "only the Limit Order owner lock script code cell and script hashes differ; both sides use the same auxiliary always-success UDT type script code and the same funding input",
        "asset_binding": match options.asset_binding {
            LimitOrderAssetBinding::SameAuxiliaryType => "same_auxiliary_type_hash",
            LimitOrderAssetBinding::DifferentAuxiliaryType => "different_auxiliary_type_hash"
        },
        "input_capacity_shannons": LIMIT_ORDER_INPUT_CAPACITY + LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_type", "always_success_funding_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "limit_order",
                "capacity_shannons": LIMIT_ORDER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&input_data),
                "order_action": options.input_data_mode.order_action(),
                "master_distance_i32": 0,
                "udt_amount": input_udt_amount as u64,
                "ckb_to_udt_ratio": null,
                "udt_to_ckb_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            },
            {
                "index": 1,
                "role": "funding_ckb",
                "capacity_shannons": LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "limit_order",
                "capacity_shannons": output_capacity,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": output_auxiliary_type_args,
                "data": hex_prefixed(&output_data),
                "order_action": options.output_data_mode.order_action(),
                "master_out_point": {
                    "tx_hash": hex_prefixed(options.master_binding.master_tx_hash()),
                    "index": options.master_binding.master_index()
                },
                "udt_amount": output_udt_amount as u64,
                "ckb_to_udt_ratio": null,
                "udt_to_ckb_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }
        ],
        "expected_status": if options.failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": options.failure_mode
    })
}

fn normalized_limit_order_udt_to_ckb_cell_shape_fixture(shape: LimitOrderCellShape) -> Value {
    let input_data = limit_order_udt_to_ckb_mint_data(LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT, 0);
    let output_data = limit_order_udt_to_ckb_match_data(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
    let duplicate_data =
        limit_order_udt_to_ckb_match_data(LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
    let outputs = match shape {
        LimitOrderCellShape::MissingMatchingOutput => vec![json!({
            "index": 0,
            "role": "non_matching_limit_order_candidate",
            "capacity_shannons": LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
            "lock": "always_success",
            "type": "auxiliary_udt_type",
            "auxiliary_type_args": "0x",
            "data": hex_prefixed(&output_data),
            "order_action": "Match",
            "master_out_point": {
                "tx_hash": hex_prefixed(&LIMIT_ORDER_MASTER_TX_HASH),
                "index": 0
            },
            "udt_amount": LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT as u64,
            "ckb_to_udt_ratio": null,
            "udt_to_ckb_ratio": {
                "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
            },
            "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
        })],
        LimitOrderCellShape::DuplicateMatchingOutputs => vec![
            json!({
                "index": 0,
                "role": "duplicate_matching_limit_order",
                "capacity_shannons": LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_FIRST_OUTPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&duplicate_data),
                "order_action": "Match",
                "master_out_point": {
                    "tx_hash": hex_prefixed(&LIMIT_ORDER_MASTER_TX_HASH),
                    "index": 0
                },
                "udt_amount": LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_OUTPUT_UDT_AMOUNT as u64,
                "ckb_to_udt_ratio": null,
                "udt_to_ckb_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }),
            json!({
                "index": 1,
                "role": "duplicate_matching_limit_order",
                "capacity_shannons": LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_SECOND_OUTPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&duplicate_data),
                "order_action": "Match",
                "master_out_point": {
                    "tx_hash": hex_prefixed(&LIMIT_ORDER_MASTER_TX_HASH),
                    "index": 0
                },
                "udt_amount": LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_OUTPUT_UDT_AMOUNT as u64,
                "ckb_to_udt_ratio": null,
                "udt_to_ckb_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            }),
        ],
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": shape.scenario(true),
        "script_under_test_roles": match shape {
            LimitOrderCellShape::MissingMatchingOutput => vec!["input_0_lock"],
            LimitOrderCellShape::DuplicateMatchingOutputs => vec!["input_0_lock", "output_0_lock", "output_1_lock"],
        },
        "script_under_test_difference": "only the Limit Order owner lock script code cell and script hashes differ; both sides use the same auxiliary always-success UDT type script code and the same funding input",
        "asset_binding": "same_auxiliary_type_hash",
        "input_capacity_shannons": limit_order_udt_to_ckb_input_capacity_for_cell_shape(shape),
        "cell_deps": ["script_under_test", "auxiliary_type", "always_success_funding_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "limit_order",
                "capacity_shannons": LIMIT_ORDER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_udt_type",
                "auxiliary_type_args": "0x",
                "data": hex_prefixed(&input_data),
                "order_action": "Mint",
                "master_distance_i32": 0,
                "udt_amount": LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT as u64,
                "ckb_to_udt_ratio": null,
                "udt_to_ckb_ratio": {
                    "ckb_mul": LIMIT_ORDER_CKB_TO_UDT_MUL,
                    "udt_mul": LIMIT_ORDER_UDT_TO_CKB_MUL
                },
                "ckb_min_match_shannons": 1u64 << LIMIT_ORDER_CKB_MIN_MATCH_LOG
            },
            {
                "index": 1,
                "role": "funding_ckb",
                "capacity_shannons": limit_order_udt_to_ckb_funding_capacity_for_cell_shape(shape),
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": outputs,
        "expected_status": "fail",
        "failure_mode": shape.failure_mode(true)
    })
}

fn normalized_limit_order_udt_to_ckb_type_shape_fixture(shape: LimitOrderTypeShape) -> Value {
    let mut fixture = normalized_limit_order_udt_to_ckb_fixture(
        LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some(shape.failure_mode(true)),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::Match,
        ),
    );
    fixture["scenario"] = json!(shape.scenario(true));
    fixture["failure_mode"] = json!(shape.failure_mode(true));
    match shape {
        LimitOrderTypeShape::MissingInputAuxiliaryType => {
            fixture["inputs"][0]["type"] = Value::Null;
            fixture["inputs"][0]["auxiliary_type_args"] = Value::Null;
        }
        LimitOrderTypeShape::MissingOutputAuxiliaryType => {
            fixture["outputs"][0]["type"] = Value::Null;
            fixture["outputs"][0]["auxiliary_type_args"] = Value::Null;
        }
    }
    fixture
}

fn occupied_capacity_shannons(outputs: &[packed::CellOutput], outputs_data: &[Bytes]) -> u64 {
    outputs
        .iter()
        .zip(outputs_data)
        .map(|(output, data)| {
            let data_capacity = Capacity::bytes(data.len()).expect("data capacity");
            output.occupied_capacity(data_capacity).expect("occupied capacity").as_u64()
        })
        .sum()
}

fn fee_shannons(input_capacity: u64, outputs: &[packed::CellOutput]) -> u64 {
    input_capacity.saturating_sub(
        outputs
            .iter()
            .map(|output| {
                let capacity: u64 = output.capacity().unpack();
                capacity
            })
            .sum(),
    )
}

fn sha256_json(value: &Value) -> String {
    sha256_prefixed(&serde_json::to_vec(value).expect("fixture json should serialize"))
}

fn sha256_prefixed(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("0x{}", hex::encode(hasher.finalize()))
}

fn patch_owned_owner_dao_hash(owned_owner_elf: &mut [u8], new_dao_hash: &[u8; 32]) {
    let mainnet_dao_hash =
        hex::decode("cc77c4deac05d68ab5b26828f0bf4565a8d73113d7bb7e92b8362b8a74e58e58").expect("mainnet DAO hash hex");
    let positions: Vec<_> = owned_owner_elf
        .windows(mainnet_dao_hash.len())
        .enumerate()
        .filter_map(|(index, window)| (window == mainnet_dao_hash.as_slice()).then_some(index))
        .collect();
    assert_eq!(positions, vec![0x771], "owned_owner DAO hash location changed: {positions:?}");
    let offset = positions[0];
    owned_owner_elf[offset..offset + 32].copy_from_slice(new_dao_hash);
}

fn hex_prefixed(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn cellscript_script_value_expr(script: &packed::Script) -> String {
    let code_hash: [u8; 32] = script.code_hash().unpack();
    let args = script.args().raw_data();
    let hash_type = match script.hash_type().as_slice()[0] {
        0 => "script::hash_type_data()",
        1 => "script::hash_type_type()",
        2 => "script::hash_type_data1()",
        4 => "script::hash_type_data2()",
        other => panic!("unsupported Script hash_type byte in fixture: {other}"),
    };
    let args_expr = if args.is_empty() {
        "script::args_empty()".to_string()
    } else {
        format!("script::args({})", cellscript_byte_string_literal(args.as_ref()))
    };
    format!("script::new(Hash::from_bytes({}), {}, {})", cellscript_byte_string_literal(&code_hash), hash_type, args_expr)
}

fn cellscript_byte_string_literal(bytes: &[u8]) -> String {
    let mut literal = String::from("b\"");
    for byte in bytes {
        literal.push_str(&format!("\\x{byte:02x}"));
    }
    literal.push('"');
    literal
}

fn parse_ckb_script_error_code(error: &str) -> Option<i64> {
    for marker in ["error code ", "error code: "] {
        if let Some(start) = error.find(marker).map(|index| index + marker.len()) {
            let digits: String = error[start..].chars().take_while(|ch| ch.is_ascii_digit() || *ch == '-').collect();
            if let Ok(code) = digits.parse() {
                return Some(code);
            }
        }
    }
    None
}
