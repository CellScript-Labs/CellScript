//! Independently derive fixed or canonical bounded-table selection layouts.
use super::*;
use crate::checker::{canonical_abi_type, is_nested_source_type};
use crate::nested_machine::cells::{self, Field};
use crate::TypedSemanticType;
use std::collections::BTreeMap;

pub(crate) const FIXED_FRAME_BYTES: u32 = 4992;

pub(crate) struct Nested {
    pub fields: Vec<Field>,
    pub key_field: u32,
}

pub(crate) struct Layout {
    pub width: u32,
    pub key_offset: u32,
    pub key_width: u32,
    pub nested: Option<Nested>,
}

fn fixed(ty: &str) -> bool {
    let ty = canonical_abi_type(ty);
    matches!(ty.as_str(), "u8" | "u16" | "u32" | "u64" | "u128" | "hash" | "address")
        || ty
            .strip_prefix("[u8;")
            .and_then(|ty| ty.strip_suffix(']'))
            .and_then(|width| width.parse::<u32>().ok())
            .is_some_and(|width| (1..=256).contains(&width))
}

pub(crate) fn derive(contract: &Contract, types: &BTreeMap<&str, &TypedSemanticType>) -> Result<Vec<Layout>, &'static str> {
    contract.validate()?;
    let mut layouts = Vec::new();
    for role in &contract.roles {
        let ty = types.get(role.type_name.as_str()).ok_or("unknown role schema")?;
        if !matches!(ty.kind.as_str(), "resource" | "shared" | "receipt") {
            return Err("role requires a Cell schema");
        }
        let key = ty.fields.iter().find(|field| field.name == role.identity_field).ok_or("missing identity field")?;
        let key_width = key.width_bytes.filter(|width| (1..=32).contains(width)).ok_or("identity requires 1–32 fixed bytes")?;
        if !fixed(&key.ty) {
            return Err("identity field must have a fixed bit-pattern layout");
        }
        let (width, key_offset, nested) = if ty.fields.iter().any(|field| is_nested_source_type(&field.ty)) {
            let (fields, capacity) = cells::layout(ty, types).map_err(|_| "invalid bounded Cell table layout")?;
            if ty.encoded_size.is_some() || capacity > 32768 {
                return Err("bounded Cell capacity exceeds its profile");
            }
            let index = key.molecule_field_index.ok_or("identity has no table field index")?;
            if !matches!(fields.get(index as usize), Some(Field::Fixed(width)) if *width == key_width) {
                return Err("identity width differs from its table field");
            }
            (capacity, 0, Some(Nested { fields, key_field: index }))
        } else {
            let width = ty.encoded_size.filter(|width| (1..=512).contains(width)).ok_or("invalid fixed role width")?;
            if ty.fields.iter().any(|field| !fixed(&field.ty)) || key.offset.checked_add(key_width).is_none_or(|end| end > width) {
                return Err("invalid fixed role fields or identity offset");
            }
            (width, key.offset, None)
        };
        if matches!(&role.correspondence, Correspondence::Merge(target) if target.len() != key_width as usize) {
            return Err("merge target differs from identity width");
        }
        layouts.push(Layout { width, key_offset, key_width, nested });
    }
    if layouts.iter().any(|layout| layout.nested.is_some()) != (contract.profile == BOUNDED_PROFILE) {
        return Err("selection profile differs from resolved role layouts");
    }
    Ok(layouts)
}

pub(crate) fn frame_bytes(layouts: &[Layout]) -> u32 {
    FIXED_FRAME_BYTES
        + layouts.iter().filter(|layout| layout.nested.is_some()).map(|layout| layout.width.next_multiple_of(16)).max().unwrap_or(0)
}
