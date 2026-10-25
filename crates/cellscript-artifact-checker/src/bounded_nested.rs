//! Bounded borrowed views of two canonical nested Molecule value shapes.
//!
//! This is a host byte-validation prerequisite. It does not certify emitted
//! machine decoders, source ownership, nominal types, or Cell authorization.
//! Reuse the existing strict Molecule parser rather than introduce another
//! serializer. All caller-controlled budgets are checked before traversal.
use crate::code_origin::molecule;
use serde::{Deserialize, Serialize};
use std::fmt;

pub const PROFILE: &str = "cellscript-bounded-nested-values-v1";
pub const PROOF_BLOCKER: &str = "nested-values-artifact-proof";
pub const MAX_ITEMS: u32 = 32;
pub const MAX_PAYLOAD_BYTES: u32 = 256;
pub const MAX_TOTAL_BYTES: u32 = 4096;
pub const MAX_FIXED_FIELDS: usize = 8;
pub const MAX_FIXED_FIELD_BYTES: u16 = 256;

/// Keep experimental source/codec/output obligations explicit even when outer
/// metadata hashes are rebound. An unused schema alone needs no runtime gate.
pub(crate) fn validate_metadata(metadata: &serde_json::Value, typed: &crate::TypedSemanticRecord) -> Result<(), crate::CheckerError> {
    let nested_cells = typed
        .types
        .iter()
        .filter(|ty| ty.fields.iter().any(|field| crate::checker::is_nested_source_type(&field.ty)))
        .map(|ty| ty.name.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let nested = |ty: &str| {
        let canonical = crate::checker::canonical_abi_type(ty);
        let base = canonical.strip_prefix("&mut").or_else(|| canonical.strip_prefix('&')).unwrap_or(&canonical);
        crate::checker::is_nested_source_type(base)
            || nested_cells.contains(base)
            || base.strip_prefix("CellDataView<").and_then(|ty| ty.strip_suffix('>')).is_some_and(|ty| nested_cells.contains(ty))
    };
    let present = typed.entries.iter().any(|entry| {
        entry.params.iter().any(|param| nested(&param.ty))
            || entry.locals.iter().any(|local| nested(&local.ty))
            || entry.blocks.iter().flat_map(|block| &block.operations).any(|op| {
                matches!(&op.detail, crate::TypedSemanticOperationDetail::ExactRoleSelection { contract }
                    if contract.profile == crate::cell_selection::BOUNDED_PROFILE)
            })
    });
    if present
        && !metadata
            .get("runtime")
            .and_then(|runtime| runtime.get("fail_closed_runtime_features"))
            .and_then(serde_json::Value::as_array)
            .is_some_and(|features| features.iter().any(|feature| feature.as_str() == Some(PROOF_BLOCKER)))
    {
        return Err(crate::CheckerError::new(
            crate::CheckerRejectionCode::V2410MetadataBindingMismatch,
            "nested witness/Cell values and views require their source/codec/output proof admission blocker",
        ));
    }
    Ok(())
}

/// Fixed fields describe only encoded bitpattern widths. A producer/consumer
/// must additionally bind the source schema and its type/authority contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ElementShape {
    Bytes {},
    Record { fixed_field_widths: Vec<u16> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NestedValueContract {
    pub profile: String,
    pub element: ElementShape,
    pub max_items: u32,
    pub max_payload_bytes: u32,
    pub max_total_bytes: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NestedValueError {
    InvalidContract,
    TotalBytesExceeded,
    ItemCountExceeded,
    MalformedMolecule,
    FixedFieldWidthMismatch,
    PayloadBytesExceeded,
}
impl fmt::Display for NestedValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidContract => "nested value contract is unknown or exceeds the finite profile",
            Self::TotalBytesExceeded => "nested value exceeds its total encoded byte budget",
            Self::ItemCountExceeded => "nested value exceeds its item count budget",
            Self::MalformedMolecule => "nested value has noncanonical or truncated Molecule encoding",
            Self::FixedFieldWidthMismatch => "nested record fixed field differs from its declared width",
            Self::PayloadBytesExceeded => "nested value payload exceeds its per-item byte budget",
        })
    }
}
impl std::error::Error for NestedValueError {}

/// Immutable, non-owning data; the payload excludes the Molecule Bytes header.
/// Construction is private so only a fully validated value can expose elements.
#[derive(Debug, PartialEq, Eq)]
pub struct NestedElementView<'a> {
    fixed_fields: Vec<&'a [u8]>,
    payload: &'a [u8],
}
impl<'a> NestedElementView<'a> {
    pub fn fixed_fields(&self) -> &[&'a [u8]] {
        &self.fixed_fields
    }
    pub fn payload(&self) -> &'a [u8] {
        self.payload
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct CheckedNestedValue<'a> {
    bytes: &'a [u8],
    elements: Vec<NestedElementView<'a>>,
}
impl<'a> CheckedNestedValue<'a> {
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }
    pub fn elements(&self) -> &[NestedElementView<'a>] {
        &self.elements
    }
}

impl NestedValueContract {
    /// Does not allocate or recurse into caller-supplied schemas. The two enum
    /// variants fix maximum container depth (2 for bytes, 3 for records).
    pub fn validate(&self) -> Result<(), NestedValueError> {
        if self.profile != PROFILE
            || self.max_items > MAX_ITEMS
            || self.max_payload_bytes > MAX_PAYLOAD_BYTES
            || !(4..=MAX_TOTAL_BYTES).contains(&self.max_total_bytes)
        {
            return Err(NestedValueError::InvalidContract);
        }
        if let ElementShape::Record { fixed_field_widths } = &self.element
            && (fixed_field_widths.is_empty()
                || fixed_field_widths.len() > MAX_FIXED_FIELDS
                || fixed_field_widths.iter().any(|width| !(1..=MAX_FIXED_FIELD_BYTES).contains(width)))
        {
            return Err(NestedValueError::InvalidContract);
        }
        Ok(())
    }

    /// Decode the entire value before returning any view. Limits precede every
    /// per-item allocation; individual payloads are borrowed, never copied.
    pub fn check<'a>(&self, bytes: &'a [u8]) -> Result<CheckedNestedValue<'a>, NestedValueError> {
        self.validate()?;
        if bytes.len() > self.max_total_bytes as usize {
            return Err(NestedValueError::TotalBytesExceeded);
        }
        let count = dynvec_count(bytes)?;
        if count > self.max_items as usize {
            return Err(NestedValueError::ItemCountExceeded);
        }
        let items = molecule::dynamic(bytes, self.max_items as usize).map_err(|_| NestedValueError::MalformedMolecule)?;
        let mut elements = Vec::with_capacity(count);
        for item in items {
            let (fixed_fields, encoded_payload) = match &self.element {
                ElementShape::Bytes {} => (Vec::new(), item),
                ElementShape::Record { fixed_field_widths } => {
                    // This field count is bounded by validate(), never by the
                    // untrusted table's first offset.
                    let mut fields =
                        molecule::fields(item, fixed_field_widths.len() + 1).map_err(|_| NestedValueError::MalformedMolecule)?;
                    let payload = fields.pop().ok_or(NestedValueError::MalformedMolecule)?;
                    if fields.iter().zip(fixed_field_widths).any(|(field, width)| field.len() != usize::from(*width)) {
                        return Err(NestedValueError::FixedFieldWidthMismatch);
                    }
                    (fields, payload)
                }
            };
            let payload_len = word(encoded_payload, 0)?;
            if payload_len > self.max_payload_bytes as usize {
                return Err(NestedValueError::PayloadBytesExceeded);
            }
            let payload = molecule::fixed(encoded_payload, 1, self.max_payload_bytes as usize)
                .map_err(|_| NestedValueError::MalformedMolecule)?;
            elements.push(NestedElementView { fixed_fields, payload });
        }
        Ok(CheckedNestedValue { bytes, elements })
    }
}

fn word(bytes: &[u8], offset: usize) -> Result<usize, NestedValueError> {
    let end = offset.checked_add(4).ok_or(NestedValueError::MalformedMolecule)?;
    let word = bytes.get(offset..end).ok_or(NestedValueError::MalformedMolecule)?;
    Ok(u32::from_le_bytes(word.try_into().map_err(|_| NestedValueError::MalformedMolecule)?) as usize)
}

/// Check count arithmetic before calling the common offset-table parser.
fn dynvec_count(bytes: &[u8]) -> Result<usize, NestedValueError> {
    if word(bytes, 0)? != bytes.len() {
        return Err(NestedValueError::MalformedMolecule);
    }
    if bytes.len() == 4 {
        return Ok(0);
    }
    let first = word(bytes, 4)?;
    if first < 8 || first % 4 != 0 || first > bytes.len() {
        return Err(NestedValueError::MalformedMolecule);
    }
    Ok(first / 4 - 1)
}

#[cfg(test)]
mod tests;
