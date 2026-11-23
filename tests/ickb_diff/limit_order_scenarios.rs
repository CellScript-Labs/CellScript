fn limit_order_differential_execution(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
) -> Value {
    limit_order_differential_execution_with_scenario(
        input_udt_amount,
        output_capacity,
        output_udt_amount,
        failure_mode,
        asset_binding,
        None,
    )
}

fn limit_order_min_match_boundary_differential_execution() -> Value {
    limit_order_differential_execution_with_scenario(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_MIN_MATCH_OUTPUT_CAPACITY,
        LIMIT_ORDER_MIN_MATCH_OUTPUT_UDT_AMOUNT,
        None,
        LimitOrderAssetBinding::SameAuxiliaryType,
        Some("limit_order_min_match_boundary"),
    )
}

fn limit_order_wrong_master_tx_hash_differential_execution() -> Value {
    limit_order_differential_execution_with_scenario_and_master_binding(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        LimitOrderMasterBinding::WrongTxHash.failure_mode(false),
        LimitOrderAssetBinding::SameAuxiliaryType,
        None,
        LimitOrderMasterBinding::WrongTxHash,
    )
}

fn limit_order_wrong_master_index_differential_execution() -> Value {
    limit_order_differential_execution_with_scenario_and_master_binding(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        LimitOrderMasterBinding::WrongIndex.failure_mode(false),
        LimitOrderAssetBinding::SameAuxiliaryType,
        None,
        LimitOrderMasterBinding::WrongIndex,
    )
}

fn limit_order_output_mint_action_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_output_mint_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::MintAction,
        ),
    )
}

fn limit_order_output_invalid_action_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_output_invalid_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::InvalidAction,
        ),
    )
}

fn limit_order_output_short_action_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_output_short_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::ShortAction,
        ),
    )
}

fn limit_order_output_short_master_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_output_short_master_out_point"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::ShortMasterOutPoint,
        ),
    )
}

fn limit_order_output_long_data_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_output_long_data"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::LongTrailingData,
        ),
    )
}

fn limit_order_input_invalid_action_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_input_invalid_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::InvalidAction,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_input_short_action_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_input_short_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::ShortAction,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_input_short_master_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_input_short_master_out_point"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::ShortMasterOutPoint,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_input_long_data_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_input_long_data"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::LongTrailingData,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_input_absolute_match_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            None,
            LimitOrderAssetBinding::SameAuxiliaryType,
            Some("limit_order_input_absolute_match"),
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::MatchAbsolute,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_input_wrong_master_tx_hash_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_input_wrong_master_tx_hash"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::MatchWrongTxHash,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_input_wrong_master_index_differential_execution() -> Value {
    limit_order_differential_execution_with_options(
        LIMIT_ORDER_INPUT_UDT_AMOUNT,
        LIMIT_ORDER_OUTPUT_CAPACITY,
        LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_input_wrong_master_index"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::MatchWrongIndex,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_cell_shape_differential_execution(shape: LimitOrderCellShape) -> Value {
    let original_limit_order_elf = load_original_ickb_binary("limit_order");
    let original_limit_order_binary_sha256 = sha256_prefixed(&original_limit_order_elf);
    let (original, original_auxiliary_type_sha256) = run_original_limit_order_with_cell_shape(shape);
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_sha256) = run_cellscript_limit_order_with_cell_shape(shape);

    assert_eq!(
        original_auxiliary_type_sha256, cellscript_auxiliary_type_sha256,
        "auxiliary UDT type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "limit order cell-shape differential mismatch: original={:#?}, cellscript={:#?}, shape={:?}",
        original, cellscript, shape
    );
    assert_eq!(original.status, "fail", "original limit_order status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_limit_order_cell_shape_fixture(shape);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_limit_order_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_limit_order_binary_sha256": original_limit_order_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_sha256,
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
        "failure_mode": shape.failure_mode(false),
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn limit_order_missing_matching_output_differential_execution() -> Value {
    limit_order_cell_shape_differential_execution(LimitOrderCellShape::MissingMatchingOutput)
}

fn limit_order_duplicate_matching_output_differential_execution() -> Value {
    limit_order_cell_shape_differential_execution(LimitOrderCellShape::DuplicateMatchingOutputs)
}

fn limit_order_type_shape_differential_execution(shape: LimitOrderTypeShape) -> Value {
    let original_limit_order_elf = load_original_ickb_binary("limit_order");
    let original_limit_order_binary_sha256 = sha256_prefixed(&original_limit_order_elf);
    let (original, original_auxiliary_type_sha256) = run_original_limit_order_with_type_shape(shape);
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_sha256) = run_cellscript_limit_order_with_type_shape(shape);

    assert_eq!(
        original_auxiliary_type_sha256, cellscript_auxiliary_type_sha256,
        "auxiliary UDT type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "limit order type-shape differential mismatch: original={:#?}, cellscript={:#?}, shape={:?}",
        original, cellscript, shape
    );
    assert_eq!(original.status, "fail", "original limit_order status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_limit_order_type_shape_fixture(shape);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_limit_order_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_limit_order_binary_sha256": original_limit_order_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_sha256,
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
        "failure_mode": shape.failure_mode(false),
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn limit_order_missing_input_type_differential_execution() -> Value {
    limit_order_type_shape_differential_execution(LimitOrderTypeShape::MissingInputAuxiliaryType)
}

fn limit_order_missing_output_type_differential_execution() -> Value {
    limit_order_type_shape_differential_execution(LimitOrderTypeShape::MissingOutputAuxiliaryType)
}

fn limit_order_differential_execution_with_scenario(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
    pass_scenario: Option<&'static str>,
) -> Value {
    limit_order_differential_execution_with_scenario_and_master_binding(
        input_udt_amount,
        output_capacity,
        output_udt_amount,
        failure_mode,
        asset_binding,
        pass_scenario,
        LimitOrderMasterBinding::Matching,
    )
}

fn limit_order_differential_execution_with_scenario_and_master_binding(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
    pass_scenario: Option<&'static str>,
    master_binding: LimitOrderMasterBinding,
) -> Value {
    limit_order_differential_execution_with_options(
        input_udt_amount,
        output_capacity,
        output_udt_amount,
        limit_order_options(
            failure_mode,
            asset_binding,
            pass_scenario,
            master_binding,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_differential_execution_with_options(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    options: LimitOrderScenarioOptions,
) -> Value {
    let original_limit_order_elf = load_original_ickb_binary("limit_order");
    let original_limit_order_binary_sha256 = sha256_prefixed(&original_limit_order_elf);
    let (original, original_auxiliary_type_sha256) = run_original_limit_order_fulfillment_with_master_binding_and_output_data_mode(
        input_udt_amount,
        output_capacity,
        output_udt_amount,
        options.asset_binding,
        options.master_binding,
        options.input_data_mode,
        options.output_data_mode,
    );
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_sha256) =
        run_cellscript_limit_order_fulfillment_with_master_binding_and_output_data_mode(
            input_udt_amount,
            output_capacity,
            output_udt_amount,
            options.asset_binding,
            options.master_binding,
            options.input_data_mode,
            options.output_data_mode,
        );

    assert_eq!(
        original.status, cellscript.status,
        "limit order differential mismatch: original={:#?}, cellscript={:#?}, input_udt_amount={}, output_capacity={}, output_udt_amount={}",
        original, cellscript, input_udt_amount, output_capacity, output_udt_amount
    );
    let expected_status = if options.failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original limit_order status");
    assert_eq!(cellscript.status, expected_status, "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture =
        normalized_limit_order_fixture_with_scenario(input_udt_amount, output_capacity, output_udt_amount, options);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_limit_order_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_limit_order_binary_sha256": original_limit_order_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_sha256,
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
        "failure_mode": options.failure_mode,
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn limit_order_udt_to_ckb_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_params(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        None,
        LimitOrderAssetBinding::SameAuxiliaryType,
    )
}

fn limit_order_udt_to_ckb_min_match_boundary_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_params_and_scenario(
        LIMIT_ORDER_UDT_TO_CKB_MIN_MATCH_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_MIN_MATCH_OUTPUT_UDT_AMOUNT,
        None,
        LimitOrderAssetBinding::SameAuxiliaryType,
        Some("limit_order_udt_to_ckb_min_match_boundary"),
    )
}

fn limit_order_udt_to_ckb_wrong_master_tx_hash_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_params_scenario_and_master_binding(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        LimitOrderMasterBinding::WrongTxHash.failure_mode(true),
        LimitOrderAssetBinding::SameAuxiliaryType,
        None,
        LimitOrderMasterBinding::WrongTxHash,
    )
}

fn limit_order_udt_to_ckb_wrong_master_index_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_params_scenario_and_master_binding(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        LimitOrderMasterBinding::WrongIndex.failure_mode(true),
        LimitOrderAssetBinding::SameAuxiliaryType,
        None,
        LimitOrderMasterBinding::WrongIndex,
    )
}

fn limit_order_udt_to_ckb_output_mint_action_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_output_mint_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::MintAction,
        ),
    )
}

fn limit_order_udt_to_ckb_output_invalid_action_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_output_invalid_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::InvalidAction,
        ),
    )
}

fn limit_order_udt_to_ckb_output_short_action_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_output_short_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::ShortAction,
        ),
    )
}

fn limit_order_udt_to_ckb_output_short_master_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_output_short_master_out_point"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::ShortMasterOutPoint,
        ),
    )
}

fn limit_order_udt_to_ckb_output_long_data_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_output_long_data"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::LongTrailingData,
        ),
    )
}

fn limit_order_udt_to_ckb_input_invalid_action_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_input_invalid_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::InvalidAction,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_input_short_action_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_input_short_action"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::ShortAction,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_input_short_master_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_input_short_master_out_point"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::ShortMasterOutPoint,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_input_long_data_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_input_long_data"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::LongTrailingData,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_input_absolute_match_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            None,
            LimitOrderAssetBinding::SameAuxiliaryType,
            Some("limit_order_udt_to_ckb_input_absolute_match"),
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::MatchAbsolute,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_input_wrong_master_tx_hash_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_input_wrong_master_tx_hash"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::MatchWrongTxHash,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_input_wrong_master_index_differential_execution() -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY,
        LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT,
        limit_order_options(
            Some("limit_order_udt_to_ckb_input_wrong_master_index"),
            LimitOrderAssetBinding::SameAuxiliaryType,
            None,
            LimitOrderMasterBinding::Matching,
            LimitOrderInputDataMode::MatchWrongIndex,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_cell_shape_differential_execution(shape: LimitOrderCellShape) -> Value {
    let original_limit_order_elf = load_original_ickb_binary("limit_order");
    let original_limit_order_binary_sha256 = sha256_prefixed(&original_limit_order_elf);
    let (original, auxiliary_artifact_sha256) = run_original_limit_order_udt_to_ckb_with_cell_shape(shape);
    let (cellscript, cellscript_elf, cellscript_auxiliary_artifact_sha256) =
        run_cellscript_limit_order_udt_to_ckb_with_cell_shape(shape);

    assert_eq!(
        auxiliary_artifact_sha256, cellscript_auxiliary_artifact_sha256,
        "auxiliary UDT type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "UDT-to-CKB limit order missing-output differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original iCKB Limit Order status");
    assert_eq!(cellscript.status, "fail", "CellScript Limit Order status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_limit_order_udt_to_ckb_cell_shape_fixture(shape);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_limit_order_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_limit_order_binary_sha256": original_limit_order_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": auxiliary_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_artifact_sha256,
        "shared_funding_lock_artifact_sha256": auxiliary_artifact_sha256,
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
        "failure_mode": shape.failure_mode(true),
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn limit_order_udt_to_ckb_missing_matching_output_differential_execution() -> Value {
    limit_order_udt_to_ckb_cell_shape_differential_execution(LimitOrderCellShape::MissingMatchingOutput)
}

fn limit_order_udt_to_ckb_duplicate_matching_output_differential_execution() -> Value {
    limit_order_udt_to_ckb_cell_shape_differential_execution(LimitOrderCellShape::DuplicateMatchingOutputs)
}

fn limit_order_udt_to_ckb_type_shape_differential_execution(shape: LimitOrderTypeShape) -> Value {
    let original_limit_order_elf = load_original_ickb_binary("limit_order");
    let original_limit_order_binary_sha256 = sha256_prefixed(&original_limit_order_elf);
    let (original, auxiliary_artifact_sha256) = run_original_limit_order_udt_to_ckb_with_type_shape(shape);
    let (cellscript, cellscript_elf, cellscript_auxiliary_artifact_sha256) =
        run_cellscript_limit_order_udt_to_ckb_with_type_shape(shape);

    assert_eq!(
        auxiliary_artifact_sha256, cellscript_auxiliary_artifact_sha256,
        "auxiliary UDT type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "UDT-to-CKB limit order type-shape differential mismatch: original={:#?}, cellscript={:#?}, shape={:?}",
        original, cellscript, shape
    );
    assert_eq!(original.status, "fail", "original iCKB Limit Order status");
    assert_eq!(cellscript.status, "fail", "CellScript Limit Order status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_limit_order_udt_to_ckb_type_shape_fixture(shape);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_limit_order_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_limit_order_binary_sha256": original_limit_order_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": auxiliary_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_artifact_sha256,
        "shared_funding_lock_artifact_sha256": auxiliary_artifact_sha256,
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
        "failure_mode": shape.failure_mode(true),
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn limit_order_udt_to_ckb_missing_input_type_differential_execution() -> Value {
    limit_order_udt_to_ckb_type_shape_differential_execution(LimitOrderTypeShape::MissingInputAuxiliaryType)
}

fn limit_order_udt_to_ckb_missing_output_type_differential_execution() -> Value {
    limit_order_udt_to_ckb_type_shape_differential_execution(LimitOrderTypeShape::MissingOutputAuxiliaryType)
}

fn limit_order_udt_to_ckb_differential_execution_with_params(
    output_capacity: u64,
    output_udt_amount: u128,
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
) -> Value {
    limit_order_udt_to_ckb_differential_execution_with_params_and_scenario(
        output_capacity,
        output_udt_amount,
        failure_mode,
        asset_binding,
        None,
    )
}

fn limit_order_udt_to_ckb_differential_execution_with_params_and_scenario(
    output_capacity: u64,
    output_udt_amount: u128,
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
    pass_scenario: Option<&'static str>,
) -> Value {
    limit_order_udt_to_ckb_differential_execution_with_params_scenario_and_master_binding(
        output_capacity,
        output_udt_amount,
        failure_mode,
        asset_binding,
        pass_scenario,
        LimitOrderMasterBinding::Matching,
    )
}

fn limit_order_udt_to_ckb_differential_execution_with_params_scenario_and_master_binding(
    output_capacity: u64,
    output_udt_amount: u128,
    failure_mode: Option<&'static str>,
    asset_binding: LimitOrderAssetBinding,
    pass_scenario: Option<&'static str>,
    master_binding: LimitOrderMasterBinding,
) -> Value {
    limit_order_udt_to_ckb_differential_execution_with_options(
        output_capacity,
        output_udt_amount,
        limit_order_options(
            failure_mode,
            asset_binding,
            pass_scenario,
            master_binding,
            LimitOrderInputDataMode::Mint,
            LimitOrderOutputDataMode::Match,
        ),
    )
}

fn limit_order_udt_to_ckb_differential_execution_with_options(
    output_capacity: u64,
    output_udt_amount: u128,
    options: LimitOrderScenarioOptions,
) -> Value {
    let original_limit_order_elf = load_original_ickb_binary("limit_order");
    let original_limit_order_binary_sha256 = sha256_prefixed(&original_limit_order_elf);
    let (original, auxiliary_artifact_sha256) =
        run_original_limit_order_udt_to_ckb_fulfillment_with_master_binding_and_output_data_mode(
            LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT,
            output_capacity,
            output_udt_amount,
            options.asset_binding,
            options.master_binding,
            options.input_data_mode,
            options.output_data_mode,
        );
    let (cellscript, cellscript_elf, cellscript_auxiliary_artifact_sha256) =
        run_cellscript_limit_order_udt_to_ckb_fulfillment_with_master_binding_and_output_data_mode(
            LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT,
            output_capacity,
            output_udt_amount,
            options.asset_binding,
            options.master_binding,
            options.input_data_mode,
            options.output_data_mode,
        );

    assert_eq!(
        auxiliary_artifact_sha256, cellscript_auxiliary_artifact_sha256,
        "auxiliary UDT type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "UDT-to-CKB limit order differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    let expected_status = if options.failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original iCKB Limit Order status");
    assert_eq!(cellscript.status, expected_status, "CellScript Limit Order status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_limit_order_udt_to_ckb_fixture(
        LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT,
        output_capacity,
        output_udt_amount,
        options,
    );
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_limit_order_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_limit_order_binary_sha256": original_limit_order_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": auxiliary_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_artifact_sha256,
        "shared_funding_lock_artifact_sha256": auxiliary_artifact_sha256,
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
        "failure_mode": options.failure_mode,
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}
