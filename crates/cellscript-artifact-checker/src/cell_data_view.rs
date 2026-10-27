//! Independent nominal schema and escape rules for bounded data views.
use crate::TypedSemanticType;
use std::collections::BTreeMap;

pub(crate) fn validate_types(typed: &crate::TypedSemanticRecord) -> Result<(), crate::CheckerError> {
    let contains = |ty: &str| ty.contains("CellDataView<");
    let invalid = |message: &str| crate::CheckerError::new(crate::CheckerRejectionCode::V2419TypedSemanticsInvalid, message);
    if typed.types.iter().flat_map(|ty| &ty.fields).any(|field| contains(&field.ty)) {
        return Err(invalid("CellDataView cannot be stored or serialized"));
    }
    for entry in &typed.entries {
        if contains(&entry.return_type) || entry.params.iter().any(|param| contains(&param.ty)) {
            return Err(invalid("CellDataView cannot cross a callable boundary"));
        }
        for local in &entry.locals {
            if contains(&local.ty) && !local.ty.starts_with("CellDataView<") {
                return Err(invalid("CellDataView cannot be wrapped in a reference or container"));
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
