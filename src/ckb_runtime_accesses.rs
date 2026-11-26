use super::capability_metadata::RuntimeViewProvenance;
use super::metadata_layout::operand_fixed_byte_width;
use super::runtime_availability::metadata_ir_type_fixed_width;
use super::runtime_metadata::{runtime_source_for_constructor, runtime_view_provenance_by_var};
use super::{
    aggregate_lowering, cell_data_view, cell_selection, ir, ir_const_u64, runtime_access_metadata, runtime_range_at,
    runtime_range_bounded_value, runtime_range_fixed, runtime_range_not_applicable, runtime_range_whole_value,
    runtime_scalar_bounded_scan, runtime_scalar_dynamic, runtime_scalar_from_operand, runtime_scalar_not_applicable,
    runtime_scalar_static, static_runtime_access, CkbRuntimeAccessMetadata, CkbRuntimeRangeProvenanceMetadata, MetadataTypeLayouts,
    CKB_SCRIPT_HASH_MAX_ARGS_BYTES,
};
use std::collections::HashMap;

fn runtime_range_for_call(func: &str, args: &[ir::IrOperand]) -> CkbRuntimeRangeProvenanceMetadata {
    let fixed_width = match func {
        "__ckb_header_dep_epoch_number"
        | "__ckb_header_dep_epoch_start_block_number"
        | "__ckb_header_dep_epoch_length"
        | "__ckb_cell_capacity"
        | "__ckb_cell_data_size"
        | "__ckb_input_since_at"
        | "__ckb_witness_u64_le"
        | "__ckb_cell_data_u64_le" => Some(8),
        "__ckb_input_out_point_index" | "__ckb_witness_u32_le" | "__ckb_cell_data_u32_le" => Some(4),
        "__ckb_cell_lock_hash_type"
        | "__ckb_cell_type_hash_type"
        | "__ckb_witness_u8"
        | "__ckb_cell_data_u8"
        | "__ckb_cell_lock_u8"
        | "__ckb_cell_type_u8" => Some(1),
        "__ckb_cell_data_hash_field"
        | "__ckb_transaction_hash"
        | "__ckb_cell_lock_hash"
        | "__ckb_cell_type_hash"
        | "__ckb_input_out_point_tx_hash"
        | "__ckb_witness_lock_exact32"
        | "__ckb_witness_input_type_exact32"
        | "__ckb_witness_output_type_exact32" => Some(32),
        "__ckb_require_cell_lock_exact_handle"
        | "__ckb_require_cell_type_exact_handle"
        | "__ckb_require_cell_dep_exact_verifier_handle"
        | "__ckb_require_cell_lock_deployment_line_handle"
        | "__ckb_require_cell_type_deployment_line_handle"
        | "__ckb_require_cell_dep_deployment_line_verifier_handle" => Some(32),
        // These helpers load and size-check the complete packed Header before
        // reading the typed field at its canonical RawHeader offset.
        "__ckb_header_dep_block_number" | "__ckb_header_dep_timestamp_millis" => Some(208),
        _ => None,
    };
    if let Some(width) = fixed_width {
        return if matches!(
            func,
            "__ckb_witness_u8"
                | "__ckb_witness_u32_le"
                | "__ckb_witness_u64_le"
                | "__ckb_cell_data_u8"
                | "__ckb_cell_data_u32_le"
                | "__ckb_cell_data_u64_le"
                | "__ckb_cell_lock_u8"
                | "__ckb_cell_type_u8"
        ) {
            runtime_range_at(args.get(1), runtime_scalar_static(width, Some(width)))
        } else {
            runtime_range_fixed(width)
        };
    }
    match func {
        "__ckb_witness_bytes32" | "__ckb_cell_data_bytes32" => runtime_range_at(args.get(1), runtime_scalar_static(32, Some(32))),
        "__ckb_cell_data_blake2b_span" | "__ckb_witness_blake2b_span" => {
            runtime_range_at(args.get(1), runtime_scalar_from_operand(args.get(2), None))
        }
        "__ckb_cell_occupied_capacity"
        | "__ckb_cell_unoccupied_capacity"
        | "__ckb_cell_lock_code_hash"
        | "__ckb_cell_type_code_hash"
        | "__ckb_cell_lock_args_empty"
        | "__ckb_cell_type_args_empty"
        | "__ckb_cell_lock_args32"
        | "__ckb_cell_type_args32"
        | "__ckb_cell_lock_size"
        | "__ckb_cell_type_size"
        | "__ckb_witness_size"
        | "__ckb_witness_count"
        | "__ckb_require_witness_size_at_least" => runtime_range_whole_value(),
        _ => runtime_range_not_applicable(),
    }
}

fn runtime_access_for_call(
    target: &ir::IrCallTarget,
    args: &[ir::IrOperand],
    view_provenance: &HashMap<usize, RuntimeViewProvenance>,
) -> Option<CkbRuntimeAccessMetadata> {
    let func = target.runtime_helper()?.symbol();
    if func == "__ckb_script_hash" {
        let args_width = args.get(2).and_then(operand_fixed_byte_width)?;
        if args_width > CKB_SCRIPT_HASH_MAX_ARGS_BYTES {
            return None;
        }
        return Some(runtime_access_metadata(
            "script-hash-v1",
            "CKB_BLAKE2B",
            "Script",
            "script::hash",
            "Script",
            "constructed-script",
            None,
            runtime_scalar_not_applicable(),
            runtime_range_fixed((53 + args_width) as u64),
        ));
    }
    if func == "__ckb_sighash_all_zero_lock" {
        let max_group_inputs = args.first().and_then(|operand| match operand {
            ir::IrOperand::Const(value) => ir_const_u64(value),
            ir::IrOperand::Var(_) => None,
        })?;
        let max_witness_bytes = args.get(3).and_then(|operand| match operand {
            ir::IrOperand::Const(value) => ir_const_u64(value),
            ir::IrOperand::Var(_) => None,
        })?;
        return Some(runtime_access_metadata(
            "sighash-all-zero-lock-v1",
            "CKB_SIGHASH_ALL_ZERO_LOCK_V1",
            "GroupInput",
            "env::sighash_all_zero_lock",
            "GroupInput",
            "bounded-scan",
            None,
            runtime_scalar_bounded_scan(max_group_inputs.saturating_sub(1)),
            CkbRuntimeRangeProvenanceMetadata {
                kind: "bounded-range".to_string(),
                offset: runtime_scalar_static(0, Some(max_witness_bytes)),
                length: runtime_scalar_dynamic("sighash_all_zero_lock.witness_size", Some(max_witness_bytes)),
            },
        ));
    }
    if matches!(
        func,
        "__ckb_witness_bounded_size"
            | "__ckb_witness_bounded_u8"
            | "__ckb_witness_bounded_u32_le"
            | "__ckb_witness_bounded_u64_le"
            | "__ckb_witness_bounded_blake2b"
    ) {
        let owner = args.get(1).and_then(|operand| match operand {
            ir::IrOperand::Const(value) => ir_const_u64(value),
            ir::IrOperand::Var(_) => None,
        })?;
        let owner = match owner {
            ir::CKB_WITNESS_OWNER_RAW => "raw",
            ir::CKB_WITNESS_OWNER_LOCK => "lock",
            ir::CKB_WITNESS_OWNER_ENTRY => "entry",
            ir::CKB_WITNESS_OWNER_OUTPUT_TYPE => "output_type",
            _ => return None,
        };
        let maximum = args.get(2).and_then(|operand| match operand {
            ir::IrOperand::Const(value) => ir_const_u64(value),
            ir::IrOperand::Var(_) => None,
        })?;
        let operation_suffix = match func {
            "__ckb_witness_bounded_size" => "size",
            "__ckb_witness_bounded_u8" => "u8",
            "__ckb_witness_bounded_u32_le" => "u32-le",
            "__ckb_witness_bounded_u64_le" => "u64-le",
            _ => "blake2b",
        };
        let inherited = args.first().and_then(|operand| match operand {
            ir::IrOperand::Var(var) => view_provenance.get(&var.id).map(|provenance| (var, provenance)),
            ir::IrOperand::Const(_) => None,
        });
        let (resolved_source, source_origin, source_binding, index) = if let Some((var, provenance)) = inherited {
            (provenance.resolved_source.clone(), "inherited-source-view", Some(var.name.clone()), provenance.index.clone())
        } else {
            ("Witness".to_string(), "implicit-lowering", None, runtime_scalar_not_applicable())
        };
        let range = match func {
            "__ckb_witness_bounded_size" | "__ckb_witness_bounded_blake2b" => {
                runtime_range_bounded_value(format!("bounded_{owner}.size"), maximum)
            }
            "__ckb_witness_bounded_u8" => CkbRuntimeRangeProvenanceMetadata {
                kind: "bounded-range".to_string(),
                offset: runtime_scalar_from_operand(args.get(3), Some(maximum)),
                length: runtime_scalar_static(1, Some(1)),
            },
            "__ckb_witness_bounded_u32_le" => CkbRuntimeRangeProvenanceMetadata {
                kind: "bounded-range".to_string(),
                offset: runtime_scalar_from_operand(args.get(3), Some(maximum)),
                length: runtime_scalar_static(4, Some(4)),
            },
            _ => CkbRuntimeRangeProvenanceMetadata {
                kind: "bounded-range".to_string(),
                offset: runtime_scalar_from_operand(args.get(3), Some(maximum)),
                length: runtime_scalar_static(8, Some(8)),
            },
        };
        return Some(runtime_access_metadata(
            format!("witness-bounded-{owner}-{operation_suffix}"),
            "LOAD_WITNESS",
            "Witness",
            format!("witness::bounded_{owner}"),
            resolved_source,
            source_origin,
            source_binding,
            index,
            range,
        ));
    }
    let (operation, syscall, source, binding) = ckb_v014_runtime_access(func)?;
    if let Some(resolved_source) = runtime_source_for_constructor(target) {
        return Some(runtime_access_metadata(
            operation,
            syscall,
            source,
            binding,
            resolved_source,
            "explicit-source-view",
            None,
            runtime_scalar_from_operand(args.first(), Some(u64::from(u32::MAX))),
            runtime_range_not_applicable(),
        ));
    }
    if func == "__ckb_require_bounded_cell_dep_data_hash" {
        let maximum_count = args.first().and_then(|operand| match operand {
            ir::IrOperand::Const(value) => ir_const_u64(value),
            ir::IrOperand::Var(_) => None,
        });
        return Some(runtime_access_metadata(
            operation,
            syscall,
            source,
            binding,
            "CellDep",
            "bounded-scan",
            None,
            runtime_scalar_bounded_scan(maximum_count.unwrap_or(0).saturating_sub(1)),
            runtime_range_fixed(32),
        ));
    }
    if matches!(func, "__ckb_exec_cell_dep_u8_args" | "__ckb_exec_cell_dep_hex4" | "__ckb_spawn_wait_cell_dep_hex4") {
        return Some(runtime_access_metadata(
            operation,
            syscall,
            source,
            binding,
            "CellDep",
            "external-adapter",
            None,
            runtime_scalar_from_operand(args.first(), None),
            runtime_range_not_applicable(),
        ));
    }
    if func == "__dao_accumulated_rate" {
        return Some(runtime_access_metadata(
            operation,
            syscall,
            source,
            binding,
            "HeaderDep",
            "implicit-lowering",
            None,
            runtime_scalar_from_operand(args.first(), None),
            runtime_range_fixed(32),
        ));
    }
    let inherited = args.first().and_then(|operand| match operand {
        ir::IrOperand::Var(var) => view_provenance.get(&var.id).map(|provenance| (var, provenance)),
        ir::IrOperand::Const(_) => None,
    });
    let (resolved_source, source_origin, source_binding, index) = if let Some((var, provenance)) = inherited {
        (provenance.resolved_source.clone(), "inherited-source-view", Some(var.name.clone()), provenance.index.clone())
    } else {
        (source.to_string(), "implicit-lowering", None, runtime_scalar_not_applicable())
    };
    Some(runtime_access_metadata(
        operation,
        syscall,
        source,
        binding,
        resolved_source,
        source_origin,
        source_binding,
        index,
        runtime_range_for_call(func, args),
    ))
}

pub(super) fn body_ckb_runtime_accesses(
    _name: &str,
    body: &ir::IrBody,
    _cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    type_layouts: &MetadataTypeLayouts,
) -> Vec<CkbRuntimeAccessMetadata> {
    let mut accesses = Vec::new();
    let view_provenance = runtime_view_provenance_by_var(body);
    for pattern in &body.consume_set {
        accesses.extend(resolved_cell_access(body, ir::IrCellBindingRole::Input, &pattern.binding, &pattern.operation));
    }
    for pattern in &body.read_refs {
        accesses.extend(resolved_cell_access(body, ir::IrCellBindingRole::ReadOnly, &pattern.binding, "read_ref"));
    }
    for pattern in &body.create_set {
        accesses.extend(resolved_cell_access(body, ir::IrCellBindingRole::Output, &pattern.binding, &pattern.operation));
    }
    accesses.extend(body_unique_identity_runtime_accesses(body));
    for pattern in &body.mutate_set {
        accesses.extend(resolved_cell_access(body, ir::IrCellBindingRole::Input, &pattern.binding, "mutate-input"));
        accesses.extend(resolved_cell_access(body, ir::IrCellBindingRole::Output, &pattern.binding, "mutate-output"));
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            if let ir::IrInstruction::VerifyExactCellRoles { contract, .. } = instruction {
                accesses.extend(cell_selection::runtime_accesses(contract, type_layouts));
            }
            if let ir::IrInstruction::FieldAccess { obj: ir::IrOperand::Var(view), .. } = instruction
                && let Some(schema) = cell_data_view::ir_schema(&view.ty)
            {
                let width = metadata_ir_type_fixed_width(&ir::IrType::Named(schema.to_string()), type_layouts);
                let provenance = view_provenance.get(&view.id);
                let source = provenance.map(|value| value.resolved_source.as_str()).unwrap_or("SourceView");
                accesses.push(runtime_access_metadata(
                    "typed-cell-data",
                    "LOAD_CELL_DATA",
                    source,
                    view.name.clone(),
                    source,
                    "inherited-source-view",
                    Some(view.name.clone()),
                    provenance.map(|value| value.index.clone()).unwrap_or_else(|| {
                        runtime_scalar_from_operand(Some(&ir::IrOperand::Var(view.clone())), Some(u64::from(u32::MAX)))
                    }),
                    width.map_or_else(runtime_range_whole_value, |width| runtime_range_fixed(width as u64)),
                ));
            }
            if let ir::IrInstruction::BoundedCellLoad { index, max_elements, element_type, element_width, .. } = instruction {
                for (operation, syscall) in [
                    ("bounded-type-group-input-data", "LOAD_CELL_DATA"),
                    ("bounded-type-group-input-type-hash", "LOAD_CELL_BY_FIELD"),
                    ("bounded-type-group-input-lock-hash", "LOAD_CELL_BY_FIELD"),
                ] {
                    let width = if syscall == "LOAD_CELL_DATA" { *element_width as u64 } else { 32 };
                    accesses.push(runtime_access_metadata(
                        operation,
                        syscall,
                        "GroupInput",
                        element_type.clone(),
                        "GroupInput",
                        "bounded-scan",
                        None,
                        runtime_scalar_from_operand(Some(index), Some(*max_elements as u64)),
                        runtime_range_fixed(width),
                    ));
                }
                accesses.push(runtime_access_metadata(
                    "bounded-current-script-hash",
                    "LOAD_SCRIPT_HASH",
                    "CurrentScript",
                    element_type.clone(),
                    "CurrentScript",
                    "bounded-scan",
                    None,
                    runtime_scalar_not_applicable(),
                    runtime_range_fixed(32),
                ));
            }
            if let ir::IrInstruction::BoundedPlanLoad { index, max_elements, element_type, element_width, .. } = instruction {
                accesses.push(runtime_access_metadata(
                    "bounded-output-plan-decode",
                    "LOAD_WITNESS",
                    "Witness",
                    element_type.clone(),
                    "Witness",
                    "bounded-scan",
                    None,
                    runtime_scalar_static(0, Some(0)),
                    CkbRuntimeRangeProvenanceMetadata {
                        kind: "bounded-range".to_string(),
                        offset: runtime_scalar_from_operand(Some(index), Some(max_elements.saturating_sub(1) as u64)),
                        length: runtime_scalar_static(*element_width as u64, Some(*element_width as u64)),
                    },
                ));
            }
            if let ir::IrInstruction::BoundedOutputVerify { index, pattern, .. } = instruction {
                for (operation, syscall) in [
                    ("bounded-group-output-data", "LOAD_CELL_DATA"),
                    ("bounded-group-output-lock-hash", "LOAD_CELL_BY_FIELD"),
                    ("bounded-group-output-capacity", "LOAD_CELL_BY_FIELD"),
                    ("bounded-current-script-hash", "LOAD_SCRIPT_HASH"),
                ] {
                    let source = if syscall == "LOAD_SCRIPT_HASH" { "CurrentScript" } else { "GroupOutput" };
                    let source_index = if source == "CurrentScript" {
                        runtime_scalar_not_applicable()
                    } else {
                        runtime_scalar_from_operand(Some(index), None)
                    };
                    let range = match operation {
                        "bounded-group-output-lock-hash" | "bounded-current-script-hash" => runtime_range_fixed(32),
                        "bounded-group-output-capacity" => runtime_range_fixed(8),
                        _ => runtime_range_whole_value(),
                    };
                    accesses.push(runtime_access_metadata(
                        operation,
                        syscall,
                        source,
                        pattern.ty.clone(),
                        source,
                        "bounded-scan",
                        None,
                        source_index,
                        range,
                    ));
                }
            }
            if let ir::IrInstruction::BoundedOutputEnd { index } = instruction {
                accesses.push(runtime_access_metadata(
                    "bounded-group-output-count-end",
                    "LOAD_CELL_BY_FIELD",
                    "GroupOutput",
                    "create_each",
                    "GroupOutput",
                    "bounded-scan",
                    None,
                    runtime_scalar_from_operand(Some(index), None),
                    runtime_range_not_applicable(),
                ));
            }
            if matches!(instruction, ir::IrInstruction::Call { func, .. } if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputSince))
            {
                accesses.push(runtime_access_metadata(
                    "input-since",
                    "LOAD_INPUT_BY_FIELD",
                    "GroupInput",
                    "ckb::input_since",
                    "GroupInput",
                    "implicit-lowering",
                    None,
                    runtime_scalar_static(0, Some(0)),
                    runtime_range_fixed(8),
                ));
            }
            if let ir::IrInstruction::Call { func, args, .. } = instruction {
                if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSpawnWaitCellDepHex4)
                {
                    accesses.push(runtime_access_metadata(
                        "spawn-child-exit-checked",
                        "WAIT",
                        "Process",
                        "ckb::spawn_wait_cell_dep_hex4",
                        "Process",
                        "external-adapter",
                        None,
                        runtime_scalar_not_applicable(),
                        runtime_range_not_applicable(),
                    ));
                }
                if matches!(
                    func.runtime_helper(),
                    Some(
                        cellscript_artifact_checker::runtime_contract::RuntimeHelper::Bip340RequireSignature
                            | cellscript_artifact_checker::runtime_contract::RuntimeHelper::Bip340RequireSignatureFromCellDep
                    )
                ) {
                    let dep_index = if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::Bip340RequireSignatureFromCellDep)
                    {
                        runtime_scalar_from_operand(args.first(), Some(63))
                    } else {
                        runtime_scalar_static(0, Some(0))
                    };
                    accesses.extend([
                        runtime_access_metadata(
                            "bip340-pipe",
                            "PIPE",
                            "Process",
                            "verifier::btc::bip340::require_signature",
                            "Process",
                            "external-adapter",
                            None,
                            runtime_scalar_not_applicable(),
                            runtime_range_not_applicable(),
                        ),
                        runtime_access_metadata(
                            "bip340-spawn",
                            "SPAWN",
                            "CellDep",
                            if func.runtime_helper()
                                == Some(
                                    cellscript_artifact_checker::runtime_contract::RuntimeHelper::Bip340RequireSignatureFromCellDep,
                                )
                            {
                                "verifier::btc::bip340::require_signature_from_cell_dep".to_string()
                            } else {
                                "cellscript_btc_bip340_verifier_riscv".to_string()
                            },
                            "CellDep",
                            "external-adapter",
                            None,
                            dep_index,
                            runtime_range_not_applicable(),
                        ),
                        runtime_access_metadata(
                            "bip340-pipe-write-18-words",
                            "PIPE_WRITE",
                            "Process",
                            "cellscript-btc-bip340-ipc-v0",
                            "Process",
                            "external-adapter",
                            None,
                            runtime_scalar_not_applicable(),
                            runtime_range_fixed(18 * 8),
                        ),
                        runtime_access_metadata(
                            "bip340-close",
                            "CLOSE",
                            "Process",
                            "cellscript-btc-bip340-ipc-v0",
                            "Process",
                            "external-adapter",
                            None,
                            runtime_scalar_not_applicable(),
                            runtime_range_not_applicable(),
                        ),
                        runtime_access_metadata(
                            "bip340-wait",
                            "WAIT",
                            "Process",
                            "cellscript_btc_bip340_verifier_riscv",
                            "Process",
                            "external-adapter",
                            None,
                            runtime_scalar_not_applicable(),
                            runtime_range_not_applicable(),
                        ),
                    ]);
                }
                if let Some(access) = runtime_access_for_call(func, args, &view_provenance) {
                    accesses.push(access);
                }
            }
        }
    }
    accesses
}

fn cell_source(source: ir::IrCellSource) -> &'static str {
    match source {
        ir::IrCellSource::Input => "Input",
        ir::IrCellSource::Output => "Output",
        ir::IrCellSource::GroupInput => "GroupInput",
        ir::IrCellSource::GroupOutput => "GroupOutput",
        ir::IrCellSource::CellDep => "CellDep",
    }
}

fn resolved_cell_access(
    body: &ir::IrBody,
    role: ir::IrCellBindingRole,
    binding: &str,
    operation: &str,
) -> Option<CkbRuntimeAccessMetadata> {
    let resolved = body.cell_binding(role, binding)?;
    Some(static_runtime_access(operation, "LOAD_CELL", cell_source(resolved.source), resolved.ordinal, binding))
}

fn body_unique_identity_runtime_accesses(body: &ir::IrBody) -> Vec<CkbRuntimeAccessMetadata> {
    let mut accesses = Vec::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::CreateUnique { pattern, identity, .. } => {
                    if let Some(output) = body.cell_binding(ir::IrCellBindingRole::Output, &pattern.binding) {
                        push_create_unique_identity_accesses(&mut accesses, output, pattern, identity);
                    }
                }
                ir::IrInstruction::ReplaceUnique { operand: ir::IrOperand::Var(input), pattern, identity, .. } => {
                    if let (Some(input), Some(output)) =
                        (body.cell_binding_for_local(input.id), body.cell_binding(ir::IrCellBindingRole::Output, &pattern.binding))
                    {
                        push_replace_unique_identity_accesses(&mut accesses, input, output, pattern, identity);
                    }
                }
                _ => {}
            }
        }
    }
    accesses
}

fn push_create_unique_identity_accesses(
    accesses: &mut Vec<CkbRuntimeAccessMetadata>,
    output: &ir::IrCellBinding,
    pattern: &ir::CreatePattern,
    identity: &ir::IrIdentityPolicy,
) {
    match identity {
        ir::IrIdentityPolicy::ScriptArgs => {
            accesses.push(identity_runtime_access("create_unique-identity-lock_hash", "GroupInput", 0, &pattern.binding));
            accesses.push(identity_runtime_access(
                "create_unique-identity-lock_hash",
                cell_source(output.source),
                output.ordinal,
                &pattern.binding,
            ));
        }
        ir::IrIdentityPolicy::SingletonType => {
            accesses.push(identity_runtime_access("create_unique-identity-type_hash", "GroupInput", 0, &pattern.binding));
            accesses.push(identity_runtime_access(
                "create_unique-identity-type_hash",
                cell_source(output.source),
                output.ordinal,
                &pattern.binding,
            ));
        }
        ir::IrIdentityPolicy::CkbTypeId => {
            accesses.push(identity_runtime_access(
                "create_unique-identity-type_hash",
                cell_source(output.source),
                output.ordinal,
                &pattern.binding,
            ));
        }
        ir::IrIdentityPolicy::Field(_) | ir::IrIdentityPolicy::None => {}
    }
}

fn push_replace_unique_identity_accesses(
    accesses: &mut Vec<CkbRuntimeAccessMetadata>,
    input: &ir::IrCellBinding,
    output: &ir::IrCellBinding,
    pattern: &ir::CreatePattern,
    identity: &ir::IrIdentityPolicy,
) {
    match identity {
        ir::IrIdentityPolicy::ScriptArgs => {
            accesses.push(identity_runtime_access(
                "replace_unique-identity-lock_hash",
                cell_source(input.source),
                input.ordinal,
                &pattern.binding,
            ));
            accesses.push(identity_runtime_access(
                "replace_unique-identity-lock_hash",
                cell_source(output.source),
                output.ordinal,
                &pattern.binding,
            ));
        }
        ir::IrIdentityPolicy::CkbTypeId | ir::IrIdentityPolicy::SingletonType => {
            accesses.push(identity_runtime_access(
                "replace_unique-identity-type_hash",
                cell_source(input.source),
                input.ordinal,
                &pattern.binding,
            ));
            accesses.push(identity_runtime_access(
                "replace_unique-identity-type_hash",
                cell_source(output.source),
                output.ordinal,
                &pattern.binding,
            ));
        }
        ir::IrIdentityPolicy::Field(_) | ir::IrIdentityPolicy::None => {}
    }
}

fn identity_runtime_access(operation: &str, source: &str, index: usize, binding: &str) -> CkbRuntimeAccessMetadata {
    let mut access = static_runtime_access(operation, "LOAD_CELL_BY_FIELD", source, index, binding);
    access.provenance.range = runtime_range_fixed(32);
    access
}

fn ckb_v014_runtime_access(func: &str) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    match func {
        "__ckb_witness_bytes32" => Some(("witness-bytes32-exact", "LOAD_WITNESS", "Witness", "witness::bytes32")),
        "__ckb_transaction_u32_le" => Some(("transaction-u32-le-exact", "LOAD_TRANSACTION", "Transaction", "ckb::transaction_u32_le")),
        "__ckb_transaction_blake2b_gather" => {
            Some(("transaction-blake2b-gather", "LOAD_TRANSACTION", "Transaction", "ckb::transaction_blake2b_gather"))
        }
        "__ckb_witness_blake2b_select_chunks" => {
            Some(("witness-blake2b-select-chunks", "LOAD_WITNESS", "Witness", "witness::blake2b_select_chunks"))
        }
        "__ckb_cell_data_hash_field" => {
            Some(("cell-data-hash-field", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_data_hash_field"))
        }
        "__ckb_raw_transaction_hash_without_cell_deps" => Some((
            "raw-transaction-empty-cell-deps-hash",
            "LOAD_TRANSACTION",
            "Transaction",
            "ckb::raw_transaction_hash_without_cell_deps",
        )),
        "__ckb_exec_cell_dep_u8_args" => Some(("exec-process-replacement", "EXEC", "CellDep", "ckb::exec_cell_dep_u8_args")),
        "__ckb_exec_cell_dep_hex4" => Some(("exec-process-replacement", "EXEC", "CellDep", "ckb::exec_cell_dep_hex4")),
        "__ckb_spawn_wait_cell_dep_hex4" => Some(("spawn-wait-checked", "SPAWN", "CellDep", "ckb::spawn_wait_cell_dep_hex4")),
        "__ckb_wait" => Some(("wait", "WAIT", "Process", "wait")),
        "__ckb_process_id" => Some(("process-id", "PROCESS_ID", "Process", "process_id")),
        "__ckb_pipe" => Some(("pipe", "PIPE", "Process", "pipe")),
        "__ckb_pipe_write" => Some(("pipe-write", "PIPE_WRITE", "Process", "pipe_write")),
        "__ckb_pipe_read" => Some(("pipe-read", "PIPE_READ", "Process", "pipe_read")),
        "__ckb_inherited_fd" => Some(("inherited-fd", "INHERITED_FD", "Process", "inherited_fd")),
        "__ckb_close" => Some(("close-fd", "CLOSE", "Process", "close")),
        "__ckb_source_input" => Some(("source-input", "SOURCE_VIEW", "Input", "source::input")),
        "__ckb_source_output" => Some(("source-output", "SOURCE_VIEW", "Output", "source::output")),
        "__ckb_source_cell_dep" => Some(("source-cell-dep", "SOURCE_VIEW", "CellDep", "source::cell_dep")),
        "__ckb_source_header_dep" => Some(("source-header-dep", "SOURCE_VIEW", "HeaderDep", "source::header_dep")),
        "__ckb_header_dep_epoch_number" => {
            Some(("header-dep-epoch-number", "LOAD_HEADER_BY_FIELD", "HeaderDep", "HeaderDepView.epoch_number"))
        }
        "__ckb_header_dep_epoch_start_block_number" => Some((
            "header-dep-epoch-start-block-number",
            "LOAD_HEADER_BY_FIELD",
            "HeaderDep",
            "HeaderDepView.epoch_start_block_number",
        )),
        "__ckb_header_dep_epoch_length" => {
            Some(("header-dep-epoch-length", "LOAD_HEADER_BY_FIELD", "HeaderDep", "HeaderDepView.epoch_length"))
        }
        "__ckb_header_dep_block_number" => Some(("header-dep-block-number", "LOAD_HEADER", "HeaderDep", "HeaderDepView.block_number")),
        "__ckb_header_dep_timestamp_millis" => {
            Some(("header-dep-timestamp-millis", "LOAD_HEADER", "HeaderDep", "HeaderDepView.timestamp"))
        }
        "__ckb_source_group_input" => Some(("source-group-input", "SOURCE_VIEW", "GroupInput", "source::group_input")),
        "__ckb_source_group_output" => Some(("source-group-output", "SOURCE_VIEW", "GroupOutput", "source::group_output")),
        "__ckb_since_epoch_absolute" => {
            Some(("since-epoch-absolute", "CKB_SINCE_ENCODING", "Expression", "ckb::since_epoch_absolute"))
        }
        "__ckb_since_epoch_relative" => {
            Some(("since-epoch-relative", "CKB_SINCE_ENCODING", "Expression", "ckb::since_epoch_relative"))
        }
        "__ckb_since_block_absolute" => {
            Some(("since-block-absolute", "CKB_SINCE_ENCODING", "Expression", "ckb::since_absolute_block"))
        }
        "__ckb_since_block_relative" => {
            Some(("since-block-relative", "CKB_SINCE_ENCODING", "Expression", "ckb::since_relative_block"))
        }
        "__ckb_since_timestamp_absolute" => {
            Some(("since-timestamp-absolute", "CKB_SINCE_ENCODING", "Expression", "ckb::since_absolute_timestamp"))
        }
        "__ckb_since_timestamp_relative" => {
            Some(("since-timestamp-relative", "CKB_SINCE_ENCODING", "Expression", "ckb::since_relative_timestamp"))
        }
        "__ckb_since_decode" => Some(("since-decode-checked", "CKB_SINCE_ENCODING", "Expression", "ckb::since_decode")),
        "__ckb_since_from_raw_checked" => {
            Some(("since-from-raw-checked", "CKB_SINCE_ENCODING", "Expression", "ckb::since_from_raw_checked"))
        }
        "__ckb_since_as_absolute_block" => {
            Some(("since-narrow-absolute-block", "CKB_SINCE_ENCODING", "Expression", "ckb::since_as_absolute_block"))
        }
        "__ckb_since_as_relative_block" => {
            Some(("since-narrow-relative-block", "CKB_SINCE_ENCODING", "Expression", "ckb::since_as_relative_block"))
        }
        "__ckb_since_as_absolute_epoch" => {
            Some(("since-narrow-absolute-epoch", "CKB_SINCE_ENCODING", "Expression", "ckb::since_as_absolute_epoch"))
        }
        "__ckb_since_as_relative_epoch" => {
            Some(("since-narrow-relative-epoch", "CKB_SINCE_ENCODING", "Expression", "ckb::since_as_relative_epoch"))
        }
        "__ckb_since_as_absolute_timestamp" => {
            Some(("since-narrow-absolute-timestamp", "CKB_SINCE_ENCODING", "Expression", "ckb::since_as_absolute_timestamp"))
        }
        "__ckb_since_as_relative_timestamp" => {
            Some(("since-narrow-relative-timestamp", "CKB_SINCE_ENCODING", "Expression", "ckb::since_as_relative_timestamp"))
        }
        "__ckb_since_is_relative" => Some(("since-relative-flag", "CKB_SINCE_ENCODING", "Expression", "ckb::since_is_relative")),
        "__ckb_since_is_disabled" => Some(("since-disabled", "CKB_SINCE_ENCODING", "Expression", "ckb::since_is_disabled")),
        "__ckb_since_metric" => Some(("since-metric", "CKB_SINCE_ENCODING", "Expression", "ckb::since_metric")),
        "__ckb_since_value" => Some(("since-value", "CKB_SINCE_ENCODING", "Expression", "ckb::since_value")),
        "__ckb_epoch_duration" => Some(("epoch-duration-checked", "CKB_EPOCH_ARITHMETIC", "Expression", "ckb::epoch_duration")),
        "__ckb_epoch_add" => Some(("epoch-add-checked", "CKB_EPOCH_ARITHMETIC", "Expression", "ckb::epoch_add")),
        "__ckb_epoch_sub" => Some(("epoch-sub-checked", "CKB_EPOCH_ARITHMETIC", "Expression", "ckb::epoch_sub")),
        "__ckb_cell_capacity" => Some(("cell-capacity", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_capacity")),
        "__ckb_cell_occupied_capacity" => {
            Some(("cell-occupied-capacity", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_occupied_capacity"))
        }
        "__ckb_cell_unoccupied_capacity" => {
            Some(("cell-unoccupied-capacity", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_unoccupied_capacity"))
        }
        "__ckb_cell_output_index" => Some(("cell-output-index", "SOURCE_VIEW", "SourceView", "ckb::cell_output_index")),
        "__ckb_input_out_point_index" => {
            Some(("input-out-point-index", "LOAD_INPUT_BY_FIELD", "SourceView", "ckb::input_out_point_index"))
        }
        "__ckb_input_out_point_tx_hash_low" => {
            Some(("input-out-point-tx-hash-low", "LOAD_INPUT_BY_FIELD", "SourceView", "ckb::input_out_point_tx_hash_low"))
        }
        "__ckb_input_out_point_tx_hash" => {
            Some(("input-out-point-tx-hash-read", "LOAD_INPUT_BY_FIELD", "SourceView", "ckb::input_out_point_tx_hash"))
        }
        "__ckb_require_input_out_point_tx_hash" => {
            Some(("input-out-point-tx-hash-require", "LOAD_INPUT_BY_FIELD", "SourceView", "ckb::require_input_out_point_tx_hash"))
        }
        "__ckb_require_input_out_point" => {
            Some(("input-out-point-require", "LOAD_INPUT_BY_FIELD", "SourceView", "ckb::require_input_out_point"))
        }
        "__ckb_require_metapoint_relative" => {
            Some(("metapoint-relative-require", "LOAD_INPUT_BY_FIELD/SOURCE_VIEW", "Input/Output", "ckb::require_metapoint_relative"))
        }
        "__ckb_require_lock_type_metapoint_pairs" => Some((
            "metapoint-lock-type-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output",
            "ckb::require_lock_type_metapoint_pairs",
        )),
        "__ckb_require_type_lock_metapoint_pairs" => Some((
            "metapoint-type-lock-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output",
            "ckb::require_type_lock_metapoint_pairs",
        )),
        "__ckb_require_lock_type_metapoint_pairs_from_i32_data" => Some((
            "metapoint-lock-type-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output",
            "ckb::require_lock_type_metapoint_pairs_from_i32_data",
        )),
        "__ckb_require_type_lock_metapoint_pairs_from_i32_data" => Some((
            "metapoint-type-lock-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output",
            "ckb::require_type_lock_metapoint_pairs_from_i32_data",
        )),
        "__ckb_require_lock_type_metapoint_pairs_from_i32_data_filtered" => Some((
            "metapoint-lock-type-filtered-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output",
            "ckb::require_lock_type_metapoint_pairs_from_i32_data_filtered",
        )),
        "__ckb_require_type_lock_metapoint_pairs_from_i32_data_filtered" => Some((
            "metapoint-type-lock-filtered-data-pair-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD/SOURCE_VIEW",
            "Input/Output",
            "ckb::require_type_lock_metapoint_pairs_from_i32_data_filtered",
        )),
        "__ckb_require_lock_match_master_out_point_pairs_from_data" => Some((
            "metapoint-lock-match-master-out-point-data-cardinality",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_INPUT_BY_FIELD",
            "Input/Output",
            "ckb::require_lock_match_master_out_point_pairs_from_data",
        )),
        "__ckb_cell_lock_hash_low" => Some(("cell-lock-hash-low", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_hash_low")),
        "__ckb_cell_type_hash_low" => Some(("cell-type-hash-low", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_hash_low")),
        "__ckb_cell_lock_hash" => Some(("cell-lock-hash-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_hash")),
        "__ckb_cell_type_hash" => Some(("cell-type-hash-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_hash")),
        "__ckb_cell_lock_code_hash" => {
            Some(("cell-lock-script-code-hash-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_code_hash"))
        }
        "__ckb_cell_type_code_hash" => {
            Some(("cell-type-script-code-hash-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_code_hash"))
        }
        "__ckb_cell_lock_hash_type" => {
            Some(("cell-lock-script-hash-type-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_hash_type"))
        }
        "__ckb_cell_type_hash_type" => {
            Some(("cell-type-script-hash-type-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_hash_type"))
        }
        "__ckb_cell_lock_args_empty" => {
            Some(("cell-lock-script-args-empty-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_args_empty"))
        }
        "__ckb_cell_type_args_empty" => {
            Some(("cell-type-script-args-empty-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_args_empty"))
        }
        "__ckb_cell_lock_args32" => {
            Some(("cell-lock-script-args32-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_args32"))
        }
        "__ckb_cell_type_args32" => {
            Some(("cell-type-script-args32-read", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_args32"))
        }
        "__ckb_require_cell_lock_hash" => {
            Some(("cell-lock-hash-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_lock_hash"))
        }
        "__ckb_require_cell_type_hash" => {
            Some(("cell-type-hash-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_type_hash"))
        }
        "__ckb_require_cell_lock_exact_handle" => {
            Some(("exact-handle-lock-script-binding", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_lock_exact_handle"))
        }
        "__ckb_require_cell_type_exact_handle" => {
            Some(("exact-handle-type-script-binding", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_type_exact_handle"))
        }
        "__ckb_require_cell_dep_exact_verifier_handle" => Some((
            "exact-handle-verifier-artifact-binding",
            "LOAD_CELL_BY_FIELD",
            "CellDep",
            "ckb::require_cell_dep_exact_verifier_handle",
        )),
        "__ckb_require_cell_lock_deployment_line_handle" => Some((
            "deployment-line-lock-script-binding",
            "LOAD_CELL_BY_FIELD",
            "SourceView",
            "ckb::require_cell_lock_deployment_line_handle",
        )),
        "__ckb_require_cell_type_deployment_line_handle" => Some((
            "deployment-line-type-script-binding",
            "LOAD_CELL_BY_FIELD",
            "SourceView",
            "ckb::require_cell_type_deployment_line_handle",
        )),
        "__ckb_require_cell_dep_deployment_line_verifier_handle" => Some((
            "deployment-line-verifier-artifact-binding",
            "LOAD_CELL_BY_FIELD",
            "CellDep",
            "ckb::require_cell_dep_deployment_line_verifier_handle",
        )),
        "__ckb_require_cell_data_hash" => {
            Some(("cell-data-hash-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_data_hash"))
        }
        "__ckb_require_bounded_cell_dep_data_hash" => {
            Some(("bounded-cell-dep-data-hash-scan", "LOAD_CELL_BY_FIELD", "CellDep", "ckb::require_bounded_cell_dep_data_hash"))
        }
        "__ckb_require_cell_lock_args_empty" => {
            Some(("cell-lock-script-empty-args-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_lock_args_empty"))
        }
        "__ckb_require_cell_type_args_empty" => {
            Some(("cell-type-script-empty-args-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_type_args_empty"))
        }
        "__ckb_require_cell_lock_args32" => {
            Some(("cell-lock-script-args32-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_lock_args32"))
        }
        "__ckb_require_cell_type_args32" => {
            Some(("cell-type-script-args32-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_type_args32"))
        }
        "__ckb_require_cell_lock_args_exact" => {
            Some(("cell-lock-script-exact-args-require", "LOAD_CELL_BY_FIELD", "SourceView", "script::require_cell_lock_matches"))
        }
        "__ckb_require_cell_type_args_exact" => {
            Some(("cell-type-script-exact-args-require", "LOAD_CELL_BY_FIELD", "SourceView", "script::require_cell_type_matches"))
        }
        "__ckb_require_cell_lock_args_prefix32" => Some((
            "cell-lock-script-prefix32-args-require",
            "LOAD_CELL_BY_FIELD",
            "SourceView",
            "ckb::require_cell_lock_args_prefix32",
        )),
        "__ckb_require_cell_type_args_prefix32" => Some((
            "cell-type-script-prefix32-args-require",
            "LOAD_CELL_BY_FIELD",
            "SourceView",
            "ckb::require_cell_type_args_prefix32",
        )),
        "__ckb_require_cell_lock_args_suffix32" => Some((
            "cell-lock-script-suffix32-args-require",
            "LOAD_CELL_BY_FIELD",
            "SourceView",
            "ckb::require_cell_lock_args_suffix32",
        )),
        "__ckb_require_cell_type_args_suffix32" => Some((
            "cell-type-script-suffix32-args-require",
            "LOAD_CELL_BY_FIELD",
            "SourceView",
            "ckb::require_cell_type_args_suffix32",
        )),
        "__ckb_require_cell_lock_script_hash_type" => {
            Some(("cell-lock-script-identity-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_lock_script_hash_type"))
        }
        "__ckb_require_cell_type_script_hash_type" => {
            Some(("cell-type-script-identity-require", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::require_cell_type_script_hash_type"))
        }
        "__ckb_require_current_script_args_empty" => Some((
            "current-script-empty-args-require",
            "LOAD_SCRIPT+LOAD_CELL_BY_FIELD",
            "CurrentScript/Output",
            "ckb::require_current_script_args_empty",
        )),
        "__ckb_current_script_hash" => Some(("current-script-hash", "LOAD_SCRIPT_HASH", "CurrentScript", "ckb::current_script_hash")),
        "__ckb_transaction_hash" => Some(("transaction-hash", "LOAD_TX_HASH", "Transaction", "ckb::transaction_hash")),
        "__ckb_cell_data_bytes32" => Some(("cell-data-bytes32", "LOAD_CELL_DATA", "SourceView", "ckb::cell_data_bytes32")),
        "__ckb_cell_data_blake2b_span" => {
            Some(("cell-data-blake2b-span", "LOAD_CELL_DATA", "SourceView", "ckb::cell_data_blake2b_span"))
        }
        "__ckb_witness_blake2b_span" => Some(("witness-blake2b-span", "LOAD_WITNESS", "Witness", "witness::blake2b_span")),
        "__ckb_cell_data_size" => Some(("cell-data-size", "LOAD_CELL_DATA", "SourceView", "ckb::cell_data_size")),
        "__ckb_cell_data_equal" => {
            Some(("cell-data-equality", "LOAD_CELL_DATA", "SourceView", "optimized exact Cell-data equality loop"))
        }
        "__ckb_source_bytes_equal" => Some((
            "source-byte-range-equality",
            "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_WITNESS",
            "SourceView",
            "optimized exact CKB source byte-range equality loop",
        )),
        "__ckb_source_bytes_equal_memory" => Some((
            "source-byte-range-memory-equality",
            "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_WITNESS",
            "SourceView",
            "optimized CKB source byte-range to fixed-memory equality loop",
        )),
        "__ckb_source_bytes_zero" => Some((
            "source-byte-range-zero",
            "LOAD_CELL_BY_FIELD+LOAD_CELL_DATA+LOAD_WITNESS",
            "SourceView",
            "optimized exact all-zero CKB source byte-range loop",
        )),
        "__ckb_cell_lock_size" => Some(("cell-lock-script-size", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_size")),
        "__ckb_cell_type_size" => Some(("cell-type-script-size", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_size")),
        "__ckb_cell_lock_u8" => Some(("cell-lock-script-byte", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_lock_u8")),
        "__ckb_cell_type_u8" => Some(("cell-type-script-byte", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_type_u8")),
        "__ckb_input_since_at" => Some(("input-since-at", "LOAD_INPUT_BY_FIELD", "SourceView", "ckb::input_since_at")),
        "__ckb_cell_count" => Some(("cell-source-count", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_count")),
        "__ckb_cell_has_type" => Some(("cell-type-presence", "LOAD_CELL_BY_FIELD", "SourceView", "ckb::cell_has_type")),
        "__ckb_cell_data_u8" => Some(("cell-data-u8-exact", "LOAD_CELL_DATA", "SourceView", "ckb::cell_data_u8")),
        "__ckb_cell_data_u32_le" => Some(("cell-data-u32-le", "LOAD_CELL_DATA", "SourceView", "ckb::cell_data_u32_le")),
        "__ckb_cell_data_u64_le" => Some(("cell-data-u64-le", "LOAD_CELL_DATA", "SourceView", "ckb::cell_data_u64_le")),
        "__dao_accumulated_rate" => Some(("dao-accumulated-rate", "LOAD_HEADER", "HeaderDep", "dao::accumulated_rate")),
        "__dao_input_accumulated_rate" => {
            Some(("dao-input-accumulated-rate", "LOAD_HEADER", "Input/GroupInput", "dao::input_accumulated_rate"))
        }
        "__dao_has_dao_type" => Some(("dao-type-hash-classifier", "LOAD_CELL_BY_FIELD", "SourceView", "dao::has_dao_type")),
        "__dao_is_deposit_data" => Some(("dao-deposit-data-classifier", "LOAD_CELL_DATA", "SourceView", "dao::is_deposit_data")),
        "__dao_is_withdrawal_request_data" => {
            Some(("dao-withdrawal-request-data-classifier", "LOAD_CELL_DATA", "SourceView", "dao::is_withdrawal_request_data"))
        }
        "__dao_require_header_dep_for_input" => {
            Some(("dao-header-dep-input-lineage", "LOAD_HEADER", "Input/HeaderDep", "dao::require_header_dep_for_input"))
        }
        "__dao_require_input_since_at_least" => {
            Some(("dao-input-since-maturity", "LOAD_INPUT_BY_FIELD", "Input/GroupInput", "dao::require_input_since_at_least"))
        }
        "__dao_require_input_relative_epoch_since_at_least" => Some((
            "dao-input-relative-epoch-since-maturity",
            "LOAD_INPUT_BY_FIELD",
            "Input/GroupInput",
            "dao::require_input_relative_epoch_since_at_least",
        )),
        "__xudt_amount_low" => Some(("xudt-amount-low", "LOAD_CELL_DATA", "SourceView", "xudt::amount_low")),
        "__xudt_amount_high" => Some(("xudt-amount-high", "LOAD_CELL_DATA", "SourceView", "xudt::amount_high")),
        "__xudt_owner_mode_input_type_hash" => {
            Some(("xudt-owner-mode-input-type-hash", "LOAD_CELL_BY_FIELD", "SourceView", "xudt::owner_mode_input_type_hash"))
        }
        "__xudt_require_owner_mode_input_type" => {
            Some(("xudt-require-owner-mode-input-type", "LOAD_CELL_BY_FIELD", "SourceView", "xudt::require_owner_mode_input_type"))
        }
        "__xudt_require_owner_mode_type_args" => {
            Some(("xudt-require-owner-mode-type-args", "LOAD_CELL_BY_FIELD", "SourceView", "xudt::require_owner_mode_type_args"))
        }
        "__xudt_require_owner_mode_type_args_current_script" => Some((
            "xudt-require-owner-mode-type-args-current-script",
            "LOAD_SCRIPT_HASH+LOAD_CELL_BY_FIELD",
            "CurrentScript/SourceView",
            "xudt::require_owner_mode_type_args_current_script",
        )),
        "__xudt_require_group_amount_conserved" => Some((
            "xudt-group-amount-conservation",
            "LOAD_CELL_DATA",
            "GroupInput/GroupOutput",
            "xudt::require_group_amount_conserved",
        )),
        aggregate_lowering::FUNGIBLE_TYPE_GROUP_V1_CODEGEN_HELPER => Some((
            "fungible-type-group-owner-or-conservation",
            "LOAD_SCRIPT+LOAD_CELL_BY_FIELD+LOAD_CELL_DATA",
            "CurrentScript/Input/GroupInput/GroupOutput",
            aggregate_lowering::FUNGIBLE_TYPE_GROUP_V1_METADATA_HELPER,
        )),
        "__xudt_require_group_amount_minted" => {
            Some(("xudt-group-amount-minted-delta", "LOAD_CELL_DATA", "GroupInput/GroupOutput", "xudt::require_group_amount_minted"))
        }
        "__xudt_require_group_amount_burned" => {
            Some(("xudt-group-amount-burned-delta", "LOAD_CELL_DATA", "GroupInput/GroupOutput", "xudt::require_group_amount_burned"))
        }
        "__ckb_witness_lock_exact32" => Some(("witness-lock", "LOAD_WITNESS_ARGS_LOCK", "GroupInput", "witness::lock_exact32")),
        "__ckb_witness_input_type_exact32" => {
            Some(("witness-input-type", "LOAD_WITNESS_ARGS_INPUT_TYPE", "GroupInput", "witness::input_type_exact32"))
        }
        "__ckb_witness_output_type_exact32" => {
            Some(("witness-output-type", "LOAD_WITNESS_ARGS_OUTPUT_TYPE", "GroupOutput", "witness::output_type_exact32"))
        }
        "__ckb_witness_bounded_size" => Some(("witness-bounded-size", "LOAD_WITNESS", "Witness", "witness::bounded.size")),
        "__ckb_witness_bounded_u8" => Some(("witness-bounded-u8", "LOAD_WITNESS", "Witness", "witness::byte")),
        "__ckb_witness_bounded_u32_le" => Some(("witness-bounded-u32-le", "LOAD_WITNESS", "Witness", "witness::u32_le")),
        "__ckb_witness_bounded_u64_le" => Some(("witness-bounded-u64-le", "LOAD_WITNESS", "Witness", "witness::u64_le")),
        "__ckb_witness_bounded_blake2b" => Some(("witness-bounded-blake2b", "LOAD_WITNESS", "Witness", "witness::blake2b")),
        "__ckb_witness_size" => Some(("witness-size", "LOAD_WITNESS", "Witness", "witness::size")),
        "__ckb_witness_count" => Some(("witness-count", "LOAD_WITNESS", "Witness", "witness::count")),
        "__ckb_witness_u8" => Some(("witness-u8-exact", "LOAD_WITNESS", "Witness", "witness::byte")),
        "__ckb_witness_u32_le" => Some(("witness-u32-le-exact", "LOAD_WITNESS", "Witness", "witness::u32_le")),
        "__ckb_witness_u64_le" => Some(("witness-u64-le-exact", "LOAD_WITNESS", "Witness", "witness::u64_le")),
        "__ckb_require_witness_size_at_least" => {
            Some(("require-witness-size-at-least", "LOAD_WITNESS", "Witness", "ckb::require_witness_size_at_least"))
        }
        "__ckb_sighash_all" => Some(("sighash-all-deferred", "EXIT", "Process", "env::sighash_all")),
        "__ckb_sighash_all_zero_lock" => {
            Some(("sighash-all-zero-lock-v1", "CKB_SIGHASH_ALL_ZERO_LOCK_V1", "GroupInput", "env::sighash_all_zero_lock"))
        }
        "__ckb_require_maturity" => Some(("require-maturity", "LOAD_INPUT_BY_FIELD", "GroupInput", "require_maturity")),
        "__ckb_require_time" => Some(("require-time", "LOAD_INPUT_BY_FIELD", "GroupInput", "require_time")),
        "__ckb_require_epoch_after" => Some(("require-epoch-after", "LOAD_INPUT_BY_FIELD", "GroupInput", "require_epoch_after")),
        "__ckb_require_epoch_relative" => {
            Some(("require-epoch-relative", "LOAD_INPUT_BY_FIELD", "GroupInput", "require_epoch_relative"))
        }
        "__ckb_occupied_capacity" => Some(("occupied-capacity", "CAPACITY_POLICY", "Output", "occupied_capacity")),
        "__ckb_hash_chain" => Some(("hash-chain", "CKB_BLAKE2B", "Profile", "hash_chain")),
        "__ckb_hash_pair" => Some(("hash-pair", "CKB_BLAKE2B", "Profile", "hash_pair")),
        "__ckb_hash_blake2b" => Some(("hash-blake2b", "CKB_BLAKE2B", "Profile", "hash_blake2b")),
        "__ckb_hash_sha256" => Some(("hash-sha256", "SHA256", "Profile", "ckb::hash_sha256")),
        "__ckb_hash_sha256d" => Some(("hash-sha256d", "SHA256", "Profile", "ckb::hash_sha256d")),
        "__ckb_hash_sha256_pair" => Some(("hash-sha256-pair", "SHA256", "Profile", "ckb::hash_sha256_pair")),
        "__ckb_hash_sha256d_pair" => Some(("hash-sha256d-pair", "SHA256", "Profile", "ckb::hash_sha256d_pair")),
        "__ckb_require_sha256d_merkle_root" => {
            Some(("bounded-sha256d-merkle-proof", "SHA256", "Profile", "ckb::require_sha256d_merkle_root"))
        }
        _ => None,
    }
}
