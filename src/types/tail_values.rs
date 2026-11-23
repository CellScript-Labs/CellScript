use super::{expr_span, TypeChecker, TypeEnv};
use crate::ast::{IfStmt, Stmt, Type};
use crate::error::{CompileError, Result, Span};

impl TypeChecker<'_> {
    pub(super) fn infer_tail_block_value(&mut self, env: &mut TypeEnv, stmts: &[Stmt]) -> Result<Type> {
        let Some((last, prefix)) = stmts.split_last() else {
            return Ok(Type::Unit);
        };
        for stmt in prefix {
            self.check_stmt(env, stmt)?;
        }
        match last {
            Stmt::Expr(expr) => {
                let ty = self.infer_expr(env, expr)?;
                if self.is_linear_type(&ty) {
                    self.mark_expr_as_moved(env, expr)?;
                }
                Ok(ty)
            }
            Stmt::If(if_stmt) if if_stmt.else_branch.is_some() => self.infer_tail_if_stmt_value(env, if_stmt),
            stmt => {
                self.check_stmt(env, stmt)?;
                Ok(Type::Unit)
            }
        }
    }

    pub(super) fn infer_tail_block_value_with_expected_type(
        &mut self,
        env: &mut TypeEnv,
        stmts: &[Stmt],
        expected_ty: &Type,
        span: Span,
    ) -> Result<Type> {
        let Some((last, prefix)) = stmts.split_last() else {
            return Ok(Type::Unit);
        };
        for stmt in prefix {
            self.check_stmt(env, stmt)?;
        }
        match last {
            Stmt::Expr(expr) => {
                let ty = self.infer_expr_with_expected_type(env, expr, expected_ty, expr_span(expr))?;
                if !self.initializer_types_equal(&ty, expected_ty) {
                    return Err(CompileError::new(
                        format!("block expression type mismatch: expected {:?}, found {:?}", expected_ty, ty),
                        span,
                    ));
                }
                if self.is_linear_type(&ty) {
                    self.mark_expr_as_moved(env, expr)?;
                }
                Ok(expected_ty.clone())
            }
            Stmt::If(if_stmt) if if_stmt.else_branch.is_some() => {
                self.infer_tail_if_stmt_value_with_expected_type(env, if_stmt, expected_ty)
            }
            stmt => {
                self.check_stmt(env, stmt)?;
                Ok(Type::Unit)
            }
        }
    }

    fn infer_tail_if_stmt_value(&mut self, env: &mut TypeEnv, if_stmt: &IfStmt) -> Result<Type> {
        let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
        if !self.is_bool_type(&cond_ty) {
            return Err(CompileError::new("if condition must be boolean", if_stmt.span));
        }
        let Some(else_branch) = &if_stmt.else_branch else {
            return Ok(Type::Unit);
        };

        let mut then_env = env.child();
        let then_ty = self.infer_tail_block_value(&mut then_env, &if_stmt.then_branch)?;
        then_env.check_linear_complete()?;

        let mut else_env = env.child();
        let else_ty = self.infer_tail_block_value(&mut else_env, else_branch)?;
        else_env.check_linear_complete()?;

        if !self.types_equal(&then_ty, &else_ty) {
            return Err(CompileError::new(
                format!("if expression branches must have matching types, got {:?} and {:?}", then_ty, else_ty),
                if_stmt.span,
            ));
        }

        env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_stmt.span)?;
        Ok(then_ty)
    }

    fn infer_tail_if_stmt_value_with_expected_type(
        &mut self,
        env: &mut TypeEnv,
        if_stmt: &IfStmt,
        expected_ty: &Type,
    ) -> Result<Type> {
        let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
        if !self.is_bool_type(&cond_ty) {
            return Err(CompileError::new("if condition must be boolean", if_stmt.span));
        }
        let Some(else_branch) = &if_stmt.else_branch else {
            return Ok(Type::Unit);
        };

        let mut then_env = env.child();
        let then_ty =
            self.infer_tail_block_value_with_expected_type(&mut then_env, &if_stmt.then_branch, expected_ty, if_stmt.span)?;
        then_env.check_linear_complete()?;

        let mut else_env = env.child();
        let else_ty = self.infer_tail_block_value_with_expected_type(&mut else_env, else_branch, expected_ty, if_stmt.span)?;
        else_env.check_linear_complete()?;

        if !self.initializer_types_equal(&then_ty, expected_ty) || !self.initializer_types_equal(&else_ty, expected_ty) {
            return Err(CompileError::new(
                format!("if expression branches must have matching types, got {:?} and {:?}", then_ty, else_ty),
                if_stmt.span,
            ));
        }

        env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_stmt.span)?;
        Ok(expected_ty.clone())
    }
}
