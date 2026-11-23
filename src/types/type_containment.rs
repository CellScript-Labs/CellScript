use super::{nested, type_repr, TypeChecker};
use crate::ast::{Type, TypeArgument};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn validate_callable_return_type(
        &self,
        callable_kind: &str,
        callable_name: &str,
        return_type: &Type,
        span: Span,
    ) -> Result<()> {
        self.reject_collection_container(return_type, span)?;
        self.validate_type(return_type)?;
        if crate::cell_data_view::invocation_local(return_type) {
            return Err(CompileError::new("ExactCellSelection and CellDataView cannot escape their invocation", span));
        }
        if nested::contains_nested_type(return_type) {
            return Err(CompileError::new(
                "nested values and derived views cannot escape their invocation through a return value",
                span,
            ));
        }
        if self.type_contains_opening(return_type) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' cannot return an Opening<T>; openings are witness-only one-shot values",
                    callable_kind, callable_name
                ),
                span,
            ));
        }
        if self.type_contains_reference(return_type) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' cannot return reference type {}; references cannot escape callable boundaries",
                    callable_kind,
                    callable_name,
                    type_repr(return_type)
                ),
                span,
            ));
        }
        if callable_kind == "function" && self.type_contains_cell_backed_value(return_type) {
            return Err(CompileError::new(
                format!(
                    "function '{}' cannot return cell-backed type {}; pure helpers must return non-Cell values",
                    callable_name,
                    type_repr(return_type)
                ),
                span,
            ));
        }
        if callable_kind != "function" && self.enum_type_contains_linear_payload(return_type, &mut HashSet::new()) {
            return Err(CompileError::new(
                format!(
                    "{} '{}' cannot return a linear payload enum across an entry ABI; destructure and discharge the Cell payload locally",
                    callable_kind, callable_name
                ),
                span,
            ));
        }
        if let Type::Named(enum_name) = return_type
            && let Some(variants) = self.resolve_enum_variant_fields(enum_name)
        {
            let has_payload = variants.values().any(|fields| !fields.is_empty());
            if has_payload && callable_kind != "function" {
                return Err(CompileError::new(
                    format!(
                        "{} '{}' cannot return a payload enum across an entry ABI; payload enum returns are pure-helper ABI values",
                        callable_kind, callable_name
                    ),
                    span,
                ));
            }
            if has_payload {
                let width = self.payload_type_runtime_width(return_type, &mut HashSet::new()).ok_or_else(|| {
                    CompileError::new(format!("function '{}' payload enum return has no fixed-width ABI layout", callable_name), span)
                })?;
                if width > 16 {
                    return Err(CompileError::new(
                        format!(
                            "function '{}' payload enum return is {} bytes; 0.22 register-pair return ABI supports at most 16 bytes",
                            callable_name, width
                        ),
                        span,
                    ));
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_stored_type_has_no_references(&self, ty: &Type, owner: &str, span: Span) -> Result<()> {
        if self.type_contains_opening(ty) {
            return Err(CompileError::new(
                format!("{} cannot store Opening<T>; openings are witness-only one-shot values", owner),
                span,
            ));
        }
        if self.type_contains_reference(ty) {
            return Err(CompileError::new(
                format!("{} cannot use reference type {}; schema storage must use owned serializable values", owner, type_repr(ty)),
                span,
            ));
        }
        Ok(())
    }

    pub(super) fn type_contains_reference(&self, ty: &Type) -> bool {
        if matches!(
            self.source_nested_type(ty).as_deref(),
            Some(crate::nested_values::NestedType::BytesView { .. } | crate::nested_values::NestedType::RecordView { .. })
        ) {
            return true;
        }
        match ty {
            Type::Applied(application) => {
                application.arguments.iter().filter_map(TypeArgument::as_type).any(|ty| self.type_contains_reference(ty))
            }
            Type::Ref(_) | Type::MutRef(_) => true,
            Type::Array(inner, _) => self.type_contains_reference(inner),
            Type::Tuple(items) => items.iter().any(|item| self.type_contains_reference(item)),
            Type::Named(name) => self.named_type_contains_reference(name),
            _ => false,
        }
    }

    pub(super) fn type_contains_opening(&self, ty: &Type) -> bool {
        self.type_contains_opening_with_seen(ty, &mut HashSet::new())
    }

    fn type_contains_opening_with_seen(&self, ty: &Type, visiting: &mut HashSet<String>) -> bool {
        match ty {
            Type::Applied(application) => {
                application.constructor == crate::commitment_contract::OPENING_TYPE
                    || application
                        .arguments
                        .iter()
                        .filter_map(TypeArgument::as_type)
                        .any(|ty| self.type_contains_opening_with_seen(ty, visiting))
            }
            Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => self.type_contains_opening_with_seen(inner, visiting),
            Type::Tuple(items) => items.iter().any(|item| self.type_contains_opening_with_seen(item, visiting)),
            Type::Named(name) => {
                let base = name.as_str();
                if !visiting.insert(base.to_string()) {
                    return false;
                }
                let contains = self
                    .resolve_named_type_fields(base)
                    .is_some_and(|fields| fields.values().any(|field| self.type_contains_opening_with_seen(field, visiting)))
                    || self.resolve_enum_variant_fields(base).is_some_and(|variants| {
                        variants.values().flatten().any(|field| self.type_contains_opening_with_seen(field, visiting))
                    });
                visiting.remove(base);
                contains
            }
            _ => false,
        }
    }

    pub(super) fn type_contains_commitment(&self, ty: &Type) -> bool {
        self.type_contains_commitment_with_seen(ty, &mut HashSet::new())
    }

    fn type_contains_commitment_with_seen(&self, ty: &Type, visiting: &mut HashSet<String>) -> bool {
        match ty {
            Type::Applied(application) => {
                application.constructor == crate::commitment_contract::COMMITMENT_TYPE
                    || application
                        .arguments
                        .iter()
                        .filter_map(TypeArgument::as_type)
                        .any(|ty| self.type_contains_commitment_with_seen(ty, visiting))
            }
            Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => self.type_contains_commitment_with_seen(inner, visiting),
            Type::Tuple(items) => items.iter().any(|item| self.type_contains_commitment_with_seen(item, visiting)),
            Type::Named(name) => {
                let base = name.as_str();
                if !visiting.insert(base.to_string()) {
                    return false;
                }
                let contains = self
                    .resolve_named_type_fields(base)
                    .is_some_and(|fields| fields.values().any(|field| self.type_contains_commitment_with_seen(field, visiting)))
                    || self.resolve_enum_variant_fields(base).is_some_and(|variants| {
                        variants.values().flatten().any(|field| self.type_contains_commitment_with_seen(field, visiting))
                    });
                visiting.remove(base);
                contains
            }
            _ => false,
        }
    }

    pub(super) fn type_contains_mutable_reference(ty: &Type) -> bool {
        match ty {
            Type::Applied(application) => {
                application.arguments.iter().filter_map(TypeArgument::as_type).any(Self::type_contains_mutable_reference)
            }
            Type::MutRef(_) => true,
            Type::Array(inner, _) => Self::type_contains_mutable_reference(inner),
            Type::Tuple(items) => items.iter().any(Self::type_contains_mutable_reference),
            _ => false,
        }
    }

    pub(super) fn type_contains_cell_backed_value(&self, ty: &Type) -> bool {
        self.type_contains_cell_backed_value_with_seen(ty, &mut HashSet::new())
    }

    fn type_contains_cell_backed_value_with_seen(&self, ty: &Type, visiting: &mut HashSet<String>) -> bool {
        match ty {
            Type::Applied(application) => application
                .arguments
                .iter()
                .filter_map(TypeArgument::as_type)
                .any(|ty| self.type_contains_cell_backed_value_with_seen(ty, visiting)),
            Type::Array(inner, _) => self.type_contains_cell_backed_value_with_seen(inner, visiting),
            Type::Tuple(items) => items.iter().any(|item| self.type_contains_cell_backed_value_with_seen(item, visiting)),
            Type::Named(name) => {
                if self.resolve_cell_type_kind(name).is_some() {
                    return true;
                }
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let contains_struct = self.resolve_named_type_fields(name).is_some_and(|fields| {
                    fields.values().any(|field| self.type_contains_cell_backed_value_with_seen(field, visiting))
                });
                let contains_enum = self.resolve_enum_variant_fields(name).is_some_and(|variants| {
                    variants.values().flatten().any(|field| self.type_contains_cell_backed_value_with_seen(field, visiting))
                });
                visiting.remove(name);
                contains_struct || contains_enum
            }
            Type::Ref(_) | Type::MutRef(_) => false,
            _ => false,
        }
    }

    fn named_type_contains_reference(&self, name: &str) -> bool {
        name.contains("read_ref ") || name.contains('&')
    }

    pub(super) fn reference_target_is_cell_backed_aggregate(&self, ty: &Type) -> bool {
        match ty {
            Type::Array(_, _) | Type::Tuple(_) => self.type_contains_cell_backed_value(ty),
            _ => false,
        }
    }
}
