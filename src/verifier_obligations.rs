use super::ckb_runtime_accesses::body_ckb_runtime_accesses;
use super::conservation_metadata::body_resource_conservation_obligations;
use super::metadata_layout::{
    ir_operand_cell_backed_collection_type_names, ir_operand_cell_backed_type_names, ir_type_cell_backed_collection_type_names,
    metadata_type_defs_by_name,
};
use super::pool_metadata::{body_pool_primitive_metadata, operand_var_name};
use super::pool_transaction_requirements::body_pool_primitive_obligations;
use super::runtime_availability::{
    body_fail_closed_runtime_features, metadata_can_verify_create_output_fields, metadata_can_verify_output_lock,
    metadata_layout_flow_state_width, metadata_prelude_availability, MetadataPreludeAvailability,
};
use super::runtime_metadata::{
    auto_lowered_aggregate_runtime_accesses_for_action, auto_lowered_aggregate_runtime_features_for_action, body_ckb_runtime_features,
    merge_ckb_runtime_accesses, merge_ckb_runtime_features, schema_pointer_var_ids,
};
use super::schema_metadata::{metadata_identity_policy, named_type_name};
use super::transaction_requirements::{body_mutable_cell_state_obligations, destroy_group_output_absence_scan_is_checked};
use super::{
    ast, executable_surface, flow, ir, CkbRuntimeAccessMetadata, MetadataFieldLayout, MetadataTypeLayouts, VerifierObligationMetadata,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(super) fn action_terminal_flow_obligations(ir: &ir::IrModule, action: &ir::IrAction) -> Vec<VerifierObligationMetadata> {
    let type_defs = metadata_type_defs_by_name(ir);
    action
        .state_transition_edges
        .iter()
        .filter_map(|edge| {
            let type_def = type_defs.get(&edge.type_name)?;
            if !type_def.flow_terminal_states.iter().any(|terminal| terminal == &edge.to) {
                return None;
            }
            Some(VerifierObligationMetadata {
                scope: format!("action:{}", action.name),
                category: "flow-terminal".to_string(),
                feature: "terminal-by-output-state".to_string(),
                status: "checked-runtime".to_string(),
                detail: format!(
                    "output binding '{}' for '{}.{}' is checked to reach declared terminal state '{}'; this is transaction-local terminal discharge, not a global liveness proof",
                    edge.output_binding.as_deref().unwrap_or("unnamed-output"), edge.type_name, edge.field_name, edge.to
                ),
            })
        })
        .collect()
}

pub(super) fn module_verifier_obligations(
    ir: &ir::IrModule,
    type_layouts: &MetadataTypeLayouts,
    flow_states: &HashMap<String, Vec<String>>,
    flow_state_fields: &HashMap<String, String>,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<VerifierObligationMetadata> {
    let mut obligations = Vec::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                let param_schema_vars = schema_pointer_var_ids(&action.body, &action.params);
                let fail_closed_runtime_features = body_fail_closed_runtime_features(
                    &action.body,
                    &param_schema_vars,
                    type_layouts,
                    &action.params,
                    cell_type_kinds,
                    action.return_type.as_ref(),
                    pure_const_returns,
                );
                let mut ckb_runtime_features = auto_lowered_aggregate_runtime_features_for_action(ir, action);
                ckb_runtime_features.extend(merge_ckb_runtime_features(
                    body_ckb_runtime_features(&action.name, &action.body, cell_type_kinds, type_layouts),
                    &action.params,
                ));
                let ckb_runtime_features = ckb_runtime_features.into_iter().collect::<Vec<_>>();
                let mut ckb_runtime_accesses = auto_lowered_aggregate_runtime_accesses_for_action(ir, action);
                ckb_runtime_accesses.extend(merge_ckb_runtime_accesses(
                    body_ckb_runtime_accesses(&action.name, &action.body, cell_type_kinds, type_layouts),
                    &action.params,
                ));
                let mut action_obligations = body_verifier_obligations(
                    "action",
                    &action.name,
                    &action.body,
                    &action.source_dispositions,
                    &fail_closed_runtime_features,
                    &ckb_runtime_features,
                    &ckb_runtime_accesses,
                    type_layouts,
                    &action.params,
                    flow_states,
                    flow_state_fields,
                    cell_type_kinds,
                    action.return_type.as_ref(),
                    pure_const_returns,
                );
                action_obligations.extend(action_terminal_flow_obligations(ir, action));
                obligations.extend(action_obligations);
            }
            ir::IrItem::Function(function) => {
                let param_schema_vars = schema_pointer_var_ids(&function.body, &function.params);
                let fail_closed_runtime_features = body_fail_closed_runtime_features(
                    &function.body,
                    &param_schema_vars,
                    type_layouts,
                    &function.params,
                    cell_type_kinds,
                    function.return_type.as_ref(),
                    pure_const_returns,
                );
                let ckb_runtime_features = merge_ckb_runtime_features(
                    body_ckb_runtime_features(&function.name, &function.body, cell_type_kinds, type_layouts),
                    &function.params,
                );
                let ckb_runtime_accesses = merge_ckb_runtime_accesses(
                    body_ckb_runtime_accesses(&function.name, &function.body, cell_type_kinds, type_layouts),
                    &function.params,
                );
                obligations.extend(body_verifier_obligations(
                    "fn",
                    &function.name,
                    &function.body,
                    &[],
                    &fail_closed_runtime_features,
                    &ckb_runtime_features,
                    &ckb_runtime_accesses,
                    type_layouts,
                    &function.params,
                    flow_states,
                    flow_state_fields,
                    cell_type_kinds,
                    function.return_type.as_ref(),
                    pure_const_returns,
                ));
            }
            ir::IrItem::Lock(lock) => {
                let param_schema_vars = schema_pointer_var_ids(&lock.body, &lock.params);
                let fail_closed_runtime_features = body_fail_closed_runtime_features(
                    &lock.body,
                    &param_schema_vars,
                    type_layouts,
                    &lock.params,
                    cell_type_kinds,
                    None,
                    pure_const_returns,
                );
                let ckb_runtime_features = merge_ckb_runtime_features(
                    body_ckb_runtime_features(&lock.name, &lock.body, cell_type_kinds, type_layouts),
                    &lock.params,
                );
                let ckb_runtime_accesses = merge_ckb_runtime_accesses(
                    body_ckb_runtime_accesses(&lock.name, &lock.body, cell_type_kinds, type_layouts),
                    &lock.params,
                );
                obligations.extend(body_verifier_obligations(
                    "lock",
                    &lock.name,
                    &lock.body,
                    &[],
                    &fail_closed_runtime_features,
                    &ckb_runtime_features,
                    &ckb_runtime_accesses,
                    type_layouts,
                    &lock.params,
                    flow_states,
                    flow_state_fields,
                    cell_type_kinds,
                    None,
                    pure_const_returns,
                ));
            }
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => {}
        }
    }
    obligations
}

#[allow(clippy::too_many_arguments)]
pub(super) fn body_verifier_obligations(
    scope_kind: &str,
    name: &str,
    body: &ir::IrBody,
    source_dispositions: &[ir::IrSourceDisposition],
    fail_closed_runtime_features: &[String],
    ckb_runtime_features: &[String],
    ckb_runtime_accesses: &[CkbRuntimeAccessMetadata],
    type_layouts: &MetadataTypeLayouts,
    params: &[ir::IrParam],
    flow_states: &HashMap<String, Vec<String>>,
    flow_state_fields: &HashMap<String, String>,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    return_type: Option<&ir::IrType>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<VerifierObligationMetadata> {
    let scope = format!("{}:{}", scope_kind, name);
    let mut seen = BTreeSet::new();
    let mut obligations = Vec::new();

    for feature in ckb_runtime_features {
        push_verifier_obligation(
            &mut obligations,
            &mut seen,
            &scope,
            "ckb-runtime",
            feature,
            "ckb-runtime",
            "Requires the CKB-style transaction context and syscall ABI during verification",
        );
    }

    for access in ckb_runtime_accesses {
        push_verifier_obligation(
            &mut obligations,
            &mut seen,
            &scope,
            "cell-access",
            &format!("{}:{}#{}", access.operation, access.source, access.index),
            "ckb-runtime",
            &format!("{} {} from {}#{} bound to {}", access.syscall, access.operation, access.source, access.index, access.binding),
        );
        if access.operation == "spawn" {
            push_verifier_obligation(
                &mut obligations,
                &mut seen,
                &scope,
                "spawn-target",
                &format!("spawn-target:{}#{}", access.source, access.index),
                "runtime-required",
                "Spawn target must resolve to a transaction CellDep or DepGroup script reference; DSL spawn syntax does not inline or authenticate the child script",
            );
        }
        if access.operation == "exec-process-replacement" {
            push_verifier_obligation(
                &mut obligations,
                &mut seen,
                &scope,
                "exec-delegation",
                "exec-target-and-replaced-continuation",
                "runtime-required",
                "EXEC replaces this process: the selected CellDep ELF decides acceptance and parent checks after EXEC do not execute; artifact validation does not authenticate or prove the external verifier",
            );
        }
        if access.operation == "spawn-wait-checked" {
            let open_bound = body_open_verifier_spawn_is_bound(body);
            push_verifier_obligation(
                &mut obligations,
                &mut seen,
                &scope,
                "spawn-delegation",
                "spawn-target-and-checked-child-exit",
                if open_bound { "checked-runtime" } else { "runtime-required" },
                if open_bound {
                    "Committed-args VerifierHandle membership binds the exact direct CellDep before SPAWN; WAIT checks child exit before continuation. Child interface admission requires the private ProtocolBundle receipt; this does not claim behavioral equivalence or consensus acceptance"
                } else {
                    "SPAWN and WAIT require a successful child exit before continuing parent checks; artifact validation does not authenticate or prove the selected external CellDep verifier"
                },
            );
        }
    }

    for check in body_static_resource_operation_checks(body) {
        push_verifier_obligation(
            &mut obligations,
            &mut seen,
            &scope,
            "resource-operation",
            &check.feature,
            "checked-static",
            &check.detail,
        );
    }

    for check in body_transaction_resource_obligations(
        name,
        body,
        source_dispositions,
        type_layouts,
        params,
        flow_states,
        flow_state_fields,
        cell_type_kinds,
        pure_const_returns,
    ) {
        push_verifier_obligation(&mut obligations, &mut seen, &scope, check.category, &check.feature, check.status, &check.detail);
    }

    for check in body_linear_collection_obligations(body, return_type, cell_type_kinds) {
        push_verifier_obligation(&mut obligations, &mut seen, &scope, check.category, &check.feature, check.status, &check.detail);
    }

    for check in body_mutable_cell_state_obligations(body, params, cell_type_kinds, type_layouts) {
        push_verifier_obligation(&mut obligations, &mut seen, &scope, check.category, &check.feature, check.status, &check.detail);
    }

    let pool_primitives =
        body_pool_primitive_metadata(scope_kind, name, body, params, type_layouts, cell_type_kinds, pure_const_returns);
    for check in body_pool_primitive_obligations(&pool_primitives) {
        push_verifier_obligation(&mut obligations, &mut seen, &scope, check.category, &check.feature, check.status, &check.detail);
    }

    for feature in fail_closed_runtime_features {
        let detail = executable_surface::experimental_artifact_admission_detail(feature)
            .unwrap_or("Generated code rejects this path instead of accepting an unimplemented runtime operation");
        push_verifier_obligation(&mut obligations, &mut seen, &scope, "runtime-fail-closed", feature, "fail-closed", detail);
    }

    for check in body_state_transition_checks(body, flow_states, flow_state_fields, type_layouts, params, pure_const_returns) {
        push_verifier_obligation(
            &mut obligations,
            &mut seen,
            &scope,
            "state-transition",
            &format!("{}.{}", check.feature, check.field),
            &check.status,
            &check.detail,
        );
    }

    obligations
}

/// Only straight-line entry-block membership followed by the same fixed direct
/// dependency may discharge target/exit binding. Every raw spawn in the body
/// must be covered; conditional or mixed delegation retains the existing gap.
pub(super) fn body_open_verifier_spawn_is_bound(body: &ir::IrBody) -> bool {
    use ir::{IrConst, IrInstruction, IrOperand, IrType};
    let Some(entry) = body.blocks.first() else { return false };
    let raw = body
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .filter(|op| matches!(op, IrInstruction::Call { func, .. } if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSpawnWaitCellDepHex4)))
        .count();
    if raw != 1 {
        return false;
    }
    let mut views = BTreeMap::new();
    let mut checked = BTreeSet::new();
    let mut covered = 0;
    for op in &entry.instructions {
        let IrInstruction::Call { dest, func, args } = op else { continue };
        if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceCellDep) {
            if let (Some(dest), Some(IrOperand::Const(IrConst::U64(index)))) = (dest, args.first())
                && *index < 64
            {
                views.insert(dest.id, *index);
            }
        } else if func.runtime_helper()
            == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellDepOpenVerifierHandleFromArgs)
        {
            if let [IrOperand::Var(view), IrOperand::Var(handle)] = args.as_slice()
                && matches!(&handle.ty, IrType::OpenHandle(handle)
                    if handle.class == ast::OpenHandleClass::Verifier && handle.has_bounded_module_path())
                && let Some(index) = views.get(&view.id)
            {
                checked.insert(*index);
            }
        } else if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSpawnWaitCellDepHex4)
            && let Some(IrOperand::Const(IrConst::U64(index))) = args.first()
            && checked.contains(index)
        {
            covered += 1;
        }
    }
    covered == raw
}

struct StaticResourceOperationCheck {
    feature: String,
    detail: String,
}

pub(super) struct TransactionResourceObligation {
    pub(super) category: &'static str,
    pub(super) feature: String,
    pub(super) status: &'static str,
    pub(super) detail: String,
}

fn body_static_resource_operation_checks(body: &ir::IrBody) -> Vec<StaticResourceOperationCheck> {
    let mut checks = Vec::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Destroy { operand, .. } => {
                    if let Some(type_name) = operand_named_type_name(operand) {
                        checks.push(StaticResourceOperationCheck {
                            feature: format!("destroy:{}", type_name),
                            detail: format!(
                                "Type checker verified '{}' declares destroy capability or consume+burn kernel effects and the source value is marked destroyed; transaction-level absence of replacement outputs remains a runtime/protocol obligation",
                                type_name
                            ),
                        });
                    }
                }
                ir::IrInstruction::Claim { receipt, .. } => {
                    if let Some(type_name) = operand_named_type_name(receipt) {
                        checks.push(StaticResourceOperationCheck {
                            feature: format!("claim:{}", type_name),
                            detail: format!(
                                "Type checker verified '{}' is a receipt value and the receipt is linearly consumed; witness/time-lock claim conditions remain runtime/protocol obligations",
                                type_name
                            ),
                        });
                    }
                }
                ir::IrInstruction::Transfer { operand, .. } => {
                    if let Some(type_name) = operand_named_type_name(operand) {
                        checks.push(StaticResourceOperationCheck {
                            feature: format!("transfer:{}", type_name),
                            detail: format!(
                                "Type checker verified '{}' declares transfer capability or replace+relock kernel effects and the source value is linearly transferred; output relation remains a runtime/protocol obligation",
                                type_name
                            ),
                        });
                    }
                }
                ir::IrInstruction::Settle { operand, .. } => {
                    if let Some(type_name) = operand_named_type_name(operand) {
                        checks.push(StaticResourceOperationCheck {
                            feature: format!("settle:{}", type_name),
                            detail: format!(
                                "Type checker verified '{}' is a cell-backed linear value and settle consumes it; finalization invariants remain runtime/protocol obligations",
                                type_name
                            ),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    checks
}

fn body_transaction_resource_obligations(
    name: &str,
    body: &ir::IrBody,
    source_dispositions: &[ir::IrSourceDisposition],
    type_layouts: &MetadataTypeLayouts,
    params: &[ir::IrParam],
    _flow_states: &HashMap<String, Vec<String>>,
    _flow_state_fields: &HashMap<String, String>,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<TransactionResourceObligation> {
    let param_schema_vars = schema_pointer_var_ids(body, params);
    let availability = metadata_prelude_availability(body, &param_schema_vars, type_layouts, params, pure_const_returns);
    let mut checks = Vec::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Consume { operand } => {
                    if let Some(check) = operation_input_data_obligation(body, "consume", operand) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::Create { pattern, .. } => {
                    if let Some(check) = create_output_verification_obligation(pattern, type_layouts, &availability) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::CreateUnique { pattern, identity, .. } => {
                    if let Some(check) = create_output_verification_obligation(pattern, type_layouts, &availability) {
                        checks.push(check);
                    }
                    if let Some(check) = unique_identity_obligation("create_unique", &pattern.ty, &pattern.binding, identity) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::ReplaceUnique { operand, pattern, identity, .. } => {
                    if let Some(check) = operation_input_data_obligation(body, "replace_unique", operand) {
                        checks.push(check);
                    }
                    if let Some(check) = create_output_verification_obligation(pattern, type_layouts, &availability) {
                        checks.push(check);
                    }
                    if let Some(check) = unique_identity_obligation("replace_unique", &pattern.ty, &pattern.binding, identity) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::Transfer { operand, .. } => {
                    if let Some(check) = operation_input_data_obligation(body, "transfer", operand) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::Claim { receipt, .. } => {
                    if let Some(check) = operation_input_data_obligation(body, "claim", receipt) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::Settle { operand, .. } => {
                    if let Some(check) = operation_input_data_obligation(body, "settle", operand) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::CellMetadataEquality { left, right, field } => {
                    if let Some(check) = cell_metadata_equality_obligation(left, right, *field) {
                        checks.push(check);
                    }
                }
                ir::IrInstruction::Destroy { operand, .. } => {
                    if let Some(check) = operation_input_data_obligation(body, "destroy", operand) {
                        checks.push(check);
                    }
                    if let Some(type_name) = operand_named_type_name(operand) {
                        let binding = operand_var_name(operand).unwrap_or(&type_name);
                        let scan_detail = if destroy_group_output_absence_scan_is_checked(body, &type_name, binding) {
                            "; destroy-output-absence=checked-runtime; destroy-output-scan=checked-runtime"
                        } else {
                            ""
                        };
                        checks.push(TransactionResourceObligation {
                            category: "transaction-invariant",
                            feature: format!("destroy-output-scan:{}", type_name),
                            status: if scan_detail.is_empty() { "runtime-required" } else { "checked-runtime" },
                            detail: format!(
                                "Runtime verifier must scan transaction outputs to prove the destroyed '{}' instance is not recreated by the same state transition{}",
                                type_name, scan_detail
                            ),
                        });
                    }
                }
                _ => {}
            }
        }
    }
    for pattern in &body.create_set {
        if matches!(pattern.operation.as_str(), "transfer" | "claim" | "settle")
            && let Some(check) = create_output_verification_obligation(pattern, type_layouts, &availability)
        {
            checks.push(check);
        }
    }
    checks.extend(read_ref_cell_dep_data_obligations(body));
    checks.extend(body_resource_conservation_obligations(
        name,
        body,
        source_dispositions,
        type_layouts,
        &availability,
        params,
        cell_type_kinds,
    ));
    checks
}

fn operation_input_data_obligation(
    body: &ir::IrBody,
    operation: &'static str,
    operand: &ir::IrOperand,
) -> Option<TransactionResourceObligation> {
    let type_name = operand_named_type_name(operand)?;
    let binding = operand_var_name(operand).unwrap_or(type_name.as_str());
    let pattern = body.consume_set.iter().find(|pattern| pattern.operation == operation && pattern.binding == binding)?;
    pattern.type_hash?;
    let component = format!("{operation}-input-data");
    let feature_operation = operation.replace('_', "-");
    Some(TransactionResourceObligation {
        category: "transaction-invariant",
        feature: format!("{feature_operation}-input:{}:{}", type_name, binding),
        status: "checked-runtime",
        detail: format!(
            "Compiler-emitted runtime verifier loads '{}' Input cell data for '{}' through LOAD_CELL Source::Input; {}=checked-runtime",
            type_name, binding, component
        ),
    })
}

fn cell_metadata_equality_obligation(
    left: &ir::IrOperand,
    right: &ir::IrOperand,
    field: ir::CellMetadataField,
) -> Option<TransactionResourceObligation> {
    let left_binding = operand_var_name(left)?;
    let right_binding = operand_var_name(right)?;
    let field_name = cell_metadata_field_name(field);
    Some(TransactionResourceObligation {
        category: "transaction-invariant",
        feature: format!("cell-metadata-equality:{}:{}:{}", field_name, left_binding, right_binding),
        status: "checked-runtime",
        detail: format!(
            "Compiler-emitted runtime verifier compares cell metadata '{}' for '{}' and '{}'; cell-metadata-{}=checked-runtime",
            field_name, left_binding, right_binding, field_name
        ),
    })
}

fn cell_metadata_field_name(field: ir::CellMetadataField) -> &'static str {
    match field {
        ir::CellMetadataField::LockHash => "lock_hash",
        ir::CellMetadataField::Capacity => "capacity",
    }
}

pub(super) fn cell_metadata_field_byte_len(field_name: &str) -> Option<usize> {
    match field_name {
        "lock_hash" => Some(32),
        "capacity" => Some(8),
        _ => None,
    }
}

fn read_ref_cell_dep_data_obligations(body: &ir::IrBody) -> Vec<TransactionResourceObligation> {
    body.read_refs
        .iter()
        .enumerate()
        .filter(|(_, pattern)| pattern.operation == "read_ref" && pattern.type_hash.is_some())
        .map(|(index, pattern)| TransactionResourceObligation {
            category: "transaction-invariant",
            feature: format!("read-ref:{}#{}", pattern.binding, index),
            status: "checked-runtime",
            detail: format!(
                "Compiler-emitted runtime verifier loads read_ref CellDep data for '{}' through LOAD_CELL Source::CellDep index {}; read-ref-cell-dep-data=checked-runtime",
                pattern.binding, index
            ),
        })
        .collect()
}

fn create_output_verification_obligation(
    pattern: &ir::CreatePattern,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> Option<TransactionResourceObligation> {
    if !matches!(
        pattern.operation.as_str(),
        "create" | "output" | "create_unique" | "replace_unique" | "transfer" | "claim" | "settle"
    ) {
        return None;
    }
    let fields_checked = metadata_can_verify_create_output_fields(pattern, type_layouts, availability);
    let lock_checked = metadata_can_verify_output_lock(pattern, availability);
    let fields_status = if fields_checked { "checked-runtime" } else { "runtime-required" };
    let lock_status = match &pattern.lock {
        Some(_) if lock_checked => "checked-runtime",
        Some(_) => "runtime-required",
        None => "not-required",
    };
    let status = if fields_checked && lock_checked { "checked-runtime" } else { "runtime-required" };
    let operation_feature = match pattern.operation.as_str() {
        "output" => "create".to_string(),
        operation => operation.replace('_', "-"),
    };
    Some(TransactionResourceObligation {
        category: "transaction-invariant",
        feature: format!("{}-output:{}:{}", operation_feature, pattern.ty, pattern.binding),
        status,
        detail: if status == "checked-runtime" {
            format!(
                "Compiler-emitted runtime verifier checks {} output '{}' bound to '{}' has verifier-covered fields{}; {}-output-fields={}; {}-output-lock={}",
                pattern.operation,
                pattern.ty,
                pattern.binding,
                if pattern.lock.is_some() { " and lock binding" } else { "" },
                operation_feature,
                fields_status,
                operation_feature,
                lock_status
            )
        } else {
            format!(
                "Runtime verifier must prove {} output '{}' bound to '{}' has verifier-covered fields and any explicit lock binding; {}-output-fields={}; {}-output-lock={}",
                pattern.operation, pattern.ty, pattern.binding, operation_feature, fields_status, operation_feature, lock_status
            )
        },
    })
}

fn unique_identity_obligation(
    operation: &'static str,
    type_name: &str,
    binding: &str,
    identity: &ir::IrIdentityPolicy,
) -> Option<TransactionResourceObligation> {
    if matches!(identity, ir::IrIdentityPolicy::None) {
        return None;
    }
    let identity_policy = metadata_identity_policy(identity).unwrap_or_else(|| "none".to_string());
    let operation_feature = operation.replace('_', "-");
    let local_boundary = match (operation, identity) {
        ("create_unique", ir::IrIdentityPolicy::Field(_)) => {
            "; global field uniqueness remains a builder/indexer obligation outside CKB-VM"
        }
        ("create_unique", ir::IrIdentityPolicy::CkbTypeId) => "; TYPE_ID uniqueness remains bound to the CKB TYPE_ID builder plan",
        _ => "",
    };
    Some(TransactionResourceObligation {
        category: "transaction-invariant",
        feature: format!("{}-identity:{}:{}", operation_feature, type_name, identity_policy),
        status: "checked-runtime",
        detail: format!(
            "Compiler-emitted runtime verifier checks {} identity policy '{}' for '{}' bound to '{}'; {}-identity=checked-runtime{}",
            operation, identity_policy, type_name, binding, operation_feature, local_boundary
        ),
    })
}

fn body_linear_collection_obligations(
    body: &ir::IrBody,
    return_type: Option<&ir::IrType>,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
) -> Vec<TransactionResourceObligation> {
    let mut operations_by_type = BTreeMap::<String, BTreeSet<&'static str>>::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::CollectionPush { value, .. } => {
                    for type_name in ir_operand_cell_backed_type_names(value, cell_type_kinds) {
                        operations_by_type.entry(type_name).or_default().insert("push");
                    }
                }
                ir::IrInstruction::CollectionExtend { slice, .. } => {
                    for type_name in ir_operand_cell_backed_collection_type_names(slice, cell_type_kinds) {
                        operations_by_type.entry(type_name).or_default().insert("extend");
                    }
                }
                ir::IrInstruction::CollectionInsert { value, .. } => {
                    for type_name in ir_operand_cell_backed_type_names(value, cell_type_kinds) {
                        operations_by_type.entry(type_name).or_default().insert("insert");
                    }
                }
                ir::IrInstruction::CollectionSet { value, .. } => {
                    for type_name in ir_operand_cell_backed_type_names(value, cell_type_kinds) {
                        operations_by_type.entry(type_name).or_default().insert("set");
                    }
                }
                _ => {}
            }
        }
    }

    if let Some(return_type) = return_type {
        for type_name in ir_type_cell_backed_collection_type_names(return_type, cell_type_kinds) {
            operations_by_type.entry(type_name).or_default().insert("return");
        }
    }

    operations_by_type
        .into_iter()
        .map(|(type_name, operations)| {
            let operations = operations.into_iter().collect::<Vec<_>>().join("+");
            TransactionResourceObligation {
                category: "transaction-invariant",
                feature: format!("linear-collection:{}", type_name),
                status: "runtime-required",
                detail: format!(
                    "Cell-backed collection carrying '{}' crosses {} path(s); linear-collection-ownership=runtime-required; generated code fails closed through cell-backed-collection-* features until a real linear collection ownership model exists",
                    type_name, operations
                ),
            }
        })
        .collect()
}

#[derive(Debug, Clone)]
pub(super) struct MetadataFieldAlias {
    pub(super) root_id: usize,
    pub(super) root_name: String,
    pub(super) field: String,
}

#[derive(Debug, Clone)]
pub(super) enum MetadataU64Source {
    Field(MetadataFieldAlias),
    Add(Box<MetadataU64Source>, Box<MetadataU64Source>),
    Sub(Box<MetadataU64Source>, ir::IrOperand),
}

fn push_verifier_obligation(
    obligations: &mut Vec<VerifierObligationMetadata>,
    seen: &mut BTreeSet<(String, String, String, String)>,
    scope: &str,
    category: &str,
    feature: &str,
    status: &str,
    detail: &str,
) {
    let key = (scope.to_string(), category.to_string(), feature.to_string(), status.to_string());
    if seen.insert(key) {
        obligations.push(VerifierObligationMetadata {
            scope: scope.to_string(),
            category: category.to_string(),
            feature: feature.to_string(),
            status: status.to_string(),
            detail: detail.to_string(),
        });
    }
}

struct StateTransitionCheck {
    feature: String,
    field: String,
    status: String,
    detail: String,
}

fn body_state_transition_checks(
    body: &ir::IrBody,
    flow_states: &HashMap<String, Vec<String>>,
    flow_state_fields: &HashMap<String, String>,
    type_layouts: &MetadataTypeLayouts,
    params: &[ir::IrParam],
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<StateTransitionCheck> {
    let mut consumed_types = body_consumed_named_types(body);
    for pattern in &body.consume_set {
        if let Some(type_name) = params.iter().find(|param| param.name == pattern.binding).and_then(|param| named_type_name(&param.ty))
        {
            consumed_types.insert(type_name.to_string());
        }
    }
    let param_schema_vars = schema_pointer_var_ids(body, params);
    let availability = metadata_prelude_availability(body, &param_schema_vars, type_layouts, params, pure_const_returns);
    let mut checks = Vec::new();
    let mut seen = BTreeSet::new();
    for pattern in &body.create_set {
        if pattern.operation == "settle" {
            continue;
        }
        if !flow_states.contains_key(&pattern.ty) || !consumed_types.contains(&pattern.ty) {
            continue;
        }
        let state_field = flow_state_fields.get(&pattern.ty).cloned().unwrap_or_else(|| flow::FLOW_STATE_FIELD_NAME.to_string());
        if !seen.insert(pattern.ty.clone()) {
            continue;
        }
        if metadata_can_verify_state_transition(pattern, type_layouts, flow_state_fields, &availability) {
            checks.push(StateTransitionCheck {
                feature: pattern.ty.clone(),
                field: state_field,
                status: "checked-runtime".to_string(),
                detail: "Compiler emits runtime state graph and input/output state range checks, and the state output is already fully covered by the fixed-field verifier".to_string(),
            });
        } else {
            checks.push(StateTransitionCheck {
                feature: pattern.ty.clone(),
                field: state_field,
                status: "checked-partial".to_string(),
                detail: "Compiler emits declaration/static checks, but this transition still lacks a complete fixed-field runtime verifier path".to_string(),
            });
        }
    }
    checks
}

fn metadata_can_verify_state_transition(
    pattern: &ir::CreatePattern,
    type_layouts: &MetadataTypeLayouts,
    flow_state_fields: &HashMap<String, String>,
    availability: &MetadataPreludeAvailability,
) -> bool {
    let Some(layouts) = type_layouts.get(&pattern.ty) else {
        return false;
    };
    if metadata_type_encoded_size_from_layouts(layouts).is_none() {
        return false;
    }
    let state_field = flow_state_fields.get(&pattern.ty).map_or(flow::FLOW_STATE_FIELD_NAME, String::as_str);
    let Some(state_layout) = layouts.get(state_field) else {
        return false;
    };
    if metadata_layout_flow_state_width(state_layout).is_none() {
        return false;
    }
    if pattern.operation == "output" {
        return true;
    }
    metadata_can_verify_create_output_fields(pattern, type_layouts, availability)
}

pub(super) fn metadata_type_encoded_size_from_layouts(layouts: &HashMap<String, MetadataFieldLayout>) -> Option<usize> {
    layouts.values().try_fold(0usize, |acc, layout| layout.fixed_size.and_then(|size| acc.checked_add(size)))
}

fn body_consumed_named_types(body: &ir::IrBody) -> BTreeSet<String> {
    let mut types = BTreeSet::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            let operand = match instruction {
                ir::IrInstruction::Consume { operand }
                | ir::IrInstruction::Transfer { operand, .. }
                | ir::IrInstruction::Destroy { operand, .. }
                | ir::IrInstruction::Settle { operand, .. }
                | ir::IrInstruction::ReplaceUnique { operand, .. } => Some(operand),
                ir::IrInstruction::Claim { receipt, .. } => Some(receipt),
                _ => None,
            };
            if let Some(ir::IrOperand::Var(var)) = operand
                && let Some(type_name) = named_type_name(&var.ty)
            {
                types.insert(type_name.to_string());
            }
        }
    }
    types
}

pub(super) fn operand_named_type_name(operand: &ir::IrOperand) -> Option<String> {
    match operand {
        ir::IrOperand::Var(var) => named_type_name(&var.ty).map(str::to_string),
        ir::IrOperand::Const(_) => None,
    }
}
