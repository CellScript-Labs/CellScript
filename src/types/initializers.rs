use super::{expr_span, is_state_storage_type, CellTypeKind, TypeChecker, TypeEnv};
use crate::ast::{Expr, IdentityPolicy, Type};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn check_field_initializer(
        &mut self,
        env: &mut TypeEnv,
        type_name: &str,
        fields: &[(String, Expr)],
        span: Span,
        context: &str,
    ) -> Result<()> {
        let Some(expected_fields) = self.resolve_named_type_fields(type_name) else {
            return Err(CompileError::new(format!("{} target type '{}' has no declared fields", context, type_name), span));
        };

        let mut seen = HashSet::new();
        for (field_name, value) in fields {
            if !seen.insert(field_name.clone()) {
                return Err(CompileError::new(format!("duplicate field '{}' in {} for '{}'", field_name, context, type_name), span));
            }
            let Some(expected_ty) = expected_fields.get(field_name) else {
                return Err(CompileError::new(format!("unknown field '{}' in {} for '{}'", field_name, context, type_name), span));
            };
            let actual_ty = if let Some(flow_ty) = self.flow_state_initializer_type(type_name, field_name, value, expected_ty)? {
                flow_ty
            } else {
                self.infer_expr_with_expected_type(env, value, expected_ty, expr_span(value))?
            };
            if !self.initializer_types_equal(&actual_ty, expected_ty) {
                return Err(CompileError::new(
                    format!(
                        "field '{}' in {} for '{}' has type mismatch: expected {:?}, found {:?}",
                        field_name, context, type_name, expected_ty, actual_ty
                    ),
                    expr_span(value),
                ));
            }
        }

        let missing = expected_fields
            .keys()
            .filter(|field_name| !seen.contains(*field_name))
            .filter(|field_name| {
                !(self.resolve_flow_states(type_name).is_some()
                    && !self.flows.contains_key(type_name)
                    && self.flow_state_fields.get(type_name).is_some_and(|state_field| state_field == field_name.as_str()))
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(CompileError::new(
                format!("{} for '{}' is missing field(s): {}", context, type_name, missing.join(", ")),
                span,
            ));
        }

        Ok(())
    }

    fn flow_state_initializer_type(
        &self,
        type_name: &str,
        field_name: &str,
        value: &Expr,
        expected_ty: &Type,
    ) -> Result<Option<Type>> {
        if self.flow_state_fields.get(type_name).is_none_or(|state_field| state_field != field_name) {
            return Ok(None);
        }
        let Expr::Identifier(state_name) = value else {
            return Ok(None);
        };
        if let Type::Named(enum_name) = expected_ty {
            if self.enum_variant_expr_type(state_name, expr_span(value))?.is_some_and(|ty| self.types_equal(&ty, expected_ty)) {
                return Ok(Some(expected_ty.clone()));
            }
            let Some(states) = self.resolve_flow_states(type_name) else {
                return Ok(None);
            };
            if self.canonical_state_name_for_flow(type_name, Some(enum_name), &states, state_name, expr_span(value)).is_ok() {
                return Ok(Some(expected_ty.clone()));
            }
        } else if !is_state_storage_type(expected_ty) {
            return Ok(None);
        }
        let Some((qualified_type, qualified_state)) = state_name.rsplit_once("::") else {
            return Ok(self.flow_state_index(type_name, state_name).map(|_| expected_ty.clone()));
        };
        if qualified_type == type_name {
            if self.flow_state_index(type_name, state_name).is_some() {
                return Ok(Some(expected_ty.clone()));
            }
            return Err(CompileError::new(format!("unknown flow state '{}::{}'", qualified_type, qualified_state), expr_span(value)));
        }
        if self.resolve_flow_states(qualified_type).is_some() {
            return Err(CompileError::new(
                format!(
                    "flow field '{}.{}' cannot be initialized with '{}::{}'",
                    type_name, field_name, qualified_type, qualified_state
                ),
                expr_span(value),
            ));
        }
        Ok(None)
    }

    pub(super) fn flow_state_expr_type(&self, name: &str, span: Span) -> Result<Option<Type>> {
        let Some((type_name, state_name)) = name.rsplit_once("::") else {
            return Ok(None);
        };
        let Some(states) = self.resolve_flow_states(type_name) else {
            return Ok(None);
        };
        if states.iter().any(|state| state == state_name) {
            if let Some(spec) = self.flows.get(type_name).and_then(|spec| spec.field_enum_type.as_ref()) {
                return Ok(Some(Type::Named(spec.clone())));
            }
            return Ok(Some(self.flow_state_field_type(type_name).unwrap_or(Type::U64)));
        }
        Err(CompileError::new(format!("unknown flow state '{}::{}'", type_name, state_name), span))
    }

    fn flow_state_field_type(&self, type_name: &str) -> Option<Type> {
        let field_name = self.flow_state_fields.get(type_name)?;
        self.resolve_named_type_fields(type_name)?.get(field_name).cloned()
    }

    fn flow_state_index(&self, type_name: &str, name: &str) -> Option<usize> {
        let states = self.resolve_flow_states(type_name)?;
        if let Some((qualified_type, state_name)) = name.rsplit_once("::") {
            if qualified_type != type_name {
                return None;
            }
            states.iter().position(|state| state == state_name)
        } else {
            states.iter().position(|state| state == name)
        }
    }

    pub(super) fn validate_unique_identity_policy(
        &self,
        type_name: &str,
        identity: &IdentityPolicy,
        span: Span,
        operation: &str,
    ) -> Result<()> {
        match identity {
            IdentityPolicy::None | IdentityPolicy::CkbTypeId | IdentityPolicy::ScriptArgs | IdentityPolicy::SingletonType => Ok(()),
            IdentityPolicy::Field(field) => {
                let Some(expected_fields) = self.resolve_named_type_fields(type_name) else {
                    return Err(CompileError::new(
                        format!("{} identity field target type '{}' has no declared fields", operation, type_name),
                        span,
                    ));
                };
                let Some(field_ty) = expected_fields.get(field) else {
                    return Err(CompileError::new(
                        format!("{} identity field '{}' does not exist on '{}'", operation, field, type_name),
                        span,
                    ));
                };
                if Self::identity_static_width(field_ty).is_none() {
                    return Err(CompileError::new(
                        format!(
                            "{} identity field '{}.{}' must be fixed-width so CKB runtime can compare it",
                            operation, type_name, field
                        ),
                        span,
                    ));
                }
                Ok(())
            }
        }
    }

    pub(super) fn require_create_target_cell_backed(&self, type_name: &str, span: Span) -> Result<()> {
        match self.resolve_cell_type_kind(type_name) {
            Some(CellTypeKind::Resource | CellTypeKind::Shared | CellTypeKind::Receipt) => Ok(()),
            None => Err(CompileError::new(
                format!("create target type '{}' must be a resource, shared, or receipt cell type", type_name),
                span,
            )),
        }
    }

    pub(super) fn require_read_ref_target_cell_backed(&self, type_name: &str, span: Span) -> Result<()> {
        match self.resolve_cell_type_kind(type_name) {
            Some(CellTypeKind::Resource | CellTypeKind::Shared | CellTypeKind::Receipt) => Ok(()),
            None => Err(CompileError::new(
                format!("read_ref target type '{}' must be a resource, shared, or receipt cell type", type_name),
                span,
            )),
        }
    }
}
