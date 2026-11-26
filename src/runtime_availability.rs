use super::metadata_abi_binding::metadata_collection_push_is_verified_append;
use super::metadata_layout::{
    const_usize_operand, ir_operand_contains_cell_backed_value, ir_operand_is_cell_backed_collection,
    ir_type_is_cell_backed_collection, metadata_aggregate_or_named_field_layout, metadata_fixed_byte_const_len,
    metadata_fixed_scalar_const_value, metadata_fixed_scalar_size, metadata_tuple_return_field_type, operand_fixed_byte_width,
    operand_static_length, type_static_length,
};
use super::runtime_metadata::{
    consumed_schema_var_id, is_executable_aggregate_field_access, is_executable_destroy, is_executable_output_type_hash,
    is_executable_schema_field_access, is_executable_tuple_call_return_field_access,
};
use super::schema_metadata::named_type_name;
use super::{
    ast, cell_data_view, ir, nested_values, script_handle_contract, zk_contract, MetadataFieldLayout, MetadataTypeLayouts,
    CKB_SCRIPT_HASH_MAX_ARGS_BYTES,
};
use std::collections::{BTreeSet, HashMap, HashSet};

const COLLECTION_FAIL_CLOSED_FEATURE_NEW: &str = "collection-new";

const COLLECTION_FAIL_CLOSED_FEATURE_CAPACITY: &str = "collection-capacity";

const COLLECTION_FAIL_CLOSED_FEATURE_PUSH: &str = "collection-push";

const COLLECTION_FAIL_CLOSED_FEATURE_EXTEND: &str = "collection-extend";

const COLLECTION_FAIL_CLOSED_FEATURE_CLEAR: &str = "collection-clear";

const COLLECTION_FAIL_CLOSED_FEATURE_REVERSE: &str = "collection-reverse";

const COLLECTION_FAIL_CLOSED_FEATURE_TRUNCATE: &str = "collection-truncate";

const COLLECTION_FAIL_CLOSED_FEATURE_SWAP: &str = "collection-swap";

const COLLECTION_FAIL_CLOSED_FEATURE_CONTAINS: &str = "collection-contains";

const COLLECTION_FAIL_CLOSED_FEATURE_REMOVE: &str = "collection-remove";

const COLLECTION_FAIL_CLOSED_FEATURE_INSERT: &str = "collection-insert";

const COLLECTION_FAIL_CLOSED_FEATURE_SET: &str = "collection-set";

const COLLECTION_FAIL_CLOSED_FEATURE_POP: &str = "collection-pop";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CAPACITY: &str = "cell-backed-collection-capacity";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_PUSH: &str = "cell-backed-collection-push";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_EXTEND: &str = "cell-backed-collection-extend";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CLEAR: &str = "cell-backed-collection-clear";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_REVERSE: &str = "cell-backed-collection-reverse";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_TRUNCATE: &str = "cell-backed-collection-truncate";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_SWAP: &str = "cell-backed-collection-swap";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CONTAINS: &str = "cell-backed-collection-contains";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_REMOVE: &str = "cell-backed-collection-remove";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_INSERT: &str = "cell-backed-collection-insert";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_SET: &str = "cell-backed-collection-set";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_POP: &str = "cell-backed-collection-pop";

const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_RETURN: &str = "cell-backed-collection-return";

#[cfg(test)]
pub(super) const COLLECTION_FAIL_CLOSED_FEATURE_NAMES: &[&str] = &[
    COLLECTION_FAIL_CLOSED_FEATURE_NEW,
    COLLECTION_FAIL_CLOSED_FEATURE_CAPACITY,
    COLLECTION_FAIL_CLOSED_FEATURE_PUSH,
    COLLECTION_FAIL_CLOSED_FEATURE_EXTEND,
    COLLECTION_FAIL_CLOSED_FEATURE_CLEAR,
    COLLECTION_FAIL_CLOSED_FEATURE_REVERSE,
    COLLECTION_FAIL_CLOSED_FEATURE_TRUNCATE,
    COLLECTION_FAIL_CLOSED_FEATURE_SWAP,
    COLLECTION_FAIL_CLOSED_FEATURE_CONTAINS,
    COLLECTION_FAIL_CLOSED_FEATURE_REMOVE,
    COLLECTION_FAIL_CLOSED_FEATURE_INSERT,
    COLLECTION_FAIL_CLOSED_FEATURE_SET,
    COLLECTION_FAIL_CLOSED_FEATURE_POP,
];

#[cfg(test)]
pub(super) const CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_NAMES: &[&str] = &[
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CAPACITY,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_PUSH,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_EXTEND,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CLEAR,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_REVERSE,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_TRUNCATE,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_SWAP,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CONTAINS,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_REMOVE,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_INSERT,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_SET,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_POP,
    CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_RETURN,
];

pub(super) fn body_fail_closed_runtime_features(
    body: &ir::IrBody,
    param_schema_vars: &BTreeSet<usize>,
    type_layouts: &MetadataTypeLayouts,
    params: &[ir::IrParam],
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    return_type: Option<&ir::IrType>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<String> {
    let mut features = BTreeSet::new();
    for operation in &body.bounded_collection_ops {
        if operation.runtime_contract.is_some() {
            continue;
        }
        features.insert(
            if operation.operation == "create_each" { "bounded-create-each-runtime" } else { "bounded-consume-each-runtime" }
                .to_string(),
        );
    }
    let prelude_availability = metadata_prelude_availability(body, param_schema_vars, type_layouts, params, pure_const_returns);
    if body.create_set.iter().any(|pattern| {
        pattern.operation != "output" && !metadata_can_verify_create_output_fields(pattern, type_layouts, &prelude_availability)
    }) {
        features.insert("output-verification-incomplete".to_string());
    }
    if body
        .create_set
        .iter()
        .any(|pattern| pattern.operation != "output" && !metadata_can_verify_output_lock(pattern, &prelude_availability))
    {
        features.insert("output-lock-verification-incomplete".to_string());
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Call { func, args, .. }
                    if matches!(
                        func.runtime_helper(),
                        Some(
                            cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbTransactionBlake2bGather
                                | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBlake2bSelectChunks
                        )
                    ) =>
                {
                    let select = func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBlake2bSelectChunks);
                    let valid = args.len() == if select { 6 } else { 4 }
                        && args.iter().enumerate().skip(if select { 3 } else { 0 }).all(|(index, arg)| {
                            let element = if !select && index < 2 { ir::IrType::U64 } else { ir::IrType::U8 };
                            matches!(arg, ir::IrOperand::Var(var) if var.ty.vector_element() == Some(&element) && prelude_availability.stack_collection_vars.contains(&var.id))
                        });
                    if !valid {
                        features.insert("gather-hash-materialization".to_string());
                    }
                }
                ir::IrInstruction::Call { func: ir::IrCallTarget::Runtime(helper), .. } if helper.deferred().is_some() => {
                    let deferred =
                        ir::IrDeferredRuntimeFeature::from_runtime_helper(*helper).expect("guarded deferred runtime feature");
                    features.insert(deferred.feature().to_string());
                }
                ir::IrInstruction::Call { func, args, .. }
                    if matches!(
                        func.runtime_helper(),
                        Some(
                            cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbExecCellDepHex4
                                | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSpawnWaitCellDepHex4
                        )
                    ) =>
                {
                    let materialized = args.len() == 6
                        && args.get(1).is_some_and(|arg| {
                            matches!(arg, ir::IrOperand::Var(var) if var.ty == ir::IrType::vector(ir::IrType::U8)
                            && prelude_availability.stack_collection_vars.contains(&var.id))
                        });
                    if !materialized {
                        features.insert(
                            if func.runtime_helper()
                                == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbExecCellDepHex4)
                            {
                                "exec-argv-materialization"
                            } else {
                                "spawn-argv-materialization"
                            }
                            .to_string(),
                        );
                    }
                }
                ir::IrInstruction::FieldAccess { dest, obj, field } => {
                    if matches!((obj, &dest.ty), (ir::IrOperand::Var(var), ir::IrType::Selection(projected))
                        if matches!(&var.ty, ir::IrType::Selection(owner)
                            if owner.project_role(field).as_ref() == Some(projected.as_ref())))
                    {
                        // The catalog projection retains its nominal role and
                        // owner; independent selection proofs bind both.
                        continue;
                    }
                    if matches!(obj, ir::IrOperand::Var(var) if cell_data_view::ir_schema(&var.ty).is_some() || (field == "data" && cell_data_view::ir_transaction_schema(&var.ty).is_some()))
                    {
                        // Nominal view reads have dedicated bounded machine/lifetime contracts.
                        continue;
                    }
                    if !is_executable_schema_field_access(obj, field, param_schema_vars, type_layouts)
                        && !is_executable_aggregate_field_access(obj, field, &prelude_availability, type_layouts)
                        && !is_executable_tuple_call_return_field_access(obj, field, &prelude_availability)
                        && nested_values::ir_record_field(obj, field).as_ref() != Some(&dest.ty)
                    {
                        features.insert("field-access".to_string());
                    }
                }
                ir::IrInstruction::Binary { op: ast::BinaryOp::Eq | ast::BinaryOp::Ne, left, right, .. }
                    if (operand_fixed_byte_width(left).is_some() || operand_fixed_byte_width(right).is_some())
                        && !metadata_can_verify_fixed_byte_comparison(left, right, &prelude_availability) =>
                {
                    features.insert("fixed-byte-comparison".to_string());
                }
                ir::IrInstruction::Index { dest, arr, idx }
                    if nested_values::ir_index_result(arr).as_ref() != Some(&dest.ty)
                        && !prelude_availability.aggregate_pointer_vars.contains_key(&dest.id)
                        && !prelude_availability.fixed_value_vars.contains(&dest.id)
                        && !prelude_availability.scalar_vars.contains(&dest.id)
                        && !metadata_stack_collection_index_is_runtime_supported(
                            dest,
                            arr,
                            idx,
                            &prelude_availability,
                            type_layouts,
                        ) =>
                {
                    features.insert("index-access".to_string());
                }
                ir::IrInstruction::Length { operand, .. }
                    if operand_static_length(operand).is_none()
                        && !nested_values::ir_has_length(operand)
                        && !metadata_dynamic_length_available(operand, &prelude_availability) =>
                {
                    features.insert("dynamic-length".to_string());
                }
                ir::IrInstruction::TypeHash { .. } => {
                    if !is_executable_output_type_hash(instruction, &prelude_availability) {
                        features.insert("type-hash".to_string());
                    }
                }
                ir::IrInstruction::CollectionNew { dest, .. } => {
                    if !prelude_availability.stack_collection_vars.contains(&dest.id)
                        && !metadata_collection_new_is_verified_create_value(dest.id, body, type_layouts, &prelude_availability)
                    {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_NEW.to_string());
                    }
                }
                ir::IrInstruction::CollectionCapacity { collection, .. } => {
                    if !metadata_stack_collection_capacity_is_runtime_supported(collection, &prelude_availability, type_layouts) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_CAPACITY.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CAPACITY.to_string());
                    }
                }
                ir::IrInstruction::CollectionPush { collection, value } => {
                    if !metadata_collection_push_is_verified_append(collection, value, body, type_layouts)
                        && !metadata_collection_mutation_is_verified_create_vector(
                            collection,
                            value,
                            body,
                            type_layouts,
                            &prelude_availability,
                        )
                        && !metadata_stack_collection_push_is_runtime_supported(collection, value, &prelude_availability, type_layouts)
                    {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_PUSH.to_string());
                    }
                    if ir_operand_contains_cell_backed_value(value, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_PUSH.to_string());
                    }
                }
                ir::IrInstruction::CollectionExtend { collection, slice } => {
                    if !metadata_collection_mutation_is_verified_create_vector(
                        collection,
                        slice,
                        body,
                        type_layouts,
                        &prelude_availability,
                    ) && !metadata_stack_collection_extend_is_runtime_supported(
                        collection,
                        slice,
                        &prelude_availability,
                        type_layouts,
                    ) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_EXTEND.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(slice, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_EXTEND.to_string());
                    }
                }
                ir::IrInstruction::CollectionClear { collection } => {
                    if !metadata_stack_collection_clear_is_runtime_supported(collection, &prelude_availability) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_CLEAR.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CLEAR.to_string());
                    }
                }
                ir::IrInstruction::CollectionReverse { collection } => {
                    if !metadata_stack_collection_reverse_is_runtime_supported(collection, &prelude_availability, type_layouts) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_REVERSE.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_REVERSE.to_string());
                    }
                }
                ir::IrInstruction::CollectionTruncate { collection, len } => {
                    if !metadata_stack_collection_truncate_is_runtime_supported(collection, len, &prelude_availability) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_TRUNCATE.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_TRUNCATE.to_string());
                    }
                }
                ir::IrInstruction::CollectionSwap { collection, left, right } => {
                    if !metadata_stack_collection_swap_is_runtime_supported(
                        collection,
                        left,
                        right,
                        &prelude_availability,
                        type_layouts,
                    ) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_SWAP.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_SWAP.to_string());
                    }
                }
                ir::IrInstruction::CollectionContains { collection, value, .. } => {
                    if !metadata_stack_collection_contains_is_runtime_supported(collection, value, &prelude_availability, type_layouts)
                    {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_CONTAINS.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds)
                        || ir_operand_contains_cell_backed_value(value, cell_type_kinds)
                    {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_CONTAINS.to_string());
                    }
                }
                ir::IrInstruction::CollectionRemove { dest, collection, index } => {
                    if !metadata_stack_collection_remove_is_runtime_supported(
                        dest,
                        collection,
                        index,
                        &prelude_availability,
                        type_layouts,
                    ) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_REMOVE.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_REMOVE.to_string());
                    }
                }
                ir::IrInstruction::CollectionInsert { collection, index, value } => {
                    if !metadata_stack_collection_insert_is_runtime_supported(
                        collection,
                        index,
                        value,
                        &prelude_availability,
                        type_layouts,
                    ) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_INSERT.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds)
                        || ir_operand_contains_cell_backed_value(value, cell_type_kinds)
                    {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_INSERT.to_string());
                    }
                }
                ir::IrInstruction::CollectionSet { collection, index, value } => {
                    if !metadata_stack_collection_set_is_runtime_supported(
                        collection,
                        index,
                        value,
                        &prelude_availability,
                        type_layouts,
                    ) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_SET.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds)
                        || ir_operand_contains_cell_backed_value(value, cell_type_kinds)
                    {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_SET.to_string());
                    }
                }
                ir::IrInstruction::CollectionPop { dest, collection } => {
                    if !metadata_stack_collection_pop_is_runtime_supported(dest, collection, &prelude_availability, type_layouts) {
                        features.insert(COLLECTION_FAIL_CLOSED_FEATURE_POP.to_string());
                    }
                    if ir_operand_is_cell_backed_collection(collection, cell_type_kinds) {
                        features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_POP.to_string());
                    }
                }
                ir::IrInstruction::Consume { operand } if consumed_schema_var_id(instruction).is_none() => {
                    features.insert("consume-expression".to_string());
                    if matches!(operand, ir::IrOperand::Const(_)) {
                        features.insert("non-cell-consume".to_string());
                    }
                }
                ir::IrInstruction::Create { .. } => {}
                ir::IrInstruction::CreateUnique { dest, pattern, identity } => {
                    if !metadata_output_operation_is_verifier_covered(body, "create_unique", dest, type_layouts, &prelude_availability)
                        || !metadata_unique_identity_policy_is_executable(pattern, identity, type_layouts)
                    {
                        features.insert("create-unique-expression".to_string());
                    }
                }
                ir::IrInstruction::ReplaceUnique { dest, pattern, identity, .. } => {
                    if !metadata_output_operation_is_verifier_covered(
                        body,
                        "replace_unique",
                        dest,
                        type_layouts,
                        &prelude_availability,
                    ) || !metadata_unique_identity_policy_is_executable(pattern, identity, type_layouts)
                    {
                        features.insert("replace-unique-expression".to_string());
                    }
                }
                ir::IrInstruction::Transfer { dest, .. } => {
                    if !metadata_output_operation_is_verifier_covered(body, "transfer", dest, type_layouts, &prelude_availability) {
                        features.insert("transfer-expression".to_string());
                    }
                }
                ir::IrInstruction::Destroy { operand, policy } => {
                    if matches!(policy, ir::IrDestructionPolicy::BurnAmount { .. }) || !is_executable_destroy(operand) {
                        features.insert("destroy-expression".to_string());
                    }
                }
                ir::IrInstruction::Claim { dest, .. } => {
                    if !metadata_output_operation_is_verifier_covered(body, "claim", dest, type_layouts, &prelude_availability) {
                        features.insert("claim-expression".to_string());
                    }
                }
                ir::IrInstruction::Settle { dest, .. }
                    if !metadata_output_operation_is_verifier_covered(body, "settle", dest, type_layouts, &prelude_availability) =>
                {
                    features.insert("settle-expression".to_string());
                }
                _ => {}
            }
        }
        if matches!(block.terminator, ir::IrTerminator::Return(Some(_)))
            && return_type.is_some_and(|ty| ir_type_is_cell_backed_collection(ty, cell_type_kinds))
        {
            features.insert(CELL_BACKED_COLLECTION_FAIL_CLOSED_FEATURE_RETURN.to_string());
        }
    }
    features.into_iter().collect()
}

fn metadata_output_operation_is_verifier_covered(
    body: &ir::IrBody,
    operation: &str,
    dest: &ir::IrVar,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    let Some(dest_ty) = named_type_name(&dest.ty) else {
        return false;
    };
    body.create_set.iter().any(|pattern| {
        pattern.operation == operation
            && pattern.binding == dest.name
            && pattern.ty == dest_ty
            && metadata_can_verify_create_output_fields(pattern, type_layouts, availability)
            && metadata_can_verify_output_lock(pattern, availability)
    })
}

fn metadata_unique_identity_policy_is_executable(
    pattern: &ir::CreatePattern,
    identity: &ir::IrIdentityPolicy,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    match identity {
        ir::IrIdentityPolicy::None
        | ir::IrIdentityPolicy::CkbTypeId
        | ir::IrIdentityPolicy::ScriptArgs
        | ir::IrIdentityPolicy::SingletonType => true,
        ir::IrIdentityPolicy::Field(field) => type_layouts
            .get(pattern.ty.as_str())
            .and_then(|fields| fields.get(field))
            .is_some_and(|layout| metadata_layout_or_named_fixed_byte_width(layout, type_layouts).is_some()),
    }
}

#[derive(Debug, Default)]
pub(super) struct MetadataPreludeAvailability {
    pub(super) scalar_vars: HashSet<usize>,
    pub(super) fixed_value_vars: HashSet<usize>,
    pub(super) schema_pointer_vars: HashSet<usize>,
    pub(super) empty_molecule_vector_vars: HashSet<usize>,
    pub(super) stack_collection_vars: HashSet<usize>,
    pub(super) constructed_byte_vector_vars: HashMap<usize, usize>,
    pub(super) constructed_byte_vector_roots: HashMap<usize, usize>,
    pub(super) u64_value_vars: HashSet<usize>,
    pub(super) u64_operand_vars: HashSet<usize>,
    pub(super) dynamic_collection_vars: HashSet<usize>,
    pub(super) aggregate_pointer_vars: HashMap<usize, MetadataAggregatePointerSource>,
    pub(super) tuple_call_return_vars: HashMap<usize, ir::IrType>,
    pub(super) created_output_vars: HashMap<usize, usize>,
    pub(super) output_type_hash_vars: HashSet<usize>,
    pub(super) param_type_hash_vars: HashSet<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct MetadataAggregatePointerSource {
    pub(super) ty: ir::IrType,
}

pub(super) fn metadata_pure_const_returns(ir: &ir::IrModule) -> HashMap<String, ir::IrConst> {
    ir.items
        .iter()
        .filter_map(|item| {
            let ir::IrItem::Function(function) = item else {
                return None;
            };
            metadata_pure_const_return(&function.body).map(|value| (function.name.clone(), value))
        })
        .collect()
}

fn metadata_pure_const_return(body: &ir::IrBody) -> Option<ir::IrConst> {
    let [block] = body.blocks.as_slice() else {
        return None;
    };
    match (&block.instructions[..], &block.terminator) {
        ([], ir::IrTerminator::Return(Some(ir::IrOperand::Const(value)))) => Some(value.clone()),
        ([ir::IrInstruction::LoadConst { dest, value }], ir::IrTerminator::Return(Some(ir::IrOperand::Var(var))))
            if dest.id == var.id =>
        {
            Some(value.clone())
        }
        _ => None,
    }
}

pub(super) fn metadata_prelude_availability(
    body: &ir::IrBody,
    param_schema_vars: &BTreeSet<usize>,
    type_layouts: &MetadataTypeLayouts,
    params: &[ir::IrParam],
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> MetadataPreludeAvailability {
    use cellscript_artifact_checker::runtime_contract::RuntimeHelper;
    let mut availability = MetadataPreludeAvailability::default();
    let schema_param_ids = params
        .iter()
        .filter(|param| named_type_name(&param.ty).is_some() || param.ty.has_structured_span_abi())
        .map(|param| param.binding.id)
        .collect::<HashSet<_>>();
    availability.schema_pointer_vars.extend(schema_param_ids.iter().copied());

    for param in params {
        if metadata_fixed_scalar_size(&param.ty).is_some() {
            availability.scalar_vars.insert(param.binding.id);
            availability.fixed_value_vars.insert(param.binding.id);
            if param.ty == ir::IrType::U64 {
                availability.u64_value_vars.insert(param.binding.id);
                availability.u64_operand_vars.insert(param.binding.id);
            }
        } else if metadata_fixed_byte_width(&param.ty, type_static_length(&param.ty)).is_some() {
            availability.fixed_value_vars.insert(param.binding.id);
            if metadata_fixed_byte_width(&param.ty, type_static_length(&param.ty)).is_some_and(|width| width > 8) {
                availability.aggregate_pointer_vars.insert(param.binding.id, MetadataAggregatePointerSource { ty: param.ty.clone() });
            }
        } else if metadata_ir_type_fixed_width(&param.ty, type_layouts).is_some() {
            availability.fixed_value_vars.insert(param.binding.id);
            availability.aggregate_pointer_vars.insert(param.binding.id, MetadataAggregatePointerSource { ty: param.ty.clone() });
        } else if metadata_fixed_aggregate_pointer_size(&param.ty).is_some() {
            availability.aggregate_pointer_vars.insert(param.binding.id, MetadataAggregatePointerSource { ty: param.ty.clone() });
        }
        if metadata_molecule_vector_element_fixed_width(&param.ty, type_layouts).is_some() {
            availability.dynamic_collection_vars.insert(param.binding.id);
        }
    }

    let mut named_constructed_vectors = HashMap::<String, usize>::new();
    let mut named_stack_collections = HashMap::<String, usize>::new();
    let mut loaded_constructed_vector_names = HashMap::<usize, String>::new();
    let mut named_fixed_vars = params
        .iter()
        .filter(|param| availability.fixed_value_vars.contains(&param.binding.id))
        .map(|param| (param.name.clone(), param.binding.id))
        .collect::<HashMap<_, _>>();
    let mut named_scalar_vars = params
        .iter()
        .filter(|param| availability.scalar_vars.contains(&param.binding.id))
        .map(|param| (param.name.clone(), param.binding.id))
        .collect::<HashMap<_, _>>();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::AllocatedValue(op) => {
                    if let Some(dest) = &op.dest
                        && metadata_ir_type_fixed_width(&dest.ty, type_layouts).is_some()
                    {
                        availability.fixed_value_vars.insert(dest.id);
                        if metadata_fixed_scalar_size(&dest.ty).is_some() {
                            availability.scalar_vars.insert(dest.id);
                            if dest.ty == ir::IrType::U64 {
                                availability.u64_value_vars.insert(dest.id);
                                availability.u64_operand_vars.insert(dest.id);
                            }
                        } else {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                }
                ir::IrInstruction::StoreVar { name, src: ir::IrOperand::Var(src) } => {
                    if availability.stack_collection_vars.contains(&src.id) {
                        named_stack_collections.insert(name.clone(), src.id);
                    }
                    if availability.constructed_byte_vector_vars.contains_key(&src.id) {
                        named_constructed_vectors.insert(name.clone(), src.id);
                    }
                    if availability.fixed_value_vars.contains(&src.id) {
                        named_fixed_vars.insert(name.clone(), src.id);
                    }
                    if availability.scalar_vars.contains(&src.id) {
                        named_scalar_vars.insert(name.clone(), src.id);
                    }
                }
                ir::IrInstruction::LoadVar { dest, name } => {
                    if let Some(source_id) = named_stack_collections.get(name).copied() {
                        availability.stack_collection_vars.insert(dest.id);
                        named_stack_collections.insert(name.clone(), dest.id);
                        if let Some(byte_count) = availability.constructed_byte_vector_vars.get(&source_id).copied() {
                            availability.constructed_byte_vector_vars.insert(dest.id, byte_count);
                            if let Some(root_id) = availability.constructed_byte_vector_roots.get(&source_id).copied() {
                                availability.constructed_byte_vector_roots.insert(dest.id, root_id);
                            }
                            loaded_constructed_vector_names.insert(dest.id, name.clone());
                        }
                    }
                    if let Some(source_id) = named_constructed_vectors.get(name).copied()
                        && let Some(byte_count) = availability.constructed_byte_vector_vars.get(&source_id).copied()
                    {
                        availability.constructed_byte_vector_vars.insert(dest.id, byte_count);
                        if let Some(root_id) = availability.constructed_byte_vector_roots.get(&source_id).copied() {
                            availability.constructed_byte_vector_roots.insert(dest.id, root_id);
                        }
                        loaded_constructed_vector_names.insert(dest.id, name.clone());
                    }
                    if named_fixed_vars.contains_key(name) {
                        availability.fixed_value_vars.insert(dest.id);
                        if let Some(source) = named_fixed_vars
                            .get(name)
                            .and_then(|id| availability.aggregate_pointer_vars.get(id))
                            .filter(|source| type_layouts.types_equal(&source.ty, &dest.ty))
                            .cloned()
                        {
                            availability.aggregate_pointer_vars.insert(dest.id, source);
                        }
                    }
                    if named_scalar_vars.contains_key(name) {
                        availability.scalar_vars.insert(dest.id);
                        if dest.ty == ir::IrType::U64 {
                            availability.u64_value_vars.insert(dest.id);
                            availability.u64_operand_vars.insert(dest.id);
                        }
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), .. } if matches!(dest.ty, ir::IrType::Tuple(_)) => {
                    availability.tuple_call_return_vars.insert(dest.id, dest.ty.clone());
                    if metadata_ir_type_fixed_width(&dest.ty, type_layouts).is_some() {
                        // Fixed tuple returns are materialized in caller-owned
                        // storage, so ordinary copies retain field availability.
                        availability.fixed_value_vars.insert(dest.id);
                        availability.aggregate_pointer_vars.insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), func, .. }
                    if func
                        .function_symbol()
                        .and_then(|symbol| type_layouts.fixed_struct_returns.get(symbol))
                        .is_some_and(|ty| type_layouts.types_equal(ty, &dest.ty)) =>
                {
                    // Local fixed struct/wide-byte helpers use caller-owned
                    // result storage; short byte arrays retain the scalar ABI.
                    // Cell, enum and dynamic returns have separate rules.
                    availability.fixed_value_vars.insert(dest.id);
                    if matches!(&dest.ty, ir::IrType::Array(inner, count) if **inner == ir::IrType::U8 && (1..=8).contains(count)) {
                        availability.scalar_vars.insert(dest.id);
                    } else {
                        availability.aggregate_pointer_vars.insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), func, .. }
                    if func.function_symbol().is_some_and(|symbol| pure_const_returns.contains_key(symbol)) =>
                {
                    let value =
                        func.function_symbol().and_then(|symbol| pure_const_returns.get(symbol)).expect("guarded pure const return");
                    if metadata_fixed_scalar_const_value(value).is_some() {
                        availability.scalar_vars.insert(dest.id);
                        availability.fixed_value_vars.insert(dest.id);
                        if dest.ty == ir::IrType::U64 {
                            availability.u64_value_vars.insert(dest.id);
                            availability.u64_operand_vars.insert(dest.id);
                        }
                    } else if metadata_fixed_byte_const_len(value)
                        .is_some_and(|len| type_static_length(&dest.ty).is_some_and(|dest_len| dest_len == len))
                    {
                        availability.fixed_value_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), func, args }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashBlake2bPacked)
                        && matches!(&dest.ty, ir::IrType::Commitment(_)) =>
                {
                    if args.first().is_some_and(|arg| metadata_fixed_hash_input_available(arg, &availability, type_layouts)) {
                        availability.fixed_value_vars.insert(dest.id);
                        availability.aggregate_pointer_vars.insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), func, args }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCommitmentOpen) =>
                {
                    let width = metadata_ir_type_fixed_width(&dest.ty, type_layouts);
                    if args.len() == 2
                        && metadata_fixed_value_available_with_layout_width(&args[0], &availability, 32, type_layouts)
                        && width.is_some_and(|width| {
                            metadata_fixed_value_available_with_layout_width(&args[1], &availability, width, type_layouts)
                        })
                    {
                        availability.fixed_value_vars.insert(dest.id);
                        if width.is_some_and(|width| width > 8) || matches!(dest.ty, ir::IrType::Named(_)) {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        } else {
                            availability.scalar_vars.insert(dest.id);
                            if dest.ty == ir::IrType::U64 {
                                availability.u64_value_vars.insert(dest.id);
                                availability.u64_operand_vars.insert(dest.id);
                            }
                        }
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), func, args }
                    if dest.ty == ir::IrType::Hash
                        || func.runtime_helper().is_some_and(|helper| ir::is_ckb_script_hash_property_result(helper, &dest.ty))
                        || func.runtime_helper().is_some_and(|helper| ir::is_ckb_sighash_all_result(helper, &dest.ty))
                        || func.runtime_helper().is_some_and(|helper| ir::is_raw_bytes32_runtime_result(helper, &dest.ty)) =>
                {
                    let fixed_hash_result = match func.runtime_helper() {
                        Some(
                            RuntimeHelper::CkbHashChain
                            | RuntimeHelper::CkbHashBlake2b
                            | RuntimeHelper::CkbHashSha256
                            | RuntimeHelper::CkbHashSha256d,
                        ) => args.first().is_some_and(|arg| metadata_fixed_value_available_with_width(arg, &availability, 32)),
                        Some(RuntimeHelper::CkbHashPair | RuntimeHelper::CkbHashSha256Pair | RuntimeHelper::CkbHashSha256dPair) => {
                            args.len() == 2 && args.iter().all(|arg| metadata_fixed_value_available_with_width(arg, &availability, 32))
                        }
                        Some(
                            RuntimeHelper::CkbHashBlake2bVar | RuntimeHelper::CkbHashBlake2bPacked | RuntimeHelper::CkbHashDataPacked,
                        ) => {
                            args.len() == 1
                                && args
                                    .first()
                                    .is_some_and(|arg| metadata_fixed_hash_input_available(arg, &availability, type_layouts))
                        }
                        Some(RuntimeHelper::CkbScriptHash) => {
                            let args_width = args.get(2).and_then(operand_fixed_byte_width);
                            args.len() == 3
                                && metadata_fixed_value_available_with_width(&args[0], &availability, 32)
                                && metadata_scalar_available(&args[1], &availability)
                                && args_width.is_some_and(|width| {
                                    width <= CKB_SCRIPT_HASH_MAX_ARGS_BYTES
                                        && metadata_fixed_value_available_with_width(&args[2], &availability, width)
                                })
                        }
                        Some(
                            RuntimeHelper::CkbCurrentScriptHash
                            | RuntimeHelper::CkbTransactionHash
                            | RuntimeHelper::CkbRawTransactionHashWithoutCellDeps,
                        ) => args.is_empty(),
                        Some(RuntimeHelper::CkbSighashAllZeroLock) => args.len() == 4,
                        Some(
                            RuntimeHelper::CkbInputOutPointTxHash
                            | RuntimeHelper::CkbCellLockHash
                            | RuntimeHelper::CkbCellTypeHash
                            | RuntimeHelper::CkbCellDataHashField
                            | RuntimeHelper::CkbCellLockCodeHash
                            | RuntimeHelper::CkbCellTypeCodeHash
                            | RuntimeHelper::CkbCellLockArgs32
                            | RuntimeHelper::CkbCellTypeArgs32
                            | RuntimeHelper::CkbWitnessLockExact32
                            | RuntimeHelper::CkbWitnessInputTypeExact32
                            | RuntimeHelper::CkbWitnessOutputTypeExact32,
                        ) => args.len() == 1,
                        Some(RuntimeHelper::CkbWitnessBoundedBlake2b) => args.len() == 3,
                        Some(RuntimeHelper::CkbCellDataBytes32) => args.len() == 2,
                        Some(RuntimeHelper::CkbWitnessBytes32) => args.len() == 2,
                        Some(RuntimeHelper::CkbCellDataBlake2bSpan | RuntimeHelper::CkbWitnessBlake2bSpan) => args.len() == 3,
                        Some(RuntimeHelper::CkbTransactionBlake2bGather) => args.len() == 4,
                        Some(RuntimeHelper::CkbWitnessBlake2bSelectChunks) => args.len() == 6,
                        _ => false,
                    };
                    if fixed_hash_result {
                        availability.fixed_value_vars.insert(dest.id);
                        if dest.ty == ir::IrType::Hash
                            || matches!(&dest.ty, ir::IrType::Named(name) if ir::is_ckb_fixed_hash_domain_name(name))
                        {
                            // A checked 32-byte hash result also owns the storage
                            // used by its explicit `.0` byte serialization.
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), .. }
                    if named_type_name(&dest.ty).and_then(|name| type_layouts.enum_fixed_sizes.get(name)).is_some() =>
                {
                    availability.fixed_value_vars.insert(dest.id);
                    availability.aggregate_pointer_vars.insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                }
                ir::IrInstruction::Create { dest, .. }
                | ir::IrInstruction::CreateUnique { dest, .. }
                | ir::IrInstruction::ReplaceUnique { dest, .. } => {
                    let output_index = availability.created_output_vars.len();
                    availability.created_output_vars.insert(dest.id, output_index);
                }
                ir::IrInstruction::CollectionNew { dest, .. } => {
                    availability.stack_collection_vars.insert(dest.id);
                    availability.empty_molecule_vector_vars.insert(dest.id);
                    availability.constructed_byte_vector_vars.insert(dest.id, 0);
                    availability.constructed_byte_vector_roots.insert(dest.id, dest.id);
                }
                ir::IrInstruction::TypeHash { dest, operand: ir::IrOperand::Var(var) } => {
                    if availability.created_output_vars.contains_key(&var.id) {
                        availability.fixed_value_vars.insert(dest.id);
                        availability.output_type_hash_vars.insert(dest.id);
                    } else if schema_param_ids.contains(&var.id) {
                        availability.fixed_value_vars.insert(dest.id);
                        availability.param_type_hash_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::LoadConst { dest, value } => {
                    if metadata_fixed_scalar_const_value(value).is_some() {
                        availability.scalar_vars.insert(dest.id);
                        availability.fixed_value_vars.insert(dest.id);
                        if dest.ty == ir::IrType::U64 {
                            availability.u64_value_vars.insert(dest.id);
                            availability.u64_operand_vars.insert(dest.id);
                        }
                    } else if metadata_fixed_byte_const_len(value)
                        .is_some_and(|len| type_static_length(&dest.ty).is_some_and(|dest_len| dest_len == len))
                    {
                        availability.fixed_value_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::Tuple { dest, fields } => {
                    if metadata_ir_type_fixed_width(&dest.ty, type_layouts).is_some()
                        && fields.iter().all(|field| metadata_fixed_aggregate_part_available(field, &availability, type_layouts))
                    {
                        availability.fixed_value_vars.insert(dest.id);
                        availability.aggregate_pointer_vars.insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                    }
                }
                ir::IrInstruction::EnumConstruct { dest, .. } => {
                    if metadata_ir_type_fixed_width(&dest.ty, type_layouts).is_some() {
                        availability.fixed_value_vars.insert(dest.id);
                        availability.aggregate_pointer_vars.insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                    }
                }
                ir::IrInstruction::EnumTag { dest, .. } => {
                    availability.scalar_vars.insert(dest.id);
                    availability.fixed_value_vars.insert(dest.id);
                }
                ir::IrInstruction::EnumPayload { dest, enum_name, variant, field_index, .. } => {
                    let Some(&scalar) = type_layouts.enum_payload_storage.get(&(enum_name.clone(), variant.clone(), *field_index))
                    else {
                        continue;
                    };
                    if let Some(width) = metadata_ir_type_fixed_width(&dest.ty, type_layouts) {
                        availability.fixed_value_vars.insert(dest.id);
                        if scalar {
                            availability.scalar_vars.insert(dest.id);
                        } else if width > 0 {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                }
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(obj), field } => {
                    if availability
                        .tuple_call_return_vars
                        .get(&obj.id)
                        .and_then(|ty| metadata_tuple_return_field_type(ty, field))
                        .is_some_and(|field_ty| field_ty == dest.ty)
                        && !availability.aggregate_pointer_vars.contains_key(&obj.id)
                    {
                        continue;
                    }
                    let layout = if let Some(schema) = cell_data_view::ir_schema(&obj.ty) {
                        let Some(layout) = type_layouts.get(schema).and_then(|fields| fields.get(field)).cloned() else { continue };
                        layout
                    } else if param_schema_vars.contains(&obj.id) {
                        let Some(type_name) = named_type_name(&obj.ty) else {
                            continue;
                        };
                        let Some(layout) = type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned() else {
                            continue;
                        };
                        layout
                    } else {
                        let Some(source) = availability.aggregate_pointer_vars.get(&obj.id) else {
                            continue;
                        };
                        let Some(layout) = metadata_aggregate_or_named_field_layout(&source.ty, field, type_layouts) else {
                            continue;
                        };
                        layout
                    };
                    if metadata_layout_or_named_fixed_byte_width(&layout, type_layouts).is_some()
                        && type_layouts.types_equal(&layout.ty, &dest.ty)
                    {
                        availability.fixed_value_vars.insert(dest.id);
                        if metadata_ir_type_fixed_width(&dest.ty, type_layouts)
                            .is_some_and(|width| width > 8 || matches!(dest.ty, ir::IrType::Tuple(_) | ir::IrType::Named(_)))
                            || cell_data_view::ir_fixed_record_array_layout(&dest.ty, |name| {
                                type_layouts.fixed_cell_record_widths.get(name).copied()
                            })
                            .is_some()
                        {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                    if metadata_layout_fixed_scalar_width(&layout).is_some() && type_layouts.types_equal(&layout.ty, &dest.ty) {
                        availability.scalar_vars.insert(dest.id);
                        if dest.ty == ir::IrType::U64 {
                            availability.u64_value_vars.insert(dest.id);
                            availability.u64_operand_vars.insert(dest.id);
                        }
                    }
                    if metadata_layout_or_named_fixed_byte_width(&layout, type_layouts).is_none()
                        && metadata_molecule_vector_element_fixed_width(&layout.ty, type_layouts).is_some()
                        && type_layouts.types_equal(&layout.ty, &dest.ty)
                    {
                        availability.dynamic_collection_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::Index { dest, arr: ir::IrOperand::Var(arr), idx } => {
                    if availability.fixed_value_vars.contains(&arr.id)
                        && matches!(&arr.ty, ir::IrType::Array(element, _) if **element == dest.ty)
                        && cell_data_view::ir_fixed_record_array_layout(&arr.ty, |name| {
                            type_layouts.fixed_cell_record_widths.get(name).copied()
                        })
                        .is_some()
                        && (const_usize_operand(idx).is_some() || metadata_scalar_available(idx, &availability))
                    {
                        availability.fixed_value_vars.insert(dest.id);
                        availability.aggregate_pointer_vars.insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                    } else if availability.dynamic_collection_vars.contains(&arr.id)
                        && metadata_molecule_vector_element_fixed_width(&arr.ty, type_layouts).is_some()
                        && metadata_u64_operand_available(idx, &availability)
                    {
                        let fixed_size = metadata_ir_type_fixed_width(&dest.ty, type_layouts);
                        if metadata_fixed_scalar_width(&dest.ty, fixed_size).is_some() {
                            availability.scalar_vars.insert(dest.id);
                            availability.fixed_value_vars.insert(dest.id);
                            if dest.ty == ir::IrType::U64 {
                                availability.u64_value_vars.insert(dest.id);
                                availability.u64_operand_vars.insert(dest.id);
                            }
                        } else if metadata_fixed_byte_width(&dest.ty, fixed_size).is_some() {
                            availability.fixed_value_vars.insert(dest.id);
                        } else if matches!(dest.ty, ir::IrType::Named(_) | ir::IrType::Array(_, _) | ir::IrType::Tuple(_))
                            && fixed_size.is_some()
                        {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    } else if availability.aggregate_pointer_vars.contains_key(&arr.id)
                        || (availability.fixed_value_vars.contains(&arr.id)
                            && matches!(&arr.ty, ir::IrType::Array(inner, _) if **inner == ir::IrType::U8))
                    {
                        // Known fixed arrays have checked scalar/fixed-byte indexing in
                        // both packed-byte and pointer-backed representations.
                        // Dynamic indexing of aggregate elements stays separate.
                        if matches!(&arr.ty, ir::IrType::Array(inner, _) if inner.as_ref() == &dest.ty
                            && (metadata_fixed_scalar_size(inner).is_some()
                                || metadata_fixed_byte_width(inner, type_static_length(inner)).is_some()))
                            && const_usize_operand(idx).is_none()
                            && metadata_u64_operand_available(idx, &availability)
                        {
                            availability.fixed_value_vars.insert(dest.id);
                            if metadata_fixed_scalar_size(&dest.ty).is_some() {
                                availability.scalar_vars.insert(dest.id);
                                if dest.ty == ir::IrType::U64 {
                                    availability.u64_value_vars.insert(dest.id);
                                    availability.u64_operand_vars.insert(dest.id);
                                }
                            } else {
                                availability
                                    .aggregate_pointer_vars
                                    .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                            }
                        } else if let (ir::IrType::Array(inner, len), Some(index)) = (&arr.ty, const_usize_operand(idx)) {
                            let element_ty = inner.as_ref();
                            let fixed_size = type_static_length(element_ty);
                            if index < *len && fixed_size.is_some() {
                                if metadata_fixed_scalar_width(element_ty, fixed_size).is_some() && element_ty == &dest.ty {
                                    availability.scalar_vars.insert(dest.id);
                                    availability.fixed_value_vars.insert(dest.id);
                                    if dest.ty == ir::IrType::U64 {
                                        availability.u64_value_vars.insert(dest.id);
                                        availability.u64_operand_vars.insert(dest.id);
                                    }
                                } else if metadata_fixed_byte_width(element_ty, fixed_size).is_some() && element_ty == &dest.ty {
                                    availability.fixed_value_vars.insert(dest.id);
                                } else {
                                    availability
                                        .aggregate_pointer_vars
                                        .insert(dest.id, MetadataAggregatePointerSource { ty: element_ty.clone() });
                                }
                            }
                        }
                    } else if availability.stack_collection_vars.contains(&arr.id)
                        && metadata_molecule_vector_element_fixed_width(&arr.ty, type_layouts).is_some()
                        && (const_usize_operand(idx).is_some() || metadata_u64_operand_available(idx, &availability))
                    {
                        let fixed_size = metadata_ir_type_fixed_width(&dest.ty, type_layouts);
                        if metadata_fixed_scalar_width(&dest.ty, fixed_size).is_some() {
                            availability.scalar_vars.insert(dest.id);
                            availability.fixed_value_vars.insert(dest.id);
                            if dest.ty == ir::IrType::U64 {
                                availability.u64_value_vars.insert(dest.id);
                                availability.u64_operand_vars.insert(dest.id);
                            }
                        } else if metadata_fixed_byte_width(&dest.ty, fixed_size).is_some()
                            || matches!(dest.ty, ir::IrType::Named(_) | ir::IrType::Array(_, _) | ir::IrType::Tuple(_))
                                && fixed_size.is_some()
                        {
                            availability.fixed_value_vars.insert(dest.id);
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                }
                ir::IrInstruction::Length { dest, operand } if dest.ty == ir::IrType::U64 => {
                    if operand_static_length(operand).is_some() || metadata_dynamic_length_available(operand, &availability) {
                        availability.scalar_vars.insert(dest.id);
                        availability.fixed_value_vars.insert(dest.id);
                        availability.u64_value_vars.insert(dest.id);
                        availability.u64_operand_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::CollectionCapacity { dest, collection } if dest.ty == ir::IrType::U64 => {
                    if metadata_stack_collection_capacity_is_runtime_supported(collection, &availability, type_layouts) {
                        availability.scalar_vars.insert(dest.id);
                        availability.fixed_value_vars.insert(dest.id);
                        availability.u64_value_vars.insert(dest.id);
                        availability.u64_operand_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::CollectionContains { dest, collection, value } if dest.ty == ir::IrType::Bool => {
                    if metadata_stack_collection_contains_is_runtime_supported(collection, value, &availability, type_layouts) {
                        availability.scalar_vars.insert(dest.id);
                        availability.fixed_value_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::CollectionRemove { dest, collection, index } => {
                    if metadata_stack_collection_remove_is_runtime_supported(dest, collection, index, &availability, type_layouts) {
                        availability.fixed_value_vars.insert(dest.id);
                        let fixed_size = metadata_ir_type_fixed_width(&dest.ty, type_layouts);
                        if metadata_fixed_scalar_width(&dest.ty, fixed_size).is_some() {
                            availability.scalar_vars.insert(dest.id);
                            if dest.ty == ir::IrType::U64 {
                                availability.u64_value_vars.insert(dest.id);
                                availability.u64_operand_vars.insert(dest.id);
                            }
                        } else if metadata_fixed_byte_width(&dest.ty, fixed_size).is_some()
                            || matches!(dest.ty, ir::IrType::Named(_) | ir::IrType::Array(_, _) | ir::IrType::Tuple(_))
                                && fixed_size.is_some()
                        {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                }
                ir::IrInstruction::CollectionPop { dest, collection } => {
                    if metadata_stack_collection_pop_is_runtime_supported(dest, collection, &availability, type_layouts) {
                        availability.fixed_value_vars.insert(dest.id);
                        let fixed_size = metadata_ir_type_fixed_width(&dest.ty, type_layouts);
                        if metadata_fixed_scalar_width(&dest.ty, fixed_size).is_some() {
                            availability.scalar_vars.insert(dest.id);
                            if dest.ty == ir::IrType::U64 {
                                availability.u64_value_vars.insert(dest.id);
                                availability.u64_operand_vars.insert(dest.id);
                            }
                        } else if metadata_fixed_byte_width(&dest.ty, fixed_size).is_some()
                            || matches!(dest.ty, ir::IrType::Named(_) | ir::IrType::Array(_, _) | ir::IrType::Tuple(_))
                                && fixed_size.is_some()
                        {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                    if let ir::IrOperand::Var(collection) = collection {
                        availability.constructed_byte_vector_vars.remove(&collection.id);
                        availability.empty_molecule_vector_vars.remove(&collection.id);
                    }
                }
                ir::IrInstruction::Binary { dest, op, left, right }
                    if dest.ty == ir::IrType::U64 && matches!(op, ast::BinaryOp::Add | ast::BinaryOp::Sub) =>
                {
                    if metadata_u64_value_available(left, &availability) && metadata_u64_operand_available(right, &availability) {
                        availability.scalar_vars.insert(dest.id);
                        availability.u64_value_vars.insert(dest.id);
                        availability.u64_operand_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::Call { dest: Some(dest), func, args }
                    if dest.ty == ir::IrType::U64
                        && matches!(
                            func.runtime_helper(),
                            Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::EnvCurrentTimepoint)
                        )
                        && args.is_empty() =>
                {
                    availability.scalar_vars.insert(dest.id);
                    availability.fixed_value_vars.insert(dest.id);
                    availability.u64_value_vars.insert(dest.id);
                    availability.u64_operand_vars.insert(dest.id);
                }
                ir::IrInstruction::Move { dest, src } => {
                    if metadata_scalar_available(src, &availability) && metadata_fixed_scalar_size(&dest.ty).is_some() {
                        availability.scalar_vars.insert(dest.id);
                        availability.fixed_value_vars.insert(dest.id);
                    }
                    if let Some(width) = metadata_ir_type_fixed_width(&dest.ty, type_layouts)
                        && metadata_fixed_value_available_with_layout_width(src, &availability, width, type_layouts)
                    {
                        availability.fixed_value_vars.insert(dest.id);
                        if width > 8 || named_type_name(&dest.ty).is_some_and(|name| type_layouts.enum_fixed_sizes.contains_key(name))
                        {
                            availability
                                .aggregate_pointer_vars
                                .insert(dest.id, MetadataAggregatePointerSource { ty: dest.ty.clone() });
                        }
                    }
                    if dest.ty == ir::IrType::U64 && metadata_u64_value_available(src, &availability) {
                        availability.u64_value_vars.insert(dest.id);
                        if metadata_u64_operand_available(src, &availability) {
                            availability.u64_operand_vars.insert(dest.id);
                        }
                    }
                    if let ir::IrOperand::Var(src_var) = src {
                        if availability.schema_pointer_vars.contains(&src_var.id) && named_type_name(&dest.ty).is_some() {
                            availability.schema_pointer_vars.insert(dest.id);
                        }
                        // An explicit domain conversion owns storage typed as
                        // its destination. Do not overwrite that proof with
                        // the source's previous nominal type.
                        if !availability.aggregate_pointer_vars.contains_key(&dest.id)
                            && let Some(source) = availability.aggregate_pointer_vars.get(&src_var.id).cloned()
                        {
                            availability.aggregate_pointer_vars.insert(dest.id, source);
                        }
                        if availability.dynamic_collection_vars.contains(&src_var.id) && dest.ty == src_var.ty {
                            availability.dynamic_collection_vars.insert(dest.id);
                        }
                        if availability.stack_collection_vars.contains(&src_var.id) && dest.ty == src_var.ty {
                            availability.stack_collection_vars.insert(dest.id);
                        }
                        if availability.empty_molecule_vector_vars.contains(&src_var.id) && dest.ty == src_var.ty {
                            availability.empty_molecule_vector_vars.insert(dest.id);
                        }
                        if let Some(byte_count) = availability.constructed_byte_vector_vars.get(&src_var.id).copied() {
                            availability.constructed_byte_vector_vars.insert(dest.id, byte_count);
                            if let Some(root_id) = availability.constructed_byte_vector_roots.get(&src_var.id).copied() {
                                availability.constructed_byte_vector_roots.insert(dest.id, root_id);
                            }
                        }
                    }
                }
                ir::IrInstruction::Unary {
                    dest,
                    op: op @ (ast::UnaryOp::Ref | ast::UnaryOp::Deref),
                    operand: ir::IrOperand::Var(src_var),
                } => {
                    let valid_conversion = match op {
                        ast::UnaryOp::Ref => matches!(&dest.ty, ir::IrType::Ref(inner) if inner.as_ref() == &src_var.ty),
                        ast::UnaryOp::Deref => {
                            matches!(&src_var.ty, ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) if inner.as_ref() == &dest.ty)
                        }
                        _ => false,
                    };
                    if !valid_conversion {
                        continue;
                    }
                    if availability.schema_pointer_vars.contains(&src_var.id) && named_type_name(&dest.ty).is_some() {
                        availability.schema_pointer_vars.insert(dest.id);
                    }
                    if availability.dynamic_collection_vars.contains(&src_var.id) {
                        availability.dynamic_collection_vars.insert(dest.id);
                    }
                    if availability.stack_collection_vars.contains(&src_var.id) {
                        availability.stack_collection_vars.insert(dest.id);
                    }
                    if availability.empty_molecule_vector_vars.contains(&src_var.id) {
                        availability.empty_molecule_vector_vars.insert(dest.id);
                    }
                    if let Some(byte_count) = availability.constructed_byte_vector_vars.get(&src_var.id).copied() {
                        availability.constructed_byte_vector_vars.insert(dest.id, byte_count);
                        if let Some(root_id) = availability.constructed_byte_vector_roots.get(&src_var.id).copied() {
                            availability.constructed_byte_vector_roots.insert(dest.id, root_id);
                        }
                    }
                    if let Some(source) = availability.aggregate_pointer_vars.get(&src_var.id).cloned() {
                        availability.aggregate_pointer_vars.insert(dest.id, source);
                    }
                    if availability.fixed_value_vars.contains(&src_var.id) {
                        availability.fixed_value_vars.insert(dest.id);
                    }
                    if availability.scalar_vars.contains(&src_var.id) {
                        availability.scalar_vars.insert(dest.id);
                    }
                    if availability.u64_value_vars.contains(&src_var.id) && dest.ty == ir::IrType::U64 {
                        availability.u64_value_vars.insert(dest.id);
                    }
                    if availability.u64_operand_vars.contains(&src_var.id) && dest.ty == ir::IrType::U64 {
                        availability.u64_operand_vars.insert(dest.id);
                    }
                }
                ir::IrInstruction::CollectionPush { collection: ir::IrOperand::Var(collection), value } => {
                    if let Some(byte_count) = availability.constructed_byte_vector_vars.get(&collection.id).copied() {
                        if let Some(width) = metadata_constructed_vector_part_width(value, type_layouts) {
                            if metadata_fixed_value_available_with_layout_width(value, &availability, width, type_layouts) {
                                availability.constructed_byte_vector_vars.insert(collection.id, byte_count + width);
                                if let Some(name) = loaded_constructed_vector_names.get(&collection.id).cloned() {
                                    named_constructed_vectors.insert(name, collection.id);
                                }
                            } else {
                                availability.constructed_byte_vector_vars.remove(&collection.id);
                            }
                        } else {
                            availability.constructed_byte_vector_vars.remove(&collection.id);
                        }
                    }
                }
                ir::IrInstruction::CollectionExtend { collection: ir::IrOperand::Var(collection), slice } => {
                    if let Some(byte_count) = availability.constructed_byte_vector_vars.get(&collection.id).copied() {
                        if let Some(width) = metadata_constructed_vector_part_width(slice, type_layouts) {
                            if metadata_fixed_value_available_with_layout_width(slice, &availability, width, type_layouts) {
                                availability.constructed_byte_vector_vars.insert(collection.id, byte_count + width);
                                if let Some(name) = loaded_constructed_vector_names.get(&collection.id).cloned() {
                                    named_constructed_vectors.insert(name, collection.id);
                                }
                            } else {
                                availability.constructed_byte_vector_vars.remove(&collection.id);
                            }
                        } else {
                            availability.constructed_byte_vector_vars.remove(&collection.id);
                        }
                    }
                }
                ir::IrInstruction::CollectionClear { collection: ir::IrOperand::Var(collection) } => {
                    if let Some(byte_count) = availability.constructed_byte_vector_vars.get_mut(&collection.id) {
                        *byte_count = 0;
                        availability.empty_molecule_vector_vars.insert(collection.id);
                        if let Some(name) = loaded_constructed_vector_names.get(&collection.id).cloned() {
                            named_constructed_vectors.insert(name, collection.id);
                        }
                    }
                }
                ir::IrInstruction::CollectionReverse { collection: ir::IrOperand::Var(collection) } => {
                    availability.constructed_byte_vector_vars.remove(&collection.id);
                    availability.empty_molecule_vector_vars.remove(&collection.id);
                }
                ir::IrInstruction::CollectionTruncate { collection: ir::IrOperand::Var(collection), .. } => {
                    availability.constructed_byte_vector_vars.remove(&collection.id);
                    availability.empty_molecule_vector_vars.remove(&collection.id);
                }
                ir::IrInstruction::CollectionSwap { collection: ir::IrOperand::Var(collection), .. } => {
                    availability.constructed_byte_vector_vars.remove(&collection.id);
                    availability.empty_molecule_vector_vars.remove(&collection.id);
                }
                ir::IrInstruction::CollectionInsert { collection: ir::IrOperand::Var(collection), .. } => {
                    availability.constructed_byte_vector_vars.remove(&collection.id);
                    availability.empty_molecule_vector_vars.remove(&collection.id);
                }
                ir::IrInstruction::CollectionSet { collection: ir::IrOperand::Var(collection), .. } => {
                    availability.constructed_byte_vector_vars.remove(&collection.id);
                    availability.empty_molecule_vector_vars.remove(&collection.id);
                }
                _ => {}
            }
        }
    }

    availability
}

fn metadata_constructed_vector_part_width(operand: &ir::IrOperand, type_layouts: &MetadataTypeLayouts) -> Option<usize> {
    match operand {
        ir::IrOperand::Var(var) => {
            metadata_ir_type_fixed_width(&var.ty, type_layouts).or_else(|| metadata_operand_fixed_value_width(operand))
        }
        ir::IrOperand::Const(_) => metadata_operand_fixed_value_width(operand),
    }
}

pub(super) fn metadata_can_verify_create_output_fields(
    pattern: &ir::CreatePattern,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if pattern.fields.is_empty() {
        return false;
    }
    let Some(layouts) = type_layouts.get(&pattern.ty) else {
        return false;
    };
    let covered_fields = pattern.fields.iter().map(|(field, _)| field.as_str()).collect::<BTreeSet<_>>();
    if !layouts.keys().all(|field| covered_fields.contains(field.as_str())) {
        return false;
    }
    pattern.fields.iter().all(|(field, value)| {
        layouts.get(field).is_some_and(|layout| {
            if matches!(nested_values::ir_type(&layout.ty), Some(nested_values::NestedType::Values(_))) {
                // Nested values cannot be constructed or mutated. The output
                // verifier validates its complete table and compares the exact
                // bounded source span; the checker binds that comparison.
                matches!(value, ir::IrOperand::Var(var) if var.ty == layout.ty)
            } else if let Some(width) = metadata_layout_or_named_fixed_byte_width(layout, type_layouts) {
                metadata_fixed_value_available_with_layout_width(value, availability, width, type_layouts)
            } else {
                metadata_dynamic_create_output_value_available(value, layout, availability, type_layouts)
            }
        })
    })
}

fn metadata_dynamic_create_output_value_available(
    operand: &ir::IrOperand,
    layout: &MetadataFieldLayout,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    match operand {
        ir::IrOperand::Var(var) => {
            availability.schema_pointer_vars.contains(&var.id)
                || availability.constructed_byte_vector_vars.contains_key(&var.id)
                || (availability.empty_molecule_vector_vars.contains(&var.id)
                    && metadata_molecule_vector_element_fixed_width(&layout.ty, type_layouts).is_some())
        }
        ir::IrOperand::Const(_) => false,
    }
}

fn metadata_collection_new_is_verified_create_value(
    var_id: usize,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    if !availability.empty_molecule_vector_vars.contains(&var_id) && !availability.constructed_byte_vector_vars.contains_key(&var_id) {
        return false;
    }
    body.create_set.iter().any(|pattern| {
        let Some(layouts) = type_layouts.get(&pattern.ty) else {
            return false;
        };
        pattern.fields.iter().any(|(field, value)| {
            let Some(field_var) = (match value {
                ir::IrOperand::Var(var) => Some(var),
                ir::IrOperand::Const(_) => None,
            }) else {
                return false;
            };
            let directly_used = field_var.id == var_id;
            let rooted_at_new =
                availability.constructed_byte_vector_roots.get(&field_var.id).is_some_and(|root_id| *root_id == var_id);
            (directly_used || rooted_at_new)
                && layouts.get(field).is_some_and(|layout| {
                    metadata_molecule_vector_element_fixed_width(&layout.ty, type_layouts).is_some()
                        && metadata_dynamic_create_output_value_available(value, layout, availability, type_layouts)
                })
        })
    })
}

fn metadata_collection_mutation_is_verified_create_vector(
    collection: &ir::IrOperand,
    value: &ir::IrOperand,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
    availability: &MetadataPreludeAvailability,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    if !availability.constructed_byte_vector_vars.contains_key(&collection.id) {
        return false;
    }
    let root_id = availability.constructed_byte_vector_roots.get(&collection.id).copied().unwrap_or(collection.id);
    if !metadata_collection_new_is_verified_create_value(root_id, body, type_layouts, availability) {
        return false;
    }
    match value {
        ir::IrOperand::Var(var) => availability.fixed_value_vars.contains(&var.id) || availability.scalar_vars.contains(&var.id),
        ir::IrOperand::Const(_) => true,
    }
}

pub(super) fn metadata_can_verify_output_lock(pattern: &ir::CreatePattern, availability: &MetadataPreludeAvailability) -> bool {
    match &pattern.lock {
        Some(lock) => metadata_fixed_value_available_with_width(lock, availability, 32),
        None => true,
    }
}

fn metadata_can_verify_fixed_byte_comparison(
    left: &ir::IrOperand,
    right: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
) -> bool {
    let Some(width) = operand_fixed_byte_width(left) else {
        return false;
    };
    if operand_fixed_byte_width(right) != Some(width) {
        return false;
    }
    metadata_fixed_value_available_with_width(left, availability, width)
        && metadata_fixed_value_available_with_width(right, availability, width)
}

fn metadata_fixed_hash_input_available(
    operand: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let width = operand_fixed_byte_width(operand).or_else(|| match operand {
        ir::IrOperand::Var(var) => metadata_ir_type_fixed_width(&var.ty, type_layouts),
        ir::IrOperand::Const(_) => None,
    });
    width.is_some_and(|width| metadata_fixed_value_available_with_layout_width(operand, availability, width, type_layouts))
}

fn metadata_fixed_aggregate_part_available(
    operand: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    match operand {
        ir::IrOperand::Const(value) => {
            metadata_fixed_scalar_const_value(value).is_some() || metadata_fixed_byte_const_len(value).is_some()
        }
        ir::IrOperand::Var(var) => metadata_ir_type_fixed_width(&var.ty, type_layouts)
            .is_some_and(|width| metadata_fixed_value_available_with_layout_width(operand, availability, width, type_layouts)),
    }
}

pub(super) fn metadata_fixed_value_available_with_width(
    operand: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    expected_width: usize,
) -> bool {
    if let ir::IrOperand::Const(value) = operand
        && metadata_fixed_scalar_const_value(value).is_some()
    {
        return metadata_scalar_const_fits_width(value, expected_width);
    }
    if expected_width <= 8 && matches!(operand, ir::IrOperand::Var(_)) && metadata_scalar_available(operand, availability) {
        return true;
    }
    metadata_fixed_value_available(operand, availability)
        && metadata_operand_fixed_value_width(operand).is_some_and(|width| expected_width <= width)
}

fn metadata_fixed_value_available_with_layout_width(
    operand: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    expected_width: usize,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    if let ir::IrOperand::Const(value) = operand
        && metadata_fixed_scalar_const_value(value).is_some()
    {
        return metadata_scalar_const_fits_width(value, expected_width);
    }
    if expected_width <= 8 && matches!(operand, ir::IrOperand::Var(_)) && metadata_scalar_available(operand, availability) {
        return true;
    }
    match operand {
        ir::IrOperand::Const(value) => metadata_fixed_byte_const_len(value).is_some_and(|width| expected_width <= width),
        ir::IrOperand::Var(var) => {
            availability.fixed_value_vars.contains(&var.id)
                && metadata_ir_type_fixed_width(&var.ty, type_layouts)
                    .or_else(|| metadata_operand_fixed_value_width(operand))
                    .is_some_and(|width| expected_width <= width)
        }
    }
}

fn metadata_fixed_value_available(operand: &ir::IrOperand, availability: &MetadataPreludeAvailability) -> bool {
    if metadata_scalar_available(operand, availability) {
        return true;
    }
    match operand {
        ir::IrOperand::Const(value) => metadata_fixed_byte_const_len(value).is_some(),
        ir::IrOperand::Var(var) if metadata_fixed_byte_width(&var.ty, type_static_length(&var.ty)).is_some() => {
            availability.fixed_value_vars.contains(&var.id)
        }
        _ => false,
    }
}

fn metadata_scalar_const_fits_width(value: &ir::IrConst, expected_width: usize) -> bool {
    let Some(value) = metadata_fixed_scalar_const_value(value) else {
        return false;
    };
    match expected_width {
        1 => value <= u8::MAX as u64,
        2 => value <= u16::MAX as u64,
        4 => value <= u32::MAX as u64,
        8 => true,
        _ => false,
    }
}

fn metadata_operand_fixed_value_width(operand: &ir::IrOperand) -> Option<usize> {
    match operand {
        ir::IrOperand::Const(ir::IrConst::Bool(_) | ir::IrConst::U8(_)) => Some(1),
        ir::IrOperand::Const(ir::IrConst::U16(_)) => Some(2),
        ir::IrOperand::Const(ir::IrConst::U32(_)) => Some(4),
        ir::IrOperand::Const(ir::IrConst::U64(_)) => Some(8),
        ir::IrOperand::Const(value) => metadata_fixed_byte_const_len(value),
        ir::IrOperand::Var(var) => metadata_fixed_byte_width(&var.ty, type_static_length(&var.ty)),
    }
}

fn metadata_scalar_available(operand: &ir::IrOperand, _availability: &MetadataPreludeAvailability) -> bool {
    match operand {
        ir::IrOperand::Const(value) => metadata_fixed_scalar_const_value(value).is_some(),
        ir::IrOperand::Var(var) => metadata_fixed_scalar_size(&var.ty).is_some(),
    }
}

fn metadata_u64_value_available(operand: &ir::IrOperand, availability: &MetadataPreludeAvailability) -> bool {
    match operand {
        ir::IrOperand::Const(ir::IrConst::U64(_)) => true,
        ir::IrOperand::Var(var) if var.ty == ir::IrType::U64 => availability.u64_value_vars.contains(&var.id),
        _ => false,
    }
}

fn metadata_u64_operand_available(operand: &ir::IrOperand, availability: &MetadataPreludeAvailability) -> bool {
    match operand {
        ir::IrOperand::Const(ir::IrConst::U64(_)) => true,
        ir::IrOperand::Var(var) if var.ty == ir::IrType::U64 => availability.u64_operand_vars.contains(&var.id),
        _ => false,
    }
}

fn metadata_dynamic_length_available(operand: &ir::IrOperand, availability: &MetadataPreludeAvailability) -> bool {
    match operand {
        ir::IrOperand::Var(var) => {
            availability.dynamic_collection_vars.contains(&var.id) || availability.stack_collection_vars.contains(&var.id)
        }
        _ => false,
    }
}

pub(super) fn metadata_stack_collection_push_is_runtime_supported(
    collection: &ir::IrOperand,
    value: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
        && metadata_runtime_collection_part_width(value, availability, type_layouts).is_some()
}

pub(super) fn metadata_stack_collection_extend_is_runtime_supported(
    collection: &ir::IrOperand,
    slice: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    let Some(width) = (match slice {
        ir::IrOperand::Var(var) => metadata_ir_type_fixed_width(&var.ty, type_layouts),
        ir::IrOperand::Const(value) => metadata_fixed_byte_const_len(value),
    }) else {
        return false;
    };
    let element_width = metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts).unwrap_or(1);
    element_width != 0
        && width <= 256
        && width % element_width == 0
        && metadata_fixed_aggregate_part_available(slice, availability, type_layouts)
        && availability.stack_collection_vars.contains(&collection.id)
}

pub(super) fn metadata_stack_collection_clear_is_runtime_supported(
    collection: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
}

pub(super) fn metadata_stack_collection_capacity_is_runtime_supported(
    collection: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
        && metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts).is_some_and(|width| width != 0)
}

pub(super) fn metadata_stack_collection_reverse_is_runtime_supported(
    collection: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
        && metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts).is_some_and(|width| width != 0)
}

pub(super) fn metadata_stack_collection_truncate_is_runtime_supported(
    collection: &ir::IrOperand,
    len: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
        && (const_usize_operand(len).is_some() || metadata_u64_operand_available(len, availability))
}

pub(super) fn metadata_stack_collection_swap_is_runtime_supported(
    collection: &ir::IrOperand,
    left: &ir::IrOperand,
    right: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
        && metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts).is_some_and(|width| width != 0)
        && (const_usize_operand(left).is_some() || metadata_u64_operand_available(left, availability))
        && (const_usize_operand(right).is_some() || metadata_u64_operand_available(right, availability))
}

pub(super) fn metadata_stack_collection_contains_is_runtime_supported(
    collection: &ir::IrOperand,
    value: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    if !availability.stack_collection_vars.contains(&collection.id) {
        return false;
    }
    let Some(value_width) = metadata_runtime_collection_part_width(value, availability, type_layouts) else {
        return false;
    };
    let element_width = metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts).unwrap_or(value_width);
    element_width != 0 && element_width == value_width
}

pub(super) fn metadata_stack_collection_remove_is_runtime_supported(
    dest: &ir::IrVar,
    collection: &ir::IrOperand,
    index: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
        && metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts).is_some_and(|width| {
            metadata_fixed_scalar_width(&dest.ty, Some(width)).is_some()
                || metadata_ir_type_fixed_width(&dest.ty, type_layouts).is_some_and(|dest_width| dest_width == width)
        })
        && (const_usize_operand(index).is_some() || metadata_u64_operand_available(index, availability))
}

pub(super) fn metadata_stack_collection_pop_is_runtime_supported(
    dest: &ir::IrVar,
    collection: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    availability.stack_collection_vars.contains(&collection.id)
        && metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts).is_some_and(|width| {
            metadata_fixed_scalar_width(&dest.ty, Some(width)).is_some()
                || metadata_ir_type_fixed_width(&dest.ty, type_layouts).is_some_and(|dest_width| dest_width == width)
        })
}

pub(super) fn metadata_stack_collection_insert_is_runtime_supported(
    collection: &ir::IrOperand,
    index: &ir::IrOperand,
    value: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(collection) = collection else {
        return false;
    };
    if !availability.stack_collection_vars.contains(&collection.id) {
        return false;
    }
    let Some(value_width) = metadata_runtime_collection_part_width(value, availability, type_layouts) else {
        return false;
    };
    let Some(element_width) = metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts) else {
        return false;
    };
    element_width != 0
        && element_width == value_width
        && metadata_fixed_value_available_with_layout_width(value, availability, value_width, type_layouts)
        && (const_usize_operand(index).is_some() || metadata_u64_operand_available(index, availability))
}

pub(super) fn metadata_stack_collection_set_is_runtime_supported(
    collection: &ir::IrOperand,
    index: &ir::IrOperand,
    value: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    metadata_stack_collection_insert_is_runtime_supported(collection, index, value, availability, type_layouts)
}

pub(super) fn metadata_stack_collection_index_is_runtime_supported(
    dest: &ir::IrVar,
    arr: &ir::IrOperand,
    idx: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(arr) = arr else {
        return false;
    };
    availability.stack_collection_vars.contains(&arr.id)
        && metadata_molecule_vector_element_fixed_width(&arr.ty, type_layouts).is_some_and(|width| {
            metadata_fixed_scalar_width(&dest.ty, Some(width)).is_some()
                || metadata_ir_type_fixed_width(&dest.ty, type_layouts).is_some_and(|dest_width| dest_width == width)
        })
        && (const_usize_operand(idx).is_some() || metadata_u64_operand_available(idx, availability))
}

pub(super) fn metadata_runtime_collection_part_width(
    operand: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> Option<usize> {
    match operand {
        ir::IrOperand::Var(var) => metadata_fixed_scalar_width(&var.ty, type_static_length(&var.ty)).or_else(|| {
            let width = metadata_fixed_byte_width(&var.ty, type_static_length(&var.ty))
                .or_else(|| metadata_ir_type_fixed_width(&var.ty, type_layouts))?;
            availability.fixed_value_vars.contains(&var.id).then_some(width)
        }),
        ir::IrOperand::Const(ir::IrConst::Bool(_) | ir::IrConst::U8(_)) => Some(1),
        ir::IrOperand::Const(ir::IrConst::U16(_)) => Some(2),
        ir::IrOperand::Const(ir::IrConst::U32(_)) => Some(4),
        ir::IrOperand::Const(ir::IrConst::U64(_)) => Some(8),
        ir::IrOperand::Const(value) => metadata_fixed_byte_const_len(value),
    }
}

fn metadata_fixed_scalar_width(ty: &ir::IrType, fixed_size: Option<usize>) -> Option<usize> {
    match (ty, fixed_size) {
        (ir::IrType::Bool | ir::IrType::U8, Some(1)) => Some(1),
        (ir::IrType::U16, Some(2)) => Some(2),
        (ir::IrType::U32, Some(4)) => Some(4),
        (ir::IrType::I32, Some(4)) => Some(4),
        (ir::IrType::U64, Some(8)) => Some(8),
        (ir::IrType::U128, Some(16)) => Some(16),
        (ir::IrType::Named(name), Some(8)) if ir::is_ckb_temporal_scalar_name(name) => Some(8),
        _ => None,
    }
}

pub(super) fn metadata_fixed_byte_width(ty: &ir::IrType, fixed_size: Option<usize>) -> Option<usize> {
    if let Some(width) = metadata_fixed_scalar_width(ty, fixed_size) {
        return Some(width);
    }
    match (ty, fixed_size) {
        (ir::IrType::OpenHandle(handle), Some(width)) if width == handle.class.encoded_width() => Some(width),
        (ir::IrType::Address | ir::IrType::Hash, Some(32)) => Some(32),
        (ir::IrType::Array(inner, len), Some(size)) if matches!(inner.as_ref(), ir::IrType::U8) && *len == size => Some(size),
        (ir::IrType::Named(name), Some(size)) if zk_contract::fixed_width(name) == Some(size) => Some(size),
        (ir::IrType::Named(name), Some(size))
            if name == script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE
                && size == script_handle_contract::EXACT_SCRIPT_HANDLE_BYTES =>
        {
            Some(size)
        }
        (ir::IrType::Named(name), Some(size))
            if name == script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE
                && size == script_handle_contract::DEPLOYMENT_LINE_HANDLE_BYTES =>
        {
            Some(size)
        }
        (ir::IrType::Named(name), Some(32)) if ir::is_ckb_fixed_hash_domain_name(name) => Some(32),
        (ir::IrType::Commitment(_), Some(32)) => Some(32),
        (ir::IrType::Ref(inner) | ir::IrType::MutRef(inner), _) => metadata_fixed_byte_width(inner, type_static_length(inner)),
        _ => None,
    }
}

pub(super) fn metadata_layout_fixed_scalar_width(layout: &MetadataFieldLayout) -> Option<usize> {
    metadata_fixed_scalar_width(&layout.ty, layout.fixed_size).or_else(|| (layout.fixed_enum_size == Some(1)).then_some(1))
}

fn metadata_layout_fixed_byte_width(layout: &MetadataFieldLayout) -> Option<usize> {
    metadata_fixed_byte_width(&layout.ty, layout.fixed_size).or(layout.fixed_enum_size)
}

pub(super) fn metadata_layout_flow_state_width(layout: &MetadataFieldLayout) -> Option<usize> {
    metadata_layout_fixed_scalar_width(layout)
}

pub(super) fn metadata_layout_or_named_fixed_byte_width(
    layout: &MetadataFieldLayout,
    type_layouts: &MetadataTypeLayouts,
) -> Option<usize> {
    metadata_layout_fixed_byte_width(layout).or(layout.fixed_size).or_else(|| metadata_ir_type_fixed_width(&layout.ty, type_layouts))
}

pub(super) fn metadata_fixed_aggregate_pointer_size(ty: &ir::IrType) -> Option<usize> {
    match ty {
        ir::IrType::Array(_, _) | ir::IrType::Tuple(_) => type_static_length(ty).filter(|width| *width > 8),
        _ => None,
    }
}

pub(super) fn metadata_molecule_vector_element_fixed_width(ty: &ir::IrType, type_layouts: &MetadataTypeLayouts) -> Option<usize> {
    if matches!(ty, ir::IrType::Named(name) if name == "String") {
        return Some(1);
    }
    metadata_ir_type_fixed_width(ty.vector_element()?, type_layouts).filter(|width| *width > 0)
}

pub(super) fn metadata_named_type_fixed_width(ty: &str, type_layouts: &MetadataTypeLayouts) -> Option<usize> {
    // This key comes from admitted nominal IR, not a rendered application.
    // Commitment and Opening widths use their structured IR variants.
    match ty {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" => Some(4),
        "i32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "Address" | "Hash" => Some(32),
        other => type_layouts.enum_fixed_sizes.get(other).copied().or_else(|| {
            type_layouts.get(other).and_then(|fields| {
                fields.values().try_fold(0usize, |acc, layout| {
                    layout.fixed_size.or(layout.fixed_enum_size).and_then(|width| acc.checked_add(width))
                })
            })
        }),
    }
}

pub(super) fn metadata_ir_type_fixed_width(ty: &ir::IrType, type_layouts: &MetadataTypeLayouts) -> Option<usize> {
    type_static_length(ty).or_else(|| match ty {
        ir::IrType::Named(name) => metadata_named_type_fixed_width(name, type_layouts),
        ir::IrType::Array(inner, count) => metadata_ir_type_fixed_width(inner, type_layouts)?.checked_mul(*count),
        ir::IrType::Tuple(items) => {
            items.iter().try_fold(0usize, |size, item| size.checked_add(metadata_ir_type_fixed_width(item, type_layouts)?))
        }
        ir::IrType::Opening(value) => metadata_ir_type_fixed_width(value, type_layouts),
        _ => None,
    })
}
