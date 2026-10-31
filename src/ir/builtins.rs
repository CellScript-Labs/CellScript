//! Resolved source binding and builtin-call lowering.
use super::*;

impl IrGenerator<'_> {
    pub(super) fn try_lower_builtin_call(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        if let Expr::FieldAccess(method) = call.func.as_ref()
            && let Some(read) = crate::cell_selection::method(&method.field)
            && self.selection_receiver_type(&method.expr, vars).is_some()
        {
            let lowered = self.lower_expr(&method.expr, current, blocks, vars);
            let mut active = lowered.current?;
            let selection = lowered.operand;
            let IrType::Named(owner) = self.operand_type(&selection) else { return None };
            let contract = crate::cell_selection::parse_selection_type(&owner)?;
            let role = contract.selected?;
            let index = if read.indexed() {
                let lowered = self.lower_expr(call.args.first()?, active, blocks, vars);
                active = lowered.current?;
                Some(lowered.operand)
            } else {
                None
            };
            let ty = if read.indexed() { IrType::Named(read.result_type(&contract.schemas[role as usize])) } else { IrType::U64 };
            let dest = self.new_var("selected_role_read", ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::ReadExactCellRole {
                dest: dest.clone(),
                selection,
                role,
                read,
                index,
            });
            return Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) });
        }
        match call.func.as_ref() {
            Expr::Identifier(name) => match name.as_str() {
                "ckb::verify_cell_roles" | "ckb::select_cell_roles" => {
                    let mut result = IrOperand::Const(IrConst::Unit);
                    match crate::cell_selection::resolve_call(call, &self.cell_role_catalogs) {
                        Ok(mut contract) => {
                            if contract.roles.iter().any(|role| {
                                self.type_fields.get(&role.type_name).is_some_and(|fields| {
                                    fields.values().any(|ty| {
                                        matches!(crate::nested_values::ir_type(ty), Some(crate::nested_values::NestedType::Values(_)))
                                    })
                                })
                            }) {
                                contract.profile = crate::cell_selection::BOUNDED_PROFILE.into();
                            }
                            let dest = (name == crate::cell_selection::SELECT).then(|| {
                                self.new_var("exact_selection", IrType::Named(crate::cell_selection::selection_type(&contract)))
                            });
                            if let Some(dest) = &dest {
                                result = IrOperand::Var(dest.clone());
                            }
                            self.block_mut(blocks, current).instructions.push(IrInstruction::VerifyExactCellRoles { dest, contract })
                        }
                        Err(error) => self.errors.borrow_mut().push(error),
                    }
                    Some(LoweredExpr { operand: result, current: Some(current) })
                }
                name if crate::cell_selection::read(name).is_some() => {
                    let read = crate::cell_selection::read(name)?;
                    let Expr::Integer(role) = call.args.get(1)? else { return None };
                    let role = u32::try_from(*role).ok()?;
                    let lowered = self.lower_expr(call.args.first()?, current, blocks, vars);
                    let mut active = lowered.current?;
                    let selection = lowered.operand;
                    let index = if read.indexed() {
                        let lowered = self.lower_expr(call.args.get(2)?, active, blocks, vars);
                        active = lowered.current?;
                        Some(lowered.operand)
                    } else {
                        None
                    };
                    // Keep executable views bound to the verified role's schema.
                    // The source type argument may be another alias of that same
                    // declaration; it must not introduce a second runtime layout.
                    let IrType::Named(owner) = self.operand_type(&selection) else { return None };
                    let selected = crate::cell_selection::parse_selection_type(&owner)?;
                    let element = selected.schemas.get(role as usize)?;
                    let ty = if read.indexed() { IrType::Named(read.result_type(element)) } else { IrType::U64 };
                    let dest = self.new_var("selected_role_read", ty);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::ReadExactCellRole {
                        dest: dest.clone(),
                        selection,
                        role,
                        read,
                        index,
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "__cellscript_consume_each" => {
                    if let Some(lowered) = self.lower_bounded_consume_each(call, current, blocks, vars) {
                        return Some(lowered);
                    }
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: None,
                        func: BOUNDED_CONSUME_EACH_FAIL_CLOSED_HELPER.to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) })
                }
                "__cellscript_create_each" => {
                    if let Some(lowered) = self.lower_bounded_create_each(call, current, blocks, vars) {
                        return Some(lowered);
                    }
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: None,
                        func: BOUNDED_CREATE_EACH_FAIL_CLOSED_HELPER.to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) })
                }
                "Address::zero" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Address([0; 32])), current: Some(current) })
                }
                "Hash::zero" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(current) })
                }
                "Hash::from_bytes" | "VerificationKeyCommitment::from_bytes" if call.args.len() == 1 => {
                    Some(self.lower_hash_from_bytes(&call.args[0], current, blocks, vars, call.span))
                }
                "Hash::from_sighash_all" if call.args.len() == 1 => {
                    let lowered = self.lower_expr(&call.args[0], current, blocks, vars);
                    let active = lowered.current?;
                    let dest = self.new_var("sighash_as_hash", IrType::Hash);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: dest.clone(), src: lowered.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "script::hash_type_data" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) })
                }
                "script::hash_type_type" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::U64(1)), current: Some(current) })
                }
                "script::hash_type_data1" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::U64(2)), current: Some(current) })
                }
                "script::hash_type_data2" if call.args.is_empty() => {
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::U64(4)), current: Some(current) })
                }
                "script::args_empty" if call.args.is_empty() => Some(self.lower_script_args_empty(current, blocks)),
                "script::args" if call.args.len() == 1 => Some(self.lower_script_args(&call.args[0], current, blocks, vars)),
                "script::new" if call.args.len() == 3 => Some(self.lower_script_value(call, current, blocks, vars)),
                "script::hash" if call.args.len() == 1 => Some(self.lower_script_hash(call, current, blocks, vars)),
                "script::require_cell_lock_matches" if call.args.len() == 2 => {
                    Some(self.lower_script_match_requirement(call, true, current, blocks, vars))
                }
                "script::require_cell_type_matches" if call.args.len() == 2 => {
                    Some(self.lower_script_match_requirement(call, false, current, blocks, vars))
                }
                "env::current_timepoint" if call.args.is_empty() => {
                    let dest = self.new_var("current_timepoint", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__env_current_timepoint".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "env::current_timepoint" if call.args.is_empty() => {
                    let dest = self.new_var("current_timepoint", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__env_current_timepoint".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "env::sighash_all" if call.args.len() == 1 => {
                    self.lower_simple_runtime_call("__ckb_sighash_all", "sighash_all", IrType::Hash, &call.args, current, blocks, vars)
                }
                "env::sighash_all_zero_lock" if call.args.len() == 4 => self.lower_simple_runtime_call(
                    "__ckb_sighash_all_zero_lock",
                    "sighash_all_zero_lock",
                    IrType::Named(CKB_SIGHASH_ALL_DIGEST_TYPE.to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::header_epoch_number" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_header_epoch_number", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_header_epoch_number".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "ckb::header_epoch_start_block_number" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_header_epoch_start_block_number", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_header_epoch_start_block_number".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "ckb::header_epoch_length" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_header_epoch_length", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_header_epoch_length".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "ckb::input_since" | "ckb::input_since_raw" if call.args.is_empty() => {
                    let dest = self.new_var("ckb_input_since", IrType::U64);
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_input_since".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "ckb::since_epoch_absolute" if call.args.len() == 3 => self.lower_simple_runtime_call(
                    "__ckb_since_epoch_absolute",
                    "ckb_since_epoch_absolute",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_epoch_relative" if call.args.len() == 3 => self.lower_simple_runtime_call(
                    "__ckb_since_epoch_relative",
                    "ckb_since_epoch_relative",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_absolute_epoch" if call.args.len() == 3 => self.lower_simple_runtime_call(
                    "__ckb_since_epoch_absolute",
                    "ckb_since_absolute_epoch",
                    IrType::Named("AbsoluteEpochSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_relative_epoch" if call.args.len() == 3 => self.lower_simple_runtime_call(
                    "__ckb_since_epoch_relative",
                    "ckb_since_relative_epoch",
                    IrType::Named("RelativeEpochSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_absolute_block" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_block_absolute",
                    "ckb_since_absolute_block",
                    IrType::Named("AbsoluteBlockSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_relative_block" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_block_relative",
                    "ckb_since_relative_block",
                    IrType::Named("RelativeBlockSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_absolute_timestamp" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_timestamp_absolute",
                    "ckb_since_absolute_timestamp",
                    IrType::Named("AbsoluteTimestampSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_relative_timestamp" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_timestamp_relative",
                    "ckb_since_relative_timestamp",
                    IrType::Named("RelativeTimestampSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_decode" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_decode",
                    "ckb_since_decode",
                    IrType::Named("DecodedSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_from_raw_checked" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_from_raw_checked",
                    "ckb_since_from_raw_checked",
                    IrType::Named("DecodedSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_as_absolute_block" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_as_absolute_block",
                    "ckb_since_as_absolute_block",
                    IrType::Named("AbsoluteBlockSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_as_relative_block" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_as_relative_block",
                    "ckb_since_as_relative_block",
                    IrType::Named("RelativeBlockSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_as_absolute_epoch" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_as_absolute_epoch",
                    "ckb_since_as_absolute_epoch",
                    IrType::Named("AbsoluteEpochSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_as_relative_epoch" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_as_relative_epoch",
                    "ckb_since_as_relative_epoch",
                    IrType::Named("RelativeEpochSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_as_absolute_timestamp" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_as_absolute_timestamp",
                    "ckb_since_as_absolute_timestamp",
                    IrType::Named("AbsoluteTimestampSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_as_relative_timestamp" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_as_relative_timestamp",
                    "ckb_since_as_relative_timestamp",
                    IrType::Named("RelativeTimestampSince".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_is_relative" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_is_relative",
                    "ckb_since_is_relative",
                    IrType::Bool,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_is_disabled" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_is_disabled",
                    "ckb_since_is_disabled",
                    IrType::Bool,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_metric" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_metric",
                    "ckb_since_metric",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_value" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_since_value",
                    "ckb_since_value",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::epoch_duration" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_epoch_duration",
                    "ckb_epoch_duration",
                    IrType::Named("EpochDuration".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::epoch_add" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_epoch_add",
                    "ckb_epoch_add",
                    IrType::Named("EpochNumber".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::epoch_sub" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_epoch_sub",
                    "ckb_epoch_sub",
                    IrType::Named("EpochNumber".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::since_to_raw"
                | "ckb::epoch_number_to_u64"
                | "ckb::epoch_duration_to_u64"
                | "ckb::block_number_to_u64"
                | "ckb::epoch_length_to_u64"
                | "ckb::timestamp_millis_to_u64"
                    if call.args.len() == 1 =>
                {
                    let helper = match name.as_str() {
                        "ckb::since_to_raw" => "__ckb_since_to_raw",
                        "ckb::epoch_number_to_u64" => "__ckb_epoch_number_to_u64",
                        "ckb::epoch_duration_to_u64" => "__ckb_epoch_duration_to_u64",
                        "ckb::block_number_to_u64" => "__ckb_block_number_to_u64",
                        "ckb::epoch_length_to_u64" => "__ckb_epoch_length_to_u64",
                        _ => "__ckb_timestamp_millis_to_u64",
                    };
                    self.lower_simple_runtime_call(helper, "ckb_temporal_raw", IrType::U64, &call.args, current, blocks, vars)
                }
                "ckb::current_role" if call.args.is_empty() => {
                    let role = if self.lowering_lock_entry { 1 } else { 2 };
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::U64(role)), current: Some(current) })
                }
                "ckb::current_script_hash" if call.args.is_empty() => self.lower_simple_runtime_call(
                    "__ckb_current_script_hash",
                    "ckb_current_script_hash",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::script_hash" if call.args.len() == 1 => {
                    self.lower_transparent_view_wrapper(call, CKB_SCRIPT_HASH_TYPE, current, blocks, vars)
                }
                "ckb::cell_capacity" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_capacity",
                    "ckb_cell_capacity",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_occupied_capacity" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_occupied_capacity",
                    "ckb_cell_occupied_capacity",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_unoccupied_capacity" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_unoccupied_capacity",
                    "ckb_cell_unoccupied_capacity",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_output_index" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_output_index",
                    "ckb_cell_output_index",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::input_out_point_index" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_input_out_point_index",
                    "ckb_input_out_point_index",
                    IrType::U32,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::input_out_point_tx_hash_low" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_input_out_point_tx_hash_low",
                    "ckb_input_out_point_tx_hash_low",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::input_out_point_tx_hash" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_input_out_point_tx_hash",
                    "ckb_input_out_point_tx_hash",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::hash_data_packed" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_hash_data_packed",
                    "ckb_hash_data_packed",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::hash_sha256" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_hash_sha256",
                    "ckb_hash_sha256",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::hash_sha256d" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_hash_sha256d",
                    "ckb_hash_sha256d",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::hash_sha256_pair" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_hash_sha256_pair",
                    "ckb_hash_sha256_pair",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::hash_sha256d_pair" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_hash_sha256d_pair",
                    "ckb_hash_sha256d_pair",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::require_sha256d_merkle_root" if call.args.len() == 5 => {
                    self.lower_void_runtime_call("__ckb_require_sha256d_merkle_root", &call.args, current, blocks, vars)
                }
                "zk::require_valid" if call.args.len() == 8 => {
                    let mut active = current;
                    let mut lowered = Vec::new();
                    let Expr::String(policy) = &call.args[0] else { return None };
                    self.zk_origins.push(cellscript_artifact_checker::zk_profile::ZkSourceOrigin {
                        policy: policy.clone(),
                        start: call.span.start,
                        end: call.span.end,
                        line: call.span.line,
                        column: call.span.column,
                        fields: cellscript_artifact_checker::zk_profile::statement_origins(),
                    });
                    lowered.push(IrOperand::Const(IrConst::Array(policy.bytes().map(IrConst::U8).collect())));
                    for arg in &call.args[1..] {
                        let value = self.lower_expr(arg, active, blocks, vars);
                        active = value.current?;
                        lowered.push(value.operand);
                    }
                    if lowered[4..].iter().any(|value| !matches!(value, IrOperand::Const(IrConst::Hash(_)))) {
                        self.record_error("ZK handle, VK, domain and action identities must be compile-time Hash literals", call.span);
                    }
                    self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                        dest: None,
                        func: "__ckb_require_cell_dep_exact_verifier_handle".to_string(),
                        args: vec![lowered[2].clone(), lowered[3].clone(), lowered[4].clone()],
                    });
                    self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                        dest: None,
                        func: crate::zk_contract::REQUIRE_HELPER.to_string(),
                        args: vec![
                            lowered[0].clone(),
                            lowered[1].clone(),
                            lowered[2].clone(),
                            lowered[4].clone(),
                            lowered[5].clone(),
                            lowered[6].clone(),
                            lowered[7].clone(),
                        ],
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
                }
                "verifier::btc::bip340::require_signature" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__novaseal_bip340_require_signature", &call.args, current, blocks, vars)
                }
                "verifier::btc::bip340::require_signature_from_cell_dep" if call.args.len() == 4 => self.lower_void_runtime_call(
                    "__novaseal_bip340_require_signature_from_cell_dep",
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_lock_hash_low" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_lock_hash_low",
                    "ckb_cell_lock_hash_low",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_type_hash_low" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_type_hash_low",
                    "ckb_cell_type_hash_low",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_lock_hash" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_lock_hash",
                    "ckb_cell_lock_hash",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_type_hash" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_type_hash",
                    "ckb_cell_type_hash",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_data_hash_field" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_data_hash_field",
                    "cell_data_hash_field",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::raw_transaction_hash_without_cell_deps" if call.args.is_empty() => self.lower_simple_runtime_call(
                    "__ckb_raw_transaction_hash_without_cell_deps",
                    "raw_transaction_hash_without_cell_deps",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::transaction_hash" if call.args.is_empty() => self.lower_simple_runtime_call(
                    "__ckb_transaction_hash",
                    "transaction_hash",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::transaction_u32_le" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_transaction_u32_le",
                    "transaction_u32_le",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::bytes32" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_witness_bytes32",
                    "witness_bytes32",
                    IrType::Array(Box::new(IrType::U8), 32),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::transaction_blake2b_gather" if call.args.len() == 4 => self.lower_simple_runtime_call(
                    "__ckb_transaction_blake2b_gather",
                    "transaction_blake2b_gather",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::blake2b_select_chunks" if call.args.len() == 6 => self.lower_simple_runtime_call(
                    "__ckb_witness_blake2b_select_chunks",
                    "witness_blake2b_select_chunks",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_data_blake2b_span" | "witness::blake2b_span" if call.args.len() == 3 => {
                    let (helper, result_name) = if name == "ckb::cell_data_blake2b_span" {
                        ("__ckb_cell_data_blake2b_span", "cell_data_blake2b_span")
                    } else {
                        ("__ckb_witness_blake2b_span", "witness_blake2b_span")
                    };
                    self.lower_simple_runtime_call(helper, result_name, IrType::Hash, &call.args, current, blocks, vars)
                }
                "ckb::cell_data_bytes32" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_cell_data_bytes32",
                    "ckb_cell_data_bytes32",
                    IrType::Array(Box::new(IrType::U8), 32),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_lock_code_hash" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_lock_code_hash",
                    "ckb_cell_lock_code_hash",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_type_code_hash" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_type_code_hash",
                    "ckb_cell_type_code_hash",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_lock_hash_type" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_lock_hash_type",
                    "ckb_cell_lock_hash_type",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_type_hash_type" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_type_hash_type",
                    "ckb_cell_type_hash_type",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_lock_args_empty" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_lock_args_empty",
                    "ckb_cell_lock_args_empty",
                    IrType::Bool,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_type_args_empty" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_type_args_empty",
                    "ckb_cell_type_args_empty",
                    IrType::Bool,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_lock_args32" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_lock_args32",
                    "ckb_cell_lock_args32",
                    IrType::Array(Box::new(IrType::U8), 32),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_type_args32" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_type_args32",
                    "ckb_cell_type_args32",
                    IrType::Array(Box::new(IrType::U8), 32),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::require_cell_lock_hash" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_lock_hash", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_type_hash" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_type_hash", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_lock_exact_handle" if call.args.len() == 3 => self.lower_exact_script_handle_requirement(
                    "__ckb_require_cell_lock_exact_handle",
                    &call.args,
                    current,
                    blocks,
                    vars,
                    call.span,
                ),
                "ckb::require_cell_type_exact_handle" if call.args.len() == 3 => self.lower_exact_script_handle_requirement(
                    "__ckb_require_cell_type_exact_handle",
                    &call.args,
                    current,
                    blocks,
                    vars,
                    call.span,
                ),
                "ckb::require_cell_dep_exact_verifier_handle" if call.args.len() == 3 => self.lower_exact_script_handle_requirement(
                    "__ckb_require_cell_dep_exact_verifier_handle",
                    &call.args,
                    current,
                    blocks,
                    vars,
                    call.span,
                ),
                "ckb::require_cell_dep_open_script_handle" if call.args.len() == 3 => self.lower_open_handle_requirement(
                    "__ckb_require_cell_dep_open_script_handle",
                    &call.args,
                    current,
                    blocks,
                    vars,
                    call.span,
                ),
                "ckb::require_cell_dep_open_script_handle_from_args" if call.args.len() == 2 => self
                    .lower_open_handle_from_args_requirement(
                        "__ckb_require_cell_dep_open_script_handle_from_args",
                        &call.args,
                        current,
                        blocks,
                        vars,
                        call.span,
                    ),
                "ckb::require_instance_script_participant_from_args" if call.args.len() == 3 => self
                    .lower_open_handle_from_args_requirement(
                        "__ckb_require_instance_script_participant_from_args",
                        &call.args,
                        current,
                        blocks,
                        vars,
                        call.span,
                    ),
                "ckb::require_open_script_participant_from_args" if call.args.len() == 3 => self
                    .lower_open_handle_from_args_requirement(
                        "__ckb_require_open_script_participant_from_args",
                        &call.args,
                        current,
                        blocks,
                        vars,
                        call.span,
                    ),
                "ckb::require_cell_dep_open_verifier_handle_from_args" if call.args.len() == 2 => self
                    .lower_open_handle_from_args_requirement(
                        "__ckb_require_cell_dep_open_verifier_handle_from_args",
                        &call.args,
                        current,
                        blocks,
                        vars,
                        call.span,
                    ),
                "ckb::require_cell_dep_open_verifier_handle" if call.args.len() == 3 => self.lower_open_handle_requirement(
                    "__ckb_require_cell_dep_open_verifier_handle",
                    &call.args,
                    current,
                    blocks,
                    vars,
                    call.span,
                ),
                "ckb::require_cell_lock_deployment_line_handle" if call.args.len() == 5 => self
                    .lower_deployment_line_handle_requirement(
                        "__ckb_require_cell_lock_deployment_line_handle",
                        &call.args,
                        current,
                        blocks,
                        vars,
                        call.span,
                    ),
                "ckb::require_cell_type_deployment_line_handle" if call.args.len() == 5 => self
                    .lower_deployment_line_handle_requirement(
                        "__ckb_require_cell_type_deployment_line_handle",
                        &call.args,
                        current,
                        blocks,
                        vars,
                        call.span,
                    ),
                "ckb::require_cell_dep_deployment_line_verifier_handle" if call.args.len() == 4 => self
                    .lower_deployment_line_handle_requirement(
                        "__ckb_require_cell_dep_deployment_line_verifier_handle",
                        &call.args,
                        current,
                        blocks,
                        vars,
                        call.span,
                    ),
                "ckb::require_cell_data_hash" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_data_hash", &call.args, current, blocks, vars)
                }
                "ckb::require_bounded_cell_dep_data_hash" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_bounded_cell_dep_data_hash", &call.args, current, blocks, vars)
                }
                "ckb::require_current_script_args_empty" if call.args.is_empty() => {
                    self.lower_void_runtime_call("__ckb_require_current_script_args_empty", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_lock_args_empty" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__ckb_require_cell_lock_args_empty", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_type_args_empty" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__ckb_require_cell_type_args_empty", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_lock_args32" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_lock_args32", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_type_args32" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_type_args32", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_lock_args_prefix32" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_lock_args_prefix32", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_type_args_prefix32" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_type_args_prefix32", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_lock_args_suffix32" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_lock_args_suffix32", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_type_args_suffix32" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_cell_type_args_suffix32", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_lock_script_hash_type" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_cell_lock_script_hash_type", &call.args, current, blocks, vars)
                }
                "ckb::require_cell_type_script_hash_type" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_cell_type_script_hash_type", &call.args, current, blocks, vars)
                }
                "ckb::require_input_out_point_tx_hash" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_input_out_point_tx_hash", &call.args, current, blocks, vars)
                }
                "ckb::require_input_out_point" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_input_out_point", &call.args, current, blocks, vars)
                }
                "ckb::require_metapoint_relative" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_metapoint_relative", &call.args, current, blocks, vars)
                }
                "ckb::require_lock_type_metapoint_pairs" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_lock_type_metapoint_pairs", &call.args, current, blocks, vars)
                }
                "ckb::require_type_lock_metapoint_pairs" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_type_lock_metapoint_pairs", &call.args, current, blocks, vars)
                }
                "ckb::require_lock_type_metapoint_pairs_from_i32_data" if call.args.len() == 2 => self.lower_void_runtime_call(
                    "__ckb_require_lock_type_metapoint_pairs_from_i32_data",
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::require_type_lock_metapoint_pairs_from_i32_data" if call.args.len() == 2 => self.lower_void_runtime_call(
                    "__ckb_require_type_lock_metapoint_pairs_from_i32_data",
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::require_lock_type_metapoint_pairs_from_i32_data_filtered" if call.args.len() == 4 => self
                    .lower_void_runtime_call(
                        "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered",
                        &call.args,
                        current,
                        blocks,
                        vars,
                    ),
                "ckb::require_type_lock_metapoint_pairs_from_i32_data_filtered" if call.args.len() == 4 => self
                    .lower_void_runtime_call(
                        "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered",
                        &call.args,
                        current,
                        blocks,
                        vars,
                    ),
                "ckb::require_lock_match_master_out_point_pairs_from_data" if call.args.len() == 5 => self.lower_void_runtime_call(
                    "__ckb_require_lock_match_master_out_point_pairs_from_data",
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::exec_cell_dep_u8_args" if call.args.len() == 6 => {
                    self.lower_void_runtime_call("__ckb_exec_cell_dep_u8_args", &call.args, current, blocks, vars)
                }
                "ckb::trusted_exec_cell_dep_u8_args" if call.args.len() == 7 => self.lower_trusted_external_call(
                    "exec",
                    "__ckb_exec_cell_dep_u8_args",
                    &call.args,
                    current,
                    blocks,
                    vars,
                    call.span,
                ),
                "ckb::exec_cell_dep_hex4" if call.args.len() == 6 => {
                    self.lower_void_runtime_call("__ckb_exec_cell_dep_hex4", &call.args, current, blocks, vars)
                }
                "ckb::trusted_exec_cell_dep_hex4" if call.args.len() == 7 => {
                    self.lower_trusted_external_call("exec", "__ckb_exec_cell_dep_hex4", &call.args, current, blocks, vars, call.span)
                }
                "ckb::spawn_wait_cell_dep_hex4" if call.args.len() == 6 => {
                    self.lower_void_runtime_call("__ckb_spawn_wait_cell_dep_hex4", &call.args, current, blocks, vars)
                }
                "ckb::trusted_spawn_wait_cell_dep_hex4" if call.args.len() == 7 => self.lower_trusted_external_call(
                    "spawn-wait",
                    "__ckb_spawn_wait_cell_dep_hex4",
                    &call.args,
                    current,
                    blocks,
                    vars,
                    call.span,
                ),
                "ckb::cell_count" | "ckb::cell_has_type" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    if name == "ckb::cell_count" { "__ckb_cell_count" } else { "__ckb_cell_has_type" },
                    if name == "ckb::cell_count" { "ckb_cell_count" } else { "ckb_cell_has_type" },
                    if name == "ckb::cell_count" { IrType::U64 } else { IrType::Bool },
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_lock_size" | "ckb::cell_type_size" | "ckb::input_since_at" if call.args.len() == 1 => {
                    let (helper, result) = match name.as_str() {
                        "ckb::cell_lock_size" => ("__ckb_cell_lock_size", "cell_lock_size"),
                        "ckb::cell_type_size" => ("__ckb_cell_type_size", "cell_type_size"),
                        _ => ("__ckb_input_since_at", "input_since_at"),
                    };
                    self.lower_simple_runtime_call(helper, result, IrType::U64, &call.args, current, blocks, vars)
                }
                "ckb::cell_lock_u8" | "ckb::cell_type_u8" if call.args.len() == 2 => {
                    let (helper, result) = if name == "ckb::cell_lock_u8" {
                        ("__ckb_cell_lock_u8", "cell_lock_u8")
                    } else {
                        ("__ckb_cell_type_u8", "cell_type_u8")
                    };
                    self.lower_simple_runtime_call(helper, result, IrType::U64, &call.args, current, blocks, vars)
                }
                "ckb::cell_data_size" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_cell_data_size",
                    "ckb_cell_data_size",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_data_u8" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_cell_data_u8",
                    "ckb_cell_data_u8",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_data_u32_le" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_cell_data_u32_le",
                    "ckb_cell_data_u32_le",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::cell_data_u64_le" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_cell_data_u64_le",
                    "ckb_cell_data_u64_le",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::input" | "ckb::group_input" if call.args.len() == 1 && call.type_args.len() == 1 => {
                    let type_name = inline_ir_type_repr(&ast_type_to_ir(&call.type_args[0])).unwrap_or_else(|| "unknown".to_string());
                    self.lower_simple_runtime_call(
                        if name == "ckb::input" { "__ckb_source_input" } else { "__ckb_source_group_input" },
                        if name == "ckb::input" { "typed_input_view" } else { "typed_group_input_view" },
                        IrType::Named(format!("InputView<{type_name}>")),
                        &call.args,
                        current,
                        blocks,
                        vars,
                    )
                }
                "ckb::output" | "ckb::group_output" if call.args.len() == 1 && call.type_args.len() == 1 => {
                    let type_name = inline_ir_type_repr(&ast_type_to_ir(&call.type_args[0])).unwrap_or_else(|| "unknown".to_string());
                    self.lower_simple_runtime_call(
                        if name == "ckb::output" { "__ckb_source_output" } else { "__ckb_source_group_output" },
                        if name == "ckb::output" { "typed_output_view" } else { "typed_group_output_view" },
                        IrType::Named(format!("OutputView<{type_name}>")),
                        &call.args,
                        current,
                        blocks,
                        vars,
                    )
                }
                "ckb::cell_dep" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_cell_dep",
                    "typed_cell_dep_view",
                    IrType::Named("CellDepView".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::header_dep" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_header_dep",
                    "typed_header_dep_view",
                    IrType::Named("HeaderDepView".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::args" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_input",
                    "typed_witness_args_view",
                    IrType::Named("WitnessArgsView".to_string()),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::bounded_raw" | "witness::bounded_lock" | "witness::bounded_entry" | "witness::bounded_output_type"
                    if call.args.len() == 2 =>
                {
                    self.lower_bounded_witness_view(name.as_str(), call, current, blocks, vars)
                }
                "ckb::input_out_point" if call.args.len() == 1 => {
                    self.lower_transparent_view_wrapper(call, CKB_INPUT_OUT_POINT_REF_TYPE, current, blocks, vars)
                }
                "ckb::lock_script" if call.args.len() == 1 => {
                    self.lower_transparent_view_wrapper(call, CKB_LOCK_SCRIPT_REF_TYPE, current, blocks, vars)
                }
                "ckb::type_script" if call.args.len() == 1 => {
                    self.lower_transparent_view_wrapper(call, CKB_TYPE_SCRIPT_REF_TYPE, current, blocks, vars)
                }
                "source::input" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_input",
                    "source_input",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::output" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_output",
                    "source_output",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::cell_dep" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_cell_dep",
                    "source_cell_dep",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::header_dep" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_header_dep",
                    "source_header_dep",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::group_input" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_group_input",
                    "source_group_input",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "source::group_output" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_source_group_output",
                    "source_group_output",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::count" if call.args.is_empty() => self.lower_simple_runtime_call(
                    "__ckb_witness_count",
                    "witness_count",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::byte" | "witness::u32_le" | "witness::u64_le" if call.args.len() == 2 => {
                    self.lower_witness_scalar_read(name.as_str(), call, current, blocks, vars)
                }
                "witness::blake2b" if call.args.len() == 1 => self.lower_bounded_witness_blake2b(call, current, blocks, vars),
                "witness::lock_exact32" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_witness_lock_exact32",
                    "witness_lock_exact32",
                    IrType::Array(Box::new(IrType::U8), 32),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::input_type_exact32" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_witness_input_type_exact32",
                    "witness_input_type_exact32",
                    IrType::Array(Box::new(IrType::U8), 32),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::output_type_exact32" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_witness_output_type_exact32",
                    "witness_output_type_exact32",
                    IrType::Array(Box::new(IrType::U8), 32),
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "witness::size" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_witness_size",
                    "witness_size",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "ckb::require_witness_size_at_least" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__ckb_require_witness_size_at_least", &call.args, current, blocks, vars)
                }
                "dao::accumulated_rate" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__dao_accumulated_rate",
                    "dao_accumulated_rate",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "dao::input_accumulated_rate" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__dao_input_accumulated_rate",
                    "dao_input_accumulated_rate",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "dao::is_deposit_data" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__dao_is_deposit_data",
                    "dao_is_deposit_data",
                    IrType::Bool,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "dao::is_withdrawal_request_data" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__dao_is_withdrawal_request_data",
                    "dao_is_withdrawal_request_data",
                    IrType::Bool,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "dao::has_dao_type" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__dao_has_dao_type",
                    "dao_has_dao_type",
                    IrType::Bool,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "dao::require_header_dep_for_input" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__dao_require_header_dep_for_input", &call.args, current, blocks, vars)
                }
                "dao::require_input_since_at_least" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__dao_require_input_since_at_least", &call.args, current, blocks, vars)
                }
                "dao::require_input_relative_epoch_since_at_least" if call.args.len() == 4 => self.lower_void_runtime_call(
                    "__dao_require_input_relative_epoch_since_at_least",
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "xudt::amount_low" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__xudt_amount_low",
                    "xudt_amount_low",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "xudt::amount_high" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__xudt_amount_high",
                    "xudt_amount_high",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "xudt::owner_mode_input_type_hash" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__xudt_owner_mode_input_type_hash",
                    "xudt_owner_mode_input_type_hash",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "xudt::require_owner_mode_input_type" if call.args.len() == 2 => {
                    self.lower_void_runtime_call("__xudt_require_owner_mode_input_type", &call.args, current, blocks, vars)
                }
                "xudt::require_owner_mode_type_args" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__xudt_require_owner_mode_type_args", &call.args, current, blocks, vars)
                }
                "xudt::require_owner_mode_type_args_current_script" if call.args.len() == 2 => self.lower_void_runtime_call(
                    "__xudt_require_owner_mode_type_args_current_script",
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "xudt::require_group_amount_conserved" if call.args.is_empty() => {
                    self.lower_void_runtime_call("__xudt_require_group_amount_conserved", &call.args, current, blocks, vars)
                }
                "xudt::require_group_amount_minted" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__xudt_require_group_amount_minted", &call.args, current, blocks, vars)
                }
                "xudt::require_group_amount_burned" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__xudt_require_group_amount_burned", &call.args, current, blocks, vars)
                }
                "c256::require_product_lte" if call.args.len() == 4 => {
                    self.lower_void_runtime_call("__c256_require_u128_product_lte", &call.args, current, blocks, vars)
                }
                "c256::require_product_eq" if call.args.len() == 4 => {
                    self.lower_void_runtime_call("__c256_require_u128_product_eq", &call.args, current, blocks, vars)
                }
                "c256::require_sum2_products_lte" if call.args.len() == 8 => {
                    self.lower_void_runtime_call("__c256_require_u128_sum2_products_lte", &call.args, current, blocks, vars)
                }
                "c256::require_sum2_products_eq" if call.args.len() == 8 => {
                    self.lower_void_runtime_call("__c256_require_u128_sum2_products_eq", &call.args, current, blocks, vars)
                }
                "spawn" => {
                    self.errors.borrow_mut().push(crate::error::bare_spawn_error(call.span));
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) })
                }
                "pipe" if call.args.is_empty() => {
                    let dest = self.new_var("pipe_pair", IrType::Tuple(vec![IrType::U64, IrType::U64]));
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_pipe".to_string(),
                        args: Vec::new(),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "wait" if call.args.is_empty() => {
                    self.lower_simple_runtime_call("__ckb_wait", "wait_result", IrType::U64, &call.args, current, blocks, vars)
                }
                "process_id" if call.args.is_empty() => {
                    self.lower_simple_runtime_call("__ckb_process_id", "process_id", IrType::U64, &call.args, current, blocks, vars)
                }
                "pipe_write" if call.args.len() == 2 => self.lower_simple_runtime_call(
                    "__ckb_pipe_write",
                    "pipe_write_result",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "pipe_read" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_pipe_read",
                    "pipe_read_result",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "inherited_fd" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_inherited_fd",
                    "inherited_fd",
                    IrType::U64,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "close" if call.args.len() == 1 => {
                    self.lower_simple_runtime_call("__ckb_close", "close_result", IrType::U64, &call.args, current, blocks, vars)
                }
                "require_maturity" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__ckb_require_maturity", &call.args, current, blocks, vars)
                }
                "require_time" if call.args.len() == 1 => {
                    self.lower_void_runtime_call("__ckb_require_time", &call.args, current, blocks, vars)
                }
                "require_epoch_after" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_epoch_after", &call.args, current, blocks, vars)
                }
                "require_epoch_relative" if call.args.len() == 3 => {
                    self.lower_void_runtime_call("__ckb_require_epoch_relative", &call.args, current, blocks, vars)
                }
                "occupied_capacity" if call.args.len() == 1 => {
                    let dest = self.new_var("occupied_capacity", IrType::U64);
                    let tag = match &call.args[0] {
                        Expr::String(value) => stable_u64_tag(value),
                        _ => 0,
                    };
                    self.block_mut(blocks, current).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_occupied_capacity".to_string(),
                        args: vec![IrOperand::Const(IrConst::U64(tag))],
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(current) })
                }
                "hash_chain" if call.args.len() == 1 => {
                    self.lower_simple_runtime_call("__ckb_hash_chain", "hash_chain", IrType::Hash, &call.args, current, blocks, vars)
                }
                "hash_pair" if call.args.len() == 2 => {
                    self.lower_simple_runtime_call("__ckb_hash_pair", "hash_pair", IrType::Hash, &call.args, current, blocks, vars)
                }
                "hash_blake2b" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_hash_blake2b",
                    "hash_blake2b",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                "hash_blake2b_packed" if call.args.len() == 1 => self.lower_simple_runtime_call(
                    "__ckb_hash_blake2b_packed",
                    "hash_blake2b_packed",
                    IrType::Hash,
                    &call.args,
                    current,
                    blocks,
                    vars,
                ),
                name if name == crate::commitment_contract::COMMIT_FUNCTION && call.args.len() == 1 => {
                    let lowered = self.lower_expr(&call.args[0], current, blocks, vars);
                    let active = lowered.current?;
                    let inner = ir_type_display(&self.operand_type(&lowered.operand));
                    let dest = self.new_var("commitment", IrType::Named(crate::commitment_contract::commitment_type(&inner)));
                    self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_hash_blake2b_packed".to_string(),
                        args: vec![lowered.operand],
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                name if name == crate::commitment_contract::OPEN_FUNCTION && call.args.len() == 2 => {
                    let expected = self.lower_expr(&call.args[0], current, blocks, vars);
                    let active = expected.current?;
                    let opening = self.lower_expr(&call.args[1], active, blocks, vars);
                    let active = opening.current?;
                    let inner = match self.operand_type(&opening.operand) {
                        IrType::Named(name) => crate::commitment_contract::opening_inner_type(&name)
                            .map(parse_inline_ir_type_repr)
                            .unwrap_or(IrType::Unit),
                        _ => IrType::Unit,
                    };
                    let dest = self.new_var("opened_value", inner);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                        dest: Some(dest.clone()),
                        func: "__ckb_commitment_open".to_string(),
                        args: vec![expected.operand, opening.operand],
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "Vec::new" if call.args.is_empty() => Some(self.lower_vec_constructor_expr(call, "Vec", current, blocks, vars)),
                "Vec::with_capacity" if call.args.len() == 1 => {
                    Some(self.lower_vec_constructor_expr(call, "Vec", current, blocks, vars))
                }
                _ => None,
            },
            Expr::FieldAccess(field) => match field.field.as_str() {
                "len" if call.args.is_empty() => {
                    let lowered = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered.current?;
                    if let IrOperand::Var(var) = &lowered.operand
                        && let Some(elements) = self.aggregate_elements.get(&var.id)
                    {
                        return Some(LoweredExpr {
                            operand: IrOperand::Const(IrConst::U64(elements.len() as u64)),
                            current: Some(active),
                        });
                    }
                    let dest = self.new_var("len_tmp", IrType::U64);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::Length { dest: dest.clone(), operand: lowered.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "is_empty" if call.args.is_empty() => {
                    let lowered = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered.current?;
                    if let IrOperand::Var(var) = &lowered.operand
                        && let Some(elements) = self.aggregate_elements.get(&var.id)
                    {
                        return Some(LoweredExpr {
                            operand: IrOperand::Const(IrConst::Bool(elements.is_empty())),
                            current: Some(active),
                        });
                    }
                    let len_dest = self.new_var("is_empty_len_tmp", IrType::U64);
                    let dest = self.new_var("is_empty_tmp", IrType::Bool);
                    let block = self.block_mut(blocks, active);
                    block.instructions.push(IrInstruction::Length { dest: len_dest.clone(), operand: lowered.operand });
                    block.instructions.push(IrInstruction::Binary {
                        dest: dest.clone(),
                        op: BinaryOp::Eq,
                        left: IrOperand::Var(len_dest),
                        right: IrOperand::Const(IrConst::U64(0)),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "capacity" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let dest = self.new_var("capacity_tmp", IrType::U64);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionCapacity { dest: dest.clone(), collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "first" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let dest = self.new_var("first_tmp", item_ty);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::Index {
                        dest: dest.clone(),
                        arr: lowered_collection.operand,
                        idx: IrOperand::Const(IrConst::U64(0)),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "last" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let len_dest = self.new_var("last_len_tmp", IrType::U64);
                    let index_dest = self.new_var("last_index_tmp", IrType::U64);
                    let dest = self.new_var("last_tmp", item_ty);
                    let block = self.block_mut(blocks, active);
                    block
                        .instructions
                        .push(IrInstruction::Length { dest: len_dest.clone(), operand: lowered_collection.operand.clone() });
                    block.instructions.push(IrInstruction::Binary {
                        dest: index_dest.clone(),
                        op: BinaryOp::Sub,
                        left: IrOperand::Var(len_dest),
                        right: IrOperand::Const(IrConst::U64(1)),
                    });
                    block.instructions.push(IrInstruction::Index {
                        dest: dest.clone(),
                        arr: lowered_collection.operand,
                        idx: IrOperand::Var(index_dest),
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "type_hash" if call.args.is_empty() => {
                    let lowered = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered.current?;
                    let dest = self.new_var("type_hash_tmp", IrType::Hash);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::TypeHash { dest: dest.clone(), operand: lowered.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "push" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_value = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                        && matches!(&collection_var.ty, IrType::Named(name) if name == "Vec")
                        && let (Some(item_ty), Some(receiver_var)) =
                            (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                        && receiver_var.id == collection_var.id
                    {
                        receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                    }
                    let block = self.block_mut(blocks, active);
                    block.instructions.push(IrInstruction::CollectionPush {
                        collection: collection_operand.clone(),
                        value: lowered_value.operand.clone(),
                    });
                    if let IrOperand::Var(collection_var) = &collection_operand
                        && let Some((root_id, field_name)) = self.schema_field_roots.get(&collection_var.id).cloned()
                    {
                        self.mutated_fields.entry(root_id).or_default().insert(field_name.clone());
                        self.mutated_field_transitions.entry(root_id).or_default().insert(
                            field_name.clone(),
                            MutateFieldTransition {
                                field: field_name,
                                op: MutateTransitionOp::Append,
                                operand: lowered_value.operand,
                            },
                        );
                    }
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "clear" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionClear { collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "reverse" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionReverse { collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "truncate" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_len = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_len.current?;
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionTruncate { collection: lowered_collection.operand, len: lowered_len.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "swap" if call.args.len() == 2 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_left = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_left.current?;
                    let lowered_right = self.lower_expr(&call.args[1], active, blocks, vars);
                    let active = lowered_right.current?;
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionSwap {
                        collection: lowered_collection.operand,
                        left: lowered_left.operand,
                        right: lowered_right.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "contains" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_value = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                        && matches!(&collection_var.ty, IrType::Named(name) if name == "Vec")
                        && let (Some(item_ty), Some(receiver_var)) =
                            (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                        && receiver_var.id == collection_var.id
                    {
                        receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                    }
                    let dest = self.new_var("contains_tmp", IrType::Bool);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionContains {
                        dest: dest.clone(),
                        collection: collection_operand,
                        value: lowered_value.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "remove" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_index = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_index.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let dest = self.new_var("remove_tmp", item_ty);
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionRemove {
                        dest: dest.clone(),
                        collection: lowered_collection.operand,
                        index: lowered_index.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "pop" if call.args.is_empty() => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let item_ty = collection_item_ir_type(&self.operand_type(&lowered_collection.operand)).unwrap_or(IrType::U64);
                    let dest = self.new_var("pop_tmp", item_ty);
                    self.block_mut(blocks, active)
                        .instructions
                        .push(IrInstruction::CollectionPop { dest: dest.clone(), collection: lowered_collection.operand });
                    Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
                }
                "insert" if call.args.len() == 2 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_index = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_index.current?;
                    let lowered_value = self.lower_expr(&call.args[1], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                        && matches!(&collection_var.ty, IrType::Named(name) if name == "Vec")
                        && let (Some(item_ty), Some(receiver_var)) =
                            (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                        && receiver_var.id == collection_var.id
                    {
                        receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                    }
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionInsert {
                        collection: collection_operand,
                        index: lowered_index.operand,
                        value: lowered_value.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "set" if call.args.len() == 2 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_index = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_index.current?;
                    let lowered_value = self.lower_expr(&call.args[1], active, blocks, vars);
                    let active = lowered_value.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                        && matches!(&collection_var.ty, IrType::Named(name) if name == "Vec")
                        && let (Some(item_ty), Some(receiver_var)) =
                            (inline_ir_type_repr(&self.operand_type(&lowered_value.operand)), vars.get_mut(receiver_name))
                        && receiver_var.id == collection_var.id
                    {
                        receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                    }
                    self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionSet {
                        collection: collection_operand,
                        index: lowered_index.operand,
                        value: lowered_value.operand,
                    });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                "extend_from_slice" if call.args.len() == 1 => {
                    let lowered_collection = self.lower_expr(&field.expr, current, blocks, vars);
                    let active = lowered_collection.current?;
                    let lowered_slice = self.lower_expr(&call.args[0], active, blocks, vars);
                    let active = lowered_slice.current?;
                    let collection_operand = lowered_collection.operand;
                    if let (Expr::Identifier(receiver_name), IrOperand::Var(collection_var)) =
                        (field.expr.as_ref(), &collection_operand)
                        && matches!(&collection_var.ty, IrType::Named(name) if name == "Vec")
                        && let (IrType::Array(inner, _), Some(receiver_var)) =
                            (self.operand_type(&lowered_slice.operand), vars.get_mut(receiver_name))
                        && receiver_var.id == collection_var.id
                        && let Some(item_ty) = inline_ir_type_repr(inner.as_ref())
                    {
                        receiver_var.ty = IrType::Named(format!("Vec<{}>", item_ty));
                    }
                    let block = self.block_mut(blocks, active);
                    block
                        .instructions
                        .push(IrInstruction::CollectionExtend { collection: collection_operand, slice: lowered_slice.operand });
                    Some(LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) })
                }
                _ => None,
            },
            _ => None,
        }
    }
}
