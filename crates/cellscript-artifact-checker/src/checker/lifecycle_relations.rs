//! Independent occurrence and arithmetic checks for structured lifecycle policies.

use super::*;

pub(super) fn reject_quantity_retirement_claim(
    disposition: &CellDisposition,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> Result<(), CheckerError> {
    if !matches!(disposition.input, Some(InputDisposition::Retired { .. }))
        || disposition.execution.is_some()
        || disposition.lifecycle_execution.is_some()
        || disposition.kernel_execution.is_some()
    {
        return Ok(());
    }
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == disposition.entry_id) else { return Ok(()) };
    let Some(role) = foundation.roles.iter().find(|role| Some(&role.role_id) == disposition.input_role.as_ref()) else {
        return Ok(());
    };
    let blocks = entry.blocks.iter().map(|block| (block.id, block)).collect::<BTreeMap<_, _>>();
    let mut pending = vec![entry.entry_block];
    let mut reachable = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(block) = blocks.get(&id) else { return typed_error("quantity-policy entry has a missing block") };
        pending.extend(successor_relations::executable_successors(block).iter().copied());
    }
    let quantity_burn = entry
        .cell_bindings
        .iter()
        .filter(|binding| binding.role == CellBindingRole::Input && binding.binding == role.binding)
        .filter_map(|binding| binding.local_id)
        .any(|local| {
            entry.blocks.iter().filter(|block| reachable.contains(&block.id)).flat_map(|block| &block.operations).any(|operation| {
                operation.opcode == "destroy"
                    && operation.operands.first().is_some_and(|operand| operand.local == Some(local))
                    && matches!(&operation.detail,TypedSemanticOperationDetail::Destroy {policy} if policy.starts_with("burn-amount:"))
            })
        });
    if quantity_burn {
        return typed_error("quantity destruction cannot establish an object-absence disposition");
    }
    Ok(())
}

pub(super) fn requires_execution(disposition: &CellDisposition) -> bool {
    matches!(disposition.input, Some(InputDisposition::Pooled { .. }))
        || matches!(disposition.output, Some(OutputOrigin::PoolResult { .. }))
        || matches!(disposition.input, Some(InputDisposition::Retired { .. })) && disposition.envelope.type_script == "absence-checked"
        || matches!(disposition.output, Some(OutputOrigin::Fresh { .. }))
            && matches!(disposition.envelope.lock_script.as_str(), "set-and-check-address" | "set-and-check-exact-hash")
}

pub(super) fn validate_execution(
    disposition: &CellDisposition,
    execution: &LifecycleExecutionBinding,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> Result<(), CheckerError> {
    let invalid = || typed_error(format!("lifecycle '{}' has no matching executable occurrence", disposition.id));
    if !matches!(
        (&disposition.input, &disposition.output),
        (Some(InputDisposition::Pooled { .. } | InputDisposition::Retired { .. }), None)
            | (None, Some(OutputOrigin::Fresh { .. } | OutputOrigin::PoolResult { .. }))
    ) {
        return invalid();
    }
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == disposition.entry_id) else { return invalid() };
    let Some(dominators) = successor_relations::dominators(entry) else { return invalid() };
    let dominates = |parent, child| dominators.get(&child).is_some_and(|parents| parents.contains(&parent));
    if !dominates(execution.entry_block, execution.operation_block) || !dominates(execution.operation_block, execution.success_block) {
        return invalid();
    }
    let input = disposition.input_role.as_ref();
    let role_id = input.or(disposition.output_role.as_ref());
    let Some(role) = foundation.roles.iter().find(|role| Some(&role.role_id) == role_id) else { return invalid() };
    let Some(binding) = entry.cell_bindings.iter().find(|binding| {
        binding.binding == role.binding
            && binding.local_id == Some(execution.local)
            && binding.role == if input.is_some() { CellBindingRole::Input } else { CellBindingRole::Output }
    }) else {
        return invalid();
    };
    if role.entry_id != entry.id {
        return invalid();
    }
    let Some(block) = entry.blocks.iter().find(|block| block.id == execution.operation_block) else { return invalid() };
    let mut operations = block.operations.iter().filter(|operation| {
        if input.is_some() {
            matches!(operation.opcode.as_str(), "consume" | "destroy")
                && operation.operands.len() == 1
                && operation.operands[0].local == Some(execution.local)
        } else {
            matches!(operation.opcode.as_str(), "create" | "create-unique") && operation.destinations == [execution.local]
        }
    });
    let Some(operation) = operations.next() else { return invalid() };
    if operations.next().is_some() {
        return invalid();
    }
    if let Some(InputDisposition::Retired { absence_policy }) = &disposition.input {
        let TypedSemanticOperationDetail::Destroy { policy } = &operation.detail else { return invalid() };
        let matches = match policy.as_str() {
            "singleton-type" => absence_policy == "same-type-hash-output-absent",
            "unique:ckb_type_id" => absence_policy == "same-ckb-type-id-output-absent",
            _ => policy
                .strip_prefix("instance:")
                .is_some_and(|field| absence_policy == &format!("same-field-identity-output-absent:{field}")),
        };
        if !matches || !execution.fields.is_empty() || execution.lock_target.is_some() || execution.pool.is_some() {
            return invalid();
        }
    } else if input.is_some() && operation.opcode != "consume" {
        return invalid();
    }
    if input.is_some() && (!execution.fields.is_empty() || execution.lock_target.is_some()) {
        return invalid();
    }
    if input.is_none() {
        let pattern = match &operation.detail {
            TypedSemanticOperationDetail::Create { pattern } | TypedSemanticOperationDetail::CreateUnique { pattern, .. } => pattern,
            _ => return invalid(),
        };
        let mut fields = pattern
            .field_names
            .iter()
            .zip(&operation.operands)
            .map(|(field, value)| SuccessorFieldValue { field: field.clone(), value: value.clone() })
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.field.cmp(&right.field));
        let target = pattern.has_lock.then(|| operation.operands.last()).flatten();
        if execution.fields != fields
            || execution.lock_target.as_ref() != target
            || !target.is_some_and(|target| match disposition.envelope.lock_script.as_str() {
                "set-and-check-address" => target.ty == "address",
                "set-and-check-exact-hash" => target.ty == "ScriptHash",
                _ => false,
            })
        {
            return invalid();
        }
        if let Some(OutputOrigin::Fresh { identity_policy }) = &disposition.output
            && (&pattern.identity != identity_policy
                || execution.pool.is_some()
                || disposition.envelope.type_script != kernel_relations::creation_type_script_policy(binding, identity_policy))
        {
            return invalid();
        }
    }
    let pool_id = match (&disposition.input, &disposition.output) {
        (Some(InputDisposition::Pooled { pool_id, .. }), _) | (_, Some(OutputOrigin::PoolResult { pool_id, .. })) => Some(pool_id),
        _ => None,
    };
    if let Some(pool_id) = pool_id {
        let policy = if binding.membership == CellBindingMembership::CurrentTypeGroup {
            "checked-type-group-member"
        } else {
            "unconstrained-by-this-relation"
        };
        if disposition.envelope.type_script != policy {
            return invalid();
        }
        let Some(pool) = &execution.pool else { return invalid() };
        if !validate_pool(entry, foundation, pool_id, pool, &dominators) {
            return invalid();
        }
    }
    Ok(())
}

fn validate_pool(
    entry: &TypedSemanticEntry,
    foundation: &SemanticFoundationRecord,
    pool_id: &str,
    pool: &PoolExecutionBinding,
    dominators: &BTreeMap<u32, BTreeSet<u32>>,
) -> bool {
    if pool.inputs.is_empty()
        || pool.outputs.is_empty()
        || pool.conserved_fields.is_empty()
        || pool.inputs.iter().collect::<BTreeSet<_>>().len() != pool.inputs.len()
        || pool.outputs.iter().collect::<BTreeSet<_>>().len() != pool.outputs.len()
        || pool.conserved_fields.windows(2).any(|fields| fields[0] >= fields[1])
    {
        return false;
    }
    let mut inputs = BTreeSet::new();
    let mut outputs = BTreeSet::new();
    let mut sites = Vec::new();
    let mut occurrence = None;
    for disposition in foundation.dispositions.iter().filter(|disposition| disposition.entry_id == entry.id) {
        let input = matches!(&disposition.input, Some(InputDisposition::Pooled { pool_id: actual, .. }) if actual == pool_id);
        let output = matches!(&disposition.output, Some(OutputOrigin::PoolResult { pool_id: actual, .. }) if actual == pool_id);
        if !input && !output {
            continue;
        }
        let accounting = match (&disposition.input, &disposition.output) {
            (Some(InputDisposition::Pooled { accounting_obligation, .. }), _)
            | (_, Some(OutputOrigin::PoolResult { accounting_obligation, .. })) => accounting_obligation,
            _ => return false,
        };
        let fields =
            accounting.split('+').map(|part| part.strip_prefix("checked-u128-field-sum-equality:")).collect::<Option<Vec<_>>>();
        if fields.as_ref().is_none_or(|fields| {
            fields.len() != pool.conserved_fields.len()
                || fields.iter().copied().collect::<BTreeSet<_>>() != pool.conserved_fields.iter().map(String::as_str).collect()
        }) {
            return false;
        }
        let Some(execution) = &disposition.lifecycle_execution else { return false };
        if execution.pool.as_ref() != Some(pool) || occurrence.is_some_and(|previous| previous != execution.occurrence) {
            return false;
        }
        occurrence = Some(execution.occurrence);
        if !(if input { &mut inputs } else { &mut outputs }).insert(execution.local) {
            return false;
        }
        sites.push(execution.operation_block);
    }
    if inputs != pool.inputs.iter().copied().collect() || outputs != pool.outputs.iter().copied().collect() {
        return false;
    }
    let mut definitions = BTreeMap::new();
    for block in &entry.blocks {
        for operation in &block.operations {
            for destination in &operation.destinations {
                definitions.entry(*destination).and_modify(|value| *value = None).or_insert(Some((block.id, operation)));
            }
        }
    }
    pool.conserved_fields.iter().all(|field| {
        entry.blocks.iter().any(|guard| {
            if guard.terminator != "branch"
                || guard.successors.len() != 2
                || !entry.blocks.iter().any(|failure| {
                    failure.id == guard.successors[1]
                        && failure.terminator == "verifier-failure"
                        && failure.runtime_error.as_ref().is_some_and(|error| error.name == "assertion-failed" && error.code != 0)
                })
                || !sites.iter().all(|site| dominators.get(site).is_some_and(|parents| parents.contains(&guard.successors[0])))
            {
                return false;
            }
            let Some((at, equality)) = guard
                .operations
                .last()
                .filter(|operation| operation.opcode == "branch-condition")
                .and_then(|operation| operation.operands.first())
                .and_then(|value| value.local)
                .and_then(|local| definitions.get(&local).and_then(Option::as_ref))
            else {
                return false;
            };
            if !dominators.get(&guard.id).is_some_and(|parents| parents.contains(at))
                || equality.operands.len() != 2
                || !matches!(&equality.detail, TypedSemanticOperationDetail::BinaryOperator { operator } if operator == "eq")
                || equality.operands.iter().any(|value| value.ty != "u128")
            {
                return false;
            }
            let terms = |operand| sum_terms(operand, field, guard.id, &definitions, dominators);
            let expected = |locals: &[u32]| locals.iter().map(|local| (*local, 1)).collect::<BTreeMap<_, _>>();
            let left = terms(&equality.operands[0]);
            let right = terms(&equality.operands[1]);
            left == Some(expected(&pool.inputs)) && right == Some(expected(&pool.outputs))
                || left == Some(expected(&pool.outputs)) && right == Some(expected(&pool.inputs))
        })
    })
}

fn sum_terms(
    value: &TypedSemanticOperand,
    field: &str,
    at: u32,
    definitions: &BTreeMap<u32, Option<(u32, &TypedSemanticOperation)>>,
    dominators: &BTreeMap<u32, BTreeSet<u32>>,
) -> Option<BTreeMap<u32, usize>> {
    // Iterative traversal is bounded by the independently admitted record.
    // Reusing a term would count a member twice, which is not a pool sum.
    let mut pending = vec![value];
    let mut seen = BTreeSet::new();
    let mut terms = BTreeMap::new();
    while let Some(value) = pending.pop() {
        let local = value.local?;
        if !seen.insert(local) {
            return None;
        }
        let (block, operation) = definitions.get(&local)?.as_ref()?;
        if !dominators.get(&at)?.contains(block) {
            return None;
        }
        match (operation.opcode.as_str(), &operation.detail) {
            ("field-access", TypedSemanticOperationDetail::Field { name })
                if name == field
                    && operation.operands.len() == 1
                    && matches!(value.ty.as_str(), "u8" | "u16" | "u32" | "u64" | "u128") =>
            {
                *terms.entry(operation.operands[0].local?).or_default() += 1;
            }
            ("move", _)
                if operation.operands.len() == 1
                    && unsigned_width(&operation.operands[0].ty)
                        .zip(unsigned_width(&value.ty))
                        .is_some_and(|(source, target)| source <= target) =>
            {
                pending.push(&operation.operands[0]);
            }
            ("binary", TypedSemanticOperationDetail::BinaryOperator { operator })
                if operator == "add"
                    && value.ty == "u128"
                    && operation.operands.len() == 2
                    && operation.operands.iter().all(|value| value.ty == "u128") =>
            {
                pending.extend(&operation.operands);
            }
            _ => return None,
        }
    }
    Some(terms)
}

fn unsigned_width(ty: &str) -> Option<u32> {
    match ty {
        "u8" => Some(8),
        "u16" => Some(16),
        "u32" => Some(32),
        "u64" => Some(64),
        "u128" => Some(128),
        _ => None,
    }
}
