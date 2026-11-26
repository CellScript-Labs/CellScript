//! Independent typed binding for unique replacement, transfer and receipt results.

use super::*;

pub(super) fn validate(
    record: &CellDisposition,
    execution: &KernelExecutionBinding,
    typed: &TypedSemanticRecord,
    foundation: &SemanticFoundationRecord,
) -> Result<(), CheckerError> {
    let invalid = || typed_error(format!("paired kernel '{}' has no matching operation or declared policy", record.id));
    let Some(entry) = typed.entries.iter().find(|entry| entry.id == record.entry_id) else { return invalid() };
    let Some(operation) = entry
        .blocks
        .iter()
        .find(|block| block.id == execution.block)
        .and_then(|block| block.operations.get(execution.operation as usize))
    else {
        return invalid();
    };
    let Some(result_local) = execution.result_local else { return invalid() };
    if operation.index != execution.operation
        || operation.opcode != execution.opcode
        || operation.operands.first().and_then(|operand| operand.local) != Some(execution.local)
        || operation.destinations != [result_local]
    {
        return invalid();
    }
    let Some(input) = binding_for_local(entry, execution.local, true) else { return invalid() };
    let output = binding_for_local(entry, result_local, false);
    if operation.opcode != "claim" && output.is_none() {
        return invalid();
    }
    let input_role = input.role_id(&entry.id);
    let output_role = output.map(|binding| binding.role_id(&entry.id));
    if record.input_role.as_deref() != Some(&input_role)
        || record.output_role != output_role
        || !foundation.roles.iter().any(|role| role.entry_id == entry.id && role.role_id == input_role)
        || output_role.as_ref().is_some_and(|id| !foundation.roles.iter().any(|role| role.entry_id == entry.id && &role.role_id == id))
    {
        return invalid();
    }
    let Some(result_type) = entry.locals.iter().find(|value| value.id == result_local).map(|value| value.ty.as_str()) else {
        return invalid();
    };
    let Some(input_type) = typed.types.iter().find(|schema| schema.name == input.ty) else { return invalid() };
    let identity = match &operation.detail {
        TypedSemanticOperationDetail::ReplaceUnique { pattern, identity } => {
            if identity == "none"
                || &input_type.identity_policy != identity
                || &pattern.identity != identity
                || pattern.ty != input.ty
                || output.is_none_or(|output| output.ty != input.ty)
                || identity
                    .strip_prefix("field:")
                    .is_some_and(|field| !input_type.fields.iter().any(|candidate| candidate.name == field))
            {
                return invalid();
            }
            Some(identity.as_str())
        }
        TypedSemanticOperationDetail::None if matches!(operation.opcode.as_str(), "transfer" | "claim" | "settle") => None,
        _ => return invalid(),
    };
    if operation.opcode == "claim" {
        let declared = input_type.claim_output.as_deref().unwrap_or("u64");
        if input_type.kind != "receipt"
            || canonical_abi_type(declared) != canonical_abi_type(result_type) && !same_nominal_type(declared, result_type, typed)
        {
            return invalid();
        }
    } else if output.is_none_or(|output| output.ty != input.ty) {
        return invalid();
    }
    let successor = operation.opcode != "claim" && output.is_some();
    let expected_input = if successor {
        InputDisposition::Successor { output_role: output_role.clone().expect("checked paired output") }
    } else {
        InputDisposition::Consumed { operation: operation.opcode.clone(), boundary: format!("receipt-claim-result:{result_type}") }
    };
    let expected_output = output.map(|_| {
        if successor {
            OutputOrigin::SuccessorOf { input_role: input_role.clone() }
        } else {
            OutputOrigin::Created { operation: operation.opcode.clone() }
        }
    });
    let mut fields = if let Some(output) = output {
        let Some(schema) = typed.types.iter().find(|schema| schema.name == output.ty) else { return invalid() };
        schema
            .fields
            .iter()
            .map(|field| FieldDisposition {
                field: field.name.clone(),
                treatment: if identity.is_some() { "set-from-expression" } else { "declared-output-relation" }.into(),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    fields.sort_by(|left, right| left.field.cmp(&right.field));
    let envelope = CellEnvelopeDisposition {
        completeness: "kernel-policy-bound".into(),
        data_fields: fields,
        logical_identity: identity.map_or_else(
            || {
                if operation.opcode == "claim" {
                    format!("receipt-claim-result:{result_type}")
                } else {
                    "application-identity-not-established".into()
                }
            },
            |identity| format!("preserve-declared:{identity}"),
        ),
        lock_script: if operation.opcode == "transfer" {
            "declared-target"
        } else if identity == Some("script-args") {
            "declared-preserve-lock-hash"
        } else {
            "unconstrained-by-this-relation"
        }
        .into(),
        type_script: if matches!(identity, Some("ckb-type-id" | "singleton-type")) {
            "declared-preserve-type-hash"
        } else {
            "unconstrained-by-this-relation"
        }
        .into(),
        capacity: "unconstrained-by-this-relation".into(),
        cardinality: if output.is_some() { "one-input-to-one-output" } else { "one-input-to-value-result" }.into(),
        correspondence: format!(
            "{}->{}",
            input.selector(),
            output.map_or_else(|| format!("typed-local:{result_local}"), TypedSemanticCellBinding::selector)
        ),
    };
    let enforcement = if output.is_some() { "builder-evidence-required" } else { "runtime-helper-required" };
    if record.input != Some(expected_input)
        || record.output != expected_output
        || record.envelope != envelope
        || record.enforcement != enforcement
    {
        return invalid();
    }
    Ok(())
}
