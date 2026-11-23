use super::{lock_args_static_type_len, nested, param_source_repr, type_repr, TypeChecker, TypeEnv};
use crate::ast::{parse_bounded_collection_type, BoundedCollectionKind, Param, ParamSource, Type};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn bind_callable_params(
        &self,
        env: &mut TypeEnv,
        params: &[Param],
        callable_kind: &str,
        callable_name: &str,
    ) -> Result<()> {
        self.bind_callable_params_with_non_linear(env, params, callable_kind, callable_name, &HashSet::new())
    }

    pub(super) fn bind_callable_params_with_non_linear(
        &self,
        env: &mut TypeEnv,
        params: &[Param],
        callable_kind: &str,
        callable_name: &str,
        non_linear_params: &HashSet<String>,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        for param in params {
            if param.name == "_" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' parameter must have a stable name; '_' is reserved for local wildcard bindings",
                        callable_kind, callable_name
                    ),
                    param.span,
                ));
            }
            if !seen.insert(param.name.clone()) {
                return Err(CompileError::new(
                    format!("duplicate parameter '{}' in {} '{}'", param.name, callable_kind, callable_name),
                    param.span,
                ));
            }
            self.validate_type(&param.ty).map_err(|mut error| {
                if error.span == Span::default() {
                    error.span = param.span;
                }
                error
            })?;
            // Action and Lock witness parameters are the entry-ABI position for the
            // bounded 656-byte handle encoding; every other parameter kind
            // and callable class stays identity-only.
            let witness_admitted = matches!(param.source, ParamSource::Witness)
                && matches!(callable_kind, "action" | "lock")
                && self.is_open_handle_type(&param.ty);
            if !witness_admitted {
                self.reject_open_handle_value_type(
                    &param.ty,
                    &format!("{} '{}' parameter '{}'", callable_kind, callable_name, param.name),
                    param.span,
                )?;
            }
            if callable_kind != "function" && self.enum_type_contains_linear_payload(&param.ty, &mut HashSet::new()) {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' parameter '{}' cannot cross an entry ABI with a linear payload enum; construct and destructure it within one callable",
                        callable_kind, callable_name, param.name
                    ),
                    param.span,
                ));
            }
            self.validate_callable_param_source(param, callable_kind, callable_name)?;
            self.validate_callable_param_reference_shape(param, callable_kind, callable_name)?;
            self.validate_callable_param_state_authority(param, callable_kind, callable_name)?;
            self.validate_callable_param_mutability(param)?;
            let is_opening = param.ty.opening_value_type().is_some();
            let is_linear = is_opening || (self.is_linear_type(&param.ty) && !non_linear_params.contains(param.name.as_str()));
            env.bind_new(param.name.clone(), param.ty.clone(), is_linear, param.is_mut, param.span)?;
        }
        Ok(())
    }

    fn validate_callable_param_source(&self, param: &Param, callable_kind: &str, callable_name: &str) -> Result<()> {
        if self.contains_collection_owner(&param.ty)
            && !(callable_kind == "function"
                && self.collection_reference(&param.ty).is_some()
                && param.source == ParamSource::Default
                && !param.is_mut
                && self.nested_readonly_helper(callable_name))
        {
            return Err(CompileError::new(
                "collection parameters require an immutable &Owner in an observation-only helper; owners cannot cross entry boundaries",
                param.span,
            ));
        }
        if crate::cell_data_view::invocation_local(&param.ty) {
            return Err(CompileError::new(
                "ExactCellSelection and CellDataView must be constructed locally, never supplied through a callable parameter",
                param.span,
            ));
        }
        if nested::contains_nested_type(&param.ty)
            && !(!param.is_mut
                && !param.is_ref
                && ((nested::whole_values(&param.ty)
                    && param.source == ParamSource::Witness
                    && matches!(callable_kind, "action" | "lock"))
                    || (param.source == ParamSource::Default
                        && nested::direct_view(&param.ty)
                        && callable_kind == "function"
                        && self.nested_readonly_helper(callable_name))))
        {
            return Err(CompileError::new("nested parameters require an immutable value binding on an observation-only helper; only complete NestedValues may enter through an explicit action or lock witness binding", param.span));
        }
        if self.type_contains_opening(&param.ty) && !param.ty.opening_value_type().is_some() {
            return Err(CompileError::new(
                format!("parameter '{}' cannot nest Opening<T>; openings must be direct witness bindings", param.name),
                param.span,
            ));
        }
        if param.ty.opening_value_type().is_some()
            && (param.source != ParamSource::Witness || !matches!(callable_kind, "action" | "lock"))
        {
            return Err(CompileError::new(
                format!("opening parameter '{}' must be an explicit witness binding on an action or lock", param.name),
                param.span,
            ));
        }
        if param.source == ParamSource::Default {
            if parse_bounded_collection_type(&param.ty).is_some() {
                return Err(CompileError::new(
                    format!(
                        "bounded collection parameter '{}' requires an explicit source classification; use input for BoundedCellSet or witness for BoundedList",
                        param.name
                    ),
                    param.span,
                ));
            }
            return Ok(());
        }
        if param.source == ParamSource::Input {
            if callable_kind != "action" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' parameter '{}' cannot use input source classification; input parameters are action verifier bindings",
                        callable_kind, callable_name, param.name
                    ),
                    param.span,
                ));
            }
            if param.is_mut || param.is_ref || param.is_read_ref || matches!(param.ty, Type::Ref(_) | Type::MutRef(_)) {
                return Err(CompileError::new(
                    format!("input action parameter '{}' must use 'input name: T' without mut/ref/read_ref modifiers", param.name),
                    param.span,
                ));
            }
            if let Some(collection) = parse_bounded_collection_type(&param.ty) {
                if collection.kind != BoundedCollectionKind::CellSet {
                    return Err(CompileError::new(
                        format!("input parameter '{}' may use BoundedCellSet<T, N>, not BoundedList<T, N>", param.name),
                        param.span,
                    ));
                }
                return Ok(());
            }
            let Some(name) = Self::base_type_name(&param.ty) else {
                return Err(CompileError::new(
                    format!("input action parameter '{}' must name a Cell-backed resource, shared cell, or receipt type", param.name),
                    param.span,
                ));
            };
            if self.resolve_cell_type_kind(name).is_none() {
                return Err(CompileError::new(
                    format!(
                        "input action parameter '{}' references non-Cell type {}; input only marks a consumed transaction input Cell",
                        param.name,
                        type_repr(&param.ty)
                    ),
                    param.span,
                ));
            }
            return Ok(());
        }
        if param.source == ParamSource::Output {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use output source classification; bind transaction outputs on the action return side (`action f(...) -> name: T`)",
                    callable_kind, callable_name, param.name
                ),
                param.span,
            ));
        }
        if param.source == ParamSource::Witness {
            if callable_kind != "action" && callable_kind != "lock" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' parameter '{}' cannot use witness source classification; witness parameters are action or lock verifier bindings",
                        callable_kind, callable_name, param.name
                    ),
                    param.span,
                ));
            }
            if let Some(collection) = parse_bounded_collection_type(&param.ty)
                && collection.kind != BoundedCollectionKind::List
            {
                return Err(CompileError::new(
                    format!("witness parameter '{}' may use BoundedList<T, N>, not BoundedCellSet<T, N>", param.name),
                    param.span,
                ));
            }
        } else if callable_kind != "lock" {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use {} source classification; protected and lock_args are lock parameter source syntax",
                    callable_kind,
                    callable_name,
                    param.name,
                    param_source_repr(param.source)
                ),
                param.span,
            ));
        }

        match param.source {
            ParamSource::Protected => {
                if param.is_mut || param.is_read_ref || param.is_ref {
                    return Err(CompileError::new(
                        format!(
                            "protected lock parameter '{}' must use 'protected name: T' without mut/ref/read_ref modifiers",
                            param.name
                        ),
                        param.span,
                    ));
                }
                let Type::Ref(inner) = &param.ty else {
                    return Err(CompileError::new(
                        format!("protected lock parameter '{}' must lower to a read-only Cell view", param.name),
                        param.span,
                    ));
                };
                let Some(name) = Self::base_type_name(inner) else {
                    return Err(CompileError::new(
                        format!(
                            "protected lock parameter '{}' must name a Cell-backed resource, shared cell, or receipt type",
                            param.name
                        ),
                        param.span,
                    ));
                };
                if self.resolve_cell_type_kind(name).is_none() {
                    return Err(CompileError::new(
                        format!(
                            "protected lock parameter '{}' references non-Cell type {}; protected only marks the current lock invocation's Cell spend surface",
                            param.name,
                            type_repr(inner)
                        ),
                        param.span,
                    ));
                }
            }
            ParamSource::Witness => {
                if param.is_mut || param.is_read_ref || matches!(param.ty, Type::Ref(_) | Type::MutRef(_)) {
                    return Err(CompileError::new(
                        format!(
                            "witness lock parameter '{}' must be plain transaction witness data, not a Cell reference",
                            param.name
                        ),
                        param.span,
                    ));
                }
            }
            ParamSource::LockArgs => {
                if param.is_mut || param.is_read_ref || matches!(param.ty, Type::Ref(_) | Type::MutRef(_)) {
                    return Err(CompileError::new(
                        format!(
                            "lock_args lock parameter '{}' must be plain typed script args data, not a Cell reference",
                            param.name
                        ),
                        param.span,
                    ));
                }
                if lock_args_static_type_len(&param.ty).is_none() {
                    return Err(CompileError::new(
                        format!(
                            "lock_args lock parameter '{}' must use a fixed-width script-args type such as Address, Hash, integer, bool, or [u8; N]",
                            param.name
                        ),
                        param.span,
                    ));
                }
            }
            ParamSource::Default | ParamSource::Input | ParamSource::Output => {}
        }
        Ok(())
    }

    pub(super) fn identity_static_width(ty: &Type) -> Option<usize> {
        lock_args_static_type_len(ty)
    }

    fn validate_callable_param_reference_shape(&self, param: &Param, callable_kind: &str, callable_name: &str) -> Result<()> {
        if matches!(param.ty, Type::MutRef(_)) {
            return Err(CompileError::new(
                format!(
                    "`&mut` Cell parameters are not valid at callable boundaries; use `action(before: T) -> after: T` plus `transition` and `require` constraints in {} '{}'",
                    callable_kind, callable_name
                ),
                param.span,
            ));
        }
        if matches!(callable_kind, "action" | "lock")
            && matches!(param.ty, Type::Ref(_))
            && !param.is_read_ref
            && param.source != ParamSource::Protected
        {
            let help = if callable_kind == "lock" {
                "use `protected name: T` for the guarded lock cell or `read name: T` for a read-only referenced cell"
            } else {
                "use `read name: T` for a read-only referenced cell, or a signature input/output binding for consumed/proposed cells"
            };
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use bare `&T` at the verifier boundary; {}",
                    callable_kind, callable_name, param.name, help
                ),
                param.span,
            ));
        }
        let nested_reference = match &param.ty {
            Type::Ref(inner) | Type::MutRef(inner) => self.type_contains_reference(inner),
            ty => self.type_contains_reference(ty),
        };
        if nested_reference && !nested::direct_view(&param.ty) {
            return Err(CompileError::new(
                format!(
                    "parameter '{}' in {} '{}' cannot contain nested reference type {}; references are only supported as top-level callable parameter types",
                    param.name,
                    callable_kind,
                    callable_name,
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        if let Type::Ref(inner) | Type::MutRef(inner) = &param.ty
            && self.reference_target_is_cell_backed_aggregate(inner)
        {
            return Err(CompileError::new(
                    format!(
                        "parameter '{}' in {} '{}' cannot use reference to aggregate containing cell-backed values {}; use a direct '&T' helper view or named action outputs instead",
                        param.name,
                        callable_kind,
                        callable_name,
                        type_repr(&param.ty)
                    ),
                    param.span,
                ));
        }
        Ok(())
    }

    fn validate_callable_param_state_authority(&self, param: &Param, callable_kind: &str, callable_name: &str) -> Result<()> {
        // A typed transaction view carries a source/index word, not the Cell
        // itself. Permit that word through immutable read-only helpers without
        // exempting containers, references or public entry ABI parameters from
        // the ordinary Cell-ownership checks below.
        if callable_kind == "function"
            && param.source == ParamSource::Default
            && !param.is_mut
            && crate::cell_data_view::ast_transaction_schema(&param.ty).is_some()
        {
            self.require_effect(callable_name, false, param.span, "transaction-view helper must be observation-only");
            return Ok(());
        }
        if callable_kind != "action" && matches!(param.ty, Type::MutRef(_)) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use mutable reference type {}; use signature-direction outputs for Cell updates",
                    callable_kind,
                    callable_name,
                    param.name,
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        if callable_kind != "action" && self.type_contains_cell_backed_value(&param.ty) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' parameter '{}' cannot use owned cell-backed type {}; use a read-only '&T' parameter for predicate/helper reads or transition ownership in an action",
                    callable_kind,
                    callable_name,
                    param.name,
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        Ok(())
    }

    fn validate_callable_param_mutability(&self, param: &Param) -> Result<()> {
        if !param.is_mut {
            return Ok(());
        }
        if param.is_read_ref || matches!(param.ty, Type::Ref(_)) {
            return Err(CompileError::new(
                format!(
                    "parameter '{}' is a read-only reference; Cell state updates must be modeled with `action(before: T) -> after: T` plus `transition` and `require` constraints",
                    param.name
                ),
                param.span,
            ));
        }
        if matches!(param.ty, Type::MutRef(_)) {
            return Err(CompileError::new(
                format!(
                    "parameter '{}' cannot use `&mut` Cell syntax; use `action(before: T) -> after: T` plus `transition` and `require` constraints",
                    param.name
                ),
                param.span,
            ));
        }
        if Self::base_type_name(&param.ty).and_then(|name| self.resolve_cell_type_kind(name)).is_some() {
            return Err(CompileError::new(
                format!(
                    "cell-backed parameter '{}' cannot use leading 'mut'; use named action outputs (`action(before: {}) -> after: {}`) or consume/create sugar",
                    param.name,
                    type_repr(&param.ty),
                    type_repr(&param.ty)
                ),
                param.span,
            ));
        }
        Ok(())
    }
}
