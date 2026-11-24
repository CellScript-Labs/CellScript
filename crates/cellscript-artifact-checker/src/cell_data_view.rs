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
    let width = packed_width(ty, |ty| primitive_width(ty).or_else(|| record_array_layout(ty, types).map(|(_, _, bytes)| bytes)))?;
    if !(1..=512).contains(&width) {
        return None;
    }
    if projection {
        Some(format!("CellDataView<{schema}>"))
    } else {
        ty.fields.iter().find(|candidate| candidate.name == field).map(|field| field.ty.clone())
    }
}

fn primitive_width(ty: &str) -> Option<u32> {
    match crate::checker::canonical_abi_type(ty).as_str() {
        "u8" => Some(1),
        "u16" => Some(2),
        "u32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "hash" | "address" => Some(32),
        ty => ty.strip_prefix("[u8;")?.strip_suffix(']')?.parse::<u32>().ok().filter(|width| (1..=256).contains(width)),
    }
}

pub(crate) fn record_array_layout(ty: &str, types: &BTreeMap<&str, &TypedSemanticType>) -> Option<(u32, u32, u32)> {
    let (name, count) = ty.trim().strip_prefix('[')?.strip_suffix(']')?.rsplit_once(';')?;
    let record = types.get(name.trim()).filter(|record| record.kind == "struct")?;
    let count = count.trim().parse::<u32>().ok()?;
    let width = packed_width(record, primitive_width)?;
    let bytes = count.checked_mul(width)?;
    (1..=512).contains(&bytes).then_some((count, width, bytes))
}

fn packed_width(ty: &TypedSemanticType, field_width: impl Fn(&str) -> Option<u32>) -> Option<u32> {
    let total = ty.fields.iter().try_fold(0u32, |offset, field| {
        let width = field_width(&field.ty)?;
        if field.offset != offset || field.width_bytes != Some(width) {
            return None;
        }
        offset.checked_add(width)
    })?;
    (ty.encoded_size == Some(total)).then_some(total)
}
