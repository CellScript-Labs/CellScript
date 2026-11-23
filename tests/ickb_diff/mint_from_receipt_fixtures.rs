fn normalized_non_empty_args_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "non_empty_script_args",
        "script_under_test_roles": ["output_0_type"],
        "script_under_test_difference": "only code cell and script hashes differ between original iCKB and CellScript",
        "input_capacity_shannons": NON_EMPTY_ARGS_INPUT_CAPACITY,
        "cell_deps": [],
        "header_deps": [],
        "witnesses": ["0x"],
        "outputs": [
            {
                "index": 0,
                "role": "script_args_reject_probe",
                "capacity_shannons": NON_EMPTY_ARGS_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "type_args": hex_prefixed(&non_empty_script_args()),
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "non_empty_args_rejected"
    })
}

fn normalized_mint_from_receipt_fixture_with_header_dep_and_receipt_data_mode(
    output_udt_amount: u128,
    accumulated_rate: u64,
    failure_mode: Option<&str>,
    xudt_binding: MintXudtBinding,
    header_dep_mode: MintHeaderDepMode,
    receipt_data_mode: MintReceiptDataMode,
) -> Value {
    let receipt_data = receipt_group_input_data(receipt_data_mode, 0);
    let xudt_data = xudt_output_data(output_udt_amount);
    let scenario = match failure_mode {
        Some("amount_inflation") => "amount_inflation",
        Some("amount_high_nonzero") => "amount_high_nonzero",
        Some("amount_deflation") => "amount_deflation",
        Some("wrong_xudt_binding") => "wrong_xudt_binding",
        Some("wrong_accumulated_rate") => "wrong_accumulated_rate",
        Some("missing_header_dep") => "missing_header_dep",
        Some("mint_malformed_receipt_data") => "mint_from_receipt_malformed_receipt_data",
        Some(_) => "mint_from_receipt_reject",
        None if receipt_data_mode == MintReceiptDataMode::QuantityZero => "mint_from_receipt_quantity_zero",
        None if receipt_data_mode == MintReceiptDataMode::QuantityTwo => "mint_from_receipt_quantity_two",
        None if receipt_data_mode == MintReceiptDataMode::LongTrailingData => "mint_from_receipt_long_data",
        None => "mint_from_receipt",
    };
    let receipt_role = match receipt_data_mode {
        MintReceiptDataMode::Valid => "ickb_receipt",
        MintReceiptDataMode::QuantityZero => "zero_quantity_ickb_receipt",
        MintReceiptDataMode::QuantityTwo => "quantity_two_ickb_receipt",
        MintReceiptDataMode::ZeroFirstQuantity => "zero_first_ickb_receipt",
        MintReceiptDataMode::MixedQuantities => "mixed_quantity_ickb_receipt",
        MintReceiptDataMode::LongTrailingData => "long_ickb_receipt",
        MintReceiptDataMode::MalformedFirstInput => "malformed_ickb_receipt",
        MintReceiptDataMode::MalformedSecondInput => "ickb_receipt",
    };
    let (receipt_quantity, receipt_deposit_amount) = receipt_fields_for_mode(receipt_data_mode, 0);
    let expected_xudt_amount = expected_mint_for_receipt_mode(receipt_data_mode, 0);
    let owner = match xudt_binding {
        MintXudtBinding::ScriptUnderTest => json!("script_under_test_hash"),
        MintXudtBinding::WrongOwnerHash => json!(hex_prefixed(&WRONG_XUDT_OWNER_HASH)),
    };
    let script_under_test_roles = match xudt_binding {
        MintXudtBinding::ScriptUnderTest => json!(["input_0_type", "output_0_xudt_owner"]),
        MintXudtBinding::WrongOwnerHash => json!(["input_0_type"]),
    };
    let script_under_test_difference = match xudt_binding {
        MintXudtBinding::ScriptUnderTest => {
            "only the iCKB owner script code cell and owner script hashes differ; both sides use the original xUDT binary with Data1 hash_type"
        }
        MintXudtBinding::WrongOwnerHash => {
            "only the input script-under-test code cell and script hash differ; both sides use the same wrong xUDT owner-mode args"
        }
    };
    let header_deps = match header_dep_mode {
        MintHeaderDepMode::Present => json!([
            {
                "index": 0,
                "linked_input": 0,
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
        "input_capacity_shannons": MINT_RECEIPT_INPUT_CAPACITY,
        "cell_deps": ["xudt"],
        "header_deps": header_deps,
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": receipt_role,
                "capacity_shannons": MINT_RECEIPT_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&receipt_data),
                "receipt_quantity": receipt_quantity,
                "receipt_deposit_amount_shannons": receipt_deposit_amount,
                "receipt_deposit_accumulated_rate": accumulated_rate
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "minted_ickb_xudt",
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
                "expected_xudt_amount": expected_xudt_amount as u64
            }
        ],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}
