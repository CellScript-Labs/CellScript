//! Retain paired advanced operations without promoting their envelope policy.

use super::*;

pub(super) fn append(
    dispositions: &mut Vec<CellDisposition>,
    operation_nodes: &mut Vec<OperationSemanticNode>,
    entry: &TypedSemanticEntry,
    types: &BTreeMap<&str, &TypedSemanticType>,
) -> BTreeSet<String> {
    let reachable = kernel::reachable_blocks(entry);
    let mut roles = BTreeSet::new();
    for block in entry.blocks.iter().filter(|block| reachable.contains(&block.id)) {
        for operation in &block.operations {
            if !matches!(operation.opcode.as_str(), "replace-unique" | "transfer" | "claim" | "settle") {
                continue;
            }
            let (Some(local), Some(result_local)) =
                (operation.operands.first().and_then(|value| value.local), operation.destinations.first().copied())
            else {
                continue;
            };
            let Some(input) = kernel::binding_for_local(entry, local, true) else { continue };
            let output = kernel::binding_for_local(entry, result_local, false);
            if operation.opcode != "claim" && output.is_none() {
                continue;
            }
            let input_role = input.role_id(&entry.id);
            let output_role = output.map(|binding| binding.role_id(&entry.id));
            let identity = match &operation.detail {
                TypedSemanticOperationDetail::ReplaceUnique { identity, .. } => Some(identity.as_str()),
                _ => None,
            };
            let result_type = entry.locals.iter().find(|value| value.id == result_local).expect("typed operation result").ty.as_str();
            let successor = operation.opcode != "claim" && output.is_some();
            let input_disposition = if successor {
                InputDisposition::Successor { output_role: output_role.clone().expect("paired output role") }
            } else {
                InputDisposition::Consumed {
                    operation: operation.opcode.clone(),
                    boundary: format!("receipt-claim-result:{result_type}"),
                }
            };
            let output_origin = output.map(|_| {
                if successor {
                    OutputOrigin::SuccessorOf { input_role: input_role.clone() }
                } else {
                    OutputOrigin::Created { operation: operation.opcode.clone() }
                }
            });
            let fields = output.map_or_else(Vec::new, |output| {
                type_fields(types, &output.ty)
                    .into_iter()
                    .map(|field| FieldDisposition {
                        field: field.into(),
                        treatment: if identity.is_some() { "set-from-expression" } else { "declared-output-relation" }.into(),
                    })
                    .collect()
            });
            let logical_identity = identity.map_or_else(
                || {
                    if operation.opcode == "claim" {
                        format!("receipt-claim-result:{result_type}")
                    } else {
                        "application-identity-not-established".into()
                    }
                },
                |identity| format!("preserve-declared:{identity}"),
            );
            let lock_script = if operation.opcode == "transfer" {
                "declared-target"
            } else if identity == Some("script-args") {
                "declared-preserve-lock-hash"
            } else {
                "unconstrained-by-this-relation"
            };
            let type_script = if matches!(identity, Some("ckb-type-id" | "singleton-type")) {
                "declared-preserve-type-hash"
            } else {
                "unconstrained-by-this-relation"
            };
            let mut record = make_disposition(
                format!("disposition:{}:kernel-relation:{}:{}", entry.id, block.id, operation.index),
                &entry.id,
                Some(input_role.clone()),
                output_role.clone(),
                Some(input_disposition),
                output_origin,
                CellEnvelopeDisposition {
                    completeness: "kernel-policy-bound".into(),
                    data_fields: fields,
                    logical_identity,
                    lock_script: lock_script.into(),
                    type_script: type_script.into(),
                    capacity: "unconstrained-by-this-relation".into(),
                    cardinality: if output.is_some() { "one-input-to-one-output" } else { "one-input-to-value-result" }.into(),
                    correspondence: format!(
                        "{}->{}",
                        input.selector(),
                        output.map_or_else(|| format!("typed-local:{result_local}"), TypedSemanticCellBinding::selector)
                    ),
                },
                if output.is_some() { "builder-evidence-required" } else { "runtime-helper-required" },
            );
            record.kernel_execution = Some(KernelExecutionBinding {
                block: block.id,
                operation: operation.index,
                opcode: operation.opcode.clone(),
                local,
                result_local: Some(result_local),
            });
            record.semantic_node_id = disposition_node_id(&record);
            operation_nodes.push(make_operation_node(format!("operation:{}", record.id), &operation.opcode,
                format!("paired {} consumes local {} and produces local {}", operation.opcode, local, result_local),
                "the typed operation retains its declared identity/receipt/settlement policy; complete replacement envelope and arbitrary machine equivalence are not established"));
            dispositions.push(record);
            roles.insert(input_role);
            roles.extend(output_role);
        }
    }
    roles
}
