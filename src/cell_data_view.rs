//! Read-only, invocation-local decoding views over typed transaction Cells.
use crate::ast::ContextView;
use crate::ast::Type;
use crate::ir::IrType;

pub(crate) const PREFIX: &str = "CellDataView<";

pub(crate) fn schema(name: &str) -> Option<&str> {
    name.strip_prefix(PREFIX)?.strip_suffix('>')
}

pub(crate) fn transaction_schema(name: &str) -> Option<&str> {
    name.strip_prefix("InputView<").or_else(|| name.strip_prefix("OutputView<"))?.strip_suffix('>')
}

pub(crate) fn ast_schema(ty: &Type) -> Option<&str> {
    match ty.as_context_view()? {
        ContextView::CellData(Type::Named(schema)) => Some(schema),
        _ => None,
    }
}

pub(crate) fn ast_transaction_schema(ty: &Type) -> Option<&str> {
    match ty.as_context_view()? {
        ContextView::Input(Type::Named(schema)) | ContextView::Output(Type::Named(schema)) => Some(schema),
        _ => None,
    }
}

pub(crate) fn ir_schema(ty: &crate::ir::IrType) -> Option<&str> {
    use crate::ir::IrType;
    let IrType::Context(ContextView::CellData(schema)) = ty else { return None };
    match schema.as_ref() {
        IrType::Named(name) => Some(name),
        _ => None,
    }
}

pub(crate) fn ir_transaction_schema(ty: &crate::ir::IrType) -> Option<&str> {
    use crate::ir::IrType;
    let IrType::Context(ContextView::Input(schema) | ContextView::Output(schema)) = ty else { return None };
    match schema.as_ref() {
        IrType::Named(name) => Some(name),
        _ => None,
    }
}

pub(crate) fn contains(ty: &Type) -> bool {
    match ty {
        Type::Applied(application) => {
            matches!(ty.as_context_view(), Some(ContextView::CellData(_)))
                || application.arguments.iter().filter_map(crate::ast::TypeArgument::as_type).any(contains)
        }
        Type::Named(name) => name.split('<').next().is_some_and(|base| base.trim() == "CellDataView"),
        Type::Ref(inner) | Type::MutRef(inner) | Type::Array(inner, _) => contains(inner),
        Type::Tuple(items) => items.iter().any(contains),
        _ => false,
    }
}

pub(crate) fn invocation_local(ty: &Type) -> bool {
    contains(ty) || crate::cell_selection::contains_selection(ty)
}

pub(crate) fn ir_fixed_record_width<'a>(fields: impl Iterator<Item = &'a IrType>) -> Option<usize> {
    fields
        .map(|ty| match ty {
            IrType::U8 | IrType::U16 | IrType::U32 | IrType::U64 | IrType::U128 | IrType::Hash | IrType::Address => {
                crate::ir::fixed_layout_width(ty)
            }
            IrType::Array(item, count) if **item == IrType::U8 && (1..=256).contains(count) => Some(*count),
            _ => None,
        })
        .try_fold(0usize, |width, field| width.checked_add(field?))
        .filter(|width| *width > 0)
}

pub(crate) fn ir_fixed_record_array_layout(ty: &IrType, record_width: impl FnOnce(&str) -> Option<usize>) -> Option<(usize, usize)> {
    let IrType::Array(element, count) = ty else { return None };
    let IrType::Named(name) = element.as_ref() else { return None };
    let width = record_width(name)?;
    let bytes = width.checked_mul(*count)?;
    (1..=512).contains(&bytes).then_some((width, bytes))
}
