fn owned_owner_valid_differential_execution() -> Value {
    owned_owner_differential_execution(OWNED_OWNER_VALID_DISTANCE, None)
}

fn owned_owner_output_valid_differential_execution() -> Value {
    owned_owner_output_differential_execution(OWNED_OWNER_OUTPUT_OWNER_DISTANCE, None)
}

fn owned_owner_output_relative_mismatch_differential_execution() -> Value {
    owned_owner_output_differential_execution(OWNED_OWNER_OUTPUT_MISMATCH_DISTANCE, Some("output_relative_distance_mismatch"))
}

fn owned_owner_output_duplicate_owner_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner_output_duplicate_owner();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) = run_cellscript_owned_owner_output_duplicate_owner();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output duplicate-owner differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 8, "original owned_owner Mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_duplicate_owner_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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
        "failure_mode": "output_duplicate_owner_pair",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_missing_owner_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner_output_missing_owner();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) = run_cellscript_owned_owner_output_missing_owner();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output missing-owner differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 8, "original owned_owner Mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_missing_owner_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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
        "failure_mode": "output_missing_owner_pair",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_missing_owned_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let original = run_original_owned_owner_output_missing_owned();
    let (cellscript, cellscript_elf) = run_cellscript_owned_owner_output_missing_owned();

    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output missing-owned differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 8, "original owned_owner Mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_missing_owned_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
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
        "failure_mode": "output_missing_owned_pair",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_script_misuse_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let original = run_original_owned_owner_output_script_misuse();
    let (cellscript, cellscript_elf) = run_cellscript_owned_owner_output_script_misuse();

    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output script misuse differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 7, "original owned_owner ScriptMisuse exit code");
    assert_eq!(cellscript.exit_code, 7, "CellScript owned-owner ScriptMisuse exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_script_misuse_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
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
        "failure_mode": "output_script_misuse",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_not_withdrawal_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let original = run_original_owned_owner_output_not_withdrawal();
    let (cellscript, cellscript_elf) = run_cellscript_owned_owner_output_not_withdrawal();

    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output non-withdrawal differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 6, "original owned_owner NotWithdrawalRequest exit code");
    assert_eq!(cellscript.exit_code, 6, "CellScript owned-owner NotWithdrawalRequest exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_not_withdrawal_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
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
        "failure_mode": "output_not_withdrawal_request",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_owner_data_length_mismatch_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner_output_owner_data_length_mismatch();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) =
        run_cellscript_owned_owner_output_owner_data_length_mismatch();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output owner data length mismatch differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 4, "original owned_owner output owner data length mismatch exit code");
    assert_eq!(cellscript.exit_code, 34, "CellScript owned-owner output owner data length mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_owner_data_length_mismatch_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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
        "failure_mode": "output_owner_data_length_mismatch",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_related_type_hash_mismatch_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (
        original,
        patched_original_owned_owner_binary_sha256,
        original_auxiliary_type_artifact_sha256,
        original_expected_type_hash,
        original_actual_type_hash,
    ) = run_original_owned_owner_output_related_type_hash_mismatch();
    let (
        cellscript,
        cellscript_elf,
        cellscript_auxiliary_type_artifact_sha256,
        cellscript_expected_type_hash,
        cellscript_actual_type_hash,
    ) = run_cellscript_owned_owner_output_related_type_hash_mismatch();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(original_expected_type_hash, cellscript_expected_type_hash, "expected related type hash should match across sides");
    assert_eq!(original_actual_type_hash, cellscript_actual_type_hash, "actual related type hash should match across sides");
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output related type hash mismatch differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 6, "original owned_owner output related type mismatch exit code");
    assert_eq!(cellscript.exit_code, 38, "CellScript owned-owner output related type mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture =
        normalized_owned_owner_output_related_type_hash_mismatch_fixture(&original_expected_type_hash, &original_actual_type_hash);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
        "expected_related_type_hash": original_expected_type_hash,
        "actual_related_type_hash": original_actual_type_hash,
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
        "failure_mode": "output_related_type_hash_mismatch",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_related_data_rule_mismatch_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256, original_expected_type_hash) =
        run_original_owned_owner_output_related_data_rule_mismatch();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256, cellscript_expected_type_hash) =
        run_cellscript_owned_owner_output_related_data_rule_mismatch();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(original_expected_type_hash, cellscript_expected_type_hash, "expected related type hash should match across sides");
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output related data rule mismatch differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 6, "original owned_owner output related data rule mismatch exit code");
    assert_eq!(cellscript.exit_code, 47, "CellScript owned-owner output related data rule mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_related_data_rule_mismatch_fixture(&original_expected_type_hash);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
        "expected_related_type_hash": original_expected_type_hash,
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
        "failure_mode": "output_related_data_rule_mismatch",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_related_type_hash_mismatch_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (
        original,
        patched_original_owned_owner_binary_sha256,
        original_auxiliary_type_artifact_sha256,
        original_expected_type_hash,
        original_actual_type_hash,
    ) = run_original_owned_owner_related_type_hash_mismatch();
    let (
        cellscript,
        cellscript_elf,
        cellscript_auxiliary_type_artifact_sha256,
        cellscript_expected_type_hash,
        cellscript_actual_type_hash,
    ) = run_cellscript_owned_owner_related_type_hash_mismatch();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(original_expected_type_hash, cellscript_expected_type_hash, "expected related type hash should match across sides");
    assert_eq!(original_actual_type_hash, cellscript_actual_type_hash, "actual related type hash should match across sides");
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner related type hash mismatch differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 6, "original owned_owner related type mismatch exit code");
    assert_eq!(cellscript.exit_code, 38, "CellScript owned-owner related type mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture =
        normalized_owned_owner_related_type_hash_mismatch_fixture(&original_expected_type_hash, &original_actual_type_hash);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
        "expected_related_type_hash": original_expected_type_hash,
        "actual_related_type_hash": original_actual_type_hash,
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
        "failure_mode": "related_type_hash_mismatch",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_related_data_rule_mismatch_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256, original_expected_type_hash) =
        run_original_owned_owner_related_data_rule_mismatch();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256, cellscript_expected_type_hash) =
        run_cellscript_owned_owner_related_data_rule_mismatch();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(original_expected_type_hash, cellscript_expected_type_hash, "expected related type hash should match across sides");
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner related data rule mismatch differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 6, "original owned_owner related data rule mismatch exit code");
    assert_eq!(cellscript.exit_code, 47, "CellScript owned-owner related data rule mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_related_data_rule_mismatch_fixture(&original_expected_type_hash);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
        "expected_related_type_hash": original_expected_type_hash,
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
        "failure_mode": "related_data_rule_mismatch",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_owner_data_length_mismatch_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner_owner_data_length_mismatch();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) =
        run_cellscript_owned_owner_owner_data_length_mismatch();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner owner data length mismatch differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 4, "original owned_owner owner data length mismatch exit code");
    assert_eq!(cellscript.exit_code, 34, "CellScript owned-owner owner data length mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_owner_data_length_mismatch_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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
        "failure_mode": "owner_data_length_mismatch",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_output_differential_execution(owner_relative_distance: i32, failure_mode: Option<&str>) -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner_output_pair(owner_relative_distance);
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) =
        run_cellscript_owned_owner_output_pair(owner_relative_distance);

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner output pairing differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original owned_owner status");
    assert_eq!(cellscript.status, expected_status, "CellScript owned-owner status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_output_fixture(owner_relative_distance, failure_mode);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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

fn owned_owner_relative_mismatch_differential_execution() -> Value {
    owned_owner_differential_execution(OWNED_OWNER_MISMATCH_DISTANCE, Some("relative_distance_mismatch"))
}

fn owned_owner_script_misuse_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let original = run_original_owned_owner_script_misuse();
    let (cellscript, cellscript_elf) = run_cellscript_owned_owner_script_misuse();

    assert_eq!(
        original.status, cellscript.status,
        "owned-owner script misuse differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_script_misuse_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
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
        "failure_mode": "script_misuse",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_not_withdrawal_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let original = run_original_owned_owner_not_withdrawal();
    let (cellscript, cellscript_elf) = run_cellscript_owned_owner_not_withdrawal();

    assert_eq!(
        original.status, cellscript.status,
        "owned-owner non-withdrawal differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 6, "original owned_owner NotWithdrawalRequest exit code");
    assert_eq!(cellscript.exit_code, 6, "CellScript owned-owner NotWithdrawalRequest exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_not_withdrawal_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
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
        "failure_mode": "not_withdrawal_request",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_missing_owner_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner_missing_owner();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) = run_cellscript_owned_owner_missing_owner();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner missing-owner differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 8, "original owned_owner Mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_missing_owner_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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
        "failure_mode": "missing_owner_pair",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_missing_owned_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let original = run_original_owned_owner_missing_owned();
    let (cellscript, cellscript_elf) = run_cellscript_owned_owner_missing_owned();

    assert_eq!(
        original.status, cellscript.status,
        "owned-owner missing-owned differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 8, "original owned_owner Mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_missing_owned_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
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
        "failure_mode": "missing_owned_pair",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_duplicate_owner_differential_execution() -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner_duplicate_owner();
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) = run_cellscript_owned_owner_duplicate_owner();

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner duplicate-owner differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original owned_owner status");
    assert_eq!(cellscript.status, "fail", "CellScript owned-owner status");
    assert_eq!(original.exit_code, 8, "original owned_owner Mismatch exit code");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_duplicate_owner_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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
        "failure_mode": "duplicate_owner_pair",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn owned_owner_differential_execution(owner_relative_distance: i32, failure_mode: Option<&str>) -> Value {
    let original_owned_owner_elf = load_original_ickb_binary("owned_owner");
    let original_owned_owner_binary_sha256 = sha256_prefixed(&original_owned_owner_elf);
    let (original, patched_original_owned_owner_binary_sha256, original_auxiliary_type_artifact_sha256) =
        run_original_owned_owner(owner_relative_distance);
    let (cellscript, cellscript_elf, cellscript_auxiliary_type_artifact_sha256) = run_cellscript_owned_owner(owner_relative_distance);

    assert_eq!(
        original_auxiliary_type_artifact_sha256, cellscript_auxiliary_type_artifact_sha256,
        "auxiliary withdrawal type script artifact should match across sides"
    );
    assert_eq!(
        original.status, cellscript.status,
        "owned-owner differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original owned_owner status");
    assert_eq!(cellscript.status, expected_status, "CellScript owned-owner status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_owned_owner_fixture(owner_relative_distance, failure_mode);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_owned_owner_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_owned_owner_binary_sha256,
        "original_owned_owner_binary_sha256": original_owned_owner_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "original_auxiliary_type_artifact_sha256": original_auxiliary_type_artifact_sha256,
        "cellscript_auxiliary_type_artifact_sha256": cellscript_auxiliary_type_artifact_sha256,
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
