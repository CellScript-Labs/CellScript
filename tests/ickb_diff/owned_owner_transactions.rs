fn build_owned_owner_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
    owner_relative_distance: i32,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let owned_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNED_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owned_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
            .build(),
        owned_owner_withdrawal_request_data(),
    );
    let owner_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNER_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owner_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        owned_owner_distance_data(owner_relative_distance),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(owned_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(owner_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_pair_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
    owner_relative_distance: i32,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let owned_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
        .build();
    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![owned_output, owner_output];
    let outputs_data = vec![owned_owner_withdrawal_request_data(), owned_owner_distance_data(owner_relative_distance)];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_duplicate_owner_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>((OWNED_OWNER_INPUT_CAPACITY * 3).pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let owned_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
        .build();
    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let duplicate_owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![owned_output, owner_output, duplicate_owner_output];
    let outputs_data = vec![
        owned_owner_withdrawal_request_data(),
        owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE),
        owned_owner_distance_data(OWNED_OWNER_OUTPUT_DUPLICATE_OWNER_DISTANCE),
    ];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_missing_owner_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>((OWNED_OWNER_INPUT_CAPACITY * 3).pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let missing_owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
        .build();
    let paired_owned_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
        .build();
    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![missing_owner_output, paired_owned_output, owner_output];
    let outputs_data = vec![
        owned_owner_withdrawal_request_data(),
        owned_owner_withdrawal_request_data(),
        owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE),
    ];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_missing_owned_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![owner_output];
    let outputs_data = vec![owned_owner_distance_data(OWNED_OWNER_OUTPUT_MISMATCH_DISTANCE)];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_script_misuse_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let misused_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![misused_output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_not_withdrawal_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let non_withdrawal_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .build();
    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![non_withdrawal_output, owner_output];
    let outputs_data = vec![Bytes::default(), owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_owner_data_length_mismatch_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let owned_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
        .build();
    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![owned_output, owner_output];
    let outputs_data = vec![owned_owner_withdrawal_request_data(), owned_owner_malformed_distance_data()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_related_type_hash_mismatch_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    actual_wrong_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let owned_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(actual_wrong_type_script.clone()))
        .build();
    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![owned_output, owner_output];
    let outputs_data = vec![owned_owner_withdrawal_request_data(), owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_output_related_data_rule_mismatch_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let funding_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OUTPUT_FUNDING_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        funding_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .build(),
        Bytes::default(),
    );

    let owned_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(script_under_test.clone())
        .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
        .build();
    let owner_output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .type_(packed::ScriptOpt::from(script_under_test.clone()))
        .build();
    let outputs = vec![owned_output, owner_output];
    let outputs_data = vec![owned_owner_deposit_data(), owned_owner_distance_data(OWNED_OWNER_OUTPUT_OWNER_DISTANCE)];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(funding_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_related_type_hash_mismatch_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    actual_wrong_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let owned_out_point = fixed_owned_owner_out_point(OWNED_OWNER_RELATED_TYPE_HASH_MISMATCH_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owned_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(actual_wrong_type_script.clone()))
            .build(),
        owned_owner_withdrawal_request_data(),
    );
    let owner_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNER_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owner_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(owned_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(owner_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_related_data_rule_mismatch_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let owned_out_point = fixed_owned_owner_out_point(OWNED_OWNER_RELATED_DATA_RULE_MISMATCH_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owned_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
            .build(),
        owned_owner_deposit_data(),
    );
    let owner_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNER_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owner_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(owned_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(owner_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_owner_data_length_mismatch_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let owned_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNED_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owned_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
            .build(),
        owned_owner_withdrawal_request_data(),
    );
    let owner_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNER_DATA_LENGTH_MISMATCH_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owner_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        owned_owner_malformed_distance_data(),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_OUTPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(owned_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(owner_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_script_misuse_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let misused_out_point = fixed_owned_owner_out_point(OWNED_OWNER_SCRIPT_MISUSE_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        misused_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        Bytes::default(),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(misused_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_not_withdrawal_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let non_withdrawal_out_point = fixed_owned_owner_out_point(OWNED_OWNER_NOT_WITHDRAWAL_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        non_withdrawal_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .build(),
        Bytes::default(),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(non_withdrawal_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_missing_owner_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let owned_out_point = fixed_owned_owner_out_point(OWNED_OWNER_MISSING_OWNER_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owned_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
            .build(),
        owned_owner_withdrawal_request_data(),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(owned_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_missing_owned_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let owner_out_point = fixed_owned_owner_out_point(OWNED_OWNER_MISSING_OWNED_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owner_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(owner_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}

fn build_owned_owner_duplicate_owner_tx(
    context: &mut ckb_testtool::context::Context,
    script_under_test: &packed::Script,
    withdrawal_type_script: &packed::Script,
    always_success_lock: &packed::Script,
) -> (TransactionView, Vec<packed::CellOutput>, Vec<Bytes>) {
    let owned_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNED_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owned_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(script_under_test.clone())
            .type_(packed::ScriptOpt::from(withdrawal_type_script.clone()))
            .build(),
        owned_owner_withdrawal_request_data(),
    );
    let owner_out_point = fixed_owned_owner_out_point(OWNED_OWNER_OWNER_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        owner_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        owned_owner_distance_data(OWNED_OWNER_VALID_DISTANCE),
    );
    let duplicate_owner_out_point = fixed_owned_owner_out_point(OWNED_OWNER_DUPLICATE_OWNER_OUT_POINT_INDEX);
    context.create_cell_with_out_point(
        duplicate_owner_out_point.clone(),
        packed::CellOutput::new_builder()
            .capacity::<packed::Uint64>(OWNED_OWNER_INPUT_CAPACITY.pack())
            .lock(always_success_lock.clone())
            .type_(packed::ScriptOpt::from(script_under_test.clone()))
            .build(),
        owned_owner_distance_data(OWNED_OWNER_DUPLICATE_OWNER_DISTANCE),
    );

    let output = packed::CellOutput::new_builder()
        .capacity::<packed::Uint64>((OWNED_OWNER_INPUT_CAPACITY * 3).pack())
        .lock(always_success_lock.clone())
        .build();
    let outputs = vec![output];
    let outputs_data = vec![Bytes::default()];
    let tx = ckb_testtool::ckb_types::core::TransactionBuilder::default()
        .input(packed::CellInput::new_builder().previous_output(owned_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(owner_out_point).build())
        .input(packed::CellInput::new_builder().previous_output(duplicate_owner_out_point).build())
        .outputs(outputs.clone())
        .outputs_data(outputs_data.clone().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack())
        .witness(Bytes::default().pack())
        .build();
    let tx = context.complete_tx(tx);
    (tx, outputs, outputs_data)
}
