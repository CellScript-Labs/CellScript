fn run_original_limit_order_fulfillment_with_master_binding_and_output_data_mode(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    asset_binding: LimitOrderAssetBinding,
    master_binding: LimitOrderMasterBinding,
    input_data_mode: LimitOrderInputDataMode,
    output_data_mode: LimitOrderOutputDataMode,
) -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let limit_order_elf = load_original_ickb_binary("limit_order");
    let limit_order_out_point = context.deploy_cell(Bytes::copy_from_slice(&limit_order_elf));
    let limit_order_script = context.build_script(&limit_order_out_point, Bytes::default()).expect("original limit_order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, asset_binding);

    let (tx, outputs, outputs_data) = build_limit_order_tx_with_master_binding(
        &mut context,
        &limit_order_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        LimitOrderBuildParams {
            input_udt_amount,
            output_capacity,
            output_udt_amount,
            master_binding,
            input_data_mode,
            output_data_mode,
        },
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, auxiliary_type_sha256)
}

fn run_cellscript_limit_order_fulfillment_with_master_binding_and_output_data_mode(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    asset_binding: LimitOrderAssetBinding,
    master_binding: LimitOrderMasterBinding,
    input_data_mode: LimitOrderInputDataMode,
    output_data_mode: LimitOrderOutputDataMode,
) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf = compile_cellscript_source_to_elf(LIMIT_ORDER_CELLSCRIPT_PROGRAM, LIMIT_ORDER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, asset_binding);

    let (tx, outputs, outputs_data) = build_limit_order_tx_with_master_binding(
        &mut context,
        &cellscript_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        LimitOrderBuildParams {
            input_udt_amount,
            output_capacity,
            output_udt_amount,
            master_binding,
            input_data_mode,
            output_data_mode,
        },
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_sha256)
}

fn run_original_limit_order_with_cell_shape(shape: LimitOrderCellShape) -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let limit_order_elf = load_original_ickb_binary("limit_order");
    let limit_order_out_point = context.deploy_cell(Bytes::copy_from_slice(&limit_order_elf));
    let limit_order_script = context.build_script(&limit_order_out_point, Bytes::default()).expect("original limit_order script");
    let (auxiliary_type_script, _, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);
    let (tx, outputs, outputs_data) =
        build_limit_order_tx_with_cell_shape(&mut context, &limit_order_script, &auxiliary_type_script, shape);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, auxiliary_type_sha256)
}

fn run_cellscript_limit_order_with_cell_shape(shape: LimitOrderCellShape) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf = compile_cellscript_source_to_elf(LIMIT_ORDER_CELLSCRIPT_PROGRAM, LIMIT_ORDER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let (auxiliary_type_script, _, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);
    let (tx, outputs, outputs_data) =
        build_limit_order_tx_with_cell_shape(&mut context, &cellscript_script, &auxiliary_type_script, shape);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_sha256)
}

fn run_original_limit_order_with_type_shape(shape: LimitOrderTypeShape) -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let limit_order_elf = load_original_ickb_binary("limit_order");
    let limit_order_out_point = context.deploy_cell(Bytes::copy_from_slice(&limit_order_elf));
    let limit_order_script = context.build_script(&limit_order_out_point, Bytes::default()).expect("original limit_order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);
    let (tx, outputs, outputs_data) = build_limit_order_tx_with_type_shape(
        &mut context,
        &limit_order_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        shape,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, auxiliary_type_sha256)
}

fn run_cellscript_limit_order_with_type_shape(shape: LimitOrderTypeShape) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf = compile_cellscript_source_to_elf(LIMIT_ORDER_CELLSCRIPT_PROGRAM, LIMIT_ORDER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);
    let (tx, outputs, outputs_data) = build_limit_order_tx_with_type_shape(
        &mut context,
        &cellscript_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        shape,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_sha256)
}

fn run_original_limit_order_udt_to_ckb_fulfillment_with_master_binding_and_output_data_mode(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    asset_binding: LimitOrderAssetBinding,
    master_binding: LimitOrderMasterBinding,
    input_data_mode: LimitOrderInputDataMode,
    output_data_mode: LimitOrderOutputDataMode,
) -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let limit_order_elf = load_original_ickb_binary("limit_order");
    let limit_order_out_point = context.deploy_cell(Bytes::copy_from_slice(&limit_order_elf));
    let limit_order_script = context.build_script(&limit_order_out_point, Bytes::default()).expect("original limit_order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, asset_binding);

    let (tx, outputs, outputs_data) = build_limit_order_udt_to_ckb_tx_with_master_binding(
        &mut context,
        &limit_order_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        LimitOrderBuildParams {
            input_udt_amount,
            output_capacity,
            output_udt_amount,
            master_binding,
            input_data_mode,
            output_data_mode,
        },
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY + LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, auxiliary_type_sha256)
}

fn run_cellscript_limit_order_udt_to_ckb_fulfillment_with_master_binding_and_output_data_mode(
    input_udt_amount: u128,
    output_capacity: u64,
    output_udt_amount: u128,
    asset_binding: LimitOrderAssetBinding,
    master_binding: LimitOrderMasterBinding,
    input_data_mode: LimitOrderInputDataMode,
    output_data_mode: LimitOrderOutputDataMode,
) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_PROGRAM, LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript Limit Order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, asset_binding);

    let (tx, outputs, outputs_data) = build_limit_order_udt_to_ckb_tx_with_master_binding(
        &mut context,
        &cellscript_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        LimitOrderBuildParams {
            input_udt_amount,
            output_capacity,
            output_udt_amount,
            master_binding,
            input_data_mode,
            output_data_mode,
        },
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY + LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_sha256)
}

fn run_original_limit_order_udt_to_ckb_with_cell_shape(shape: LimitOrderCellShape) -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let limit_order_elf = load_original_ickb_binary("limit_order");
    let limit_order_out_point = context.deploy_cell(Bytes::copy_from_slice(&limit_order_elf));
    let limit_order_script = context.build_script(&limit_order_out_point, Bytes::default()).expect("original limit_order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);

    let (tx, outputs, outputs_data) = build_limit_order_udt_to_ckb_tx_with_cell_shape(
        &mut context,
        &limit_order_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        shape,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(limit_order_udt_to_ckb_input_capacity_for_cell_shape(shape), &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, auxiliary_type_sha256)
}

fn run_cellscript_limit_order_udt_to_ckb_with_cell_shape(shape: LimitOrderCellShape) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_PROGRAM, LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript Limit Order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);

    let (tx, outputs, outputs_data) = build_limit_order_udt_to_ckb_tx_with_cell_shape(
        &mut context,
        &cellscript_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        shape,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(limit_order_udt_to_ckb_input_capacity_for_cell_shape(shape), &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_sha256)
}

fn run_original_limit_order_udt_to_ckb_with_type_shape(shape: LimitOrderTypeShape) -> (DepositPhase1SideRun, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let limit_order_elf = load_original_ickb_binary("limit_order");
    let limit_order_out_point = context.deploy_cell(Bytes::copy_from_slice(&limit_order_elf));
    let limit_order_script = context.build_script(&limit_order_out_point, Bytes::default()).expect("original limit_order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);

    let (tx, outputs, outputs_data) = build_limit_order_udt_to_ckb_tx_with_type_shape(
        &mut context,
        &limit_order_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        shape,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY + LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, auxiliary_type_sha256)
}

fn run_cellscript_limit_order_udt_to_ckb_with_type_shape(shape: LimitOrderTypeShape) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_PROGRAM, LIMIT_ORDER_UDT_TO_CKB_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript Limit Order script");
    let (input_auxiliary_type_script, output_auxiliary_type_script, auxiliary_type_sha256) =
        deploy_auxiliary_type_scripts(&mut context, LimitOrderAssetBinding::SameAuxiliaryType);

    let (tx, outputs, outputs_data) = build_limit_order_udt_to_ckb_tx_with_type_shape(
        &mut context,
        &cellscript_script,
        &input_auxiliary_type_script,
        &output_auxiliary_type_script,
        shape,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(LIMIT_ORDER_INPUT_CAPACITY + LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, LIMIT_ORDER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_sha256)
}

fn deposit_phase1_outputs_with_receipt_data(
    deposit_capacity: u64,
    receipt_data: Bytes,
    shapes: DepositPhase1Shapes,
    script_under_test: &packed::Script,
    dao_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (Vec<packed::CellOutput>, Vec<Bytes>) {
    let deposit_lock = match shapes.lock {
        DepositPhase1LockShape::Valid => script_under_test.clone(),
        DepositPhase1LockShape::Wrong => always_success_lock.clone(),
    };
    let deposit_output_builder =
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(deposit_capacity.pack()).lock(deposit_lock);
    let deposit_output = match shapes.dao_type {
        DepositPhase1DaoTypeShape::Valid => deposit_output_builder.type_(packed::ScriptOpt::from(dao_script.clone())).build(),
        DepositPhase1DaoTypeShape::Missing => deposit_output_builder.build(),
        DepositPhase1DaoTypeShape::Wrong => deposit_output_builder.type_(packed::ScriptOpt::from(always_success_lock.clone())).build(),
    };
    let receipt_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(deposit_capacity.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    (vec![deposit_output, receipt_output], vec![shapes.deposit_data.data(), receipt_data])
}

fn duplicate_receipt_output_outputs(
    script_under_test: &packed::Script,
    dao_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (Vec<packed::CellOutput>, Vec<Bytes>) {
    let deposit_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(DUPLICATE_RECEIPT_OUTPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(dao_script.clone()))
        .build();
    let receipt_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(DUPLICATE_RECEIPT_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let receipt_data = deposit_phase1_receipt_data(DUPLICATE_RECEIPT_OUTPUT_CAPACITY);
    (vec![deposit_output, receipt_output.clone(), receipt_output], vec![Bytes::from(vec![0u8; 8]), receipt_data.clone(), receipt_data])
}

fn receipt_without_deposit_outputs(
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (Vec<packed::CellOutput>, Vec<Bytes>) {
    let receipt_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(RECEIPT_WITHOUT_DEPOSIT_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    (vec![receipt_output], vec![receipt_without_deposit_data()])
}

fn non_empty_args_outputs(
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (Vec<packed::CellOutput>, Vec<Bytes>) {
    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(NON_EMPTY_ARGS_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    (vec![output], vec![Bytes::default()])
}

fn build_xudt_owner_mode_script(
    context: &mut ckb_testtool::context::Context,
    xudt_out_point: &packed::OutPoint,
    owner_script: &packed::Script,
    binding: MintXudtBinding,
) -> packed::Script {
    let owner_hash: [u8; 32] = match binding {
        MintXudtBinding::ScriptUnderTest => owner_script.calc_script_hash().unpack(),
        MintXudtBinding::WrongOwnerHash => WRONG_XUDT_OWNER_HASH,
    };
    let mut xudt_args = Vec::with_capacity(36);
    xudt_args.extend_from_slice(&owner_hash);
    xudt_args.extend_from_slice(&XUDT_OWNER_MODE_TYPE_FLAGS.to_le_bytes());
    context
        .build_script_with_hash_type(xudt_out_point, ScriptHashType::Data1, Bytes::from(xudt_args))
        .expect("iCKB xUDT owner-mode script")
}

fn insert_and_link_mint_receipt_header(
    context: &mut ckb_testtool::context::Context,
    receipt_out_point: &packed::OutPoint,
    accumulated_rate: u64,
) -> packed::Byte32 {
    let dao_field = ckb_script_runner::make_dao_field(accumulated_rate);
    let header = HeaderBuilder::default().number(0u64).dao(dao_field.pack()).build();
    let header_hash = header.hash();
    context.insert_header(header);
    context.link_cell_with_block(receipt_out_point.clone(), header_hash.clone(), 0);
    header_hash
}

fn mint_from_receipt_outputs(
    output_udt_amount: u128,
    xudt_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (Vec<packed::CellOutput>, Vec<Bytes>) {
    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(MINT_XUDT_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(xudt_script.clone()))
        .build();
    (vec![output], vec![xudt_output_data(output_udt_amount)])
}

fn deploy_owned_owner_auxiliary_withdrawal_type(context: &mut ckb_testtool::context::Context) -> (packed::Script, String) {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_PASS_PROGRAM, VM_HARNESS_PASS_ACTION, None);
    let out_point = context.deploy_cell(Bytes::copy_from_slice(&elf));
    let script = context.build_script(&out_point, Bytes::default()).expect("owned-owner auxiliary withdrawal type");
    (script, sha256_prefixed(&elf))
}

fn deploy_owned_owner_auxiliary_withdrawal_type_pair(
    context: &mut ckb_testtool::context::Context,
) -> (packed::Script, packed::Script, String) {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_PASS_PROGRAM, VM_HARNESS_PASS_ACTION, None);
    let out_point = context.deploy_cell(Bytes::copy_from_slice(&elf));
    let expected_script = context.build_script(&out_point, Bytes::default()).expect("owned-owner expected auxiliary type");
    let wrong_script = context.build_script(&out_point, Bytes::from_static(&[1])).expect("owned-owner mismatched auxiliary type");
    (expected_script, wrong_script, sha256_prefixed(&elf))
}

fn deploy_auxiliary_type_scripts(
    context: &mut ckb_testtool::context::Context,
    asset_binding: LimitOrderAssetBinding,
) -> (packed::Script, packed::Script, String) {
    let elf = compile_cellscript_source_to_elf(VM_HARNESS_PASS_PROGRAM, VM_HARNESS_PASS_ACTION, None);
    let out_point = context.deploy_cell(Bytes::copy_from_slice(&elf));
    let input_script = context.build_script(&out_point, Bytes::default()).expect("auxiliary input type script");
    let output_args = match asset_binding {
        LimitOrderAssetBinding::SameAuxiliaryType => Bytes::default(),
        LimitOrderAssetBinding::DifferentAuxiliaryType => Bytes::from_static(&[1]),
    };
    let output_script = context.build_script(&out_point, output_args).expect("auxiliary output type script");
    (input_script, output_script, sha256_prefixed(&elf))
}
