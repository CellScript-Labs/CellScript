use super::{
    assignment_root_name, expr_contains_borrow_marker, expr_span, expr_uses_identifier, type_repr, ActiveBorrow, CallableKind,
    TypeChecker, TypeEnv,
};
use crate::ast::{expression_fails_closed, CallExpr, Expr, ReturnStmt, Stmt, Type, UnaryOp};
use crate::error::{CompileError, Result, Span};

impl TypeChecker<'_> {
    pub(super) fn mark_expr_as_moved(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Identifier(name) => {
                self.reject_active_borrow_root_lifecycle(name, "move", expr_span(expr))?;
                if let Some(ty) = env.lookup(name).cloned()
                    && self.is_linear_type(&ty)
                {
                    env.consume(name)?;
                }
                Ok(())
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.mark_expr_as_moved(env, item)?;
                }
                Ok(())
            }
            Expr::Cast(cast) => self.mark_expr_as_moved(env, &cast.expr),
            Expr::Assign(assign) => self.mark_expr_as_moved(env, &assign.value),
            Expr::Preserve(_) => Ok(()),
            Expr::Claim(_) | Expr::Settle(_) | Expr::CreateUnique(_) | Expr::ReplaceUnique(_) => Ok(()),
            Expr::Assert(assert_expr) => self.mark_expr_as_moved(env, &assert_expr.condition),
            Expr::Require(require_expr) => {
                self.mark_expr_as_moved(env, &require_expr.condition)?;
                if let Some(message) = &require_expr.message {
                    self.mark_expr_as_moved(env, message)?;
                }
                Ok(())
            }
            Expr::If(if_expr) => {
                let mut then_env = env.child();
                self.mark_expr_as_moved(&mut then_env, &if_expr.then_branch)?;
                let mut else_env = env.child();
                self.mark_expr_as_moved(&mut else_env, &if_expr.else_branch)?;
                match (expression_fails_closed(&if_expr.then_branch), expression_fails_closed(&if_expr.else_branch)) {
                    (true, true) => Ok(()),
                    (true, false) => {
                        env.merge_existing_linear_states_from(&else_env);
                        Ok(())
                    }
                    (false, true) => {
                        env.merge_existing_linear_states_from(&then_env);
                        Ok(())
                    }
                    (false, false) => env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_expr.span),
                }
            }
            Expr::Match(match_expr) => {
                let mut arm_envs = Vec::with_capacity(match_expr.arms.len());
                for arm in &match_expr.arms {
                    let mut arm_env = env.child();
                    self.mark_expr_as_moved(&mut arm_env, &arm.value)?;
                    if !expression_fails_closed(&arm.value) {
                        arm_envs.push(arm_env);
                    }
                }
                env.merge_match_linear_states(&arm_envs, match_expr.span)
            }
            Expr::Block(_) => Ok(()),
            _ => Ok(()),
        }
    }

    pub(super) fn reject_local_reference_to_linear_root(&self, env: &TypeEnv, value: &Expr, ty: &Type, span: Span) -> Result<()> {
        self.reject_stored_linear_reference_alias(env, value, span)?;
        if matches!(value, Expr::Unary(_)) {
            self.reject_unrooted_linear_reference_type(ty, span)?;
        }
        Ok(())
    }

    pub(super) fn reject_borrow_escape_type(&self, expr: &Expr, ty: &Type, span: Span) -> Result<()> {
        if !self.type_contains_reference(ty) {
            return Ok(());
        }
        let Some(borrow) = self.active_borrow_used_by_expr(expr) else {
            return Ok(());
        };
        Err(CompileError::new(
            format!("borrow '{}' cannot escape borrow block rooted at linear value '{}'", borrow.binding, borrow.root),
            span,
        ))
    }

    pub(super) fn reject_direct_borrow_escape(&self, expr: &Expr, span: Span) -> Result<()> {
        let Some(borrow) = self.active_borrows.iter().rev().find(|borrow| expr_contains_borrow_marker(expr, &borrow.binding)) else {
            return Ok(());
        };
        Err(CompileError::new(
            format!("borrow '{}' cannot escape borrow block rooted at linear value '{}'", borrow.binding, borrow.root),
            span,
        ))
    }

    pub(super) fn active_borrow_used_by_expr(&self, expr: &Expr) -> Option<&ActiveBorrow> {
        self.active_borrows.iter().rev().find(|borrow| expr_uses_identifier(expr, &borrow.binding))
    }

    pub(super) fn reject_active_borrow_root_lifecycle(&self, root: &str, operation: &str, span: Span) -> Result<()> {
        let Some(borrow) = self.active_borrows.iter().rev().find(|borrow| borrow.root == root) else {
            return Ok(());
        };
        Err(CompileError::new(
            format!("borrow '{}' cannot cross {} of linear root '{}'", borrow.binding, operation, borrow.root),
            span,
        ))
    }

    pub(super) fn validate_borrow_call(
        &self,
        callee_name: &str,
        callee_kind: CallableKind,
        params: &[Type],
        arg_types: &[Type],
        call: &CallExpr,
    ) -> Result<()> {
        for (index, arg) in call.args.iter().enumerate() {
            let Some(borrow) = self.active_borrow_used_by_expr(arg) else {
                continue;
            };
            // Collection reads return copies. A scalar/tuple computed from a
            // borrow does not retain it; the argument has already been checked
            // without allowing owner or member-handle materialization.
            if self.collection_owner(&borrow.root_ty).is_some()
                && arg_types.get(index).is_some_and(|ty| !self.contains_collection_owner(ty) && !self.type_contains_reference(ty))
            {
                continue;
            }
            if !matches!(arg, Expr::Identifier(name) if name == &borrow.binding) {
                return Err(CompileError::new(
                    format!("borrow '{}' cannot escape borrow block rooted at linear value '{}'", borrow.binding, borrow.root),
                    call.span,
                ));
            }
            if callee_kind != CallableKind::Function {
                return Err(CompileError::new(
                    format!("callable '{}' cannot receive borrowed linear view '{}'", callee_name, borrow.binding),
                    call.span,
                ));
            }
            self.require_effect(
                callee_name,
                false,
                call.span,
                "function cannot receive borrowed linear view unless it is observation-only",
            );
            let compatible =
                params.get(index).is_some_and(|param| matches!(param, Type::Ref(inner) if self.types_equal(inner, &borrow.root_ty)));
            if !compatible {
                return Err(CompileError::new(
                    format!(
                        "function '{}' argument {} must declare a dedicated read-only &{} parameter to receive borrowed view '{}'",
                        callee_name,
                        index + 1,
                        type_repr(&borrow.root_ty),
                        borrow.binding
                    ),
                    call.span,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn reject_borrow_view_in_builtin_call(&self, callee_name: &str, call: &CallExpr) -> Result<()> {
        let Some(borrow) = call.args.iter().find_map(|arg| self.active_borrow_used_by_expr(arg)) else {
            return Ok(());
        };
        Err(CompileError::new(
            format!(
                "builtin '{}' has no dedicated borrowed-view parameter and cannot receive borrowed linear view '{}'",
                callee_name, borrow.binding
            ),
            call.span,
        ))
    }

    pub(super) fn reject_stored_linear_reference_alias(&self, env: &TypeEnv, expr: &Expr, span: Span) -> Result<()> {
        match expr {
            Expr::Unary(unary) if matches!(unary.op, UnaryOp::Ref) => {
                if let Some(root) = assignment_root_name(&unary.expr)
                    && let Some(root_ty) = env.lookup(root)
                    && self.is_linear_type(root_ty)
                {
                    return Err(CompileError::new(
                                format!(
                                    "local binding cannot store a read-only reference rooted at linear/resource value '{}'; pass the reference directly to a helper call",
                                    root
                                ),
                                span,
                            ));
                }
                Ok(())
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.reject_stored_linear_reference_alias(env, item, span)?;
                }
                Ok(())
            }
            Expr::Cast(cast) => self.reject_stored_linear_reference_alias(env, &cast.expr, span),
            Expr::If(if_expr) => {
                self.reject_stored_linear_reference_alias(env, &if_expr.then_branch, span)?;
                self.reject_stored_linear_reference_alias(env, &if_expr.else_branch, span)
            }
            Expr::Match(match_expr) => {
                for arm in &match_expr.arms {
                    self.reject_stored_linear_reference_alias(env, &arm.value, span)?;
                }
                Ok(())
            }
            Expr::Block(stmts) => self.reject_stored_linear_reference_alias_in_tail_stmt(env, stmts, span),
            _ => Ok(()),
        }
    }

    fn reject_stored_linear_reference_alias_in_tail_stmt(&self, env: &TypeEnv, stmts: &[Stmt], span: Span) -> Result<()> {
        let Some(last) = stmts.last() else {
            return Ok(());
        };
        match last {
            Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                self.reject_stored_linear_reference_alias(env, expr, span)
            }
            Stmt::If(if_stmt) => {
                self.reject_stored_linear_reference_alias_in_tail_stmt(env, &if_stmt.then_branch, span)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.reject_stored_linear_reference_alias_in_tail_stmt(env, else_branch, span)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn reject_unrooted_linear_reference_type(&self, ty: &Type, span: Span) -> Result<()> {
        if let Type::Ref(inner) = ty
            && self.is_linear_type(inner)
        {
            return Err(CompileError::new(
                    "local binding cannot store a read-only reference to a linear/resource value; bind the cell value itself or pass the reference directly",
                    span,
                ));
        }
        Ok(())
    }

    pub(super) fn reject_local_mutable_reference_alias(&self, ty: &Type, span: Span) -> Result<()> {
        if Self::type_contains_mutable_reference(ty) {
            return Err(CompileError::new(
                format!(
                    "local binding cannot store mutable reference type {}; use signature-direction outputs for Cell updates",
                    type_repr(ty)
                ),
                span,
            ));
        }
        Ok(())
    }
}
