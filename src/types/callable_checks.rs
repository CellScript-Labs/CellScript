use super::{push_diagnostic, CallableKind, TypeChecker};
use crate::ast::{FnDef, LockDef, Type};
use crate::error::{CompileError, Result};

impl TypeChecker<'_> {
    pub(super) fn check_function(&mut self, function: &FnDef) -> Result<()> {
        let borrow_depth = self.active_borrows.len();
        let previous_callable = self.current_callable.replace(CallableKind::Function);
        let previous_return_type = self.current_return_type.replace(function.return_type.clone());
        let result = (|| {
            let mut env = self.env.child();

            self.bind_callable_params(&mut env, &function.params, "function", &function.name)?;
            self.bind_collection_helper_borrows(&mut env, &function.params);
            if let Some(return_type) = &function.return_type {
                self.validate_callable_return_type("function", &function.name, return_type, function.span)?;
            }
            let return_env = env.clone();
            self.check_no_unreachable_stmts(&function.body)?;

            let tail = self.check_body_statements(&mut env, &function.body)?;

            if let Some(return_type) = &function.return_type {
                self.check_body_returns_or_tail_expr(
                    "function",
                    &function.name,
                    &function.body,
                    return_type,
                    function.span,
                    &return_env,
                )?;
            }

            if let Some((tail_base, stmt)) = tail {
                self.mark_stmt_as_returned(&mut env, &tail_base, stmt)?;
            }

            env.check_linear_complete()
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        self.active_borrows.truncate(borrow_depth);
        result
    }

    pub(super) fn check_function_diagnostics(&mut self, function: &FnDef) -> Vec<CompileError> {
        let borrow_depth = self.active_borrows.len();
        let previous_callable = self.current_callable.replace(CallableKind::Function);
        let previous_return_type = self.current_return_type.replace(function.return_type.clone());
        let mut diagnostics = Vec::new();
        let mut env = self.env.child();

        push_diagnostic(&mut diagnostics, self.bind_callable_params(&mut env, &function.params, "function", &function.name));
        self.bind_collection_helper_borrows(&mut env, &function.params);
        if let Some(return_type) = &function.return_type {
            push_diagnostic(
                &mut diagnostics,
                self.validate_callable_return_type("function", &function.name, return_type, function.span),
            );
        }
        let return_env = env.clone();
        push_diagnostic(&mut diagnostics, self.check_no_unreachable_stmts(&function.body));

        let body_error_start = diagnostics.len();
        let tail = self.check_body_statements_diagnostics(&mut env, &function.body, &mut diagnostics);
        let body_had_errors = diagnostics.len() > body_error_start;

        if !body_had_errors {
            if let Some(return_type) = &function.return_type {
                push_diagnostic(
                    &mut diagnostics,
                    self.check_body_returns_or_tail_expr(
                        "function",
                        &function.name,
                        &function.body,
                        return_type,
                        function.span,
                        &return_env,
                    ),
                );
            }
            if let Some((tail_base, stmt)) = tail {
                push_diagnostic(&mut diagnostics, self.mark_stmt_as_returned(&mut env, &tail_base, stmt));
            }
            push_diagnostic(&mut diagnostics, env.check_linear_complete());
        }

        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        self.active_borrows.truncate(borrow_depth);
        diagnostics
    }

    pub(super) fn check_lock(&mut self, lock: &LockDef) -> Result<()> {
        let previous_callable = self.current_callable.replace(CallableKind::Lock);
        let previous_return_type = self.current_return_type.replace(Some(Type::Bool));
        let result = (|| {
            if lock.return_type != Type::Bool {
                return Err(CompileError::new("lock definitions must return bool", lock.span));
            }

            let mut env = self.env.child();

            self.bind_callable_params(&mut env, &lock.params, "lock", &lock.name)?;
            if let Some(surface) = &lock.next_surface {
                self.check_next_audits(&env, &surface.audits)?;
            }
            self.check_no_unreachable_stmts(&lock.body)?;
            self.validate_spawn_ipc_fd_usage(&lock.body)?;

            let tail = self.check_body_statements(&mut env, &lock.body)?;

            let Some(stmt) = lock.body.last() else {
                return Err(CompileError::new("lock body must return a bool value", lock.span));
            };
            let return_ty = self.infer_lock_terminal_stmt(&mut env, stmt)?;
            if !self.is_bool_type(&return_ty) {
                return Err(CompileError::new("lock body must evaluate to bool", lock.span));
            }
            if let Some((tail_base, stmt)) = tail {
                self.mark_stmt_as_returned(&mut env, &tail_base, stmt)?;
            }

            env.check_linear_complete()
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        result
    }

    pub(super) fn check_lock_diagnostics(&mut self, lock: &LockDef) -> Vec<CompileError> {
        let previous_callable = self.current_callable.replace(CallableKind::Lock);
        let previous_return_type = self.current_return_type.replace(Some(Type::Bool));
        let mut diagnostics = Vec::new();

        if lock.return_type != Type::Bool {
            diagnostics.push(CompileError::new("lock definitions must return bool", lock.span));
        }

        let mut env = self.env.child();
        push_diagnostic(&mut diagnostics, self.bind_callable_params(&mut env, &lock.params, "lock", &lock.name));
        if let Some(surface) = &lock.next_surface {
            push_diagnostic(&mut diagnostics, self.check_next_audits(&env, &surface.audits));
        }
        push_diagnostic(&mut diagnostics, self.check_no_unreachable_stmts(&lock.body));
        push_diagnostic(&mut diagnostics, self.validate_spawn_ipc_fd_usage(&lock.body));

        let body_error_start = diagnostics.len();
        let tail = self.check_body_statements_diagnostics(&mut env, &lock.body, &mut diagnostics);
        let body_had_errors = diagnostics.len() > body_error_start;

        if !body_had_errors {
            if let Some(stmt) = lock.body.last() {
                match self.infer_lock_terminal_stmt(&mut env, stmt) {
                    Ok(return_ty) if !self.is_bool_type(&return_ty) => {
                        diagnostics.push(CompileError::new("lock body must evaluate to bool", lock.span));
                    }
                    Ok(_) => {}
                    Err(error) => diagnostics.push(error),
                }
            } else {
                diagnostics.push(CompileError::new("lock body must return a bool value", lock.span));
            }
            if let Some((tail_base, stmt)) = tail {
                push_diagnostic(&mut diagnostics, self.mark_stmt_as_returned(&mut env, &tail_base, stmt));
            }
            push_diagnostic(&mut diagnostics, env.check_linear_complete());
        }

        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        diagnostics
    }
}
