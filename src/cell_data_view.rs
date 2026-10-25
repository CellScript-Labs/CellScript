//! Read-only, invocation-local decoding views over typed transaction Cells.
use crate::ast::Type;

pub(crate) const PREFIX: &str = "CellDataView<";

pub(crate) fn schema(name: &str) -> Option<&str> {
    name.strip_prefix(PREFIX)?.strip_suffix('>')
}

pub(crate) fn transaction_schema(name: &str) -> Option<&str> {
    name.strip_prefix("InputView<").or_else(|| name.strip_prefix("OutputView<"))?.strip_suffix('>')
}

pub(crate) fn contains(ty: &Type) -> bool {
    match ty {
        Type::Named(name) => {
            name.split('<').next().is_some_and(|base| base.trim() == "CellDataView")
                || crate::generics::applied_type(name)
                    .is_some_and(|(_, args)| args.iter().any(|arg| contains(&Type::Named(arg.clone()))))
        }
        Type::Ref(inner) | Type::MutRef(inner) | Type::Array(inner, _) => contains(inner),
        Type::Tuple(items) => items.iter().any(contains),
        _ => false,
    }
}

pub(crate) fn invocation_local(ty: &Type) -> bool {
    contains(ty) || crate::cell_selection::contains_selection(ty)
}
