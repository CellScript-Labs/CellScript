use super::metadata_abi_binding::param_source_metadata;
use super::proof_plan::EvidenceTier;
use super::runtime_availability::{
    metadata_ir_type_fixed_width, metadata_molecule_vector_element_fixed_width, metadata_named_type_fixed_width,
    metadata_prelude_availability, metadata_runtime_collection_part_width, metadata_stack_collection_capacity_is_runtime_supported,
    metadata_stack_collection_clear_is_runtime_supported, metadata_stack_collection_contains_is_runtime_supported,
    metadata_stack_collection_extend_is_runtime_supported, metadata_stack_collection_index_is_runtime_supported,
    metadata_stack_collection_insert_is_runtime_supported, metadata_stack_collection_pop_is_runtime_supported,
    metadata_stack_collection_push_is_runtime_supported, metadata_stack_collection_remove_is_runtime_supported,
    metadata_stack_collection_reverse_is_runtime_supported, metadata_stack_collection_set_is_runtime_supported,
    metadata_stack_collection_swap_is_runtime_supported, metadata_stack_collection_truncate_is_runtime_supported,
    MetadataPreludeAvailability,
};
use super::schema_metadata::{ir_type_to_string, named_type_name};
use super::{
    hex_encode, ir, BoundedOutputFieldBindingMetadata, BoundedOutputPlanContractMetadata, BoundedOutputValueBindingMetadata,
    CollectionInstantiationMetadata, MetadataTypeLayouts, STACK_COLLECTION_BACKING_BYTES,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Default)]
struct CollectionInstantiationBuilder {
    scope_kind: String,
    scope_name: String,
    collection_ty: String,
    element_ty: String,
    element_width_bytes: usize,
    helpers: BTreeSet<String>,
}

impl CollectionInstantiationBuilder {
    fn into_metadata(self) -> CollectionInstantiationMetadata {
        let max_elements = STACK_COLLECTION_BACKING_BYTES.checked_div(self.element_width_bytes).unwrap_or(0);
        CollectionInstantiationMetadata {
            scope_kind: self.scope_kind,
            scope_name: self.scope_name,
            collection_ty: self.collection_ty,
            element_ty: self.element_ty,
            element_width_bytes: self.element_width_bytes,
            backing: format!("stack-fixed-buffer:{STACK_COLLECTION_BACKING_BYTES}"),
            max_elements,
            status: "checked-runtime".to_string(),
            source: "local-stack".to_string(),
            ownership: "owned-local-values".to_string(),
            actual_scanned_cardinality: None,
            vacuous_possible: false,
            output_cardinality_max: None,
            capacity_builder_evidence_required: false,
            evidence_tier: EvidenceTier::CheckedRuntime,
            helpers: self.helpers.into_iter().collect(),
            bounded_output_plan: None,
        }
    }
}

pub(super) fn module_collection_instantiation_metadata(
    ir: &ir::IrModule,
    type_layouts: &MetadataTypeLayouts,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<CollectionInstantiationMetadata> {
    let mut instantiations = Vec::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => instantiations.extend(body_collection_instantiation_metadata(
                "action",
                &action.name,
                &action.body,
                &action.params,
                type_layouts,
                pure_const_returns,
            )),
            ir::IrItem::Function(function) => instantiations.extend(body_collection_instantiation_metadata(
                "function",
                &function.name,
                &function.body,
                &function.params,
                type_layouts,
                pure_const_returns,
            )),
            ir::IrItem::Lock(lock) => instantiations.extend(body_collection_instantiation_metadata(
                "lock",
                &lock.name,
                &lock.body,
                &lock.params,
                type_layouts,
                pure_const_returns,
            )),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => {}
        }
    }
    instantiations
}

fn body_collection_instantiation_metadata(
    scope_kind: &str,
    scope_name: &str,
    body: &ir::IrBody,
    params: &[ir::IrParam],
    type_layouts: &MetadataTypeLayouts,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<CollectionInstantiationMetadata> {
    let mut bounded = body
        .bounded_collection_ops
        .iter()
        .map(|op| {
            let checked_runtime = op.runtime_contract.is_some();
            let bounded_output_plan = bounded_output_plan_contract_metadata(op, type_layouts);
            CollectionInstantiationMetadata {
                scope_kind: scope_kind.to_string(),
                scope_name: scope_name.to_string(),
                collection_ty: op.collection_type.source_text(),
                element_ty: op.element_type.source_text(),
                element_width_bytes: metadata_ir_type_fixed_width(&op.element_type, type_layouts).unwrap_or(0),
                backing: format!("bounded-source:{}", param_source_metadata(op.source)),
                max_elements: op.max_elements,
                status: if checked_runtime {
                    "checked-runtime".to_string()
                } else if op.operation == "create_each" {
                    "builder-evidence-required".to_string()
                } else {
                    "runtime-helper-required".to_string()
                },
                source: param_source_metadata(op.source).to_string(),
                ownership: if op.operation == "consume_each" {
                    "linear-cell-set".to_string()
                } else {
                    "bounded-output-plan".to_string()
                },
                actual_scanned_cardinality: None,
                vacuous_possible: true,
                output_cardinality_max: (op.operation == "create_each").then_some(op.max_elements),
                capacity_builder_evidence_required: op.operation == "create_each" && !checked_runtime,
                evidence_tier: if checked_runtime {
                    EvidenceTier::CheckedRuntime
                } else if op.operation == "create_each" {
                    EvidenceTier::BuilderEvidenceRequired
                } else {
                    EvidenceTier::RuntimeHelperRequired
                },
                helpers: op.runtime_contract.iter().cloned().chain(std::iter::once(op.operation.clone())).collect(),
                bounded_output_plan,
            }
        })
        .collect::<Vec<_>>();
    let param_schema_vars = params
        .iter()
        .filter(|param| named_type_name(&param.ty).is_some() || param.ty.has_structured_span_abi())
        .map(|param| param.binding.id)
        .collect::<BTreeSet<_>>();
    let availability = metadata_prelude_availability(body, &param_schema_vars, type_layouts, params, pure_const_returns);
    let constructor_helpers = collection_constructor_helpers(body);
    let mut builders = BTreeMap::<String, CollectionInstantiationBuilder>::new();

    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::Index { dest, arr, idx }
                    if metadata_stack_collection_index_is_runtime_supported(dest, arr, idx, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        arr,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "index",
                    );
                }
                ir::IrInstruction::Length { operand, .. } if metadata_operand_is_stack_collection(operand, &availability) => {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        operand,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "len",
                    );
                }
                ir::IrInstruction::CollectionCapacity { collection, .. }
                    if metadata_stack_collection_capacity_is_runtime_supported(collection, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "capacity",
                    );
                }
                ir::IrInstruction::CollectionPush { collection, value }
                    if metadata_stack_collection_push_is_runtime_supported(collection, value, &availability, type_layouts) =>
                {
                    record_stack_collection_push_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        value,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                    );
                }
                ir::IrInstruction::CollectionExtend { collection, slice }
                    if metadata_stack_collection_extend_is_runtime_supported(collection, slice, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "extend_from_slice",
                    );
                }
                ir::IrInstruction::CollectionClear { collection }
                    if metadata_stack_collection_clear_is_runtime_supported(collection, &availability) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "clear",
                    );
                }
                ir::IrInstruction::CollectionReverse { collection }
                    if metadata_stack_collection_reverse_is_runtime_supported(collection, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "reverse",
                    );
                }
                ir::IrInstruction::CollectionTruncate { collection, len }
                    if metadata_stack_collection_truncate_is_runtime_supported(collection, len, &availability) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "truncate",
                    );
                }
                ir::IrInstruction::CollectionSwap { collection, left, right }
                    if metadata_stack_collection_swap_is_runtime_supported(collection, left, right, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "swap",
                    );
                }
                ir::IrInstruction::CollectionContains { collection, value, .. }
                    if metadata_stack_collection_contains_is_runtime_supported(collection, value, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "contains",
                    );
                }
                ir::IrInstruction::CollectionRemove { dest, collection, index }
                    if metadata_stack_collection_remove_is_runtime_supported(dest, collection, index, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "remove",
                    );
                }
                ir::IrInstruction::CollectionPop { dest, collection }
                    if metadata_stack_collection_pop_is_runtime_supported(dest, collection, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "pop",
                    );
                }
                ir::IrInstruction::CollectionInsert { collection, index, value }
                    if metadata_stack_collection_insert_is_runtime_supported(
                        collection,
                        index,
                        value,
                        &availability,
                        type_layouts,
                    ) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "insert",
                    );
                }
                ir::IrInstruction::CollectionSet { collection, index, value }
                    if metadata_stack_collection_set_is_runtime_supported(collection, index, value, &availability, type_layouts) =>
                {
                    record_stack_collection_helper(
                        &mut builders,
                        scope_kind,
                        scope_name,
                        collection,
                        &availability,
                        &constructor_helpers,
                        type_layouts,
                        "set",
                    );
                }
                _ => {}
            }
        }
    }

    bounded.extend(builders.into_values().map(CollectionInstantiationBuilder::into_metadata));
    bounded
}

fn bounded_output_plan_contract_metadata(
    operation: &ir::IrBoundedCollectionOp,
    type_layouts: &MetadataTypeLayouts,
) -> Option<BoundedOutputPlanContractMetadata> {
    if operation.runtime_contract.as_deref() != Some(ir::BOUNDED_OUTPUT_PLAN_V1) {
        return None;
    }
    let template = operation.create_template.as_ref()?;
    let output_type = operation.output_type.as_deref()?;
    let ir::IrType::Named(element_name) = &operation.element_type else { return None };
    let plan_layouts = type_layouts.get(element_name)?;
    let output_layouts = type_layouts.get(output_type)?;
    let mut field_bindings = Vec::with_capacity(template.fields.len());
    for (output_field, rendered) in &template.fields {
        let plan_field = rendered.strip_prefix(&operation.binding)?.strip_prefix('.')?;
        if plan_field.is_empty() || plan_field.contains('.') {
            return None;
        }
        let output_layout = output_layouts.get(output_field)?;
        let plan_layout = plan_layouts.get(plan_field)?;
        let output_width = output_layout.fixed_size.or(output_layout.fixed_enum_size)?;
        let plan_width = plan_layout.fixed_size.or(plan_layout.fixed_enum_size)?;
        if output_layout.ty != plan_layout.ty || output_width != plan_width {
            return None;
        }
        field_bindings.push(BoundedOutputFieldBindingMetadata {
            output_field: output_field.clone(),
            output_offset_bytes: output_layout.offset,
            plan_field: plan_field.to_string(),
            plan_offset_bytes: plan_layout.offset,
            width_bytes: output_width,
            ty: ir_type_to_string(&output_layout.ty),
        });
    }
    field_bindings.sort_by_key(|binding| binding.output_offset_bytes);
    let lock_rendered = template.lock.as_deref()?;
    let lock_field = lock_rendered.strip_prefix(&operation.binding)?.strip_prefix('.')?;
    if lock_field.is_empty() || lock_field.contains('.') {
        return None;
    }
    let lock_layout = plan_layouts.get(lock_field)?;
    let lock_width = lock_layout.fixed_size.or(lock_layout.fixed_enum_size)?;
    if lock_width != 32 {
        return None;
    }
    let element_width_bytes = operation.element_width?;
    let output_width_bytes = metadata_named_type_fixed_width(output_type, type_layouts)?;
    Some(BoundedOutputPlanContractMetadata {
        schema: "cellscript-bounded-output-plan-contract".to_string(),
        version: 1,
        binding: operation.collection_binding.clone(),
        codec: "molecule-fixvec-fixed-item-v1".to_string(),
        codec_magic_hex: format!("0x{}", hex_encode(ir::BOUNDED_OUTPUT_PLAN_MAGIC_V1)),
        placement: "entry-witness-args-v1-param-payload".to_string(),
        allow_zero: true,
        element_ty: operation.element_type.source_text(),
        element_width_bytes,
        max_elements: operation.max_elements,
        output_source: "ckb-group-output".to_string(),
        ordering: "plan-index-equals-group-output-index".to_string(),
        correspondence: "exactly-one-group-output-per-plan-element".to_string(),
        output_ty: output_type.to_string(),
        output_width_bytes,
        type_script_policy: "exact-current-script-hash".to_string(),
        lock_policy: "exact-plan-field-hash".to_string(),
        capacity_floor_shannons: operation.output_capacity_floor_shannons?,
        identity_policy: "fresh-output-outpoint-plus-type-group-ordinal".to_string(),
        field_bindings,
        lock_binding: BoundedOutputValueBindingMetadata {
            plan_field: lock_field.to_string(),
            plan_offset_bytes: lock_layout.offset,
            width_bytes: lock_width,
            ty: ir_type_to_string(&lock_layout.ty),
        },
    })
}

fn metadata_operand_is_stack_collection(operand: &ir::IrOperand, availability: &MetadataPreludeAvailability) -> bool {
    matches!(operand, ir::IrOperand::Var(var) if availability.stack_collection_vars.contains(&var.id))
}

fn collection_constructor_helpers(body: &ir::IrBody) -> HashMap<usize, &'static str> {
    let mut constructors = HashMap::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            if let ir::IrInstruction::CollectionNew { dest, capacity, .. } = instruction {
                let helper = if capacity.is_some() { "with_capacity" } else { "new" };
                constructors.insert(dest.id, helper);
            }
        }
    }
    constructors
}

fn stack_collection_constructor_helper(
    collection: &ir::IrVar,
    availability: &MetadataPreludeAvailability,
    constructor_helpers: &HashMap<usize, &'static str>,
) -> &'static str {
    availability
        .constructed_byte_vector_roots
        .get(&collection.id)
        .and_then(|root_id| constructor_helpers.get(root_id).copied())
        .unwrap_or("new")
}

fn record_stack_collection_helper(
    builders: &mut BTreeMap<String, CollectionInstantiationBuilder>,
    scope_kind: &str,
    scope_name: &str,
    collection: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    constructor_helpers: &HashMap<usize, &'static str>,
    type_layouts: &MetadataTypeLayouts,
    helper: &str,
) {
    let ir::IrOperand::Var(collection) = collection else {
        return;
    };
    let collection_ty = ir_type_to_string(&collection.ty);
    let Some(element_ty) = metadata_vec_element_type_name(&collection.ty) else {
        return;
    };
    let Some(element_width_bytes) = metadata_molecule_vector_element_fixed_width(&collection.ty, type_layouts) else {
        return;
    };
    let constructor_helper = stack_collection_constructor_helper(collection, availability, constructor_helpers);
    let key = format!("{scope_kind}:{scope_name}:{collection_ty}");
    let builder = builders.entry(key).or_insert_with(|| {
        let mut helpers = BTreeSet::new();
        helpers.insert(constructor_helper.to_string());
        CollectionInstantiationBuilder {
            scope_kind: scope_kind.to_string(),
            scope_name: scope_name.to_string(),
            collection_ty: collection_ty.clone(),
            element_ty: element_ty.to_string(),
            element_width_bytes,
            helpers,
        }
    });
    builder.helpers.insert(helper.to_string());
}

fn record_stack_collection_push_helper(
    builders: &mut BTreeMap<String, CollectionInstantiationBuilder>,
    scope_kind: &str,
    scope_name: &str,
    collection: &ir::IrOperand,
    value: &ir::IrOperand,
    availability: &MetadataPreludeAvailability,
    constructor_helpers: &HashMap<usize, &'static str>,
    type_layouts: &MetadataTypeLayouts,
) {
    let ir::IrOperand::Var(collection) = collection else {
        return;
    };
    if metadata_vec_element_type_name(&collection.ty).is_some() {
        record_stack_collection_helper(
            builders,
            scope_kind,
            scope_name,
            &ir::IrOperand::Var(collection.clone()),
            availability,
            constructor_helpers,
            type_layouts,
            "push",
        );
        return;
    }

    let Some(element_width_bytes) = metadata_runtime_collection_part_width(value, availability, type_layouts) else {
        return;
    };
    let Some(element_ty) = metadata_operand_type_name(value) else {
        return;
    };
    let constructor_helper = stack_collection_constructor_helper(collection, availability, constructor_helpers);
    let collection_ty = format!("Vec<{element_ty}>");
    let key = format!("{scope_kind}:{scope_name}:{collection_ty}");
    let builder = builders.entry(key).or_insert_with(|| {
        let mut helpers = BTreeSet::new();
        helpers.insert(constructor_helper.to_string());
        CollectionInstantiationBuilder {
            scope_kind: scope_kind.to_string(),
            scope_name: scope_name.to_string(),
            collection_ty: collection_ty.clone(),
            element_ty: element_ty.to_string(),
            element_width_bytes,
            helpers,
        }
    });
    builder.helpers.insert("push".to_string());
}

fn metadata_operand_type_name(operand: &ir::IrOperand) -> Option<String> {
    match operand {
        ir::IrOperand::Var(var) => Some(ir_type_to_string(&var.ty)),
        ir::IrOperand::Const(ir::IrConst::Bool(_)) => Some("bool".to_string()),
        ir::IrOperand::Const(ir::IrConst::U8(_)) => Some("u8".to_string()),
        ir::IrOperand::Const(ir::IrConst::U16(_)) => Some("u16".to_string()),
        ir::IrOperand::Const(ir::IrConst::U32(_)) => Some("u32".to_string()),
        ir::IrOperand::Const(ir::IrConst::U64(_)) => Some("u64".to_string()),
        ir::IrOperand::Const(ir::IrConst::Address(_)) => Some("Address".to_string()),
        ir::IrOperand::Const(ir::IrConst::Hash(_)) => Some("Hash".to_string()),
        ir::IrOperand::Const(ir::IrConst::Array(_) | ir::IrConst::U128(_) | ir::IrConst::Unit) => None,
    }
}

fn metadata_vec_element_type_name(ty: &ir::IrType) -> Option<String> {
    ty.vector_element().map(ir::IrType::source_text)
}
