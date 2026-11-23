fn run_original_owned_owner(owner_relative_distance: i32) -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    let withdrawal_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &withdrawal_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");

    let (tx, outputs, outputs_data) = build_owned_owner_tx(
        &mut context,
        &owned_owner_script,
        &withdrawal_type_script,
        &always_success_lock,
        owner_relative_distance,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner(owner_relative_distance: i32) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf = compile_cellscript_source_to_elf(OWNED_OWNER_CELLSCRIPT_PROGRAM, OWNED_OWNER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");

    let (tx, outputs, outputs_data) =
        build_owned_owner_tx(&mut context, &cellscript_script, &withdrawal_type_script, &always_success_lock, owner_relative_distance);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}

fn run_original_owned_owner_output_pair(owner_relative_distance: i32) -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    let withdrawal_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &withdrawal_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_pair_tx(
        &mut context,
        &owned_owner_script,
        &withdrawal_type_script,
        &always_success_lock,
        owner_relative_distance,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner_output_pair(owner_relative_distance: i32) -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(OWNED_OWNER_OUTPUT_CELLSCRIPT_PROGRAM, OWNED_OWNER_OUTPUT_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_pair_tx(
        &mut context,
        &cellscript_script,
        &withdrawal_type_script,
        &always_success_lock,
        owner_relative_distance,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}

fn run_original_owned_owner_output_duplicate_owner() -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    let withdrawal_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &withdrawal_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_duplicate_owner_tx(&mut context, &owned_owner_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 3, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner_output_duplicate_owner() -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(OWNED_OWNER_OUTPUT_CELLSCRIPT_PROGRAM, OWNED_OWNER_OUTPUT_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_duplicate_owner_tx(&mut context, &cellscript_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 3, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}

fn run_original_owned_owner_output_missing_owner() -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    let withdrawal_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &withdrawal_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_missing_owner_tx(&mut context, &owned_owner_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 3, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner_output_missing_owner() -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(OWNED_OWNER_OUTPUT_CELLSCRIPT_PROGRAM, OWNED_OWNER_OUTPUT_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_missing_owner_tx(&mut context, &cellscript_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 3, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}

fn run_original_owned_owner_output_missing_owned() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let owned_owner_elf = load_original_ickb_binary("owned_owner");
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_missing_owned_tx(&mut context, &owned_owner_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_owned_owner_output_missing_owned() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(OWNED_OWNER_OUTPUT_CELLSCRIPT_PROGRAM, OWNED_OWNER_OUTPUT_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_missing_owned_tx(&mut context, &cellscript_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_owned_owner_output_script_misuse() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let owned_owner_elf = load_original_ickb_binary("owned_owner");
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_script_misuse_tx(&mut context, &owned_owner_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_owned_owner_output_script_misuse() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(
        OWNED_OWNER_OUTPUT_SCRIPT_MISUSE_CELLSCRIPT_PROGRAM,
        OWNED_OWNER_OUTPUT_SCRIPT_MISUSE_CELLSCRIPT_ACTION,
        None,
    );
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_script_misuse_tx(&mut context, &cellscript_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_owned_owner_output_not_withdrawal() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let owned_owner_elf = load_original_ickb_binary("owned_owner");
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_not_withdrawal_tx(&mut context, &owned_owner_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_owned_owner_output_not_withdrawal() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(
        OWNED_OWNER_OUTPUT_NOT_WITHDRAWAL_CELLSCRIPT_PROGRAM,
        OWNED_OWNER_OUTPUT_NOT_WITHDRAWAL_CELLSCRIPT_ACTION,
        None,
    );
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_output_not_withdrawal_tx(&mut context, &cellscript_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_owned_owner_output_owner_data_length_mismatch() -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    let withdrawal_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &withdrawal_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_owner_data_length_mismatch_tx(
        &mut context,
        &owned_owner_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner_output_owner_data_length_mismatch() -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf =
        compile_cellscript_source_to_elf(OWNED_OWNER_OUTPUT_CELLSCRIPT_PROGRAM, OWNED_OWNER_OUTPUT_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_owner_data_length_mismatch_tx(
        &mut context,
        &cellscript_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}

fn run_original_owned_owner_output_related_type_hash_mismatch() -> (DepositPhase1SideRun, String, String, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (expected_withdrawal_type_script, actual_wrong_type_script, auxiliary_type_artifact_sha256) =
        deploy_owned_owner_auxiliary_withdrawal_type_pair(&mut context);
    let expected_type_hash: [u8; 32] = expected_withdrawal_type_script.calc_script_hash().unpack();
    let actual_type_hash: [u8; 32] = actual_wrong_type_script.calc_script_hash().unpack();
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &expected_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_related_type_hash_mismatch_tx(
        &mut context,
        &owned_owner_script,
        &actual_wrong_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (
        run,
        patched_owned_owner_sha256,
        auxiliary_type_artifact_sha256,
        hex_prefixed(&expected_type_hash),
        hex_prefixed(&actual_type_hash),
    )
}

fn run_cellscript_owned_owner_output_related_type_hash_mismatch() -> (DepositPhase1SideRun, Vec<u8>, String, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (expected_withdrawal_type_script, actual_wrong_type_script, auxiliary_type_artifact_sha256) =
        deploy_owned_owner_auxiliary_withdrawal_type_pair(&mut context);
    let expected_type_hash: [u8; 32] = expected_withdrawal_type_script.calc_script_hash().unpack();
    let actual_type_hash: [u8; 32] = actual_wrong_type_script.calc_script_hash().unpack();
    let program = owned_owner_output_related_type_hash_mismatch_cellscript_program(&expected_withdrawal_type_script);
    let cellscript_elf =
        compile_cellscript_source_to_elf(&program, OWNED_OWNER_OUTPUT_RELATED_TYPE_HASH_MISMATCH_CELLSCRIPT_ACTION, None);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_related_type_hash_mismatch_tx(
        &mut context,
        &cellscript_script,
        &actual_wrong_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256, hex_prefixed(&expected_type_hash), hex_prefixed(&actual_type_hash))
}

fn run_original_owned_owner_output_related_data_rule_mismatch() -> (DepositPhase1SideRun, String, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let expected_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &expected_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_related_data_rule_mismatch_tx(
        &mut context,
        &owned_owner_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256, hex_prefixed(&expected_type_hash))
}

fn run_cellscript_owned_owner_output_related_data_rule_mismatch() -> (DepositPhase1SideRun, Vec<u8>, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let expected_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    let program = owned_owner_output_related_data_rule_mismatch_cellscript_program(&withdrawal_type_script);
    let cellscript_elf =
        compile_cellscript_source_to_elf(&program, OWNED_OWNER_OUTPUT_RELATED_DATA_RULE_MISMATCH_CELLSCRIPT_ACTION, None);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_output_related_data_rule_mismatch_tx(
        &mut context,
        &cellscript_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_OUTPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256, hex_prefixed(&expected_type_hash))
}

fn run_original_owned_owner_related_type_hash_mismatch() -> (DepositPhase1SideRun, String, String, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (expected_withdrawal_type_script, actual_wrong_type_script, auxiliary_type_artifact_sha256) =
        deploy_owned_owner_auxiliary_withdrawal_type_pair(&mut context);
    let expected_type_hash: [u8; 32] = expected_withdrawal_type_script.calc_script_hash().unpack();
    let actual_type_hash: [u8; 32] = actual_wrong_type_script.calc_script_hash().unpack();
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &expected_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_related_type_hash_mismatch_tx(
        &mut context,
        &owned_owner_script,
        &actual_wrong_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (
        run,
        patched_owned_owner_sha256,
        auxiliary_type_artifact_sha256,
        hex_prefixed(&expected_type_hash),
        hex_prefixed(&actual_type_hash),
    )
}

fn run_cellscript_owned_owner_related_type_hash_mismatch() -> (DepositPhase1SideRun, Vec<u8>, String, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (expected_withdrawal_type_script, actual_wrong_type_script, auxiliary_type_artifact_sha256) =
        deploy_owned_owner_auxiliary_withdrawal_type_pair(&mut context);
    let expected_type_hash: [u8; 32] = expected_withdrawal_type_script.calc_script_hash().unpack();
    let actual_type_hash: [u8; 32] = actual_wrong_type_script.calc_script_hash().unpack();
    let program = owned_owner_related_type_hash_mismatch_cellscript_program(&expected_withdrawal_type_script);
    let cellscript_elf = compile_cellscript_source_to_elf(&program, OWNED_OWNER_RELATED_TYPE_HASH_MISMATCH_CELLSCRIPT_ACTION, None);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_related_type_hash_mismatch_tx(
        &mut context,
        &cellscript_script,
        &actual_wrong_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256, hex_prefixed(&expected_type_hash), hex_prefixed(&actual_type_hash))
}

fn run_original_owned_owner_related_data_rule_mismatch() -> (DepositPhase1SideRun, String, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let expected_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &expected_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_related_data_rule_mismatch_tx(
        &mut context,
        &owned_owner_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256, hex_prefixed(&expected_type_hash))
}

fn run_cellscript_owned_owner_related_data_rule_mismatch() -> (DepositPhase1SideRun, Vec<u8>, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let expected_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    let program = owned_owner_related_data_rule_mismatch_cellscript_program(&withdrawal_type_script);
    let cellscript_elf = compile_cellscript_source_to_elf(&program, OWNED_OWNER_RELATED_DATA_RULE_MISMATCH_CELLSCRIPT_ACTION, None);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_related_data_rule_mismatch_tx(
        &mut context,
        &cellscript_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256, hex_prefixed(&expected_type_hash))
}

fn run_original_owned_owner_owner_data_length_mismatch() -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let expected_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &expected_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_owner_data_length_mismatch_tx(
        &mut context,
        &owned_owner_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner_owner_data_length_mismatch() -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf = compile_cellscript_source_to_elf(OWNED_OWNER_CELLSCRIPT_PROGRAM, OWNED_OWNER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_owner_data_length_mismatch_tx(
        &mut context,
        &cellscript_script,
        &withdrawal_type_script,
        &always_success_lock,
    );
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 2, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}

fn run_original_owned_owner_script_misuse() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let owned_owner_elf = load_original_ickb_binary("owned_owner");
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_script_misuse_tx(&mut context, &owned_owner_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_owned_owner_script_misuse() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(
        OWNED_OWNER_SCRIPT_MISUSE_CELLSCRIPT_PROGRAM,
        OWNED_OWNER_SCRIPT_MISUSE_CELLSCRIPT_ACTION,
        None,
    );
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_script_misuse_tx(&mut context, &cellscript_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_owned_owner_not_withdrawal() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let owned_owner_elf = load_original_ickb_binary("owned_owner");
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_not_withdrawal_tx(&mut context, &owned_owner_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_owned_owner_not_withdrawal() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(
        OWNED_OWNER_NOT_WITHDRAWAL_CELLSCRIPT_PROGRAM,
        OWNED_OWNER_NOT_WITHDRAWAL_CELLSCRIPT_ACTION,
        None,
    );
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_not_withdrawal_tx(&mut context, &cellscript_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_owned_owner_missing_owner() -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    let withdrawal_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &withdrawal_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_missing_owner_tx(&mut context, &owned_owner_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner_missing_owner() -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf = compile_cellscript_source_to_elf(OWNED_OWNER_CELLSCRIPT_PROGRAM, OWNED_OWNER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_missing_owner_tx(&mut context, &cellscript_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}

fn run_original_owned_owner_missing_owned() -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let owned_owner_elf = load_original_ickb_binary("owned_owner");
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_missing_owned_tx(&mut context, &owned_owner_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons)
}

fn run_cellscript_owned_owner_missing_owned() -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(OWNED_OWNER_CELLSCRIPT_PROGRAM, OWNED_OWNER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) = build_owned_owner_missing_owned_tx(&mut context, &cellscript_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf)
}

fn run_original_owned_owner_duplicate_owner() -> (DepositPhase1SideRun, String, String) {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let mut owned_owner_elf = load_original_ickb_binary("owned_owner");
    let withdrawal_type_hash: [u8; 32] = withdrawal_type_script.calc_script_hash().unpack();
    patch_owned_owner_dao_hash(&mut owned_owner_elf, &withdrawal_type_hash);
    let patched_owned_owner_sha256 = sha256_prefixed(&owned_owner_elf);
    let owned_owner_out_point = context.deploy_cell(Bytes::copy_from_slice(&owned_owner_elf));
    let owned_owner_script = context.build_script(&owned_owner_out_point, Bytes::default()).expect("owned_owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_duplicate_owner_tx(&mut context, &owned_owner_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 3, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, patched_owned_owner_sha256, auxiliary_type_artifact_sha256)
}

fn run_cellscript_owned_owner_duplicate_owner() -> (DepositPhase1SideRun, Vec<u8>, String) {
    let cellscript_elf = compile_cellscript_source_to_elf(OWNED_OWNER_CELLSCRIPT_PROGRAM, OWNED_OWNER_CELLSCRIPT_ACTION, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let always_success_lock = deploy_always_success_lock(&mut context);
    let (withdrawal_type_script, auxiliary_type_artifact_sha256) = deploy_owned_owner_auxiliary_withdrawal_type(&mut context);
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript owned-owner script");
    let (tx, outputs, outputs_data) =
        build_owned_owner_duplicate_owner_tx(&mut context, &cellscript_script, &withdrawal_type_script, &always_success_lock);
    let occupied_capacity_shannons = occupied_capacity_shannons(&outputs, &outputs_data);
    let fee_shannons = fee_shannons(OWNED_OWNER_INPUT_CAPACITY * 3, &outputs);
    let run = side_run_from_result(context.verify_tx(&tx, OWNED_OWNER_MAX_CYCLES), &tx, occupied_capacity_shannons, fee_shannons);
    (run, cellscript_elf, auxiliary_type_artifact_sha256)
}
