//! Preserve lower-level lifecycle occurrences alongside structured policies.

use super::*;

pub(super) fn bind_fixed_successor(disposition: &mut CellDisposition, entry: &TypedSemanticEntry) {
    let binding = |role: Option<&String>| entry.cell_bindings.iter().find(|binding| Some(&binding.role_id(&entry.id)) == role);
    let (Some(input), Some(output)) = (binding(disposition.input_role.as_ref()), binding(disposition.output_role.as_ref())) else {
        return;
    };
    if input.source != CellBindingSource::GroupInput
        || output.source != CellBindingSource::GroupOutput
        || input.membership != CellBindingMembership::CurrentTypeGroup
        || output.membership != CellBindingMembership::CurrentTypeGroup
    {
        return;
    }
    let (Some(input_local), Some(output_local)) = (input.local_id, output.local_id) else { return };
    let mut consumes = entry.blocks.iter().flat_map(|block| block.operations.iter().map(move |operation| (block, operation))).filter(
        |(_, operation)| {
            operation.opcode == "consume" && operation.operands.first().is_some_and(|operand| operand.local == Some(input_local))
        },
    );
    let mut creates = entry
        .blocks
        .iter()
        .flat_map(|block| block.operations.iter().map(move |operation| (block, operation)))
        .filter(|(_, operation)| operation.opcode == "create" && operation.destinations == [output_local]);
    let (Some((consume, _)), Some((create, operation))) = (consumes.next(), creates.next()) else { return };
    if consumes.next().is_some() || creates.next().is_some() {
        return;
    }
    let TypedSemanticOperationDetail::Create { pattern } = &operation.detail else { return };
    if !pattern.has_lock {
        return;
    }
    let Some(target) = operation.operands.last() else { return };
    let lock = match target.ty.as_str() {
        "address" => "set-and-check-address",
        "ScriptHash" => "set-and-check-exact-hash",
        _ => return,
    };
    let reachable = reachable_blocks(entry);
    let Some(success) = entry.blocks.iter().find(|block| reachable.contains(&block.id) && block.terminator == "return") else {
        return;
    };
    let Some(occurrence) = 0x8000_0000_u32.checked_add(output.ordinal) else { return };
    let mut fields = pattern
        .field_names
        .iter()
        .zip(&operation.operands)
        .map(|(field, value)| SuccessorFieldValue { field: field.clone(), value: value.clone() })
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| left.field.cmp(&right.field));
    disposition.envelope.data_fields =
        fields.iter().map(|field| FieldDisposition { field: field.field.clone(), treatment: "set-from-expression".into() }).collect();
    disposition.envelope.lock_script = lock.into();
    disposition.envelope.correspondence = format!("{}->{}", input.selector(), output.selector());
    disposition.execution = Some(SuccessorExecutionBinding {
        contract: SuccessorExecutionContract::FixedGroupKernel,
        occurrence,
        input_local,
        output_local,
        consume_block: consume.id,
        create_block: create.id,
        success_block: success.id,
        fields,
        lock_target: Some(target.clone()),
    });
}

pub(super) fn append(
    dispositions: &mut Vec<CellDisposition>,
    operation_nodes: &mut Vec<OperationSemanticNode>,
    entry: &TypedSemanticEntry,
    types: &BTreeMap<&str, &TypedSemanticType>,
) -> BTreeSet<String> {
    let reachable = reachable_blocks(entry);
    // A legacy inferred relation still owns its complete summary. Structured
    // policies, by contrast, cover only their actual instructions.
    let legacy_roles = dispositions
        .iter()
        .filter(|record| {
            record.entry_id == entry.id
                && record.execution.is_none()
                && record.lifecycle_execution.is_none()
                && record.kernel_execution.is_none()
        })
        .flat_map(|record| record.input_role.iter().chain(&record.output_role).cloned())
        .collect::<BTreeSet<_>>();
    let mut roles = BTreeSet::new();
    for block in entry.blocks.iter().filter(|block| reachable.contains(&block.id)) {
        for operation in &block.operations {
            let (input, local) = match operation.opcode.as_str() {
                "consume" | "destroy" => (true, operation.operands.first().and_then(|operand| operand.local)),
                "create" | "create-unique" => (false, operation.destinations.first().copied()),
                _ => continue,
            };
            let Some(local) = local else { continue };
            let Some(binding) = binding_for_local(entry, local, input) else {
                continue;
            };
            let role = binding.role_id(&entry.id);
            if legacy_roles.contains(&role) || covered(dispositions, &entry.id, block.id, local, input) {
                continue;
            }
            let pattern = match &operation.detail {
                TypedSemanticOperationDetail::Create { pattern } | TypedSemanticOperationDetail::CreateUnique { pattern, .. } => {
                    Some(pattern)
                }
                _ => None,
            };
            let quantity =
                matches!(&operation.detail, TypedSemanticOperationDetail::Destroy { policy } if policy.starts_with("burn-amount:"));
            let input_disposition = input.then(|| match &operation.detail {
                TypedSemanticOperationDetail::Destroy { .. } if quantity => InputDisposition::Consumed {
                    operation: "burn-amount".into(),
                    boundary: "quantity-delta-runtime-required; object-absence-not-established".into(),
                },
                TypedSemanticOperationDetail::Destroy { policy } => InputDisposition::Retired { absence_policy: policy.clone() },
                _ => InputDisposition::Consumed {
                    operation: operation.opcode.clone(),
                    boundary: "business-disposition-not-established".into(),
                },
            });
            let output = pattern.map(|pattern| OutputOrigin::Fresh { identity_policy: pattern.identity.clone() });
            let fields = type_fields(types, &binding.ty)
                .into_iter()
                .map(|field| FieldDisposition {
                    field: field.to_string(),
                    treatment: if input {
                        "decoded-before-discharge"
                    } else if pattern.is_some_and(|pattern| pattern.field_names.iter().any(|name| name == field)) {
                        "set-from-expression"
                    } else {
                        "missing-in-source"
                    }
                    .to_string(),
                })
                .collect::<Vec<_>>();
            let mut disposition = make_disposition(
                format!("disposition:{}:kernel:{}:{}:{}", entry.id, block.id, operation.index, if input { "input" } else { "output" }),
                &entry.id,
                input.then(|| role.clone()),
                (!input).then(|| role.clone()),
                input_disposition,
                output,
                CellEnvelopeDisposition {
                    completeness: if quantity {
                        "quantity-policy-runtime-required"
                    } else if operation.opcode == "destroy" {
                        "declared-policy-bound"
                    } else if input {
                        "business-disposition-unspecified"
                    } else if pattern.is_some_and(|pattern| pattern.has_lock)
                        && fields.iter().all(|field| field.treatment != "missing-in-source")
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
                        creation_type_script_policy(
                            binding.membership == CellBindingMembership::CurrentTypeGroup,
                            pattern.map_or("none", |pattern| pattern.identity.as_str()),
                        )
                    }
                    .into(),
                    capacity: if input { "ledger-input-consumed" } else { "builder-computed-and-chain-checked" }.into(),
                    cardinality: "exactly-one".into(),
                    correspondence: if input { "none".into() } else { binding.selector() },
                },
                if quantity {
                    "runtime-helper-required"
                } else if input {
                    "checked-runtime"
                } else {
                    "builder-evidence-required"
                },
            );
            disposition.kernel_execution = Some(KernelExecutionBinding {
                block: block.id,
                operation: operation.index,
                opcode: operation.opcode.clone(),
                local,
                result_local: None,
            });
            disposition.semantic_node_id = disposition_node_id(&disposition);
            if input {
                operation_nodes.push(make_operation_node(
                    format!("operation:{}", disposition.id),
                    "input-disposition",
                    format!(
                        "operation '{}' discharges linear ownership at block {} instruction {}",
                        operation.opcode, block.id, operation.index
                    ),
                    if quantity {
                        "quantity destruction requires a delta proof and does not establish object retirement"
                    } else if operation.opcode == "destroy" {
                        "absence guarantees are limited to the declared destruction policy"
                    } else {
                        "successor, pooled accounting, and retirement guarantees are not established by consume"
                    },
                ));
            }
            dispositions.push(disposition);
            roles.insert(role);
        }
    }
    roles
}

fn covered(dispositions: &[CellDisposition], entry: &str, block: u32, local: u32, input: bool) -> bool {
    dispositions.iter().filter(|record| record.entry_id == entry).any(|record| {
        record.execution.as_ref().is_some_and(|site| {
            if input {
                (site.consume_block, site.input_local) == (block, local)
            } else {
                (site.create_block, site.output_local) == (block, local)
            }
        }) || record
            .lifecycle_execution
            .as_ref()
            .is_some_and(|site| record.input_role.is_some() == input && (site.operation_block, site.local) == (block, local))
    })
}

pub(super) fn binding_for_local(entry: &TypedSemanticEntry, local: u32, input: bool) -> Option<&TypedSemanticCellBinding> {
    let source = entry.locals.iter().find(|candidate| candidate.id == local)?.source_id;
    entry.cell_bindings.iter().find(|binding| {
        binding.role == if input { CellBindingRole::Input } else { CellBindingRole::Output }
            && binding
                .local_id
                .is_some_and(|root| entry.locals.iter().any(|candidate| candidate.id == root && candidate.source_id == source))
    })
}

pub(super) fn reachable_blocks(entry: &TypedSemanticEntry) -> BTreeSet<u32> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![entry.entry_block];
    while let Some(id) = pending.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(block) = entry.blocks.iter().find(|block| block.id == id) else { continue };
        let successors = if block.terminator == "branch" && block.successors.len() == 2 {
            match block
                .operations
                .last()
                .filter(|operation| operation.opcode == "branch-condition")
                .and_then(|operation| operation.operands.first())
                .and_then(|operand| operand.constant.as_ref())
            {
                Some(TypedSemanticConstant::Bool(true)) => &block.successors[..1],
                Some(TypedSemanticConstant::Bool(false)) => &block.successors[1..],
                _ => &block.successors,
            }
        } else {
            &block.successors
        };
        pending.extend(successors.iter().copied());
    }
    seen
}
