use super::{nested, push_diagnostic, stmt_span, ActiveBorrow, LinearState, TypeChecker, TypeEnv};
use crate::ast::{
    expression_fails_closed, statements_fail_closed, BindingPattern, BorrowStmt, Expr, IfStmt, LoopControlStmt, ReturnStmt, Stmt, Type,
};
use crate::error::{CompileError, Result};

impl TypeChecker<'_> {
    pub(super) fn check_body_statements<'body>(
        &mut self,
        env: &mut TypeEnv,
        body: &'body [Stmt],
    ) -> Result<Option<(TypeEnv, &'body Stmt)>> {
        let Some((last, prefix)) = body.split_last() else {
            return Ok(None);
        };
        for stmt in prefix {
            self.check_stmt(env, stmt)?;
        }
        let tail_base = env.clone();
        self.check_stmt(env, last)?;
        Ok(Some((tail_base, last)))
    }

    pub(super) fn check_body_statements_diagnostics<'body>(
        &mut self,
        env: &mut TypeEnv,
        body: &'body [Stmt],
        diagnostics: &mut Vec<CompileError>,
    ) -> Option<(TypeEnv, &'body Stmt)> {
        let (last, prefix) = body.split_last()?;
        for stmt in prefix {
            self.check_stmt_diagnostics(env, stmt, diagnostics);
        }
        let tail_base = env.clone();
        self.check_stmt_diagnostics(env, last, diagnostics);
        Some((tail_base, last))
    }

    fn check_stmt_diagnostics(&mut self, env: &mut TypeEnv, stmt: &Stmt, diagnostics: &mut Vec<CompileError>) {
        if let Stmt::For(stmt) = stmt {
            match self.check_collection_for(env, stmt) {
                Ok(true) => return,
                Err(error) => {
                    diagnostics.push(error);
                    return;
                }
                Ok(false) => {}
            }
        }
        match stmt {
            Stmt::Let(let_stmt) => {
                if let Err(error) = self.check_stmt(env, stmt) {
                    diagnostics.push(error);
                    // Recovery only: compilation already has a fatal diagnostic.
                    // A valid owned Vec annotation still defines a local for
                    // checking later statements; never invent linear ownership.
                    if let (Some(ty), BindingPattern::Name(name)) = (&let_stmt.ty, &let_stmt.pattern)
                        && ty.vector_element().is_some()
                        && self.validate_type(ty).is_ok()
                        && !self.is_linear_type(ty)
                        && !self.type_contains_reference(ty)
                        && env.lookup(name).is_none()
                    {
                        let _ = self.bind_pattern(env, &let_stmt.pattern, ty, let_stmt.is_mut, let_stmt.span);
                    }
                }
            }
            Stmt::If(if_stmt) => {
                let condition_ok = match self.infer_expr(env, &if_stmt.condition) {
                    Ok(cond_ty) if self.is_bool_type(&cond_ty) => true,
                    Ok(_) => {
                        diagnostics.push(CompileError::new("if condition must be boolean", if_stmt.span));
                        false
                    }
                    Err(error) => {
                        diagnostics.push(error);
                        false
                    }
                };

                let mut then_env = env.child();
                let then_error_start = diagnostics.len();
                self.check_body_statements_diagnostics(&mut then_env, &if_stmt.then_branch, diagnostics);
                let then_had_errors = diagnostics.len() > then_error_start;

                if let Some(else_branch) = &if_stmt.else_branch {
                    let mut else_env = env.child();
                    let else_error_start = diagnostics.len();
                    self.check_body_statements_diagnostics(&mut else_env, else_branch, diagnostics);
                    let else_had_errors = diagnostics.len() > else_error_start;
                    if condition_ok && !then_had_errors && !else_had_errors {
                        push_diagnostic(diagnostics, self.merge_checked_if_states(env, if_stmt, &then_env, Some(&else_env)));
                    }
                } else if condition_ok && !then_had_errors {
                    push_diagnostic(diagnostics, self.merge_checked_if_states(env, if_stmt, &then_env, None));
                }
            }
            Stmt::For(for_stmt) => {
                let iter_ty = match self.infer_expr(env, &for_stmt.iterable) {
                    Ok(iter_ty) => iter_ty,
                    Err(error) => {
                        diagnostics.push(error);
                        return;
                    }
                };
                let item_ty = match self.iter_item_type(&iter_ty, for_stmt.span) {
                    Ok(item_ty) => item_ty,
                    Err(error) => {
                        diagnostics.push(error);
                        return;
                    }
                };
                let mut loop_env = env.child();
                if let Err(error) = self.bind_pattern(&mut loop_env, &for_stmt.pattern, &item_ty, false, for_stmt.span) {
                    diagnostics.push(error);
                    return;
                }
                let body_error_start = diagnostics.len();
                self.loop_labels.push(for_stmt.label.clone());
                self.check_body_statements_diagnostics(&mut loop_env, &for_stmt.body, diagnostics);
                self.loop_labels.pop();
                if diagnostics.len() == body_error_start {
                    push_diagnostic(diagnostics, loop_env.check_linear_complete());
                    push_diagnostic(diagnostics, env.reject_loop_linear_state_changes(&loop_env, for_stmt.span));
                    env.merge_existing_type_refinements_from(&loop_env);
                }
            }
            Stmt::While(while_stmt) => {
                let condition_ok = match self.infer_expr(env, &while_stmt.condition) {
                    Ok(cond_ty) if self.is_bool_type(&cond_ty) => true,
                    Ok(_) => {
                        diagnostics.push(CompileError::new("while condition must be boolean", while_stmt.span));
                        false
                    }
                    Err(error) => {
                        diagnostics.push(error);
                        false
                    }
                };
                let mut while_env = env.child();
                let body_error_start = diagnostics.len();
                self.loop_labels.push(while_stmt.label.clone());
                self.check_body_statements_diagnostics(&mut while_env, &while_stmt.body, diagnostics);
                self.loop_labels.pop();
                if condition_ok && diagnostics.len() == body_error_start {
                    push_diagnostic(diagnostics, while_env.check_linear_complete());
                    push_diagnostic(diagnostics, env.reject_loop_linear_state_changes(&while_env, while_stmt.span));
                    env.merge_existing_type_refinements_from(&while_env);
                }
            }
            _ => push_diagnostic(diagnostics, self.check_stmt(env, stmt)),
        }
    }

    fn merge_checked_if_states(
        &self,
        env: &mut TypeEnv,
        source: &IfStmt,
        then_env: &TypeEnv,
        else_env: Option<&TypeEnv>,
    ) -> Result<()> {
        if expression_fails_closed(&source.condition) {
            return Ok(());
        }
        let then_fails = statements_fail_closed(&source.then_branch);
        let else_fails = source.else_branch.as_ref().is_some_and(|body| statements_fail_closed(body));
        match (then_fails, else_fails) {
            (true, true) => Ok(()),
            (true, false) => {
                if let Some(other) = else_env {
                    env.merge_existing_linear_states_from(other);
                }
                Ok(())
            }
            (false, true) => {
                env.merge_existing_linear_states_from(then_env);
                Ok(())
            }
            (false, false) => env.merge_branch_linear_states(
                then_env,
                self.stmts_always_return(&source.then_branch),
                else_env,
                source.else_branch.as_ref().is_some_and(|body| self.stmts_always_return(body)),
                source.span,
            ),
        }
    }

    pub(super) fn check_stmt(&mut self, env: &mut TypeEnv, stmt: &Stmt) -> Result<()> {
        if let Stmt::For(stmt) = stmt
            && self.check_collection_for(env, stmt)?
        {
            return Ok(());
        }
        match stmt {
            Stmt::Lifecycle(value) => {
                for statement in self.lifecycle_kernel(value)? {
                    self.check_stmt(env, &statement)?;
                }
                Ok(())
            }
            Stmt::Let(let_stmt) => {
                self.reject_direct_borrow_escape(&let_stmt.value, let_stmt.span)?;
                let ty = self.infer_let_value_type(env, let_stmt)?;
                if crate::cell_data_view::invocation_local(&ty)
                    && !(matches!(&ty, Type::Selection(_)) || crate::cell_data_view::ast_schema(&ty).is_some())
                {
                    return Err(CompileError::new(
                        "ExactCellSelection and CellDataView cannot be wrapped in a reference or container",
                        let_stmt.span,
                    ));
                }
                if let_stmt.is_mut && crate::cell_data_view::invocation_local(&ty) {
                    return Err(CompileError::new(
                        "ExactCellSelection and CellDataView are immutable; mutable aliases are not permitted",
                        let_stmt.span,
                    ));
                }
                if let_stmt.is_mut && nested::contains_nested_type(&ty) {
                    return Err(CompileError::new(
                        "nested values and derived views are read-only; a mutable alias is not permitted",
                        let_stmt.span,
                    ));
                }
                if let Some(ref declared_ty) = let_stmt.ty {
                    self.validate_type(declared_ty)?;
                    if !self.types_equal(&ty, declared_ty) {
                        return Err(CompileError::new(
                            format!("type mismatch: expected {:?}, found {:?}", declared_ty, ty),
                            let_stmt.span,
                        ));
                    }
                }
                self.ensure_expr_has_value(&let_stmt.value, &ty, let_stmt.span)?;
                self.reject_borrow_escape_type(&let_stmt.value, &ty, let_stmt.span)?;
                self.reject_local_reference_to_linear_root(env, &let_stmt.value, &ty, let_stmt.span)?;
                self.reject_local_mutable_reference_alias(&ty, let_stmt.span)?;
                self.mark_expr_as_moved(env, &let_stmt.value)?;
                self.bind_pattern(env, &let_stmt.pattern, &ty, let_stmt.is_mut, let_stmt.span)?;
                Ok(())
            }
            Stmt::Expr(expr) | Stmt::Discard(expr) => {
                self.infer_expr(env, expr)?;
                Ok(())
            }
            Stmt::Return(ReturnStmt { value: None, .. }) => {
                if let Some(Some(expected)) = &self.current_return_type {
                    return Err(CompileError::new(
                        format!("return without value in function returning {:?}", expected),
                        stmt_span(stmt),
                    ));
                }
                Ok(())
            }
            Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                let return_span = stmt_span(stmt);
                self.reject_direct_borrow_escape(expr, return_span)?;
                let ty = match self.current_return_type.clone() {
                    Some(Some(expected)) => self.infer_expr_with_expected_type(env, expr, &expected, return_span)?,
                    _ => self.infer_expr(env, expr)?,
                };
                match &self.current_return_type {
                    Some(Some(expected)) if !self.expr_type_compatible_with_expected(expr, &ty, expected, return_span)? => {
                        return Err(CompileError::new(
                            format!("return type mismatch: expected {:?}, found {:?}", expected, ty),
                            return_span,
                        ));
                    }
                    Some(None) => {
                        return Err(CompileError::new("return value is not allowed in a function without a return type", return_span));
                    }
                    _ => {}
                }
                self.reject_borrow_escape_type(expr, &ty, return_span)?;
                self.mark_expr_as_moved(env, expr)?;
                Ok(())
            }
            Stmt::If(if_stmt) => {
                let cond_ty = self.infer_expr(env, &if_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if condition must be boolean", if_stmt.span));
                }
                let mut then_env = env.child();
                for stmt in &if_stmt.then_branch {
                    self.check_stmt(&mut then_env, stmt)?;
                }
                if let Some(ref else_branch) = if_stmt.else_branch {
                    let mut else_env = env.child();
                    for stmt in else_branch {
                        self.check_stmt(&mut else_env, stmt)?;
                    }
                    self.merge_checked_if_states(env, if_stmt, &then_env, Some(&else_env))?;
                } else {
                    self.merge_checked_if_states(env, if_stmt, &then_env, None)?;
                }
                Ok(())
            }
            Stmt::For(for_stmt) => {
                let iter_ty = self.infer_expr(env, &for_stmt.iterable)?;
                let mut loop_env = env.child();
                let item_ty = self.iter_item_type(&iter_ty, for_stmt.span)?;
                self.bind_pattern(&mut loop_env, &for_stmt.pattern, &item_ty, false, for_stmt.span)?;
                self.loop_labels.push(for_stmt.label.clone());
                let body_result: Result<()> = (|| {
                    for stmt in &for_stmt.body {
                        self.check_stmt(&mut loop_env, stmt)?;
                    }
                    Ok(())
                })();
                self.loop_labels.pop();
                body_result?;
                loop_env.check_linear_complete()?;
                env.reject_loop_linear_state_changes(&loop_env, for_stmt.span)?;
                env.merge_existing_type_refinements_from(&loop_env);
                Ok(())
            }
            Stmt::While(while_stmt) => {
                let cond_ty = self.infer_expr(env, &while_stmt.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("while condition must be boolean", while_stmt.span));
                }
                let mut while_env = env.child();
                self.loop_labels.push(while_stmt.label.clone());
                let body_result: Result<()> = (|| {
                    for stmt in &while_stmt.body {
                        self.check_stmt(&mut while_env, stmt)?;
                    }
                    Ok(())
                })();
                self.loop_labels.pop();
                body_result?;
                while_env.check_linear_complete()?;
                env.reject_loop_linear_state_changes(&while_env, while_stmt.span)?;
                env.merge_existing_type_refinements_from(&while_env);
                Ok(())
            }
            Stmt::Break(control) | Stmt::Continue(control) => self.check_loop_control(control),
            Stmt::Borrow(borrow_stmt) => self.check_borrow_stmt(env, borrow_stmt),
        }
    }

    fn check_loop_control(&self, control: &LoopControlStmt) -> Result<()> {
        if self.loop_labels.is_empty() {
            return Err(CompileError::new("break and continue are only valid inside a loop", control.span));
        }
        if let Some(label) = &control.label
            && !self.loop_labels.iter().rev().any(|candidate| candidate.as_deref() == Some(label.as_str()))
        {
            return Err(CompileError::new(format!("unknown loop label '{label}'"), control.span));
        }
        Ok(())
    }

    fn check_borrow_stmt(&mut self, env: &mut TypeEnv, borrow_stmt: &BorrowStmt) -> Result<()> {
        let source_ty = env.lookup(&borrow_stmt.root).cloned().ok_or_else(|| {
            CompileError::new(format!("borrow root '{}' is not a local linear value", borrow_stmt.root), borrow_stmt.span)
        })?;
        let (linear_root, mut view_ty) = match source_ty {
            Type::Ref(inner) => {
                let active = self.active_borrows.iter().rev().find(|borrow| borrow.binding == borrow_stmt.root).ok_or_else(|| {
                    CompileError::new(
                        format!("reference '{}' is not an active Cell borrow and cannot be reborrowed", borrow_stmt.root),
                        borrow_stmt.span,
                    )
                })?;
                (active.root.clone(), *inner)
            }
            root_ty => {
                if !self.is_linear_type(&root_ty) {
                    return Err(CompileError::new(
                        format!("borrow root '{}' must be a cell-backed linear value or active read-only borrow", borrow_stmt.root),
                        borrow_stmt.span,
                    ));
                }
                (borrow_stmt.root.clone(), root_ty)
            }
        };
        let collection_parameter = env.lookup(&linear_root).is_some_and(|ty| self.collection_reference(ty).is_some());
        if !collection_parameter && env.linear_state(&linear_root) != Some(LinearState::Available) {
            return Err(CompileError::new(format!("borrow root '{}' is no longer available", linear_root), borrow_stmt.span));
        }
        for field in &borrow_stmt.path {
            view_ty = self.lookup_field_type(&view_ty, field, borrow_stmt.span)?;
        }

        let mut borrow_env = env.child();
        borrow_env.bind_new(borrow_stmt.binding.clone(), Type::Ref(Box::new(view_ty.clone())), false, false, borrow_stmt.span)?;
        self.active_borrows.push(ActiveBorrow { root: linear_root, binding: borrow_stmt.binding.clone(), root_ty: view_ty });
        let result = (|| {
            for stmt in &borrow_stmt.body {
                self.check_stmt(&mut borrow_env, stmt)?;
            }
            borrow_env.check_linear_complete()
        })();
        self.active_borrows.pop();
        result?;

        env.merge_existing_linear_states_from(&borrow_env);
        env.merge_existing_type_refinements_from(&borrow_env);
        Ok(())
    }

    pub(super) fn check_no_unreachable_stmts(&self, stmts: &[Stmt]) -> Result<()> {
        let mut previous_termination_message = None;
        for stmt in stmts {
            if let Some(message) = previous_termination_message {
                return Err(CompileError::new(message, stmt_span(stmt)));
            }
            self.check_no_unreachable_nested(stmt)?;
            previous_termination_message = if self.stmt_always_returns(stmt) {
                Some("unreachable statement after guaranteed return")
            } else if self.stmt_always_terminates_sequence(stmt) {
                Some("unreachable statement after terminating control flow")
            } else {
                None
            };
        }
        Ok(())
    }

    fn stmt_always_terminates_sequence(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Break(_) | Stmt::Continue(_) => true,
            Stmt::If(if_stmt) => if_stmt.else_branch.as_ref().is_some_and(|else_branch| {
                if_stmt.then_branch.iter().any(|stmt| self.stmt_always_terminates_sequence(stmt))
                    && else_branch.iter().any(|stmt| self.stmt_always_terminates_sequence(stmt))
            }),
            Stmt::Expr(Expr::Block(stmts)) | Stmt::Discard(Expr::Block(stmts)) => {
                stmts.iter().any(|stmt| self.stmt_always_terminates_sequence(stmt))
            }
            Stmt::Borrow(borrow_stmt) => borrow_stmt.body.iter().any(|stmt| self.stmt_always_terminates_sequence(stmt)),
            _ => self.stmt_always_returns(stmt),
        }
    }

    fn check_no_unreachable_nested(&self, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::If(if_stmt) => {
                self.check_no_unreachable_stmts(&if_stmt.then_branch)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.check_no_unreachable_stmts(else_branch)?;
                }
            }
            Stmt::For(for_stmt) => self.check_no_unreachable_stmts(&for_stmt.body)?,
            Stmt::While(while_stmt) => self.check_no_unreachable_stmts(&while_stmt.body)?,
            Stmt::Borrow(borrow_stmt) => self.check_no_unreachable_stmts(&borrow_stmt.body)?,
            Stmt::Expr(Expr::Block(stmts)) | Stmt::Discard(Expr::Block(stmts)) => self.check_no_unreachable_stmts(stmts)?,
            _ => {}
        }
        Ok(())
    }
}
