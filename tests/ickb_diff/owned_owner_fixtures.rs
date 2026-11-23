fn normalized_owned_owner_fixture(owner_relative_distance: i32, failure_mode: Option<&str>) -> Value {
    let scenario = match failure_mode {
        Some("relative_distance_mismatch") => "owned_owner_relative_distance_mismatch",
        Some(_) => "owned_owner_reject",
        None => "valid_owned_owner_pairing",
    };
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": ["input_0_lock", "input_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 2,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x", "0x"],
        "inputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNED_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNER_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(owner_relative_distance)),
                "owner_relative_distance_i32": owner_relative_distance,
                "valid_owner_relative_distance_i32": OWNED_OWNER_VALID_DISTANCE
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}

fn normalized_owned_owner_output_fixture(owner_relative_distance: i32, failure_mode: Option<&str>) -> Value {
    let scenario = match failure_mode {
        Some("output_relative_distance_mismatch") => "owned_owner_output_relative_distance_mismatch",
        Some(_) => "owned_owner_output_reject",
        None => "valid_owned_owner_output_pairing",
    };
    let points_to_owned_output_index = 1i64 + i64::from(owner_relative_distance);
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": scenario,
        "script_under_test_roles": ["output_0_lock", "output_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(owner_relative_distance)),
                "owner_relative_distance_i32": owner_relative_distance,
                "valid_owner_relative_distance_i32": OWNED_OWNER_OUTPUT_OWNER_DISTANCE,
                "points_to_owned_output_index": points_to_owned_output_index
            }
        ],
        "expected_status": if failure_mode.is_some() { "fail" } else { "pass" },
        "failure_mode": failure_mode
    })
}

fn normalized_owned_owner_output_duplicate_owner_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_duplicate_owner_pair",
        "script_under_test_roles": ["output_0_lock", "output_1_type", "output_2_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 3,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 3,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_OUTPUT_OWNER_DISTANCE,
                "points_to_owned_output_index": 0
            },
            {
                "index": 2,
                "role": "duplicate_owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_OUTPUT_DUPLICATE_OWNER_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_OUTPUT_DUPLICATE_OWNER_DISTANCE,
                "points_to_owned_output_index": 0
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_duplicate_owner_pair"
    })
}

fn normalized_owned_owner_output_missing_owner_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_missing_owner_pair",
        "script_under_test_roles": ["output_0_lock", "output_1_lock", "output_2_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 3,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 3,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request_without_owner",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true,
                "matching_owner_present": false
            },
            {
                "index": 1,
                "role": "paired_owned_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true,
                "matching_owner_present": true
            },
            {
                "index": 2,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_OUTPUT_OWNER_DISTANCE,
                "points_to_owned_output_index": 1
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_missing_owner_pair"
    })
}

fn normalized_owned_owner_output_missing_owned_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_missing_owned_pair",
        "script_under_test_roles": ["output_0_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; no DAO hash patch is used because only an output owner-side type cell is present and the scripts reject on pair accounting",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owner_cell_without_owned",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_OUTPUT_MISMATCH_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_OUTPUT_MISMATCH_DISTANCE,
                "matching_owned_present": false
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_missing_owned_pair"
    })
}

fn normalized_owned_owner_output_script_misuse_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_script_misuse",
        "script_under_test_roles": ["output_0_lock", "output_0_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; no DAO hash patch is used because both scripts reject the output role misuse before DAO type/data classification",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "misused_owned_owner_output",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "script_under_test",
                "data": "0x",
                "script_misuse": true
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_script_misuse"
    })
}

fn normalized_owned_owner_output_not_withdrawal_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_non_withdrawal_request",
        "script_under_test_roles": ["output_0_lock", "output_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; no DAO hash patch is used because both scripts reject the output lock-owned cell before owner-pair matching",
        "input_capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
        "cell_deps": ["script_under_test", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owned_non_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": null,
                "data": "0x",
                "withdrawal_request": false
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_OUTPUT_OWNER_DISTANCE,
                "points_to_owned_output_index": 0
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_not_withdrawal_request"
    })
}

fn normalized_owned_owner_output_owner_data_length_mismatch_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_owner_data_length_mismatch",
        "script_under_test_roles": ["output_0_lock", "output_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true
            },
            {
                "index": 1,
                "role": "owner_cell_with_malformed_distance_data",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_malformed_distance_data()),
                "owner_relative_distance_bytes": 3,
                "expected_owner_relative_distance_bytes": 4,
                "owner_relative_distance_i32_decodable": false
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_owner_data_length_mismatch"
    })
}

fn normalized_owned_owner_output_related_type_hash_mismatch_fixture(expected_type_hash: &str, actual_type_hash: &str) -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_related_type_hash_mismatch",
        "script_under_test_roles": ["output_0_lock", "output_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the original Owned-Owner binary is patched to the expected auxiliary withdrawal type hash, while the lock-owned output deliberately uses the same auxiliary code with different args and therefore a different type hash",
        "input_capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type_code", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "expected_related_type_hash": expected_type_hash,
        "actual_related_type_hash": actual_type_hash,
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request_with_wrong_type_hash",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type_with_non_empty_args",
                "expected_type_hash": expected_type_hash,
                "actual_type_hash": actual_type_hash,
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true,
                "related_type_hash_matches": false
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_OUTPUT_OWNER_DISTANCE,
                "points_to_owned_output_index": 0
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_related_type_hash_mismatch"
    })
}

fn normalized_owned_owner_output_related_data_rule_mismatch_fixture(expected_type_hash: &str) -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_output_related_data_rule_mismatch",
        "script_under_test_roles": ["output_0_lock", "output_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the original Owned-Owner binary is patched to the expected auxiliary withdrawal type hash, and the lock-owned output uses that expected type hash but deliberately carries deposit-style zero data instead of withdrawal-request data",
        "input_capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "expected_related_type_hash": expected_type_hash,
        "inputs": [
            {
                "index": 0,
                "role": "funding_cell",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request_with_wrong_data_rule",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "expected_type_hash": expected_type_hash,
                "actual_type_hash": expected_type_hash,
                "data": hex_prefixed(&owned_owner_deposit_data()),
                "withdrawal_request": false,
                "related_type_hash_matches": true,
                "related_data_rule_matches": false
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_OUTPUT_OWNER_DISTANCE,
                "points_to_owned_output_index": 0
            }
        ],
        "expected_status": "fail",
        "failure_mode": "output_related_data_rule_mismatch"
    })
}

fn normalized_owned_owner_related_type_hash_mismatch_fixture(expected_type_hash: &str, actual_type_hash: &str) -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_related_type_hash_mismatch",
        "script_under_test_roles": ["input_0_lock", "input_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the original Owned-Owner binary is patched to the expected auxiliary withdrawal type hash, while the lock-owned input deliberately uses the same auxiliary code with different args and therefore a different type hash",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 2,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type_code", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x", "0x"],
        "expected_related_type_hash": expected_type_hash,
        "actual_related_type_hash": actual_type_hash,
        "inputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request_with_wrong_type_hash",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_RELATED_TYPE_HASH_MISMATCH_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type_with_non_empty_args",
                "expected_type_hash": expected_type_hash,
                "actual_type_hash": actual_type_hash,
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true,
                "related_type_hash_matches": false
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNER_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_VALID_DISTANCE
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "related_type_hash_mismatch"
    })
}

fn normalized_owned_owner_related_data_rule_mismatch_fixture(expected_type_hash: &str) -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_related_data_rule_mismatch",
        "script_under_test_roles": ["input_0_lock", "input_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the original Owned-Owner binary is patched to the expected auxiliary withdrawal type hash, and the lock-owned input uses that expected type hash but deliberately carries deposit-style zero data instead of withdrawal-request data",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 2,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x", "0x"],
        "expected_related_type_hash": expected_type_hash,
        "inputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request_with_wrong_data_rule",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_RELATED_DATA_RULE_MISMATCH_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "expected_type_hash": expected_type_hash,
                "actual_type_hash": expected_type_hash,
                "data": hex_prefixed(&owned_owner_deposit_data()),
                "withdrawal_request": false,
                "related_type_hash_matches": true,
                "related_data_rule_matches": false
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNER_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_VALID_DISTANCE
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "related_data_rule_mismatch"
    })
}

fn normalized_owned_owner_owner_data_length_mismatch_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_owner_data_length_mismatch",
        "script_under_test_roles": ["input_0_lock", "input_1_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 2,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x", "0x"],
        "inputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNED_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true
            },
            {
                "index": 1,
                "role": "owner_cell_with_malformed_distance_data",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNER_DATA_LENGTH_MISMATCH_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_malformed_distance_data()),
                "owner_relative_distance_bytes": 3,
                "expected_owner_relative_distance_bytes": 4,
                "owner_relative_distance_i32_decodable": false
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_OUTPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "owner_data_length_mismatch"
    })
}

fn normalized_owned_owner_script_misuse_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_script_misuse",
        "script_under_test_roles": ["input_0_lock", "input_0_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; no DAO hash patch is used because both scripts reject before DAO type/data classification",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "misused_owned_owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_SCRIPT_MISUSE_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": "script_under_test",
                "data": "0x",
                "script_misuse": true
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "script_misuse"
    })
}

fn normalized_owned_owner_not_withdrawal_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_non_withdrawal_request",
        "script_under_test_roles": ["input_0_lock"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; no DAO hash patch is used because both scripts reject the lock-owned input before owner-pair matching",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "owned_non_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_NOT_WITHDRAWAL_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": null,
                "data": "0x",
                "withdrawal_request": false
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "not_withdrawal_request"
    })
}

fn normalized_owned_owner_missing_owner_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_missing_owner_pair",
        "script_under_test_roles": ["input_0_lock"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request_without_owner",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_MISSING_OWNER_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true,
                "matching_owner_present": false
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "missing_owner_pair"
    })
}

fn normalized_owned_owner_missing_owned_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_missing_owned_pair",
        "script_under_test_roles": ["input_0_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; no DAO hash patch is used because only an owner-side type cell is present and the scripts reject on pair accounting",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
        "cell_deps": ["script_under_test", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x"],
        "inputs": [
            {
                "index": 0,
                "role": "owner_cell_without_owned",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_MISSING_OWNED_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_VALID_DISTANCE,
                "matching_owned_present": false
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "missing_owned_pair"
    })
}

fn normalized_owned_owner_duplicate_owner_fixture() -> Value {
    json!({
        "schema": "cellscript-ickb-normalized-fixture-v1",
        "scenario": "owned_owner_duplicate_owner_pair",
        "script_under_test_roles": ["input_0_lock", "input_1_type", "input_2_type"],
        "script_under_test_difference": "only the Owned-Owner script code cell and script hashes differ; the auxiliary withdrawal type script is shared, and the original Owned-Owner binary is patched so its DAO hash matches that auxiliary type script hash in ckb-testtool",
        "input_capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 3,
        "cell_deps": ["script_under_test", "auxiliary_withdrawal_type", "always_success_lock"],
        "header_deps": [],
        "witnesses": ["0x", "0x", "0x"],
        "inputs": [
            {
                "index": 0,
                "role": "owned_withdrawal_request",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNED_OUT_POINT_INDEX
                },
                "lock": "script_under_test",
                "type": "auxiliary_withdrawal_type",
                "data": hex_prefixed(&owned_owner_withdrawal_request_data()),
                "withdrawal_request": true
            },
            {
                "index": 1,
                "role": "owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_OWNER_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_VALID_DISTANCE,
                "points_to_owned_out_point_index": OWNED_OWNER_OWNED_OUT_POINT_INDEX
            },
            {
                "index": 2,
                "role": "duplicate_owner_cell",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY,
                "out_point": {
                    "tx_hash": hex_prefixed(&OWNED_OWNER_TX_HASH),
                    "index": OWNED_OWNER_DUPLICATE_OWNER_OUT_POINT_INDEX
                },
                "lock": "always_success",
                "type": "script_under_test",
                "data": hex_prefixed(&owned_owner_distance_data(OWNED_OWNER_DUPLICATE_OWNER_DISTANCE)),
                "owner_relative_distance_i32": OWNED_OWNER_DUPLICATE_OWNER_DISTANCE,
                "points_to_owned_out_point_index": OWNED_OWNER_OWNED_OUT_POINT_INDEX
            }
        ],
        "outputs": [
            {
                "index": 0,
                "role": "capacity_sink",
                "capacity_shannons": OWNED_OWNER_INPUT_CAPACITY * 3,
                "lock": "always_success",
                "type": null,
                "data": "0x"
            }
        ],
        "expected_status": "fail",
        "failure_mode": "duplicate_owner_pair"
    })
}
