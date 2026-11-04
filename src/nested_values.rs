//! Explicit bounded nested source types. Wire validation lives in the
//! independent checker; these names bind source bounds and read-only views.
use crate::ast::{Type, TypeApplication, TypeArgument};
use crate::error::{CompileError, Result};
use cellscript_artifact_checker::bounded_nested::{
    ElementShape, NestedValueContract, MAX_ITEMS, MAX_PAYLOAD_BYTES, MAX_TOTAL_BYTES, PROFILE,
};

mod descriptions;
pub(crate) use descriptions::{Descriptions, Element, RecordRef};

pub(crate) const VALUES: &str = "NestedValues";
pub(crate) const BYTES_VIEW: &str = "NestedBytesView";
pub(crate) const RECORD_VIEW: &str = "NestedRecordView";

pub(crate) fn validate_limits(def: &crate::ast::DataShapeDef) -> Result<()> {
    let limits = &def.limits;
    if limits.items > MAX_ITEMS || limits.payload_bytes > MAX_PAYLOAD_BYTES || !(4..=MAX_TOTAL_BYTES).contains(&limits.encoded_bytes) {
        return Err(CompileError::new("data_shape limits require items <= 32, payload_bytes <= 256 and encoded_bytes in 4..=4096; entry-witness budgets remain separate", def.span));
    }
    Ok(())
}

pub(crate) fn declared_values(def: &crate::ast::DataShapeDef) -> Result<ValuesType> {
    validate_limits(def)?;
    let limits = &def.limits;
    let element = SourceElement::from_source(&def.element).map_err(|_| {
        CompileError::new(
            "data_shape element must be Vec<u8> or a record with fixed identity fields and a trailing Vec<u8> payload",
            def.span,
        )
    })?;
    Ok(ValuesType { element, max_items: limits.items, max_payload_bytes: limits.payload_bytes, max_total_bytes: limits.encoded_bytes })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ValuesType<E = SourceElement> {
    pub element: E,
    pub max_items: u32,
    pub max_payload_bytes: u32,
    pub max_total_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum NestedType<E = SourceElement> {
    Values(ValuesType<E>),
    BytesView { max_payload_bytes: u32 },
    RecordView { element: E, max_payload_bytes: u32 },
}

/// The source element category is retained before resolving a record's owner.
/// `Vec<u8>` is a type node, never a nominal record spelling to parse later.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum SourceElement {
    Bytes,
    Record(String),
}

impl SourceElement {
    fn from_source(ty: &Type) -> Result<Self> {
        match ty {
            Type::Named(name) => Ok(Self::Record(name.clone())),
            ty if ty.vector_element() == Some(&Type::U8) => Ok(Self::Bytes),
            _ => Err(invalid_type()),
        }
    }

    fn from_ir(ty: &crate::ir::IrType) -> Result<Self> {
        use crate::ir::IrType;
        match ty {
            IrType::Named(name) => Ok(Self::Record(name.clone())),
            ty if ty.vector_element() == Some(&IrType::U8) => Ok(Self::Bytes),
            _ => Err(invalid_type()),
        }
    }

    pub(crate) fn record_name(&self) -> Option<&str> {
        match self {
            Self::Record(name) => Some(name),
            Self::Bytes => None,
        }
    }

    fn source_type(&self) -> Type {
        match self {
            Self::Bytes => Type::vector(Type::U8),
            Self::Record(name) => Type::Named(name.clone()),
        }
    }
}

impl std::fmt::Display for SourceElement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes => formatter.write_str("Vec<u8>"),
            Self::Record(name) => formatter.write_str(name),
        }
    }
}

fn invalid_type() -> CompileError {
    CompileError::without_span("nested values require NestedValues<T, N, P, B> with N <= 32, P <= 256 and 4 <= B <= 4096; the element must be Vec<u8> or an ordinary record, and derived views carry only their declared payload bound")
        .with_code(crate::error::DiagnosticCode::E2111)
}

pub(crate) fn is_constructor(name: &str) -> bool {
    matches!(name, VALUES | BYTES_VIEW | RECORD_VIEW)
}

fn application<T>(application: &TypeApplication<T>, element: impl Fn(&T) -> Result<SourceElement>) -> Result<Option<NestedType>> {
    if !is_constructor(&application.constructor) {
        return Ok(None);
    }
    let bound = |value: &u128, maximum: u32| -> Result<u32> {
        u32::try_from(*value).ok().filter(|value| *value <= maximum).ok_or_else(invalid_type)
    };
    use TypeArgument::{Type as Ty, UnsignedConstant as N};
    Ok(Some(match (application.constructor.as_str(), application.arguments.as_slice()) {
        (VALUES, [Ty(ty), N(count), N(payload), N(total)]) => {
            let total = bound(total, MAX_TOTAL_BYTES)?;
            if total < 4 {
                return Err(invalid_type());
            }
            NestedType::Values(ValuesType {
                element: element(ty)?,
                max_items: bound(count, MAX_ITEMS)?,
                max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)?,
                max_total_bytes: total,
            })
        }
        (BYTES_VIEW, [N(payload)]) => NestedType::BytesView { max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)? },
        (RECORD_VIEW, [Ty(ty), N(payload)]) => {
            NestedType::RecordView { element: element(ty)?, max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)? }
        }
        _ => return Err(invalid_type()),
    }))
}

pub(crate) fn from_source(ty: &Type) -> Result<Option<NestedType>> {
    match ty {
        Type::Applied(applied) => application(applied, SourceElement::from_source),
        Type::Named(name) if is_nested_name(name) => Err(invalid_type()),
        _ => Ok(None),
    }
}

pub(crate) fn from_ir_application(ty: &crate::ir::IrType) -> Result<Option<NestedType>> {
    match ty {
        crate::ir::IrType::Applied(applied) => application(applied, SourceElement::from_ir),
        crate::ir::IrType::Named(name) if is_nested_name(name) => Err(invalid_type()),
        _ => Ok(None),
    }
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
    let invalid = invalid_type;
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
                element: if is_bytes_element(element) { SourceElement::Bytes } else { SourceElement::Record(element.clone()) },
                max_items: bound(count, MAX_ITEMS)?,
                max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)?,
                max_total_bytes: total,
            })
        }
        (BYTES_VIEW, [payload]) => NestedType::BytesView { max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)? },
        (RECORD_VIEW, [element, payload]) if !element.is_empty() => NestedType::RecordView {
            element: if is_bytes_element(element) { SourceElement::Bytes } else { SourceElement::Record(element.clone()) },
            max_payload_bytes: bound(payload, MAX_PAYLOAD_BYTES)?,
        },
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
    bytes_view(max_payload_bytes).source_text()
}

pub(crate) fn bytes_view(max_payload_bytes: u32) -> Type {
    Type::applied(BYTES_VIEW, vec![TypeArgument::UnsignedConstant(u128::from(max_payload_bytes))])
}

impl ValuesType {
    pub(crate) fn source_type(&self) -> Type {
        Type::applied(
            VALUES,
            vec![
                TypeArgument::Type(self.element.source_type()),
                TypeArgument::UnsignedConstant(u128::from(self.max_items)),
                TypeArgument::UnsignedConstant(u128::from(self.max_payload_bytes)),
                TypeArgument::UnsignedConstant(u128::from(self.max_total_bytes)),
            ],
        )
    }
    pub fn item_type(&self) -> Type {
        match &self.element {
            SourceElement::Bytes => bytes_view(self.max_payload_bytes),
            SourceElement::Record(_) => Type::applied(
                RECORD_VIEW,
                vec![
                    TypeArgument::Type(self.element.source_type()),
                    TypeArgument::UnsignedConstant(u128::from(self.max_payload_bytes)),
                ],
            ),
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
