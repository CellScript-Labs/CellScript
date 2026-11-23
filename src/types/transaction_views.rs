use super::{
    type_repr, TypeChecker, CKB_CELL_DEP_VIEW_TYPE, CKB_HEADER_DEP_VIEW_TYPE, CKB_INPUT_VIEW_TYPE, CKB_OUTPUT_VIEW_TYPE,
    CKB_OUT_POINT_TYPE, CKB_SCRIPT_VIEW_TYPE, CKB_WITNESS_ARGS_VIEW_TYPE, CKB_WITNESS_BYTES_MAX,
};
use crate::ast::{CallExpr, Expr, Type};
use crate::error::{CompileError, Result};

impl TypeChecker<'_> {
    pub(super) fn is_cell_view_type(ty: &Type) -> bool {
        matches!(ty, Type::U64)
            || matches!(
                ty.as_context_view(),
                Some(crate::ast::ContextView::Input(_) | crate::ast::ContextView::Output(_) | crate::ast::ContextView::CellDep)
            )
    }

    pub(super) fn is_witness_args_view_type(ty: &Type) -> bool {
        matches!(ty, Type::U64) || matches!(ty, Type::Named(name) if name == CKB_WITNESS_ARGS_VIEW_TYPE)
    }

    pub(super) fn witness_bytes_view_parts(ty: &Type) -> Option<(String, u64)> {
        let view = ty.as_witness_bytes_view()?;
        (view.maximum <= CKB_WITNESS_BYTES_MAX).then(|| (view.owner.as_str().to_string(), view.maximum))
    }

    pub(super) fn witness_bytes_view_type(name: &str, call: &CallExpr, arg_types: &[Type]) -> Result<Option<Type>> {
        use cellscript_artifact_checker::view_types::WitnessBytesOwner;
        let owner = match name {
            "witness::bounded_raw" => WitnessBytesOwner::Raw,
            "witness::bounded_lock" => WitnessBytesOwner::Lock,
            "witness::bounded_entry" => WitnessBytesOwner::Entry,
            "witness::bounded_output_type" => WitnessBytesOwner::OutputType,
            _ => return Ok(None),
        };
        if arg_types.len() != 2 {
            return Err(CompileError::new(format!("{} expects (witness_view, maximum_bytes)", name), call.span));
        }
        if !Self::is_witness_args_view_type(&arg_types[0]) || arg_types[1] != Type::U64 {
            return Err(CompileError::new(format!("{} expects (witness_view, maximum_bytes: u64)", name), call.span));
        }
        let maximum = match call.args.get(1) {
            Some(Expr::Integer(value)) if *value <= u128::from(CKB_WITNESS_BYTES_MAX) => *value as u64,
            Some(Expr::Integer(_)) => {
                return Err(CompileError::new(format!("{} maximum_bytes must be in 0..={}", name, CKB_WITNESS_BYTES_MAX), call.span));
            }
            _ => {
                return Err(CompileError::new(format!("{} maximum_bytes must be an integer literal", name), call.span));
            }
        };
        Ok(Some(Type::witness_bytes_view(owner, maximum)))
    }

    pub(super) fn is_input_view_type(ty: &Type) -> bool {
        matches!(ty, Type::U64) || matches!(ty.as_context_view(), Some(crate::ast::ContextView::Input(_)))
    }

    pub(super) fn is_header_dep_view_type(ty: &Type) -> bool {
        matches!(ty, Type::U64) || matches!(ty, Type::Named(name) if name == CKB_HEADER_DEP_VIEW_TYPE)
    }

    fn typed_view_type_argument(&self, name: &str, call: &CallExpr) -> Result<Type> {
        if call.type_args.len() != 1 {
            return Err(CompileError::new(format!("{} requires exactly one cell type argument", name), call.span));
        }
        let ty = &call.type_args[0];
        let Some(type_name) = Self::base_type_name(ty) else {
            return Err(CompileError::new(format!("{} type argument must be a named cell-backed type", name), call.span));
        };
        if self.resolve_cell_type_kind(type_name).is_none() {
            return Err(CompileError::new(
                format!("{} type argument '{}' is not a cell-backed resource, shared type, or receipt", name, type_repr(ty)),
                call.span,
            ));
        }
        Ok(ty.clone())
    }

    pub(super) fn infer_transaction_view_constructor(&self, name: &str, call: &CallExpr, arg_types: &[Type]) -> Result<Option<Type>> {
        let typed_view = match name {
            "ckb::input" | "ckb::group_input" => Some(CKB_INPUT_VIEW_TYPE),
            "ckb::output" | "ckb::group_output" => Some(CKB_OUTPUT_VIEW_TYPE),
            _ => None,
        };
        if let Some(view_type) = typed_view {
            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
            if arg_types[0] != Type::U64 {
                return Err(CompileError::new(format!("{} expects a u64 index", name), call.span));
            }
            let schema = self.typed_view_type_argument(name, call)?;
            let view = if view_type == CKB_INPUT_VIEW_TYPE {
                crate::ast::ContextView::Input(schema)
            } else {
                crate::ast::ContextView::Output(schema)
            };
            let result = Type::context_view(view);
            self.validate_type(&result)?;
            return Ok(Some(result));
        }

        let result = match name {
            "ckb::cell_dep" => Some(Type::Named(CKB_CELL_DEP_VIEW_TYPE.to_string())),
            "ckb::header_dep" => Some(Type::Named(CKB_HEADER_DEP_VIEW_TYPE.to_string())),
            "witness::args" => Some(Type::Named(CKB_WITNESS_ARGS_VIEW_TYPE.to_string())),
            _ => None,
        };
        if let Some(result) = result {
            self.validate_type(&result)?;
            if !call.type_args.is_empty() {
                return Err(CompileError::new(format!("{} does not accept type arguments", name), call.span));
            }
            self.validate_builtin_arity(name, 1, arg_types, call.span)?;
            if arg_types[0] != Type::U64 {
                return Err(CompileError::new(format!("{} expects a u64 index", name), call.span));
            }
            return Ok(Some(result));
        }

        let result = match name {
            "ckb::input_out_point" => {
                self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                if !Self::is_input_view_type(&arg_types[0]) {
                    return Err(CompileError::new("ckb::input_out_point expects an InputView<T>", call.span));
                }
                Some(Type::Named(CKB_OUT_POINT_TYPE.to_string()))
            }
            "ckb::lock_script" | "ckb::type_script" => {
                self.validate_builtin_arity(name, 1, arg_types, call.span)?;
                if !Self::is_cell_view_type(&arg_types[0]) {
                    return Err(CompileError::new(format!("{} expects a cell-backed transaction view", name), call.span));
                }
                Some(Type::Named(CKB_SCRIPT_VIEW_TYPE.to_string()))
            }
            _ => None,
        };
        if result.is_some() && !call.type_args.is_empty() {
            return Err(CompileError::new(format!("{} does not accept type arguments", name), call.span));
        }
        if let Some(result) = &result {
            self.validate_type(result)?;
        }
        Ok(result)
    }
}
