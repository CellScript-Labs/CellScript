use super::error::{CompileError, Result};
use super::proof_plan::EvidenceTier;
use super::{aggregate_lowering, artifact, ast, ir, nested_values, CompileEntryScope, FungibleTypeGroupEntryMetadata};
use std::collections::{BTreeSet, HashMap};

pub(super) const FUNGIBLE_TYPE_GROUP_V1_CONTRACT: &str = "fungible-type-group-v1";

pub(super) const FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION: &str = "__cellscript_fungible_type_group_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
struct FungibleTypeGroupV1Candidate {
    invariant: String,
    type_name: String,
    field: String,
}

fn invariant_mentions_type(invariant: &ir::IrInvariant, type_name: &str) -> bool {
    let target_mentions_type = |target: &ast::AggregateTarget| target.type_name.as_deref() == Some(type_name);
    invariant.reads.iter().any(target_mentions_type)
        || invariant
            .aggregates
            .iter()
            .any(|aggregate| target_mentions_type(&aggregate.target) || aggregate.rhs.as_ref().is_some_and(target_mentions_type))
        || invariant.quantifiers.iter().any(|quantifier| target_mentions_type(&quantifier.range))
}

fn fungible_type_group_v1_candidates(ir: &ir::IrModule) -> Vec<FungibleTypeGroupV1Candidate> {
    let type_defs = ir
        .items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::TypeDef(type_def) => Some((type_def.name.as_str(), type_def)),
            _ => None,
        })
        .chain(ir.external_type_defs.iter().map(|type_def| (type_def.name.as_str(), type_def)))
        .collect::<HashMap<_, _>>();

    ir.items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::Invariant(invariant) => Some(invariant),
            _ => None,
        })
        .filter_map(|invariant| {
            let [aggregate] = invariant.aggregates.as_slice() else {
                return None;
            };
            if !invariant.quantifiers.is_empty() || invariant.assert_count != 0 {
                return None;
            }
            let selected = aggregate_lowering::group_u128_conservation_field(ir, invariant, aggregate)?;
            let (type_name, selected_field) = (selected.type_name, selected.field_name);
            if invariant.scope.as_deref() != Some("group")
                || invariant.reads.len() != 2
                || !aggregate_lowering::has_group_conservation_reads(invariant, &aggregate.target)
            {
                return None;
            }
            let type_def = type_defs.get(type_name)?;
            let [field] = type_def.fields.as_slice() else {
                return None;
            };
            if field.name != selected_field
                || !matches!(&field.ty, ir::IrType::U128)
                || field.offset != 0
                || type_def.kind == ir::IrTypeKind::Struct
                || !type_def.validity_predicates.is_empty()
                || type_def.type_id.is_some()
                || type_def.capacity_floor_shannons.is_some()
                || type_def.claim_output.is_some()
                || type_def.flow_states.as_ref().is_some_and(|states| !states.is_empty())
                || type_def.flow_state_field.is_some()
                || !type_def.flow_rules.is_empty()
                || type_def.flow_initial_state.is_some()
                || !type_def.flow_terminal_states.is_empty()
                || type_def.flow_terminal_discharge.is_some()
                || type_def.flow_state_model.is_some()
                || type_def.identity != ir::IrIdentityPolicy::None
            {
                return None;
            }
            Some(FungibleTypeGroupV1Candidate {
                invariant: invariant.name.clone(),
                type_name: type_name.to_string(),
                field: selected_field.to_string(),
            })
        })
        .filter(|candidate| {
            ir.items
                .iter()
                .filter_map(|item| match item {
                    ir::IrItem::Invariant(invariant) => Some(invariant),
                    _ => None,
                })
                .all(|invariant| invariant.name == candidate.invariant || !invariant_mentions_type(invariant, &candidate.type_name))
        })
        .collect()
}

pub(super) fn fungible_type_group_entry_metadata(ir: &ir::IrModule) -> Option<FungibleTypeGroupEntryMetadata> {
    let ir::IrEntrySelection::FungibleTypeGroupV1(profile) = &ir.entry_selection else { return None };
    let has_entry = ir.items.iter().any(|item| {
        matches!(item, ir::IrItem::Action(action)
            if action.name == profile.entry_action && action.params.is_empty()
                && aggregate_lowering::body_checks_fungible_profile(&action.body))
    });
    let candidates = fungible_type_group_v1_candidates(ir);
    let [candidate] = candidates.as_slice() else {
        return None;
    };
    if profile.entry_action != FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION
        || profile.invariant != candidate.invariant
        || profile.type_name != candidate.type_name
        || profile.field != candidate.field
    {
        return None;
    }
    has_entry.then(|| FungibleTypeGroupEntryMetadata {
        contract: FUNGIBLE_TYPE_GROUP_V1_CONTRACT.to_string(),
        entry_action: FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION.to_string(),
        invariant: candidate.invariant.clone(),
        type_name: candidate.type_name.clone(),
        field: candidate.field.clone(),
        data_length_bytes: 16,
        amount_offset_bytes: 0,
        amount_width_bytes: 16,
        endianness: "little".to_string(),
        arithmetic: "checked-u128-sum-equality".to_string(),
        group_scope: "complete-ckb-type-script-group".to_string(),
        owner_mode: "script-args-32-byte-owner-lock-hash".to_string(),
        owner_args_length_bytes: 32,
        authority_modes: vec!["input-lock-hash".to_string(), "tagged-input-type-script-hash".to_string()],
        authority_args_lengths_bytes: vec![32, 33],
        owner_authorized_mint: true,
        owner_authorized_burn: true,
        non_owner_input_group_non_empty: true,
        non_owner_output_group_non_empty: true,
        non_owner_conservation_required: true,
        payload_required: false,
        witness_policy: "ignored; no empty-witness requirement".to_string(),
        runtime_helper: aggregate_lowering::FUNGIBLE_TYPE_GROUP_V1_METADATA_HELPER.to_string(),
        evidence_tier: EvidenceTier::CheckedRuntime,
    })
}

fn scope_ir_to_fungible_type_group_v1(ir: &ir::IrModule, selected_type: Option<&str>) -> Result<ir::IrModule> {
    let candidates = fungible_type_group_v1_candidates(ir)
        .into_iter()
        .filter(|candidate| selected_type.is_none_or(|selected| candidate.type_name == selected))
        .collect::<Vec<_>>();
    let [candidate] = candidates.as_slice() else {
        let detail = if candidates.is_empty() {
            selected_type.map_or_else(
                || "no eligible invariant was found; expected one persistent type encoded as exactly one u128 field and one type_group/group equality between complete group input and output sums, with no additional type or invariant rules".to_string(),
                |selected| format!("no eligible invariant was found for selected type '{selected}'"),
            )
        } else {
            format!(
                "{} eligible invariants were found ({}); the entry must resolve to exactly one structural candidate",
                candidates.len(),
                candidates.iter().map(|candidate| candidate.invariant.as_str()).collect::<Vec<_>>().join(", ")
            )
        };
        return Err(CompileError::without_span(format!("cannot compile {} entry: {}", FUNGIBLE_TYPE_GROUP_V1_CONTRACT, detail)));
    };

    let selected_type = ir.items.iter().find_map(|item| match item {
        ir::IrItem::TypeDef(type_def) if type_def.name == candidate.type_name => Some(item.clone()),
        _ => None,
    });
    let selected_invariant = ir
        .items
        .iter()
        .find_map(|item| match item {
            ir::IrItem::Invariant(invariant) if invariant.name == candidate.invariant => Some(item.clone()),
            _ => None,
        })
        .ok_or_else(|| CompileError::without_span("selected fungible invariant disappeared during entry scoping"))?;
    let entry = ir::IrItem::Action(ir::IrAction {
        name: FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION.to_string(),
        entry_cells: None,
        source_dispositions: Vec::new(),
        successor_relations: Vec::new(),
        audit_claims: Vec::new(),
        params: Vec::new(),
        return_type: None,
        state_transition_edges: Vec::new(),
        protocol_role_candidates: Vec::new(),
        body: ir::IrBody {
            source_bindings: Vec::new(),
            cell_bindings: Vec::new(),
            consume_set: Vec::new(),
            read_refs: Vec::new(),
            create_set: Vec::new(),
            mutate_set: Vec::new(),
            write_intents: Vec::new(),
            bounded_collection_ops: Vec::new(),
            borrow_regions: Vec::new(),
            trusted_external_calls: Vec::new(),
            enforced_claims: Vec::new(),
            zk_origins: Vec::new(),
            blocks: vec![ir::IrBlock {
                id: ir::BlockId(0),
                instructions: vec![ir::IrInstruction::Call {
                    dest: None,
                    func: crate::ir::IrCallTarget::runtime_symbol(aggregate_lowering::FUNGIBLE_TYPE_GROUP_V1_CODEGEN_HELPER),
                    args: Vec::new(),
                }],
                terminator: ir::IrTerminator::Return(None),
                runtime_error: None,
            }],
        },
        effects: ir::EffectFacts::empty(),
        scheduler_hints: ir::SchedulerHints { parallelizable: false, touches_shared: Vec::new(), estimated_cycles: 0 },
    });

    let mut items = Vec::new();
    if let Some(type_def) = selected_type {
        items.push(type_def);
    }
    items.push(selected_invariant);
    items.push(entry);

    Ok(ir::IrModule {
        name: ir.name.clone(),
        items,
        entry_selection: ir::IrEntrySelection::FungibleTypeGroupV1(ir::IrFungibleTypeGroupProfile {
            entry_action: FUNGIBLE_TYPE_GROUP_V1_ENTRY_ACTION.to_string(),
            invariant: candidate.invariant.clone(),
            type_name: candidate.type_name.clone(),
            field: candidate.field.clone(),
        }),
        generic_contracts: ir.generic_contracts.clone(),
        source_type_origins: ir.source_type_origins.clone(),
        nominal_identities: ir.nominal_identities.clone(),
        source_callable_targets: ir.source_callable_targets.clone(),
        external_type_defs: ir.external_type_defs.iter().filter(|type_def| type_def.name == candidate.type_name).cloned().collect(),
        external_callable_abis: Vec::new(),
        enum_fixed_sizes: HashMap::new(),
        enum_layouts: HashMap::new(),
    })
}

pub(super) fn select_compile_entry(ir: ir::IrModule, scope: Option<&CompileEntryScope>) -> Result<ir::IrModule> {
    match scope {
        Some(scope) => scope_ir_to_entry(&ir, scope),
        None => Ok(ir),
    }
}

pub(super) fn scope_ir_to_entry(ir: &ir::IrModule, scope: &CompileEntryScope) -> Result<ir::IrModule> {
    match scope {
        CompileEntryScope::Artifact(declaration) => return artifact::scope_ir_to_artifact(ir, declaration),
        CompileEntryScope::FungibleTypeGroupV1 => return scope_ir_to_fungible_type_group_v1(ir, None),
        CompileEntryScope::FungibleTypeGroupV1For(type_name) => {
            return scope_ir_to_fungible_type_group_v1(ir, Some(type_name));
        }
        _ => {}
    }
    let selected_item = ir
        .items
        .iter()
        .find(|item| match (scope, item) {
            (CompileEntryScope::Action(name), ir::IrItem::Action(action)) => action.name == *name,
            (CompileEntryScope::Lock(name), ir::IrItem::Lock(lock)) => lock.name == *name,
            (CompileEntryScope::FungibleTypeGroupV1, _) => false,
            (CompileEntryScope::FungibleTypeGroupV1For(_), _) => false,
            _ => false,
        })
        .cloned()
        .ok_or_else(|| match scope {
            CompileEntryScope::Artifact(_) => CompileError::without_span("policy entry unexpectedly reached single-entry lookup"),
            CompileEntryScope::Action(name) => CompileError::without_span(format!("entry action '{}' was not found", name)),
            CompileEntryScope::Lock(name) => CompileError::without_span(format!("entry lock '{}' was not found", name)),
            CompileEntryScope::FungibleTypeGroupV1 => {
                CompileError::without_span("fungible type-group entry selection unexpectedly reached ordinary entry lookup")
            }
            CompileEntryScope::FungibleTypeGroupV1For(type_name) => CompileError::without_span(format!(
                "fungible type-group entry selection for '{type_name}' unexpectedly reached ordinary entry lookup"
            )),
        })?;

    let action_by_name = ir
        .items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::Action(action) => Some((action.name.as_str(), action)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let function_by_name = ir
        .items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::Function(function) => Some((function.name.as_str(), function)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let type_by_name = ir
        .items
        .iter()
        .filter_map(|item| match item {
            ir::IrItem::TypeDef(type_def) => Some((type_def.name.as_str(), type_def)),
            _ => None,
        })
        .chain(ir.external_type_defs.iter().map(|type_def| (type_def.name.as_str(), type_def)))
        .collect::<HashMap<_, _>>();

    let mut used_functions = BTreeSet::new();
    let mut used_actions = BTreeSet::new();
    let selected_action_name = match &selected_item {
        ir::IrItem::Action(action) => Some(action.name.clone()),
        _ => None,
    };
    let mut pending_callables = Vec::new();
    let mut used_types = BTreeSet::new();

    // Invariants survive entry pruning, so their resolved schema domains must
    // survive with them even when the selected action calls only a raw helper.
    for invariant in ir.items.iter().filter_map(|item| match item {
        ir::IrItem::Invariant(invariant) => Some(invariant),
        _ => None,
    }) {
        for target in invariant
            .reads
            .iter()
            .chain(invariant.aggregates.iter().flat_map(|aggregate| std::iter::once(&aggregate.target).chain(&aggregate.rhs)))
            .chain(invariant.quantifiers.iter().map(|quantifier| &quantifier.range))
        {
            if let Some(name) = &target.type_name {
                used_types.insert(name.clone());
            }
            for argument in &target.type_args {
                collect_ir_type_named_types(&ir::ast_type_to_ir(argument), &mut used_types);
            }
        }
    }
    collect_entry_item_scope(&selected_item, &mut used_types, &mut pending_callables);
    while let Some(callable_name) = pending_callables.pop() {
        if let Some(function) = function_by_name.get(callable_name.as_str()) {
            if !used_functions.insert(callable_name) {
                continue;
            }
            if let Some(contract) = ir.generic_contracts.get(&function.name) {
                used_types.extend(contract.type_dependencies.iter().cloned());
            }
            collect_params_named_types(&function.params, &mut used_types);
            if let Some(return_type) = &function.return_type {
                collect_ir_type_named_types(return_type, &mut used_types);
            }
            collect_body_scope(&function.body, &mut used_types, &mut pending_callables);
            continue;
        }

        if selected_action_name.as_deref() == Some(callable_name.as_str()) {
            continue;
        }
        let Some(action) = action_by_name.get(callable_name.as_str()) else {
            continue;
        };
        if !used_actions.insert(callable_name) {
            continue;
        }
        collect_params_named_types(&action.params, &mut used_types);
        if let Some(return_type) = &action.return_type {
            collect_ir_type_named_types(return_type, &mut used_types);
        }
        collect_body_scope(&action.body, &mut used_types, &mut pending_callables);
    }

    let mut pending_types = used_types.iter().cloned().collect::<Vec<_>>();
    let mut visited_types = BTreeSet::new();
    while let Some(type_name) = pending_types.pop() {
        if !visited_types.insert(type_name.clone()) {
            continue;
        }
        for dependency in
            ir.generic_contracts.get(&type_name).into_iter().flat_map(|contract| contract.type_dependencies.iter().cloned())
        {
            if used_types.insert(dependency.clone()) {
                pending_types.push(dependency);
            }
        }
        // Enum payloads participate in the same transitive layout closure as
        // structural fields. A named payload can be needed solely by the entry
        // witness decoder, without an expression mentioning its type directly.
        if let Some(layout) = ir.enum_layouts.get(&type_name) {
            for field in layout.variants.iter().flat_map(|variant| &variant.fields) {
                let before = used_types.len();
                collect_ir_type_named_types(&field.ty, &mut used_types);
                if used_types.len() != before {
                    pending_types.extend(used_types.iter().cloned());
                }
            }
        }
        let Some(type_def) = type_by_name.get(type_name.as_str()) else {
            continue;
        };
        for field in &type_def.fields {
            let before = used_types.len();
            collect_ir_type_named_types(&field.ty, &mut used_types);
            if used_types.len() != before {
                pending_types.extend(used_types.iter().cloned());
            }
        }
        if let Some(claim_output) = &type_def.claim_output {
            let before = used_types.len();
            collect_ir_type_named_types(claim_output, &mut used_types);
            if used_types.len() != before {
                pending_types.extend(used_types.iter().cloned());
            }
        }
    }

    let mut items = Vec::new();
    items.extend(ir.items.iter().filter_map(|item| match item {
        ir::IrItem::TypeDef(type_def) if used_types.contains(&type_def.name) => Some(item.clone()),
        _ => None,
    }));
    items.extend(ir.items.iter().filter_map(|item| match item {
        ir::IrItem::Invariant(_) => Some(item.clone()),
        _ => None,
    }));
    items.push(selected_item);
    items.extend(ir.items.iter().filter_map(|item| match item {
        ir::IrItem::Action(action) if used_actions.contains(&action.name) => Some(item.clone()),
        _ => None,
    }));
    items.extend(ir.items.iter().filter_map(|item| match item {
        ir::IrItem::Function(function) if used_functions.contains(&function.name) => Some(item.clone()),
        _ => None,
    }));

    Ok(ir::IrModule {
        name: ir.name.clone(),
        items,
        entry_selection: match scope {
            CompileEntryScope::Action(name) => ir::IrEntrySelection::Action(name.clone()),
            CompileEntryScope::Lock(name) => ir::IrEntrySelection::Lock(name.clone()),
            CompileEntryScope::Artifact(_) => unreachable!("policy entry scopes are handled before single-entry lookup"),
            CompileEntryScope::FungibleTypeGroupV1 | CompileEntryScope::FungibleTypeGroupV1For(_) => {
                unreachable!("fungible entry scopes are handled before ordinary entry lookup")
            }
        },
        generic_contracts: ir.generic_contracts.clone(),
        source_type_origins: ir.source_type_origins.clone(),
        nominal_identities: ir.nominal_identities.clone(),
        source_callable_targets: ir.source_callable_targets.clone(),
        external_type_defs: ir.external_type_defs.iter().filter(|type_def| used_types.contains(&type_def.name)).cloned().collect(),
        external_callable_abis: ir
            .external_callable_abis
            .iter()
            .filter(|abi| used_functions.contains(&abi.name) || used_actions.contains(&abi.name))
            .cloned()
            .collect(),
        enum_fixed_sizes: ir
            .enum_fixed_sizes
            .iter()
            .filter(|(name, _)| used_types.contains(*name))
            .map(|(name, size)| (name.clone(), *size))
            .collect(),
        enum_layouts: ir
            .enum_layouts
            .iter()
            .filter(|(name, _)| used_types.contains(*name))
            .map(|(name, layout)| (name.clone(), layout.clone()))
            .collect(),
    })
}

fn collect_entry_item_scope(item: &ir::IrItem, used_types: &mut BTreeSet<String>, pending_functions: &mut Vec<String>) {
    match item {
        ir::IrItem::Action(action) => {
            collect_params_named_types(&action.params, used_types);
            if let Some(return_type) = &action.return_type {
                collect_ir_type_named_types(return_type, used_types);
            }
            collect_body_scope(&action.body, used_types, pending_functions);
        }
        ir::IrItem::Lock(lock) => {
            collect_params_named_types(&lock.params, used_types);
            collect_body_scope(&lock.body, used_types, pending_functions);
        }
        ir::IrItem::TypeDef(_) | ir::IrItem::Invariant(_) | ir::IrItem::Function(_) => {}
    }
}

fn collect_params_named_types(params: &[ir::IrParam], used_types: &mut BTreeSet<String>) {
    for param in params {
        collect_ir_type_named_types(&param.ty, used_types);
        collect_ir_type_named_types(&param.binding.ty, used_types);
    }
}

fn collect_body_scope(body: &ir::IrBody, used_types: &mut BTreeSet<String>, pending_functions: &mut Vec<String>) {
    for pattern in body.consume_set.iter().chain(body.read_refs.iter()) {
        for (_, operand) in &pattern.fields {
            collect_operand_named_types(operand, used_types);
        }
    }
    for pattern in &body.create_set {
        used_types.insert(pattern.ty.clone());
        for (_, operand) in &pattern.fields {
            collect_operand_named_types(operand, used_types);
        }
        if let Some(lock) = &pattern.lock {
            collect_operand_named_types(lock, used_types);
        }
    }
    for pattern in &body.mutate_set {
        used_types.insert(pattern.ty.clone());
        for transition in &pattern.transitions {
            collect_operand_named_types(&transition.operand, used_types);
        }
    }
    for intent in &body.write_intents {
        used_types.insert(intent.ty.clone());
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            collect_instruction_scope(instruction, used_types, pending_functions);
        }
        match &block.terminator {
            ir::IrTerminator::Return(Some(operand)) | ir::IrTerminator::Branch { cond: operand, .. } => {
                collect_operand_named_types(operand, used_types);
            }
            ir::IrTerminator::Return(None) | ir::IrTerminator::Jump(_) => {}
        }
    }
}

fn collect_instruction_scope(instruction: &ir::IrInstruction, used_types: &mut BTreeSet<String>, pending_functions: &mut Vec<String>) {
    match instruction {
        ir::IrInstruction::AllocatedValue(op) => {
            for member in &op.declaration.members {
                let types = match &member.kind {
                    ast::ValueCollectionKind::Vec(value) | ast::ValueCollectionKind::OrderedSet(value) => vec![value],
                    ast::ValueCollectionKind::OrderedMap { key, value } => vec![key, value],
                };
                for ty in types {
                    collect_ir_type_named_types(&ir::ast_type_to_ir(ty), used_types);
                }
            }
            if let Some(dest) = &op.dest {
                collect_ir_type_named_types(&dest.ty, used_types);
            }
            for operand in op.owner.iter().chain(&op.args) {
                collect_operand_named_types(operand, used_types);
            }
        }
        ir::IrInstruction::LoadConst { dest, .. } => collect_ir_type_named_types(&dest.ty, used_types),
        ir::IrInstruction::LoadVar { dest, .. } => collect_ir_type_named_types(&dest.ty, used_types),
        ir::IrInstruction::StoreVar { src, .. } => collect_operand_named_types(src, used_types),
        ir::IrInstruction::Binary { dest, left, right, .. } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(left, used_types);
            collect_operand_named_types(right, used_types);
        }
        ir::IrInstruction::Unary { dest, operand, .. }
        | ir::IrInstruction::FieldAccess { dest, obj: operand, .. }
        | ir::IrInstruction::Index { dest, arr: operand, .. }
        | ir::IrInstruction::Length { dest, operand }
        | ir::IrInstruction::TypeHash { dest, operand }
        | ir::IrInstruction::Move { dest, src: operand } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(operand, used_types);
        }
        ir::IrInstruction::CollectionNew { dest, ty, capacity } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_ir_type_named_types(ty, used_types);
            if let Some(capacity) = capacity {
                collect_operand_named_types(capacity, used_types);
            }
        }
        ir::IrInstruction::CollectionCapacity { dest, collection } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(collection, used_types);
        }
        ir::IrInstruction::CollectionPush { collection, value } | ir::IrInstruction::CollectionExtend { collection, slice: value } => {
            collect_operand_named_types(collection, used_types);
            collect_operand_named_types(value, used_types);
        }
        ir::IrInstruction::CollectionClear { collection } => {
            collect_operand_named_types(collection, used_types);
        }
        ir::IrInstruction::CollectionReverse { collection } => {
            collect_operand_named_types(collection, used_types);
        }
        ir::IrInstruction::CollectionTruncate { collection, len } => {
            collect_operand_named_types(collection, used_types);
            collect_operand_named_types(len, used_types);
        }
        ir::IrInstruction::CollectionSwap { collection, left, right } => {
            collect_operand_named_types(collection, used_types);
            collect_operand_named_types(left, used_types);
            collect_operand_named_types(right, used_types);
        }
        ir::IrInstruction::CollectionContains { dest, collection, value } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(collection, used_types);
            collect_operand_named_types(value, used_types);
        }
        ir::IrInstruction::CollectionRemove { dest, collection, index } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(collection, used_types);
            collect_operand_named_types(index, used_types);
        }
        ir::IrInstruction::CollectionInsert { collection, index, value } => {
            collect_operand_named_types(collection, used_types);
            collect_operand_named_types(index, used_types);
            collect_operand_named_types(value, used_types);
        }
        ir::IrInstruction::CollectionSet { collection, index, value } => {
            collect_operand_named_types(collection, used_types);
            collect_operand_named_types(index, used_types);
            collect_operand_named_types(value, used_types);
        }
        ir::IrInstruction::CollectionPop { dest, collection } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(collection, used_types);
        }
        ir::IrInstruction::BoundedCellLoad { dest, found, index, element_type, .. } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_ir_type_named_types(&found.ty, used_types);
            collect_operand_named_types(index, used_types);
            used_types.insert(element_type.clone());
        }
        ir::IrInstruction::BoundedPlanLoad { dest, found, plan, index, element_type, .. } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_ir_type_named_types(&found.ty, used_types);
            collect_operand_named_types(plan, used_types);
            collect_operand_named_types(index, used_types);
            used_types.insert(element_type.clone());
        }
        ir::IrInstruction::BoundedOutputVerify { index, pattern, .. } => {
            collect_operand_named_types(index, used_types);
            used_types.insert(pattern.ty.clone());
            for (_, operand) in &pattern.fields {
                collect_operand_named_types(operand, used_types);
            }
            if let Some(lock) = &pattern.lock {
                collect_operand_named_types(lock, used_types);
            }
        }
        ir::IrInstruction::BoundedOutputEnd { index } => collect_operand_named_types(index, used_types),
        ir::IrInstruction::VerifyExactCellRoles { contract, .. } => {
            used_types.extend(contract.roles.iter().map(|role| role.type_name.clone()));
        }
        ir::IrInstruction::ReadExactCellRole { dest, selection, index, .. } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(selection, used_types);
            if let Some(index) = index {
                collect_operand_named_types(index, used_types);
            }
        }
        ir::IrInstruction::Call { dest, func, args } => {
            if let Some(dest) = dest {
                collect_ir_type_named_types(&dest.ty, used_types);
            }
            if let Some(symbol) = func.function_symbol() {
                pending_functions.push(symbol.to_owned());
            }
            for arg in args {
                collect_operand_named_types(arg, used_types);
            }
        }
        ir::IrInstruction::ReadRef { dest, ty } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            used_types.insert(ty.clone());
        }
        ir::IrInstruction::Tuple { dest, fields } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            for field in fields {
                collect_operand_named_types(field, used_types);
            }
        }
        ir::IrInstruction::EnumConstruct { dest, enum_name, fields, .. } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            used_types.insert(enum_name.clone());
            for field in fields {
                collect_operand_named_types(field, used_types);
            }
        }
        ir::IrInstruction::EnumTag { dest, operand, enum_name } | ir::IrInstruction::EnumPayload { dest, operand, enum_name, .. } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(operand, used_types);
            used_types.insert(enum_name.clone());
        }
        ir::IrInstruction::Consume { operand } | ir::IrInstruction::Destroy { operand, .. } => {
            collect_operand_named_types(operand, used_types);
        }
        ir::IrInstruction::Transfer { dest, operand, to } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(operand, used_types);
            collect_operand_named_types(to, used_types);
        }
        ir::IrInstruction::Claim { dest, receipt } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(receipt, used_types);
        }
        ir::IrInstruction::Settle { dest, operand } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(operand, used_types);
        }
        ir::IrInstruction::CellMetadataEquality { left, right, .. } => {
            collect_operand_named_types(left, used_types);
            collect_operand_named_types(right, used_types);
        }
        ir::IrInstruction::Create { dest, pattern } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            used_types.insert(pattern.ty.clone());
            for (_, operand) in &pattern.fields {
                collect_operand_named_types(operand, used_types);
            }
            if let Some(lock) = &pattern.lock {
                collect_operand_named_types(lock, used_types);
            }
        }
        ir::IrInstruction::CreateUnique { dest, pattern, identity: _ } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            used_types.insert(pattern.ty.clone());
            for (_, operand) in &pattern.fields {
                collect_operand_named_types(operand, used_types);
            }
            if let Some(lock) = &pattern.lock {
                collect_operand_named_types(lock, used_types);
            }
        }
        ir::IrInstruction::ReplaceUnique { dest, operand, pattern, identity: _ } => {
            collect_ir_type_named_types(&dest.ty, used_types);
            collect_operand_named_types(operand, used_types);
            used_types.insert(pattern.ty.clone());
            for (_, field_operand) in &pattern.fields {
                collect_operand_named_types(field_operand, used_types);
            }
        }
    }
}

fn collect_operand_named_types(operand: &ir::IrOperand, used_types: &mut BTreeSet<String>) {
    if let ir::IrOperand::Var(var) = operand {
        collect_ir_type_named_types(&var.ty, used_types);
    }
}

fn collect_ir_type_named_types(ty: &ir::IrType, used_types: &mut BTreeSet<String>) {
    match ty {
        ir::IrType::OpenHandle(handle) => {
            used_types.insert(handle.source_text());
        }
        ir::IrType::WitnessBytes(_) | ir::IrType::Selection(_) => {
            used_types.insert(ty.source_text());
        }
        ir::IrType::Commitment(value) | ir::IrType::Opening(value) => {
            used_types.insert(ty.source_text());
            collect_ir_type_named_types(value, used_types);
        }
        ir::IrType::Bounded(collection) => {
            used_types.insert(ty.source_text());
            collect_ir_type_named_types(&collection.element_type, used_types);
        }
        ir::IrType::Context(view) => {
            used_types.insert(ty.source_text());
            if let Some(schema) = view.schema() {
                collect_ir_type_named_types(schema, used_types);
            }
        }
        ir::IrType::Vector(element) => {
            used_types.insert(ty.source_text());
            if let Some(element) = element {
                collect_ir_type_named_types(element, used_types);
            }
        }
        ir::IrType::Applied(application) => {
            used_types.insert(ty.source_text());
            for argument in application.arguments.iter().filter_map(ast::TypeArgument::as_type) {
                collect_ir_type_named_types(argument, used_types);
            }
        }
        ir::IrType::Named(name) => {
            used_types.insert(name.clone());
        }
        ir::IrType::Nested(view) => {
            used_types.insert(view.canonical_text());
            if let nested_values::NestedType::Values(nested_values::ValuesType {
                element: nested_values::Element::Record(record),
                ..
            })
            | nested_values::NestedType::RecordView { element: nested_values::Element::Record(record), .. } = view.description()
            {
                used_types.insert(record.spelling().to_string());
            }
        }
        ir::IrType::Array(inner, _) | ir::IrType::Ref(inner) | ir::IrType::MutRef(inner) => {
            collect_ir_type_named_types(inner, used_types)
        }
        ir::IrType::Tuple(items) => {
            for item in items {
                collect_ir_type_named_types(item, used_types);
            }
        }
        ir::IrType::U8
        | ir::IrType::U16
        | ir::IrType::U32
        | ir::IrType::I32
        | ir::IrType::U64
        | ir::IrType::U128
        | ir::IrType::Bool
        | ir::IrType::Unit
        | ir::IrType::Address
        | ir::IrType::Hash => {}
    }
}
