use super::capability_metadata::RuntimeViewProvenance;
use super::ckb_runtime_accesses::body_ckb_runtime_accesses;
use super::metadata_layout::{metadata_aggregate_or_named_field_layout, metadata_tuple_return_field_type};
use super::proof_plan::EvidenceTier;
use super::runtime_availability::{
    body_fail_closed_runtime_features, metadata_layout_or_named_fixed_byte_width, metadata_molecule_vector_element_fixed_width,
    MetadataPreludeAvailability,
};
use super::schema_metadata::named_type_name;
use super::verifier_obligations::operand_named_type_name;
use super::{
    aggregate_lowering, ast, cell_data_view, cell_selection, ir, ir_const_u64, nested_values, runtime_access_metadata,
    runtime_range_bounded_value, runtime_range_not_applicable, runtime_range_whole_value, runtime_scalar_dynamic,
    runtime_scalar_from_operand, runtime_scalar_metadata_ordinal, runtime_scalar_not_applicable, BorrowRegionMetadata,
    CkbRuntimeAccessMetadata, CkbRuntimeAccessProvenanceMetadata, CkbRuntimeSourceProvenanceMetadata, CkbScriptGroupMetadata,
    CkbSigningMessageDomainMetadata, MetadataTypeLayouts, TargetProfile, TransactionViewHandleMetadata,
    CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT, CKB_SIGHASH_ALL_ZERO_LOCK_CONTRACT,
};
use std::collections::{BTreeSet, HashMap};

pub(super) fn module_fail_closed_runtime_features(
    ir: &ir::IrModule,
    type_layouts: &MetadataTypeLayouts,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    pure_const_returns: &HashMap<String, ir::IrConst>,
) -> Vec<String> {
    let mut features = BTreeSet::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                let param_schema_vars = schema_pointer_var_ids(&action.body, &action.params);
                features.extend(body_fail_closed_runtime_features(
                    &action.body,
                    &param_schema_vars,
                    type_layouts,
                    &action.params,
                    cell_type_kinds,
                    action.return_type.as_ref(),
                    pure_const_returns,
                ));
            }
            ir::IrItem::Function(function) => {
                let param_schema_vars = schema_pointer_var_ids(&function.body, &function.params);
                features.extend(body_fail_closed_runtime_features(
                    &function.body,
                    &param_schema_vars,
                    type_layouts,
                    &function.params,
                    cell_type_kinds,
                    function.return_type.as_ref(),
                    pure_const_returns,
                ));
            }
            ir::IrItem::Lock(lock) => {
                let param_schema_vars = schema_pointer_var_ids(&lock.body, &lock.params);
                features.extend(body_fail_closed_runtime_features(
                    &lock.body,
                    &param_schema_vars,
                    type_layouts,
                    &lock.params,
                    cell_type_kinds,
                    None,
                    pure_const_returns,
                ));
            }
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => {}
        }
    }
    features.into_iter().collect()
}

pub(super) fn module_ckb_runtime_accesses(
    ir: &ir::IrModule,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    type_layouts: &MetadataTypeLayouts,
) -> Vec<CkbRuntimeAccessMetadata> {
    let mut accesses = Vec::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                accesses.extend(auto_lowered_aggregate_runtime_accesses_for_action(ir, action));
                accesses.extend(merge_ckb_runtime_accesses(
                    body_ckb_runtime_accesses(&action.name, &action.body, cell_type_kinds, type_layouts),
                    &action.params,
                ));
            }
            ir::IrItem::Function(function) => accesses.extend(merge_ckb_runtime_accesses(
                body_ckb_runtime_accesses(&function.name, &function.body, cell_type_kinds, type_layouts),
                &function.params,
            )),
            ir::IrItem::Lock(lock) => accesses.extend(merge_ckb_runtime_accesses(
                body_ckb_runtime_accesses(&lock.name, &lock.body, cell_type_kinds, type_layouts),
                &lock.params,
            )),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => {}
        }
    }
    accesses
}

pub(super) fn ckb_signing_message_domains(ir: &ir::IrModule) -> Vec<CkbSigningMessageDomainMetadata> {
    let mut domains = Vec::new();
    for item in &ir.items {
        let (scope_kind, scope_name, body) = match item {
            ir::IrItem::Action(action) => ("action", action.name.as_str(), &action.body),
            ir::IrItem::Function(function) => ("function", function.name.as_str(), &function.body),
            ir::IrItem::Lock(lock) => ("lock", lock.name.as_str(), &lock.body),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => continue,
        };
        for block in &body.blocks {
            for instruction in &block.instructions {
                let ir::IrInstruction::Call { func, args, .. } = instruction else {
                    continue;
                };
                if func.runtime_helper() != Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSighashAllZeroLock)
                    || args.len() != 4
                {
                    continue;
                }
                let limits = args
                    .iter()
                    .map(|operand| match operand {
                        ir::IrOperand::Const(value) => ir_const_u64(value),
                        ir::IrOperand::Var(_) => None,
                    })
                    .collect::<Option<Vec<_>>>();
                let Some(limits) = limits else {
                    continue;
                };
                domains.push(CkbSigningMessageDomainMetadata {
                    contract: CKB_SIGHASH_ALL_ZERO_LOCK_CONTRACT.to_string(),
                    scope_kind: scope_kind.to_string(),
                    scope_name: scope_name.to_string(),
                    binding: "env::sighash_all_zero_lock".to_string(),
                    digest_type: "SighashAllDigest".to_string(),
                    hash_algorithm: "ckb-default-hash-blake2b-256".to_string(),
                    transaction_hash_source: "LOAD_TX_HASH-exact-32".to_string(),
                    group_scope: "current-input-script-group".to_string(),
                    first_witness_source: "GroupInput[0]-WitnessArgs".to_string(),
                    first_witness_lock_transform: "replace-entire-lock-payload-with-equal-length-zero-bytes".to_string(),
                    witness_length_prefix: "u64-little-endian-byte-length".to_string(),
                    later_group_witness_order: "GroupInput[1..]-script-group-order-if-present".to_string(),
                    extra_witness_source: "Input[input_count..witness_count]-transaction-order".to_string(),
                    max_group_inputs: limits[0],
                    max_inputs: limits[1],
                    max_extra_witnesses: limits[2],
                    max_witness_bytes: limits[3],
                    runtime_helper: "__ckb_sighash_all_zero_lock".to_string(),
                    evidence_tier: EvidenceTier::CheckedRuntime,
                });
            }
        }
    }
    domains
}

pub(super) fn transaction_view_handle_metadata(ir: &ir::IrModule) -> Vec<TransactionViewHandleMetadata> {
    let mut handles = Vec::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                handles.extend(body_transaction_view_handles("action", &action.name, &action.body));
            }
            ir::IrItem::Function(function) => {
                handles.extend(body_transaction_view_handles("function", &function.name, &function.body));
            }
            ir::IrItem::Lock(lock) => {
                handles.extend(body_transaction_view_handles("lock", &lock.name, &lock.body));
            }
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => {}
        }
    }
    handles.sort_by(|left, right| {
        (left.scope_kind.as_str(), left.scope_name.as_str(), left.binding.as_str(), left.handle_type.as_str(), left.source.as_str())
            .cmp(&(
                right.scope_kind.as_str(),
                right.scope_name.as_str(),
                right.binding.as_str(),
                right.handle_type.as_str(),
                right.source.as_str(),
            ))
    });
    handles.dedup();
    handles
}

pub(super) fn borrow_region_metadata(ir: &ir::IrModule) -> Vec<BorrowRegionMetadata> {
    let mut regions = Vec::new();
    for item in &ir.items {
        let (scope_kind, scope_name, body) = match item {
            ir::IrItem::Action(action) => ("action", action.name.as_str(), &action.body),
            ir::IrItem::Function(function) => ("function", function.name.as_str(), &function.body),
            ir::IrItem::Lock(lock) => ("lock", lock.name.as_str(), &lock.body),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => continue,
        };
        regions.extend(body.borrow_regions.iter().map(|region| BorrowRegionMetadata {
            scope_kind: scope_kind.to_string(),
            scope_name: scope_name.to_string(),
            root: region.root.clone(),
            path: region.path.clone(),
            binding: region.binding.clone(),
            view_type: format!("View<{}>", region.view_type),
            storage: "none".to_string(),
            abi: "none".to_string(),
            evidence_tier: EvidenceTier::CheckedStatic,
            source_span: crate::proof_plan::ProofPlanSourceSpanMetadata {
                start: region.span.start,
                end: region.span.end,
                line: region.span.line,
                column: region.span.column,
            },
        }));
    }
    regions
}

pub(super) fn runtime_view_provenance_by_var(body: &ir::IrBody) -> HashMap<usize, RuntimeViewProvenance> {
    let mut values = HashMap::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(src), field }
                    if field == "data" && cell_data_view::ir_schema(&dest.ty).is_some() =>
                {
                    // Helper parameters retain their encoded source/index at
                    // runtime. Their nominal InputView/OutputView type cannot
                    // distinguish global from group coordinates; do not invent
                    // a concrete source when the caller is outside this body.
                    let provenance = values.get(&src.id).cloned().unwrap_or_else(|| RuntimeViewProvenance {
                        resolved_source: "SourceView".to_string(),
                        index: runtime_scalar_dynamic(format!("{}.index", src.name), Some(u64::from(u32::MAX))),
                    });
                    values.insert(dest.id, provenance);
                }
                ir::IrInstruction::ReadExactCellRole { dest, role, read, .. } if read.indexed() => {
                    let input = *read == cell_selection::Read::Input;
                    values.insert(
                        dest.id,
                        RuntimeViewProvenance {
                            resolved_source: if input { "Input" } else { "Output" }.into(),
                            // An ordinal selects a record; it is not the resulting
                            // global index. Preserve that distinction for builders.
                            // Preserve the existing source-view ABI bound. The
                            // stricter scan bound remains in the typed catalog.
                            index: runtime_scalar_dynamic(format!("{}.role{role}.global_index", dest.name), Some(u64::from(u32::MAX))),
                        },
                    );
                }
                ir::IrInstruction::Call { dest: Some(dest), func, args } => {
                    let Some(source) = runtime_source_for_constructor(func) else {
                        continue;
                    };
                    values.insert(
                        dest.id,
                        RuntimeViewProvenance {
                            resolved_source: source.to_string(),
                            index: runtime_scalar_from_operand(args.first(), Some(u64::from(u32::MAX))),
                        },
                    );
                }
                ir::IrInstruction::Move { dest, src: ir::IrOperand::Var(src) } => {
                    if let Some(provenance) = values.get(&src.id).cloned() {
                        values.insert(dest.id, provenance);
                    } else if transaction_view_handle_type(&src.ty).is_some() && transaction_view_handle_type(&dest.ty).is_some() {
                        // A ScriptRef/OutPoint projection from a helper's view
                        // parameter copies its encoded source/index. Its caller
                        // may supply a global or group coordinate.
                        values.insert(
                            dest.id,
                            RuntimeViewProvenance {
                                resolved_source: "SourceView".to_string(),
                                index: runtime_scalar_dynamic(format!("{}.index", src.name), Some(u64::from(u32::MAX))),
                            },
                        );
                    }
                }
                _ => {}
            }
        }
    }
    values
}

pub(super) fn runtime_source_for_constructor(target: &ir::IrCallTarget) -> Option<&'static str> {
    let func = target.runtime_helper()?.symbol();
    match func {
        "__ckb_source_input" => Some("Input"),
        "__ckb_source_output" => Some("Output"),
        "__ckb_source_cell_dep" => Some("CellDep"),
        "__ckb_source_header_dep" => Some("HeaderDep"),
        "__ckb_source_group_input" => Some("GroupInput"),
        "__ckb_source_group_output" => Some("GroupOutput"),
        _ => None,
    }
}

fn body_transaction_view_handles(scope_kind: &str, scope_name: &str, body: &ir::IrBody) -> Vec<TransactionViewHandleMetadata> {
    let mut handles = Vec::new();
    let source_values = runtime_view_provenance_by_var(body);
    let mut sources = HashMap::<usize, String>::new();
    for block in &body.blocks {
        for instruction in &block.instructions {
            let (dest, explicit_source, inherited_source, runtime_source, source_origin, source_binding) = match instruction {
                ir::IrInstruction::FieldAccess { dest, obj: ir::IrOperand::Var(src), field }
                    if field == "data" && cell_data_view::ir_schema(&dest.ty).is_some() =>
                {
                    (
                        dest,
                        None,
                        Some(sources.get(&src.id).cloned().unwrap_or_else(|| "SourceView".to_string())),
                        None,
                        "inherited-source-view",
                        Some(src.name.clone()),
                    )
                }
                ir::IrInstruction::ReadExactCellRole { dest, selection: ir::IrOperand::Var(selection), read, .. }
                    if read.indexed() =>
                {
                    (
                        dest,
                        Some(if *read == cell_selection::Read::Input { "Input" } else { "Output" }.to_string()),
                        None,
                        None,
                        "inherited-source-view",
                        Some(selection.name.clone()),
                    )
                }
                ir::IrInstruction::Call { dest: Some(dest), func, args } => {
                    let inherited = args.first().and_then(|operand| match operand {
                        ir::IrOperand::Var(var) => sources.get(&var.id).cloned(),
                        ir::IrOperand::Const(_) => None,
                    });
                    (
                        dest,
                        transaction_view_source_for_constructor(func, &dest.ty),
                        inherited,
                        runtime_source_for_constructor(func).map(str::to_string),
                        "explicit-source-view",
                        None,
                    )
                }
                ir::IrInstruction::Move { dest, src: ir::IrOperand::Var(src) } => (
                    dest,
                    None,
                    sources
                        .get(&src.id)
                        .cloned()
                        .or_else(|| source_values.get(&dest.id).map(|provenance| provenance.resolved_source.clone())),
                    None,
                    "inherited-source-view",
                    Some(src.name.clone()),
                ),
                _ => continue,
            };
            let source = explicit_source.or(inherited_source).or(runtime_source).unwrap_or_else(|| "Derived".to_string());
            sources.insert(dest.id, source.clone());
            let Some(handle_type) = transaction_view_handle_type(&dest.ty) else {
                continue;
            };
            let runtime_provenance = source_values.get(&dest.id);
            let resolved_source =
                runtime_provenance.map(|provenance| provenance.resolved_source.clone()).unwrap_or_else(|| source.clone());
            let index = runtime_provenance.map(|provenance| provenance.index.clone()).unwrap_or_else(runtime_scalar_not_applicable);
            let witness_view = ir::witness_bytes_view_parts(&dest.ty);
            handles.push(TransactionViewHandleMetadata {
                scope_kind: scope_kind.to_string(),
                scope_name: scope_name.to_string(),
                binding: dest.name.clone(),
                handle_type,
                source,
                provenance: CkbRuntimeAccessProvenanceMetadata {
                    contract: CKB_RUNTIME_ACCESS_PROVENANCE_CONTRACT.to_string(),
                    source: CkbRuntimeSourceProvenanceMetadata {
                        resolved_source,
                        origin: source_origin.to_string(),
                        binding: source_binding,
                    },
                    index,
                    range: witness_view
                        .map(|(_, _, maximum)| runtime_range_bounded_value(format!("{}.size", dest.name), maximum))
                        .unwrap_or_else(runtime_range_not_applicable),
                },
                ownership: "read-only-view".to_string(),
                witness_owner: witness_view.map(|(owner, _, _)| owner.to_string()),
                max_bytes: witness_view.map(|(_, _, maximum)| maximum),
                lifecycle_authority: false,
                typing_evidence_tier: EvidenceTier::CheckedStatic,
                read_evidence_tier: EvidenceTier::CheckedRuntime,
            });
        }
    }
    handles
}

fn transaction_view_handle_type(ty: &ir::IrType) -> Option<String> {
    if matches!(ty, ir::IrType::Context(_) | ir::IrType::WitnessBytes(_)) {
        return Some(ty.source_text());
    }
    let ir::IrType::Named(name) = ty else {
        return None;
    };
    match name.as_str() {
        "__ckb_input_out_point_ref" => Some("OutPoint".to_string()),
        "__ckb_lock_script_ref" | "__ckb_type_script_ref" => Some("ScriptView".to_string()),
        _ => None,
    }
}

fn transaction_view_source_for_constructor(target: &ir::IrCallTarget, ty: &ir::IrType) -> Option<String> {
    let func = target.runtime_helper()?.symbol();
    let base = ty.context_constructor()?;
    let source = match (func, base) {
        ("__ckb_source_input", "InputView") => "Input",
        ("__ckb_source_group_input", "InputView") => "GroupInput",
        ("__ckb_source_output", "OutputView") => "Output",
        ("__ckb_source_group_output", "OutputView") => "GroupOutput",
        ("__ckb_source_cell_dep", "CellDepView") => "CellDep",
        ("__ckb_source_header_dep", "HeaderDepView") => "HeaderDep",
        ("__ckb_source_input", "WitnessArgsView") => "WitnessArgs/Input",
        _ => return None,
    };
    Some(source.to_string())
}

pub(super) fn module_ckb_runtime_features(
    ir: &ir::IrModule,
    cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    type_layouts: &MetadataTypeLayouts,
) -> Vec<String> {
    let mut features = BTreeSet::new();
    for item in &ir.items {
        match item {
            ir::IrItem::Action(action) => {
                features.extend(auto_lowered_aggregate_runtime_features_for_action(ir, action));
                features.extend(merge_ckb_runtime_features(
                    body_ckb_runtime_features(&action.name, &action.body, cell_type_kinds, type_layouts),
                    &action.params,
                ));
            }
            ir::IrItem::Function(function) => features.extend(merge_ckb_runtime_features(
                body_ckb_runtime_features(&function.name, &function.body, cell_type_kinds, type_layouts),
                &function.params,
            )),
            ir::IrItem::Lock(lock) => features.extend(merge_ckb_runtime_features(
                body_ckb_runtime_features(&lock.name, &lock.body, cell_type_kinds, type_layouts),
                &lock.params,
            )),
            ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) => {}
        }
    }
    features.into_iter().collect()
}

pub(super) fn auto_lowered_aggregate_runtime_features_for_action(ir: &ir::IrModule, action: &ir::IrAction) -> BTreeSet<String> {
    let mut features = BTreeSet::new();
    for helper in auto_lowered_aggregate_runtime_helpers_for_action(ir, action) {
        if helper == "xudt::require_group_amount_conserved" {
            features.insert("ckb-xudt-layout-binding".to_string());
            features.insert("ckb-xudt-group-amount-conservation".to_string());
        }
    }
    features
}

pub(super) fn auto_lowered_aggregate_runtime_accesses_for_action(
    ir: &ir::IrModule,
    action: &ir::IrAction,
) -> Vec<CkbRuntimeAccessMetadata> {
    auto_lowered_aggregate_runtime_helpers_for_action(ir, action)
        .into_iter()
        .enumerate()
        .map(|(index, helper)| {
            let operation = match helper.as_str() {
                "xudt::require_group_amount_conserved" => "xudt-group-amount-conservation".to_string(),
                _ => "auto-lowered-aggregate".to_string(),
            };
            runtime_access_metadata(
                operation,
                "LOAD_CELL_DATA",
                "GroupInput/GroupOutput",
                helper,
                "GroupInput/GroupOutput",
                "metadata-summary",
                None,
                runtime_scalar_metadata_ordinal(index as u64),
                runtime_range_whole_value(),
            )
        })
        .collect()
}

fn auto_lowered_aggregate_runtime_helpers_for_action(ir: &ir::IrModule, action: &ir::IrAction) -> BTreeSet<String> {
    ir.items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::Invariant(invariant) => Some(invariant),
            _ => None,
        })
        .flat_map(|invariant| {
            invariant
                .aggregates
                .iter()
                .filter_map(|aggregate| auto_lowered_aggregate_runtime_helper_for_action(invariant, aggregate, action))
        })
        .map(str::to_string)
        .collect()
}

pub(super) fn auto_lowered_aggregate_runtime_helper_for_action(
    invariant: &ir::IrInvariant,
    aggregate: &ir::IrAggregateInvariant,
    action: &ir::IrAction,
) -> Option<&'static str> {
    let type_name = aggregate_lowering::xudt_group_amount_conservation_type(invariant, aggregate)?;
    if aggregate_lowering::body_contains_runtime_helper(&action.body, aggregate_lowering::XUDT_GROUP_AMOUNT_CONSERVED_CODEGEN_HELPER) {
        return None;
    }
    aggregate_lowering::action_has_group_amount_conservation_evidence(action, type_name)
        .then_some(aggregate_lowering::XUDT_GROUP_AMOUNT_CONSERVED_METADATA_HELPER)
}

pub(super) fn body_ckb_runtime_features(
    _name: &str,
    body: &ir::IrBody,
    _cell_type_kinds: &HashMap<String, ir::IrTypeKind>,
    _type_layouts: &MetadataTypeLayouts,
) -> Vec<String> {
    let mut features = BTreeSet::new();
    if !body.consume_set.is_empty() {
        features.insert("consume-input-cell".to_string());
    }
    if !body.read_refs.is_empty() {
        features.insert("read-cell-dep".to_string());
    }
    if !body.create_set.is_empty() {
        features.insert("verify-output-cell".to_string());
    }
    if body.create_set.iter().any(|pattern| matches!(pattern.identity, ir::IrIdentityPolicy::Field(_))) {
        features.insert("verify-unique-identity-field".to_string());
    }
    if body.create_set.iter().any(|pattern| matches!(pattern.identity, ir::IrIdentityPolicy::ScriptArgs)) {
        features.insert("load-identity-lock-hash".to_string());
    }
    if body
        .create_set
        .iter()
        .any(|pattern| matches!(pattern.identity, ir::IrIdentityPolicy::CkbTypeId | ir::IrIdentityPolicy::SingletonType))
    {
        features.insert("load-identity-type-hash".to_string());
    }
    if !body.mutate_set.is_empty() {
        features.insert("mutate-input-cell".to_string());
        features.insert("verify-mutate-output-cell".to_string());
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            match instruction {
                ir::IrInstruction::BoundedCellLoad { .. } => {
                    features.insert("ckb-bounded-type-group-inputs-v1".to_string());
                    features.insert("ckb-source-view".to_string());
                    features.insert("ckb-cell-data-decode".to_string());
                    features.insert("ckb-script-identity-requirements".to_string());
                }
                ir::IrInstruction::VerifyExactCellRoles { .. } => {
                    features.insert("ckb-bounded-exact-role-selection-v1".to_string());
                    features.insert("ckb-source-view".to_string());
                    features.insert("ckb-cell-data-decode".to_string());
                    features.insert("ckb-script-identity-requirements".to_string());
                }
                ir::IrInstruction::BoundedPlanLoad { .. } => {
                    features.insert("ckb-bounded-output-plan-v1".to_string());
                    features.insert("ckb-witness-args-input-type".to_string());
                    features.insert("ckb-molecule-fixvec-decode".to_string());
                }
                ir::IrInstruction::BoundedOutputVerify { .. } | ir::IrInstruction::BoundedOutputEnd { .. } => {
                    features.insert("ckb-bounded-output-plan-v1".to_string());
                    features.insert("ckb-source-view".to_string());
                    features.insert("ckb-cell-data-decode".to_string());
                    features.insert("ckb-script-identity-requirements".to_string());
                    features.insert("ckb-output-capacity-floor".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::EnvCurrentTimepoint) =>
                {
                    features.insert("load-header-timepoint".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderEpochNumber | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderDepEpochNumber)) =>
                {
                    features.insert("ckb-header-epoch-number".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderEpochStartBlockNumber | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderDepEpochStartBlockNumber)) =>
                {
                    features.insert("ckb-header-epoch-start-block-number".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderEpochLength | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderDepEpochLength)) =>
                {
                    features.insert("ckb-header-epoch-length".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderDepBlockNumber) =>
                {
                    features.insert("ckb-header-full-decode".to_string());
                    features.insert("ckb-header-block-number".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHeaderDepTimestampMillis) =>
                {
                    features.insert("ckb-header-full-decode".to_string());
                    features.insert("ckb-header-timestamp-millis".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputSince) =>
                {
                    features.insert("ckb-input-since".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(
                            cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRawTransactionHashWithoutCellDeps,
                        ) =>
                {
                    features.insert("ckb-blake2b".to_string());
                    features.insert("ckb-raw-transaction-empty-cell-deps-hash".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbTransactionHash) =>
                {
                    features.insert("ckb-transaction-hash".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbTransactionBlake2bGather | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBlake2bSelectChunks)) =>
                {
                    features.insert("ckb-blake2b".to_string());
                    features.insert("ckb-gather-hash".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbTransactionU32Le) =>
                {
                    features.insert("ckb-transaction-byte-read".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBytes32) =>
                {
                    features.insert("ckb-witness-read".to_string());
                    features.insert("ckb-witness-exact-fixed-bytes".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataHashField) =>
                {
                    features.insert("ckb-source-cell-fields".to_string());
                    features.insert("ckb-consensus-data-hash-field".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbExecCellDepU8Args | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbExecCellDepHex4)) =>
                {
                    features.insert("ckb-exec-process-replacement".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputSinceAt) =>
                {
                    features.insert("ckb-input-since".to_string());
                    features.insert("ckb-source-cell-fields".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockSize | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeSize | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockU8 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeU8)) =>
                {
                    features.insert("ckb-source-cell-fields".to_string());
                    features.insert("ckb-serialized-script-read".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceEpochAbsolute | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceEpochRelative)) =>
                {
                    features.insert("ckb-since-epoch-encoding".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceBlockAbsolute | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceBlockRelative | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceTimestampAbsolute | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceTimestampRelative)) =>
                {
                    features.insert("ckb-since-scalar-encoding".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceDecode | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceFromRawChecked | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceAsAbsoluteBlock | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceAsRelativeBlock | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceAsAbsoluteEpoch | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceAsRelativeEpoch | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceAsAbsoluteTimestamp | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceAsRelativeTimestamp | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceIsRelative | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceIsDisabled | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceMetric | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSinceValue)) =>
                {
                    features.insert("ckb-since-checked-decoding".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbEpochDuration | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbEpochAdd | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbEpochSub)) =>
                {
                    features.insert("ckb-epoch-checked-arithmetic".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputOutPointIndex | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputOutPointTxHashLow | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputOutPointTxHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireInputOutPointTxHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireInputOutPoint | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireMetapointRelative | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockMatchMasterOutPointPairsFromData)) =>
                {
                    features.insert("ckb-source-input-out-point".to_string());
                    features.insert("ckb-source-cell-fields".to_string());
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireMetapointRelative | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockMatchMasterOutPointPairsFromData)) {
                        features.insert("ckb-metapoint-relative".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockMatchMasterOutPointPairsFromData)) {
                        features.insert("ckb-metapoint-pair-cardinality".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32DataFiltered)) {
                        features.insert("ckb-metapoint-data-driven-cardinality".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32DataFiltered)) {
                        features.insert("ckb-metapoint-filtered-cardinality".to_string());
                    }
                    if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockMatchMasterOutPointPairsFromData) {
                        features.insert("ckb-metapoint-master-outpoint-data-cardinality".to_string());
                    }
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellCapacity | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellOccupiedCapacity | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellUnoccupiedCapacity | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellOutputIndex | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputOutPointIndex | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputOutPointTxHashLow | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInputOutPointTxHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireInputOutPointTxHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireInputOutPoint | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireMetapointRelative | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairs | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32Data | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockTypeMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireTypeLockMetapointPairsFromI32DataFiltered | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireLockMatchMasterOutPointPairsFromData | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockHashLow | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeHashLow | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockCodeHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeCodeHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockHashType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeHashType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockArgs32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeArgs32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgs32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgs32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsExact | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsExact | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockExactHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeExactHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellDepExactVerifierHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockDeploymentLineHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeDeploymentLineHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellDepDeploymentLineVerifierHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsPrefix32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsPrefix32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsSuffix32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsSuffix32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockScriptHashType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeScriptHashType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataBytes32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataSize | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataEqual | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceBytesEqual | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceBytesEqualMemory | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceBytesZero | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellCount | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellHasType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataU8 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataU32Le | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataU64Le)) =>
                {
                    features.insert("ckb-source-cell-fields".to_string());
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockArgs32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeArgs32)) {
                        features.insert("ckb-script-args-read".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsEmpty | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgs32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgs32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsExact | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsExact | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsPrefix32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsPrefix32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsSuffix32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsSuffix32)) {
                        features.insert("ckb-script-args-requirements".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockArgsExact | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeArgsExact)) {
                        features.insert("ckb-script-construction".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockExactHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeExactHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellDepExactVerifierHandle)) {
                        features.insert("ckb-exact-script-handle-v1".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockDeploymentLineHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeDeploymentLineHandle | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellDepDeploymentLineVerifierHandle)) {
                        features.insert("ckb-deployment-line-handle-v1".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockCodeHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeCodeHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockHashType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeHashType)) {
                        features.insert("ckb-script-ref-read".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataBytes32 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataEqual | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceBytesEqual | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceBytesEqualMemory | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceBytesZero | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataU8 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataU32Le | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataU64Le)) {
                        features.insert("ckb-cell-data-decode".to_string());
                    }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellLockScriptHashType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellTypeScriptHashType))
                    {
                        features.insert("ckb-script-identity-requirements".to_string());
                    }
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataBlake2bSpan | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBlake2bSpan)) =>
                {
                    features.insert("ckb-blake2b".to_string());
                    features.insert("ckb-exact-span-hash".to_string());
                    features.insert(
                        if func.runtime_helper()
                            == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBlake2bSpan)
                        {
                            "ckb-witness-read"
                        } else {
                            "ckb-source-cell-fields"
                        }
                        .to_string(),
                    );
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCurrentScriptArgsEmpty) =>
                {
                    features.insert("ckb-script-args-requirements".to_string());
                    features.insert("ckb-source-cell-fields".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoAccumulatedRate) =>
                {
                    features.insert("ckb-dao-header-accumulated-rate".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoInputAccumulatedRate) =>
                {
                    features.insert("ckb-dao-input-header-accumulated-rate".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoHasDaoType | cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoIsDepositData | cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoIsWithdrawalRequestData)) =>
                {
                    features.insert("ckb-dao-cell-classification".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoRequireHeaderDepForInput) =>
                {
                    features.insert("ckb-dao-header-lineage".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoRequireInputSinceAtLeast) =>
                {
                    features.insert("ckb-dao-input-since-maturity".to_string());
                    features.insert("ckb-input-since".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(
                            cellscript_artifact_checker::runtime_contract::RuntimeHelper::DaoRequireInputRelativeEpochSinceAtLeast,
                        ) =>
                {
                    features.insert("ckb-dao-relative-epoch-maturity".to_string());
                    features.insert("ckb-input-since".to_string());
                    features.insert("ckb-since-epoch-encoding".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::XudtRequireGroupAmountConserved) =>
                {
                    features.insert("ckb-xudt-layout-binding".to_string());
                    features.insert("ckb-xudt-group-amount-conservation".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(
                            cellscript_artifact_checker::runtime_contract::RuntimeHelper::CellscriptRequireFungibleTypeGroupV1,
                        ) =>
                {
                    features.insert("ckb-fungible-type-group-v1".to_string());
                    features.insert("ckb-u128-group-amount-conservation".to_string());
                    features.insert("ckb-owner-lock-authorized-issuance".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::XudtRequireGroupAmountMinted | cellscript_artifact_checker::runtime_contract::RuntimeHelper::XudtRequireGroupAmountBurned)) =>
                {
                    features.insert("ckb-xudt-layout-binding".to_string());
                    features.insert("ckb-xudt-group-amount-delta".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper().is_some_and(|helper| helper.symbol().starts_with("__xudt_")) =>
                {
                    features.insert("ckb-xudt-layout-binding".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper().is_some_and(|helper| helper.symbol().starts_with("__c256_")) =>
                {
                    features.insert("ckb-c256-product-arithmetic".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper().is_some_and(|helper| helper.symbol().starts_with("__ckb_spawn")) =>
                {
                    features.insert("ckb-spawn-ipc".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::ZkRequireTransition) =>
                {
                    features.insert("zk-exact-transition-verifier".into());
                    features.insert("ckb-spawn-ipc".into());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::Bip340RequireSignature | cellscript_artifact_checker::runtime_contract::RuntimeHelper::Bip340RequireSignatureFromCellDep)) =>
                {
                    features.insert("btc-bip340-runtime-verifier".to_string());
                    features.insert("ckb-spawn-ipc".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireBoundedCellDepDataHash) =>
                {
                    features.insert("ckb-bounded-cell-dep-scan".to_string());
                    features.insert("ckb-cell-dep-data-hash-binding".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWait | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbProcessId | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbPipe | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbPipeWrite | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbPipeRead | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbInheritedFd | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbClose)) =>
                {
                    features.insert("ckb-spawn-ipc".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper().is_some_and(|helper| helper.symbol().starts_with("__ckb_source_")) =>
                {
                    features.insert("ckb-source-view".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper().is_some_and(|helper| helper.symbol().starts_with("__ckb_witness_bounded_")) =>
                {
                    features.insert("ckb-bounded-witness-view".to_string());
                    features.insert("ckb-witness-args".to_string());
                    features.insert("ckb-witness-read".to_string());
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBoundedBlake2b)
                    {
                        features.insert("ckb-blake2b".to_string());
                    }
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper().is_some_and(|helper| helper.symbol().starts_with("__ckb_witness_")) =>
                {
                    features.insert("ckb-witness-args".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSighashAll) =>
                {
                    features.insert(ir::IrDeferredRuntimeFeature::CkbSighashAll.feature().to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSighashAllZeroLock) =>
                {
                    features.insert("ckb-sighash-all-zero-lock-v1".to_string());
                    features.insert("ckb-signing-message-domain".to_string());
                    features.insert("ckb-transaction-hash".to_string());
                    features.insert("ckb-witness-read".to_string());
                    features.insert("ckb-blake2b".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireWitnessSizeAtLeast) =>
                {
                    features.insert("ckb-witness-args".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper().is_some_and(|helper| helper.symbol().starts_with("__ckb_require_"))
                        && !matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireBoundedCellDepDataHash | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireSha256dMerkleRoot)) =>
                {
                    features.insert("ckb-declarative-since".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbOccupiedCapacity) =>
                {
                    features.insert("ckb-declarative-capacity".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashChain) =>
                {
                    features.insert("profile-hash-chain".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashPair) =>
                {
                    features.insert("profile-hash-pair".to_string());
                    features.insert("ckb-blake2b".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper() == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashBlake2b) =>
                {
                    features.insert("ckb-blake2b".to_string());
                }
                ir::IrInstruction::Call { func, .. }
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashSha256 | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashSha256Pair | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashSha256d | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashSha256dPair)) =>
                {
                    features.insert("bounded-sha256".to_string());
                    if matches!(func.runtime_helper(), Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashSha256d | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbHashSha256dPair)) {
                        features.insert("bounded-sha256d".to_string());
                    }
                }
                ir::IrInstruction::Call { func, .. }
                    if func.runtime_helper()
                        == Some(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireSha256dMerkleRoot) =>
                {
                    features.insert("bounded-sha256".to_string());
                    features.insert("bounded-sha256d".to_string());
                    features.insert("bounded-sha256d-merkle-proof".to_string());
                }
                _ => {}
            }
        }
    }
    features.into_iter().collect()
}

fn param_ckb_runtime_features(params: &[ir::IrParam]) -> Vec<String> {
    let mut features = BTreeSet::new();
    if params.iter().any(|param| param.source == ast::ParamSource::LockArgs) {
        features.insert("ckb-lock-args".to_string());
    }
    features.into_iter().collect()
}

pub(super) fn merge_ckb_runtime_features(body_features: Vec<String>, params: &[ir::IrParam]) -> Vec<String> {
    body_features.into_iter().chain(param_ckb_runtime_features(params)).collect::<BTreeSet<_>>().into_iter().collect()
}

fn param_ckb_runtime_accesses(params: &[ir::IrParam]) -> Vec<CkbRuntimeAccessMetadata> {
    params
        .iter()
        .filter(|param| param.source == ast::ParamSource::LockArgs)
        .enumerate()
        .map(|(index, param)| {
            runtime_access_metadata(
                "lock-args",
                "LOAD_SCRIPT_ARGS",
                "ScriptArgs",
                param.name.clone(),
                "ScriptArgs",
                "metadata-summary",
                None,
                runtime_scalar_metadata_ordinal(index as u64),
                runtime_range_whole_value(),
            )
        })
        .collect()
}

pub(super) fn merge_ckb_runtime_accesses(
    mut body_accesses: Vec<CkbRuntimeAccessMetadata>,
    params: &[ir::IrParam],
) -> Vec<CkbRuntimeAccessMetadata> {
    body_accesses.extend(param_ckb_runtime_accesses(params));
    body_accesses
}

pub(super) fn is_executable_schema_field_access(
    obj: &ir::IrOperand,
    field: &str,
    param_schema_vars: &BTreeSet<usize>,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(var) = obj else {
        return false;
    };
    if !param_schema_vars.contains(&var.id) {
        return false;
    }
    let Some(type_name) = named_type_name(&var.ty) else {
        return false;
    };
    let Some(layout) = type_layouts.get(type_name).and_then(|fields| fields.get(field)) else {
        return false;
    };
    metadata_layout_or_named_fixed_byte_width(layout, type_layouts).is_some()
        || metadata_molecule_vector_element_fixed_width(&layout.ty, type_layouts).is_some()
        || matches!(nested_values::ir_type(&layout.ty), Some(nested_values::NestedType::Values(_)))
}

pub(super) fn is_executable_aggregate_field_access(
    obj: &ir::IrOperand,
    field: &str,
    availability: &MetadataPreludeAvailability,
    type_layouts: &MetadataTypeLayouts,
) -> bool {
    let ir::IrOperand::Var(var) = obj else {
        return false;
    };
    let Some(source) = availability.aggregate_pointer_vars.get(&var.id) else {
        return false;
    };
    let Some(layout) = metadata_aggregate_or_named_field_layout(&source.ty, field, type_layouts) else {
        return false;
    };
    metadata_layout_or_named_fixed_byte_width(&layout, type_layouts).is_some()
}

pub(super) fn is_executable_tuple_call_return_field_access(
    obj: &ir::IrOperand,
    field: &str,
    availability: &MetadataPreludeAvailability,
) -> bool {
    let ir::IrOperand::Var(var) = obj else {
        return false;
    };
    availability.tuple_call_return_vars.get(&var.id).and_then(|ty| metadata_tuple_return_field_type(ty, field)).is_some()
}

pub(super) fn is_executable_output_type_hash(instruction: &ir::IrInstruction, availability: &MetadataPreludeAvailability) -> bool {
    let ir::IrInstruction::TypeHash { dest, .. } = instruction else {
        return false;
    };
    availability.output_type_hash_vars.contains(&dest.id) || availability.param_type_hash_vars.contains(&dest.id)
}

pub(super) fn is_executable_destroy(operand: &ir::IrOperand) -> bool {
    operand_named_type_name(operand).is_some()
}

pub(super) fn schema_pointer_var_ids(body: &ir::IrBody, params: &[ir::IrParam]) -> BTreeSet<usize> {
    let mut vars = params
        .iter()
        .filter(|param| named_type_name(&param.ty).is_some() || param.ty.has_structured_span_abi())
        .map(|param| param.binding.id)
        .collect::<BTreeSet<_>>();

    for block in &body.blocks {
        for instruction in &block.instructions {
            if let ir::IrInstruction::ReadRef { dest, .. } = instruction {
                vars.insert(dest.id);
            }
            if let ir::IrInstruction::BoundedCellLoad { dest, .. } = instruction {
                vars.insert(dest.id);
            }
            if let ir::IrInstruction::BoundedPlanLoad { dest, .. } = instruction {
                vars.insert(dest.id);
            }
            if let Some(var_id) = consumed_schema_var_id(instruction) {
                vars.insert(var_id);
            }
        }
    }

    vars
}

pub(super) fn consumed_schema_var_id(instruction: &ir::IrInstruction) -> Option<usize> {
    let operand = match instruction {
        ir::IrInstruction::Consume { operand }
        | ir::IrInstruction::Transfer { operand, .. }
        | ir::IrInstruction::Destroy { operand, .. }
        | ir::IrInstruction::Settle { operand, .. }
        | ir::IrInstruction::ReplaceUnique { operand, .. } => operand,
        ir::IrInstruction::Claim { receipt, .. } => receipt,
        _ => return None,
    };
    match operand {
        ir::IrOperand::Var(var) if named_type_name(&var.ty).is_some() => Some(var.id),
        _ => None,
    }
}

pub(super) fn scheduler_accesses_from_metadata(accesses: &[CkbRuntimeAccessMetadata]) -> Vec<crate::stdlib::SchedulerAccess> {
    accesses
        .iter()
        .filter(|access| scheduler_access_is_cell_state_access(access))
        .map(|access| crate::stdlib::SchedulerAccess {
            operation: access.operation.clone(),
            source: access.source.clone(),
            index: u32::try_from(access.index).unwrap_or(u32::MAX),
            binding: access.binding.clone(),
        })
        .collect()
}

pub(super) fn ckb_script_group_metadata(
    entry_kind: &str,
    ckb_runtime_accesses: &[CkbRuntimeAccessMetadata],
    target_profile: TargetProfile,
) -> Option<CkbScriptGroupMetadata> {
    if !target_profile.is_ckb() {
        return None;
    }

    let group_kind = match entry_kind {
        "lock" => "lock",
        _ => "type",
    };
    let active_script_group = match group_kind {
        "lock" => "lock-group",
        _ => "type-group",
    };
    let mut selected_sources = BTreeSet::new();
    let mut input_sources = BTreeSet::new();
    let mut output_sources = BTreeSet::new();
    let mut cell_dep_sources = BTreeSet::new();
    let mut header_dep_sources = BTreeSet::new();
    let mut group_scoped_sources = BTreeSet::new();

    for access in ckb_runtime_accesses {
        selected_sources.insert(access.source.clone());
        match access.source.as_str() {
            "Input" | "GroupInput" => {
                input_sources.insert(access.source.clone());
            }
            "Input/GroupInput" => {
                input_sources.insert("Input".to_string());
                input_sources.insert("GroupInput".to_string());
                group_scoped_sources.insert("GroupInput".to_string());
            }
            "Output" | "GroupOutput" => {
                output_sources.insert(access.source.clone());
            }
            "CellDep" | "GroupCellDep" => {
                cell_dep_sources.insert(access.source.clone());
            }
            "HeaderDep" | "GroupHeaderDep" => {
                header_dep_sources.insert(access.source.clone());
            }
            _ => {}
        }
        if access.source.starts_with("Group") {
            group_scoped_sources.insert(access.source.clone());
        }
    }

    Some(CkbScriptGroupMetadata {
        entry_kind: entry_kind.to_string(),
        group_kind: group_kind.to_string(),
        active_script_group: active_script_group.to_string(),
        source_encoding: "ckb-source-group-high-bit".to_string(),
        selected_sources: selected_sources.into_iter().collect(),
        input_sources: input_sources.into_iter().collect(),
        output_sources: output_sources.into_iter().collect(),
        cell_dep_sources: cell_dep_sources.into_iter().collect(),
        header_dep_sources: header_dep_sources.into_iter().collect(),
        group_scoped_sources: group_scoped_sources.into_iter().collect(),
    })
}

fn scheduler_access_is_cell_state_access(access: &CkbRuntimeAccessMetadata) -> bool {
    matches!(access.source.as_str(), "Input" | "CellDep" | "Output")
        && matches!(
            access.operation.as_str(),
            "input"
                | "consume"
                | "transfer"
                | "destroy"
                | "claim"
                | "settle"
                | "read_ref"
                | "output"
                | "create"
                | "create_unique"
                | "replace_unique"
                | "create_unique-identity-lock_hash"
                | "create_unique-identity-type_hash"
                | "replace_unique-identity-lock_hash"
                | "replace_unique-identity-type_hash"
                | "mutate-input"
                | "mutate-output"
        )
}
