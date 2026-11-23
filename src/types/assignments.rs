use super::{assignment_root_name, expr_span, type_repr, TypeChecker, TypeEnv};
use crate::ast::{AssignExpr, AssignOp, Expr, Type};
use crate::error::{CompileError, Result, Span};

impl TypeChecker<'_> {
    pub(super) fn infer_assign_expr(&mut self, env: &mut TypeEnv, assign: &AssignExpr) -> Result<Type> {
        self.reject_direct_borrow_escape(&assign.value, assign.span)?;
        match assign.target.as_ref() {
            Expr::Identifier(name) => {
                let Some(target_ty) = env.lookup(name).cloned() else {
                    return Err(CompileError::new(format!("undefined variable '{}'", name), assign.span));
                };
                let value_ty = self.infer_expr_with_expected_type(env, &assign.value, &target_ty, expr_span(&assign.value))?;
                self.reject_borrow_escape_type(&assign.value, &value_ty, assign.span)?;
                self.reject_assignment_reference_to_linear_root(env, &assign.value, assign.span)?;
                self.reject_assignment_mutable_reference_alias(&value_ty, assign.span)?;
                if self.is_linear_type(&target_ty) {
                    return Err(CompileError::new("assignment to linear/resource variables is not supported yet", assign.span));
                }
                if !env.is_mutable(name) {
                    return Err(CompileError::new(format!("variable '{}' is not mutable", name), assign.span));
                }
                match assign.op {
                    AssignOp::Assign => {
                        if !self.expr_type_compatible_with_expected(&assign.value, &value_ty, &target_ty, assign.span)? {
                            return Err(CompileError::new("assignment requires matching types", assign.span));
                        }
                    }
                    AssignOp::AddAssign => {
                        if !self.is_numeric_type(&target_ty) || !self.is_numeric_type(&value_ty) {
                            return Err(CompileError::new("'+=' requires numeric types", assign.span));
                        }
                        if !self.expr_type_compatible_with_expected(&assign.value, &value_ty, &target_ty, assign.span)? {
                            return Err(CompileError::new("'+=' requires matching numeric types", assign.span));
                        }
                    }
                }
                Ok(target_ty)
            }
            Expr::FieldAccess(_) | Expr::Index(_) => {
                let Some(root) = assignment_root_name(assign.target.as_ref()) else {
                    return Err(CompileError::new("assignment target must be rooted at a named local or parameter", assign.span));
                };
                let Some(root_ty) = env.lookup(root).cloned() else {
                    return Err(CompileError::new(format!("undefined variable '{}'", root), assign.span));
                };
                if matches!(root_ty, Type::Ref(_)) {
                    return Err(CompileError::new(
                        format!("assignment target rooted at '{}' is a read-only reference", root),
                        assign.span,
                    ));
                }
                let root_is_mut_ref = matches!(root_ty, Type::MutRef(_));
                if !root_is_mut_ref && self.is_linear_type(&root_ty) {
                    return Err(CompileError::new(
                        format!(
                            "assignment target rooted at linear/resource value '{}' is not supported; use explicit input/output parameters or consume/create sugar for ownership transitions",
                            root
                        ),
                        assign.span,
                    ));
                }
                if !env.is_mutable(root) && !root_is_mut_ref {
                    return Err(CompileError::new(format!("assignment target rooted at '{}' is not mutable", root), assign.span));
                }
                let target_ty = self.infer_expr(env, &assign.target)?;
                let value_ty = self.infer_expr_with_expected_type(env, &assign.value, &target_ty, expr_span(&assign.value))?;
                self.reject_borrow_escape_type(&assign.value, &value_ty, assign.span)?;
                self.reject_assignment_reference_to_linear_root(env, &assign.value, assign.span)?;
                self.reject_assignment_mutable_reference_alias(&value_ty, assign.span)?;
                match assign.op {
                    AssignOp::Assign => {
                        if !self.expr_type_compatible_with_expected(&assign.value, &value_ty, &target_ty, assign.span)? {
                            return Err(CompileError::new("assignment requires matching types", assign.span));
                        }
                    }
                    AssignOp::AddAssign => {
                        if !self.is_numeric_type(&target_ty) || !self.is_numeric_type(&value_ty) {
                            return Err(CompileError::new("'+=' requires numeric types", assign.span));
                        }
                        if !self.expr_type_compatible_with_expected(&assign.value, &value_ty, &target_ty, assign.span)? {
                            return Err(CompileError::new("'+=' requires matching numeric types", assign.span));
                        }
                    }
                }
                Ok(target_ty)
            }
            _ => Err(CompileError::new("invalid assignment target", assign.span)),
        }
    }

    fn reject_assignment_reference_to_linear_root(&self, env: &TypeEnv, value: &Expr, span: Span) -> Result<()> {
        self.reject_stored_linear_reference_alias(env, value, span)
    }

    fn reject_assignment_mutable_reference_alias(&self, ty: &Type, span: Span) -> Result<()> {
        if Self::type_contains_mutable_reference(ty) {
            return Err(CompileError::new(
                format!(
                    "assignment cannot store mutable reference type {}; use signature-direction outputs for Cell updates",
                    type_repr(ty)
                ),
                span,
            ));
        }
        Ok(())
    }
}
