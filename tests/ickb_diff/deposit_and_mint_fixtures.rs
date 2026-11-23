fn normalized_deposit_phase1_fixture(deposit_capacity: u64, failure_mode: Option<&str>) -> Value {
    let receipt_data = deposit_phase1_receipt_data(deposit_capacity);
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "deposit_phase_1",
        "script_under_test_roles": ["output_0_lock", "output_1_type"],
        "script_under_test_difference": "only code cell and script hashes differ between original iCKB and CellScript",
        "input_capacity_shannons": DEPOSIT_PHASE1_INPUT_CAPACITY,
        "minimum_deposit_capacity_shannons": ICKB_MIN_DEPOSIT_CAPACITY,
        "cell_deps": ["dao"],
        "header_deps": [],
        "witnesses": ["0x"],
        "outputs": [
            {
                "index": 0,
                "role": "dao_deposit",
                "capacity_shannons": deposit_capacity,
                "lock": "script_under_test",
                "type": "dao",
                "data": "0x0000000000000000"
            },
            {
                "index": 1,
                "role": "ickb_receipt",
                "capacity_shannons": deposit_capacity,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&receipt_data),
                "receipt_quantity": 1,
                "receipt_deposit_amount_shannons": deposit_phase1_unoccupied_capacity(deposit_capacity)
            }
        ],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}

fn normalized_deposit_phase1_upper_bound_fixture() -> Value {
    let receipt_data = deposit_phase1_receipt_data(HUGE_DEPOSIT_PHASE1_CAPACITY);
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "deposit_phase_1_capacity_upper_bound",
        "script_under_test_roles": ["output_0_lock", "output_1_type"],
        "script_under_test_difference": "only code cell and script hashes differ between original iCKB and CellScript; CellScript side uses a capacity-upper-bound probe for this fixture",
        "input_capacity_shannons": HUGE_DEPOSIT_PHASE1_INPUT_CAPACITY,
        "minimum_deposit_capacity_shannons": ICKB_MIN_DEPOSIT_CAPACITY,
        "maximum_deposit_capacity_shannons": 100_000_000_000_000u64,
        "cell_deps": ["dao"],
        "header_deps": [],
        "witnesses": ["0x"],
        "outputs": [
            {
                "index": 0,
                "role": "dao_deposit",
                "capacity_shannons": HUGE_DEPOSIT_PHASE1_CAPACITY,
                "lock": "script_under_test",
                "type": "dao",
                "data": "0x0000000000000000"
            },
            {
                "index": 1,
                "role": "ickb_receipt",
                "capacity_shannons": HUGE_DEPOSIT_PHASE1_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&receipt_data),
                "receipt_quantity": 1,
                "receipt_deposit_amount_shannons": deposit_phase1_unoccupied_capacity(HUGE_DEPOSIT_PHASE1_CAPACITY)
            }
        ],
        "expected_status": "fail",
        "failure_mode": "deposit_capacity_upper_bound_rejected"
    })
}

fn normalized_deposit_phase1_receipt_shape_fixture(receipt_quantity: u32, receipt_deposit_amount: u64, failure_mode: &str) -> Value {
    let mut fixture = normalized_deposit_phase1_fixture(VALID_DEPOSIT_PHASE1_CAPACITY, Some(failure_mode));
    let receipt_data = deposit_phase1_receipt_data_with(receipt_quantity, receipt_deposit_amount);
    fixture["scenario"] = json!(failure_mode);
    fixture["outputs"][1]["data"] = json!(hex_prefixed(&receipt_data));
    fixture["outputs"][1]["receipt_quantity"] = json!(receipt_quantity);
    fixture["outputs"][1]["receipt_deposit_amount_shannons"] = json!(receipt_deposit_amount);
    fixture["outputs"][1]["expected_receipt_deposit_amount_shannons"] =
        json!(deposit_phase1_unoccupied_capacity(VALID_DEPOSIT_PHASE1_CAPACITY));
    fixture["expected_status"] = json!("fail");
    fixture["failure_mode"] = json!(failure_mode);
    fixture
}

fn normalized_deposit_phase1_receipt_raw_data_fixture(
    receipt_data: Bytes,
    scenario: &str,
    failure_mode: Option<&str>,
    expected_status: &str,
) -> Value {
    let mut fixture = normalized_deposit_phase1_fixture(VALID_DEPOSIT_PHASE1_CAPACITY, failure_mode);
    fixture["scenario"] = json!(scenario);
    fixture["outputs"][1]["data"] = json!(hex_prefixed(&receipt_data));
    fixture["outputs"][1]["receipt_data_length_bytes"] = json!(receipt_data.len());
    fixture["outputs"][1]["minimum_decoded_receipt_data_length_bytes"] = json!(12);
    fixture["outputs"][1]["trailing_receipt_data_bytes"] = json!(receipt_data.len().saturating_sub(12));
    fixture["outputs"][1]["receipt_quantity"] = Value::Null;
    fixture["outputs"][1]["receipt_deposit_amount_shannons"] = Value::Null;
    fixture["expected_status"] = json!(expected_status);
    fixture["failure_mode"] = json!(failure_mode);
    fixture
}

fn normalized_deposit_phase1_dao_type_shape_fixture(dao_type_shape: DepositPhase1DaoTypeShape) -> Value {
    let failure_mode = dao_type_shape.failure_mode().expect("invalid DAO type shape must have failure mode");
    let mut fixture = normalized_deposit_phase1_fixture(VALID_DEPOSIT_PHASE1_CAPACITY, Some(failure_mode));
    fixture["scenario"] = json!(failure_mode);
    fixture["outputs"][0]["type"] = dao_type_shape.fixture_type_label();
    fixture["expected_status"] = json!("fail");
    fixture["failure_mode"] = json!(failure_mode);
    fixture
}

fn normalized_deposit_phase1_lock_shape_fixture(lock_shape: DepositPhase1LockShape) -> Value {
    let failure_mode = lock_shape.failure_mode().expect("invalid lock shape must have failure mode");
    let mut fixture = normalized_deposit_phase1_fixture(VALID_DEPOSIT_PHASE1_CAPACITY, Some(failure_mode));
    fixture["scenario"] = json!(failure_mode);
    fixture["outputs"][0]["lock"] = json!(lock_shape.fixture_lock_label());
    fixture["expected_status"] = json!("fail");
    fixture["failure_mode"] = json!(failure_mode);
    fixture
}

fn normalized_deposit_phase1_deposit_data_shape_fixture(deposit_data_shape: DepositPhase1DepositDataShape) -> Value {
    let failure_mode = deposit_data_shape.failure_mode();
    let mut fixture = normalized_deposit_phase1_fixture(VALID_DEPOSIT_PHASE1_CAPACITY, failure_mode);
    let deposit_data = deposit_data_shape.data();
    fixture["scenario"] = json!(deposit_data_shape.scenario());
    fixture["outputs"][0]["data"] = json!(hex_prefixed(&deposit_data));
    fixture["outputs"][0]["deposit_data_length_bytes"] = json!(deposit_data.len());
    fixture["outputs"][0]["minimum_decoded_deposit_data_length_bytes"] = json!(8);
    fixture["outputs"][0]["trailing_deposit_data_bytes"] = json!(deposit_data.len().saturating_sub(8));
    fixture["expected_status"] = json!(deposit_data_shape.expected_status());
    fixture["failure_mode"] = json!(failure_mode);
    fixture
}

fn normalized_duplicate_receipt_output_fixture() -> Value {
    let receipt_data = deposit_phase1_receipt_data(DUPLICATE_RECEIPT_OUTPUT_CAPACITY);
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "duplicate_receipt_output",
        "script_under_test_roles": ["output_0_lock", "output_1_type", "output_2_type"],
        "script_under_test_difference": "only code cell and script hashes differ between original iCKB and CellScript",
        "input_capacity_shannons": DEPOSIT_PHASE1_INPUT_CAPACITY,
        "minimum_deposit_capacity_shannons": ICKB_MIN_DEPOSIT_CAPACITY,
        "cell_deps": ["dao"],
        "header_deps": [],
        "witnesses": ["0x"],
        "outputs": [
            {
                "index": 0,
                "role": "dao_deposit",
                "capacity_shannons": DUPLICATE_RECEIPT_OUTPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "dao",
                "data": "0x0000000000000000"
            },
            {
                "index": 1,
                "role": "ickb_receipt",
                "capacity_shannons": DUPLICATE_RECEIPT_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&receipt_data),
                "receipt_quantity": 1,
                "receipt_deposit_amount_shannons": deposit_phase1_unoccupied_capacity(DUPLICATE_RECEIPT_OUTPUT_CAPACITY)
            },
            {
                "index": 2,
                "role": "duplicate_ickb_receipt",
                "capacity_shannons": DUPLICATE_RECEIPT_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&receipt_data),
                "receipt_quantity": 1,
                "receipt_deposit_amount_shannons": deposit_phase1_unoccupied_capacity(DUPLICATE_RECEIPT_OUTPUT_CAPACITY)
            }
        ],
        "expected_status": "fail",
        "failure_mode": "duplicate_receipt_output"
    })
}

fn normalized_receipt_group_mint_fixture(
    output_udt_amount: u128,
    accumulated_rate: u64,
    failure_mode: Option<&str>,
    header_dep_mode: MintHeaderDepMode,
    xudt_binding: MintXudtBinding,
    receipt_data_mode: MintReceiptDataMode,
) -> Value {
    let first_receipt_data = receipt_group_input_data(receipt_data_mode, 0);
    let second_receipt_data = receipt_group_input_data(receipt_data_mode, 1);
    let xudt_data = xudt_output_data(output_udt_amount);
    let first_expected_mint = expected_mint_for_receipt_mode(receipt_data_mode, 0);
    let second_expected_mint = expected_mint_for_receipt_mode(receipt_data_mode, 1);
    let expected_xudt_amount = first_expected_mint + second_expected_mint;
    let scenario = match failure_mode {
        Some("receipt_group_under_mint") => "receipt_group_under_mint",
        Some("receipt_group_over_mint") => "receipt_group_over_mint",
        Some("receipt_group_missing_header_dep") => "receipt_group_missing_header_dep",
        Some("receipt_group_wrong_accumulated_rate") => "receipt_group_wrong_accumulated_rate",
        Some("receipt_group_wrong_xudt_binding") => "receipt_group_wrong_xudt_binding",
        Some("receipt_group_malformed_receipt_data") => "receipt_group_malformed_receipt_data",
        Some("receipt_group_second_malformed_receipt_data") => "receipt_group_second_malformed_receipt_data",
        Some("receipt_group_amount_high_nonzero") => "receipt_group_amount_high_nonzero",
        Some(_) => "receipt_group_mint_reject",
        None if receipt_data_mode == MintReceiptDataMode::QuantityZero => "receipt_group_quantity_zero",
        None if receipt_data_mode == MintReceiptDataMode::QuantityTwo => "receipt_group_quantity_two",
        None if receipt_data_mode == MintReceiptDataMode::ZeroFirstQuantity => "receipt_group_zero_first_quantity",
        None if receipt_data_mode == MintReceiptDataMode::MixedQuantities => "receipt_group_mixed_quantities",
        None if receipt_data_mode == MintReceiptDataMode::LongTrailingData => "receipt_group_long_receipt_data",
        None => "receipt_group_exact_mint",
    };
    let first_input_role = match receipt_data_mode {
        MintReceiptDataMode::Valid => "ickb_receipt",
        MintReceiptDataMode::QuantityZero => "zero_quantity_ickb_receipt",
        MintReceiptDataMode::QuantityTwo => "quantity_two_ickb_receipt",
        MintReceiptDataMode::ZeroFirstQuantity => "zero_first_ickb_receipt",
        MintReceiptDataMode::MixedQuantities => "mixed_first_ickb_receipt",
        MintReceiptDataMode::LongTrailingData => "long_first_ickb_receipt",
        MintReceiptDataMode::MalformedFirstInput => "malformed_ickb_receipt",
        MintReceiptDataMode::MalformedSecondInput => "ickb_receipt",
    };
    let second_input_role = match receipt_data_mode {
        MintReceiptDataMode::MalformedSecondInput => "malformed_second_ickb_receipt",
        MintReceiptDataMode::MixedQuantities => "mixed_second_ickb_receipt",
        MintReceiptDataMode::QuantityTwo => "quantity_two_second_ickb_receipt",
        MintReceiptDataMode::LongTrailingData => "long_second_ickb_receipt",
        MintReceiptDataMode::QuantityZero => "zero_quantity_second_ickb_receipt",
        _ => "second_ickb_receipt",
    };
    let (first_quantity, first_deposit_amount) = receipt_fields_for_mode(receipt_data_mode, 0);
    let (second_quantity, second_deposit_amount) = receipt_fields_for_mode(receipt_data_mode, 1);
    let output_role = match failure_mode {
        Some("receipt_group_under_mint") => "under_minted_ickb_xudt",
        Some("receipt_group_over_mint") => "over_minted_ickb_xudt",
        Some("receipt_group_missing_header_dep") => "minted_ickb_xudt",
        Some("receipt_group_wrong_accumulated_rate") => "minted_ickb_xudt",
        Some("receipt_group_wrong_xudt_binding") => "wrong_owner_ickb_xudt",
        Some("receipt_group_malformed_receipt_data") => "minted_ickb_xudt",
        Some("receipt_group_second_malformed_receipt_data") => "minted_ickb_xudt",
        Some("receipt_group_amount_high_nonzero") => "high_word_ickb_xudt",
        Some(_) => "invalid_minted_ickb_xudt",
        None => "minted_ickb_xudt",
    };
    let owner = match xudt_binding {
        MintXudtBinding::ScriptUnderTest => json!("script_under_test_hash"),
        MintXudtBinding::WrongOwnerHash => json!(hex_prefixed(&WRONG_XUDT_OWNER_HASH)),
    };
    let script_under_test_roles = match xudt_binding {
        MintXudtBinding::ScriptUnderTest => json!(["input_0_type", "input_1_type", "output_0_xudt_owner"]),
        MintXudtBinding::WrongOwnerHash => json!(["input_0_type", "input_1_type"]),
    };
    let script_under_test_difference = match xudt_binding {
        MintXudtBinding::ScriptUnderTest => "only the iCKB owner script code cell and owner script hashes differ; both sides use two same-shaped receipt inputs and the original xUDT binary with Data1 hash_type",
        MintXudtBinding::WrongOwnerHash => "only the input script-under-test code cell and script hashes differ; both sides use two same-shaped receipt inputs and the same wrong xUDT owner-mode args",
    };
    let header_deps = match header_dep_mode {
        MintHeaderDepMode::Present => json!([
            {
                "index": 0,
                "linked_inputs": [0, 1],
                "dao_accumulated_rate": accumulated_rate
            }
        ]),
        MintHeaderDepMode::Omitted => json!([]),
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": script_under_test_roles,
        "script_under_test_difference": script_under_test_difference,
        "input_capacity_shannons": MINT_RECEIPT_INPUT_CAPACITY * 2,
        "cell_deps": ["xudt"],
        "header_deps": header_deps,
        "witnesses": ["0x", "0x"],
        "inputs": [
            {
                "index": 0,
                "role": first_input_role,
                "capacity_shannons": MINT_RECEIPT_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&first_receipt_data),
                "receipt_quantity": first_quantity,
                "receipt_deposit_amount_shannons": first_deposit_amount,
                "receipt_deposit_accumulated_rate": accumulated_rate
            },
            {
                "index": 1,
                "role": second_input_role,
                "capacity_shannons": MINT_RECEIPT_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&second_receipt_data),
                "receipt_quantity": second_quantity,
                "receipt_deposit_amount_shannons": second_deposit_amount,
                "receipt_deposit_accumulated_rate": accumulated_rate
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": output_role,
                "capacity_shannons": MINT_XUDT_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": "original_xudt",
                "xudt_hash_type": "Data1",
                "xudt_owner_mode_args": {
                    "owner": owner,
                    "flags_le_u32": XUDT_OWNER_MODE_TYPE_FLAGS
                },
                "xudt_binding": match xudt_binding {
                    MintXudtBinding::ScriptUnderTest => "script_under_test_hash+owner_mode_input_type",
                    MintXudtBinding::WrongOwnerHash => "wrong_owner_hash+owner_mode_input_type"
                },
                "data": hex_prefixed(&xudt_data),
                "xudt_amount": output_udt_amount as u64,
                "xudt_amount_low_u64": output_udt_amount as u64,
                "xudt_amount_high_u64": (output_udt_amount >> 64) as u64,
                "expected_xudt_amount": expected_xudt_amount as u64,
                "expected_xudt_amount_low_u64": expected_xudt_amount as u64,
                "expected_xudt_amount_high_u64": (expected_xudt_amount >> 64) as u64
            }
        ],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}

fn normalized_receipt_group_missing_second_input_fixture() -> Value {
    let receipt_data = mint_receipt_data();
    let xudt_data = xudt_output_data(MINT_RECEIPT_OUTPUT_AMOUNT * 2);
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "receipt_group_missing_second_input",
        "script_under_test_roles": ["input_0_type", "output_0_xudt_owner"],
        "script_under_test_difference": "only the iCKB owner script code cell and owner script hashes differ; both sides use one receipt input while the xUDT output claims the two-receipt mint amount",
        "input_capacity_shannons": MINT_RECEIPT_INPUT_CAPACITY,
        "cell_deps": ["xudt"],
        "header_deps": [
            {
                "index": 0,
                "linked_inputs": [0],
                "dao_accumulated_rate": MINT_RECEIPT_ACCUMULATED_RATE
            }
        ],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "single_ickb_receipt",
                "capacity_shannons": MINT_RECEIPT_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&receipt_data),
                "receipt_quantity": MINT_RECEIPT_QUANTITY,
                "receipt_deposit_amount_shannons": MINT_RECEIPT_DEPOSIT_AMOUNT,
                "receipt_deposit_accumulated_rate": MINT_RECEIPT_ACCUMULATED_RATE
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "over_minted_ickb_xudt_missing_second_receipt",
                "capacity_shannons": MINT_XUDT_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": "original_xudt",
                "xudt_hash_type": "Data1",
                "xudt_owner_mode_args": {
                    "owner": "script_under_test_hash",
                    "flags_le_u32": XUDT_OWNER_MODE_TYPE_FLAGS
                },
                "xudt_binding": "script_under_test_hash+owner_mode_input_type",
                "data": hex_prefixed(&xudt_data),
                "xudt_amount": (MINT_RECEIPT_OUTPUT_AMOUNT * 2) as u64,
                "expected_xudt_amount": MINT_RECEIPT_OUTPUT_AMOUNT as u64
            }
        ],
        "expected_status": "fail",
        "failure_mode": "receipt_group_missing_second_input"
    })
}

fn normalized_receipt_without_deposit_fixture() -> Value {
    let receipt_data = receipt_without_deposit_data();
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "receipt_without_deposit",
        "script_under_test_roles": ["output_0_type"],
        "script_under_test_difference": "only code cell and script hashes differ between original iCKB and CellScript",
        "input_capacity_shannons": RECEIPT_WITHOUT_DEPOSIT_INPUT_CAPACITY,
        "cell_deps": [],
        "header_deps": [],
        "witnesses": ["0x"],
        "outputs": [
            {
                "index": 0,
                "role": "ickb_receipt",
                "capacity_shannons": RECEIPT_WITHOUT_DEPOSIT_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&receipt_data),
                "receipt_quantity": 1,
                "receipt_deposit_amount_shannons": 100_000_000_000u64
            }
        ],
        "expected_status": "fail",
        "failure_mode": "receipt_without_deposit_rejected"
    })
}
