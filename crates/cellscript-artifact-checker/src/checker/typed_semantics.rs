//! Typed operation, foundation, ownership and dataflow admission.
use super::*;

pub(super) fn validate_typed_semantics(record: &VerifiedLoweringRecord) -> Result<(), CheckerError> {
    let typed = &record.typed_semantics;
    if typed.schema != TYPED_SEMANTICS_SCHEMA
        || typed.version != TYPED_SEMANTICS_VERSION
        || typed.module != record.module
        || typed.interface_hash.is_empty()
    {
        return Err(CheckerError::new(
            CheckerRejectionCode::V2419TypedSemanticsInvalid,
            "typed semantic record has an invalid schema, module, or interface identity",
        ));
    }
    ensure_sorted_unique(&typed.types, |item| item.name.as_str(), "typed type")?;
    ensure_sorted_unique(&typed.entries, |item| item.id.as_str(), "typed entry")?;
    ensure_sorted_unique(&typed.instantiations, |item| item.identity.as_str(), "typed instantiation")?;
    if typed.trusted_external_verifiers.len() > 1_024 {
        return typed_error("trusted external verifier record count exceeds 1024");
    }
    if typed.trusted_external_verifiers.windows(2).any(|pair| {
        (&pair[0].scope, &pair[0].operation, &pair[0].adapter, &pair[0].code_hash, &pair[0].name)
            >= (&pair[1].scope, &pair[1].operation, &pair[1].adapter, &pair[1].code_hash, &pair[1].name)
    }) {
        return typed_error("trusted external verifier records are not strictly sorted and unique");
    }
    validate_semantic_foundation(typed, record)?;
    let lowering_entries = record.entries.iter().map(|entry| (entry.id.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let typed_types = typed.types.iter().map(|ty| (ty.name.as_str(), ty)).collect::<BTreeMap<_, _>>();
    let typed_entries_by_name = typed.entries.iter().map(|entry| (entry.name.as_str(), entry)).collect::<BTreeMap<_, _>>();
    let allocated_owner_types = crate::allocated_values::owner_types(typed);
    let inferred_effects = infer_typed_effects(typed)?;
    let called_targets = typed
        .entries
        .iter()
        .flat_map(|entry| entry.blocks.iter())
        .flat_map(|block| block.operations.iter())
        .filter_map(|operation| operation.call.as_ref())
        .map(|call| call.target.as_str())
        .collect::<BTreeSet<_>>();
    let proof_ids = record.proof_records.iter().map(|proof| proof.id.as_str()).collect::<BTreeSet<_>>();
    validate_trusted_external_verifiers(typed, record)?;
    for ty in &typed.types {
        validate_typed_type(ty)?;
    }
    for instantiation in &typed.instantiations {
        if instantiation.identity.is_empty()
            || instantiation.module.is_empty()
            || instantiation.template.is_empty()
            || instantiation.type_arguments.is_empty()
            || !instantiation.constraints_verified
            || !matches!(instantiation.kind.as_str(), "struct" | "enum" | "function")
            || instantiation.value_ability_registry_version != 1
            || !instantiation.identity_includes_phantom_arguments
            || instantiation.cell_backed_layout_rejected != instantiation.fixed_layout_required
        {
            return typed_error(format!("generic instantiation '{}' is incomplete or unchecked", instantiation.identity));
        }
        let canonical_arguments = instantiation.type_arguments.join(",");
        let expected_concrete = format!("{}__mono__{}", instantiation.template, hex_encode(canonical_arguments.as_bytes()));
        let expected_identity = format!("{}::{}<{}>", instantiation.module, instantiation.template, canonical_arguments);
        if instantiation.concrete_name != expected_concrete || instantiation.identity != expected_identity {
            return typed_error(format!("generic instantiation '{}' has a non-canonical identity", instantiation.identity));
        }
    }
    crate::value_abilities::verify(typed)?;
    crate::generic_projection::verify(typed)?;
    crate::interface::verify_nominal_declarations(typed)?;
    let nominal_aliases = crate::generic_projection::nominal_aliases(typed);
    for entry in &typed.entries {
        let Some(lowering) = lowering_entries.get(entry.id.as_str()) else {
            if entry.kind == "helper" && !called_targets.contains(entry.name.as_str()) {
                continue;
            }
            return Err(CheckerError::new(
                CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                format!("typed entry '{}' has no machine lowering entry", entry.id),
            ));
        };
        if entry.name != lowering.name
            || entry.kind != lowering_entry_kind(lowering.kind)
            || canonical_abi_type(&entry.return_type) != canonical_abi_type(&lowering.return_type)
            || entry.params.len() != lowering.params.len()
        {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                format!("typed entry '{}' signature/effect differs from its machine entry", entry.id),
            ));
        }
        for (typed_param, lowered_param) in entry.params.iter().zip(&lowering.params) {
            if typed_param.index != lowered_param.index
                || typed_param.name != lowered_param.name
                || canonical_abi_type(&typed_param.ty) != canonical_abi_type(&lowered_param.ty)
            {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                    format!("typed entry '{}' parameter {} differs from its machine ABI", entry.id, typed_param.index),
                ));
            }
        }
        let locals = entry.locals.iter().map(|local| (local.id, local)).collect::<BTreeMap<_, _>>();
        if locals.len() != entry.locals.len() || entry.locals.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return typed_error(format!("typed entry '{}' locals are not strictly ordered and unique", entry.id));
        }
        for param in &entry.params {
            if locals.get(&param.binding_id).is_none_or(|local| local.name != param.name || local.ty != param.ty) {
                return typed_error(format!("typed entry '{}' parameter '{}' has no matching local", entry.id, param.name));
            }
        }
        let block_ids = entry.blocks.iter().map(|block| block.id).collect::<BTreeSet<_>>();
        if block_ids.len() != entry.blocks.len() || entry.blocks.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return typed_error(format!("typed entry '{}' blocks are not strictly ordered and unique", entry.id));
        }
        if !block_ids.contains(&entry.entry_block) {
            return typed_error(format!("typed entry '{}' references a missing entry block", entry.id));
        }
        for block in &entry.blocks {
            if block.successors.iter().any(|successor| !block_ids.contains(successor)) {
                return typed_error(format!("typed entry '{}' block {} references a missing successor", entry.id, block.id));
            }
            for (index, operation) in block.operations.iter().enumerate() {
                if operation.index != u32::try_from(index).unwrap_or(u32::MAX) {
                    return typed_error(format!("typed entry '{}' block {} has non-canonical operation indices", entry.id, block.id));
                }
                for destination in &operation.destinations {
                    if !locals.contains_key(destination) {
                        return typed_error(format!("typed operation '{}' defines unknown local {}", operation.opcode, destination));
                    }
                }
                for operand in &operation.operands {
                    if operand.ty.is_empty()
                        || (operand.local.is_some() == operand.constant.is_some())
                        || operand.local.is_some_and(|local_id| locals.get(&local_id).is_none_or(|local| local.ty != operand.ty))
                        || operand.constant.as_ref().is_some_and(|constant| constant_type(constant).is_none_or(|ty| ty != operand.ty))
                    {
                        return typed_error(format!("typed operation '{}' uses an unknown local or wrong type", operation.opcode));
                    }
                }
                validate_typed_operation(operation, &locals, &typed_types, &typed_entries_by_name, entry, block, typed)?;
                if let Some(call) = &operation.call
                    && (call.target.is_empty()
                        || call.contract.is_empty()
                        || call.params.len() != operation.operands.len()
                        || call
                            .params
                            .iter()
                            .zip(&operation.operands)
                            .any(|(param, operand)| !typed_call_operand_matches(entry, param, operand, &nominal_aliases, typed)))
                {
                    return typed_error(format!(
                        "typed call '{}' has an invalid signature contract: parameters {:?}, operands {:?}",
                        call.target,
                        call.params,
                        operation.operands.iter().map(|operand| &operand.ty).collect::<Vec<_>>()
                    ));
                }
                if let Some(call) = &operation.call {
                    match operation.destinations.as_slice() {
                        [] if call.return_type != "unit" => {
                            return typed_error(format!("typed call '{}' discards a non-unit return value", call.target));
                        }
                        [destination]
                            if locals.get(destination).is_none_or(|local| {
                                crate::generic_projection::checked_source_type(&local.ty, &nominal_aliases)
                                    != crate::generic_projection::checked_source_type(&call.return_type, &nominal_aliases)
                                    && !same_nominal_type(&local.ty, &call.return_type, typed)
                            }) =>
                        {
                            return typed_error(format!("typed call '{}' return type differs from its destination", call.target));
                        }
                        destinations if destinations.len() > 1 => {
                            return typed_error(format!("typed call '{}' has multiple destinations", call.target));
                        }
                        _ => {}
                    }
                }
            }
        }
        validate_typed_cfg_and_dataflow(entry, &locals, typed)?;
        crate::allocated_values::validate_lifetimes(entry, &allocated_owner_types)?;
        if Some(&entry.effects) != inferred_effects.get(entry.name.as_str()) {
            return typed_error(format!("typed entry '{}' differs from its independently reconstructed effect facts", entry.id));
        }
        if entry.kind == "lock" && !entry.effects.lifecycle_ops().is_empty() {
            return typed_error("lock predicate has lifecycle effects");
        }
        for borrow in &entry.borrows {
            let root_matches =
                locals.values().any(|local| local.name == borrow.root && strip_reference(&local.ty) == borrow.root_type);
            let binding_matches = locals.values().any(|local| local.name == borrow.binding && local.ty == borrow.view_type);
            let path_type = typed_borrow_path_type(&borrow.root_type, &borrow.path, &typed_types);
            let start_valid = entry
                .blocks
                .iter()
                .find(|block| block.id == borrow.start_block)
                .is_some_and(|block| usize::try_from(borrow.start_operation).is_ok_and(|index| index <= block.operations.len()));
            let end_valid = match (borrow.end_block, borrow.end_operation) {
                (Some(block_id), Some(operation)) => entry
                    .blocks
                    .iter()
                    .find(|block| block.id == block_id)
                    .is_some_and(|block| usize::try_from(operation).is_ok_and(|index| index <= block.operations.len())),
                (None, None) => true,
                _ => false,
            };
            if borrow.root.is_empty()
                || borrow.binding.is_empty()
                || borrow.root_type.is_empty()
                || !borrow.view_type.starts_with('&')
                || borrow.escapes
                || !root_matches
                || !binding_matches
                || path_type.as_deref() != Some(strip_reference(&borrow.view_type))
                || !start_valid
                || !end_valid
            {
                return typed_error(format!("typed borrow '{} -> {}' is invalid or escaping", borrow.root, borrow.binding));
            }
        }
        for ownership in &entry.ownership {
            let valid = match ownership.operation.as_str() {
                "read_ref" | "mutate" => ownership.initial_state == "available" && ownership.final_state == "available",
                "consume" => ownership.initial_state == "available" && ownership.final_state == "consumed",
                "input" => ownership.initial_state == "available" && ownership.final_state == "consumed",
                "destroy" => ownership.initial_state == "available" && ownership.final_state == "destroyed",
                "transfer" => {
                    (ownership.initial_state == "available" && ownership.final_state == "transferred")
                        || (ownership.initial_state == "unbound" && ownership.final_state == "available")
                }
                "replace_unique" => {
                    (ownership.initial_state == "available" && ownership.final_state == "replaced")
                        || (ownership.initial_state == "unbound" && ownership.final_state == "available")
                }
                "claim" => {
                    (ownership.initial_state == "available" && ownership.final_state == "claimed")
                        || (ownership.initial_state == "unbound" && ownership.final_state == "available")
                }
                "settle" => {
                    (ownership.initial_state == "available" && ownership.final_state == "settled")
                        || (ownership.initial_state == "unbound" && ownership.final_state == "available")
                }
                "create" | "create_unique" | "output" => ownership.initial_state == "unbound" && ownership.final_state == "available",
                _ => false,
            };
            if !valid || ownership.binding.is_empty() || ownership.ty.is_empty() {
                return typed_error(format!("typed ownership transition for '{}' is invalid", ownership.binding));
            }
        }
        if entry.obligations.iter().any(|obligation| !proof_ids.contains(obligation.as_str())) {
            return typed_error(format!("typed entry '{}' references an undischarged obligation", entry.id));
        }
        validate_ownership_bindings(entry, &locals)?;

        if lowering.typed_blocks.len() != entry.blocks.len() || lowering.typed_blocks.windows(2).any(|pair| pair[0].id >= pair[1].id) {
            return Err(CheckerError::new(
                CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                format!("typed entry '{}' does not have one canonical lowering binding per typed block", entry.id),
            ));
        }
        for typed_block in &entry.blocks {
            let expected_hash = canonical_hash("cellscript-typed-block-v1", typed_block)?;
            let mapped = record
                .blocks
                .iter()
                .filter(|block| block.owner_entry == entry.id && block.lowering_block_id == Some(typed_block.id))
                .collect::<Vec<_>>();
            let Some(binding) = lowering.typed_blocks.iter().find(|binding| binding.id == typed_block.id) else {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                    format!("typed entry '{}' block {} has no lowering binding", entry.id, typed_block.id),
                ));
            };
            let mapped_ids = mapped.iter().map(|block| block.id.as_str()).collect::<Vec<_>>();
            if binding.hash != expected_hash
                || binding.machine_block_ids.iter().map(String::as_str).ne(mapped_ids)
                || mapped.iter().any(|block| block.typed_block_hash.as_deref() != Some(expected_hash.as_str()))
            {
                return Err(CheckerError::new(
                    CheckerRejectionCode::V2420TypedMachineBindingInvalid,
                    format!("typed entry '{}' block {} has an invalid machine lowering binding", entry.id, typed_block.id),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_trusted_external_verifiers(
    typed: &TypedSemanticRecord,
    lowering: &VerifiedLoweringRecord,
) -> Result<(), CheckerError> {
    let binding_for_delegate =
        |block: &crate::schema::TypedSemanticBlock, delegate_index: usize| -> Option<(&'static str, &'static str, String)> {
            let [source_operation, hash_operation, delegate_operation] =
                block.operations.get(delegate_index.checked_sub(2)?..=delegate_index)?
            else {
                return None;
            };
            let source_call = source_operation.call.as_ref()?;
            let hash_call = hash_operation.call.as_ref()?;
            let delegate_call = delegate_operation.call.as_ref()?;
            let source_local = *source_operation.destinations.as_slice().first()?;
            if source_operation.destinations.len() != 1
                || source_call.contract != "versioned-runtime-helper"
                || hash_call.contract != "versioned-runtime-helper"
                || delegate_call.contract != "versioned-runtime-helper"
                || source_call.target != "__ckb_source_cell_dep"
                || hash_call.target != "__ckb_require_cell_data_hash"
                || hash_operation.operands.first()?.local != Some(source_local)
                || source_operation.operands.first()? != delegate_operation.operands.first()?
            {
                return None;
            }
            let (operation, adapter) = match delegate_call.target.as_str() {
                "__ckb_exec_cell_dep_u8_args" => ("exec", "u8-args-v1"),
                "__ckb_exec_cell_dep_hex4" => ("exec", "hex4-v1"),
                "__ckb_spawn_wait_cell_dep_hex4" => ("spawn-wait", "hex4-v1"),
                _ => return None,
            };
            let hash = match hash_operation.operands.get(1)?.constant.as_ref()? {
                TypedSemanticConstant::Hash(hash) => hash.clone(),
                _ => return None,
            };
            Some((operation, adapter, hash))
        };

    for verifier in &typed.trusted_external_verifiers {
        let canonical_hash = verifier.code_hash.len() == 64
            && verifier.code_hash.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if verifier.schema != "cellscript-trusted-external-verifier-v1"
            || verifier.version != 1
            || verifier.name.trim().is_empty()
            || verifier.scope.trim().is_empty()
            || !matches!(verifier.operation.as_str(), "exec" | "spawn-wait")
            || !matches!(
                (verifier.operation.as_str(), verifier.adapter.as_str()),
                ("exec", "u8-args-v1" | "hex4-v1") | ("spawn-wait", "hex4-v1")
            )
            || !canonical_hash
            || verifier.hash_type != "data"
            || verifier.source_identity.trim().is_empty()
            || verifier.source_identity.len() > 4_096
            || verifier.applicability.trim().is_empty()
            || verifier.applicability.len() > 4_096
            || verifier.trust_basis.trim().is_empty()
            || verifier.trust_basis.len() > 4_096
            || verifier.name.len() > 4_096
            || verifier.scope.len() > 4_096
            || verifier.guarantees.is_empty()
            || verifier.guarantees.len() > 64
            || verifier.guarantees.iter().any(|item| item.trim().is_empty() || item.len() > 4_096)
            || !verifier.guarantees.windows(2).all(|pair| pair[0] < pair[1])
            || verifier.identity_binding != "runtime-load-cell-data-hash-before-delegation-v1"
            || verifier.evidence_tier != "trusted-external"
            || verifier.compiler_proves_internal_semantics
        {
            return typed_error(format!("trusted external verifier '{}' has an invalid or overstated trust record", verifier.name));
        }
        let Some(entry) = typed.entries.iter().find(|entry| entry.id == verifier.scope) else {
            return typed_error(format!(
                "trusted external verifier '{}' references missing scope '{}'",
                verifier.name, verifier.scope
            ));
        };
        let sequence_bound = entry.blocks.iter().any(|block| {
            block.operations.iter().enumerate().any(|(index, _)| {
                binding_for_delegate(block, index).is_some_and(|(operation, adapter, hash)| {
                    operation == verifier.operation && adapter == verifier.adapter && hash == verifier.code_hash
                })
            })
        });
        let (feature, category) = if verifier.operation == "exec" {
            ("exec-target-and-replaced-continuation", "exec-delegation")
        } else {
            ("spawn-target-and-checked-child-exit", "spawn-delegation")
        };
        let proof_bound = lowering.proof_records.iter().any(|proof| {
            proof.entry_id == verifier.scope
                && proof.evidence_tier == "trusted-external"
                && proof.obligation == format!("{feature}:{category}:trusted-external")
        });
        if !sequence_bound || !proof_bound {
            return typed_error(format!(
                "trusted external verifier '{}' is not jointly bound to an exact data-hash check, delegated call, and trusted-external ProofPlan",
                verifier.name
            ));
        }
    }
    for entry in &typed.entries {
        let declared = typed.trusted_external_verifiers.iter().filter(|verifier| verifier.scope == entry.id).collect::<Vec<_>>();
        if declared.is_empty() {
            continue;
        }
        for block in &entry.blocks {
            for (index, operation) in block.operations.iter().enumerate() {
                let Some(call) = operation.call.as_ref() else { continue };
                if call.contract != "versioned-runtime-helper"
                    || !matches!(
                        call.target.as_str(),
                        "__ckb_exec_cell_dep_u8_args" | "__ckb_exec_cell_dep_hex4" | "__ckb_spawn_wait_cell_dep_hex4"
                    )
                {
                    continue;
                }
                let Some((delegation, adapter, hash)) = binding_for_delegate(block, index) else {
                    return typed_error(format!(
                        "typed entry '{}' contains delegated execution outside the required source/hash/delegate sequence",
                        entry.id
                    ));
                };
                if !declared
                    .iter()
                    .any(|verifier| verifier.operation == delegation && verifier.adapter == adapter && verifier.code_hash == hash)
                {
                    return typed_error(format!(
                        "typed entry '{}' delegates to an identity absent from its trusted external verifier records",
                        entry.id
                    ));
                }
            }
        }
    }
    let trusted_proof_count = lowering.proof_records.iter().filter(|proof| proof.evidence_tier == "trusted-external").count();
    if typed.trusted_external_verifiers.is_empty() != (trusted_proof_count == 0) {
        return typed_error("trusted-external ProofPlan evidence and typed verifier records do not agree");
    }
    Ok(())
}

pub(super) fn validate_semantic_foundation(typed: &TypedSemanticRecord, record: &VerifiedLoweringRecord) -> Result<(), CheckerError> {
    crate::bindings::validate(typed)?;
    let foundation = &typed.foundation;
    if foundation.schema != SEMANTIC_FOUNDATION_SCHEMA
        || foundation.version != SEMANTIC_FOUNDATION_VERSION
        || foundation.provenance.schema != PROVENANCE_GRAPH_SCHEMA
        || foundation.provenance.version != PROVENANCE_GRAPH_VERSION
    {
        return typed_error("semantic foundation or provenance DAG uses an unsupported schema".to_string());
    }
    if foundation.provenance.nodes.len() > 65_536 || foundation.provenance.bindings.len() > 262_144 {
        return typed_error("semantic foundation provenance DAG exceeds its bounded contract".to_string());
    }
    ensure_sorted_unique(&foundation.provenance.nodes, |node| node.id.as_str(), "provenance node")?;
    let node_ids = foundation.provenance.nodes.iter().map(|node| node.id.as_str()).collect::<BTreeSet<_>>();
    for node in &foundation.provenance.nodes {
        if node.id != canonical_hash("cellscript-value-provenance-node-v1", &node.provenance)? {
            return typed_error(format!("provenance node '{}' does not match its canonical contents", node.id));
        }
        if let ValueProvenance::Derived { operation, inputs } = &node.provenance
            && (operation.is_empty() || inputs.iter().any(|input| input == &node.id || !node_ids.contains(input.as_str())))
        {
            return typed_error(format!("derived provenance node '{}' has an invalid operation or input", node.id));
        }
    }
    let root_node_ids = foundation
        .provenance
        .nodes
        .iter()
        .filter(|node| !matches!(node.provenance, ValueProvenance::Derived { .. }))
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    validate_provenance_acyclic(&foundation.provenance.nodes)?;
    let entry_locals = typed
        .entries
        .iter()
        .flat_map(|entry| entry.locals.iter().map(move |local| ((entry.id.as_str(), local.id), local)))
        .collect::<BTreeMap<_, _>>();
    let required_locals =
        typed
            .entries
            .iter()
            .flat_map(|entry| {
                let entry_id = entry.id.as_str();
                entry.params.iter().map(move |param| (entry_id, param.binding_id)).chain(entry.blocks.iter().flat_map(move |block| {
                    block.operations.iter().flat_map(move |operation| {
                        operation.destinations.iter().copied().map(move |local_id| (entry_id, local_id)).chain(
                            operation.operands.iter().filter_map(move |operand| operand.local.map(|local_id| (entry_id, local_id))),
                        )
                    })
                }))
            })
            .collect::<BTreeSet<_>>();
    let mut bound_locals = BTreeSet::new();
    let mut previous_binding = None::<(&str, u32, &str)>;
    for binding in &foundation.provenance.bindings {
        let key = (binding.entry_id.as_str(), binding.local_id, binding.node_id.as_str());
        if previous_binding.is_some_and(|previous| previous >= key)
            || !node_ids.contains(binding.node_id.as_str())
            || !entry_locals.contains_key(&(binding.entry_id.as_str(), binding.local_id))
        {
            return typed_error("provenance bindings are not canonical or reference unknown typed locals".to_string());
        }
        bound_locals.insert((binding.entry_id.as_str(), binding.local_id));
        previous_binding = Some(key);
    }
    if !required_locals.is_subset(&bound_locals) {
        let missing = required_locals.difference(&bound_locals).map(|(entry, local)| format!("{entry}:{local}")).collect::<Vec<_>>();
        return typed_error(format!(
            "not every value-bearing typed local has at least one provenance binding: {}",
            missing.join(", ")
        ));
    }

    let contract = &foundation.entry_contract;
    let dispatch_label = match &contract.dispatch {
        EntryDispatchContract::SingleEntry => "single-entry",
        EntryDispatchContract::PolicyWitnessV1(policy) => {
            crate::policy::validate_policy_contract(policy, typed)?;
            if contract.script_role != "type"
                || contract.trigger != format!("type-group<{}>", policy.resource)
                || contract.exact_entry != "wrapper:_cellscript_entry"
                || contract.entry_payload_abi != crate::POLICY_PAYLOAD_ABI
                || contract.witness_placement_abi != crate::POLICY_PLACEMENT_ABI
                || contract.witness_placement_field != "input_type"
                || contract.witness_placement_source != crate::POLICY_WITNESS_SOURCE
                || !record.entries.iter().any(|entry| entry.id == contract.exact_entry && entry.kind == EntryKind::Wrapper)
            {
                return typed_error("policy dispatch entry does not bind its Type wrapper and witness ABI".to_string());
            }
            "policy-witness-v1"
        }
        EntryDispatchContract::ExplicitVersionedDispatch { selector_node_id, selector_type, variants, unknown_selector } => {
            if !root_node_ids.contains(selector_node_id.as_str())
                || selector_type.is_empty()
                || variants.is_empty()
                || unknown_selector != "reject"
                || variants.windows(2).any(|pair| (&pair[0].tag, &pair[0].entry_id) >= (&pair[1].tag, &pair[1].entry_id))
                || variants
                    .iter()
                    .any(|variant| !typed.entries.iter().any(|entry| entry.id == variant.entry_id) || variant.tag.is_empty())
            {
                return typed_error("explicit entry dispatch contract is incomplete or non-canonical".to_string());
            }
            "explicit-versioned-dispatch"
        }
    };
    let expected_contract_node = if matches!(contract.dispatch, EntryDispatchContract::PolicyWitnessV1(_)) {
        canonical_hash(
            "cellscript-semantic-node-entry-contract-v2",
            &(
                contract.script_role.as_str(),
                contract.trigger.as_str(),
                contract.exact_entry.as_str(),
                &contract.dispatch,
                contract.entry_payload_abi.as_str(),
                contract.witness_placement_abi.as_str(),
                contract.witness_placement_field.as_str(),
                contract.witness_placement_source.as_str(),
            ),
        )?
    } else {
        canonical_hash(
            "cellscript-semantic-node-entry-contract-v1",
            &(
                contract.script_role.as_str(),
                contract.trigger.as_str(),
                contract.exact_entry.as_str(),
                dispatch_label,
                contract.entry_payload_abi.as_str(),
                contract.witness_placement_abi.as_str(),
                contract.witness_placement_field.as_str(),
                contract.witness_placement_source.as_str(),
            ),
        )?
    };
    let trigger_valid = match contract.script_role.as_str() {
        "type" => {
            contract.trigger == "type-group"
                || contract
                    .trigger
                    .strip_prefix("type-group<")
                    .and_then(|trigger| trigger.strip_suffix('>'))
                    .is_some_and(|trigger| !trigger.is_empty())
        }
        "lock" => contract.trigger == "lock-group",
        "none" => contract.trigger == "none",
        _ => false,
    };
    if contract.semantic_node_id != expected_contract_node
        || !trigger_valid
        || (contract.exact_entry != "none"
            && !matches!(contract.dispatch, EntryDispatchContract::PolicyWitnessV1(_))
            && !typed.entries.iter().any(|entry| entry.id == contract.exact_entry))
        || contract.entry_payload_abi.is_empty()
        || contract.witness_placement_abi.is_empty()
        || contract.witness_placement_field.is_empty()
        || contract.witness_placement_source.is_empty()
    {
        return typed_error("artifact entry-selection contract is invalid".to_string());
    }

    ensure_sorted_unique(&foundation.roles, |role| role.role_id.as_str(), "semantic role")?;
    let role_ids = foundation.roles.iter().map(|role| role.role_id.as_str()).collect::<BTreeSet<_>>();
    let typed_types = typed.types.iter().map(|ty| (ty.name.as_str(), ty)).collect::<BTreeMap<_, _>>();
    for role in &foundation.roles {
        let expected_node = canonical_hash(
            "cellscript-semantic-node-role-v1",
            &(
                role.role_id.as_str(),
                role.entry_id.as_str(),
                role.binding.as_str(),
                role.ty.as_str(),
                role.direction.as_str(),
                role.source.as_str(),
                role.selector.as_str(),
                role.cardinality.as_str(),
                role.lock_or_type_role.as_str(),
                role.script_identity_policy.as_str(),
                role.schema_identity.as_str(),
                role.correspondence_policy.as_str(),
            ),
        )?;
        let schema_type = role.ty.strip_prefix('&').map(str::trim).unwrap_or(role.ty.as_str());
        let schema_type = bounded_collection_element(schema_type).unwrap_or(schema_type);
        if role.semantic_node_id != expected_node
            || !typed.entries.iter().any(|entry| entry.id == role.entry_id)
            || role.binding.is_empty()
            || !matches!(role.direction.as_str(), "input" | "output" | "read-only-dependency")
            || role.locality != "local"
            || role.selector.is_empty()
            || role.cardinality.is_empty()
            || !matches!(role.lock_or_type_role.as_str(), "lock" | "type")
            || typed_types.get(schema_type).is_none_or(|layout| layout.layout_hash != role.schema_identity)
        {
            return typed_error(format!("semantic role '{}' is incomplete or not schema-bound", role.role_id));
        }
    }

    ensure_sorted_unique(&foundation.dispositions, |item| item.id.as_str(), "Cell disposition")?;
    let mut disposed_roles = BTreeMap::new();
    for disposition in &foundation.dispositions {
        let expected_node = canonical_hash(
            "cellscript-semantic-node-disposition-v2",
            &(
                disposition.id.as_str(),
                disposition.entry_id.as_str(),
                disposition.input_role.as_deref(),
                disposition.output_role.as_deref(),
                &disposition.input,
                &disposition.output,
                &disposition.envelope,
                disposition.enforcement.as_str(),
                &disposition.execution,
            ),
        )?;
        let input_shape_valid = match &disposition.input {
            None => true,
            Some(InputDisposition::Successor { output_role }) => !output_role.is_empty(),
            Some(InputDisposition::Pooled { pool_id, accounting_obligation }) => {
                !pool_id.is_empty() && !accounting_obligation.is_empty()
            }
            Some(InputDisposition::Retired { absence_policy }) => !absence_policy.is_empty(),
            Some(InputDisposition::AuthorizationOnly { disposition_owner }) => !disposition_owner.is_empty(),
            Some(InputDisposition::Consumed { operation, boundary }) => !operation.is_empty() && !boundary.is_empty(),
        };
        let output_shape_valid = match &disposition.output {
            None => true,
            Some(OutputOrigin::SuccessorOf { input_role }) => !input_role.is_empty(),
            Some(OutputOrigin::Fresh { identity_policy }) => !identity_policy.is_empty(),
            Some(OutputOrigin::PoolResult { pool_id, accounting_obligation }) => {
                !pool_id.is_empty() && !accounting_obligation.is_empty()
            }
            Some(OutputOrigin::Created { operation }) => !operation.is_empty(),
        };
        let schema_role = disposition
            .output_role
            .as_ref()
            .or(disposition.input_role.as_ref())
            .and_then(|role_id| foundation.roles.iter().find(|role| &role.role_id == role_id));
        let exhaustive_fields_valid = schema_role.is_none_or(|role| {
            let schema_type = role.ty.strip_prefix('&').map(str::trim).unwrap_or(role.ty.as_str());
            let schema_type = bounded_collection_element(schema_type).unwrap_or(schema_type);
            let expected = typed_types
                .get(schema_type)
                .map(|layout| layout.fields.iter().map(|field| field.name.as_str()).collect::<BTreeSet<_>>())
                .unwrap_or_default();
            let actual = disposition.envelope.data_fields.iter().map(|field| field.field.as_str()).collect::<BTreeSet<_>>();
            disposition.envelope.completeness != "exhaustive" || actual == expected
        });
        if disposition.semantic_node_id != expected_node
            || !typed.entries.iter().any(|entry| entry.id == disposition.entry_id)
            || !input_shape_valid
            || !output_shape_valid
            || !exhaustive_fields_valid
            || disposition.input_role.is_none() != disposition.input.is_none()
            || disposition.output_role.is_none() != disposition.output.is_none()
            || disposition.input_role.iter().chain(disposition.output_role.iter()).any(|role| {
                let conditional = disposition.execution.is_some();
                !role_ids.contains(role.as_str())
                    || disposed_roles.insert(role.as_str(), conditional).is_some_and(|previous| !previous || !conditional)
            })
            || disposition.envelope.completeness.is_empty()
            || disposition.envelope.logical_identity.is_empty()
            || disposition.envelope.lock_script.is_empty()
            || disposition.envelope.type_script.is_empty()
            || disposition.envelope.capacity.is_empty()
            || disposition.envelope.cardinality.is_empty()
            || disposition.envelope.correspondence.is_empty()
            || disposition.envelope.data_fields.windows(2).any(|pair| pair[0].field >= pair[1].field)
            || disposition.envelope.data_fields.iter().any(|field| field.field.is_empty() || field.treatment.is_empty())
            || !matches!(
                disposition.enforcement.as_str(),
                "checked-static"
                    | "checked-runtime"
                    | "trusted-external"
                    | "runtime-helper-required"
                    | "builder-evidence-required"
                    | "metadata-only"
                    | "chain-evidence-required"
            )
        {
            return typed_error(format!("Cell disposition '{}' is incomplete, duplicated, or malformed", disposition.id));
        }
        match (&disposition.input, &disposition.output) {
            (Some(InputDisposition::Successor { output_role }), Some(OutputOrigin::SuccessorOf { input_role }))
                if disposition.output_role.as_deref() == Some(output_role)
                    && disposition.input_role.as_deref() == Some(input_role) => {}
            (Some(InputDisposition::Successor { .. }), _) | (_, Some(OutputOrigin::SuccessorOf { .. })) => {
                return typed_error(format!("successor disposition '{}' is not bidirectionally linked", disposition.id));
            }
            _ => {}
        }
        if let Some(execution) = &disposition.execution {
            successor_relations::validate_execution(disposition, execution, typed, foundation)?;
        } else if successor_relations::requires_execution(disposition, typed, foundation) {
            return typed_error(format!("successor '{}' claims preservation without executable evidence", disposition.id));
        }
    }
    for role in &foundation.roles {
        if role.direction != "read-only-dependency"
            && !disposed_roles.contains_key(role.role_id.as_str())
            && !successor_relations::rejects_every_path(typed, &role.entry_id)
        {
            return typed_error(format!("Cell role '{}' has no exhaustive disposition record", role.role_id));
        }
    }
    successor_relations::validate_paths(typed, foundation)?;
    let mut pooled_inputs = BTreeMap::new();
    let mut pooled_outputs = BTreeMap::new();
    for disposition in &foundation.dispositions {
        if let Some(InputDisposition::Pooled { pool_id, accounting_obligation }) = &disposition.input {
            if !valid_pool_accounting_contract(pool_id, accounting_obligation) {
                return typed_error(format!("pooled input '{}' has a malformed accounting contract", pool_id));
            }
            if pooled_inputs
                .insert(pool_id.as_str(), accounting_obligation.as_str())
                .is_some_and(|existing| existing != accounting_obligation.as_str())
            {
                return typed_error(format!("pooled input '{}' changes its accounting obligation", pool_id));
            }
        }
        if let Some(OutputOrigin::PoolResult { pool_id, accounting_obligation }) = &disposition.output {
            if !valid_pool_accounting_contract(pool_id, accounting_obligation) {
                return typed_error(format!("pooled output '{}' has a malformed accounting contract", pool_id));
            }
            if pooled_outputs
                .insert(pool_id.as_str(), accounting_obligation.as_str())
                .is_some_and(|existing| existing != accounting_obligation.as_str())
            {
                return typed_error(format!("pooled output '{}' changes its accounting obligation", pool_id));
            }
        }
    }
    if pooled_inputs != pooled_outputs {
        return typed_error("pooled dispositions must have matching input/output pools and accounting obligations");
    }

    ensure_sorted_unique(&foundation.claims, |claim| claim.id.as_str(), "semantic claim")?;
    for claim in &foundation.claims {
        let expected_node = canonical_hash(
            "cellscript-semantic-node-claim-v1",
            &(
                claim.id.as_str(),
                claim.entry_id.as_str(),
                claim.category.as_str(),
                claim.statement.as_str(),
                claim.enforcement.as_str(),
                claim.on_chain_checked,
                claim.evidence_reference.as_str(),
                &claim.execution,
            ),
        )?;
        let evidence_valid = match &claim.execution {
            Some(execution) => validate_claim_execution(claim, execution, typed, foundation),
            None if claim.category == "audit" => {
                let prefix = "expected external policy evidence for ";
                claim.enforcement == "metadata-only"
                    && !claim.on_chain_checked
                    && claim.evidence_reference == "audit:external-policy"
                    && claim.statement.starts_with(prefix)
                    && claim.statement.len() > prefix.len()
            }
            None => claim.evidence_reference.starts_with("proof-plan:") && claim.evidence_reference.len() > "proof-plan:".len(),
        };
        if claim.semantic_node_id != expected_node
            || !typed.entries.iter().any(|entry| entry.id == claim.entry_id)
            || claim.category.is_empty()
            || claim.statement.is_empty()
            || claim.evidence_reference.is_empty()
            || !evidence_valid
            || !matches!(
                claim.enforcement.as_str(),
                "checked-static"
                    | "checked-runtime"
                    | "trusted-external"
                    | "runtime-helper-required"
                    | "builder-evidence-required"
                    | "metadata-only"
                    | "chain-evidence-required"
            )
            || (claim.on_chain_checked
                && !matches!(claim.enforcement.as_str(), "checked-static" | "checked-runtime" | "trusted-external"))
        {
            return typed_error(format!("semantic claim '{}' has an invalid enforcement classification", claim.id));
        }
    }

    ensure_sorted_unique(&foundation.operation_nodes, |legacy| legacy.id.as_str(), "operation semantic node")?;
    for legacy in &foundation.operation_nodes {
        let expected_node = canonical_hash(
            "cellscript-semantic-node-operation-v1",
            &(legacy.id.as_str(), legacy.kind.as_str(), legacy.meaning.as_str(), legacy.boundary.as_str()),
        )?;
        if legacy.semantic_node_id != expected_node
            || legacy.kind.is_empty()
            || legacy.meaning.is_empty()
            || legacy.boundary.is_empty()
        {
            return typed_error(format!("operation semantic node '{}' is incomplete", legacy.id));
        }
    }

    let entry_programs = typed
        .entries
        .iter()
        .map(|entry| {
            (
                entry.id.as_str(),
                entry.kind.as_str(),
                entry.name.as_str(),
                &entry.params,
                &entry.cell_bindings,
                entry.return_type.as_str(),
                &entry.effects,
                entry.entry_block,
                &entry.locals,
                &entry.blocks,
                &entry.borrows,
                &entry.ownership,
                &entry.obligations,
                entry.zk_origins.iter().map(|origin| (origin.policy.as_str(), &origin.fields)).collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let core_semantic_id = canonical_hash(
        "cellscript-core-semantic-id-v3",
        &(
            typed.failure_semantics,
            &typed.types,
            &entry_programs,
            &foundation.roles,
            &foundation.dispositions,
            &foundation.claims,
            &foundation.operation_nodes,
        ),
    )?;
    let provenance_roots = foundation
        .provenance
        .nodes
        .iter()
        .filter(|node| !matches!(node.provenance, ValueProvenance::Derived { .. }))
        .map(|node| node.id.as_str())
        .collect::<Vec<_>>();
    let entry_contract_id = canonical_hash(
        "cellscript-entry-contract-id-v1",
        &(
            core_semantic_id.as_str(),
            &foundation.entry_contract,
            provenance_roots,
            foundation.entry_contract.entry_payload_abi.as_str(),
            foundation.entry_contract.witness_placement_abi.as_str(),
        ),
    )?;
    let artifact_contract_id =
        canonical_hash("cellscript-artifact-contract-id-v1", &(entry_contract_id.as_str(), &foundation.artifact_contract))?;
    if foundation.identities.core_semantic_id != core_semantic_id
        || foundation.identities.entry_contract_id != entry_contract_id
        || foundation.identities.artifact_contract_id != artifact_contract_id
        || foundation.artifact_contract.target_profile != record.target_profile
        || foundation.artifact_contract.artifact_format != record.artifact_format
        || foundation.artifact_contract.lowering_record_schema != LOWERING_RECORD_SCHEMA
        || foundation.artifact_contract.typed_semantics_schema != TYPED_SEMANTICS_SCHEMA
    {
        return typed_error("layered semantic identities do not match their canonical projections".to_string());
    }
    Ok(())
}

pub(super) fn valid_pool_accounting_contract(pool_id: &str, accounting_obligation: &str) -> bool {
    let pool_parts = pool_id.split(':').collect::<Vec<_>>();
    if pool_parts.len() != 3 || pool_parts[0] != "pool" || pool_parts[1..].iter().any(|part| part.is_empty()) {
        return false;
    }
    let prefix = "checked-u128-field-sum-equality:";
    accounting_obligation.split('+').all(|obligation| obligation.strip_prefix(prefix).is_some_and(|field| !field.is_empty()))
}

pub(super) fn validate_claim_execution(
    claim: &SemanticClaim,
    execution: &ClaimExecutionBinding,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> bool {
    if claim.category != "entry-condition"
        || claim.enforcement != "checked-runtime"
        || !claim.on_chain_checked
        || !claim.statement.starts_with("require ")
        || claim.evidence_reference != format!("typed-entry:{}:block:{}:branch-condition", claim.entry_id, execution.condition_block)
    {
        return false;
    }
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == claim.entry_id) else {
        return false;
    };
    let Some(condition_block) = entry.blocks.iter().find(|block| block.id == execution.condition_block) else {
        return false;
    };
    let Some(success_block) = entry.blocks.iter().find(|block| block.id == execution.success_block) else {
        return false;
    };
    let Some(failure_block) = entry.blocks.iter().find(|block| block.id == execution.failure_block) else {
        return false;
    };
    if condition_block.terminator != "branch"
        || condition_block.successors != [execution.success_block, execution.failure_block]
        || success_block.id == failure_block.id
        || failure_block.terminator != "verifier-failure"
        || failure_block
            .runtime_error
            .as_ref()
            .is_none_or(|error| error.code != execution.failure_error_code || error.name != "assertion-failed")
    {
        return false;
    }
    let Some(condition) = condition_block
        .operations
        .iter()
        .rev()
        .find(|operation| operation.opcode == "branch-condition")
        .and_then(|operation| operation.operands.as_slice().first().filter(|_| operation.operands.len() == 1))
    else {
        return false;
    };
    let node_matches = if let Some(local_id) = condition.local {
        foundation.provenance.bindings.iter().any(|binding| {
            binding.entry_id == claim.entry_id && binding.local_id == local_id && binding.node_id == execution.condition_node_id
        })
    } else if let Some(constant) = &condition.constant {
        canonical_hash("cellscript-value-provenance-node-v1", &ValueProvenance::Constant { declaration: format!("{constant:?}") })
            .is_ok_and(|node_id| node_id == execution.condition_node_id)
    } else {
        false
    };
    node_matches
        && foundation.provenance.nodes.iter().any(|node| node.id == execution.condition_node_id)
        && execution.failure_error_code != 0
}

pub(super) fn validate_provenance_acyclic(nodes: &[ProvenanceNode]) -> Result<(), CheckerError> {
    let edges = nodes
        .iter()
        .map(|node| {
            let inputs = match &node.provenance {
                ValueProvenance::Derived { inputs, .. } => inputs.iter().map(String::as_str).collect(),
                _ => Vec::new(),
            };
            (node.id.as_str(), inputs)
        })
        .collect::<BTreeMap<_, Vec<_>>>();
    let mut complete = BTreeSet::new();
    for root in edges.keys() {
        let mut active = BTreeSet::new();
        let mut stack = vec![(*root, false, 0usize)];
        while let Some((node, exiting, depth)) = stack.pop() {
            if depth > 256 {
                return typed_error("provenance DAG exceeds the maximum depth of 256".to_string());
            }
            if exiting {
                active.remove(node);
                complete.insert(node);
                continue;
            }
            if complete.contains(node) {
                continue;
            }
            if !active.insert(node) {
                return typed_error(format!("provenance DAG contains a cycle at '{node}'"));
            }
            stack.push((node, true, depth));
            if let Some(inputs) = edges.get(node) {
                for input in inputs.iter().rev() {
                    stack.push((*input, false, depth + 1));
                }
            }
        }
    }
    Ok(())
}

pub(super) fn bounded_collection_element(ty: &str) -> Option<&str> {
    ty.strip_prefix("BoundedCellSet<")
        .and_then(|value| value.strip_suffix('>'))
        .and_then(|value| value.rsplit_once(','))
        .map(|(element, _)| element.trim())
}

pub(super) fn validate_typed_type(ty: &TypedSemanticType) -> Result<(), CheckerError> {
    let nested_cell = matches!(ty.kind.as_str(), "resource" | "shared" | "receipt")
        && ty.fields.iter().any(|field| nested_source_parts(&field.ty, "NestedValues", 4).is_some());
    let order = ty.fields.iter().filter_map(|field| field.molecule_field_index).collect::<BTreeSet<_>>();
    if (nested_cell && (order.len() != ty.fields.len() || order.iter().copied().ne(0..ty.fields.len() as u32)))
        || (!nested_cell && !order.is_empty())
    {
        return typed_error(format!("typed type '{}' has missing, duplicate or unsupported Molecule field indices", ty.name));
    }
    if ty.name.is_empty()
        || ty.layout_hash.is_empty()
        || !matches!(ty.kind.as_str(), "resource" | "shared" | "receipt" | "struct" | "enum")
        || ty.identity_policy.is_empty()
    {
        return typed_error(format!("typed type '{}' has an invalid kind or layout identity", ty.name));
    }
    if !strictly_sorted(&ty.capabilities) && !ty.capabilities.is_empty() {
        return typed_error(format!("typed type '{}' capabilities are not canonical", ty.name));
    }
    if !matches!(ty.identity_policy.as_str(), "none" | "ckb-type-id" | "script-args" | "singleton-type")
        && !ty.identity_policy.starts_with("field:")
    {
        return typed_error(format!("typed type '{}' has an invalid identity policy", ty.name));
    }

    if ty.kind == "enum" {
        if !ty.fields.is_empty() || ty.tag_width_bytes.is_none_or(|width| width == 0) || ty.variants.is_empty() {
            return typed_error(format!("typed enum '{}' has an incomplete tagged layout", ty.name));
        }
        let mut names = BTreeSet::new();
        let mut tags = BTreeSet::new();
        for variant in &ty.variants {
            if variant.name.is_empty() || !names.insert(variant.name.as_str()) || !tags.insert(variant.tag) {
                return typed_error(format!("typed enum '{}' has duplicate or empty variants", ty.name));
            }
            for (index, field) in variant.fields.iter().enumerate() {
                if field.index != u32::try_from(index).unwrap_or(u32::MAX)
                    || field.ty.is_empty()
                    || field.width_bytes == 0
                    || ty.encoded_size.is_none_or(|size| field.offset.saturating_add(field.width_bytes) > size)
                {
                    return typed_error(format!("typed enum '{}::{}' has an invalid payload layout", ty.name, variant.name));
                }
            }
        }
    } else {
        if ty.tag_width_bytes.is_some() || !ty.variants.is_empty() {
            return typed_error(format!("non-enum typed type '{}' carries enum layout state", ty.name));
        }
        let fixed_layout = ty.encoded_size.is_some() && ty.fields.iter().all(|field| field.width_bytes.is_some());
        let mut previous_end = 0u32;
        for field in &ty.fields {
            if field.name.is_empty() || field.ty.is_empty() || (fixed_layout && field.offset < previous_end) {
                return typed_error(format!("typed type '{}' has overlapping or incomplete field layout", ty.name));
            }
            previous_end = field.offset.saturating_add(field.width_bytes.unwrap_or(0));
        }
        if fixed_layout && ty.encoded_size.is_some_and(|size| previous_end > size) {
            return typed_error(format!("typed type '{}' fields exceed its encoded size", ty.name));
        }
    }

    let expected_layout_hash = canonical_hash(
        "cellscript-typed-layout-v2",
        &(ty.kind.as_str(), ty.encoded_size, &ty.fields, ty.tag_width_bytes, &ty.variants, &ty.capabilities, &ty.identity_policy),
    )?;
    if ty.layout_hash != expected_layout_hash {
        return typed_error(format!("typed type '{}' layout hash does not match its canonical layout", ty.name));
    }
    Ok(())
}

pub(super) fn lowering_entry_kind(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Action => "action",
        EntryKind::Lock => "lock",
        EntryKind::Helper => "helper",
        EntryKind::Runtime => "runtime",
        EntryKind::Wrapper => "wrapper",
    }
}

pub(super) fn constant_type(constant: &TypedSemanticConstant) -> Option<String> {
    let scalar = |value: &String, max: u128, ty: &str| {
        value.parse::<u128>().ok().filter(|parsed| *parsed <= max && parsed.to_string() == *value).map(|_| ty.to_string())
    };
    match constant {
        TypedSemanticConstant::Unit => Some("unit".to_string()),
        TypedSemanticConstant::U8(value) => scalar(value, u8::MAX.into(), "u8"),
        TypedSemanticConstant::U16(value) => scalar(value, u16::MAX.into(), "u16"),
        TypedSemanticConstant::U32(value) => scalar(value, u32::MAX.into(), "u32"),
        TypedSemanticConstant::U64(value) => scalar(value, u64::MAX.into(), "u64"),
        TypedSemanticConstant::U128(value) => scalar(value, u128::MAX, "u128"),
        TypedSemanticConstant::Bool(_) => Some("bool".to_string()),
        TypedSemanticConstant::Address(value) | TypedSemanticConstant::Hash(value)
            if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) =>
        {
            Some(if matches!(constant, TypedSemanticConstant::Address(_)) { "address" } else { "hash" }.to_string())
        }
        TypedSemanticConstant::Address(_) | TypedSemanticConstant::Hash(_) => None,
        TypedSemanticConstant::Array(values) => {
            let first = match values.first() {
                Some(value) => constant_type(value)?,
                None => "unit".to_string(),
            };
            if values.iter().all(|value| constant_type(value).as_deref() == Some(first.as_str())) {
                Some(format!("[{first}; {}]", values.len()))
            } else {
                None
            }
        }
    }
}

pub(super) fn validate_typed_operation(
    operation: &TypedSemanticOperation,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    types: &BTreeMap<&str, &TypedSemanticType>,
    entries: &BTreeMap<&str, &TypedSemanticEntry>,
    entry: &TypedSemanticEntry,
    block: &TypedSemanticBlock,
    record: &TypedSemanticRecord,
) -> Result<(), CheckerError> {
    let shape =
        |destinations: usize, operands: usize| operation.destinations.len() == destinations && operation.operands.len() == operands;
    let destination_type =
        |index: usize| operation.destinations.get(index).and_then(|id| locals.get(id)).map(|local| local.ty.as_str());
    let operand_type = |index: usize| operation.operands.get(index).map(|operand| operand.ty.as_str());
    let optional_types_equivalent = |left: Option<&str>, right: Option<&str>| {
        left.zip(right).is_some_and(|(left, right)| {
            same_packed_domains(left, right)
                && (canonical_abi_type(left) == canonical_abi_type(right) || same_nominal_type(left, right, record))
        })
    };
    let none_detail = matches!(operation.detail, TypedSemanticOperationDetail::None);
    let fail = || typed_error(format!("typed operation '{}' has an invalid shape, detail, or type rule", operation.opcode));

    // Nested values are borrowed immutable spans. A matching pointer-sized
    // representation does not authorize collection mutation, construction,
    // pointer equality, calls or returning the view from its invocation.
    let has_nested_value = operation.operands.iter().any(|operand| is_nested_source_type(&operand.ty))
        || operation.destinations.iter().any(|id| locals.get(id).is_some_and(|local| is_nested_source_type(&local.ty)));
    let nested_cell_create = matches!(operation.opcode.as_str(), "create" | "create-unique" | "replace-unique")
        && operation.operands.iter().filter(|operand| is_nested_source_type(&operand.ty)).all(|operand| {
            nested_source_parts(&operand.ty, "NestedValues", 4).is_some() && nested_index_type(&operand.ty, types).is_some()
        });
    let nested_helper_call = operation.opcode == "call"
        && operation.call.as_ref().is_some_and(|call| {
            call.contract == "typed-local"
                && entries
                    .get(call.target.as_str())
                    .is_some_and(|callee| callee.kind == "helper" && callee.effects.allows_observation())
        })
        && operation
            .operands
            .iter()
            .filter(|operand| is_nested_source_type(&operand.ty))
            .all(|operand| !canonical_abi_type(&operand.ty).starts_with('&'))
        && operation.destinations.iter().all(|id| locals.get(id).is_some_and(|local| !is_nested_source_type(&local.ty)));
    if has_nested_value
        && !nested_cell_create
        && !nested_helper_call
        && !matches!(operation.opcode.as_str(), "load-var" | "store-var" | "move" | "index" | "field-access" | "length")
    {
        return fail();
    }

    match operation.opcode.as_str() {
        "load-const" => {
            let TypedSemanticOperationDetail::Constant { value } = &operation.detail else { return fail() };
            let constant_type = constant_type(value);
            let destination_type = destination_type(0);
            let encoded_unit_enum = destination_type.and_then(|ty| types.get(ty)).is_some_and(|layout| {
                layout.kind == "enum"
                    && matches!(value, TypedSemanticConstant::U64(tag) if tag.parse::<u32>().ok().is_some_and(|tag| {
                        layout.variants.iter().any(|variant| variant.tag == tag && variant.fields.is_empty())
                    }))
            });
            let context_typed_empty_array = matches!(value, TypedSemanticConstant::Array(values) if values.is_empty())
                && destination_type.is_some_and(is_zero_length_array_type);
            // IR stores nonnegative i32 literals in the U32 constant variant.
            // Only values representable without changing sign may acquire the
            // contextual signed type; arbitrary u32 bit patterns are not i32
            // literals. Negative values are produced by a typed unary operation.
            let context_typed_i32 = destination_type == Some("i32")
                && matches!(value, TypedSemanticConstant::U32(value) if value.parse::<u32>().ok().is_some_and(|value| value <= i32::MAX as u32));
            if !shape(1, 0)
                || operation.call.is_some()
                || (constant_type.as_deref() != destination_type
                    && !encoded_unit_enum
                    && !context_typed_empty_array
                    && !context_typed_i32)
            {
                return typed_error(format!(
                    "typed load-const has an invalid shape or type: constant type {:?}, destination type {:?}",
                    constant_type, destination_type
                ));
            }
        }
        "load-var" => {
            let TypedSemanticOperationDetail::Binding { name } = &operation.detail else { return fail() };
            if !shape(1, 0) || name.is_empty() || operation.call.is_some() {
                return fail();
            }
        }
        "store-var" => {
            let TypedSemanticOperationDetail::Binding { name } = &operation.detail else { return fail() };
            if !shape(0, 1) || name.is_empty() || operation.call.is_some() {
                return fail();
            }
        }
        "binary" => {
            let TypedSemanticOperationDetail::BinaryOperator { operator } = &operation.detail else { return fail() };
            let encoded_unit_enum_comparison = matches!(operator.as_str(), "eq" | "ne")
                && destination_type(0) == Some("bool")
                && operation.operands.iter().enumerate().any(|(enum_index, operand)| {
                    let Some(layout) = types.get(operand.ty.as_str()).filter(|layout| layout.kind == "enum") else {
                        return false;
                    };
                    let Some(TypedSemanticConstant::U64(tag)) =
                        operation.operands.get(1_usize.saturating_sub(enum_index)).and_then(|operand| operand.constant.as_ref())
                    else {
                        return false;
                    };
                    tag.parse::<u32>()
                        .ok()
                        .is_some_and(|tag| layout.variants.iter().any(|variant| variant.tag == tag && variant.fields.is_empty()))
                });
            if !shape(1, 2)
                || operation.call.is_some()
                || (!validate_binary_types(operator, operand_type(0), operand_type(1), destination_type(0))
                    && !encoded_unit_enum_comparison)
            {
                return typed_error(format!(
                    "typed binary '{}' has invalid operands {:?}, {:?} or destination {:?}",
                    operator,
                    operand_type(0),
                    operand_type(1),
                    destination_type(0)
                ));
            }
        }
        "unary" => {
            let TypedSemanticOperationDetail::UnaryOperator { operator } = &operation.detail else { return fail() };
            if !shape(1, 1) || operation.call.is_some() || !validate_unary_types(operator, operand_type(0), destination_type(0)) {
                return fail();
            }
        }
        "field-access" => {
            let TypedSemanticOperationDetail::Field { name } = &operation.detail else { return fail() };
            let owner_type = operand_type(0).map(strip_reference).unwrap_or_default();
            let owner = types.get(owner_type);
            let named_field_type =
                owner.and_then(|owner| owner.fields.iter().find(|field| field.name == *name)).map(|field| field.ty.as_str());
            let tuple_field_type = tuple_field_type(owner_type, name);
            let builtin_bytes_field_type = (name == "0"
                && matches!(canonical_abi_type(owner_type).as_str(), "address" | "hash" | "ScriptHash"))
            .then_some("[u8; 32]");
            let nested_field = nested_record_field_type(owner_type, name, types);
            let data_field = crate::cell_data_view::field_type(owner_type, name, types);
            let selected_role = crate::cell_selection::project_role_type(owner_type, name);
            let field_type = named_field_type
                .or(tuple_field_type.as_deref())
                .or(builtin_bytes_field_type)
                .or(nested_field.as_deref())
                .or(data_field.as_deref())
                .or(selected_role.as_deref());
            if !shape(1, 1) || !optional_types_equivalent(field_type, destination_type(0)) || operation.call.is_some() {
                return fail();
            }
        }
        "index" => {
            let owner = operand_type(0).unwrap_or_default();
            let element = collection_element_type(owner).or_else(|| nested_index_type(owner, types));
            if !shape(1, 2)
                || !none_detail
                || operation.call.is_some()
                || !is_integer_type(operand_type(1).unwrap_or_default())
                || (is_nested_source_type(owner) && !matches!(operand_type(1), Some("u8" | "u16" | "u32" | "u64")))
                || !optional_types_equivalent(element.as_deref(), destination_type(0))
            {
                return fail();
            }
        }
        "length" | "collection-capacity" => {
            if operand_type(0).is_some_and(is_nested_source_type)
                && (operation.opcode != "length" || nested_index_type(operand_type(0).unwrap(), types).is_none())
            {
                return fail();
            }
            if !shape(1, 1) || !none_detail || destination_type(0) != Some("u64") || operation.call.is_some() {
                return fail();
            }
        }
        "type-hash" => {
            if !shape(1, 1) || !none_detail || destination_type(0) != Some("hash") || operation.call.is_some() {
                return fail();
            }
        }
        "allocated-value" => {
            if !crate::allocated_values::valid_operation(operation, destination_type(0), types) {
                return typed_error(format!(
                    "allocated operation has invalid declaration, arguments or result: {:?}",
                    operation.detail
                ));
            }
        }
        "collection-new" => {
            let TypedSemanticOperationDetail::Collection { declared_type } = &operation.detail else { return fail() };
            if operation.destinations.len() != 1
                || operation.operands.len() > 1
                || !declared_collection_type_matches(declared_type, destination_type(0).unwrap_or_default())
                || operation.operands.first().is_some_and(|operand| !is_integer_type(&operand.ty))
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "collection-push" | "collection-contains" => {
            let expected_destinations = usize::from(operation.opcode == "collection-contains");
            let element = collection_element_type(operand_type(0).unwrap_or_default());
            if !shape(expected_destinations, 2)
                || !none_detail
                || !optional_types_equivalent(element.as_deref(), operand_type(1))
                || (operation.opcode == "collection-contains" && destination_type(0) != Some("bool"))
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "collection-extend" => {
            let collection_element = collection_element_type(operand_type(0).unwrap_or_default());
            let slice_element = collection_element_type(operand_type(1).unwrap_or_default());
            if !shape(0, 2)
                || !none_detail
                || !optional_types_equivalent(collection_element.as_deref(), slice_element.as_deref())
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "collection-clear" | "collection-reverse" => {
            if !shape(0, 1) || !none_detail || operation.call.is_some() {
                return fail();
            }
        }
        "collection-remove" => {
            if !shape(1, 2)
                || !none_detail
                || !is_integer_type(operand_type(1).unwrap_or_default())
                || !optional_types_equivalent(
                    collection_element_type(operand_type(0).unwrap_or_default()).as_deref(),
                    destination_type(0),
                )
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "collection-insert" | "collection-set" => {
            if !shape(0, 3)
                || !none_detail
                || !is_integer_type(operand_type(1).unwrap_or_default())
                || !optional_types_equivalent(collection_element_type(operand_type(0).unwrap_or_default()).as_deref(), operand_type(2))
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "collection-pop" => {
            if !shape(1, 1)
                || !none_detail
                || !optional_types_equivalent(
                    collection_element_type(operand_type(0).unwrap_or_default()).as_deref(),
                    destination_type(0),
                )
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "collection-truncate" => {
            if !shape(0, 2) || !none_detail || !is_integer_type(operand_type(1).unwrap_or_default()) || operation.call.is_some() {
                return fail();
            }
        }
        "collection-swap" => {
            if !shape(0, 3)
                || !none_detail
                || !is_integer_type(operand_type(1).unwrap_or_default())
                || !is_integer_type(operand_type(2).unwrap_or_default())
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "bounded-cell-load" => {
            let TypedSemanticOperationDetail::Collection { declared_type } = &operation.detail else { return fail() };
            let element_type = destination_type(0).unwrap_or_default();
            let declared_contract = declared_type
                .strip_prefix("BoundedCellSet<")
                .and_then(|value| value.strip_suffix('>'))
                .and_then(|value| value.rsplit_once(','))
                .is_some_and(|(element, maximum)| {
                    element.trim() == element_type
                        && maximum.trim().parse::<usize>().is_ok_and(|maximum| (1..=1024).contains(&maximum))
                });
            let fixed_resource = types
                .get(element_type)
                .is_some_and(|ty| ty.kind == "resource" && ty.encoded_size.is_some_and(|width| (1..=512).contains(&width)));
            if !shape(2, 1)
                || destination_type(1) != Some("bool")
                || !is_integer_type(operand_type(0).unwrap_or_default())
                || !declared_contract
                || !fixed_resource
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "verify-exact-cell-roles" => {
            let TypedSemanticOperationDetail::ExactRoleSelection { contract } = &operation.detail else { return fail() };
            let result = crate::cell_selection::selection_type(contract);
            if !(shape(0, 0) || shape(1, 0) && destination_type(0) == Some(result.as_str()))
                || operation.call.is_some()
                || contract.validate().is_err()
            {
                return fail();
            }
            if crate::cell_selection::layout::derive(contract, types).is_err() {
                return fail();
            }
        }
        "read-exact-cell-role" => {
            let TypedSemanticOperationDetail::ExactRoleRead { role, read } = &operation.detail else { return fail() };
            let Some(selection) = operand_type(0).and_then(crate::cell_selection::parse_selection_type) else {
                return fail();
            };
            let roles = &selection.schemas;
            let Some(element) = roles.get(*role as usize) else { return fail() };
            let result = read.result_type(element);
            if !(1..=8).contains(&roles.len())
                || !shape(1, if read.indexed() { 2 } else { 1 })
                || destination_type(0) != Some(result.as_str())
                || operation.call.is_some()
                || (selection.catalog.is_some() && selection.selected != Some(*role))
                || (read.indexed() && operand_type(1) != Some("u64"))
                || roles.iter().any(|role| {
                    !types.get(role.as_str()).is_some_and(|ty| matches!(ty.kind.as_str(), "resource" | "shared" | "receipt"))
                })
            {
                return fail();
            }
        }
        "bounded-plan-load" => {
            let TypedSemanticOperationDetail::Collection { declared_type } = &operation.detail else { return fail() };
            let element_type = destination_type(0).unwrap_or_default();
            let declared_contract = declared_type
                .strip_prefix("BoundedList<")
                .and_then(|value| value.strip_suffix('>'))
                .and_then(|value| value.rsplit_once(','))
                .is_some_and(|(element, maximum)| {
                    element.trim() == element_type
                        && maximum.trim().parse::<usize>().is_ok_and(|maximum| (1..=1024).contains(&maximum))
                });
            let plan_operand_matches =
                operation.operands.first().map(|operand| operand.ty.as_str()).is_some_and(|ty| ty == declared_type);
            let fixed_struct =
                types.get(element_type).is_some_and(|ty| ty.kind == "struct" && ty.encoded_size.is_some_and(|width| width > 0));
            if !shape(2, 2)
                || destination_type(1) != Some("bool")
                || !plan_operand_matches
                || !is_integer_type(operand_type(1).unwrap_or_default())
                || !declared_contract
                || !fixed_struct
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "bounded-output-verify" => {
            let TypedSemanticOperationDetail::Create { pattern } = &operation.detail else { return fail() };
            let Some(layout) = types.get(pattern.ty.as_str()).filter(|layout| layout.kind == "resource") else { return fail() };
            if !operation.destinations.is_empty()
                || operation.operands.is_empty()
                || !is_integer_type(operand_type(0).unwrap_or_default())
                || pattern.operation != "bounded-create"
                || pattern.binding.is_empty()
                || pattern.identity != "none"
                || !pattern.has_lock
                || pattern.field_names.len() + 2 != operation.operands.len()
                || operation.call.is_some()
            {
                return fail();
            }
            let mut names = BTreeSet::new();
            for (field_index, field_name) in pattern.field_names.iter().enumerate() {
                let Some(field) = layout.fields.iter().find(|field| field.name == *field_name) else { return fail() };
                if !names.insert(field_name.as_str())
                    || canonical_abi_type(&operation.operands[field_index + 1].ty) != canonical_abi_type(&field.ty)
                {
                    return fail();
                }
            }
            if names.len() != layout.fields.len()
                || !layout.fields.iter().all(|field| names.contains(field.name.as_str()))
                || !matches!(
                    canonical_abi_type(&operation.operands.last().expect("non-empty operands").ty).as_str(),
                    "address" | "hash"
                )
            {
                return fail();
            }
        }
        "bounded-output-end" => {
            if !shape(0, 1) || !none_detail || !is_integer_type(operand_type(0).unwrap_or_default()) || operation.call.is_some() {
                return fail();
            }
        }
        "call" => {
            if !none_detail {
                return fail();
            }
            let Some(call) = &operation.call else { return fail() };
            // The wire contract preserves source/runtime provenance. A local
            // definition may legally use a runtime-looking label; validate its
            // declared callee rather than assigning helper semantics by name.
            if call.contract == "typed-local" {
                let Some(callee) = entries.get(call.target.as_str()) else { return fail() };
                if call.params != callee.params.iter().map(|param| param.ty.clone()).collect::<Vec<_>>()
                    || call.return_type != callee.return_type
                {
                    return fail();
                }
                return Ok(());
            }
            if call.contract != "versioned-runtime-helper" {
                return fail();
            }
            let is_exact_handle_call = matches!(
                call.target.as_str(),
                "__ckb_require_cell_lock_exact_handle"
                    | "__ckb_require_cell_type_exact_handle"
                    | "__ckb_require_cell_dep_exact_verifier_handle"
            );
            let is_deployment_line_handle_call = matches!(
                call.target.as_str(),
                "__ckb_require_cell_lock_deployment_line_handle"
                    | "__ckb_require_cell_type_deployment_line_handle"
                    | "__ckb_require_cell_dep_deployment_line_verifier_handle"
            );
            if operation.operands.iter().any(|operand| operand.ty == "ExactScriptHandle") && !is_exact_handle_call {
                return typed_error("ExactScriptHandle operand is passed to an unrecognized runtime helper".to_string());
            }
            if operation.operands.iter().any(|operand| operand.ty == "DeploymentLineHandle") && !is_deployment_line_handle_call {
                return typed_error("DeploymentLineHandle operand is passed to an unrecognized runtime helper".to_string());
            }
            // Compatible-open handle values (#28 H2): the bounded 656-byte
            // encoding flows only into the versioned requirement helpers,
            // whose signatures the check below pins.
            let is_open_handle_call = matches!(
                call.target.as_str(),
                "__ckb_require_cell_dep_open_script_handle"
                    | "__ckb_require_cell_dep_open_verifier_handle"
                    | "__ckb_require_cell_dep_open_script_handle_from_args"
                    | "__ckb_require_open_script_participant_from_args"
                    | "__ckb_require_instance_script_participant_from_args"
                    | "__ckb_require_cell_dep_open_verifier_handle_from_args"
            );
            let is_open_handle_from_args_call = call.target.ends_with("_from_args");
            if !is_open_handle_call
                && operation.operands.iter().any(|operand| {
                    matches!(
                        operand.ty.split('<').next().unwrap_or(operand.ty.as_str()),
                        "ScriptHandle" | "VerifierHandle" | "InstanceScriptHandle"
                    )
                })
            {
                return typed_error("compatible-open handle operand is passed to an unrecognized runtime helper".to_string());
            }
            if is_open_handle_call {
                let instance = call.target == "__ckb_require_instance_script_participant_from_args";
                let participant = instance || call.target == "__ckb_require_open_script_participant_from_args";
                let class = if instance {
                    "InstanceScriptHandle"
                } else if participant || call.target.contains("open_script_handle") {
                    "ScriptHandle"
                } else {
                    "VerifierHandle"
                };
                let handle_operand = operation
                    .operands
                    .get(1)
                    .map(|operand| operand.ty.split('<').next().unwrap_or(operand.ty.as_str()).to_string())
                    .unwrap_or_default();
                let source_type_valid = operation.operands.first().is_some_and(|operand| operand.ty.as_str() == "CellDepView");
                let root_is_constant = matches!(
                    operation.operands.get(2).and_then(|operand| operand.constant.as_ref()),
                    Some(TypedSemanticConstant::Hash(_))
                );
                let arity_valid = if participant {
                    operation.operands.len() == 3
                        && operation
                            .operands
                            .get(2)
                            .is_some_and(|operand| matches!(operand.ty.split('<').next(), Some("InputView" | "OutputView")))
                } else if is_open_handle_from_args_call {
                    operation.operands.len() == 2
                } else {
                    operation.operands.len() == 3 && root_is_constant
                };
                if !arity_valid
                    || !source_type_valid
                    || handle_operand != class
                    || call.contract != "versioned-runtime-helper"
                    || call.return_type != "unit"
                    || !operation.destinations.is_empty()
                {
                    return typed_error(
                        "compatible-open handle call does not declare its canonical expected-root commitment and runtime contract"
                            .to_string(),
                    );
                }
            }
            let committed_inner = call.return_type.strip_prefix("Commitment<").and_then(|value| value.strip_suffix('>'));
            if call.target == "__ckb_hash_blake2b_packed"
                && let Some(inner) = committed_inner
                && (call.contract != "versioned-runtime-helper"
                    || call.params.len() != 1
                    || crate::generic_projection::canonical_type(&call.params[0]) != crate::generic_projection::canonical_type(inner)
                    || operation.operands.len() != 1
                    || operation.destinations.len() != 1
                    || operand_type(0).is_none_or(|ty| {
                        crate::generic_projection::canonical_type(ty) != crate::generic_projection::canonical_type(inner)
                    })
                    || destination_type(0) != Some(call.return_type.as_str())
                    || inner.starts_with("Commitment<")
                    || inner.starts_with("Opening<"))
            {
                return typed_error(
                    "typed commitment construction does not preserve its nominal input, output, and bounded packed-hash contract"
                        .to_string(),
                );
            }
            if call.target == "__ckb_commitment_open" {
                let commitment_inner =
                    call.params.first().and_then(|value| value.strip_prefix("Commitment<")).and_then(|value| value.strip_suffix('>'));
                let opening_inner =
                    call.params.get(1).and_then(|value| value.strip_prefix("Opening<")).and_then(|value| value.strip_suffix('>'));
                if call.contract != "versioned-runtime-helper"
                    || call.params.len() != 2
                    || operation.operands.len() != 2
                    || operation.destinations.len() != 1
                    || commitment_inner.is_none()
                    || commitment_inner != opening_inner
                    || commitment_inner.is_none_or(|inner| {
                        crate::generic_projection::canonical_type(inner)
                            != crate::generic_projection::canonical_type(&call.return_type)
                    })
                    || operand_type(0) != call.params.first().map(String::as_str)
                    || operand_type(1) != call.params.get(1).map(String::as_str)
                    || destination_type(0) != Some(call.return_type.as_str())
                {
                    return typed_error(
                        "typed commitment opening does not preserve its matched Commitment<T>, Opening<T>, and authenticated result contract"
                            .to_string(),
                    );
                }
            }
            if call.target == "__ckb_sighash_all_zero_lock" {
                let bounds = operation
                    .operands
                    .iter()
                    .map(|operand| match operand.constant.as_ref() {
                        Some(TypedSemanticConstant::U64(value)) => value.parse::<u64>().ok(),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                let valid_bounds = bounds.as_deref().is_some_and(|values| {
                    values.len() == 4
                        && (1..=64).contains(&values[0])
                        && (1..=256).contains(&values[1])
                        && values[0] <= values[1]
                        && values[2] <= 64
                        && (1..=65_536).contains(&values[3])
                });
                if call.contract != "versioned-runtime-helper"
                    || call.params != ["u64", "u64", "u64", "u64"]
                    || call.return_type != "SighashAllDigest"
                    || !valid_bounds
                {
                    return typed_error("bounded zero-lock sighash call does not declare its canonical runtime contract".to_string());
                }
            }
            if is_exact_handle_call {
                let first_type = call.params.first().map(String::as_str).unwrap_or_default();
                let source_type_valid = if call.target == "__ckb_require_cell_dep_exact_verifier_handle" {
                    first_type == "CellDepView"
                } else {
                    matches!(first_type.split('<').next().unwrap_or(first_type), "InputView" | "OutputView" | "CellDepView")
                };
                let handle_hash_is_constant = matches!(
                    operation.operands.get(2).and_then(|operand| operand.constant.as_ref()),
                    Some(TypedSemanticConstant::Hash(_))
                );
                if call.contract != "versioned-runtime-helper"
                    || call.return_type != "unit"
                    || !operation.destinations.is_empty()
                    || operation.operands.len() != 3
                    || !source_type_valid
                    || call.params.get(1).map(String::as_str) != Some("ExactScriptHandle")
                    || canonical_abi_type(call.params.get(2).map(String::as_str).unwrap_or_default()) != "hash"
                    || !handle_hash_is_constant
                {
                    return typed_error(
                        "exact Script handle call does not declare its canonical full-handle commitment and runtime contract"
                            .to_string(),
                    );
                }
            }
            if is_deployment_line_handle_call {
                let verifier = call.target == "__ckb_require_cell_dep_deployment_line_verifier_handle";
                let expected_len = if verifier { 4 } else { 5 };
                let handle_index = if verifier { 2 } else { 3 };
                let hash_index = handle_index + 1;
                let source_type_valid = if verifier {
                    call.params.first().map(String::as_str) == Some("CellDepView")
                } else {
                    call.params.first().map(String::as_str).is_some_and(|first| {
                        matches!(first.split('<').next().unwrap_or(first), "InputView" | "OutputView" | "CellDepView")
                    })
                };
                let deps_valid = if verifier {
                    call.params.first().map(String::as_str) == Some("CellDepView")
                        && call.params.get(1).map(String::as_str) == Some("CellDepView")
                } else {
                    call.params.get(1).map(String::as_str) == Some("CellDepView")
                        && call.params.get(2).map(String::as_str) == Some("CellDepView")
                };
                let handle_hash_is_constant = matches!(
                    operation.operands.get(hash_index).and_then(|operand| operand.constant.as_ref()),
                    Some(TypedSemanticConstant::Hash(_))
                );
                if call.contract != "versioned-runtime-helper"
                    || call.return_type != "unit"
                    || !operation.destinations.is_empty()
                    || operation.operands.len() != expected_len
                    || call.params.len() != expected_len
                    || !source_type_valid
                    || !deps_valid
                    || call.params.get(handle_index).map(String::as_str) != Some("DeploymentLineHandle")
                    || canonical_abi_type(call.params.get(hash_index).map(String::as_str).unwrap_or_default()) != "hash"
                    || !handle_hash_is_constant
                {
                    return typed_error(
                        "deployment line handle call does not declare its canonical active-admission, exact-code, and full-handle runtime contract"
                            .to_string(),
                    );
                }
            }
        }
        "read-ref" => {
            let TypedSemanticOperationDetail::Reference { declared_type } = &operation.detail else { return fail() };
            if !shape(1, 0)
                || strip_reference(destination_type(0).unwrap_or_default()) != strip_reference(declared_type)
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "move" => {
            let zero_sized_aggregate_sentinel = matches!(
                operation.operands.first().and_then(|operand| operand.constant.as_ref()),
                Some(TypedSemanticConstant::U64(value)) if value == "0"
            ) && destination_type(0).is_some_and(is_zero_length_array_type);
            let move_types_match = operand_type(0).zip(destination_type(0)).is_some_and(|(source, destination)| {
                typed_value_assignable(source, destination) || same_nominal_type(source, destination, record)
            }) || operand_type(0)
                .zip(destination_type(0))
                .is_some_and(|(source, destination)| bounded_witness_view_retyping_move(source, destination))
                || operand_type(0)
                    .zip(destination_type(0))
                    .is_some_and(|(source, destination)| semantic_hash_domain_retyping_move(source, destination))
                || operand_type(0)
                    .zip(destination_type(0))
                    .is_some_and(|(source, destination)| script_view_retyping_move(source, destination))
                || operand_type(0)
                    .zip(destination_type(0))
                    .is_some_and(|(source, destination)| input_out_point_retyping_move(source, destination))
                || (operand_type(0) == Some("Vec")
                    && destination_type(0).is_some_and(|destination| collection_element_type(destination).is_some()))
                || checked_unsigned_narrowing_move(entry, block, operation, locals)
                || zero_sized_aggregate_sentinel;
            if !shape(1, 1) || !none_detail || !move_types_match || operation.call.is_some() {
                return typed_error(format!(
                    "typed move has an invalid shape or type: source {:?}, destination {:?}",
                    operand_type(0),
                    destination_type(0)
                ));
            }
        }
        "tuple" => {
            let expected =
                format!("({})", operation.operands.iter().map(|operand| operand.ty.as_str()).collect::<Vec<_>>().join(", "));
            let named_layout_matches = destination_type(0).and_then(|name| types.get(name)).is_some_and(|layout| {
                operation.operands.len() == layout.fields.len()
                    && operation.operands.iter().zip(&layout.fields).all(|(operand, field)| {
                        canonical_abi_type(&operand.ty) == canonical_abi_type(&field.ty)
                            || same_nominal_type(&operand.ty, &field.ty, record)
                    })
            });
            let builtin_layout_matches =
                destination_type(0).is_some_and(|destination| builtin_tuple_contract_matches(destination, &operation.operands));
            let array_layout_matches = destination_type(0)
                .and_then(|ty| ty.strip_prefix('[')?.strip_suffix(']')?.rsplit_once("; "))
                .is_some_and(|(element, count)| {
                    count.parse::<usize>().ok() == Some(operation.operands.len())
                        && operation.operands.iter().all(|operand| optional_types_equivalent(Some(&operand.ty), Some(element)))
                });
            if operation.destinations.len() != 1
                || !none_detail
                || (!optional_types_equivalent(destination_type(0), Some(expected.as_str()))
                    && !named_layout_matches
                    && !builtin_layout_matches
                    && !array_layout_matches)
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "enum-construct" => {
            let TypedSemanticOperationDetail::EnumConstruct { enum_name, variant } = &operation.detail else { return fail() };
            let Some(layout) = types.get(enum_name.as_str()).filter(|ty| ty.kind == "enum") else { return fail() };
            let Some(variant) = layout.variants.iter().find(|item| item.name == *variant) else { return fail() };
            if operation.destinations.len() != 1
                || destination_type(0) != Some(enum_name.as_str())
                || operation
                    .operands
                    .iter()
                    .zip(&variant.fields)
                    .any(|(operand, field)| !optional_types_equivalent(Some(&operand.ty), Some(&field.ty)))
                || operation.operands.len() != variant.fields.len()
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "enum-tag" => {
            let TypedSemanticOperationDetail::EnumTag { enum_name } = &operation.detail else { return fail() };
            if !shape(1, 1)
                || operand_type(0).map(strip_reference) != Some(enum_name.as_str())
                || destination_type(0) != Some("u8")
                || !types.get(enum_name.as_str()).is_some_and(|ty| ty.kind == "enum")
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "enum-payload" => {
            let TypedSemanticOperationDetail::EnumPayload { enum_name, variant, field_index } = &operation.detail else {
                return fail();
            };
            let field_type = types
                .get(enum_name.as_str())
                .and_then(|ty| ty.variants.iter().find(|item| item.name == *variant))
                .and_then(|variant| variant.fields.iter().find(|field| field.index == *field_index))
                .map(|field| field.ty.as_str());
            if !shape(1, 1)
                || operand_type(0).map(strip_reference) != Some(enum_name.as_str())
                || destination_type(0) != field_type
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "consume" | "destroy" => {
            if !shape(0, 1) || operation.call.is_some() {
                return fail();
            }
            match (&*operation.opcode, &operation.detail) {
                ("consume", TypedSemanticOperationDetail::None) => {}
                ("destroy", TypedSemanticOperationDetail::Destroy { policy }) if valid_destruction_policy(policy) => {}
                _ => return fail(),
            }
        }
        "create" | "create-unique" | "replace-unique" => {
            validate_create_operation(operation, locals, types, record)?;
        }
        "transfer" => {
            if !shape(1, 2) || !none_detail || destination_type(0) != operand_type(0) || operation.call.is_some() {
                return fail();
            }
        }
        "claim" | "settle" => {
            if !shape(1, 1) || !none_detail || operation.call.is_some() {
                return fail();
            }
        }
        "cell-metadata-equality" => {
            let TypedSemanticOperationDetail::CellMetadata { field } = &operation.detail else { return fail() };
            if !shape(0, 2)
                || !matches!(field.as_str(), "lock-hash" | "capacity")
                || operand_type(0) != operand_type(1)
                || operation.call.is_some()
            {
                return fail();
            }
        }
        "verifier-failure" => {
            let code = operation.operands.first().and_then(|operand| match &operand.constant {
                Some(TypedSemanticConstant::U64(value)) => value.parse::<u64>().ok(),
                _ => None,
            });
            if !shape(0, 1)
                || !none_detail
                || operation.call.is_some()
                || operand_type(0) != Some("u64")
                || block.terminator != "verifier-failure"
                || !block.successors.is_empty()
                || usize::try_from(operation.index).ok() != block.operations.len().checked_sub(1)
                || block
                    .runtime_error
                    .as_ref()
                    .is_none_or(|error| !(1..=255).contains(&error.code) || error.name.is_empty() || code != Some(error.code))
            {
                return fail();
            }
        }
        "return" => {
            let valid_return = match operation.operands.as_slice() {
                [] => canonical_abi_type(&entry.return_type) == "unit",
                [operand] => {
                    canonical_abi_type(&operand.ty) == canonical_abi_type(&entry.return_type)
                        || same_nominal_type(&operand.ty, &entry.return_type, record)
                        || (operand.ty == "u64" && matches!(operand.constant, Some(TypedSemanticConstant::U64(_))))
                }
                _ => false,
            };
            if !operation.destinations.is_empty() || !none_detail || !valid_return || operation.call.is_some() {
                return fail();
            }
        }
        "branch-condition" => {
            if !shape(0, 1) || !none_detail || operand_type(0) != Some("bool") || operation.call.is_some() {
                return fail();
            }
        }
        _ => return typed_error(format!("typed operation uses unknown opcode '{}'", operation.opcode)),
    }
    Ok(())
}

pub(super) fn validate_create_operation(
    operation: &TypedSemanticOperation,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    types: &BTreeMap<&str, &TypedSemanticType>,
    record: &TypedSemanticRecord,
) -> Result<(), CheckerError> {
    let (pattern, identity, source_offset) = match (&*operation.opcode, &operation.detail) {
        ("create", TypedSemanticOperationDetail::Create { pattern }) => (pattern, None, 0usize),
        ("create-unique", TypedSemanticOperationDetail::CreateUnique { pattern, identity }) => (pattern, Some(identity.as_str()), 0),
        ("replace-unique", TypedSemanticOperationDetail::ReplaceUnique { pattern, identity }) => (pattern, Some(identity.as_str()), 1),
        _ => return typed_error(format!("typed operation '{}' has mismatched create detail", operation.opcode)),
    };
    if operation.destinations.len() != 1 || operation.call.is_some() || pattern.binding.is_empty() || pattern.operation.is_empty() {
        return typed_error(format!("typed operation '{}' has an incomplete create pattern", operation.opcode));
    }
    let destination = locals.get(&operation.destinations[0]).map(|local| local.ty.as_str());
    let Some(layout) = types.get(pattern.ty.as_str()) else {
        return typed_error(format!("typed operation '{}' creates unknown type '{}'", operation.opcode, pattern.ty));
    };
    if destination != Some(pattern.ty.as_str())
        || identity.is_some_and(|identity| identity != pattern.identity)
        || pattern.field_names.len() + usize::from(pattern.has_lock) + source_offset != operation.operands.len()
    {
        return typed_error(format!("typed operation '{}' create identity or operand shape is invalid", operation.opcode));
    }
    if source_offset == 1 && operation.operands.first().map(|operand| strip_reference(&operand.ty)) != Some(pattern.ty.as_str()) {
        return typed_error("typed replace-unique source type differs from its create pattern");
    }
    let mut names = BTreeSet::new();
    for (field_index, field_name) in pattern.field_names.iter().enumerate() {
        let Some(field) = layout.fields.iter().find(|field| field.name == *field_name) else {
            return typed_error(format!("typed create pattern names unknown field '{}::{}'", pattern.ty, field_name));
        };
        let operand = operation.operands.get(source_offset + field_index);
        let encoded_unit_enum = operand.is_some_and(|operand| {
            types.get(field.ty.as_str()).is_some_and(|layout| {
                layout.kind == "enum"
                    && matches!(&operand.constant, Some(TypedSemanticConstant::U64(tag)) if tag.parse::<u32>().ok().is_some_and(
                        |tag| layout.variants.iter().any(|variant| variant.tag == tag && variant.fields.is_empty())
                    ))
            })
        });
        if !names.insert(field_name.as_str())
            || (!operand.is_some_and(|operand| {
                typed_value_assignable(&operand.ty, &field.ty) || same_nominal_type(&operand.ty, &field.ty, record)
            }) && !encoded_unit_enum)
        {
            return typed_error(format!("typed create pattern field '{}::{}' has an invalid type", pattern.ty, field_name));
        }
    }
    if pattern.field_names.len() != layout.fields.len() {
        return typed_error(format!("typed create pattern for '{}' does not initialize every field", pattern.ty));
    }
    Ok(())
}

pub(super) fn validate_binary_types(operator: &str, left: Option<&str>, right: Option<&str>, destination: Option<&str>) -> bool {
    let (Some(left), Some(right), Some(destination)) = (left, right, destination) else { return false };
    match operator {
        "add" | "sub" | "mul" | "div" | "mod" | "bit-and" | "bit-or" | "bit-xor" => {
            arithmetic_result_type(left, right).as_deref() == Some(destination)
        }
        "shl" | "shr" => is_integer_type(left) && is_integer_type(right) && destination == left,
        "eq" | "ne" => left == right && destination == "bool",
        "lt" | "le" | "gt" | "ge" => {
            (arithmetic_result_type(left, right).is_some() || left == right && is_ckb_temporal_ordered_type(left))
                && destination == "bool"
        }
        "and" | "or" => left == "bool" && right == "bool" && destination == "bool",
        _ => false,
    }
}

pub(super) fn arithmetic_result_type(left: &str, right: &str) -> Option<String> {
    if left == right && is_integer_type(left) {
        return Some(left.to_string());
    }
    let unsigned_width = |ty: &str| match ty {
        "u8" => Some(8_u16),
        "u16" => Some(16),
        "u32" => Some(32),
        "u64" | "usize" => Some(64),
        "u128" => Some(128),
        _ => None,
    };
    let width = unsigned_width(left)?.max(unsigned_width(right)?);
    Some(format!("u{width}"))
}

pub(super) fn validate_unary_types(operator: &str, operand: Option<&str>, destination: Option<&str>) -> bool {
    let (Some(operand), Some(destination)) = (operand, destination) else { return false };
    match operator {
        "neg" => operand == destination && is_integer_type(operand),
        "not" => operand == "bool" && destination == "bool",
        // A representation-preserving conversion still changes its semantic
        // type by exactly one layer. Do not accept an unchanged or unrelated
        // type merely because its machine storage happens to have equal width.
        "ref" => destination.strip_prefix('&') == Some(operand),
        "deref" => operand.strip_prefix("&mut ").or_else(|| operand.strip_prefix('&')) == Some(destination),
        _ => false,
    }
}

#[cfg(test)]
mod unary_type_tests {
    use super::validate_unary_types;

    #[test]
    fn reference_conversions_change_exactly_one_semantic_layer() {
        for (operator, input, output) in [
            ("ref", "u64", "&u64"),
            ("ref", "&u64", "&&u64"),
            ("deref", "&u64", "u64"),
            ("deref", "&mut u64", "u64"),
            ("deref", "&&u64", "&u64"),
        ] {
            assert!(validate_unary_types(operator, Some(input), Some(output)));
        }
        for (operator, input, output) in [
            ("ref", "u64", "u64"),
            ("ref", "u64", "&u32"),
            ("ref", "u64", "&mut u64"),
            ("deref", "u64", "u64"),
            ("deref", "&u64", "&u64"),
            ("deref", "&&u64", "u64"),
            ("deref", "&Left", "Right"),
        ] {
            assert!(!validate_unary_types(operator, Some(input), Some(output)));
        }
    }
}

/// Compare closed nested shapes using already independently validated generic
/// or nominal declaration bindings. Machine provenance remains separate.
pub(crate) fn same_nested_type(left: &str, right: &str, record: &TypedSemanticRecord) -> bool {
    if !is_nested_source_type(left) || !is_nested_source_type(right) {
        return false;
    }
    let aliases = crate::generic_projection::nominal_aliases(record);
    if crate::generic_projection::checked_source_type(left, &aliases)
        == crate::generic_projection::checked_source_type(right, &aliases)
    {
        return true;
    }
    same_nominal_type(left, right, record)
}

/// Catalog verification precedes typed operations and machine binding. Equal
/// layouts, labels or widths alone never establish a defining nominal owner.
pub(crate) fn same_nominal_type(left: &str, right: &str, record: &TypedSemanticRecord) -> bool {
    if !same_packed_domains(left, right) {
        return false;
    }
    let Some(catalog) = &record.nominal_declarations else {
        return false;
    };
    match (
        crate::interface::checked_nominal_type(left, catalog, record),
        crate::interface::checked_nominal_type(right, catalog, record),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

pub(super) fn typed_call_operand_matches(
    entry: &TypedSemanticEntry,
    param: &str,
    operand: &TypedSemanticOperand,
    nominal_aliases: &BTreeMap<&str, String>,
    record: &TypedSemanticRecord,
) -> bool {
    let same_type = |left: &str, right: &str| {
        if !same_packed_domains(left, right) {
            return false;
        }
        if crate::generic_projection::checked_source_type(left, nominal_aliases)
            == crate::generic_projection::checked_source_type(right, nominal_aliases)
        {
            return true;
        }
        // Only a complete independently validated declaration/layout catalog
        // can equate nominal aliases, including structured container arguments.
        // Absent evidence and distinct defining owners still reject.
        same_nominal_type(left, right, record)
    };
    if same_type(param, &operand.ty) {
        return true;
    }
    let Some(local_id) = operand.local else { return false };
    let param_pointee = strip_reference(param);
    let operand_pointee = strip_reference(&operand.ty);
    let coercion = if param_pointee != param && same_type(param_pointee, &operand.ty) {
        "ref"
    } else if operand_pointee != operand.ty && same_type(param, operand_pointee) {
        "deref"
    } else {
        return false;
    };
    entry.blocks.iter().flat_map(|block| &block.operations).any(|operation| {
        operation.destinations.as_slice() == [local_id]
            && matches!(
                &operation.detail,
                TypedSemanticOperationDetail::UnaryOperator { operator } if operator == coercion
            )
    })
}

pub(super) fn is_integer_type(ty: &str) -> bool {
    matches!(ty, "u8" | "u16" | "u32" | "i32" | "u64" | "u128")
}

pub(super) fn is_ckb_temporal_ordered_type(ty: &str) -> bool {
    matches!(
        ty,
        "EpochNumber"
            | "EpochDuration"
            | "BlockNumber"
            | "EpochLength"
            | "TimestampMillis"
            | "AbsoluteBlockSince"
            | "AbsoluteEpochSince"
            | "AbsoluteTimestampSince"
            | "RelativeBlockSince"
            | "RelativeEpochSince"
            | "RelativeTimestampSince"
    ) || ty.starts_with("Since<Absolute, ")
        || ty.starts_with("Since<Relative, ")
}

pub(super) fn strip_reference(ty: &str) -> &str {
    ty.strip_prefix("&mut ").or_else(|| ty.strip_prefix('&')).unwrap_or(ty)
}

// Independently derive the view type from the declared finite source shape.
// These rules do not claim to prove the emitted decoder or Cell authority.
pub(crate) fn is_nested_source_type(ty: &str) -> bool {
    let ty = canonical_abi_type(strip_reference(ty));
    matches!(ty.split('<').next(), Some("NestedValues" | "NestedBytesView" | "NestedRecordView"))
}

pub(crate) fn nested_source_parts(ty: &str, name: &str, count: usize) -> Option<Vec<String>> {
    let canonical = canonical_abi_type(ty);
    let inner = canonical.strip_prefix(name)?.strip_prefix('<')?.strip_suffix('>')?;
    let parts = split_top_level(inner, ',');
    (parts.len() == count).then(|| parts.into_iter().map(str::to_string).collect())
}

pub(super) fn nested_bound(text: &str, maximum: u32) -> Option<u32> {
    (!text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| text.parse::<u32>().ok())
        .flatten()
        .filter(|value| *value <= maximum)
}

pub(super) fn nested_record_type<'a>(name: &str, types: &BTreeMap<&str, &'a TypedSemanticType>) -> Option<&'a TypedSemanticType> {
    let record = *types.get(name)?;
    if record.kind != "struct" || !(2..=9).contains(&record.fields.len()) || canonical_abi_type(&record.fields.last()?.ty) != "Vec<u8>"
    {
        return None;
    }
    for field in &record.fields[..record.fields.len() - 1] {
        let ty = canonical_abi_type(&field.ty);
        if matches!(ty.as_str(), "u8" | "u16" | "u32" | "u64" | "u128" | "hash" | "address") {
            continue;
        }
        let len = ty.strip_prefix("[u8;")?.strip_suffix(']')?.parse::<u16>().ok()?;
        if !(1..=256).contains(&len) {
            return None;
        }
    }
    Some(record)
}

pub(super) fn nested_index_type(ty: &str, types: &BTreeMap<&str, &TypedSemanticType>) -> Option<String> {
    if let Some(parts) = nested_source_parts(ty, "NestedBytesView", 1) {
        nested_bound(&parts[0], crate::bounded_nested::MAX_PAYLOAD_BYTES)?;
        return Some("u8".into());
    }
    let parts = nested_source_parts(ty, "NestedValues", 4)?;
    nested_bound(&parts[1], crate::bounded_nested::MAX_ITEMS)?;
    let payload = nested_bound(&parts[2], crate::bounded_nested::MAX_PAYLOAD_BYTES)?;
    if nested_bound(&parts[3], crate::bounded_nested::MAX_TOTAL_BYTES)? < 4 {
        return None;
    }
    if parts[0] == "Vec<u8>" {
        return Some(format!("NestedBytesView<{payload}>"));
    }
    nested_record_type(&parts[0], types)?;
    Some(format!("NestedRecordView<{}, {payload}>", parts[0]))
}

pub(crate) fn nested_value_contract(
    ty: &str,
    types: &BTreeMap<&str, &TypedSemanticType>,
) -> Option<crate::bounded_nested::NestedValueContract> {
    use crate::bounded_nested::{ElementShape, NestedValueContract, PROFILE};
    let parts = nested_source_parts(ty, "NestedValues", 4)?;
    let element = if parts[0] == "Vec<u8>" {
        ElementShape::Bytes {}
    } else {
        let record = nested_record_type(&parts[0], types)?;
        let widths = record.fields[..record.fields.len() - 1]
            .iter()
            .map(|field| {
                let ty = canonical_abi_type(&field.ty);
                match ty.as_str() {
                    "u8" => Some(1),
                    "u16" => Some(2),
                    "u32" => Some(4),
                    "u64" => Some(8),
                    "u128" => Some(16),
                    "hash" | "address" => Some(32),
                    _ => ty.strip_prefix("[u8;")?.strip_suffix(']')?.parse::<u16>().ok(),
                }
            })
            .collect::<Option<Vec<_>>>()?;
        ElementShape::Record { fixed_field_widths: widths }
    };
    let contract = NestedValueContract {
        profile: PROFILE.into(),
        element,
        max_items: nested_bound(&parts[1], 32)?,
        max_payload_bytes: nested_bound(&parts[2], 256)?,
        max_total_bytes: nested_bound(&parts[3], 4096)?,
    };
    contract.validate().ok()?;
    Some(contract)
}

/// A derived view has one element's codec, never the surrounding list codec.
/// The synthetic list contract reuses only independent shape/bound validation.
pub(crate) fn nested_parameter_contract(
    ty: &str,
    types: &BTreeMap<&str, &TypedSemanticType>,
) -> Option<(crate::bounded_nested::NestedValueContract, bool)> {
    if let Some(contract) = nested_value_contract(ty, types) {
        return Some((contract, false));
    }
    let (element, payload) = if let Some(parts) = nested_source_parts(ty, "NestedBytesView", 1) {
        ("Vec<u8>".to_string(), parts[0].clone())
    } else {
        let parts = nested_source_parts(ty, "NestedRecordView", 2)?;
        (parts[0].clone(), parts[1].clone())
    };
    nested_value_contract(&format!("NestedValues<{element}, 1, {payload}, 4096>"), types).map(|contract| (contract, true))
}

pub(super) fn nested_record_field_type(ty: &str, field: &str, types: &BTreeMap<&str, &TypedSemanticType>) -> Option<String> {
    let parts = nested_source_parts(ty, "NestedRecordView", 2)?;
    let payload = nested_bound(&parts[1], crate::bounded_nested::MAX_PAYLOAD_BYTES)?;
    let record = nested_record_type(&parts[0], types)?;
    let (index, definition) = record.fields.iter().enumerate().find(|(_, definition)| definition.name == field)?;
    Some(if index + 1 == record.fields.len() { format!("NestedBytesView<{payload}>") } else { definition.ty.clone() })
}

pub(super) fn collection_element_type(ty: &str) -> Option<String> {
    let ty = strip_reference(ty);
    if let Some(inner) = ty.strip_prefix("Vec<").and_then(|value| value.strip_suffix('>')) {
        return Some(inner.to_string());
    }
    if let Some(inner) = ty.strip_prefix('[').and_then(|value| value.strip_suffix(']')) {
        return inner.rsplit_once(';').map(|(element, _)| element.trim().to_string());
    }
    None
}

pub(super) fn is_zero_length_array_type(ty: &str) -> bool {
    ty.strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.rsplit_once(';'))
        .is_some_and(|(element, len)| !element.trim().is_empty() && len.trim() == "0")
}

pub(super) fn declared_collection_type_matches(declared: &str, destination: &str) -> bool {
    declared == destination || (declared == "Vec" && destination.starts_with("Vec<") && collection_element_type(destination).is_some())
}

pub(super) fn typed_value_assignable(actual: &str, expected: &str) -> bool {
    (same_packed_domains(actual, expected) && canonical_abi_type(actual) == canonical_abi_type(expected))
        || arithmetic_result_type(actual, expected).as_deref() == Some(expected)
        || unsigned_integer_width(actual).zip(unsigned_integer_width(expected)).is_some_and(|(actual, expected)| actual <= expected)
}

pub(super) fn bounded_witness_view_retyping_move(actual: &str, expected: &str) -> bool {
    matches!(actual, "u64" | "WitnessArgsView") && bounded_witness_view_parts(expected).is_some()
}

pub(super) fn semantic_hash_domain_retyping_move(actual: &str, expected: &str) -> bool {
    (canonical_abi_type(actual) == "hash" && matches!(expected, "ScriptHash" | "SighashAllDigest"))
        || (actual == "SighashAllDigest" && canonical_abi_type(expected) == "hash")
}

pub(super) fn script_view_retyping_move(actual: &str, expected: &str) -> bool {
    let typed_cell_view = ["InputView<", "OutputView<"].into_iter().any(|prefix| {
        actual.strip_prefix(prefix).and_then(|value| value.strip_suffix('>')).is_some_and(|inner| !inner.trim().is_empty())
    });
    let cell_view = actual == "CellDepView" || actual == "SourceView" || typed_cell_view;
    cell_view && matches!(expected, "__ckb_lock_script_ref" | "__ckb_type_script_ref")
}

pub(super) fn input_out_point_retyping_move(actual: &str, expected: &str) -> bool {
    actual.strip_prefix("InputView<").and_then(|value| value.strip_suffix('>')).is_some_and(|inner| !inner.trim().is_empty())
        && expected == "__ckb_input_out_point_ref"
}

pub(super) fn checked_unsigned_narrowing_move(
    entry: &TypedSemanticEntry,
    block: &TypedSemanticBlock,
    operation: &TypedSemanticOperation,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
) -> bool {
    let Some(source) = operation.operands.first() else { return false };
    let Some(source_id) = source.local else { return false };
    let Some(destination_id) = operation.destinations.first() else { return false };
    let Some(destination) = locals.get(destination_id) else { return false };
    let Some(source_width) = unsigned_integer_width(&source.ty) else { return false };
    let Some(destination_width) = unsigned_integer_width(&destination.ty) else { return false };
    if source_width <= destination_width {
        return false;
    }
    let maximum = (1_u128 << destination_width) - 1;

    entry.blocks.iter().any(|predecessor| {
        let [success, failure] = predecessor.successors.as_slice() else { return false };
        if predecessor.terminator != "branch" || *success != block.id {
            return false;
        }
        let Some(failure_block) = entry.blocks.iter().find(|candidate| candidate.id == *failure) else {
            return false;
        };
        if !failure_block
            .runtime_error
            .as_ref()
            .is_some_and(|error| error.code == 20 && error.name == "numeric-or-discriminant-invalid")
        {
            return false;
        }
        let Some(condition_id) = predecessor.operations.last().and_then(|terminator| {
            if terminator.opcode == "branch-condition" {
                terminator.operands.first().and_then(|operand| operand.local)
            } else {
                None
            }
        }) else {
            return false;
        };
        predecessor.operations.iter().any(|candidate| {
            matches!(
                &candidate.detail,
                TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "le"
            ) && candidate.destinations.as_slice() == [condition_id]
                && candidate.operands.first().is_some_and(|operand| operand.local == Some(source_id))
                && candidate.operands.get(1).and_then(|operand| operand.constant.as_ref()).and_then(typed_constant_unsigned_value)
                    == Some(maximum)
        })
    })
}

pub(super) fn unsigned_integer_width(ty: &str) -> Option<u32> {
    match ty {
        "u8" => Some(8),
        "u16" => Some(16),
        "u32" => Some(32),
        "u64" | "usize" => Some(64),
        "u128" => Some(128),
        _ => None,
    }
}

pub(super) fn typed_constant_unsigned_value(constant: &TypedSemanticConstant) -> Option<u128> {
    match constant {
        TypedSemanticConstant::U8(value)
        | TypedSemanticConstant::U16(value)
        | TypedSemanticConstant::U32(value)
        | TypedSemanticConstant::U64(value)
        | TypedSemanticConstant::U128(value) => value.parse().ok(),
        _ => None,
    }
}

pub(super) fn tuple_field_type(ty: &str, field: &str) -> Option<String> {
    let index = field.parse::<usize>().ok()?;
    let inner = ty.strip_prefix('(')?.strip_suffix(')')?;
    let mut depth = 0_u32;
    let mut start = 0;
    let mut fields = Vec::new();
    for (offset, character) in inner.char_indices() {
        match character {
            '(' | '[' | '<' => depth = depth.checked_add(1)?,
            ')' | ']' | '>' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                fields.push(inner[start..offset].trim());
                start = offset + character.len_utf8();
            }
            _ => {}
        }
    }
    fields.push(inner[start..].trim());
    fields.get(index).filter(|field| !field.is_empty()).map(|field| (*field).to_string())
}

pub(super) fn builtin_tuple_contract_matches(destination: &str, operands: &[TypedSemanticOperand]) -> bool {
    match (destination, operands) {
        ("hash", [bytes]) => bytes.ty == "[u8; 32]",
        ("ScriptArgs", [bytes, len, is_empty]) => {
            (canonical_abi_type(&bytes.ty) == "hash"
                || (bytes.ty.starts_with('[') && collection_element_type(&bytes.ty).as_deref() == Some("u8")))
                && len.ty == "u64"
                && is_empty.ty == "bool"
        }
        ("Script", [code_hash, hash_type, args]) => {
            canonical_abi_type(&code_hash.ty) == "hash" && hash_type.ty == "u64" && args.ty == "ScriptArgs"
        }
        _ => false,
    }
}

pub(super) fn typed_borrow_path_type(root_type: &str, path: &[String], types: &BTreeMap<&str, &TypedSemanticType>) -> Option<String> {
    let mut current = root_type.to_string();
    for segment in path {
        current = types.get(strip_reference(&current))?.fields.iter().find(|field| field.name == *segment)?.ty.clone();
    }
    Some(current)
}

pub(super) fn valid_destruction_policy(policy: &str) -> bool {
    matches!(policy, "default" | "singleton-type")
        || ["unique:", "instance:", "burn-amount:"]
            .iter()
            .any(|prefix| policy.strip_prefix(prefix).is_some_and(|value| !value.is_empty()))
}

pub(super) fn validate_typed_cfg_and_dataflow(
    entry: &TypedSemanticEntry,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
    record: &TypedSemanticRecord,
) -> Result<(), CheckerError> {
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    let mut predecessors = blocks.keys().map(|id| (*id, Vec::<u32>::new())).collect::<BTreeMap<_, _>>();
    for block in &entry.blocks {
        let terminal_opcode = block.operations.last().map(|operation| operation.opcode.as_str());
        let valid_terminator = match block.terminator.as_str() {
            "return" => block.successors.is_empty() && terminal_opcode == Some("return"),
            "verifier-failure" => {
                block.successors.is_empty() && terminal_opcode == Some("verifier-failure") && block.runtime_error.is_some()
            }
            "jump" => {
                block.successors.len() == 1 && !matches!(terminal_opcode, Some("return" | "branch-condition" | "verifier-failure"))
            }
            "branch" => block.successors.len() == 2 && terminal_opcode == Some("branch-condition"),
            _ => false,
        };
        if !valid_terminator {
            return typed_error(format!("typed entry '{}' block {} has an invalid terminator contract", entry.id, block.id));
        }
        if let Some(runtime_error) = &block.runtime_error {
            let error_return = block.operations.last().and_then(|operation| operation.operands.first());
            let encoded_code = error_return.and_then(|operand| match &operand.constant {
                Some(TypedSemanticConstant::U64(value)) => value.parse::<u64>().ok(),
                _ => None,
            });
            if block.terminator != "verifier-failure"
                || runtime_error.code == 0
                || runtime_error.code > 255
                || runtime_error.name.is_empty()
                || encoded_code != Some(runtime_error.code)
            {
                return typed_error(format!("typed entry '{}' block {} has an invalid terminal verifier failure", entry.id, block.id));
            }
        } else if block.terminator == "return" {
            let return_type =
                block.operations.last().and_then(|operation| operation.operands.first()).map_or("unit", |operand| operand.ty.as_str());
            if canonical_abi_type(return_type) != canonical_abi_type(&entry.return_type)
                && !same_nominal_type(return_type, &entry.return_type, record)
            {
                return typed_error(format!("typed entry '{}' block {} returns the wrong type", entry.id, block.id));
            }
        }
        for successor in &block.successors {
            predecessors.entry(*successor).or_default().push(block.id);
        }
    }

    let mut reachable = BTreeSet::from([entry.entry_block]);
    let mut pending = vec![entry.entry_block];
    while let Some(block_id) = pending.pop() {
        for successor in &blocks[&block_id].successors {
            if reachable.insert(*successor) {
                pending.push(*successor);
            }
        }
    }
    let universe = locals.keys().copied().collect::<BTreeSet<_>>();
    let params = entry.params.iter().map(|param| param.binding_id).collect::<BTreeSet<_>>();
    let mut borrow_starts = BTreeMap::<(u32, u32), Vec<(u32, u32)>>::new();
    let mut borrow_ends = BTreeMap::<(u32, u32), Vec<u32>>::new();
    for borrow in &entry.borrows {
        let binding_id = locals
            .iter()
            .find_map(|(id, local)| (local.name == borrow.binding && local.ty == borrow.view_type).then_some(*id))
            .ok_or_else(|| {
                CheckerError::new(
                    CheckerRejectionCode::V2419TypedSemanticsInvalid,
                    format!("typed borrow binding '{}' has no local identity", borrow.binding),
                )
            })?;
        let root_id = locals
            .iter()
            .find_map(|(id, local)| (local.name == borrow.root && strip_reference(&local.ty) == borrow.root_type).then_some(*id))
            .ok_or_else(|| {
                CheckerError::new(
                    CheckerRejectionCode::V2419TypedSemanticsInvalid,
                    format!("typed borrow root '{}' has no local identity", borrow.root),
                )
            })?;
        borrow_starts.entry((borrow.start_block, borrow.start_operation)).or_default().push((binding_id, root_id));
        if let (Some(block), Some(operation)) = (borrow.end_block, borrow.end_operation) {
            borrow_ends.entry((block, operation)).or_default().push(binding_id);
        }
    }
    let block_outgoing = |block_id: u32, incoming: &BTreeSet<u32>| {
        let block = blocks[&block_id];
        let mut available = incoming.clone();
        for position in 0..=block.operations.len() {
            let position = u32::try_from(position).unwrap_or(u32::MAX);
            if let Some(bindings) = borrow_ends.get(&(block_id, position)) {
                for binding in bindings {
                    available.remove(binding);
                }
            }
            if let Some(bindings) = borrow_starts.get(&(block_id, position)) {
                available.extend(bindings.iter().map(|(binding, _)| *binding));
            }
            if let Some(operation) = usize::try_from(position).ok().and_then(|index| block.operations.get(index)) {
                available.extend(operation.destinations.iter().copied());
            }
        }
        available
    };
    let mut incoming = blocks
        .keys()
        .map(|id| {
            let initial = if *id == entry.entry_block {
                params.clone()
            } else if reachable.contains(id) {
                universe.clone()
            } else {
                BTreeSet::new()
            };
            (*id, initial)
        })
        .collect::<BTreeMap<_, _>>();
    loop {
        let mut changed = false;
        for block_id in blocks.keys().copied().filter(|id| *id != entry.entry_block && reachable.contains(id)) {
            let preds = predecessors.get(&block_id).into_iter().flatten().filter(|id| reachable.contains(id));
            let mut merged = universe.clone();
            let mut saw_predecessor = false;
            for predecessor in preds {
                saw_predecessor = true;
                let outgoing = block_outgoing(*predecessor, &incoming[predecessor]);
                merged = merged.intersection(&outgoing).copied().collect();
            }
            if !saw_predecessor {
                merged.clear();
            }
            if incoming[&block_id] != merged {
                incoming.insert(block_id, merged);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    for block_id in reachable {
        let mut available = incoming[&block_id].clone();
        let block = blocks[&block_id];
        for position in 0..=block.operations.len() {
            let position = u32::try_from(position).unwrap_or(u32::MAX);
            if let Some(bindings) = borrow_ends.get(&(block_id, position)) {
                for binding in bindings {
                    available.remove(binding);
                }
            }
            if let Some(bindings) = borrow_starts.get(&(block_id, position)) {
                for (binding, root) in bindings {
                    if !available.contains(root) {
                        return typed_error(format!(
                            "typed entry '{}' borrow at block {} operation {} starts from an unavailable root",
                            entry.id, block_id, position
                        ));
                    }
                    available.insert(*binding);
                }
            }
            let Some(operation) = usize::try_from(position).ok().and_then(|index| block.operations.get(index)) else {
                continue;
            };
            if operation.operands.iter().filter_map(|operand| operand.local).any(|local| !available.contains(&local)) {
                return typed_error(format!(
                    "typed entry '{}' block {} operation {} uses a local not defined on every incoming path",
                    entry.id, block_id, operation.index
                ));
            }
            available.extend(operation.destinations.iter().copied());
        }
    }
    Ok(())
}

pub(super) fn infer_typed_effects(
    typed: &TypedSemanticRecord,
) -> Result<BTreeMap<&str, crate::effect_facts::EffectFacts>, CheckerError> {
    crate::effect_facts::check::infer(typed)
}

pub(super) fn validate_ownership_bindings(
    entry: &TypedSemanticEntry,
    locals: &BTreeMap<u32, &TypedSemanticLocal>,
) -> Result<(), CheckerError> {
    for ownership in &entry.ownership {
        if !locals.values().any(|local| local.name == ownership.binding)
            && !entry.params.iter().any(|param| param.name == ownership.binding)
        {
            return typed_error(format!("typed ownership transition references unknown binding '{}'", ownership.binding));
        }
    }
    let has_transition = |binding: &str, operation: &str, initial: &str| {
        entry.ownership.iter().any(|item| item.binding == binding && item.operation == operation && item.initial_state == initial)
    };
    for operation in entry.blocks.iter().flat_map(|block| &block.operations) {
        let local_name =
            |operand: &TypedSemanticOperand| operand.local.and_then(|id| locals.get(&id)).map(|local| local.name.as_str());
        match operation.opcode.as_str() {
            "consume" | "destroy" => {
                let Some(binding) = operation.operands.first().and_then(local_name) else { continue };
                if !has_transition(binding, &operation.opcode, "available") {
                    return typed_error(format!("typed {} operation for '{}' has no ownership transition", operation.opcode, binding));
                }
            }
            "transfer" | "claim" | "settle" | "replace-unique" => {
                let Some(binding) = operation.operands.first().and_then(local_name) else { continue };
                if !has_transition(binding, &operation.opcode.replace('-', "_"), "available") {
                    return typed_error(format!(
                        "typed {} operation for '{}' has no consume-side ownership transition",
                        operation.opcode, binding
                    ));
                }
                if operation.opcode == "replace-unique"
                    && let TypedSemanticOperationDetail::ReplaceUnique { pattern, .. } = &operation.detail
                    && !has_transition(&pattern.binding, &pattern.operation, "unbound")
                {
                    return typed_error(format!(
                        "typed replace-unique operation for '{}' has no create-side ownership transition",
                        pattern.binding
                    ));
                }
            }
            "read-ref" => {
                let Some(binding) = operation.destinations.first().and_then(|id| locals.get(id)).map(|local| local.name.as_str())
                else {
                    continue;
                };
                if !has_transition(binding, "read_ref", "available") {
                    return typed_error(format!("typed read-ref operation for '{}' has no ownership transition", binding));
                }
            }
            "create" | "create-unique" => {
                let pattern = match &operation.detail {
                    TypedSemanticOperationDetail::Create { pattern }
                    | TypedSemanticOperationDetail::CreateUnique { pattern, .. }
                    | TypedSemanticOperationDetail::ReplaceUnique { pattern, .. } => pattern,
                    _ => continue,
                };
                if !has_transition(&pattern.binding, &pattern.operation, "unbound") {
                    return typed_error(format!(
                        "typed {} operation for '{}' has no create-side ownership transition",
                        operation.opcode, pattern.binding
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(crate) fn canonical_abi_type(ty: &str) -> String {
    if ty.trim() == "()" {
        return "unit".to_string();
    }
    let mut canonical = String::with_capacity(ty.len());
    let mut identifier = String::new();
    let flush_identifier = |canonical: &mut String, identifier: &mut String, qualified: bool| {
        if identifier.is_empty() {
            return;
        }
        canonical.push_str(if qualified {
            identifier.as_str()
        } else {
            match identifier.as_str() {
                "Address" | "address" => "address",
                "Hash" | "hash" => "hash",
                "Bool" | "bool" => "bool",
                "String" | "string" => "string",
                value => value,
            }
        });
        identifier.clear();
    };
    for (offset, character) in ty.char_indices() {
        if character.is_alphanumeric() || character == '_' {
            identifier.push(character);
        } else {
            let qualified = canonical.ends_with("::") || ty[offset..].trim_start().starts_with("::");
            flush_identifier(&mut canonical, &mut identifier, qualified);
            if !character.is_ascii_whitespace() {
                canonical.push(character);
            }
        }
    }
    let qualified = canonical.ends_with("::");
    flush_identifier(&mut canonical, &mut identifier, qualified);
    // Source unit is `()`, while typed records spell the same zero-byte
    // leaf `unit`. Apply the existing alias inside aggregates too; all other
    // tuple delimiters, leaves and arity remain part of the checked signature.
    canonical.replace("()", "unit")
}

pub(super) fn typed_error(message: impl Into<String>) -> Result<(), CheckerError> {
    Err(CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message))
}

#[cfg(test)]
mod unit_abi_tests {
    use super::canonical_abi_type;
    #[test]
    fn unit_aliases_normalize_recursively_without_erasing_tuple_arity() {
        for (source, typed) in [
            ("()", "unit"),
            ("((), u64, ())", "(unit,u64,unit)"),
            ("[((), u8); 2]", "[(unit,u8);2]"),
            ("&(() , Hash)", "&(unit,hash)"),
        ] {
            assert_eq!(canonical_abi_type(source), canonical_abi_type(typed));
        }
        for other in ["(u8,u64,unit)", "(u64,unit)", "(unit,u64)", "u64"] {
            assert_ne!(canonical_abi_type("((),u64,())"), canonical_abi_type(other));
        }
    }
}
