//! Explicit bounded nested source types. Wire validation lives in the
//! independent checker; these names bind source bounds and read-only views.
use crate::ast::Type;
use crate::error::{CompileError, Result, Span};
use cellscript_artifact_checker::bounded_nested::{
    ElementShape, NestedValueContract, MAX_ITEMS, MAX_PAYLOAD_BYTES, MAX_TOTAL_BYTES, PROFILE,
};

pub(crate) const VALUES: &str = "NestedValues";
pub(crate) const BYTES_VIEW: &str = "NestedBytesView";
pub(crate) const RECORD_VIEW: &str = "NestedRecordView";

pub(crate) fn declared_values(def: &crate::ast::DataShapeDef) -> Result<ValuesType> {
    let limits = &def.limits;
    if limits.items > MAX_ITEMS || limits.payload_bytes > MAX_PAYLOAD_BYTES || !(4..=MAX_TOTAL_BYTES).contains(&limits.encoded_bytes) {
        return Err(CompileError::new("data_shape limits require items <= 32, payload_bytes <= 256 and encoded_bytes in 4..=4096; entry-witness budgets remain separate", def.span));
    }
    let Type::Named(element) = &def.element else {
        return Err(CompileError::new(
            "data_shape element must be Vec<u8> or a record with fixed identity fields and a trailing Vec<u8> payload",
            def.span,
        ));
    };
    Ok(ValuesType {
        element: element.clone(),
        max_items: limits.items,
        max_payload_bytes: limits.payload_bytes,
        max_total_bytes: limits.encoded_bytes,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValuesType {
    pub element: String,
    pub max_items: u32,
    pub max_payload_bytes: u32,
    pub max_total_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NestedType {
    Values(ValuesType),
    BytesView { max_payload_bytes: u32 },
    RecordView { element: String, max_payload_bytes: u32 },
}

pub(crate) fn is_nested_name(name: &str) -> bool {
    matches!(name.split('<').next().unwrap_or(name).trim(), VALUES | BYTES_VIEW | RECORD_VIEW)
}

pub(crate) fn parse(name: &str) -> Result<Option<NestedType>> {
    if !is_nested_name(name) {
        return Ok(None);
    }
    let invalid = || {
        CompileError::new(
        "nested values require NestedValues<T, N, P, B> with N <= 32, P <= 256 and 4 <= B <= 4096; derived views carry only their declared payload bound",
        Span::default(),
    )
    };
    let (base, args) = crate::generics::applied_type(name).ok_or_else(invalid)?;
    let bound = |value: &str, maximum: u32| -> Result<u32> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid());
        }
        value.parse::<u32>().ok().filter(|number| *number <= maximum).ok_or_else(invalid)
    };
    Ok(Some(match (base, args.as_slice()) {
        (VALUES, [element, count, payload, total]) if !element.is_empty() => {
            let total = bound(total, MAX_TOTAL_BYTES)?;
            if total < 4 {
                return Err(invalid());
            }
            NestedType::Values(ValuesType {
                element: element.clone(),
                max_items: bound(count, MAX_ITEMS)?,
                max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)?,
                max_total_bytes: total,
            })
        }
        (BYTES_VIEW, [payload]) => NestedType::BytesView { max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)? },
        (RECORD_VIEW, [element, payload]) if !element.is_empty() => {
            NestedType::RecordView { element: element.clone(), max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)? }
        }
        _ => return Err(invalid()),
    }))
}

pub(crate) fn parsed(name: &str) -> Option<NestedType> {
    parse(name).ok().flatten()
}

pub(crate) fn is_bytes_element(element: &str) -> bool {
    crate::generics::applied_type(element).is_some_and(|(base, args)| base == "Vec" && args == ["u8"])
}

pub(crate) fn bytes_view_type(max_payload_bytes: u32) -> String {
    format!("{BYTES_VIEW}<{max_payload_bytes}>")
}

impl ValuesType {
    pub(crate) fn source_type(&self) -> Type {
        Type::Named(format!("{VALUES}<{}, {}, {}, {}>", self.element, self.max_items, self.max_payload_bytes, self.max_total_bytes))
    }
    pub fn item_type(&self) -> String {
        if is_bytes_element(&self.element) {
            bytes_view_type(self.max_payload_bytes)
        } else {
            format!("{RECORD_VIEW}<{}, {}>", self.element, self.max_payload_bytes)
        }
    }
    pub fn contract(&self, element: ElementShape) -> NestedValueContract {
        NestedValueContract {
            profile: PROFILE.into(),
            element,
            max_items: self.max_items,
            max_payload_bytes: self.max_payload_bytes,
            max_total_bytes: self.max_total_bytes,
        }
    }
}

/// Restrict fixed identity fields to bitpattern-complete primitive/byte-array
/// values. Boolean/enum discriminants and nested fixed aggregates need their
/// own validity evidence and are not inferred merely from equal byte widths.
pub(crate) fn fixed_field_width(ty: &Type) -> Option<u16> {
    match ty {
        Type::U8 => Some(1),
        Type::U16 => Some(2),
        Type::U32 => Some(4),
        Type::U64 => Some(8),
        Type::U128 => Some(16),
        Type::Hash | Type::Address => Some(32),
        Type::Array(inner, length) if **inner == Type::U8 && (1..=256).contains(length) => Some(*length as u16),
        _ => None,
    }
}

pub(crate) fn record_shape(fields: &[(String, Type)]) -> Result<ElementShape> {
    let invalid = || {
        CompileError::new(
        "a nested record requires 1..=8 fixed identity fields (unsigned integers, Hash, Address or byte arrays of 1..=256 bytes) followed by one Vec<u8> payload; recursive, optional and Cell fields are not admitted",
        Span::default(),
    )
    };
    if !(2..=9).contains(&fields.len()) {
        return Err(invalid());
    }
    if !matches!(&fields.last().ok_or_else(invalid)?.1, Type::Named(name) if is_bytes_element(name)) {
        return Err(invalid());
    }
    let fixed_field_widths =
        fields[..fields.len() - 1].iter().map(|(_, ty)| fixed_field_width(ty).ok_or_else(invalid)).collect::<Result<Vec<_>>>()?;
    Ok(ElementShape::Record { fixed_field_widths })
}
