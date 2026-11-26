//! Bind lower-level lifecycle summaries to individual typed instructions.

use super::*;
mod paired;

/// Reconstruct only the guarantees of the physical binding and the closed
/// unique-operation identity policy; a nominal layout grants no Script group.
pub(super) fn creation_type_script_policy(binding: &TypedSemanticCellBinding, identity: &str) -> &'static str {
    if binding.membership == CellBindingMembership::CurrentTypeGroup {
        "checked-type-group-member"
    } else {
        match identity {
            "singleton-type" => "identity-preserve-current-type-hash",
            "ckb-type-id" => "identity-type-hash-present-builder-plan-required",
            _ => "unconstrained-by-this-relation",
        }
    }
}

/// Fixed-group membership proves the shared Type Script identity. The kernel
/// checks initialized output fields and an explicit Lock target, but does not
/// promise equal predecessor/successor capacity or application identity.
pub(super) fn validate_fixed_successor(
    disposition: &CellDisposition,
    execution: &SuccessorExecutionBinding,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> Result<(), CheckerError> {
    let invalid = || typed_error(format!("kernel successor '{}' has no matching fixed-group contract", disposition.id));
    let Some(cells) = &foundation.entry_contract.cells else { return invalid() };
    let selected = match &foundation.entry_contract.dispatch {
        EntryDispatchContract::SingleEntry => foundation.entry_contract.exact_entry == disposition.entry_id,
        EntryDispatchContract::PolicyWitnessV1(policy) => {
            policy.variants.iter().any(|variant| variant.entry_id == disposition.entry_id)
        }
        EntryDispatchContract::ExplicitVersionedDispatch { .. } => false,
    };
    if cells.coverage != EntryCoverage::FixedGroup || cells.bindings != EntryCellBindings::GroupRelative || !selected {
        return invalid();
    }
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == disposition.entry_id) else { return invalid() };
    let binding = |role: Option<&String>, kind, local| {
        entry
            .cell_bindings
            .iter()
            .find(|binding| Some(&binding.role_id(&entry.id)) == role && binding.role == kind && binding.local_id == Some(local))
    };
    let Some(input) = binding(disposition.input_role.as_ref(), CellBindingRole::Input, execution.input_local) else {
        return invalid();
    };
    let Some(output) = binding(disposition.output_role.as_ref(), CellBindingRole::Output, execution.output_local) else {
        return invalid();
    };
    let Some(occurrence) = 0x8000_0000_u32.checked_add(output.ordinal) else { return invalid() };
    if input.source != CellBindingSource::GroupInput
        || output.source != CellBindingSource::GroupOutput
        || input.membership != CellBindingMembership::CurrentTypeGroup
        || output.membership != CellBindingMembership::CurrentTypeGroup
        || input.ty != output.ty
        || !typed.types.iter().any(|schema| schema.name == input.ty && schema.kind == "resource")
        || execution.occurrence != occurrence
    {
        return invalid();
    }
    let Some(consume) = entry.blocks.iter().find(|block| block.id == execution.consume_block) else { return invalid() };
    let Some(create) = entry.blocks.iter().find(|block| block.id == execution.create_block) else { return invalid() };
    let consumes = consume
        .operations
        .iter()
        .filter(|operation| {
            operation.opcode == "consume"
                && operation.operands.first().is_some_and(|operand| operand.local == Some(execution.input_local))
        })
        .count();
    let mut creates = create
        .operations
        .iter()
        .filter(|operation| operation.opcode == "create" && operation.destinations == [execution.output_local]);
    let Some(creation) = creates.next() else { return invalid() };
    if consumes != 1 || creates.next().is_some() {
        return invalid();
    }
    let TypedSemanticOperationDetail::Create { pattern } = &creation.detail else { return invalid() };
    let Some(target) = pattern.has_lock.then(|| creation.operands.last()).flatten() else { return invalid() };
    let lock = match target.ty.as_str() {
        "address" => "set-and-check-address",
        "ScriptHash" => "set-and-check-exact-hash",
        _ => return invalid(),
    };
    let mut fields = pattern
        .field_names
        .iter()
        .zip(&creation.operands)
        .map(|(field, value)| SuccessorFieldValue { field: field.clone(), value: value.clone() })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.field.cmp(&right.field));
    if creation.operands.len() != pattern.field_names.len() + 1
        || execution.fields != fields
        || execution.lock_target.as_ref() != Some(target)
        || !matches!(disposition.input, Some(InputDisposition::Successor { .. }))
        || !matches!(disposition.output, Some(OutputOrigin::SuccessorOf { .. }))
        || disposition.enforcement != "checked-runtime"
        || disposition.envelope.completeness != "exhaustive"
        || disposition.envelope.logical_identity != "application-identity-not-established"
        || disposition.envelope.lock_script != lock
        || disposition.envelope.type_script != "preserve-and-check"
        || disposition.envelope.capacity != "builder-set-and-chain-checked"
        || disposition.envelope.cardinality != "one-input-to-one-output"
        || disposition.envelope.correspondence != format!("{}->{}", input.selector(), output.selector())
        || disposition.envelope.data_fields
            != fields
                .iter()
                .map(|field| FieldDisposition { field: field.field.clone(), treatment: "set-from-expression".into() })
                .collect::<Vec<_>>()
    {
        return invalid();
    }
    let Some(dominators) = successor_relations::dominators(entry) else { return invalid() };
    if !dominators.get(&execution.success_block).is_some_and(|parents| parents.contains(&consume.id) && parents.contains(&create.id)) {
        return invalid();
    }
    Ok(())
}

pub(super) fn validate_execution(
    disposition: &CellDisposition,
    execution: &KernelExecutionBinding,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> Result<(), CheckerError> {
    if matches!(execution.opcode.as_str(), "replace-unique" | "transfer" | "claim" | "settle") {
        return paired::validate(disposition, execution, typed, foundation);
    }
    let invalid = || typed_error(format!("kernel '{}' has no matching executable occurrence or policy", disposition.id));
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == disposition.entry_id) else { return invalid() };
    let Some(block) = entry.blocks.iter().find(|block| block.id == execution.block) else { return invalid() };
    let Some(operation) = block.operations.get(execution.operation as usize) else { return invalid() };
    let input = disposition.input_role.is_some();
    if disposition.input_role.is_some() == disposition.output_role.is_some()
        || execution.result_local.is_some()
        || operation.index != execution.operation
        || operation.opcode != execution.opcode
        || !matches!(execution.opcode.as_str(), "consume" | "destroy" | "create" | "create-unique")
        || input != matches!(operation.opcode.as_str(), "consume" | "destroy")
        || if input {
            operation.operands.first().and_then(|operand| operand.local) != Some(execution.local)
        } else {
            operation.destinations != [execution.local]
        }
    {
        return invalid();
    }
    let Some(binding) = binding_for_local(entry, execution.local, input) else { return invalid() };
    let role_id = binding.role_id(&entry.id);
    if disposition.input_role.as_ref().or(disposition.output_role.as_ref()) != Some(&role_id)
        || !foundation.roles.iter().any(|role| role.role_id == role_id && role.entry_id == entry.id)
    {
        return invalid();
    }
    let pattern = match &operation.detail {
        TypedSemanticOperationDetail::Create { pattern } | TypedSemanticOperationDetail::CreateUnique { pattern, .. } => Some(pattern),
        _ => None,
    };
    if !input && pattern.is_none() {
        return invalid();
    }
    let quantity = matches!(&operation.detail, TypedSemanticOperationDetail::Destroy { policy } if policy.starts_with("burn-amount:"));
    if quantity && matches!(disposition.input, Some(InputDisposition::Retired { .. })) {
        return typed_error("quantity destruction cannot establish an object-absence disposition");
    }
    let expected_input = input.then(|| match &operation.detail {
        TypedSemanticOperationDetail::Destroy { .. } if quantity => InputDisposition::Consumed {
            operation: "burn-amount".into(),
            boundary: "quantity-delta-runtime-required; object-absence-not-established".into(),
        },
        TypedSemanticOperationDetail::Destroy { policy } => InputDisposition::Retired { absence_policy: policy.clone() },
        _ => {
            InputDisposition::Consumed { operation: operation.opcode.clone(), boundary: "business-disposition-not-established".into() }
        }
    });
    let expected_output = pattern.map(|pattern| OutputOrigin::Fresh { identity_policy: pattern.identity.clone() });
    let Some(schema) = typed.types.iter().find(|schema| schema.name == binding.ty) else { return invalid() };
    let mut fields = schema
        .fields
        .iter()
        .map(|field| FieldDisposition {
            field: field.name.clone(),
            treatment: if input {
                "decoded-before-discharge"
            } else if pattern.is_some_and(|pattern| pattern.field_names.contains(&field.name)) {
                "set-from-expression"
            } else {
                "missing-in-source"
            }
            .into(),
        })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.field.cmp(&right.field));
    let envelope = CellEnvelopeDisposition {
        completeness: if quantity {
            "quantity-policy-runtime-required"
        } else if operation.opcode == "destroy" {
            "declared-policy-bound"
        } else if input {
            "business-disposition-unspecified"
        } else if pattern.is_some_and(|pattern| pattern.has_lock) && fields.iter().all(|field| field.treatment != "missing-in-source")
        {
            "exhaustive"
        } else {
            "partial"
        }
        .into(),
        data_fields: fields,
        logical_identity: if quantity {
            "application-identity-not-established".into()
        } else if operation.opcode == "destroy" {
            "retire-under-declared-policy".into()
        } else {
            pattern.map_or_else(|| "unspecified".into(), |pattern| format!("create:{}", pattern.identity))
        },
        lock_script: if input {
            "no-successor-envelope"
        } else if pattern.is_some_and(|pattern| pattern.has_lock) {
            "set-and-check"
        } else {
            "unspecified"
        }
        .into(),
        type_script: if input {
            "no-successor-envelope"
        } else {
            creation_type_script_policy(binding, pattern.map_or("none", |pattern| pattern.identity.as_str()))
        }
        .into(),
        capacity: if input { "ledger-input-consumed" } else { "builder-computed-and-chain-checked" }.into(),
        cardinality: "exactly-one".into(),
        correspondence: if input { "none".into() } else { binding.selector() },
    };
    let enforcement = if quantity {
        "runtime-helper-required"
    } else if input {
        "checked-runtime"
    } else {
        "builder-evidence-required"
    };
    if disposition.input != expected_input
        || disposition.output != expected_output
        || disposition.envelope != envelope
        || disposition.enforcement != enforcement
    {
        return invalid();
    }
    Ok(())
}

fn binding_for_local(entry: &TypedSemanticEntry, local: u32, input: bool) -> Option<&TypedSemanticCellBinding> {
    let source = entry.locals.iter().find(|candidate| candidate.id == local)?.source_id;
    entry.cell_bindings.iter().find(|binding| {
        binding.role == if input { CellBindingRole::Input } else { CellBindingRole::Output }
            && binding
                .local_id
                .is_some_and(|root| entry.locals.iter().any(|candidate| candidate.id == root && candidate.source_id == source))
    })
}

/// A conditional record cannot hide another instruction for the same role.
/// Legacy inferred relations retain their separate admission until their own
/// operation model is migrated; they cannot exempt a structured occurrence.
pub(super) fn validate_coverage(typed: &TypedSemanticRecord, foundation: &SemanticFoundationRecord) -> Result<(), CheckerError> {
    for entry in &typed.entries {
        let dispositions = foundation.dispositions.iter().filter(|record| record.entry_id == entry.id).collect::<Vec<_>>();
        let legacy = dispositions
            .iter()
            .filter(|record| {
                record.execution.is_none()
                    && record.lifecycle_execution.is_none()
                    && record.kernel_execution.is_none()
                    && (matches!(record.input, Some(InputDisposition::Successor { .. }))
                        || record.envelope.cardinality == "bounded-plan-cardinality")
            })
            .flat_map(|record| record.input_role.iter().chain(&record.output_role))
            .collect::<BTreeSet<_>>();
        let mut pending = vec![entry.entry_block];
        let mut seen = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(block) = entry.blocks.iter().find(|block| block.id == id) else {
                return typed_error("kernel CFG has a missing block");
            };
            pending.extend(successor_relations::executable_successors(block).iter().copied());
            for operation in &block.operations {
                let paired = matches!(operation.opcode.as_str(), "replace-unique" | "transfer" | "claim" | "settle");
                let ports = match operation.opcode.as_str() {
                    "consume" | "destroy" => vec![(true, operation.operands.first().and_then(|operand| operand.local))],
                    "create" | "create-unique" => vec![(false, operation.destinations.first().copied())],
                    _ if paired => vec![
                        (true, operation.operands.first().and_then(|operand| operand.local)),
                        (false, operation.destinations.first().copied()),
                    ],
                    _ => continue,
                };
                for (input, local) in ports {
                    let Some(local) = local else { continue };
                    let Some(binding) = binding_for_local(entry, local, input) else { continue };
                    let role = binding.role_id(&entry.id);
                    if !paired && legacy.contains(&role) {
                        continue;
                    }
                    let covered = dispositions
                        .iter()
                        .filter(|record| {
                            record.kernel_execution.as_ref().is_some_and(|site| {
                                site.block == block.id
                                    && site.operation == operation.index
                                    && if input {
                                        record.input_role.as_deref() == Some(&role) && site.local == local
                                    } else {
                                        record.output_role.as_deref() == Some(&role)
                                            && site.result_local.unwrap_or(site.local) == local
                                    }
                            }) || record.lifecycle_execution.as_ref().is_some_and(|site| {
                                record.input_role.is_some() == input && (site.operation_block, site.local) == (block.id, local)
                            }) || record.execution.as_ref().is_some_and(|site| {
                                if input {
                                    (site.consume_block, site.input_local) == (block.id, local)
                                } else {
                                    (site.create_block, site.output_local) == (block.id, local)
                                }
                            })
                        })
                        .count();
                    if covered != 1 {
                        return typed_error(format!(
                            "kernel operation '{}:{}:{}' has {covered} disposition bindings",
                            entry.id, block.id, operation.index
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
