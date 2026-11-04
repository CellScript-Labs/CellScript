//! Read-only, invocation-local decoding views over typed transaction Cells.
use crate::ast::ContextView;
use crate::ast::Type;

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
