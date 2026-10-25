//! Experimental source-bound nested value encoding for native builders.
//! Produces canonical nested values and complete bounded Cell data. The caller
//! still uses the existing entry ABI and transaction/capacity builder.
//! Metadata must come from the admitted artifact; encoding grants no authority.
use crate::error::{CompileError, Result};
use crate::nested_values::{self, NestedType};
use crate::CompileMetadata;
use cellscript_artifact_checker::bounded_nested::{ElementShape, NestedValueContract};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct NestedCellFieldContract {
    pub name: String,
    pub ty: String,
    pub fixed_width: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NestedCellContract {
    pub schema_hash: String,
    pub max_encoded_bytes: u32,
    pub fields: Vec<NestedCellFieldContract>,
}

/// Named fields prevent equal-width fields from being reordered by position.
/// The variant must agree with the source field; raw nested bytes are rejected.
pub enum NestedCellFieldInput<'a> {
    FixedBytes(&'a [u8]),
    Nested(&'a [NestedElementInput<'a>]),
}

pub fn nested_cell_contracts(metadata: &CompileMetadata) -> Result<BTreeMap<String, NestedCellContract>> {
    let mut contracts = BTreeMap::new();
    for definition in &metadata.types {
        if !definition.fields.iter().any(|field| matches!(nested_values::parsed(&field.ty), Some(NestedType::Values(_)))) {
            continue;
        }
        if !matches!(definition.kind.as_str(), "Resource" | "Shared" | "Receipt") {
            return Err(invalid("nested Cell fields require a resource, shared or receipt definition"));
        }
        let schema = definition
            .molecule_schema
            .as_ref()
            .filter(|schema| schema.layout == "molecule-table-v1")
            .ok_or_else(|| invalid("nested Cell has no Molecule table schema"))?;
        let mut maximum = definition
            .fields
            .len()
            .checked_add(1)
            .and_then(|length| length.checked_mul(4))
            .ok_or_else(|| invalid("nested Cell header overflows"))?;
        let mut names = std::collections::BTreeSet::new();
        let mut fields = Vec::new();
        for field in &definition.fields {
            if !names.insert(&field.name) {
                return Err(invalid("nested Cell has duplicate field names"));
            }
            let width = fixed_width(&field.ty);
            let limit = if let Some(width) = width {
                usize::from(width)
            } else {
                nested_value_contract(metadata, &field.ty)?.max_total_bytes as usize
            };
            maximum = maximum
                .checked_add(limit)
                .filter(|size| u32::try_from(*size).is_ok())
                .ok_or_else(|| invalid("nested Cell data exceeds Molecule u32 size"))?;
            fields.push(NestedCellFieldContract { name: field.name.clone(), ty: field.ty.clone(), fixed_width: width });
        }
        if contracts
            .insert(
                definition.name.clone(),
                NestedCellContract { schema_hash: schema.schema_hash.clone(), max_encoded_bytes: maximum as u32, fields },
            )
            .is_some()
        {
            return Err(invalid("nested Cell definition is ambiguous"));
        }
    }
    Ok(contracts)
}

/// Construct all Cell data in declared field order. This does not construct a
/// CellOutput: its actual Lock/Type scripts and capacity belong to the existing
/// transaction adapter, which computes occupied capacity from these bytes.
pub fn encode_nested_cell_data(
    metadata: &CompileMetadata,
    ty: &str,
    fields: &BTreeMap<String, NestedCellFieldInput<'_>>,
) -> Result<Vec<u8>> {
    let contract =
        nested_cell_contracts(metadata)?.remove(ty).ok_or_else(|| invalid("nested Cell type is not declared by this artifact"))?;
    if fields.len() != contract.fields.len() {
        return Err(invalid("nested Cell fields must exactly match the source declaration"));
    }
    let mut encoded = Vec::with_capacity(contract.fields.len());
    let header = 4 * (contract.fields.len() + 1);
    let mut total = header;
    for field in &contract.fields {
        let input = fields.get(&field.name).ok_or_else(|| invalid("nested Cell is missing a declared field"))?;
        let bytes = match (field.fixed_width, input) {
            (Some(width), NestedCellFieldInput::FixedBytes(bytes)) if bytes.len() == usize::from(width) => bytes.to_vec(),
            (None, NestedCellFieldInput::Nested(items)) => encode_nested_values(metadata, &field.ty, items)?,
            _ => return Err(invalid("nested Cell field representation or fixed width differs from its source type")),
        };
        total = total
            .checked_add(bytes.len())
            .filter(|size| *size <= contract.max_encoded_bytes as usize)
            .ok_or_else(|| invalid("nested Cell exceeds its declared total byte bound"))?;
        encoded.push(bytes);
    }
    let mut bytes = Vec::with_capacity(total);
    put(&mut bytes, total);
    let mut offset = header;
    for field in &encoded {
        put(&mut bytes, offset);
        offset += field.len();
    }
    for field in encoded {
        bytes.extend_from_slice(&field);
    }
    Ok(bytes)
}

/// Fixed fields are encoded in declaration order, followed by the raw payload.
/// Only bitpattern-complete fixed fields are accepted by this source profile.
pub struct NestedElementInput<'a> {
    pub fixed_fields: &'a [&'a [u8]],
    pub payload: &'a [u8],
}

/// Derive limits and record widths from source metadata, not caller-supplied
/// layout widths. Nominal record names remain in the map's source-type key.
pub fn nested_value_contract(metadata: &CompileMetadata, ty: &str) -> Result<NestedValueContract> {
    let Some(NestedType::Values(values)) = nested_values::parse(ty)? else {
        return Err(invalid("expected a root NestedValues source type"));
    };
    let element = if nested_values::is_bytes_element(&values.element) {
        ElementShape::Bytes {}
    } else {
        let mut definitions = metadata.types.iter().filter(|definition| definition.name == values.element);
        let definition = definitions.next().ok_or_else(|| invalid("nested record definition is missing"))?;
        if definitions.next().is_some() || definition.kind != "Struct" || !(2..=9).contains(&definition.fields.len()) {
            return Err(invalid("nested record requires one unambiguous ordinary struct with 2..=9 fields"));
        }
        let (payload, fields) = definition.fields.split_last().expect("bounded nonempty fields");
        if !nested_values::is_bytes_element(&payload.ty) {
            return Err(invalid("nested record requires a trailing Vec<u8> payload"));
        }
        let fixed_field_widths = fields
            .iter()
            .map(|field| fixed_width(&field.ty).ok_or_else(|| invalid("nested record has an unsupported fixed field")))
            .collect::<Result<Vec<_>>>()?;
        ElementShape::Record { fixed_field_widths }
    };
    let contract = values.contract(element);
    contract.validate().map_err(|error| invalid(&error.to_string()))?;
    Ok(contract)
}

/// All declared root nested types used by Cell fields or entry/function
/// parameters. Generated SDKs freeze these limits alongside their metadata.
pub fn nested_value_contracts(metadata: &CompileMetadata) -> Result<BTreeMap<String, NestedValueContract>> {
    let names = metadata
        .types
        .iter()
        .flat_map(|ty| ty.fields.iter().map(|field| field.ty.as_str()))
        .chain(metadata.actions.iter().flat_map(|action| action.params.iter().map(|param| param.ty.as_str())))
        .chain(metadata.locks.iter().flat_map(|lock| lock.params.iter().map(|param| param.ty.as_str())))
        .chain(metadata.functions.iter().flat_map(|function| function.params.iter().map(|param| param.ty.as_str())));
    let mut contracts = BTreeMap::new();
    for name in names {
        if matches!(nested_values::parsed(name), Some(NestedType::Values(_))) {
            contracts.insert(name.to_string(), nested_value_contract(metadata, name)?);
        }
    }
    Ok(contracts)
}

/// Encode one source-declared value. Validate every width and budget before
/// allocating output; then independently validate the resulting wire value.
pub fn encode_nested_values(metadata: &CompileMetadata, ty: &str, items: &[NestedElementInput<'_>]) -> Result<Vec<u8>> {
    let contract =
        nested_value_contracts(metadata)?.remove(ty).ok_or_else(|| invalid("nested type is not declared by this artifact"))?;
    if items.len() > contract.max_items as usize {
        return Err(invalid("nested item count exceeds the declared bound"));
    }
    let widths: &[u16] = match &contract.element {
        ElementShape::Bytes {} => &[],
        ElementShape::Record { fixed_field_widths } => fixed_field_widths,
    };
    let record_header = if widths.is_empty() { 0 } else { 4 * (widths.len() + 2) };
    let fixed_size = widths.iter().map(|width| usize::from(*width)).sum::<usize>();
    let mut total = 4 * (items.len() + 1);
    for item in items {
        if item.fixed_fields.len() != widths.len()
            || item.fixed_fields.iter().zip(widths).any(|(field, width)| field.len() != usize::from(*width))
        {
            return Err(invalid("nested fixed fields differ from the declared record layout"));
        }
        if item.payload.len() > contract.max_payload_bytes as usize {
            return Err(invalid("nested payload exceeds the declared bound"));
        }
        total += record_header + fixed_size + 4 + item.payload.len();
        if total > contract.max_total_bytes as usize {
            return Err(invalid("nested encoding exceeds the declared total byte bound"));
        }
    }
    if total > contract.max_total_bytes as usize {
        return Err(invalid("nested header exceeds the declared total byte bound"));
    }
    let mut bytes = Vec::with_capacity(total);
    put(&mut bytes, total);
    let mut offset = 4 * (items.len() + 1);
    for item in items {
        put(&mut bytes, offset);
        offset += record_header + fixed_size + 4 + item.payload.len();
    }
    for item in items {
        if record_header != 0 {
            put(&mut bytes, record_header + fixed_size + 4 + item.payload.len());
            let mut offset = record_header;
            for field in item.fixed_fields {
                put(&mut bytes, offset);
                offset += field.len();
            }
            put(&mut bytes, offset);
            for field in item.fixed_fields {
                bytes.extend_from_slice(field);
            }
        }
        put(&mut bytes, item.payload.len());
        bytes.extend_from_slice(item.payload);
    }
    contract.check(&bytes).map_err(|error| invalid(&error.to_string()))?;
    Ok(bytes)
}

fn fixed_width(ty: &str) -> Option<u16> {
    match ty {
        "u8" => Some(1),
        "u16" => Some(2),
        "u32" => Some(4),
        "u64" => Some(8),
        "u128" => Some(16),
        "Hash" | "Address" => Some(32),
        _ => {
            let (element, length) = ty.strip_prefix('[')?.strip_suffix(']')?.split_once(';')?;
            if element.trim() != "u8" {
                return None;
            }
            let length = length.trim().parse::<u16>().ok()?;
            (1..=256).contains(&length).then_some(length)
        }
    }
}

fn put(bytes: &mut Vec<u8>, value: usize) {
    // Value bounds or the enclosing Cell's checked u32 size precede encoding.
    bytes.extend_from_slice(&(value as u32).to_le_bytes());
}

fn invalid(message: &str) -> CompileError {
    CompileError::without_span(message)
}
