//! Independently bind source successor occurrences to executable typed CFGs.

use super::*;

pub(super) fn requires_execution(
    disposition: &CellDisposition,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> bool {
    if !matches!(&disposition.input, Some(InputDisposition::Successor { .. })) {
        return false;
    }
    if disposition.envelope.capacity == "preserve-and-check" {
        return true;
    }
    if !matches!(foundation.entry_contract.dispatch, EntryDispatchContract::SingleEntry) {
        return false;
    }
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == disposition.entry_id) else { return false };
    let binding = |role_id: Option<&String>, kind, source| {
        foundation.roles.iter().find(|role| Some(&role.role_id) == role_id).is_some_and(|role| {
            entry
                .cell_bindings
                .iter()
                .any(|binding| binding.binding == role.binding && binding.role == kind && binding.source == source)
        })
    };
    binding(disposition.input_role.as_ref(), CellBindingRole::Input, CellBindingSource::GroupInput)
        && binding(disposition.output_role.as_ref(), CellBindingRole::Output, CellBindingSource::GroupOutput)
}

fn executable_successors(block: &TypedSemanticBlock) -> &[u32] {
    if block.terminator == "branch" && block.successors.len() == 2 {
        let constant = block
            .operations
            .last()
            .filter(|operation| operation.opcode == "branch-condition")
            .and_then(|operation| operation.operands.first())
            .and_then(|operand| operand.constant.as_ref());
        if let Some(TypedSemanticConstant::Bool(value)) = constant {
            return if *value { &block.successors[..1] } else { &block.successors[1..] };
        }
    }
    &block.successors
}

/// Missing accounting is vacuous only when the actual reachable CFG has no
/// accepting exit. Missing blocks or unknown terminators never establish that.
pub(super) fn rejects_every_path(typed: &TypedSemanticRecord, entry_id: &str) -> bool {
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == entry_id) else { return false };
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    let mut pending = vec![entry.entry_block];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(block) = blocks.get(&id) else { return false };
        match block.terminator.as_str() {
            "verifier-failure" if block.runtime_error.is_some() => {}
            "jump" | "branch" if !block.successors.is_empty() => pending.extend(executable_successors(block).iter().copied()),
            _ => return false,
        }
    }
    !seen.is_empty()
}

pub(super) fn validate_execution(
    disposition: &CellDisposition,
    execution: &SuccessorExecutionBinding,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> Result<(), CheckerError> {
    let invalid = || typed_error(format!("successor '{}' has no matching executable relation", disposition.id));
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == disposition.entry_id) else { return invalid() };
    let Some(input_role) = foundation.roles.iter().find(|role| Some(&role.role_id) == disposition.input_role.as_ref()) else {
        return invalid();
    };
    let Some(output_role) = foundation.roles.iter().find(|role| Some(&role.role_id) == disposition.output_role.as_ref()) else {
        return invalid();
    };
    let role_matches = |role: &RoleBinding, kind, local| {
        role.entry_id == entry.id
            && entry
                .cell_bindings
                .iter()
                .any(|binding| binding.binding == role.binding && binding.role == kind && binding.local_id == Some(local))
    };
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    let Some(consume) = blocks.get(&execution.consume_block) else { return invalid() };
    let Some(create) = blocks.get(&execution.create_block) else { return invalid() };
    if !blocks.contains_key(&execution.success_block)
        || !role_matches(input_role, CellBindingRole::Input, execution.input_local)
        || !role_matches(output_role, CellBindingRole::Output, execution.output_local)
        || !matches!(disposition.input, Some(InputDisposition::Successor { .. }))
        || !matches!(disposition.output, Some(OutputOrigin::SuccessorOf { .. }))
        || disposition.envelope.logical_identity != "application-identity-not-established"
        || disposition.envelope.type_script != "preserve-and-check"
        || disposition.envelope.capacity != "preserve-and-check"
        || !consume.operations.iter().any(|operation| {
            operation.opcode == "consume"
                && operation.operands.len() == 1
                && operation.operands[0].local == Some(execution.input_local)
        })
    {
        return invalid();
    }
    let Some(creation) =
        create.operations.iter().find(|operation| operation.opcode == "create" && operation.destinations == [execution.output_local])
    else {
        return invalid();
    };
    let TypedSemanticOperationDetail::Create { pattern } = &creation.detail else { return invalid() };
    let mut fields = pattern
        .field_names
        .iter()
        .zip(&creation.operands)
        .map(|(field, value)| SuccessorFieldValue { field: field.clone(), value: value.clone() })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.field.cmp(&right.field));
    let lock_target = pattern.has_lock.then(|| creation.operands.last().cloned()).flatten();
    if execution.fields != fields || execution.lock_target != lock_target {
        return invalid();
    }
    let location =
        |role: &RoleBinding, kind| entry.cell_bindings.iter().find(|binding| binding.binding == role.binding && binding.role == kind);
    let Some(input_binding) = location(input_role, CellBindingRole::Input) else { return invalid() };
    let Some(output_binding) = location(output_role, CellBindingRole::Output) else { return invalid() };
    let source_name = |source| match source {
        CellBindingSource::Input => "input",
        CellBindingSource::Output => "output",
        CellBindingSource::GroupInput => "group-input",
        CellBindingSource::GroupOutput => "group-output",
        CellBindingSource::CellDep => "cell-dep",
    };
    let correspondence = format!(
        "{}[{}]->{}[{}]",
        source_name(input_binding.source),
        input_binding.ordinal,
        source_name(output_binding.source),
        output_binding.ordinal
    );
    if disposition.envelope.cardinality != "one-input-to-one-output"
        || disposition.envelope.correspondence != correspondence
        || creation.operands.len() != pattern.field_names.len() + usize::from(pattern.has_lock)
        || !disposition.envelope.data_fields.iter().all(|field| {
            let Some(index) = pattern.field_names.iter().position(|name| name == &field.field) else { return false };
            match field.treatment.as_str() {
                "set-from-expression" => true,
                "preserve" => preserved_field(entry, &creation.operands[index], execution.input_local, &field.field),
                _ => false,
            }
        })
    {
        return invalid();
    }
    let Some(dominators) = dominators(entry) else { return invalid() };
    let dominates = |parent, child| dominators.get(&child).is_some_and(|set| set.contains(&parent));
    if !dominates(execution.consume_block, execution.create_block) || !dominates(execution.create_block, execution.success_block) {
        return invalid();
    }
    let pair = |operation: &TypedSemanticOperation| {
        operation.operands.len() == 2
            && matches!(
                (operation.operands[0].local, operation.operands[1].local),
                (Some(left), Some(right)) if
                    (left == execution.input_local && right == execution.output_local)
                    || (right == execution.input_local && left == execution.output_local)
            )
    };
    let metadata_equality = |field: &str| {
        entry.blocks.iter().any(|block| {
            dominates(block.id, execution.success_block)
                && block.operations.iter().any(|operation| {
                    operation.opcode == "cell-metadata-equality"
                        && pair(operation)
                        && matches!(&operation.detail, TypedSemanticOperationDetail::CellMetadata { field: actual } if actual == field)
                })
        })
    };
    let lock = disposition.envelope.lock_script.as_str();
    let lock_valid = if lock == "preserve-and-check" {
        !pattern.has_lock && metadata_equality("lock-hash")
    } else {
        pattern.has_lock
            && creation.operands.last().is_some_and(|target| match lock {
                "set-and-check-address" => target.ty == "address",
                "set-and-check-exact-hash" => target.ty == "ScriptHash",
                _ => false,
            })
    };
    if !metadata_equality("capacity") || !lock_valid || !type_hash_guard(entry, execution, &dominators) {
        return invalid();
    }
    Ok(())
}

fn preserved_field(entry: &TypedSemanticEntry, value: &TypedSemanticOperand, input: u32, field: &str) -> bool {
    if value.ty == "unit" && value.constant == Some(TypedSemanticConstant::Unit) {
        return true;
    }
    let Some(mut local) = value.local else { return false };
    let mut seen = BTreeSet::new();
    while seen.insert(local) {
        let mut definitions =
            entry.blocks.iter().flat_map(|block| &block.operations).filter(|operation| operation.destinations.contains(&local));
        let Some(operation) = definitions.next() else { return false };
        if definitions.next().is_some() {
            return false;
        }
        match (operation.opcode.as_str(), &operation.detail) {
            ("field-access", TypedSemanticOperationDetail::Field { name }) => {
                return name == field && operation.operands.len() == 1 && operation.operands[0].local == Some(input);
            }
            ("move", _) if operation.operands.len() == 1 => {
                let Some(next) = operation.operands[0].local else { return false };
                local = next;
            }
            _ => return false,
        }
    }
    false
}

fn dominators(entry: &TypedSemanticEntry) -> Option<BTreeMap<u32, BTreeSet<u32>>> {
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    if blocks.len() != entry.blocks.len() || !blocks.contains_key(&entry.entry_block) {
        return None;
    }
    let mut reachable = BTreeSet::new();
    let mut pending = vec![entry.entry_block];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        pending.extend(blocks.get(&id)?.successors.iter().copied());
    }
    let mut predecessors = BTreeMap::<u32, BTreeSet<u32>>::new();
    for id in &reachable {
        for successor in &blocks[id].successors {
            predecessors.entry(*successor).or_default().insert(*id);
        }
    }
    let mut result = reachable
        .iter()
        .map(|id| (*id, if *id == entry.entry_block { BTreeSet::from([*id]) } else { reachable.clone() }))
        .collect::<BTreeMap<_, _>>();
    loop {
        let mut changed = false;
        for id in reachable.iter().filter(|id| **id != entry.entry_block) {
            let mut incoming = reachable.clone();
            for parent in predecessors.get(id)? {
                incoming = incoming.intersection(&result[parent]).copied().collect();
            }
            incoming.insert(*id);
            if result[id] != incoming {
                result.insert(*id, incoming);
                changed = true;
            }
        }
        if !changed {
            return Some(result);
        }
    }
}

fn type_hash_guard(
    entry: &TypedSemanticEntry,
    execution: &SuccessorExecutionBinding,
    dominators: &BTreeMap<u32, BTreeSet<u32>>,
) -> bool {
    let definition = |local| {
        let mut matches = entry
            .blocks
            .iter()
            .flat_map(|block| block.operations.iter().map(move |op| (block.id, op)))
            .filter(|(_, op)| op.destinations.contains(&local));
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    };
    let hash_of = |local, cell, at_block| {
        let Some((block, operation)) = definition(local) else { return false };
        operation.opcode == "type-hash"
            && operation.operands.len() == 1
            && operation.operands[0].local == Some(cell)
            && dominators.get(&at_block).is_some_and(|parents| parents.contains(&block))
    };
    entry.blocks.iter().any(|block| {
        if block.terminator != "branch"
            || block.successors.len() != 2
            || !dominators.get(&execution.success_block).is_some_and(|parents| parents.contains(&block.id))
        {
            return false;
        }
        let Some(failure) = entry.blocks.iter().find(|candidate| candidate.id == block.successors[1]) else { return false };
        if failure.terminator != "verifier-failure"
            || failure.runtime_error.as_ref().is_none_or(|error| error.name != "assertion-failed" || error.code == 0)
            || !dominators.get(&execution.success_block).is_some_and(|parents| parents.contains(&block.successors[0]))
        {
            return false;
        }
        let Some(condition) = block
            .operations
            .last()
            .filter(|op| op.opcode == "branch-condition")
            .and_then(|op| op.operands.first())
            .and_then(|operand| operand.local)
        else {
            return false;
        };
        let Some((comparison_block, comparison)) = definition(condition) else { return false };
        if !dominators.get(&block.id).is_some_and(|parents| parents.contains(&comparison_block))
            || comparison.opcode != "binary"
            || comparison.operands.len() != 2
            || !matches!(&comparison.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "eq")
        {
            return false;
        }
        let (Some(left), Some(right)) = (comparison.operands[0].local, comparison.operands[1].local) else { return false };
        (hash_of(left, execution.input_local, comparison_block) && hash_of(right, execution.output_local, comparison_block))
            || (hash_of(right, execution.input_local, comparison_block) && hash_of(left, execution.output_local, comparison_block))
    })
}

/// A role must be accounted for exactly once on every successful CFG path.
/// Per-role saturated counts reach a fixed point even in the presence of loops.
pub(super) fn validate_paths(typed: &TypedSemanticRecord, foundation: &SemanticFoundationRecord) -> Result<(), CheckerError> {
    for entry in &typed.entries {
        let relations = foundation
            .dispositions
            .iter()
            .filter(|disposition| disposition.entry_id == entry.id)
            .filter_map(|disposition| disposition.execution.as_ref().map(|execution| (disposition, execution)))
            .collect::<Vec<_>>();
        if relations.is_empty() {
            continue;
        }
        let mut occurrences = BTreeSet::new();
        let mut sites = BTreeSet::new();
        let mut events = BTreeMap::<&str, BTreeMap<u32, u8>>::new();
        for (disposition, execution) in relations {
            if !occurrences.insert(execution.occurrence)
                || !sites.insert((execution.consume_block, execution.input_local, execution.create_block, execution.output_local))
            {
                return typed_error(format!("entry '{}' duplicates a successor occurrence", entry.id));
            }
            for (role, block) in [
                (disposition.input_role.as_deref(), execution.consume_block),
                (disposition.output_role.as_deref(), execution.create_block),
            ] {
                let Some(role) = role else { return typed_error("executable successor lacks a role") };
                let event = events.entry(role).or_default().entry(block).or_default();
                *event = event.saturating_add(1).min(2);
            }
        }
        let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
        for (role, events) in events {
            let mut seen = BTreeSet::new();
            let mut pending = vec![(entry.entry_block, 0_u8)];
            while let Some((id, count)) = pending.pop() {
                if !seen.insert((id, count)) {
                    continue;
                }
                let Some(block) = blocks.get(&id) else { return typed_error("successor CFG references a missing block") };
                let outgoing = count.saturating_add(events.get(&id).copied().unwrap_or_default()).min(2);
                if block.terminator == "return" && outgoing != 1 {
                    return typed_error(format!("role '{role}' is accounted for {outgoing} times on an accepting path"));
                }
                pending.extend(executable_successors(block).iter().map(|id| (*id, outgoing)));
            }
        }
    }
    Ok(())
}
