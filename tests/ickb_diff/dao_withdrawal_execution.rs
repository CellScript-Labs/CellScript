fn run_original_dao_withdrawal_with_header_dep_mode(
    input_since: u64,
    output_capacity: u64,
    header_dep_mode: DaoWithdrawalHeaderDepMode,
) -> DepositPhase1SideRun {
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let dao_elf = load_original_ickb_binary("dao");
    let dao_code_out_point = context.deploy_cell(Bytes::copy_from_slice(&dao_elf));
    let dao_script = context.build_script(&dao_code_out_point, Bytes::default()).expect("original DAO script");
    let always_success_lock = deploy_always_success_lock(&mut context);

    let deposit_accumulated_rate = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
        _ => ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
    };
    let deposit_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
        deposit_accumulated_rate,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH,
    );
    let deposit_header_hash = deposit_header.hash();
    context.insert_header(deposit_header);
    let withdraw_accumulated_rate = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
        _ => ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
    };
    let withdraw_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
        withdraw_accumulated_rate,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
    );
    let withdraw_header_hash = withdraw_header.hash();
    context.insert_header(withdraw_header);

    let withdrawing_cell_data = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::DepositDataInput => Bytes::from(vec![0u8; 8]),
        DaoWithdrawalHeaderDepMode::MalformedInputData => Bytes::from(vec![0x12, 0x06, 0x00, 0x00]),
        DaoWithdrawalHeaderDepMode::LongInputData => dao_long_withdrawal_request_cell_data(),
        _ => Bytes::from(ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK.to_le_bytes().to_vec()),
    };
    let withdrawing_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(dao_script))
            .build(),
        withdrawing_cell_data,
    );
    let committed_withdraw_header_hash = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => deposit_header_hash.clone(),
        _ => withdraw_header_hash.clone(),
    };
    context.link_cell_with_block(withdrawing_out_point.clone(), committed_withdraw_header_hash, 0);

    let output =
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(output_capacity.pack()).lock(always_success_lock).build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let witness_header_dep_index = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::Present => 1u64,
        DaoWithdrawalHeaderDepMode::DepositDataInput => 1u64,
        DaoWithdrawalHeaderDepMode::MalformedInputData => 1u64,
        DaoWithdrawalHeaderDepMode::LongInputData => 1u64,
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader => 0u64,
        DaoWithdrawalHeaderDepMode::MissingDepositHeader => 1u64,
        DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds => 2u64,
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => 1u64,
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => 1u64,
        DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex => 0u64,
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => 1u64,
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::LongWitnessInputType => 1u64,
    };
    let witness = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType => packed::WitnessArgs::new_builder().build(),
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType => {
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::default()).pack()).build()
        }
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType => {
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(vec![1u8])).pack()).build()
        }
        DaoWithdrawalHeaderDepMode::LongWitnessInputType => {
            let mut input_type = witness_header_dep_index.to_le_bytes().to_vec();
            input_type.push(0x99);
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(input_type)).pack()).build()
        }
        _ => packed::WitnessArgs::new_builder()
            .input_type(Some(Bytes::from(witness_header_dep_index.to_le_bytes().to_vec())).pack())
            .build(),
    };
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(withdrawing_out_point).since(input_since).build())
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .output(outputs[0].clone())
        .output_data(outputs_data[0].clone().pack())
        .witness(witness.as_bytes().pack());
    tx_builder = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::Present
        | DaoWithdrawalHeaderDepMode::DepositDataInput
        | DaoWithdrawalHeaderDepMode::MalformedInputData
        | DaoWithdrawalHeaderDepMode::LongInputData
        | DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate
        | DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate
        | DaoWithdrawalHeaderDepMode::MissingWitnessInputType
        | DaoWithdrawalHeaderDepMode::EmptyWitnessInputType
        | DaoWithdrawalHeaderDepMode::ShortWitnessInputType
        | DaoWithdrawalHeaderDepMode::LongWitnessInputType => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader => tx_builder.header_dep(deposit_header_hash),
        DaoWithdrawalHeaderDepMode::MissingDepositHeader => tx_builder.header_dep(withdraw_header_hash),
        DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
        DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
    };
    let tx = tx_builder.build();
    let tx = context.complete_tx(tx);

    side_run_from_result(
        context.verify_tx(&tx, ORIGINAL_DAO_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY, &outputs),
    )
}

fn run_cellscript_dao_withdrawal_with_program(
    input_since: u64,
    output_capacity: u64,
    header_dep_mode: DaoWithdrawalHeaderDepMode,
    program: &str,
    action: &str,
) -> (DepositPhase1SideRun, Vec<u8>) {
    let cellscript_elf = compile_cellscript_source_to_elf(program, action, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script = context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript DAO withdrawal script");
    let always_success_lock = deploy_always_success_lock(&mut context);

    let deposit_accumulated_rate = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
        _ => ORIGINAL_DAO_WITHDRAW_PHASE1_ACCUMULATED_RATE,
    };
    let deposit_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
        deposit_accumulated_rate,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH,
    );
    let deposit_header_hash = deposit_header.hash();
    context.insert_header(deposit_header);
    let withdraw_accumulated_rate = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
        _ => ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
    };
    let withdraw_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
        withdraw_accumulated_rate,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
    );
    let withdraw_header_hash = withdraw_header.hash();
    context.insert_header(withdraw_header);

    let withdrawing_cell_data = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::DepositDataInput => Bytes::from(vec![0u8; 8]),
        DaoWithdrawalHeaderDepMode::MalformedInputData => Bytes::from(vec![0x12, 0x06, 0x00, 0x00]),
        DaoWithdrawalHeaderDepMode::LongInputData => dao_long_withdrawal_request_cell_data(),
        _ => Bytes::from(ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK.to_le_bytes().to_vec()),
    };
    let withdrawing_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script))
            .build(),
        withdrawing_cell_data,
    );
    let committed_withdraw_header_hash = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => deposit_header_hash.clone(),
        _ => withdraw_header_hash.clone(),
    };
    context.link_cell_with_block(withdrawing_out_point.clone(), committed_withdraw_header_hash, 0);

    let output =
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(output_capacity.pack()).lock(always_success_lock).build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let witness_header_dep_index = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::Present => 1u64,
        DaoWithdrawalHeaderDepMode::DepositDataInput => 1u64,
        DaoWithdrawalHeaderDepMode::MalformedInputData => 1u64,
        DaoWithdrawalHeaderDepMode::LongInputData => 1u64,
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader => 0u64,
        DaoWithdrawalHeaderDepMode::MissingDepositHeader => 1u64,
        DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds => 2u64,
        DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate => 1u64,
        DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate => 1u64,
        DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex => 0u64,
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => 1u64,
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType => 1u64,
        DaoWithdrawalHeaderDepMode::LongWitnessInputType => 1u64,
    };
    let witness = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::MissingWitnessInputType => packed::WitnessArgs::new_builder().build(),
        DaoWithdrawalHeaderDepMode::EmptyWitnessInputType => {
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::default()).pack()).build()
        }
        DaoWithdrawalHeaderDepMode::ShortWitnessInputType => {
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(vec![1u8])).pack()).build()
        }
        DaoWithdrawalHeaderDepMode::LongWitnessInputType => {
            let mut input_type = witness_header_dep_index.to_le_bytes().to_vec();
            input_type.push(0x99);
            packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(input_type)).pack()).build()
        }
        _ => packed::WitnessArgs::new_builder()
            .input_type(Some(Bytes::from(witness_header_dep_index.to_le_bytes().to_vec())).pack())
            .build(),
    };
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(withdrawing_out_point).since(input_since).build())
        .cell_dep(packed::CellDep::new_builder().out_point(cellscript_out_point).dep_type(DepType::Code).build())
        .output(outputs[0].clone())
        .output_data(outputs_data[0].clone().pack())
        .witness(witness.as_bytes().pack());
    tx_builder = match header_dep_mode {
        DaoWithdrawalHeaderDepMode::Present
        | DaoWithdrawalHeaderDepMode::DepositDataInput
        | DaoWithdrawalHeaderDepMode::MalformedInputData
        | DaoWithdrawalHeaderDepMode::LongInputData
        | DaoWithdrawalHeaderDepMode::WrongDepositAccumulatedRate
        | DaoWithdrawalHeaderDepMode::WrongWithdrawAccumulatedRate
        | DaoWithdrawalHeaderDepMode::MissingWitnessInputType
        | DaoWithdrawalHeaderDepMode::EmptyWitnessInputType
        | DaoWithdrawalHeaderDepMode::ShortWitnessInputType
        | DaoWithdrawalHeaderDepMode::LongWitnessInputType => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
        DaoWithdrawalHeaderDepMode::MissingWithdrawHeader => tx_builder.header_dep(deposit_header_hash),
        DaoWithdrawalHeaderDepMode::MissingDepositHeader => tx_builder.header_dep(withdraw_header_hash),
        DaoWithdrawalHeaderDepMode::DepositHeaderIndexOutOfBounds => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
        DaoWithdrawalHeaderDepMode::WrongDepositHeaderIndex => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
        DaoWithdrawalHeaderDepMode::WrongWithdrawCommittedHeader => {
            tx_builder.header_dep(withdraw_header_hash).header_dep(deposit_header_hash)
        }
    };
    let tx = tx_builder.build();
    let tx = context.complete_tx(tx);

    let run = side_run_from_result(
        context.verify_tx(&tx, ORIGINAL_DAO_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY, &outputs),
    );
    (run, cellscript_elf)
}

fn run_original_dao_two_input_withdrawal(output_capacity: u64, mode: DaoTwoInputWithdrawalMode) -> DepositPhase1SideRun {
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
    let second_deposit_header_hash = if matches!(mode, DaoTwoInputWithdrawalMode::MixedDeposit | DaoTwoInputWithdrawalMode::MixedBoth)
    {
        let second_deposit_header = dao_test_header(
            ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH,
        );
        let hash = second_deposit_header.hash();
        context.insert_header(second_deposit_header);
        Some(hash)
    } else {
        None
    };
    let withdraw_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
        ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
    );
    let withdraw_header_hash = withdraw_header.hash();
    context.insert_header(withdraw_header);
    let second_withdraw_header_hash =
        if matches!(mode, DaoTwoInputWithdrawalMode::MixedWithdraw | DaoTwoInputWithdrawalMode::MixedBoth) {
            let second_withdraw_header = dao_test_header(
                ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
                ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
                ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
            );
            let hash = second_withdraw_header.hash();
            context.insert_header(second_withdraw_header);
            Some(hash)
        } else {
            None
        };

    let first_withdrawal_data = dao_two_input_cell_data(mode, 0);
    let second_withdrawal_data = dao_two_input_cell_data(mode, 1);
    let first_withdrawing_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(dao_script.clone()))
            .build(),
        first_withdrawal_data,
    );
    let second_withdrawing_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(dao_script))
            .build(),
        second_withdrawal_data,
    );
    context.link_cell_with_block(first_withdrawing_out_point.clone(), withdraw_header_hash.clone(), 0);
    context.link_cell_with_block(
        second_withdrawing_out_point.clone(),
        second_withdraw_header_hash.clone().unwrap_or_else(|| withdraw_header_hash.clone()),
        0,
    );

    let output =
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(output_capacity.pack()).lock(always_success_lock).build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let first_witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(1u64.to_le_bytes().to_vec())).pack()).build();
    let second_witness = dao_two_input_second_witness(mode);
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(
            packed::CellInput::new_builder()
                .previous_output(first_withdrawing_out_point)
                .since(ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE)
                .build(),
        )
        .input(
            packed::CellInput::new_builder()
                .previous_output(second_withdrawing_out_point)
                .since(ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE)
                .build(),
        )
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .header_dep(withdraw_header_hash)
        .header_dep(deposit_header_hash)
        .output(outputs[0].clone())
        .output_data(outputs_data[0].clone().pack());
    if let Some(second_deposit_header_hash) = second_deposit_header_hash {
        tx_builder = tx_builder.header_dep(second_deposit_header_hash);
    }
    if let Some(second_withdraw_header_hash) = second_withdraw_header_hash {
        tx_builder = tx_builder.header_dep(second_withdraw_header_hash);
    }
    let tx = tx_builder.witness(first_witness.as_bytes().pack()).witness(second_witness.as_bytes().pack()).build();
    let tx = context.complete_tx(tx);

    side_run_from_result(
        context.verify_tx(&tx, ORIGINAL_DAO_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY * 2, &outputs),
    )
}

fn run_cellscript_dao_two_input_withdrawal(output_capacity: u64, mode: DaoTwoInputWithdrawalMode) -> (DepositPhase1SideRun, Vec<u8>) {
    let (program, action) = match mode {
        DaoTwoInputWithdrawalMode::SameDeposit => {
            (DAO_TWO_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM, DAO_TWO_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION)
        }
        DaoTwoInputWithdrawalMode::MixedDeposit => {
            (DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_PROGRAM, DAO_TWO_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_ACTION)
        }
        DaoTwoInputWithdrawalMode::MixedWithdraw => {
            (DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_PROGRAM, DAO_TWO_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_ACTION)
        }
        DaoTwoInputWithdrawalMode::MixedBoth => {
            (DAO_TWO_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_PROGRAM, DAO_TWO_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_ACTION)
        }
        DaoTwoInputWithdrawalMode::SecondDepositDataInput
        | DaoTwoInputWithdrawalMode::SecondMalformedInputData
        | DaoTwoInputWithdrawalMode::SecondLongInputData => {
            (DAO_TWO_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM, DAO_TWO_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION)
        }
        DaoTwoInputWithdrawalMode::SecondWitnessMissing
        | DaoTwoInputWithdrawalMode::SecondWitnessEmpty
        | DaoTwoInputWithdrawalMode::SecondWitnessShort
        | DaoTwoInputWithdrawalMode::SecondWitnessLong => {
            (DAO_TWO_INPUT_WITNESS_SHAPE_CELLSCRIPT_PROGRAM, DAO_TWO_INPUT_WITNESS_SHAPE_CELLSCRIPT_ACTION)
        }
        DaoTwoInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex | DaoTwoInputWithdrawalMode::SecondWitnessOutOfBoundsIndex => {
            (DAO_TWO_INPUT_WITNESS_INDEX_CELLSCRIPT_PROGRAM, DAO_TWO_INPUT_WITNESS_INDEX_CELLSCRIPT_ACTION)
        }
    };
    let cellscript_elf = compile_cellscript_source_to_elf(program, action, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script =
        context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript DAO two-input withdrawal script");
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
    let second_deposit_header_hash = if matches!(mode, DaoTwoInputWithdrawalMode::MixedDeposit | DaoTwoInputWithdrawalMode::MixedBoth)
    {
        let second_deposit_header = dao_test_header(
            ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH,
        );
        let hash = second_deposit_header.hash();
        context.insert_header(second_deposit_header);
        Some(hash)
    } else {
        None
    };
    let withdraw_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
        ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
    );
    let withdraw_header_hash = withdraw_header.hash();
    context.insert_header(withdraw_header);
    let second_withdraw_header_hash =
        if matches!(mode, DaoTwoInputWithdrawalMode::MixedWithdraw | DaoTwoInputWithdrawalMode::MixedBoth) {
            let second_withdraw_header = dao_test_header(
                ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
                ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
                ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
                ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
                ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
            );
            let hash = second_withdraw_header.hash();
            context.insert_header(second_withdraw_header);
            Some(hash)
        } else {
            None
        };

    let first_withdrawal_data = dao_two_input_cell_data(mode, 0);
    let second_withdrawal_data = dao_two_input_cell_data(mode, 1);
    let first_withdrawing_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script.clone()))
            .build(),
        first_withdrawal_data,
    );
    let second_withdrawing_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(cellscript_script))
            .build(),
        second_withdrawal_data,
    );
    context.link_cell_with_block(first_withdrawing_out_point.clone(), withdraw_header_hash.clone(), 0);
    context.link_cell_with_block(
        second_withdrawing_out_point.clone(),
        second_withdraw_header_hash.clone().unwrap_or_else(|| withdraw_header_hash.clone()),
        0,
    );

    let output =
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(output_capacity.pack()).lock(always_success_lock).build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let first_witness = packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(1u64.to_le_bytes().to_vec())).pack()).build();
    let second_witness = dao_two_input_second_witness(mode);
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(
            packed::CellInput::new_builder()
                .previous_output(first_withdrawing_out_point)
                .since(ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE)
                .build(),
        )
        .input(
            packed::CellInput::new_builder()
                .previous_output(second_withdrawing_out_point)
                .since(ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE)
                .build(),
        )
        .cell_dep(packed::CellDep::new_builder().out_point(cellscript_out_point).dep_type(DepType::Code).build())
        .header_dep(withdraw_header_hash)
        .header_dep(deposit_header_hash)
        .output(outputs[0].clone())
        .output_data(outputs_data[0].clone().pack());
    if let Some(second_deposit_header_hash) = second_deposit_header_hash {
        tx_builder = tx_builder.header_dep(second_deposit_header_hash);
    }
    if let Some(second_withdraw_header_hash) = second_withdraw_header_hash {
        tx_builder = tx_builder.header_dep(second_withdraw_header_hash);
    }
    let tx = tx_builder.witness(first_witness.as_bytes().pack()).witness(second_witness.as_bytes().pack()).build();
    let tx = context.complete_tx(tx);

    let run = side_run_from_result(
        context.verify_tx(&tx, ORIGINAL_DAO_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY * 2, &outputs),
    );
    (run, cellscript_elf)
}

fn run_original_dao_three_input_withdrawal(output_capacity: u64, mode: DaoThreeInputWithdrawalMode) -> DepositPhase1SideRun {
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
    let mixed_deposit_header_hash = if matches!(
        mode,
        DaoThreeInputWithdrawalMode::MixedDepositSecond
            | DaoThreeInputWithdrawalMode::MixedBothSecond
            | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            | DaoThreeInputWithdrawalMode::MixedDepositThird
            | DaoThreeInputWithdrawalMode::MixedBothThird
    ) {
        let mixed_deposit_header = dao_test_header(
            ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH,
        );
        let hash = mixed_deposit_header.hash();
        context.insert_header(mixed_deposit_header);
        Some(hash)
    } else {
        None
    };
    let withdraw_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
        ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
    );
    let withdraw_header_hash = withdraw_header.hash();
    context.insert_header(withdraw_header);
    let mixed_withdraw_header_hash = if matches!(
        mode,
        DaoThreeInputWithdrawalMode::MixedWithdrawSecond
            | DaoThreeInputWithdrawalMode::MixedBothSecond
            | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedBothThird
    ) {
        let mixed_withdraw_header = dao_test_header(
            ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
            ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
            ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
            ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
            ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
        );
        let hash = mixed_withdraw_header.hash();
        context.insert_header(mixed_withdraw_header);
        Some(hash)
    } else {
        None
    };

    let mut withdrawing_out_points = Vec::new();
    for index in 0..3 {
        let out_point = context.create_cell(
            packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
                .lock(always_success_lock.clone())
                .type_(packed::ScriptOpt::from(dao_script.clone()))
                .build(),
            dao_three_input_cell_data(mode, index),
        );
        let linked_withdraw_header_hash = match (index, mode) {
            (
                1,
                DaoThreeInputWithdrawalMode::MixedWithdrawSecond
                | DaoThreeInputWithdrawalMode::MixedBothSecond
                | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird,
            )
            | (
                2,
                DaoThreeInputWithdrawalMode::MixedWithdrawThird
                | DaoThreeInputWithdrawalMode::MixedBothThird
                | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird,
            ) => mixed_withdraw_header_hash.clone().expect("mixed withdraw header for three-input DAO fixture"),
            _ => withdraw_header_hash.clone(),
        };
        context.link_cell_with_block(out_point.clone(), linked_withdraw_header_hash, 0);
        withdrawing_out_points.push(out_point);
    }

    let output =
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(output_capacity.pack()).lock(always_success_lock).build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .cell_dep(packed::CellDep::new_builder().out_point(dao_code_out_point).dep_type(DepType::Code).build())
        .header_dep(withdraw_header_hash)
        .header_dep(deposit_header_hash)
        .output(outputs[0].clone())
        .output_data(outputs_data[0].clone().pack());
    if let Some(mixed_deposit_header_hash) = mixed_deposit_header_hash {
        tx_builder = tx_builder.header_dep(mixed_deposit_header_hash);
    }
    if let Some(mixed_withdraw_header_hash) = mixed_withdraw_header_hash {
        tx_builder = tx_builder.header_dep(mixed_withdraw_header_hash);
    }
    for out_point in withdrawing_out_points {
        tx_builder = tx_builder.input(
            packed::CellInput::new_builder().previous_output(out_point).since(ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE).build(),
        );
    }
    for index in 0..3 {
        let witness = dao_three_input_witness(mode, index);
        tx_builder = tx_builder.witness(witness.as_bytes().pack());
    }
    let tx = context.complete_tx(tx_builder.build());

    side_run_from_result(
        context.verify_tx(&tx, ORIGINAL_DAO_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY * 3, &outputs),
    )
}

fn run_cellscript_dao_three_input_withdrawal(
    output_capacity: u64,
    mode: DaoThreeInputWithdrawalMode,
) -> (DepositPhase1SideRun, Vec<u8>) {
    let (program, action) = match mode {
        DaoThreeInputWithdrawalMode::SameDeposit => {
            (DAO_THREE_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::MixedDepositSecond => {
            (DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_SECOND_MIXED_DEPOSIT_RATE_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::MixedWithdrawSecond => (
            DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_CELLSCRIPT_PROGRAM,
            DAO_THREE_INPUT_SECOND_MIXED_WITHDRAW_RATE_CELLSCRIPT_ACTION,
        ),
        DaoThreeInputWithdrawalMode::MixedBothSecond => {
            (DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_SECOND_MIXED_BOTH_RATE_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird => (
            DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_CELLSCRIPT_PROGRAM,
            DAO_THREE_INPUT_SECOND_DEPOSIT_THIRD_WITHDRAW_RATE_CELLSCRIPT_ACTION,
        ),
        DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird => (
            DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_CELLSCRIPT_PROGRAM,
            DAO_THREE_INPUT_SECOND_WITHDRAW_THIRD_DEPOSIT_RATE_CELLSCRIPT_ACTION,
        ),
        DaoThreeInputWithdrawalMode::MixedDepositThird => {
            (DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_MIXED_DEPOSIT_RATE_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::MixedWithdrawThird => {
            (DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_MIXED_WITHDRAW_RATE_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::MixedBothThird => {
            (DAO_THREE_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_MIXED_BOTH_RATE_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::SecondDepositDataInput
        | DaoThreeInputWithdrawalMode::SecondMalformedInputData
        | DaoThreeInputWithdrawalMode::SecondLongInputData
        | DaoThreeInputWithdrawalMode::ThirdDepositDataInput
        | DaoThreeInputWithdrawalMode::ThirdMalformedInputData
        | DaoThreeInputWithdrawalMode::ThirdLongInputData => {
            (DAO_THREE_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_WITHDRAWAL_CAPACITY_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::SecondWitnessMissing
        | DaoThreeInputWithdrawalMode::SecondWitnessEmpty
        | DaoThreeInputWithdrawalMode::SecondWitnessShort
        | DaoThreeInputWithdrawalMode::SecondWitnessLong
        | DaoThreeInputWithdrawalMode::ThirdWitnessMissing
        | DaoThreeInputWithdrawalMode::ThirdWitnessEmpty
        | DaoThreeInputWithdrawalMode::ThirdWitnessShort
        | DaoThreeInputWithdrawalMode::ThirdWitnessLong => {
            (DAO_THREE_INPUT_WITNESS_SHAPE_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_WITNESS_SHAPE_CELLSCRIPT_ACTION)
        }
        DaoThreeInputWithdrawalMode::SecondWitnessWithdrawHeaderIndex
        | DaoThreeInputWithdrawalMode::SecondWitnessOutOfBoundsIndex
        | DaoThreeInputWithdrawalMode::ThirdWitnessWithdrawHeaderIndex
        | DaoThreeInputWithdrawalMode::ThirdWitnessOutOfBoundsIndex => {
            (DAO_THREE_INPUT_WITNESS_INDEX_CELLSCRIPT_PROGRAM, DAO_THREE_INPUT_WITNESS_INDEX_CELLSCRIPT_ACTION)
        }
    };
    let cellscript_elf = compile_cellscript_source_to_elf(program, action, None);
    let mut context = ckb_testtool::context::Context::new_with_deterministic_rng();
    let cellscript_out_point = context.deploy_cell(Bytes::copy_from_slice(&cellscript_elf));
    let cellscript_script =
        context.build_script(&cellscript_out_point, Bytes::default()).expect("CellScript DAO three-input withdrawal script");
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
    let mixed_deposit_header_hash = if matches!(
        mode,
        DaoThreeInputWithdrawalMode::MixedDepositSecond
            | DaoThreeInputWithdrawalMode::MixedBothSecond
            | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            | DaoThreeInputWithdrawalMode::MixedDepositThird
            | DaoThreeInputWithdrawalMode::MixedBothThird
    ) {
        let mixed_deposit_header = dao_test_header(
            ORIGINAL_DAO_WITHDRAW_PHASE1_BLOCK,
            ORIGINAL_DAO_WITHDRAW_PHASE1_WRONG_ACCUMULATED_RATE,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_NUMBER,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_INDEX,
            ORIGINAL_DAO_WITHDRAW_PHASE1_EPOCH_LENGTH,
        );
        let hash = mixed_deposit_header.hash();
        context.insert_header(mixed_deposit_header);
        Some(hash)
    } else {
        None
    };
    let withdraw_header = dao_test_header(
        ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
        ORIGINAL_DAO_WITHDRAW_PHASE2_ACCUMULATED_RATE,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
        ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
    );
    let withdraw_header_hash = withdraw_header.hash();
    context.insert_header(withdraw_header);
    let mixed_withdraw_header_hash = if matches!(
        mode,
        DaoThreeInputWithdrawalMode::MixedWithdrawSecond
            | DaoThreeInputWithdrawalMode::MixedBothSecond
            | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird
            | DaoThreeInputWithdrawalMode::MixedWithdrawThird
            | DaoThreeInputWithdrawalMode::MixedBothThird
    ) {
        let mixed_withdraw_header = dao_test_header(
            ORIGINAL_DAO_WITHDRAW_PHASE2_WITHDRAW_BLOCK,
            ORIGINAL_DAO_WITHDRAW_PHASE2_WRONG_ACCUMULATED_RATE,
            ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_NUMBER,
            ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_INDEX,
            ORIGINAL_DAO_WITHDRAW_PHASE2_EPOCH_LENGTH,
        );
        let hash = mixed_withdraw_header.hash();
        context.insert_header(mixed_withdraw_header);
        Some(hash)
    } else {
        None
    };

    let mut withdrawing_out_points = Vec::new();
    for index in 0..3 {
        let out_point = context.create_cell(
            packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY.pack())
                .lock(always_success_lock.clone())
                .type_(packed::ScriptOpt::from(cellscript_script.clone()))
                .build(),
            dao_three_input_cell_data(mode, index),
        );
        let linked_withdraw_header_hash = match (index, mode) {
            (
                1,
                DaoThreeInputWithdrawalMode::MixedWithdrawSecond
                | DaoThreeInputWithdrawalMode::MixedBothSecond
                | DaoThreeInputWithdrawalMode::MixedWithdrawSecondDepositThird,
            )
            | (
                2,
                DaoThreeInputWithdrawalMode::MixedWithdrawThird
                | DaoThreeInputWithdrawalMode::MixedBothThird
                | DaoThreeInputWithdrawalMode::MixedDepositSecondWithdrawThird,
            ) => mixed_withdraw_header_hash.clone().expect("mixed withdraw header for three-input DAO fixture"),
            _ => withdraw_header_hash.clone(),
        };
        context.link_cell_with_block(out_point.clone(), linked_withdraw_header_hash, 0);
        withdrawing_out_points.push(out_point);
    }

    let output =
        packed::CellOutput::new_builder().capacity::<packed::Uint64>(output_capacity.pack()).lock(always_success_lock).build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let mut tx_builder = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .cell_dep(packed::CellDep::new_builder().out_point(cellscript_out_point).dep_type(DepType::Code).build())
        .header_dep(withdraw_header_hash)
        .header_dep(deposit_header_hash)
        .output(outputs[0].clone())
        .output_data(outputs_data[0].clone().pack());
    if let Some(mixed_deposit_header_hash) = mixed_deposit_header_hash {
        tx_builder = tx_builder.header_dep(mixed_deposit_header_hash);
    }
    if let Some(mixed_withdraw_header_hash) = mixed_withdraw_header_hash {
        tx_builder = tx_builder.header_dep(mixed_withdraw_header_hash);
    }
    for out_point in withdrawing_out_points {
        tx_builder = tx_builder.input(
            packed::CellInput::new_builder().previous_output(out_point).since(ORIGINAL_DAO_WITHDRAW_PHASE2_MATURE_SINCE).build(),
        );
    }
    for index in 0..3 {
        let witness = dao_three_input_witness(mode, index);
        tx_builder = tx_builder.witness(witness.as_bytes().pack());
    }
    let tx = context.complete_tx(tx_builder.build());

    let run = side_run_from_result(
        context.verify_tx(&tx, ORIGINAL_DAO_MAX_CYCLES),
        &tx,
        occupied_capacity_shannons(&outputs, &outputs_data),
        fee_shannons(ORIGINAL_DAO_WITHDRAW_PHASE1_CAPACITY * 3, &outputs),
    );
    (run, cellscript_elf)
}

// ---------------------------------------------------------------------------
// Original iCKB Rust script CKB VM execution tests
// ---------------------------------------------------------------------------
//
// These tests prove that the ORIGINAL iCKB Rust script (from ickb/contracts)
// can execute under real CKB VM/syscall environment via ckb-testtool.
// This is the first step towards FULL DIFFERENTIAL equivalence:
// both the original iCKB and CellScript-generated scripts must pass/fail
// consistently on the same transaction fixtures.
//
// The original iCKB Logic script (entry.rs) does:
// 1. has_empty_args() - checks script args are empty
// 2. load_script_hash() - gets its own hash
// 3. Iterates over Input/Output cells, classifying as Deposit/Receipt/Udt
// 4. Checks: in_udt + in_receipts == out_udt + in_deposits
//
// Deposit cells: lock=ICKB Logic, type=DAO, data=8 zeros
// Receipt cells: type=ICKB Logic, data=quantity(u32 LE) + amount(u64 LE)
// UDT cells: type=xUDT with iCKB Logic hash in args
//
// KNOWN LIMITATION: The original iCKB Logic script uses hardcoded DAO_HASH
// (script hash with hash_type=Type) to classify deposit cells. In ckb-testtool,
// `build_script` uses data_hash as code_hash with hash_type=Data, producing a
// different script hash than the on-chain DAO_HASH. This means the original
// iCKB Logic script cannot classify DAO deposit cells in ckb-testtool.
// Deposit/withdrawal scenario tests require the exact on-chain DAO binary
// and type_id configuration, or a real CKB node.
//
// What we CAN test:
// - Non-empty args rejection (no DAO needed)
// - Original iCKB binary loads and executes in CKB VM
// - DAO binary deployment and script hash computation
