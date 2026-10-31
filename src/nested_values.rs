//! Explicit bounded nested source types. Wire validation lives in the
//! independent checker; these names bind source bounds and read-only views.
use crate::ast::Type;
use crate::error::{CompileError, Result, Span};
use cellscript_artifact_checker::bounded_nested::{
    ElementShape, NestedValueContract, MAX_ITEMS, MAX_PAYLOAD_BYTES, MAX_TOTAL_BYTES, PROFILE,
};

mod descriptions;
pub(crate) use descriptions::{Descriptions, Element, RecordRef};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ValuesType<E = String> {
    pub element: E,
    pub max_items: u32,
    pub max_payload_bytes: u32,
    pub max_total_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum NestedType<E = String> {
    Values(ValuesType<E>),
    BytesView { max_payload_bytes: u32 },
    RecordView { element: E, max_payload_bytes: u32 },
}

impl<E: std::fmt::Display> NestedType<E> {
    pub(crate) fn canonical_text(&self) -> String {
        match self {
            Self::Values(values) => {
                format!("{VALUES}<{}, {}, {}, {}>", values.element, values.max_items, values.max_payload_bytes, values.max_total_bytes)
            }
            Self::BytesView { max_payload_bytes } => format!("{BYTES_VIEW}<{max_payload_bytes}>"),
            Self::RecordView { element, max_payload_bytes } => format!("{RECORD_VIEW}<{element}, {max_payload_bytes}>"),
        }
    }
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

/// Classify the already type-checked immutable view surface. This is metadata
/// admission, not evidence of decoding, source identity or machine lifetime;
/// those remain mandatory in the independent artifact checker.
pub(crate) fn ir_type(ty: &crate::ir::IrType) -> Option<NestedType<Element>> {
    match ty {
        crate::ir::IrType::Nested(view) => Some(view.description().clone()),
        crate::ir::IrType::Ref(inner) | crate::ir::IrType::MutRef(inner) => ir_type(inner),
        _ => None,
    }
}

pub(crate) fn ir_index_result(operand: &crate::ir::IrOperand) -> Option<crate::ir::IrType> {
    let crate::ir::IrOperand::Var(var) = operand else { return None };
    let crate::ir::IrType::Nested(view) = &var.ty else { return None };
    view.index_type()
}

pub(crate) fn ir_has_length(operand: &crate::ir::IrOperand) -> bool {
    matches!(operand, crate::ir::IrOperand::Var(var)
        if matches!(ir_type(&var.ty), Some(NestedType::Values(_) | NestedType::BytesView { .. })))
}

pub(crate) fn ir_record_field(operand: &crate::ir::IrOperand, field: &str) -> Option<crate::ir::IrType> {
    let crate::ir::IrOperand::Var(var) = operand else { return None };
    let crate::ir::IrType::Nested(view) = &var.ty else { return None };
    view.field_type(field)
}

pub(crate) fn is_bytes_element(element: &str) -> bool {
    crate::generics::applied_type(element).is_some_and(|(base, args)| base == "Vec" && args == ["u8"])
}

pub(crate) fn bytes_view_type(max_payload_bytes: u32) -> String {
    NestedType::<String>::BytesView { max_payload_bytes }.canonical_text()
}

impl ValuesType {
    pub(crate) fn source_type(&self) -> Type {
        Type::Named(NestedType::Values(self.clone()).canonical_text())
    }
    pub fn item_type(&self) -> String {
        if is_bytes_element(&self.element) {
            bytes_view_type(self.max_payload_bytes)
        } else {
            NestedType::RecordView { element: &self.element, max_payload_bytes: self.max_payload_bytes }.canonical_text()
        }
    }
}

impl<E> ValuesType<E> {
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
    descriptions::FixedFieldType::from_source(ty).map(|ty| ty.width())
}
