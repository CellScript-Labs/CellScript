#[test]
fn cellscript_ckb_script_executes_pass_with_syscall_and_fails_with_reject() {
    // Positive case: CellScript script that calls ckb::current_script_hash()
    // (real LOAD_SCRIPT_HASH syscall) and returns 0 should pass in CKB VM.
    let pass_elf = compile_cellscript_source_to_elf(VM_HARNESS_PASS_PROGRAM, VM_HARNESS_PASS_ACTION, None);

    let pass_fixture = build_simple_fixture(
        Bytes::default(), // empty script args
        1,                // one input cell
        1,                // one output cell (gets type script under test)
    );
    let pass_result = execute_cellscript_script(&pass_elf, &pass_fixture);
    assert_eq!(
        pass_result.exit_code, 0,
        "CellScript script with LOAD_SCRIPT_HASH syscall should pass in CKB VM, got exit_code={}, debug={:?}",
        pass_result.exit_code, pass_result.captured_debug
    );
    assert!(pass_result.cycles > 0, "should consume some cycles");

    // Negative case: CellScript script that returns 1 should fail in CKB VM.
    let fail_elf = compile_cellscript_source_to_elf(VM_HARNESS_FAIL_PROGRAM, VM_HARNESS_FAIL_ACTION, None);

    let fail_fixture = build_simple_fixture(
        Bytes::default(), // empty script args
        1,                // one input cell
        1,                // one output cell
    );
    let fail_result = execute_cellscript_script(&fail_elf, &fail_fixture);
    assert_eq!(
        fail_result.exit_code, 1,
        "CellScript script returning 1 should preserve script exit code, got exit_code={}, debug={:?}",
        fail_result.exit_code, fail_result.captured_debug
    );
    assert_ne!(
        fail_result.exit_code, 0,
        "CellScript script returning 1 should fail in CKB VM, got exit_code={}, debug={:?}",
        fail_result.exit_code, fail_result.captured_debug
    );
}

#[test]
fn cellscript_ckb_vm_execution_is_not_full_differential() {
    // This test documents that CELL_SCRIPT_CKB_VM_EXECUTED is NOT
    // the same as EXECUTED_CKB_VM_DIFF. The production equivalence
    // gate must not accept CellScript-only VM evidence as full
    // differential equivalence.
    let mut matrix = read_matrix();

    // If we were to mark a row as CELL_SCRIPT_CKB_VM_EXECUTED
    // but claim EXECUTED_CKB_VM_DIFF mode, the gate must reject it.
    matrix["mode"] = Value::String("EXECUTED_CKB_VM_DIFF".to_string());
    matrix["equivalence_status"] = Value::String("PROVEN".to_string());
    matrix["production_equivalence_claim"] = Value::Bool(true);

    // Mark first row as CellScript-only VM executed (not full differential)
    if let Some(rows) = matrix["rows"].as_array_mut() {
        rows[0]["evidence_level"] = Value::String("CELL_SCRIPT_CKB_VM_EXECUTED".to_string());
        rows[0]["ckb_vm_execution"] = Value::Bool(true);
        rows[0]["original_ickb_executed"] = Value::Bool(false);
        rows[0]["full_differential"] = Value::Bool(false);
        rows[0]["result"] = Value::String("cellscript-ckb-vm-pass".to_string());
    }

    let errors = validate_production_equivalence_gate(&matrix)
        .expect_err("CELL_SCRIPT_CKB_VM_EXECUTED rows must not satisfy EXECUTED_CKB_VM_DIFF");
    assert!(
        errors.iter().any(|e| e.contains("original iCKB execution")),
        "gate must reject CellScript-only VM evidence as full differential: {errors:?}"
    );
}

// ---------------------------------------------------------------------------
// DAO CKB VM execution tests
// ---------------------------------------------------------------------------
//
// These tests prove that CellScript-generated DAO scripts can execute under
// real CKB VM/syscall environment with header deps and DAO field access.
// The DAO input-accumulated-rate helper uses LOAD_HEADER (full header load),
// which reads the DAO field at absolute offset 160+8 from the serialized header.
//
// dao::accumulated_rate(source::header_dep(0)) and the input accumulated-rate
// variant both use LOAD_HEADER and parse the DAO field at absolute offset 160+8.

#[test]
fn cellscript_dao_accumulated_rate_passes_with_valid_header() {
    // Compile a CellScript program that calls dao::input_accumulated_rate(source::group_input(0))
    // which uses the real LOAD_HEADER CKB syscall to read the DAO accumulated rate
    // from the input cell's committed header.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_DAO_PASS_PROGRAM, VM_HARNESS_DAO_PASS_ACTION, None);

    // Build a fixture with a header containing DAO accumulated rate = 10000.
    // The input cell is linked to this header so LOAD_HEADER can access it.
    let fixture = build_dao_fixture(
        Bytes::default(), // empty script args
        10000,            // accumulated rate in the header DAO field
        1,                // one input cell (linked to the header)
        1,                // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript DAO accumulated_rate should pass with valid header dep, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
    assert!(result.cycles > 0, "should consume some cycles");
}

#[test]
fn cellscript_dao_accumulated_rate_fails_without_header_dep() {
    // This exercises the DAO HeaderDep helper itself: the program calls
    // dao::accumulated_rate(source::header_dep(0)), while the fixture provides
    // no header deps, so LOAD_HEADER must fail closed in the CKB VM.
    let elf =
        compile_cellscript_source_to_elf(VM_HARNESS_DAO_MISSING_HEADER_DEP_PROGRAM, VM_HARNESS_DAO_MISSING_HEADER_DEP_ACTION, None);

    let fixture = build_simple_fixture(
        Bytes::default(), // empty script args
        1,                // one input cell
        1,                // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_ne!(
        result.exit_code, 0,
        "CellScript DAO accumulated_rate should fail without header dep, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

// ---------------------------------------------------------------------------
// DAO cell classification and cell metadata CKB VM execution tests
// ---------------------------------------------------------------------------
//
// These tests prove that CellScript-generated DAO cell classification scripts
// (is_deposit_data, is_withdrawal_request_data, has_dao_type) and cell metadata
// (cell_capacity) work correctly under real CKB VM/syscall environment.
//
// - is_deposit_data: LOAD_CELL_DATA to read 8 bytes, check non-zero (deposit)
// - is_withdrawal_request_data: LOAD_CELL_DATA to read 8 bytes, check all-zero (withdrawal)
// - has_dao_type: LOAD_CELL_BY_FIELD with field=TypeHash, compare to DAO hash
// - cell_capacity: LOAD_CELL_BY_FIELD with field=Capacity

#[test]
fn cellscript_dao_is_deposit_data_passes_with_deposit_cell() {
    // Compile a CellScript program that calls dao::is_deposit_data(source::input(0))
    // which uses LOAD_CELL_DATA to read 8 bytes. A DAO deposit cell has
    // 8 non-zero bytes (deposit block number) as the first 8 bytes of data.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_DAO_IS_DEPOSIT_PROGRAM, VM_HARNESS_DAO_IS_DEPOSIT_ACTION, None);

    // Build a fixture with an input cell containing deposit data (8 zero bytes).
    // DAO deposit cells have 8 zero bytes as data (initial deposit state).
    let deposit_data = Bytes::from(vec![0u8; 8]);
    let fixture = build_dao_data_fixture(
        Bytes::default(),   // empty script args
        vec![deposit_data], // one input with deposit data
        1,                  // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript dao::is_deposit_data should pass with deposit cell, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

#[test]
fn cellscript_dao_is_withdrawal_request_data_passes_with_withdrawal_cell() {
    // Compile a CellScript program that calls dao::is_withdrawal_request_data(source::input(0))
    // which uses LOAD_CELL_DATA to read 8 bytes. A DAO withdrawal request cell has
    // 8 zero bytes as the first 8 bytes of data.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_DAO_IS_WITHDRAWAL_PROGRAM, VM_HARNESS_DAO_IS_WITHDRAWAL_ACTION, None);

    // Build a fixture with an input cell containing withdrawal request data (8 non-zero bytes).
    // DAO withdrawal request cells have 8 non-zero bytes (deposit block number) as data.
    let withdrawal_data = Bytes::from(vec![1u8, 0, 0, 0, 0, 0, 0, 0]); // deposit block number = 1
    let fixture = build_dao_data_fixture(
        Bytes::default(),      // empty script args
        vec![withdrawal_data], // one input with withdrawal data
        1,                     // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript dao::is_withdrawal_request_data should pass with withdrawal cell, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

#[test]
fn cellscript_cell_capacity_passes_with_nonzero_capacity() {
    // Compile a CellScript program that calls ckb::cell_capacity(source::input(0))
    // which uses LOAD_CELL_BY_FIELD with field=Capacity to read the cell's capacity.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_CELL_CAPACITY_PROGRAM, VM_HARNESS_CELL_CAPACITY_ACTION, None);

    // Build a fixture with an input cell having default capacity (100_000_000_000 shannons).
    let fixture = build_dao_data_fixture(
        Bytes::default(),       // empty script args
        vec![Bytes::default()], // one input with empty data (capacity is set by fixture)
        1,                      // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript ckb::cell_capacity should pass with non-zero capacity, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

#[test]
fn cellscript_dao_has_dao_type_returns_false_for_non_dao_cell() {
    // Compile a CellScript program that calls dao::has_dao_type(source::input(0))
    // which uses LOAD_CELL_BY_FIELD with field=TypeHash.
    // On a cell without the DAO type script, it should return false.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_DAO_HAS_TYPE_NEG_PROGRAM, VM_HARNESS_DAO_HAS_TYPE_NEG_ACTION, None);

    // Build a fixture with an input cell that has no DAO type script.
    let fixture = build_dao_data_fixture(
        Bytes::default(),       // empty script args
        vec![Bytes::default()], // one input with empty data and no type script
        1,                      // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript dao::has_dao_type should return false for non-DAO cell, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

// ---------------------------------------------------------------------------
// Cell metadata VM execution tests (occupied capacity, data size)
// ---------------------------------------------------------------------------
//
// These tests prove that CellScript-generated cell metadata scripts work under
// real CKB VM/syscall environment:
//
// - cell_occupied_capacity: LOAD_CELL_BY_FIELD(OccupiedCapacity)
// - cell_data_size: LOAD_CELL_DATA size probe

#[test]
fn cellscript_cell_occupied_capacity_passes_with_lock_script() {
    // Compile a CellScript program that calls ckb::cell_occupied_capacity(source::input(0))
    // which uses LOAD_CELL_BY_FIELD(OccupiedCapacity) to read the cell's
    // occupied capacity in shannons.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_OCCUPIED_CAPACITY_PROGRAM, VM_HARNESS_OCCUPIED_CAPACITY_ACTION, None);

    // Build a fixture with an input cell having a lock script and no type script.
    let fixture = build_dao_data_fixture(
        Bytes::default(),       // empty script args
        vec![Bytes::default()], // one input with empty data
        1,                      // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript ckb::cell_occupied_capacity should pass, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

#[test]
fn cellscript_cell_data_size_passes_with_data() {
    // Compile a CellScript program that calls ckb::cell_data_size(source::input(0))
    // which uses LOAD_CELL_DATA to probe the cell data byte length.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_CELL_DATA_SIZE_PROGRAM, VM_HARNESS_CELL_DATA_SIZE_ACTION, None);

    // Build a fixture with an input cell containing 8 bytes of data.
    let data = Bytes::from(vec![0u8; 8]);
    let fixture = build_dao_data_fixture(
        Bytes::default(), // empty script args
        vec![data],       // one input with 8 bytes of data
        1,                // one output cell
    );
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript ckb::cell_data_size should pass, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

#[test]
fn cellscript_cell_dep_data_size_passes_with_fixture_cell_dep() {
    // This guards the CkbVmFixture.cell_deps contract: the dependency must be
    // present in the transaction CellDep list, not only deployed in the context.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_CELL_DEP_DATA_SIZE_PROGRAM, VM_HARNESS_CELL_DEP_DATA_SIZE_ACTION, None);

    let mut fixture = build_simple_fixture(
        Bytes::default(), // empty script args
        1,                // one input cell
        1,                // one output cell
    );
    fixture.cell_deps.push(FixtureCell { capacity: 0, type_script: None, data: Bytes::from(vec![1, 2, 3, 4]) });

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript ckb::cell_data_size should read fixture CellDep data, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
    assert!(result.cycles > 0, "should consume some cycles");
}

#[test]
fn cellscript_witness_args_empty_lock_passes_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_WITNESS_ARGS_PROGRAM, VM_HARNESS_WITNESS_ARGS_ACTION, None);

    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![molecule_witness_args(None, None, None)];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "Complete raw witness reads should preserve an empty WitnessArgs table, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
    assert!(result.cycles > 0, "should consume some cycles");
}

#[test]
fn cellscript_require_witness_size_at_least_rejects_too_small_in_ckb_vm() {
    let elf =
        compile_cellscript_source_to_elf(VM_HARNESS_WITNESS_SIZE_TOO_SMALL_PROGRAM, VM_HARNESS_WITNESS_SIZE_TOO_SMALL_ACTION, None);

    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![molecule_witness_args(None, None, None)];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::WitnessMalformed.code() as i64,
        "require_witness_size_at_least should fail closed when min_size exceeds actual witness size, got exit_code={}, debug={:?}",
        result.exit_code,
        result.captured_debug
    );
}

#[test]
fn cellscript_witness_args_short_lock_rejects_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_WITNESS_SHORT_LOCK_PROGRAM, VM_HARNESS_WITNESS_SHORT_LOCK_ACTION, None);

    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![molecule_witness_args(Some(&[0u8][..]), None, None)];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::ExactSizeMismatch.code() as i64,
        "exact witness lock must reject a short field, got exit_code={}, debug={:?}",
        result.exit_code,
        result.captured_debug
    );
}

#[test]
fn cellscript_typed_witness_args_short_lock_rejects_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(
        VM_HARNESS_WITNESS_TYPED_SHORT_LOCK_PROGRAM,
        VM_HARNESS_WITNESS_TYPED_SHORT_LOCK_ACTION,
        None,
    );

    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![molecule_witness_args(Some(&[0u8][..]), None, None)];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::ExactSizeMismatch.code() as i64,
        "typed WitnessArgsView.lock must reject a non-32-byte field, got exit_code={}, debug={:?}",
        result.exit_code,
        result.captured_debug
    );

    fixture.witnesses = vec![molecule_witness_args(Some(&[0u8; 33][..]), None, None)];
    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::ExactSizeMismatch.code() as i64,
        "typed WitnessArgsView.lock must reject a field wider than 32 bytes with the same exact-size error, got exit_code={}, debug={:?}",
        result.exit_code,
        result.captured_debug
    );
}

#[test]
fn cellscript_typed_witness_args_exact_field_streams_past_large_sibling_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(
        VM_HARNESS_WITNESS_TYPED_LOCK_LARGE_SIBLING_PROGRAM,
        VM_HARNESS_WITNESS_TYPED_LOCK_LARGE_SIBLING_ACTION,
        None,
    );

    let lock = [0x11u8; 32];
    let input_type = [0x22u8; 700];
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![ckb_packed_witness_args(Some(&lock), Some(&input_type), None)];
    assert!(fixture.witnesses[0].len() > 512, "fixture must exceed the retired exact-field staging buffer");

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "a fixed 32-byte projection must not depend on the total WitnessArgs size, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
}

#[test]
fn cellscript_witness_args_lock_input_type_output_type_are_isolated_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_WITNESS_TYPED_FIELDS_PROGRAM, VM_HARNESS_WITNESS_TYPED_FIELDS_ACTION, None);

    let lock = [0x11u8; 32];
    let input_type = [0x22u8; 32];
    let output_type = [0x33u8; 32];
    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![ckb_packed_witness_args(Some(&lock), Some(&input_type), Some(&output_type))];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "WitnessArgs lock/input_type/output_type should load as distinct non-zero Hash buffers, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
    assert!(result.cycles > 0, "should consume some cycles");
}

#[test]
fn cellscript_witness_args_total_size_mismatch_rejects_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_WITNESS_MALFORMED_PROGRAM, VM_HARNESS_WITNESS_MALFORMED_ACTION, None);

    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![molecule_witness_args_with_header(17, [16, 16, 16], &[])];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::WitnessMalformed.code() as i64,
        "WitnessArgs total_size mismatch should fail closed as WitnessMalformed, got exit_code={}, debug={:?}",
        result.exit_code,
        result.captured_debug
    );
}

#[test]
fn cellscript_witness_args_reordered_offsets_reject_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_WITNESS_MALFORMED_PROGRAM, VM_HARNESS_WITNESS_MALFORMED_ACTION, None);

    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![molecule_witness_args_with_header(16, [16, 12, 16], &[])];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::WitnessMalformed.code() as i64,
        "WitnessArgs reordered offsets should fail closed as WitnessMalformed, got exit_code={}, debug={:?}",
        result.exit_code,
        result.captured_debug
    );
}

#[test]
fn cellscript_witness_args_truncated_offsets_reject_in_ckb_vm() {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_WITNESS_MALFORMED_PROGRAM, VM_HARNESS_WITNESS_MALFORMED_ACTION, None);

    let mut fixture = build_simple_fixture(Bytes::default(), 1, 1);
    fixture.witnesses = vec![molecule_witness_args_with_header(16, [16, 16, 17], &[])];

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code,
        cellscript::runtime_errors::CellScriptRuntimeError::WitnessFieldTruncated.code() as i64,
        "WitnessArgs offset beyond total_size should fail closed as WitnessFieldTruncated, got exit_code={}, debug={:?}",
        result.exit_code,
        result.captured_debug
    );
}

fn molecule_witness_args(lock: Option<&[u8]>, input_type: Option<&[u8]>, output_type: Option<&[u8]>) -> Bytes {
    let fields = [molecule_bytes_opt(lock), molecule_bytes_opt(input_type), molecule_bytes_opt(output_type)];
    let header_size = 16usize;
    let mut offset = header_size;
    let mut offsets = Vec::with_capacity(fields.len());
    for field in &fields {
        offsets.push(offset as u32);
        offset += field.len();
    }

    let mut out = Vec::with_capacity(offset);
    out.extend_from_slice(&(offset as u32).to_le_bytes());
    for field_offset in offsets {
        out.extend_from_slice(&field_offset.to_le_bytes());
    }
    for field in fields {
        out.extend_from_slice(&field);
    }
    Bytes::from(out)
}

fn molecule_witness_args_with_header(total_size: u32, offsets: [u32; 3], payload: &[u8]) -> Bytes {
    let mut out = Vec::with_capacity(16 + payload.len());
    out.extend_from_slice(&total_size.to_le_bytes());
    for offset in offsets {
        out.extend_from_slice(&offset.to_le_bytes());
    }
    out.extend_from_slice(payload);
    Bytes::from(out)
}

fn ckb_packed_witness_args(lock: Option<&[u8]>, input_type: Option<&[u8]>, output_type: Option<&[u8]>) -> Bytes {
    let mut builder = packed::WitnessArgs::new_builder();
    if let Some(bytes) = lock {
        builder = builder.lock(Some(Bytes::copy_from_slice(bytes)).pack());
    }
    if let Some(bytes) = input_type {
        builder = builder.input_type(Some(Bytes::copy_from_slice(bytes)).pack());
    }
    if let Some(bytes) = output_type {
        builder = builder.output_type(Some(Bytes::copy_from_slice(bytes)).pack());
    }
    builder.build().as_bytes()
}

fn molecule_bytes_opt(value: Option<&[u8]>) -> Vec<u8> {
    let Some(bytes) = value else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(4 + bytes.len());
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
    out
}

// ---------------------------------------------------------------------------
// Combined iCKB deposit scenario VM execution test
// ---------------------------------------------------------------------------
//
// This test proves that CellScript-generated scripts can exercise multiple
// CKB syscalls in a single script execution, simulating the core iCKB
// deposit verification path:
//
// 1. LOAD_CELL_DATA (is_deposit_data) to classify the input cell
// 2. LOAD_CELL_BY_FIELD (cell_capacity) to read the cell's capacity
// 3. LOAD_HEADER (input_accumulated_rate) to read DAO accumulated rate
//
// This is a significant milestone: it demonstrates that the CellScript
// runtime can orchestrate multiple syscall interactions correctly
// within a single CKB VM execution.

#[test]
fn cellscript_ickb_deposit_verification_passes_with_valid_dao_deposit() {
    // Compile a CellScript program that combines multiple iCKB-relevant syscalls:
    // is_deposit_data + cell_capacity + input_accumulated_rate.
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_ICKB_DEPOSIT_PROGRAM, VM_HARNESS_ICKB_DEPOSIT_ACTION, None);

    // Build a DAO fixture with:
    // - Input cell with 8 zero bytes (DAO deposit data marker)
    // - Header with DAO accumulated rate = 10000
    // - Input linked to the header for LOAD_HEADER access
    let mut fixture = build_dao_fixture(
        Bytes::default(), // empty script args
        10000,            // accumulated rate
        1,                // one input cell
        1,                // one output cell
    );
    // Set the input cell data to 8 zero bytes (DAO deposit marker).
    // The build_dao_fixture creates empty data, so we need to set deposit data.
    fixture.inputs[0].data = Bytes::from(vec![0u8; 8]);

    let result = execute_cellscript_script(&elf, &fixture);
    assert_eq!(
        result.exit_code, 0,
        "CellScript iCKB deposit verification should pass with valid DAO deposit, got exit_code={}, debug={:?}",
        result.exit_code, result.captured_debug
    );
    assert!(result.cycles > 0, "should consume some cycles");
}

#[test]
fn cellscript_immature_redeem_relative_since_rejects_in_ckb_vm() {
    const { assert!(IMMATURE_REDEEM_INPUT_EPOCH < IMMATURE_REDEEM_REQUIRED_EPOCH) };
    let run = run_cellscript_redeem_relative_since(IMMATURE_REDEEM_INPUT_EPOCH);
    assert_eq!(run.status, "fail", "immature redeem since must reject: {run:#?}");
    assert_eq!(run.exit_code, 36, "CellScript DAO maturity violation exit code: {run:#?}");
}

#[test]
fn cellscript_mature_redeem_relative_since_passes_in_ckb_vm() {
    const { assert!(MATURE_REDEEM_INPUT_EPOCH >= IMMATURE_REDEEM_REQUIRED_EPOCH) };
    let run = run_cellscript_redeem_relative_since(MATURE_REDEEM_INPUT_EPOCH);
    assert_eq!(run.status, "pass", "mature redeem since must pass: {run:#?}");
    assert_eq!(run.exit_code, 0, "CellScript mature DAO since exit code: {run:#?}");
    assert!(run.cycles > 0, "mature redeem since must consume cycles: {run:#?}");
}

#[test]
fn original_dao_binary_creates_withdrawing_cell_in_ckb_vm() {
    let run = run_original_dao_create_withdrawing_cell();
    assert_eq!(run.status, "pass", "original DAO create-withdrawing-cell should pass: {run:#?}");
    assert_eq!(run.exit_code, 0, "original DAO create-withdrawing-cell exit code: {run:#?}");
    assert!(run.cycles > 0, "original DAO create-withdrawing-cell must consume cycles: {run:#?}");
}

#[test]
fn original_dao_binary_mature_withdrawal_passes_in_ckb_vm() {
    let run = run_original_dao_withdrawal(ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE, ORIGINAL_DAO_WITHDRAW_PHASE2_OUTPUT_CAPACITY);
    assert_eq!(run.status, "pass", "original DAO mature withdrawal should pass: {run:#?}");
    assert_eq!(run.exit_code, 0, "original DAO mature withdrawal exit code: {run:#?}");
    assert!(run.cycles > 0, "original DAO mature withdrawal must consume cycles: {run:#?}");
}

#[test]
fn original_dao_binary_immature_withdrawal_rejects_in_ckb_vm() {
    let run = run_original_dao_withdrawal(
        ORIGINAL_DAO_WITHDRAW_PHASE2_IMMATURE_SINCE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_IMMATURE_OUTPUT_CAPACITY,
    );
    assert_eq!(run.status, "fail", "original DAO immature withdrawal should reject: {run:#?}");
    assert_eq!(run.exit_code, -17, "original DAO immature withdrawal should reject with ERROR_INCORRECT_SINCE: {run:#?}");
}

fn run_cellscript_redeem_relative_since(input_epoch: u64) -> DepositPhase1SideRun {
    let elf = compile_cellscript_source_to_elf(IMMATURE_REDEEM_CELLSCRIPT_PROGRAM, IMMATURE_REDEEM_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript redeem since script");
    let always_success_lock = deploy_always_success_lock(&mut context);

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(IMMATURE_REDEEM_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script))
            .build(),
        owned_owner_withdrawal_request_data(),
    );

    let input_since = ckb_epoch_relative_since(input_epoch, 0, 1);
    let input = packed::CellInput::new_builder().previous_output(input_out_point).since(input_since).build();
    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(IMMATURE_REDEEM_CAPACITY.pack())
        .lock(always_success_lock)
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(input)
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);

    side_run_from_result(
        context.verify_tx(&tx, IMMATURE_REDEEM_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(IMMATURE_REDEEM_CAPACITY, &outputs),
    )
}

fn run_original_dao_create_withdrawing_cell() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let dao_elf = load_original_ickb_binary("dao");
    let dao_code_out_point = context.deploy_cell(Bytes::copy_from_slice(&dao_elf));
    let dao_script = context.build_script(&dao_code_out_point, Bytes::default()).expect("original DAO script");
    let always_success_lock = deploy_always_success_lock(&mut context);

    let deposit_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
        ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH,
    );
    let deposit_header_hash = deposit_header.hash();
    context.insert_header(deposit_header);

    let deposit_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(dao_script.clone()))
            .build(),
        Bytes::from(vec![0u8; 8]),
    );
    context.link_cell_with_block(deposit_out_point.clone(), deposit_header_hash.clone(), 0);

    let withdrawing_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
        .lock(always_success_lock)
        .type_(packed::ScriptOpt::from(dao_script))
        .build();
    let withdrawing_data = Bytes::from(ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK.to_le_bytes().to_vec());
    let outputs = vec![withdrawing_output];
    let outputs_data = vec![withdrawing_data];
    let witness = packed::WitnessArgs::new_builder().build();
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(deposit_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .output(outputs[0].clone())
        .output_data(outputs_data[0].clone().pack())
        .header_dep(deposit_header_hash)
        .witness(witness.as_bytes().pack())
        .build();
    let tx = context.complete_tx(tx);

    side_run_from_result(
        context.verify_tx(&tx, ORIGINAL_DAO_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY, &outputs),
    )
}

fn run_original_dao_withdrawal(input_since: u64, output_capacity: u64) -> DepositPhase1SideRun {
    run_original_dao_withdrawal_with_header_dep_mode(input_since, output_capacity, DaoWithdrawalHeaderDepMode::Present)
}
