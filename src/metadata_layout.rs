use super::schema_metadata::{ir_type_encoded_size, named_type_name};
use super::{ir, script_handle_contract, zk_contract, MetadataFieldLayout, MetadataTypeLayouts};
use std::collections::{BTreeMap, BTreeSet, HashMap};

fn metadata_aggregate_field_layout(ty: &ir::IrType, field: &str) -> Option<MetadataFieldLayout> {
    match ty {
        ir::IrType::Named(name) if name == ir::CKB_SCRIPT_HASH_TYPE && field == "0" => {
            metadata_aggregate_field_layout(&ir::IrType::Hash, field)
        }
        ir::IrType::Tuple(items) => {
            let index = field.parse::<usize>().ok()?;
            let field_ty = items.get(index)?.clone();
            let offset = items
                .iter()
                .take(index)
                .try_fold(0usize, |acc, item| type_static_length(item).and_then(|size| acc.checked_add(size)))?;
            let fixed_size = type_static_length(&field_ty);
            Some(MetadataFieldLayout { ty: field_ty, offset, fixed_size, fixed_enum_size: None })
        }
        ir::IrType::Address | ir::IrType::Hash if field == "0" => Some(MetadataFieldLayout {
            ty: ir::IrType::Array(Box::new(ir::IrType::U8), 32),
            offset: 0,
            fixed_size: Some(32),
            fixed_enum_size: None,
        }),
        _ => None,
    }
}

pub(super) fn metadata_aggregate_or_named_field_layout(
    ty: &ir::IrType,
    field: &str,
    type_layouts: &MetadataTypeLayouts,
) -> Option<MetadataFieldLayout> {
    metadata_aggregate_field_layout(ty, field).or_else(|| {
        let type_name = named_type_name(ty)?;
        type_layouts.get(type_name).and_then(|fields| fields.get(field)).cloned()
    })
}

pub(super) fn metadata_tuple_return_field_type(ty: &ir::IrType, field: &str) -> Option<ir::IrType> {
    let ir::IrType::Tuple(items) = ty else {
        return None;
    };
    let index = field.parse::<usize>().ok()?;
    (index < 8).then(|| items.get(index).cloned()).flatten()
}

pub(super) fn const_usize_operand(operand: &ir::IrOperand) -> Option<usize> {
    match operand {
        ir::IrOperand::Const(ir::IrConst::U8(value)) => Some((*value).into()),
        ir::IrOperand::Const(ir::IrConst::U16(value)) => Some((*value).into()),
        ir::IrOperand::Const(ir::IrConst::U32(value)) => Some(*value as usize),
        ir::IrOperand::Const(ir::IrConst::U64(value)) => usize::try_from(*value).ok(),
        _ => None,
    }
}

pub(super) fn metadata_fixed_byte_const_len(value: &ir::IrConst) -> Option<usize> {
    match value {
        ir::IrConst::Address(_) | ir::IrConst::Hash(_) => Some(32),
        ir::IrConst::Array(items) if items.iter().all(|item| matches!(item, ir::IrConst::U8(_))) => Some(items.len()),
        _ => None,
    }
}

pub(super) fn metadata_fixed_scalar_size(ty: &ir::IrType) -> Option<usize> {
    match ty {
        ir::IrType::Bool | ir::IrType::U8 => Some(1),
        ir::IrType::U16 => Some(2),
        ir::IrType::U32 => Some(4),
        ir::IrType::I32 => Some(4),
        ir::IrType::U64 => Some(8),
        _ => None,
    }
}

pub(super) fn metadata_fixed_scalar_const_value(value: &ir::IrConst) -> Option<u64> {
    match value {
        ir::IrConst::Bool(value) => Some(u64::from(*value)),
        ir::IrConst::U8(value) => Some(*value as u64),
        ir::IrConst::U16(value) => Some(*value as u64),
        ir::IrConst::U32(value) => Some(*value as u64),
        ir::IrConst::U64(value) => Some(*value),
        _ => None,
    }
}

pub(super) fn metadata_type_layouts(ir: &ir::IrModule) -> MetadataTypeLayouts {
    let mut layouts = HashMap::new();
    let type_defs = metadata_type_defs_by_name(ir);
    for type_def in &ir.external_type_defs {
        let fields = metadata_layout_fields(type_def, &type_defs, &ir.enum_fixed_sizes);
        layouts.insert(type_def.name.clone(), fields);
    }
    for item in &ir.items {
        let ir::IrItem::TypeDef(type_def) = item else {
            continue;
        };
        let fields = metadata_layout_fields(type_def, &type_defs, &ir.enum_fixed_sizes);
        layouts.insert(type_def.name.clone(), fields);
    }
    let fixed_struct_returns = ir
        .items
        .iter()
        .filter_map(|item| {
            let ir::IrItem::Function(function) = item else { return None };
            let ty = function.return_type.as_ref()?;
            if matches!(ty, ir::IrType::Hash | ir::IrType::Address | ir::IrType::Commitment(_))
                || matches!(ty, ir::IrType::Named(name) if ir::is_ckb_fixed_hash_domain_name(name))
                || matches!(ty, ir::IrType::Array(inner, count) if **inner == ir::IrType::U8 && (1..=64).contains(count))
            {
                return Some((function.name.clone(), ty.clone()));
            }
            let ir::IrType::Named(name) = ty else { return None };
            let definition = type_defs.get(name)?;
            if definition.kind != ir::IrTypeKind::Struct
                || ir.enum_fixed_sizes.contains_key(name)
                || layouts.get(name)?.values().any(|field| field.fixed_size.is_none())
            {
                return None;
            }
            Some((function.name.clone(), ty.clone()))
        })
        .collect();
    MetadataTypeLayouts {
        fields: layouts,
        fixed_cell_record_widths: type_defs
            .values()
            .filter(|definition| definition.kind == ir::IrTypeKind::Struct)
            .filter_map(|definition| {
                let width = crate::cell_data_view::ir_fixed_record_width(definition.fields.iter().map(|field| &field.ty))?;
                Some((definition.name.clone(), width))
            })
            .collect(),
        enum_fixed_sizes: ir.enum_fixed_sizes.clone(),
        fixed_struct_returns,
        nominal_identities: ir.nominal_identities.clone(),
        enum_payload_storage: ir
            .enum_layouts
            .iter()
            .flat_map(|(name, layout)| {
                layout.variants.iter().flat_map(move |variant| {
                    variant
                        .fields
                        .iter()
                        .map(move |field| ((name.clone(), variant.name.clone(), field.index), field.has_scalar_storage()))
                })
            })
            .collect(),
    }
}

fn metadata_layout_fields(
    type_def: &ir::IrTypeDef,
    type_defs: &BTreeMap<String, &ir::IrTypeDef>,
    enum_fixed_sizes: &HashMap<String, usize>,
) -> HashMap<String, MetadataFieldLayout> {
    let mut next_offset = Some(0usize);
    type_def
        .fields
        .iter()
        .map(|field| {
            let fixed_enum_size = match &field.ty {
                ir::IrType::Named(name) => enum_fixed_sizes.get(name).copied(),
                _ => None,
            };
            let fixed_size = ir_type_encoded_size(&field.ty, type_defs, &mut BTreeSet::new()).or(field.fixed_size).or(fixed_enum_size);
            let offset = next_offset.unwrap_or(field.offset);
            next_offset = next_offset.and_then(|current| fixed_size.and_then(|size| current.checked_add(size)));
            (field.name.clone(), MetadataFieldLayout { ty: field.ty.clone(), offset, fixed_size, fixed_enum_size })
        })
        .collect()
}

pub(super) fn metadata_flow_states(ir: &ir::IrModule) -> HashMap<String, Vec<String>> {
    let mut states = HashMap::new();
    for type_def in &ir.external_type_defs {
        if let Some(flow_states) = &type_def.flow_states {
            states.insert(type_def.name.clone(), flow_states.clone());
        }
    }
    for item in &ir.items {
        let ir::IrItem::TypeDef(type_def) = item else {
            continue;
        };
        if let Some(flow_states) = &type_def.flow_states {
            states.insert(type_def.name.clone(), flow_states.clone());
        }
    }
    states
}

pub(super) fn metadata_flow_state_fields(ir: &ir::IrModule) -> HashMap<String, String> {
    let mut fields = HashMap::new();
    for type_def in &ir.external_type_defs {
        if let Some(state_field) = &type_def.flow_state_field {
            fields.insert(type_def.name.clone(), state_field.clone());
        }
    }
    for item in &ir.items {
        let ir::IrItem::TypeDef(type_def) = item else {
            continue;
        };
        if let Some(state_field) = &type_def.flow_state_field {
            fields.insert(type_def.name.clone(), state_field.clone());
        }
    }
    fields
}

pub(super) fn metadata_cell_type_kinds(ir: &ir::IrModule) -> HashMap<String, ir::IrTypeKind> {
    let mut kinds = HashMap::new();
    for type_def in &ir.external_type_defs {
        if type_def.kind != ir::IrTypeKind::Struct {
            kinds.insert(type_def.name.clone(), type_def.kind);
        }
    }
    for item in &ir.items {
        let ir::IrItem::TypeDef(type_def) = item else {
            continue;
        };
        if type_def.kind != ir::IrTypeKind::Struct {
            kinds.insert(type_def.name.clone(), type_def.kind);
        }
    }
    kinds
}

pub(super) fn metadata_type_defs_by_name(ir: &ir::IrModule) -> BTreeMap<String, &ir::IrTypeDef> {
    let mut type_defs = BTreeMap::new();
    for type_def in &ir.external_type_defs {
        type_defs.insert(type_def.name.clone(), type_def);
    }
    for item in &ir.items {
        let ir::IrItem::TypeDef(type_def) = item else {
            continue;
        };
        type_defs.insert(type_def.name.clone(), type_def);
    }
    type_defs
}

#[derive(Debug, Default)]
pub(super) struct ValidityPathCoverage {
    selected_create: BTreeMap<String, usize>,
    checked_create: BTreeMap<String, usize>,
    updates: BTreeMap<String, usize>,
}

impl ValidityPathCoverage {
    pub(super) fn selected_create_count(&self, type_name: &str) -> usize {
        self.selected_create.get(type_name).copied().unwrap_or(0)
    }

    pub(super) fn checked_create_count(&self, type_name: &str) -> usize {
        self.checked_create.get(type_name).copied().unwrap_or(0)
    }

    pub(super) fn update_count(&self, type_name: &str) -> usize {
        self.updates.get(type_name).copied().unwrap_or(0)
    }
}

fn increment_validity_path(paths: &mut BTreeMap<String, usize>, type_name: &str) {
    *paths.entry(type_name.to_string()).or_default() += 1;
}

pub(super) fn module_validity_path_coverage(ir: &ir::IrModule) -> ValidityPathCoverage {
    let mut coverage = ValidityPathCoverage::default();
    for item in &ir.items {
        let (body, params) = match item {
            ir::IrItem::Action(action) => (&action.body, action.params.as_slice()),
            ir::IrItem::Function(function) => (&function.body, function.params.as_slice()),
            ir::IrItem::Lock(lock) => (&lock.body, lock.params.as_slice()),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => continue,
        };
        let mut update_types = body.mutate_set.iter().map(|pattern| pattern.ty.clone()).collect::<BTreeSet<_>>();
        let consumed_bindings = body.consume_set.iter().map(|pattern| pattern.binding.as_str()).collect::<BTreeSet<_>>();
        let consumed_type_names = params
            .iter()
            .filter(|param| consumed_bindings.contains(param.name.as_str()))
            .filter_map(|param| match &param.ty {
                ir::IrType::Named(name) => Some(name.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        update_types.extend(
            body.create_set.iter().filter(|pattern| consumed_type_names.contains(&pattern.ty)).map(|pattern| pattern.ty.clone()),
        );
        let input_types = params
            .iter()
            .filter(|param| param.source == crate::ast::ParamSource::Input)
            .filter_map(|param| match &param.ty {
                ir::IrType::Named(name) => Some(name.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        update_types.extend(params.iter().filter(|param| param.source == crate::ast::ParamSource::Output).filter_map(|param| {
            match &param.ty {
                ir::IrType::Named(name) if input_types.contains(name) => Some(name.clone()),
                _ => None,
            }
        }));
        for type_name in update_types {
            increment_validity_path(&mut coverage.updates, &type_name);
        }
        for pattern in &body.create_set {
            increment_validity_path(&mut coverage.selected_create, &pattern.ty);
        }
        for block in &body.blocks {
            for instruction in &block.instructions {
                match instruction {
                    ir::IrInstruction::Create { pattern, .. }
                    | ir::IrInstruction::CreateUnique { pattern, .. }
                    | ir::IrInstruction::ReplaceUnique { pattern, .. } => {
                        increment_validity_path(&mut coverage.checked_create, &pattern.ty);
                    }
                    ir::IrInstruction::Claim { dest, .. } | ir::IrInstruction::Settle { dest, .. } => {
                        if let ir::IrType::Named(name) = &dest.ty {
                            increment_validity_path(&mut coverage.checked_create, name);
                        }
                    }
                    ir::IrInstruction::Tuple { dest, .. } => {
                        if let ir::IrType::Named(name) = &dest.ty {
                            increment_validity_path(&mut coverage.selected_create, name);
                            increment_validity_path(&mut coverage.checked_create, name);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    coverage
}

pub(super) fn ir_operand_contains_cell_backed_value(
    operand: &ir::IrOperand,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
) -> bool {
    match operand {
        ir::IrOperand::Var(var) => ir_type_contains_cell_backed_value(&var.ty, cell_type_kinds),
        ir::IrOperand::Const(_) => false,
    }
}

pub(super) fn ir_operand_is_cell_backed_collection(
    operand: &ir::IrOperand,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
) -> bool {
    match operand {
        ir::IrOperand::Var(var) => ir_type_is_cell_backed_collection(&var.ty, cell_type_kinds),
        ir::IrOperand::Const(_) => false,
    }
}

pub(super) fn ir_operand_cell_backed_type_names(
    operand: &ir::IrOperand,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
) -> Vec<String> {
    match operand {
        ir::IrOperand::Var(var) => ir_type_cell_backed_value_type_names(&var.ty, cell_type_kinds),
        ir::IrOperand::Const(_) => Vec::new(),
    }
}

pub(super) fn ir_operand_cell_backed_collection_type_names(
    operand: &ir::IrOperand,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
) -> Vec<String> {
    match operand {
        ir::IrOperand::Var(var) => ir_type_cell_backed_collection_type_names(&var.ty, cell_type_kinds),
        ir::IrOperand::Const(_) => Vec::new(),
    }
}

fn ir_type_contains_cell_backed_value(ty: &ir::IrType, cell_type_kinds: &HashMap<String, ir::IrTypeKind>) -> bool {
    match ty {
        ir::IrType::Bounded(collection) => ir_type_contains_cell_backed_value(&collection.element_type, cell_type_kinds),
        ir::IrType::Vector(Some(element)) => ir_type_contains_cell_backed_value(element, cell_type_kinds),
        ir::IrType::Array(inner, _) => ir_type_contains_cell_backed_value(inner, cell_type_kinds),
        ir::IrType::Tuple(items) => items.iter().any(|item| ir_type_contains_cell_backed_value(item, cell_type_kinds)),
        ir::IrType::Named(name) => cell_type_kinds.contains_key(name),
        ir::IrType::Ref(_) | ir::IrType::MutRef(_) => false,
        _ => false,
    }
}

fn ir_type_cell_backed_value_type_names(ty: &ir::IrType, cell_type_kinds: &HashMap<String, ir::IrTypeKind>) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_ir_type_cell_backed_value_type_names(ty, cell_type_kinds, &mut names);
    names.into_iter().collect()
}

fn collect_ir_type_cell_backed_value_type_names(
    ty: &ir::IrType,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    names: &mut BTreeSet<String>,
) {
    match ty {
        ir::IrType::Bounded(collection) => {
            collect_ir_type_cell_backed_value_type_names(&collection.element_type, cell_type_kinds, names)
        }
        ir::IrType::Vector(Some(element)) => collect_ir_type_cell_backed_value_type_names(element, cell_type_kinds, names),
        ir::IrType::Array(inner, _) => collect_ir_type_cell_backed_value_type_names(inner, cell_type_kinds, names),
        ir::IrType::Tuple(items) => {
            for item in items {
                collect_ir_type_cell_backed_value_type_names(item, cell_type_kinds, names);
            }
        }
        ir::IrType::Named(name) => {
            if cell_type_kinds.contains_key(name) {
                names.insert(name.clone());
            }
        }
        ir::IrType::Ref(_) | ir::IrType::MutRef(_) => {}
        _ => {}
    }
}

pub(super) fn ir_type_is_cell_backed_collection(ty: &ir::IrType, cell_type_kinds: &HashMap<String, ir::IrTypeKind>) -> bool {
    match ty {
        ir::IrType::Vector(Some(element)) => ir_type_contains_cell_backed_value(element, cell_type_kinds),
        ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => ir_type_is_cell_backed_collection(inner, cell_type_kinds),
        _ => false,
    }
}

pub(super) fn ir_type_cell_backed_collection_type_names(
    ty: &ir::IrType,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
) -> Vec<String> {
    let mut names = BTreeSet::new();
    collect_ir_type_cell_backed_collection_type_names(ty, cell_type_kinds, &mut names);
    names.into_iter().collect()
}

fn collect_ir_type_cell_backed_collection_type_names(
    ty: &ir::IrType,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    names: &mut BTreeSet<String>,
) {
    match ty {
        ir::IrType::Vector(Some(element)) => collect_ir_type_cell_backed_value_type_names(element, cell_type_kinds, names),
        ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => {
            collect_ir_type_cell_backed_collection_type_names(inner, cell_type_kinds, names);
        }
        _ => {}
    }
}

pub(super) fn operand_static_length(operand: &ir::IrOperand) -> Option<usize> {
    match operand {
        ir::IrOperand::Var(var) => type_static_length(&var.ty),
        ir::IrOperand::Const(ir::IrConst::Array(items)) => Some(items.len()),
        ir::IrOperand::Const(ir::IrConst::Address(_) | ir::IrConst::Hash(_)) => Some(32),
        _ => None,
    }
}

pub(super) fn operand_fixed_byte_width(operand: &ir::IrOperand) -> Option<usize> {
    match operand {
        ir::IrOperand::Const(ir::IrConst::Address(_) | ir::IrConst::Hash(_)) => Some(32),
        ir::IrOperand::Const(ir::IrConst::Array(items)) => Some(items.len()),
        ir::IrOperand::Var(var) => {
            let mut ty = &var.ty;
            while let ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) = ty {
                ty = inner;
            }
            match ty {
                ir::IrType::OpenHandle(handle) => Some(handle.class.encoded_width()),
                ir::IrType::Address | ir::IrType::Hash => Some(32),
                ir::IrType::Array(inner, len) if matches!(inner.as_ref(), ir::IrType::U8) => Some(*len),
                ir::IrType::Named(name) if zk_contract::fixed_width(name).is_some() => zk_contract::fixed_width(name),
                ir::IrType::Named(name) if name == script_handle_contract::EXACT_SCRIPT_HANDLE_TYPE => {
                    Some(script_handle_contract::EXACT_SCRIPT_HANDLE_BYTES)
                }
                ir::IrType::Named(name) if name == script_handle_contract::DEPLOYMENT_LINE_HANDLE_TYPE => {
                    Some(script_handle_contract::DEPLOYMENT_LINE_HANDLE_BYTES)
                }
                ir::IrType::Named(name) if ir::is_ckb_fixed_hash_domain_name(name) => Some(32),
                ir::IrType::Commitment(_) => Some(32),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn type_static_length(ty: &ir::IrType) -> Option<usize> {
    crate::ir::fixed_layout_width(ty)
}
