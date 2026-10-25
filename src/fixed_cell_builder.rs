//! Named-field encoding for flat, fixed-width Cell schemas. This encodes bytes;
//! it neither authorizes a transaction nor proves the supplied metadata's origin.
use crate::{
    error::{CompileError, Result},
    CompileMetadata,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub struct FixedCellFieldContract {
    pub name: String,
    pub ty: String,
    pub width: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixedCellContract {
    pub schema_hash: String,
    pub encoded_bytes: u32,
    pub fields: Vec<FixedCellFieldContract>,
}

fn width(ty: &str) -> Option<u32> {
    match ty {
        "bool" | "u8" => Some(1),
        "u16" => Some(2),
        "u32" | "i32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "Hash" | "Address" => Some(32),
        _ => {
            let (item, count) = ty.strip_prefix('[')?.strip_suffix(']')?.split_once(';')?;
            let count = count.trim().parse::<u32>().ok()?;
            (item.trim() == "u8" && (1..=256).contains(&count)).then_some(count)
        }
    }
}

/// Unsupported aggregate/enum/dynamic fields have no contract. They cannot
/// silently fall back to caller-supplied offsets or an unchecked raw encoder.
pub fn fixed_cell_contracts(metadata: &CompileMetadata) -> Result<BTreeMap<String, FixedCellContract>> {
    let mut contracts = BTreeMap::new();
    let mut names = BTreeSet::new();
    for definition in &metadata.types {
        if !matches!(definition.kind.as_str(), "Resource" | "Shared" | "Receipt") {
            continue;
        }
        if !names.insert(&definition.name) {
            return Err(invalid("fixed Cell definition is ambiguous"));
        }
        let Some(widths) = definition.fields.iter().map(|field| width(&field.ty)).collect::<Option<Vec<_>>>() else { continue };
        if widths.is_empty() {
            continue;
        }
        let schema = definition
            .molecule_schema
            .as_ref()
            .filter(|schema| schema.layout == "fixed-struct-v1" && schema.abi == "molecule" && schema.dynamic_fields.is_empty())
            .ok_or_else(|| invalid("fixed Cell has no fixed Molecule struct schema"))?;
        let mut offset = 0u32;
        let mut fields = Vec::new();
        let mut field_names = BTreeSet::new();
        for (field, width) in definition.fields.iter().zip(widths) {
            if !field_names.insert(&field.name)
                || field.offset != offset as usize
                || !field.fixed_width
                || field.encoded_size != Some(width as usize)
            {
                return Err(invalid("fixed Cell field layout differs from its source type"));
            }
            offset = offset.checked_add(width).ok_or_else(|| invalid("fixed Cell size exceeds u32"))?;
            fields.push(FixedCellFieldContract { name: field.name.clone(), ty: field.ty.clone(), width });
        }
        if definition.encoded_size != Some(offset as usize) || schema.fixed_size != offset as usize {
            return Err(invalid("fixed Cell size differs from its declared fields"));
        }
        contracts.insert(
            definition.name.clone(),
            FixedCellContract { schema_hash: schema.schema_hash.clone(), encoded_bytes: offset, fields },
        );
    }
    Ok(contracts)
}

/// Scalars are little-endian, hashes/byte arrays are exact bytes. Field map
/// insertion order is irrelevant; the source declaration determines wire order.
pub fn encode_fixed_cell_data(metadata: &CompileMetadata, ty: &str, fields: &BTreeMap<String, &[u8]>) -> Result<Vec<u8>> {
    let contract =
        fixed_cell_contracts(metadata)?.remove(ty).ok_or_else(|| invalid("fixed Cell type is not supported by this artifact"))?;
    if fields.len() != contract.fields.len() {
        return Err(invalid("fixed Cell fields must exactly match the source declaration"));
    }
    let mut bytes = Vec::with_capacity(contract.encoded_bytes as usize);
    for field in &contract.fields {
        let value = fields.get(&field.name).ok_or_else(|| invalid("fixed Cell is missing a declared field"))?;
        if value.len() != field.width as usize || (field.ty == "bool" && value[0] > 1) {
            return Err(invalid("fixed Cell field width or boolean value is not canonical"));
        }
        bytes.extend_from_slice(value);
    }
    Ok(bytes)
}

fn invalid(message: &str) -> CompileError {
    CompileError::without_span(message)
}
