use super::metadata_layout::type_static_length;
use super::runtime_availability::{
    metadata_fixed_aggregate_pointer_size, metadata_fixed_byte_width, metadata_ir_type_fixed_width,
    metadata_layout_fixed_scalar_width, metadata_layout_or_named_fixed_byte_width, metadata_molecule_vector_element_fixed_width,
};
use super::schema_metadata::{ckb_type_id_metadata, ir_type_to_string, metadata_identity_policy, named_type_name};
use super::verifier_obligations::metadata_type_encoded_size_from_layouts;
use super::{
    ast, hex_encode, hex_hash, ir, script_handle_contract, zk_contract, CellPatternMetadata, CkbOutputDataBindingMetadata,
    CkbTypeIdOutputMetadata, CreatePatternMetadata, MetadataTypeLayouts, MutatePatternMetadata, ParamMetadata, TargetProfile,
    CKB_TYPE_ID_ABI, CKB_TYPE_ID_ARGS_SOURCE, CKB_TYPE_ID_BUILDER, CKB_TYPE_ID_CODE_HASH, CKB_TYPE_ID_GENERATOR_SETTING,
    CKB_TYPE_ID_HASH_TYPE, CKB_TYPE_ID_OUTPUT_SOURCE, CKB_TYPE_ID_WASM_SETTING, METADATA_MUTATE_CELL_BUFFER_SIZE,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub(super) fn param_metadata_for_body(
    params: &[ir::IrParam],
    body: &ir::IrBody,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    enum_layouts: &HashMap<String, ir::IrEnumLayout>,
) -> Vec<ParamMetadata> {
    let type_hash_param_ids = param_type_hash_param_ids(body);
    params
        .iter()
        .map(|param| {
            let bounded_runtime_contract = body
                .bounded_collection_ops
                .iter()
                .find(|operation| operation.collection_binding == param.name)
                .and_then(|operation| operation.runtime_contract.clone());
            let mut metadata = param_metadata(param, &type_hash_param_ids, cell_type_kinds, enum_layouts, bounded_runtime_contract);
            // Physical binding is authoritative even for read-only plain
            // structs, which are not Cell-backed by their declaration kind.
            metadata.cell_bound_abi |= body.cell_binding_for_local(param.binding.id).is_some();
            metadata
        })
        .collect()
}

fn param_type_hash_param_ids(body: &ir::IrBody) -> BTreeSet<usize> {
    let mut ids = BTreeSet::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            if let ir::IrInstruction::TypeHash { operand: ir::IrOperand::Var(var), .. } = instruction
                && named_type_name(&var.ty).is_some()
            {
                ids.insert(var.id);
            }
        }
    }
    ids
}

fn param_metadata(
    param: &ir::IrParam,
    type_hash_param_ids: &BTreeSet<usize>,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    enum_layouts: &HashMap<String, ir::IrEnumLayout>,
    bounded_runtime_contract: Option<String>,
) -> ParamMetadata {
    let named_type = named_type_name(&param.ty);
    let enum_fixed_len =
        named_type.and_then(|name| enum_layouts.get(name)).filter(|layout| layout.has_payload()).map(|layout| layout.encoded_size);
    let schema_pointer_abi = param.ty.has_structured_span_abi()
        || (named_type.is_some_and(|name| {
            !ir::is_ckb_temporal_scalar_name(name)
                && !ir::is_ckb_fixed_hash_domain_name(name)
                && zk_contract::fixed_width(name).is_none()
                && name != script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE
                && name != script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE
        }) && enum_fixed_len.is_none());
    let fixed_byte_len = enum_fixed_len.or_else(|| {
        metadata_fixed_byte_width(&param.ty, type_static_length(&param.ty))
            .filter(|width| *width > 8)
            .or_else(|| metadata_fixed_aggregate_pointer_size(&param.ty))
    });
    let type_hash_abi = schema_pointer_abi && type_hash_param_ids.contains(&param.binding.id);
    let cell_bound_abi = param.is_ref
        || bounded_runtime_contract.as_deref() == Some(ir::BOUNDED_TYPE_GROUP_INPUTS_V1)
        || matches!(
            named_type_name(&param.ty).and_then(|name| cell_type_kinds.get(name)),
            Some(ir::IrTypeKind::Resource | ir::IrTypeKind::Shared | ir::IrTypeKind::Receipt)
        );
    ParamMetadata {
        name: param.name.clone(),
        ty: ir_type_to_string(&param.ty),
        is_mut: param.is_mut,
        is_ref: param.is_ref,
        source: if param.is_read_ref { "read" } else { param_source_metadata(param.source) }.to_string(),
        protected_spend_surface: param.source == ast::ParamSource::Protected,
        witness_data_source: param.source == ast::ParamSource::Witness,
        lock_args_data_source: param.source == ast::ParamSource::LockArgs,
        cell_bound_abi,
        schema_pointer_abi,
        schema_length_abi: schema_pointer_abi,
        fixed_byte_pointer_abi: fixed_byte_len.is_some(),
        fixed_byte_length_abi: fixed_byte_len.is_some(),
        fixed_byte_len,
        type_hash_pointer_abi: type_hash_abi,
        type_hash_length_abi: type_hash_abi,
        type_hash_len: type_hash_abi.then_some(32),
        bounded_runtime_contract,
    }
}

pub(super) fn param_source_metadata(source: ast::ParamSource) -> &'static str {
    match source {
        ast::ParamSource::Default => "default",
        ast::ParamSource::Input => "input",
        ast::ParamSource::Output => "output",
        ast::ParamSource::Protected => "protected",
        ast::ParamSource::Witness => "witness",
        ast::ParamSource::LockArgs => "lock_args",
    }
}

pub(super) fn cell_pattern_metadata(pattern: &ir::CellPattern) -> CellPatternMetadata {
    CellPatternMetadata {
        operation: pattern.operation.clone(),
        type_hash: pattern.type_hash.as_ref().map(hex_hash),
        binding: pattern.binding.clone(),
        fields: pattern.fields.iter().map(|(field, _)| field.clone()).collect(),
    }
}

pub(super) fn create_pattern_metadata(
    pattern: &ir::CreatePattern,
    output_index: usize,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    target_profile: TargetProfile,
) -> CreatePatternMetadata {
    CreatePatternMetadata {
        operation: pattern.operation.clone(),
        ty: pattern.ty.clone(),
        binding: pattern.binding.clone(),
        fields: pattern.fields.iter().map(|(field, _)| field.clone()).collect(),
        has_lock: pattern.lock.is_some(),
        identity_policy: metadata_identity_policy(&pattern.identity),
        ckb_output_data: ckb_output_data_binding_metadata(pattern, output_index, target_profile),
        ckb_type_id: ckb_type_id_output_metadata(pattern, output_index, type_defs, target_profile),
    }
}

fn ckb_output_data_binding_metadata(
    pattern: &ir::CreatePattern,
    output_index: usize,
    target_profile: TargetProfile,
) -> Option<CkbOutputDataBindingMetadata> {
    if !target_profile.is_ckb()
        || !matches!(
            pattern.operation.as_str(),
            "create" | "output" | "create_unique" | "replace_unique" | "transfer" | "claim" | "settle"
        )
    {
        return None;
    }

    Some(CkbOutputDataBindingMetadata {
        abi: "ckb-outputs-and-outputs-data-index-aligned".to_string(),
        output_source: "Output".to_string(),
        output_index,
        output_data_source: "outputs_data".to_string(),
        output_data_index: output_index,
        relation: "same-index".to_string(),
    })
}

fn ckb_type_id_output_metadata(
    pattern: &ir::CreatePattern,
    output_index: usize,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    target_profile: TargetProfile,
) -> Option<CkbTypeIdOutputMetadata> {
    if !target_profile.is_ckb() || !matches!(pattern.operation.as_str(), "create" | "output" | "create_unique") {
        return None;
    }
    if pattern.operation == "create_unique" && !matches!(pattern.identity, ir::IrIdentityPolicy::CkbTypeId) {
        return None;
    }
    let type_def = type_defs.get(&pattern.ty)?;
    ckb_type_id_metadata(type_def, target_profile)?;
    let type_id = type_def.type_id.clone()?;

    Some(CkbTypeIdOutputMetadata {
        abi: CKB_TYPE_ID_ABI.to_string(),
        type_id,
        output_source: CKB_TYPE_ID_OUTPUT_SOURCE.to_string(),
        output_index,
        script_code_hash: hex_encode(&CKB_TYPE_ID_CODE_HASH),
        hash_type: CKB_TYPE_ID_HASH_TYPE.to_string(),
        args_source: CKB_TYPE_ID_ARGS_SOURCE.to_string(),
        builder: CKB_TYPE_ID_BUILDER.to_string(),
        generator_setting: CKB_TYPE_ID_GENERATOR_SETTING.to_string(),
        wasm_setting: CKB_TYPE_ID_WASM_SETTING.to_string(),
    })
}

pub(super) fn mutate_pattern_metadata(pattern: &ir::MutatePattern, type_layouts: &MetadataTypeLayouts) -> MutatePatternMetadata {
    MutatePatternMetadata {
        operation: pattern.operation.clone(),
        ty: pattern.ty.clone(),
        binding: pattern.binding.clone(),
        fields: pattern.fields.clone(),
        preserved_fields: pattern.preserved_fields.clone(),
        input_source: "Input".to_string(),
        input_index: pattern.input_index,
        output_source: "Output".to_string(),
        output_index: pattern.output_index,
        preserve_type_hash: pattern.preserve_type_hash,
        preserve_lock_hash: pattern.preserve_lock_hash,
        type_hash_preservation_status: "checked-runtime".to_string(),
        lock_hash_preservation_status: "checked-runtime".to_string(),
        field_equality_status: mutate_field_equality_status(pattern, type_layouts).to_string(),
        field_transition_status: mutate_field_transition_status(pattern, type_layouts).to_string(),
    }
}

pub(super) fn mutate_field_equality_status(pattern: &ir::MutatePattern, type_layouts: &MetadataTypeLayouts) -> &'static str {
    if pattern.preserved_fields.is_empty() {
        return "checked-runtime";
    }
    let checked = pattern
        .preserved_fields
        .iter()
        .filter(|field| mutate_preserved_field_is_verifier_coverable(pattern, field, type_layouts))
        .count();
    if checked == pattern.preserved_fields.len()
        || mutate_preserved_data_except_transition_is_verifier_coverable(pattern, type_layouts)
    {
        "checked-runtime"
    } else if checked > 0 {
        "checked-partial"
    } else {
        "runtime-required"
    }
}

pub(super) fn mutate_preserved_field_is_verifier_coverable(
    pattern: &ir::MutatePattern,
    field: &str,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let Some(fields) = type_layouts.get(&pattern.ty) else {
        return false;
    };
    let Some(layout) = fields.get(field) else {
        return false;
    };
    if metadata_type_encoded_size_from_layouts(fields).is_none() {
        return true;
    }
    let Some(width) = metadata_layout_or_named_fixed_byte_width(layout, type_layouts) else {
        return false;
    };
    layout.offset + width <= METADATA_MUTATE_CELL_BUFFER_SIZE
}

fn mutate_preserved_data_except_transition_is_verifier_coverable(
    pattern: &ir::MutatePattern,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    if pattern.preserved_fields.is_empty() || pattern.transitions.len() != pattern.fields.len() || pattern.transitions.is_empty() {
        return false;
    }
    let Some(fields) = type_layouts.get(&pattern.ty) else {
        return false;
    };
    let Some(type_size) = metadata_type_encoded_size_from_layouts(fields) else {
        return false;
    };
    if type_size > METADATA_MUTATE_CELL_BUFFER_SIZE {
        return false;
    }
    pattern.transitions.iter().all(|transition| {
        fields.get(&transition.field).is_some_and(|layout| {
            metadata_layout_or_named_fixed_byte_width(layout, type_layouts)
                .map(|width| layout.offset + width)
                .is_some_and(|end| end <= METADATA_MUTATE_CELL_BUFFER_SIZE)
        })
    })
}

pub(super) fn mutate_field_transition_status(pattern: &ir::MutatePattern, type_layouts: &MetadataTypeLayouts) -> &'static str {
    if pattern.fields.is_empty() {
        return "checked-runtime";
    }
    let checked = pattern
        .transitions
        .iter()
        .filter(|transition| mutate_transition_is_verifier_coverable(pattern, transition, type_layouts))
        .count();
    if checked == pattern.fields.len() && pattern.transitions.len() == pattern.fields.len() {
        "checked-runtime"
    } else if checked > 0 {
        "checked-partial"
    } else {
        "runtime-required"
    }
}

fn mutate_transition_is_verifier_coverable(
    pattern: &ir::MutatePattern,
    transition: &ir::MutateFieldTransition,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let Some(fields) = type_layouts.get(&pattern.ty) else {
        return false;
    };
    let Some(layout) = fields.get(&transition.field) else {
        return false;
    };
    let dynamic_table = metadata_type_encoded_size_from_layouts(fields).is_none();
    if transition.op == ir::MutateTransitionOp::Append {
        if !dynamic_table {
            return false;
        }
        let Some(element_width) = metadata_molecule_vector_element_fixed_width(&layout.ty, type_layouts) else {
            return false;
        };
        return match &transition.operand {
            ir::IrOperand::Var(var) => metadata_ir_type_fixed_width(&var.ty, type_layouts) == Some(element_width),
            ir::IrOperand::Const(_) => false,
        };
    }
    if transition.op == ir::MutateTransitionOp::Set {
        let Some(width) = metadata_layout_or_named_fixed_byte_width(layout, type_layouts) else {
            return false;
        };
        if !dynamic_table && layout.offset + width > METADATA_MUTATE_CELL_BUFFER_SIZE {
            return false;
        }
        return match &transition.operand {
            ir::IrOperand::Const(ir::IrConst::U64(_))
            | ir::IrOperand::Const(ir::IrConst::Address(_))
            | ir::IrOperand::Const(ir::IrConst::Hash(_))
            | ir::IrOperand::Const(ir::IrConst::Array(_)) => true,
            ir::IrOperand::Var(var) => metadata_fixed_byte_width(&var.ty, type_static_length(&var.ty)).is_some(),
            _ => false,
        };
    }
    // u128 fields are verifier-coverable via 128-bit add/sub with carry.
    if dynamic_table {
        let Some(width) = metadata_layout_fixed_scalar_width(layout) else {
            return false;
        };
        if width > 8 {
            return false;
        }
        return match &transition.operand {
            ir::IrOperand::Const(ir::IrConst::U64(_)) => true,
            ir::IrOperand::Var(var) => {
                matches!(var.ty, ir::IrType::U8 | ir::IrType::U16 | ir::IrType::U32 | ir::IrType::I32 | ir::IrType::U64)
            }
            _ => false,
        };
    }
    // u128 fields are verifier-coverable via 128-bit add/sub with carry.
    if layout.ty == ir::IrType::U128 && layout.fixed_size == Some(16) {
        if layout.offset + 16 > METADATA_MUTATE_CELL_BUFFER_SIZE {
            return false;
        }
        // The delta must be u64 (fits in a single register for the carry path).
        return match &transition.operand {
            ir::IrOperand::Const(ir::IrConst::U64(_)) => true,
            ir::IrOperand::Var(var) => {
                matches!(var.ty, ir::IrType::U8 | ir::IrType::U16 | ir::IrType::U32 | ir::IrType::I32 | ir::IrType::U64)
            }
            _ => false,
        };
    }
    // Standard path: fields that fit in a single 64-bit register (≤8 bytes).
    let Some(width) = metadata_layout_fixed_scalar_width(layout) else {
        return false;
    };
    if width > 8 {
        return false;
    }
    if layout.offset + width > METADATA_MUTATE_CELL_BUFFER_SIZE {
        return false;
    }
    match &transition.operand {
        ir::IrOperand::Const(ir::IrConst::U64(_)) => true,
        ir::IrOperand::Var(var) => {
            matches!(var.ty, ir::IrType::U8 | ir::IrType::U16 | ir::IrType::U32 | ir::IrType::I32 | ir::IrType::U64)
        }
        _ => false,
    }
}

pub(super) fn metadata_collection_push_is_verified_append(
    collection: &ir::IrOperand,
    value: &ir::IrOperand,
    body: &ir::IrBody,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let collection_ty = match collection {
        ir::IrOperand::Var(var) => &var.ty,
        ir::IrOperand::Const(_) => return false,
    };
    let Some(element_width) = metadata_molecule_vector_element_fixed_width(collection_ty, type_layouts) else {
        return false;
    };
    let pushed_var_id = match value {
        ir::IrOperand::Var(var) if metadata_ir_type_fixed_width(&var.ty, type_layouts) == Some(element_width) => var.id,
        _ => return false,
    };
    body.mutate_set.iter().any(|pattern| {
        pattern.transitions.iter().any(|transition| {
            transition.op == ir::MutateTransitionOp::Append
                && matches!(&transition.operand, ir::IrOperand::Var(var) if var.id == pushed_var_id)
                && mutate_transition_is_verifier_coverable(pattern, transition, type_layouts)
        })
    })
}
