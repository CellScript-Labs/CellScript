//! Independent nominal schema and experimental-admission rules for data views.
use crate::TypedSemanticType;
use std::collections::BTreeMap;

pub(crate) fn validate_metadata(metadata: &serde_json::Value, typed: &crate::TypedSemanticRecord) -> Result<(), crate::CheckerError> {
    let contains = |ty: &str| ty.contains("CellDataView<");
    let invalid = |message: &str| crate::CheckerError::new(crate::CheckerRejectionCode::V2419TypedSemanticsInvalid, message);
    if typed.types.iter().flat_map(|ty| &ty.fields).any(|field| contains(&field.ty)) {
        return Err(invalid("CellDataView cannot be stored or serialized"));
    }
    let mut present = false;
    for entry in &typed.entries {
        present |= entry.params.iter().any(|param| param.ty.starts_with("InputView<") || param.ty.starts_with("OutputView<"));
        if contains(&entry.return_type) || entry.params.iter().any(|param| contains(&param.ty)) {
            return Err(invalid("CellDataView cannot cross a callable boundary"));
        }
        for local in &entry.locals {
            if contains(&local.ty) {
                present = true;
                if !local.ty.starts_with("CellDataView<") {
                    return Err(invalid("CellDataView cannot be wrapped in a reference or container"));
                }
            }
        }
        for operation in entry.blocks.iter().flat_map(|block| &block.operations) {
            if operation.operands.iter().any(|operand| contains(&operand.ty))
                && !matches!(operation.opcode.as_str(), "field-access" | "move" | "load-var" | "store-var")
            {
                return Err(invalid("CellDataView supports only immutable aliases and schema-field reads"));
            }
        }
    }
    if present
        && !metadata
            .get("runtime")
            .and_then(|runtime| runtime.get("fail_closed_runtime_features"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|features| features.iter().any(|feature| feature.as_str() == Some("cell-data-view-artifact-proof")))
    {
        return Err(crate::CheckerError::new(
            crate::CheckerRejectionCode::V2410MetadataBindingMismatch,
            "Cell data views and helper-carried transaction views require their independent-machine-proof admission blocker",
        ));
    }
    Ok(())
}

pub(crate) fn field_type(owner: &str, field: &str, types: &BTreeMap<&str, &TypedSemanticType>) -> Option<String> {
    let projection = field == "data" && (owner.starts_with("InputView<") || owner.starts_with("OutputView<"));
    let schema = if projection {
        owner.strip_prefix("InputView<").or_else(|| owner.strip_prefix("OutputView<"))?.strip_suffix('>')?
    } else {
        owner.strip_prefix("CellDataView<")?.strip_suffix('>')?
    };
    let ty = types.get(schema)?;
    if !matches!(ty.kind.as_str(), "resource" | "shared" | "receipt") {
        return None;
    }
    if ty.fields.iter().any(|field| crate::checker::is_nested_source_type(&field.ty)) {
        let (_, capacity) = crate::nested_machine::cells::layout(ty, types).ok()?;
        if capacity > 32768 || ty.encoded_size.is_some() {
            return None;
        }
        return if projection {
            Some(format!("CellDataView<{schema}>"))
        } else {
            ty.fields.iter().find(|candidate| candidate.name == field).map(|field| field.ty.clone())
        };
    }
    if !ty.encoded_size.is_some_and(|width| (1..=512).contains(&width)) {
        return None;
    }
    for field in &ty.fields {
        let canonical = crate::checker::canonical_abi_type(&field.ty);
        let valid = matches!(canonical.as_str(), "u8" | "u16" | "u32" | "u64" | "u128" | "hash" | "address")
            || canonical
                .strip_prefix("[u8;")
                .and_then(|ty| ty.strip_suffix(']'))
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| (1..=256).contains(&n));
        if !valid {
            return None;
        }
    }
    if projection {
        Some(format!("CellDataView<{schema}>"))
    } else {
        ty.fields.iter().find(|candidate| candidate.name == field).map(|field| field.ty.clone())
    }
}
