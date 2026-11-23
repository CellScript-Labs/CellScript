fn build_limit_order_tx_with_master_binding(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    input_auxiliary_type_script: &packed::Script,
    output_auxiliary_type_script: &packed::Script,
    params: LimitOrderBuildParams,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let input_data = limit_order_input_data_for_mode(params.input_udt_amount, params.input_data_mode);
    let input_out_point = fixed_limit_order_input_out_point();
    context.create_cell_with_out_point(
        input_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(input_auxiliary_type_script.clone()))
            .build(),
        input_data,
    );

    let output_data = limit_order_output_data_for_mode(params.output_udt_amount, params.master_binding, params.output_data_mode);
    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(params.output_capacity.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(output_auxiliary_type_script.clone()))
        .build();
    let outputs = vec![output];
    let outputs_data = vec![output_data];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_limit_order_udt_to_ckb_tx_with_master_binding(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    input_auxiliary_type_script: &packed::Script,
    output_auxiliary_type_script: &packed::Script,
    params: LimitOrderBuildParams,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let input_data = limit_order_udt_to_ckb_input_data_for_mode(params.input_udt_amount, params.input_data_mode);
    let input_out_point = fixed_limit_order_input_out_point();
    context.create_cell_with_out_point(
        input_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(input_auxiliary_type_script.clone()))
            .build(),
        input_data,
    );
    let always_success_lock = deploy_always_success_lock(context);
    let funding_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY.pack())
            .lock(always_success_lock)
            .build(),
        Bytes::default(),
    );

    let output_data =
        limit_order_udt_to_ckb_output_data_for_mode(params.output_udt_amount, params.master_binding, params.output_data_mode);
    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(params.output_capacity.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(output_auxiliary_type_script.clone()))
        .build();
    let outputs = vec![output];
    let outputs_data = vec![output_data];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_limit_order_tx_with_cell_shape(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    auxiliary_type_script: &packed::Script,
    shape: LimitOrderCellShape,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let input_out_point = fixed_limit_order_input_out_point();
    context.create_cell_with_out_point(
        input_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(auxiliary_type_script.clone()))
            .build(),
        limit_order_mint_data(LIMIT_ORDER_INPUT_UDT_AMOUNT, 0),
    );

    let always_success_lock = deploy_always_success_lock(context);
    let (outputs, outputs_data) = match shape {
        LimitOrderCellShape::MissingMatchingOutput => {
            let output = packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(LIMIT_ORDER_OUTPUT_CAPACITY.pack())
                .lock(always_success_lock)
                .type_(packed::ScriptOpt::from(auxiliary_type_script.clone()))
                .build();
            let output_data = limit_order_match_data(LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
            (vec![output], vec![output_data])
        }
        LimitOrderCellShape::DuplicateMatchingOutputs => {
            let first_output = packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(LIMIT_ORDER_DUPLICATE_FIRST_OUTPUT_CAPACITY.pack())
                .lock(script_under_test.clone())
                .type_(packed::ScriptOpt::from(auxiliary_type_script.clone()))
                .build();
            let second_output = packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(LIMIT_ORDER_DUPLICATE_SECOND_OUTPUT_CAPACITY.pack())
                .lock(script_under_test.clone())
                .type_(packed::ScriptOpt::from(auxiliary_type_script.clone()))
                .build();
            let first_data = limit_order_match_data(LIMIT_ORDER_DUPLICATE_FIRST_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
            let second_data = limit_order_match_data(LIMIT_ORDER_DUPLICATE_SECOND_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
            (vec![first_output, second_output], vec![first_data, second_data])
        }
    };
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_limit_order_tx_with_type_shape(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    input_auxiliary_type_script: &packed::Script,
    output_auxiliary_type_script: &packed::Script,
    shape: LimitOrderTypeShape,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let input_out_point = fixed_limit_order_input_out_point();
    context.create_cell_with_out_point(
        input_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(script_opt_if_present(input_auxiliary_type_script, shape.input_type_present()))
            .build(),
        limit_order_mint_data(LIMIT_ORDER_INPUT_UDT_AMOUNT, 0),
    );

    let output_data = limit_order_match_data(LIMIT_ORDER_VALID_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(LIMIT_ORDER_OUTPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(script_opt_if_present(output_auxiliary_type_script, shape.output_type_present()))
        .build();
    let outputs = vec![output];
    let outputs_data = vec![output_data];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_limit_order_udt_to_ckb_tx_with_cell_shape(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    input_auxiliary_type_script: &packed::Script,
    output_auxiliary_type_script: &packed::Script,
    shape: LimitOrderCellShape,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let input_out_point = fixed_limit_order_input_out_point();
    context.create_cell_with_out_point(
        input_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(input_auxiliary_type_script.clone()))
            .build(),
        limit_order_udt_to_ckb_mint_data(LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT, 0),
    );
    let always_success_lock = deploy_always_success_lock(context);
    let funding_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(limit_order_udt_to_ckb_funding_capacity_for_cell_shape(shape).pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );
    let (outputs, outputs_data) = match shape {
        LimitOrderCellShape::MissingMatchingOutput => {
            let output = packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY.pack())
                .lock(always_success_lock)
                .type_(packed::ScriptOpt::from(output_auxiliary_type_script.clone()))
                .build();
            let output_data =
                limit_order_udt_to_ckb_match_data(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
            (vec![output], vec![output_data])
        }
        LimitOrderCellShape::DuplicateMatchingOutputs => {
            let first_output = packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_FIRST_OUTPUT_CAPACITY.pack())
                .lock(script_under_test.clone())
                .type_(packed::ScriptOpt::from(output_auxiliary_type_script.clone()))
                .build();
            let second_output = packed::CellOutput::new_builder()
                .capacity::<packed::Uint64>(LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_SECOND_OUTPUT_CAPACITY.pack())
                .lock(script_under_test.clone())
                .type_(packed::ScriptOpt::from(output_auxiliary_type_script.clone()))
                .build();
            let first_data =
                limit_order_udt_to_ckb_match_data(LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
            let second_data =
                limit_order_udt_to_ckb_match_data(LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
            (vec![first_output, second_output], vec![first_data, second_data])
        }
    };
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_limit_order_udt_to_ckb_tx_with_type_shape(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    input_auxiliary_type_script: &packed::Script,
    output_auxiliary_type_script: &packed::Script,
    shape: LimitOrderTypeShape,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let input_out_point = fixed_limit_order_input_out_point();
    context.create_cell_with_out_point(
        input_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(script_opt_if_present(input_auxiliary_type_script, shape.input_type_present()))
            .build(),
        limit_order_udt_to_ckb_mint_data(LIMIT_ORDER_UDT_TO_CKB_INPUT_UDT_AMOUNT, 0),
    );
    let always_success_lock = deploy_always_success_lock(context);
    let funding_out_point = context.create_cell(
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY.pack())
            .lock(always_success_lock)
            .build(),
        Bytes::default(),
    );

    let output_data = limit_order_udt_to_ckb_match_data(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_UDT_AMOUNT, &LIMIT_ORDER_MASTER_TX_HASH, 0);
    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(LIMIT_ORDER_UDT_TO_CKB_OUTPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(script_opt_if_present(output_auxiliary_type_script, shape.output_type_present()))
        .build();
    let outputs = vec![output];
    let outputs_data = vec![output_data];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(input_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn limit_order_udt_to_ckb_funding_capacity_for_cell_shape(shape: LimitOrderCellShape) -> u64 {
    match shape {
        LimitOrderCellShape::MissingMatchingOutput => LIMIT_ORDER_UDT_TO_CKB_FUNDING_CAPACITY,
        LimitOrderCellShape::DuplicateMatchingOutputs => LIMIT_ORDER_UDT_TO_CKB_DUPLICATE_FUNDING_CAPACITY,
    }
}

fn limit_order_udt_to_ckb_input_capacity_for_cell_shape(shape: LimitOrderCellShape) -> u64 {
    LIMIT_ORDER_INPUT_CAPACITY + limit_order_udt_to_ckb_funding_capacity_for_cell_shape(shape)
}

fn fixed_limit_order_input_out_point() -> packed::OutPoint {
    packed::OutPoint::new_builder().tx_hash(LIMIT_ORDER_MASTER_TX_HASH.pack()).index(0u32).build()
}

fn script_opt_if_present(script: &packed::Script, present: bool) -> packed::ScriptOpt {
    if present {
        packed::ScriptOpt::from(script.clone())
    } else {
        packed::ScriptOpt::default()
    }
}

fn fixed_owned_owner_out_point(index: u32) -> packed::OutPoint {
    packed::OutPoint::new_builder().tx_hash(OWNED_OWNER_TX_HASH.pack()).index(index).build()
}

fn owned_owner_distance_data(distance: i32) -> Bytes {
    Bytes::from(distance.to_le_bytes().to_vec())
}

fn owned_owner_malformed_distance_data() -> Bytes {
    Bytes::from(vec![0x01, 0x00, 0x00])
}

fn owned_owner_withdrawal_request_data() -> Bytes {
    Bytes::from(1u64.to_le_bytes().to_vec())
}

fn owned_owner_deposit_data() -> Bytes {
    Bytes::from(0u64.to_le_bytes().to_vec())
}

fn limit_order_mint_data(udt_amount: u128, master_distance: i32) -> Bytes {
    let mut data = Vec::new();
    data.extend_from_slice(&udt_amount.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 32]);
    data.extend_from_slice(&master_distance.to_le_bytes());
    append_limit_order_info(&mut data);
    Bytes::from(data)
}

fn limit_order_input_data_for_mode(udt_amount: u128, mode: LimitOrderInputDataMode) -> Bytes {
    match mode {
        LimitOrderInputDataMode::Mint => limit_order_mint_data(udt_amount, 0),
        LimitOrderInputDataMode::MatchAbsolute => limit_order_match_data(udt_amount, &LIMIT_ORDER_MASTER_TX_HASH, 0),
        LimitOrderInputDataMode::MatchWrongTxHash => limit_order_match_data(udt_amount, &LIMIT_ORDER_WRONG_MASTER_TX_HASH, 0),
        LimitOrderInputDataMode::MatchWrongIndex => limit_order_match_data(udt_amount, &LIMIT_ORDER_MASTER_TX_HASH, 1),
        LimitOrderInputDataMode::InvalidAction => limit_order_invalid_action_data(udt_amount),
        LimitOrderInputDataMode::ShortAction => limit_order_short_action_data(udt_amount),
        LimitOrderInputDataMode::ShortMasterOutPoint => limit_order_short_match_data(udt_amount),
        LimitOrderInputDataMode::LongTrailingData => limit_order_long_match_data(udt_amount),
    }
}

fn limit_order_invalid_action_data(udt_amount: u128) -> Bytes {
    let mut data = Vec::new();
    data.extend_from_slice(&udt_amount.to_le_bytes());
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 32]);
    data.extend_from_slice(&0i32.to_le_bytes());
    append_limit_order_info(&mut data);
    Bytes::from(data)
}

fn limit_order_short_action_data(udt_amount: u128) -> Bytes {
    Bytes::from(udt_amount.to_le_bytes().to_vec())
}

fn limit_order_match_data(udt_amount: u128, master_tx_hash: &[u8; 32], master_index: u32) -> Bytes {
    let mut data = Vec::new();
    data.extend_from_slice(&udt_amount.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(master_tx_hash);
    data.extend_from_slice(&master_index.to_le_bytes());
    append_limit_order_info(&mut data);
    Bytes::from(data)
}

fn limit_order_output_data_for_mode(
    udt_amount: u128,
    master_binding: LimitOrderMasterBinding,
    mode: LimitOrderOutputDataMode,
) -> Bytes {
    match mode {
        LimitOrderOutputDataMode::Match => {
            limit_order_match_data(udt_amount, master_binding.master_tx_hash(), master_binding.master_index())
        }
        LimitOrderOutputDataMode::MintAction => limit_order_mint_data(udt_amount, 0),
        LimitOrderOutputDataMode::InvalidAction => limit_order_invalid_action_data(udt_amount),
        LimitOrderOutputDataMode::ShortAction => limit_order_short_action_data(udt_amount),
        LimitOrderOutputDataMode::ShortMasterOutPoint => limit_order_short_match_data(udt_amount),
        LimitOrderOutputDataMode::LongTrailingData => limit_order_long_match_data(udt_amount),
    }
}

fn limit_order_short_match_data(udt_amount: u128) -> Bytes {
    let mut data = Vec::new();
    data.extend_from_slice(&udt_amount.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 8]);
    Bytes::from(data)
}

fn limit_order_long_match_data(udt_amount: u128) -> Bytes {
    let mut data = limit_order_match_data(udt_amount, &LIMIT_ORDER_MASTER_TX_HASH, 0).to_vec();
    data.push(0x99);
    Bytes::from(data)
}

fn limit_order_udt_to_ckb_mint_data(udt_amount: u128, master_distance: i32) -> Bytes {
    let mut data = Vec::new();
    data.extend_from_slice(&udt_amount.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 32]);
    data.extend_from_slice(&master_distance.to_le_bytes());
    append_limit_order_udt_to_ckb_info(&mut data);
    Bytes::from(data)
}

fn limit_order_udt_to_ckb_match_data(udt_amount: u128, master_tx_hash: &[u8; 32], master_index: u32) -> Bytes {
    let mut data = Vec::new();
    data.extend_from_slice(&udt_amount.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(master_tx_hash);
    data.extend_from_slice(&master_index.to_le_bytes());
    append_limit_order_udt_to_ckb_info(&mut data);
    Bytes::from(data)
}

fn limit_order_udt_to_ckb_input_data_for_mode(udt_amount: u128, mode: LimitOrderInputDataMode) -> Bytes {
    match mode {
        LimitOrderInputDataMode::Mint => limit_order_udt_to_ckb_mint_data(udt_amount, 0),
        LimitOrderInputDataMode::MatchAbsolute => limit_order_udt_to_ckb_match_data(udt_amount, &LIMIT_ORDER_MASTER_TX_HASH, 0),
        LimitOrderInputDataMode::MatchWrongTxHash => {
            limit_order_udt_to_ckb_match_data(udt_amount, &LIMIT_ORDER_WRONG_MASTER_TX_HASH, 0)
        }
        LimitOrderInputDataMode::MatchWrongIndex => limit_order_udt_to_ckb_match_data(udt_amount, &LIMIT_ORDER_MASTER_TX_HASH, 1),
        LimitOrderInputDataMode::InvalidAction => limit_order_udt_to_ckb_invalid_action_data(udt_amount),
        LimitOrderInputDataMode::ShortAction => limit_order_short_action_data(udt_amount),
        LimitOrderInputDataMode::ShortMasterOutPoint => limit_order_short_match_data(udt_amount),
        LimitOrderInputDataMode::LongTrailingData => limit_order_udt_to_ckb_long_match_data(udt_amount),
    }
}

fn limit_order_udt_to_ckb_invalid_action_data(udt_amount: u128) -> Bytes {
    let mut data = Vec::new();
    data.extend_from_slice(&udt_amount.to_le_bytes());
    data.extend_from_slice(&2u32.to_le_bytes());
    data.extend_from_slice(&[0u8; 32]);
    data.extend_from_slice(&0i32.to_le_bytes());
    append_limit_order_udt_to_ckb_info(&mut data);
    Bytes::from(data)
}

fn limit_order_udt_to_ckb_output_data_for_mode(
    udt_amount: u128,
    master_binding: LimitOrderMasterBinding,
    mode: LimitOrderOutputDataMode,
) -> Bytes {
    match mode {
        LimitOrderOutputDataMode::Match => {
            limit_order_udt_to_ckb_match_data(udt_amount, master_binding.master_tx_hash(), master_binding.master_index())
        }
        LimitOrderOutputDataMode::MintAction => limit_order_udt_to_ckb_mint_data(udt_amount, 0),
        LimitOrderOutputDataMode::InvalidAction => limit_order_udt_to_ckb_invalid_action_data(udt_amount),
        LimitOrderOutputDataMode::ShortAction => limit_order_short_action_data(udt_amount),
        LimitOrderOutputDataMode::ShortMasterOutPoint => limit_order_short_match_data(udt_amount),
        LimitOrderOutputDataMode::LongTrailingData => limit_order_udt_to_ckb_long_match_data(udt_amount),
    }
}

fn limit_order_udt_to_ckb_long_match_data(udt_amount: u128) -> Bytes {
    let mut data = limit_order_udt_to_ckb_match_data(udt_amount, &LIMIT_ORDER_MASTER_TX_HASH, 0).to_vec();
    data.push(0x99);
    Bytes::from(data)
}

fn append_limit_order_info(data: &mut Vec<u8>) {
    data.extend_from_slice(&LIMIT_ORDER_CKB_TO_UDT_MUL.to_le_bytes());
    data.extend_from_slice(&LIMIT_ORDER_UDT_TO_CKB_MUL.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.push(LIMIT_ORDER_CKB_MIN_MATCH_LOG);
}

fn append_limit_order_udt_to_ckb_info(data: &mut Vec<u8>) {
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&0u64.to_le_bytes());
    data.extend_from_slice(&LIMIT_ORDER_CKB_TO_UDT_MUL.to_le_bytes());
    data.extend_from_slice(&LIMIT_ORDER_UDT_TO_CKB_MUL.to_le_bytes());
    data.push(LIMIT_ORDER_CKB_MIN_MATCH_LOG);
}

fn deposit_phase1_receipt_data(deposit_capacity: u64) -> Bytes {
    deposit_phase1_receipt_data_with(1, deposit_phase1_unoccupied_capacity(deposit_capacity))
}

fn deposit_phase1_receipt_data_with(quantity: u32, receipt_deposit_amount: u64) -> Bytes {
    let mut receipt_data = Vec::new();
    receipt_data.extend_from_slice(&quantity.to_le_bytes());
    receipt_data.extend_from_slice(&receipt_deposit_amount.to_le_bytes());
    Bytes::from(receipt_data)
}

fn mint_receipt_data() -> Bytes {
    let (quantity, deposit_amount) = receipt_fields_for_mode(MintReceiptDataMode::Valid, 0);
    mint_receipt_data_with(quantity, deposit_amount)
}

fn mint_receipt_data_with(quantity: u32, deposit_amount: u64) -> Bytes {
    let mut receipt_data = Vec::new();
    receipt_data.extend_from_slice(&quantity.to_le_bytes());
    receipt_data.extend_from_slice(&deposit_amount.to_le_bytes());
    Bytes::from(receipt_data)
}

fn malformed_mint_receipt_data() -> Bytes {
    Bytes::from(MINT_RECEIPT_QUANTITY.to_le_bytes().to_vec())
}

fn receipt_group_input_data(mode: MintReceiptDataMode, input_index: usize) -> Bytes {
    match (mode, input_index) {
        (MintReceiptDataMode::MalformedFirstInput, 0) => malformed_mint_receipt_data(),
        (MintReceiptDataMode::MalformedSecondInput, 1) => malformed_mint_receipt_data(),
        (MintReceiptDataMode::LongTrailingData, _) => {
            let (quantity, deposit_amount) = receipt_fields_for_mode(mode, input_index);
            let mut receipt_data = mint_receipt_data_with(quantity, deposit_amount).to_vec();
            receipt_data.push(0x99);
            Bytes::from(receipt_data)
        }
        _ => {
            let (quantity, deposit_amount) = receipt_fields_for_mode(mode, input_index);
            mint_receipt_data_with(quantity, deposit_amount)
        }
    }
}

fn receipt_fields_for_mode(mode: MintReceiptDataMode, input_index: usize) -> (u32, u64) {
    match (mode, input_index) {
        (MintReceiptDataMode::QuantityZero, _) => (0, MINT_RECEIPT_DEPOSIT_AMOUNT),
        (MintReceiptDataMode::QuantityTwo, _) => (2, MINT_RECEIPT_DEPOSIT_AMOUNT),
        (MintReceiptDataMode::ZeroFirstQuantity, 0) => (0, MINT_RECEIPT_DEPOSIT_AMOUNT),
        (MintReceiptDataMode::MixedQuantities, 1) => (MINT_RECEIPT_MIXED_SECOND_QUANTITY, MINT_RECEIPT_MIXED_SECOND_DEPOSIT_AMOUNT),
        _ => (MINT_RECEIPT_QUANTITY, MINT_RECEIPT_DEPOSIT_AMOUNT),
    }
}

fn expected_mint_for_receipt_mode(mode: MintReceiptDataMode, input_index: usize) -> u128 {
    let (quantity, deposit_amount) = receipt_fields_for_mode(mode, input_index);
    u128::from(quantity) * u128::from(deposit_amount)
}

fn xudt_output_data(amount: u128) -> Bytes {
    Bytes::from(amount.to_le_bytes().to_vec())
}

fn receipt_without_deposit_data() -> Bytes {
    let mut receipt_data = Vec::new();
    receipt_data.extend_from_slice(&1u32.to_le_bytes());
    receipt_data.extend_from_slice(&100_000_000_000u64.to_le_bytes());
    Bytes::from(receipt_data)
}

fn non_empty_script_args() -> Bytes {
    Bytes::from(vec![42u8; 4])
}

fn ckb_epoch_relative_since(number: u64, index: u64, length: u64) -> u64 {
    (1u64 << 63) | 0x2000_0000_0000_0000 | number | (index << 24) | (length << 40)
}

fn dao_test_header(number: u64, accumulated_rate: u64, epoch_number: u64, epoch_index: u64, epoch_length: u64) -> HeaderView {
    HeaderBuilder::default()
        .number(number)
        .epoch(EpochNumberWithFraction::new(epoch_number, epoch_index, epoch_length))
        .dao(ckb_script_runner::make_dao_field(accumulated_rate).pack())
        .build()
}

fn deposit_phase1_unoccupied_capacity(deposit_capacity: u64) -> u64 {
    let occupied: u64 = (37 + 37 + 8) as u64 * 100_000_000;
    deposit_capacity.saturating_sub(occupied)
}

fn side_run_from_result<E: std::fmt::Debug>(
    result: Result<u64, E>,
    tx: &TransactionView,
    occupied_capacity_shannons: u64,
    fee_shannons: u64,
) -> DepositPhase1SideRun {
    let tx_bytes = tx.data().as_bytes();
    let tx_context_sha256 = sha256_prefixed(tx_bytes.as_ref());
    let tx_size_bytes = tx_bytes.len() as u64;
    match result {
        Ok(cycles) => DepositPhase1SideRun {
            status: "pass",
            exit_code: 0,
            cycles,
            tx_context_sha256,
            tx_size_bytes,
            occupied_capacity_shannons,
            fee_shannons,
            error: None,
        },
        Err(error) => {
            let error = format!("{error:?}");
            DepositPhase1SideRun {
                status: "fail",
                exit_code: parse_ckb_script_error_code(&error).unwrap_or(-1),
                cycles: 0,
                tx_context_sha256,
                tx_size_bytes,
                occupied_capacity_shannons,
                fee_shannons,
                error: Some(error),
            }
        }
    }
}
