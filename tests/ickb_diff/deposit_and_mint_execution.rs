fn run_original_deposit_phase1(deposit_capacity: u64) -> (DepositPhase1SideRun, String) {
    run_original_deposit_phase1_with_input_capacity(deposit_capacity, DEPOSIT_PHASE1_INPUT_CAPACITY)
}

fn run_original_deposit_phase1_with_input_capacity(deposit_capacity: u64, input_capacity: u64) -> (DepositPhase1SideRun, String) {
    run_original_deposit_phase1_with_input_capacity_and_receipt_data(
        deposit_capacity,
        input_capacity,
        deposit_phase1_receipt_data(deposit_capacity),
    )
}

fn run_original_deposit_phase1_with_input_capacity_and_receipt_data(
    deposit_capacity: u64,
    input_capacity: u64,
    receipt_data: Bytes,
) -> (DepositPhase1SideRun, String) {
    run_original_deposit_phase1_with_input_capacity_receipt_data_and_all_shapes(
        deposit_capacity,
        input_capacity,
        receipt_data,
        DepositPhase1Shapes::VALID,
    )
}

fn run_original_deposit_phase1_with_input_capacity_receipt_data_and_dao_type_shape(
    deposit_capacity: u64,
    input_capacity: u64,
    receipt_data: Bytes,
    dao_type_shape: DepositPhase1DaoTypeShape,
) -> (DepositPhase1SideRun, String) {
    run_original_deposit_phase1_with_input_capacity_receipt_data_and_shapes(
        deposit_capacity,
        input_capacity,
        receipt_data,
        dao_type_shape,
        DepositPhase1LockShape::Valid,
    )
}

fn run_original_deposit_phase1_with_input_capacity_receipt_data_and_shapes(
    deposit_capacity: u64,
    input_capacity: u64,
    receipt_data: Bytes,
    dao_type_shape: DepositPhase1DaoTypeShape,
    lock_shape: DepositPhase1LockShape,
) -> (DepositPhase1SideRun, String) {
    run_original_deposit_phase1_with_input_capacity_receipt_data_and_all_shapes(
        deposit_capacity,
        input_capacity,
        receipt_data,
        DepositPhase1Shapes::new(dao_type_shape, lock_shape, DepositPhase1DepositDataShape::Valid),
    )
}

fn run_original_deposit_phase1_with_input_capacity_receipt_data_and_all_shapes(
    deposit_capacity: u64,
    input_capacity: u64,
    receipt_data: Bytes,
    shapes: DepositPhase1Shapes,
) -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let (dao_script, ickb_logic_script, dao_code_out_point) = setup_ickb_test_env(&mut context);
    let always_success_lock = deploy_always_success_lock(&mut context);
    let mut patched_ickb_logic_elf = load_original_ickb_binary("ickb_logic");
    let test_dao_hash: [u8; 32] = dao_script.calc_script_hash().unpack();
    patch_ickb_logic_dao_hash(&mut patched_ickb_logic_elf, &test_dao_hash);
    let patched_ickb_logic_sha256 = sha256_prefixed(&patched_ickb_logic_elf);

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(input_capacity.pack()).lock(always_success_lock.clone()).build(),
        Bytes::default(),
    );

    let (outputs, outputs_data) = deposit_phase1_outputs_with_receipt_data(
        deposit_capacity,
        receipt_data,
        shapes,
        &ickb_logic_script,
        &dao_script,
        &always_success_lock,
    );
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(input_capacity, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, DEPOSIT_PHASE1_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_ickb_logic_sha256)
}

fn run_cellscript_deposit_phase1(deposit_capacity: u64) -> (DepositPhase1SideRun, Vec<u8>) {
    run_cellscript_deposit_phase1_with_input_capacity_and_program(
        deposit_capacity,
        DEPOSIT_PHASE1_INPUT_CAPACITY,
        DEPOSIT_PHASE1_CELLSCRIPT_PROGRAM,
        DEPOSIT_PHASE1_CELLSCRIPT_ACTION,
    )
}

fn run_cellscript_deposit_phase1_upper_bound(deposit_capacity: u64, input_capacity: u64) -> (DepositPhase1SideRun, Vec<u8>) {
    run_cellscript_deposit_phase1_with_input_capacity_and_program(
        deposit_capacity,
        input_capacity,
        DEPOSIT_PHASE1_UPPER_BOUND_CELLSCRIPT_PROGRAM,
        DEPOSIT_PHASE1_UPPER_BOUND_CELLSCRIPT_ACTION,
    )
}

fn run_cellscript_deposit_phase1_with_input_capacity_and_program(
    deposit_capacity: u64,
    input_capacity: u64,
    program: &str,
    action: &str,
) -> (DepositPhase1SideRun, Vec<u8>) {
    run_cellscript_deposit_phase1_with_input_capacity_program_and_receipt_data(
        deposit_capacity,
        input_capacity,
        program,
        action,
        deposit_phase1_receipt_data(deposit_capacity),
    )
}

fn run_cellscript_deposit_phase1_with_input_capacity_program_and_receipt_data(
    deposit_capacity: u64,
    input_capacity: u64,
    program: &str,
    action: &str,
    receipt_data: Bytes,
) -> (DepositPhase1SideRun, Vec<u8>) {
    run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_all_shapes(
        deposit_capacity,
        input_capacity,
        program,
        action,
        receipt_data,
        DepositPhase1Shapes::VALID,
    )
}

fn run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_dao_type_shape(
    deposit_capacity: u64,
    input_capacity: u64,
    program: &str,
    action: &str,
    receipt_data: Bytes,
    dao_type_shape: DepositPhase1DaoTypeShape,
) -> (DepositPhase1SideRun, Vec<u8>) {
    run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_shapes(
        deposit_capacity,
        input_capacity,
        program,
        action,
        receipt_data,
        dao_type_shape,
        DepositPhase1LockShape::Valid,
    )
}

fn run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_shapes(
    deposit_capacity: u64,
    input_capacity: u64,
    program: &str,
    action: &str,
    receipt_data: Bytes,
    dao_type_shape: DepositPhase1DaoTypeShape,
    lock_shape: DepositPhase1LockShape,
) -> (DepositPhase1SideRun, Vec<u8>) {
    run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_all_shapes(
        deposit_capacity,
        input_capacity,
        program,
        action,
        receipt_data,
        DepositPhase1Shapes::new(dao_type_shape, lock_shape, DepositPhase1DepositDataShape::Valid),
    )
}

fn run_cellscript_deposit_phase1_with_input_capacity_program_receipt_data_and_all_shapes(
    deposit_capacity: u64,
    input_capacity: u64,
    program: &str,
    action: &str,
    receipt_data: Bytes,
    shapes: DepositPhase1Shapes,
) -> (DepositPhase1SideRun, Vec<u8>) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let dao_elf = load_original_ickb_binary("dao");
    let dao_code_out_point = context.deploy_cell(Bytes::copy_from_slice(&dao_elf));
    let dao_script = context.build_script(&dao_code_out_point, Bytes::default()).expect("DAO script");
    let program = deposit_phase1_program_with_expected_dao_script(program, &dao_script);
    let cellscript_elf = compile_cellscript_source_to_elf(&program, action, None);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let always_success_lock = deploy_always_success_lock(&mut context);

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(input_capacity.pack()).lock(always_success_lock.clone()).build(),
        Bytes::default(),
    );

    let (outputs, outputs_data) = deposit_phase1_outputs_with_receipt_data(
        deposit_capacity,
        receipt_data,
        shapes,
        &cellscript_script,
        &dao_script,
        &always_success_lock,
    );
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(input_capacity, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, DEPOSIT_PHASE1_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn deposit_phase1_program_with_expected_dao_script(program: &str, dao_script: &packed::Script) -> String {
    program.replace("__EXPECTED_DAO_TYPE_SCRIPT__", &cellscript_script_value_expr(dao_script))
}

fn run_original_duplicate_receipt_output() -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let (dao_script, ickb_logic_script, dao_code_out_point) = setup_ickb_test_env(&mut context);
    let always_success_lock = deploy_always_success_lock(&mut context);
    let mut patched_ickb_logic_elf = load_original_ickb_binary("ickb_logic");
    let test_dao_hash: [u8; 32] = dao_script.calc_script_hash().unpack();
    patch_ickb_logic_dao_hash(&mut patched_ickb_logic_elf, &test_dao_hash);
    let patched_ickb_logic_sha256 = sha256_prefixed(&patched_ickb_logic_elf);

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(DEPOSIT_PHASE1_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let (outputs, outputs_data) = duplicate_receipt_output_outputs(&ickb_logic_script, &dao_script, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(DEPOSIT_PHASE1_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, DEPOSIT_PHASE1_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_ickb_logic_sha256)
}

fn run_cellscript_duplicate_receipt_output() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(
        DUPLICATE_RECEIPT_OUTPUT_CELLSCRIPT_PROGRAM,
        DUPLICATE_RECEIPT_OUTPUT_CELLSCRIPT_ACTION,
        None,
    );
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let dao_elf = load_original_ickb_binary("dao");
    let dao_code_out_point = context.deploy_cell(Bytes::copy_from_slice(&dao_elf));
    let dao_script = context.build_script(&dao_code_out_point, Bytes::default()).expect("DAO script");
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let always_success_lock = deploy_always_success_lock(&mut context);

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(DEPOSIT_PHASE1_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let (outputs, outputs_data) = duplicate_receipt_output_outputs(&cellscript_script, &dao_script, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(DEPOSIT_PHASE1_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, DEPOSIT_PHASE1_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_receipt_without_deposit() -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let (dao_script, ickb_logic_script, _dao_code_out_point) = setup_ickb_test_env(&mut context);
    let always_success_lock = deploy_always_success_lock(&mut context);
    let mut patched_ickb_logic_elf = load_original_ickb_binary("ickb_logic");
    let test_dao_hash: [u8; 32] = dao_script.calc_script_hash().unpack();
    patch_ickb_logic_dao_hash(&mut patched_ickb_logic_elf, &test_dao_hash);
    let patched_ickb_logic_sha256 = sha256_prefixed(&patched_ickb_logic_elf);

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(RECEIPT_WITHOUT_DEPOSIT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );
    let (outputs, outputs_data) = receipt_without_deposit_outputs(&ickb_logic_script, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(RECEIPT_WITHOUT_DEPOSIT_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(
        context.verify_tx(&tx, RECEIPT_WITHOUT_DEPOSIT_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons,
        fee_shannons,
    );
    (run, patched_ickb_logic_sha256)
}

fn run_cellscript_receipt_without_deposit() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(RECEIPT_WITHOUT_DEPOSIT_CELLSCRIPT_PROGRAM, RECEIPT_WITHOUT_DEPOSIT_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(RECEIPT_WITHOUT_DEPOSIT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );
    let (outputs, outputs_data) = receipt_without_deposit_outputs(&cellscript_script, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(RECEIPT_WITHOUT_DEPOSIT_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(
        context.verify_tx(&tx, RECEIPT_WITHOUT_DEPOSIT_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons,
        fee_shannons,
    );
    (run, cellscript_elf)
}

fn run_original_non_empty_args() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let ickb_logic_elf = load_original_ickb_binary("ickb_logic");
    let ickb_logic_out_point = context.deploy_cell(Bytes::copy_from_slice(&ickb_logic_elf));
    let ickb_logic_nonempty =
        context.build_script(&ickb_logic_out_point, non_empty_script_args()).expect("iCKB Logic with non-empty args");

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(NON_EMPTY_ARGS_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );
    let (outputs, outputs_data) = non_empty_args_outputs(&ickb_logic_nonempty, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(NON_EMPTY_ARGS_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, NON_EMPTY_ARGS_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_non_empty_args() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(NON_EMPTY_ARGS_CELLSCRIPT_PROGRAM, NON_EMPTY_ARGS_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_nonempty =
        context.build_script(&cellscript_out_point, non_empty_script_args()).expect("CellScript with non-empty args");

    let input_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(NON_EMPTY_ARGS_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );
    let (outputs, outputs_data) = non_empty_args_outputs(&cellscript_nonempty, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(NON_EMPTY_ARGS_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, NON_EMPTY_ARGS_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_mint_from_receipt_with_header_dep_and_receipt_data_mode(
    output_udt_amount: u128,
    accumulated_rate: u64,
    xudt_binding: MintXudtBinding,
    header_dep_mode: MintHeaderDepMode,
    receipt_data_mode: MintReceiptDataMode,
) -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let ickb_logic_elf = load_original_ickb_binary("ickb_logic");
    let ickb_logic_out_point = context.deploy_cell(Bytes::copy_from_slice(&ickb_logic_elf));
    let ickb_logic_script = context.build_script(&ickb_logic_out_point, Bytes::default()).expect("iCKB Logic script");
    let xudt_elf = load_original_ickb_binary("xudt");
    let xudt_out_point = context.deploy_cell(Bytes::copy_from_slice(&xudt_elf));
    let xudt_script = build_xudt_owner_mode_script(&mut context, &xudt_out_point, &ickb_logic_script, xudt_binding);

    let receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(ickb_logic_script))
            .build(),
        receipt_group_input_data(receipt_data_mode, 0),
    );
    let header_hash = insert_and_link_mint_receipt_header(&mut context, &receipt_out_point, accumulated_rate);

    let (outputs, outputs_data) = mint_from_receipt_outputs(output_udt_amount, &xudt_script, &always_success_lock);
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(receipt_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(xudt_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack());
    if header_dep_mode == MintHeaderDepMode::Present {
        tx_builder = tx_builder.header_dep(header_hash);
    }
    let tx = tx_builder.build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(MINT_RECEIPT_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, MINT_FROM_RECEIPT_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_mint_from_receipt_with_header_dep_and_receipt_data_mode(
    output_udt_amount: u128,
    accumulated_rate: u64,
    xudt_binding: MintXudtBinding,
    header_dep_mode: MintHeaderDepMode,
    receipt_data_mode: MintReceiptDataMode,
) -> (DepositPhase1SideRun, Vec<u8>) {
    let (program, action) = match receipt_data_mode {
        MintReceiptDataMode::Valid
        | MintReceiptDataMode::QuantityZero
        | MintReceiptDataMode::QuantityTwo
        | MintReceiptDataMode::ZeroFirstQuantity
        | MintReceiptDataMode::MixedQuantities
        | MintReceiptDataMode::LongTrailingData
        | MintReceiptDataMode::MalformedFirstInput => {
            (MINT_FROM_RECEIPT_RECEIPT_DATA_SIZE_CELLSCRIPT_PROGRAM, MINT_FROM_RECEIPT_RECEIPT_DATA_SIZE_CELLSCRIPT_ACTION)
        }
        MintReceiptDataMode::MalformedSecondInput => (MINT_FROM_RECEIPT_CELLSCRIPT_PROGRAM, MINT_FROM_RECEIPT_CELLSCRIPT_ACTION),
    };
    let cellscript_elf = compile_cellscript_source_to_elf(program, action, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let xudt_elf = load_original_ickb_binary("xudt");
    let xudt_out_point = context.deploy_cell(Bytes::copy_from_slice(&xudt_elf));
    let xudt_script = build_xudt_owner_mode_script(&mut context, &xudt_out_point, &cellscript_script, xudt_binding);

    let receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script))
            .build(),
        receipt_group_input_data(receipt_data_mode, 0),
    );
    let header_hash = insert_and_link_mint_receipt_header(&mut context, &receipt_out_point, accumulated_rate);

    let (outputs, outputs_data) = mint_from_receipt_outputs(output_udt_amount, &xudt_script, &always_success_lock);
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(receipt_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(xudt_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack());
    if header_dep_mode == MintHeaderDepMode::Present {
        tx_builder = tx_builder.header_dep(header_hash);
    }
    let tx = tx_builder.build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(MINT_RECEIPT_INPUT_CAPACITY, &outputs);
    let run =
        side_run_from_result(context.verify_tx(&tx, MINT_FROM_RECEIPT_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_receipt_group_mint(
    output_udt_amount: u128,
    accumulated_rate: u64,
    header_dep_mode: MintHeaderDepMode,
    xudt_binding: MintXudtBinding,
    receipt_data_mode: MintReceiptDataMode,
) -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let ickb_logic_elf = load_original_ickb_binary("ickb_logic");
    let ickb_logic_out_point = context.deploy_cell(Bytes::copy_from_slice(&ickb_logic_elf));
    let ickb_logic_script = context.build_script(&ickb_logic_out_point, Bytes::default()).expect("iCKB Logic script");
    let xudt_elf = load_original_ickb_binary("xudt");
    let xudt_out_point = context.deploy_cell(Bytes::copy_from_slice(&xudt_elf));
    let xudt_script = build_xudt_owner_mode_script(&mut context, &xudt_out_point, &ickb_logic_script, xudt_binding);

    let first_receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(ickb_logic_script.clone()))
            .build(),
        receipt_group_input_data(receipt_data_mode, 0),
    );
    let second_receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(ickb_logic_script))
            .build(),
        receipt_group_input_data(receipt_data_mode, 1),
    );
    let header_hash = insert_and_link_mint_receipt_header(&mut context, &first_receipt_out_point, accumulated_rate);
    context.link_cell_with_block(second_receipt_out_point.clone(), header_hash.clone(), 0);

    let (outputs, outputs_data) = mint_from_receipt_outputs(output_udt_amount, &xudt_script, &always_success_lock);
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(first_receipt_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(second_receipt_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(xudt_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack());
    if header_dep_mode == MintHeaderDepMode::Present {
        tx_builder = tx_builder.header_dep(header_hash);
    }
    let tx = tx_builder.build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(MINT_RECEIPT_INPUT_CAPACITY * 2, &outputs);
    side_run_from_result(context.verify_tx(&tx, MINT_FROM_RECEIPT_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_receipt_group_mint(
    output_udt_amount: u128,
    accumulated_rate: u64,
    header_dep_mode: MintHeaderDepMode,
    xudt_binding: MintXudtBinding,
    receipt_data_mode: MintReceiptDataMode,
) -> (DepositPhase1SideRun, Vec<u8>) {
    let (program, action) = match receipt_data_mode {
        MintReceiptDataMode::Valid
        | MintReceiptDataMode::QuantityZero
        | MintReceiptDataMode::QuantityTwo
        | MintReceiptDataMode::ZeroFirstQuantity
        | MintReceiptDataMode::MixedQuantities
        | MintReceiptDataMode::LongTrailingData
        | MintReceiptDataMode::MalformedFirstInput
        | MintReceiptDataMode::MalformedSecondInput => {
            (RECEIPT_GROUP_RECEIPT_DATA_SIZE_CELLSCRIPT_PROGRAM, RECEIPT_GROUP_RECEIPT_DATA_SIZE_CELLSCRIPT_ACTION)
        }
    };
    let cellscript_elf = compile_cellscript_source_to_elf(program, action, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let xudt_elf = load_original_ickb_binary("xudt");
    let xudt_out_point = context.deploy_cell(Bytes::copy_from_slice(&xudt_elf));
    let xudt_script = build_xudt_owner_mode_script(&mut context, &xudt_out_point, &cellscript_script, xudt_binding);

    let first_receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script.clone()))
            .build(),
        receipt_group_input_data(receipt_data_mode, 0),
    );
    let second_receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script.clone()))
            .build(),
        receipt_group_input_data(receipt_data_mode, 1),
    );
    let header_hash = insert_and_link_mint_receipt_header(&mut context, &first_receipt_out_point, accumulated_rate);
    context.link_cell_with_block(second_receipt_out_point.clone(), header_hash.clone(), 0);

    let (outputs, outputs_data) = mint_from_receipt_outputs(output_udt_amount, &xudt_script, &always_success_lock);
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(first_receipt_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(second_receipt_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(xudt_out_point).dep_type(DepType::Code).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack());
    if header_dep_mode == MintHeaderDepMode::Present {
        tx_builder = tx_builder.header_dep(header_hash);
    }
    let tx = tx_builder.build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(MINT_RECEIPT_INPUT_CAPACITY * 2, &outputs);
    let run =
        side_run_from_result(context.verify_tx(&tx, MINT_FROM_RECEIPT_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    if matches!(xudt_binding, MintXudtBinding::WrongOwnerHash) {
        // A changed CellScript code hash can reorder the Type groups in the
        // pinned verifier's BTreeMap. Both groups reject this fixture; the
        // transaction's first error is not the selected CellScript outcome.
        for (script, expected) in [(&cellscript_script, 48), (&xudt_script, -52)] {
            let observed = cost_measurement::measure_group(
                &context,
                &tx,
                script,
                ckb_testtool::ckb_script::ScriptGroupType::Type,
                MINT_FROM_RECEIPT_MAX_CYCLES,
            );
            assert!(observed.required_cycles() > 0);
            let cost_measurement::CycleObservation::Measured { exit_code, .. } = observed.observation else {
                unreachable!("required_cycles already rejected unavailable evidence")
            };
            assert_eq!(exit_code, expected, "wrong-owner group outcome changed: {observed:?}");
            eprintln!("wrong-owner group: {}", serde_json::to_string(&observed).unwrap());
        }
    }
    (run, cellscript_elf)
}

fn run_original_receipt_group_missing_second_input() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let ickb_logic_elf = load_original_ickb_binary("ickb_logic");
    let ickb_logic_out_point = context.deploy_cell(Bytes::copy_from_slice(&ickb_logic_elf));
    let ickb_logic_script = context.build_script(&ickb_logic_out_point, Bytes::default()).expect("iCKB Logic script");
    let xudt_elf = load_original_ickb_binary("xudt");
    let xudt_out_point = context.deploy_cell(Bytes::copy_from_slice(&xudt_elf));
    let xudt_script =
        build_xudt_owner_mode_script(&mut context, &xudt_out_point, &ickb_logic_script, MintXudtBinding::ScriptUnderTest);

    let receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(ickb_logic_script))
            .build(),
        mint_receipt_data(),
    );
    let header_hash = insert_and_link_mint_receipt_header(&mut context, &receipt_out_point, MINT_RECEIPT_ACCUMULATED_RATE);

    let (outputs, outputs_data) = mint_from_receipt_outputs(MINT_RECEIPT_OUTPUT_AMOUNT * 2, &xudt_script, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(receipt_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(xudt_out_point).dep_type(DepType::Code).build())
        .header_dep(header_hash)
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(MINT_RECEIPT_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, MINT_FROM_RECEIPT_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_receipt_group_missing_second_input() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(
        RECEIPT_GROUP_UNDER_MINT_CELLSCRIPT_PROGRAM,
        RECEIPT_GROUP_UNDER_MINT_CELLSCRIPT_ACTION,
        None,
    );
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let xudt_elf = load_original_ickb_binary("xudt");
    let xudt_out_point = context.deploy_cell(Bytes::copy_from_slice(&xudt_elf));
    let xudt_script =
        build_xudt_owner_mode_script(&mut context, &xudt_out_point, &cellscript_script, MintXudtBinding::ScriptUnderTest);

    let receipt_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(MINT_RECEIPT_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script))
            .build(),
        mint_receipt_data(),
    );
    let header_hash = insert_and_link_mint_receipt_header(&mut context, &receipt_out_point, MINT_RECEIPT_ACCUMULATED_RATE);

    let (outputs, outputs_data) = mint_from_receipt_outputs(MINT_RECEIPT_OUTPUT_AMOUNT * 2, &xudt_script, &always_success_lock);
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(receipt_out_point).build())
        .cell_dep(packed::CellDep::new_builder().out_point(xudt_out_point).dep_type(DepType::Code).build())
        .header_dep(header_hash)
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(MINT_RECEIPT_INPUT_CAPACITY, &outputs);
    let run =
        side_run_from_result(context.verify_tx(&tx, MINT_FROM_RECEIPT_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}
