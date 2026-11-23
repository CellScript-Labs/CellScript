fn deposit_phase1_differential_execution(deposit_capacity: u64, failure_mode: Option<&str>) -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let (original, patched_original_ickb_binary_sha256) = run_original_deposit_phase1(deposit_capacity);
    let (cellscript, cellscript_elf) = run_cellscript_deposit_phase1(deposit_capacity);

    assert_eq!(
        original.status, cellscript.status,
        "differential mismatch: original={:#?}, cellscript={:#?}, deposit_capacity={}",
        original, cellscript, deposit_capacity
    );
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original iCKB status");
    assert_eq!(cellscript.status, expected_status, "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_deposit_phase1_fixture(deposit_capacity, failure_mode);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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

fn deposit_phase1_upper_bound_differential_execution() -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let (original, patched_original_ickb_binary_sha256) =
        run_original_deposit_phase1_with_input_capacity(HUGE_DEPOSIT_PHASE1_CAPACITY, HUGE_DEPOSIT_PHASE1_INPUT_CAPACITY);
    let (cellscript, cellscript_elf) =
        run_cellscript_deposit_phase1_upper_bound(HUGE_DEPOSIT_PHASE1_CAPACITY, HUGE_DEPOSIT_PHASE1_INPUT_CAPACITY);

    assert_eq!(
        original.status, cellscript.status,
        "differential mismatch: original={:#?}, cellscript={:#?}, deposit_capacity={}",
        original, cellscript, HUGE_DEPOSIT_PHASE1_CAPACITY
    );
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_deposit_phase1_upper_bound_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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
        "failure_mode": "deposit_capacity_upper_bound_rejected",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn deposit_phase1_receipt_shape_differential_execution(
    receipt_quantity: u32,
    receipt_deposit_amount: u64,
    failure_mode: &'static str,
) -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let receipt_data = deposit_phase1_receipt_data_with(receipt_quantity, receipt_deposit_amount);
    let (original, patched_original_ickb_binary_sha256) = run_original_deposit_phase1_with_input_capacity_and_receipt_data(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        receipt_data.clone(),
    );
    let (cellscript, cellscript_elf) = run_cellscript_deposit_phase1_with_input_capacity_program_and_receipt_data(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        DEPOSIT_PHASE1_CELLSCRIPT_PROGRAM,
        DEPOSIT_PHASE1_CELLSCRIPT_ACTION,
        receipt_data,
    );

    assert_eq!(
        original.status, cellscript.status,
        "deposit receipt-shape differential mismatch: original={:#?}, cellscript={:#?}, quantity={}, amount={}",
        original, cellscript, receipt_quantity, receipt_deposit_amount
    );
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_deposit_phase1_receipt_shape_fixture(receipt_quantity, receipt_deposit_amount, failure_mode);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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

fn deposit_phase1_receipt_amount_mismatch_differential_execution() -> Value {
    deposit_phase1_receipt_shape_differential_execution(
        1,
        deposit_phase1_unoccupied_capacity(VALID_DEPOSIT_PHASE1_CAPACITY) + 1,
        "deposit_receipt_amount_mismatch",
    )
}

fn deposit_phase1_receipt_quantity_zero_differential_execution() -> Value {
    deposit_phase1_receipt_shape_differential_execution(
        0,
        deposit_phase1_unoccupied_capacity(VALID_DEPOSIT_PHASE1_CAPACITY),
        "deposit_receipt_quantity_zero",
    )
}

fn deposit_phase1_receipt_quantity_mismatch_differential_execution() -> Value {
    deposit_phase1_receipt_shape_differential_execution(
        2,
        deposit_phase1_unoccupied_capacity(VALID_DEPOSIT_PHASE1_CAPACITY),
        "deposit_receipt_quantity_mismatch",
    )
}

fn deposit_phase1_receipt_raw_data_differential_execution(
    receipt_data: Bytes,
    scenario: &'static str,
    failure_mode: Option<&'static str>,
    expected_status: &'static str,
) -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let (original, patched_original_ickb_binary_sha256) = run_original_deposit_phase1_with_input_capacity_and_receipt_data(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        receipt_data.clone(),
    );
    let (cellscript, cellscript_elf) = run_cellscript_deposit_phase1_with_input_capacity_program_and_receipt_data(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        DEPOSIT_PHASE1_CELLSCRIPT_PROGRAM,
        DEPOSIT_PHASE1_CELLSCRIPT_ACTION,
        receipt_data.clone(),
    );

    assert_eq!(
        original.status, cellscript.status,
        "deposit receipt raw-data differential mismatch: original={:#?}, cellscript={:#?}, scenario={}",
        original, cellscript, scenario
    );
    assert_eq!(original.status, expected_status, "original iCKB status");
    assert_eq!(cellscript.status, expected_status, "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_deposit_phase1_receipt_raw_data_fixture(receipt_data, scenario, failure_mode, expected_status);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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

fn deposit_phase1_receipt_short_data_differential_execution() -> Value {
    deposit_phase1_receipt_raw_data_differential_execution(
        Bytes::from(vec![1u8, 0, 0, 0]),
        "deposit_receipt_short_data",
        Some("deposit_receipt_short_data"),
        "fail",
    )
}

fn deposit_phase1_receipt_long_data_differential_execution() -> Value {
    let mut receipt_data = deposit_phase1_receipt_data(VALID_DEPOSIT_PHASE1_CAPACITY).to_vec();
    receipt_data.push(0x99);
    deposit_phase1_receipt_raw_data_differential_execution(Bytes::from(receipt_data), "deposit_receipt_long_data", None, "pass")
}

fn deposit_phase1_dao_type_shape_differential_execution(dao_type_shape: DepositPhase1DaoTypeShape) -> Value {
    let failure_mode = dao_type_shape.failure_mode().expect("invalid DAO type shape must have failure mode");
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let receipt_data = deposit_phase1_receipt_data(VALID_DEPOSIT_PHASE1_CAPACITY);
    let (original, patched_original_ickb_binary_sha256) =
        run_original_deposit_phase1_with_input_capacity_receipt_data_and_dao_type_shape(
            VALID_DEPOSIT_PHASE1_CAPACITY,
            DEPOSIT_PHASE1_INPUT_CAPACITY,
            receipt_data.clone(),
            dao_type_shape,
        );
    let (cellscript, cellscript_elf) = run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_dao_type_shape(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        DEPOSIT_PHASE1_CELLSCRIPT_PROGRAM,
        DEPOSIT_PHASE1_CELLSCRIPT_ACTION,
        receipt_data,
        dao_type_shape,
    );

    assert_eq!(
        original.status, cellscript.status,
        "deposit DAO-type differential mismatch: original={:#?}, cellscript={:#?}, shape={:?}",
        original, cellscript, dao_type_shape
    );
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_deposit_phase1_dao_type_shape_fixture(dao_type_shape);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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

fn deposit_phase1_missing_dao_type_differential_execution() -> Value {
    deposit_phase1_dao_type_shape_differential_execution(DepositPhase1DaoTypeShape::Missing)
}

fn deposit_phase1_wrong_dao_type_differential_execution() -> Value {
    deposit_phase1_dao_type_shape_differential_execution(DepositPhase1DaoTypeShape::Wrong)
}

fn deposit_phase1_lock_shape_differential_execution(lock_shape: DepositPhase1LockShape) -> Value {
    let failure_mode = lock_shape.failure_mode().expect("invalid lock shape must have failure mode");
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let receipt_data = deposit_phase1_receipt_data(VALID_DEPOSIT_PHASE1_CAPACITY);
    let (original, patched_original_ickb_binary_sha256) = run_original_deposit_phase1_with_input_capacity_receipt_data_and_shapes(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        receipt_data.clone(),
        DepositPhase1DaoTypeShape::Valid,
        lock_shape,
    );
    let (cellscript, cellscript_elf) = run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_shapes(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        DEPOSIT_PHASE1_CELLSCRIPT_PROGRAM,
        DEPOSIT_PHASE1_CELLSCRIPT_ACTION,
        receipt_data,
        DepositPhase1DaoTypeShape::Valid,
        lock_shape,
    );

    assert_eq!(
        original.status, cellscript.status,
        "deposit lock-shape differential mismatch: original={:#?}, cellscript={:#?}, shape={:?}",
        original, cellscript, lock_shape
    );
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_deposit_phase1_lock_shape_fixture(lock_shape);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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

fn deposit_phase1_wrong_lock_differential_execution() -> Value {
    deposit_phase1_lock_shape_differential_execution(DepositPhase1LockShape::Wrong)
}

fn deposit_phase1_deposit_data_shape_differential_execution(deposit_data_shape: DepositPhase1DepositDataShape) -> Value {
    let failure_mode = deposit_data_shape.failure_mode();
    let expected_status = deposit_data_shape.expected_status();
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let receipt_data = deposit_phase1_receipt_data(VALID_DEPOSIT_PHASE1_CAPACITY);
    let (original, patched_original_ickb_binary_sha256) = run_original_deposit_phase1_with_input_capacity_receipt_data_and_all_shapes(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        receipt_data.clone(),
        DepositPhase1Shapes::new(DepositPhase1DaoTypeShape::Valid, DepositPhase1LockShape::Valid, deposit_data_shape),
    );
    let (cellscript, cellscript_elf) = run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_all_shapes(
        VALID_DEPOSIT_PHASE1_CAPACITY,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        DEPOSIT_PHASE1_CELLSCRIPT_PROGRAM,
        DEPOSIT_PHASE1_CELLSCRIPT_ACTION,
        receipt_data,
        DepositPhase1Shapes::new(DepositPhase1DaoTypeShape::Valid, DepositPhase1LockShape::Valid, deposit_data_shape),
    );

    assert_eq!(
        original.status, cellscript.status,
        "deposit data-shape differential mismatch: original={:#?}, cellscript={:#?}, shape={:?}",
        original, cellscript, deposit_data_shape
    );
    assert_eq!(original.status, expected_status, "original iCKB status");
    assert_eq!(cellscript.status, expected_status, "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_deposit_phase1_deposit_data_shape_fixture(deposit_data_shape);
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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

fn deposit_phase1_short_data_differential_execution() -> Value {
    deposit_phase1_deposit_data_shape_differential_execution(DepositPhase1DepositDataShape::Short)
}

fn deposit_phase1_nonzero_data_differential_execution() -> Value {
    deposit_phase1_deposit_data_shape_differential_execution(DepositPhase1DepositDataShape::NonZero)
}

fn deposit_phase1_long_data_differential_execution() -> Value {
    deposit_phase1_deposit_data_shape_differential_execution(DepositPhase1DepositDataShape::LongTrailingZeros)
}

fn receipt_without_deposit_differential_execution() -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let (original, patched_original_ickb_binary_sha256) = run_original_receipt_without_deposit();
    let (cellscript, cellscript_elf) = run_cellscript_receipt_without_deposit();

    assert_eq!(original.status, cellscript.status, "differential mismatch: original={:#?}, cellscript={:#?}", original, cellscript);
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_receipt_without_deposit_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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
        "failure_mode": "receipt_without_deposit_rejected",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn duplicate_receipt_output_differential_execution() -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let (original, patched_original_ickb_binary_sha256) = run_original_duplicate_receipt_output();
    let (cellscript, cellscript_elf) = run_cellscript_duplicate_receipt_output();

    assert_eq!(original.status, cellscript.status, "differential mismatch: original={:#?}, cellscript={:#?}", original, cellscript);
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_duplicate_receipt_output_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": true,
        "original_ickb_patched_binary_sha256": patched_original_ickb_binary_sha256,
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
        "failure_mode": "duplicate_receipt_output",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn receipt_group_under_mint_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_under_mint"),
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
    )
}

fn receipt_group_exact_mint_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
    )
}

fn receipt_group_over_mint_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2 + 1,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_over_mint"),
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
    )
}

fn receipt_group_amount_high_nonzero_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
        (1u128 << 64) + MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_amount_high_nonzero"),
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
    )
}

fn receipt_group_missing_header_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_missing_header_dep"),
        MintHeaderDepMode::Omitted,
        MintXudtBinding::ScriptUnderTest,
    )
}

fn receipt_group_wrong_accumulated_rate_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        WRONG_MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_wrong_accumulated_rate"),
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
    )
}

fn receipt_group_wrong_xudt_args_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_wrong_xudt_binding"),
        MintHeaderDepMode::Present,
        MintXudtBinding::WrongOwnerHash,
    )
}

fn receipt_group_malformed_receipt_data_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_malformed_receipt_data"),
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
        MintReceiptDataMode::MalformedFirstInput,
    )
}

fn receipt_group_second_malformed_receipt_data_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("receipt_group_second_malformed_receipt_data"),
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
        MintReceiptDataMode::MalformedSecondInput,
    )
}

fn receipt_group_zero_first_quantity_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
        MintReceiptDataMode::ZeroFirstQuantity,
    )
}

fn receipt_group_quantity_zero_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        MINT_RECEIPT_QUANTITY_ZERO_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
        MintReceiptDataMode::QuantityZero,
    )
}

fn receipt_group_quantity_two_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        MINT_RECEIPT_QUANTITY_TWO_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
        MintReceiptDataMode::QuantityTwo,
    )
}

fn receipt_group_mixed_quantities_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        MINT_RECEIPT_MIXED_GROUP_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
        MintReceiptDataMode::MixedQuantities,
    )
}

fn receipt_group_long_receipt_data_differential_execution() -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        MINT_RECEIPT_OUTPUT_AMOUNT * 2,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintHeaderDepMode::Present,
        MintXudtBinding::ScriptUnderTest,
        MintReceiptDataMode::LongTrailingData,
    )
}

fn receipt_group_missing_second_input_differential_execution() -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_xudt_elf = load_original_ickb_binary("xudt");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let original_xudt_binary_sha256 = sha256_prefixed(&original_xudt_elf);
    let original = run_original_receipt_group_missing_second_input();
    let (cellscript, cellscript_elf) = run_cellscript_receipt_group_missing_second_input();

    assert_eq!(
        original.status, cellscript.status,
        "receipt group missing-second-input differential mismatch: original={:#?}, cellscript={:#?}",
        original, cellscript
    );
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_receipt_group_missing_second_input_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_xudt_binary_sha256": original_xudt_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "shared_xudt_binary_sha256": original_xudt_binary_sha256,
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
        "failure_mode": "receipt_group_missing_second_input",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn receipt_group_mint_differential_execution_with_rate_header_and_xudt_binding(
    output_udt_amount: u128,
    accumulated_rate: u64,
    failure_mode: Option<&str>,
    header_dep_mode: MintHeaderDepMode,
    xudt_binding: MintXudtBinding,
) -> Value {
    receipt_group_mint_differential_execution_with_receipt_data_mode(
        output_udt_amount,
        accumulated_rate,
        failure_mode,
        header_dep_mode,
        xudt_binding,
        MintReceiptDataMode::Valid,
    )
}

fn receipt_group_mint_differential_execution_with_receipt_data_mode(
    output_udt_amount: u128,
    accumulated_rate: u64,
    failure_mode: Option<&str>,
    header_dep_mode: MintHeaderDepMode,
    xudt_binding: MintXudtBinding,
    receipt_data_mode: MintReceiptDataMode,
) -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_xudt_elf = load_original_ickb_binary("xudt");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let original_xudt_binary_sha256 = sha256_prefixed(&original_xudt_elf);
    let original =
        run_original_receipt_group_mint(output_udt_amount, accumulated_rate, header_dep_mode, xudt_binding, receipt_data_mode);
    let (cellscript, cellscript_elf) =
        run_cellscript_receipt_group_mint(output_udt_amount, accumulated_rate, header_dep_mode, xudt_binding, receipt_data_mode);

    assert_eq!(original.status, cellscript.status, "differential mismatch: original={:#?}, cellscript={:#?}", original, cellscript);
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original iCKB status");
    assert_eq!(cellscript.status, expected_status, "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_receipt_group_mint_fixture(
        output_udt_amount,
        accumulated_rate,
        failure_mode,
        header_dep_mode,
        xudt_binding,
        receipt_data_mode,
    );
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_xudt_binary_sha256": original_xudt_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "shared_xudt_binary_sha256": original_xudt_binary_sha256,
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

fn non_empty_args_differential_execution() -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let original = run_original_non_empty_args();
    let (cellscript, cellscript_elf) = run_cellscript_non_empty_args();

    assert_eq!(original.status, cellscript.status, "differential mismatch: original={:#?}, cellscript={:#?}", original, cellscript);
    assert_eq!(original.status, "fail", "original iCKB status");
    assert_eq!(cellscript.status, "fail", "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_non_empty_args_fixture();
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
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
        "failure_mode": "non_empty_args_rejected",
        "original_error": original.error,
        "cellscript_error": cellscript.error,
        "normalized_fixture": normalized_fixture
    })
}

fn mint_from_receipt_differential_execution(
    output_udt_amount: u128,
    accumulated_rate: u64,
    failure_mode: Option<&str>,
    xudt_binding: MintXudtBinding,
) -> Value {
    mint_from_receipt_differential_execution_with_header_dep(
        output_udt_amount,
        accumulated_rate,
        failure_mode,
        xudt_binding,
        MintHeaderDepMode::Present,
    )
}

fn mint_from_receipt_malformed_receipt_data_differential_execution() -> Value {
    mint_from_receipt_differential_execution_with_header_dep_and_receipt_data_mode(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("mint_malformed_receipt_data"),
        MintXudtBinding::ScriptUnderTest,
        MintHeaderDepMode::Present,
        MintReceiptDataMode::MalformedFirstInput,
    )
}

fn mint_from_receipt_quantity_zero_differential_execution() -> Value {
    mint_from_receipt_differential_execution_with_header_dep_and_receipt_data_mode(
        MINT_RECEIPT_QUANTITY_ZERO_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintXudtBinding::ScriptUnderTest,
        MintHeaderDepMode::Present,
        MintReceiptDataMode::QuantityZero,
    )
}

fn mint_from_receipt_high_word_differential_execution() -> Value {
    mint_from_receipt_differential_execution_with_header_dep_and_receipt_data_mode(
        MINT_RECEIPT_HIGH_WORD_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        Some("amount_high_nonzero"),
        MintXudtBinding::ScriptUnderTest,
        MintHeaderDepMode::Present,
        MintReceiptDataMode::Valid,
    )
}

fn mint_from_receipt_quantity_two_differential_execution() -> Value {
    mint_from_receipt_differential_execution_with_header_dep_and_receipt_data_mode(
        MINT_RECEIPT_QUANTITY_TWO_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintXudtBinding::ScriptUnderTest,
        MintHeaderDepMode::Present,
        MintReceiptDataMode::QuantityTwo,
    )
}

fn mint_from_receipt_long_data_differential_execution() -> Value {
    mint_from_receipt_differential_execution_with_header_dep_and_receipt_data_mode(
        MINT_RECEIPT_OUTPUT_AMOUNT,
        MINT_RECEIPT_ACCUMULATED_RATE,
        None,
        MintXudtBinding::ScriptUnderTest,
        MintHeaderDepMode::Present,
        MintReceiptDataMode::LongTrailingData,
    )
}

fn mint_from_receipt_differential_execution_with_header_dep(
    output_udt_amount: u128,
    accumulated_rate: u64,
    failure_mode: Option<&str>,
    xudt_binding: MintXudtBinding,
    header_dep_mode: MintHeaderDepMode,
) -> Value {
    mint_from_receipt_differential_execution_with_header_dep_and_receipt_data_mode(
        output_udt_amount,
        accumulated_rate,
        failure_mode,
        xudt_binding,
        header_dep_mode,
        MintReceiptDataMode::Valid,
    )
}

fn mint_from_receipt_differential_execution_with_header_dep_and_receipt_data_mode(
    output_udt_amount: u128,
    accumulated_rate: u64,
    failure_mode: Option<&str>,
    xudt_binding: MintXudtBinding,
    header_dep_mode: MintHeaderDepMode,
    receipt_data_mode: MintReceiptDataMode,
) -> Value {
    let original_ickb_elf = load_original_ickb_binary("ickb_logic");
    let original_xudt_elf = load_original_ickb_binary("xudt");
    let original_ickb_binary_sha256 = sha256_prefixed(&original_ickb_elf);
    let original_xudt_binary_sha256 = sha256_prefixed(&original_xudt_elf);
    let original = run_original_mint_from_receipt_with_header_dep_and_receipt_data_mode(
        output_udt_amount,
        accumulated_rate,
        xudt_binding,
        header_dep_mode,
        receipt_data_mode,
    );
    let (cellscript, cellscript_elf) = run_cellscript_mint_from_receipt_with_header_dep_and_receipt_data_mode(
        output_udt_amount,
        accumulated_rate,
        xudt_binding,
        header_dep_mode,
        receipt_data_mode,
    );

    assert_eq!(
        original.status, cellscript.status,
        "differential mismatch: original={:#?}, cellscript={:#?}, output_udt_amount={}, accumulated_rate={}, xudt_binding={:?}, header_dep_mode={:?}",
        original, cellscript, output_udt_amount, accumulated_rate, xudt_binding, header_dep_mode
    );
    let expected_status = if failure_mode.is_some() { "fail" } else { "pass" };
    assert_eq!(original.status, expected_status, "original iCKB status");
    assert_eq!(cellscript.status, expected_status, "CellScript status");
    assert_eq!(original.tx_size_bytes, cellscript.tx_size_bytes, "normalized tx sizes should match");
    assert_eq!(
        original.occupied_capacity_shannons, cellscript.occupied_capacity_shannons,
        "normalized occupied capacities should match"
    );
    assert_eq!(original.fee_shannons, cellscript.fee_shannons, "normalized fees should match");

    let normalized_fixture = normalized_mint_from_receipt_fixture_with_header_dep_and_receipt_data_mode(
        output_udt_amount,
        accumulated_rate,
        failure_mode,
        xudt_binding,
        header_dep_mode,
        receipt_data_mode,
    );
    let normalized_fixture_sha256 = sha256_json(&normalized_fixture);
    json!({
        "fixture_sha256": normalized_fixture_sha256,
        "normalized_fixture_sha256": normalized_fixture_sha256,
        "transaction_context_sha256": {
            "original": original.tx_context_sha256,
            "cellscript": cellscript.tx_context_sha256
        },
        "original_ickb_binary_sha256": original_ickb_binary_sha256,
        "original_ickb_binary_patched": false,
        "original_xudt_binary_sha256": original_xudt_binary_sha256,
        "cellscript_artifact_sha256": sha256_prefixed(&cellscript_elf),
        "shared_xudt_binary_sha256": original_xudt_binary_sha256,
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
