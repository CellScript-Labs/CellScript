use super::{binding_pattern_name, direct_call_name, is_direct_call, SpawnIpcFdState, TypeChecker};
use crate::ast::{BindingPattern, Expr, LetStmt, ReplaceDataTreatment, ReplaceFieldTreatment, ReplaceLockTreatment, ReturnStmt, Stmt};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn validate_spawn_ipc_fd_usage(&self, body: &[Stmt]) -> Result<()> {
        let mut state = SpawnIpcFdState::default();
        self.validate_spawn_ipc_fd_usage_statements(body, &mut state)?;
        self.reject_unclosed_spawn_ipc_fds(&state)
    }

    fn validate_spawn_ipc_fd_usage_statements(&self, body: &[Stmt], state: &mut SpawnIpcFdState) -> Result<()> {
        for stmt in body {
            self.validate_spawn_ipc_fd_usage_stmt(stmt, state)?;
        }
        Ok(())
    }

    fn validate_spawn_ipc_fd_usage_stmt(&self, stmt: &Stmt, state: &mut SpawnIpcFdState) -> Result<()> {
        match stmt {
            Stmt::Lifecycle(value) => {
                for expression in value.value_exprs() {
                    self.validate_spawn_ipc_fd_usage_expr(expression, state)?;
                }
            }
            Stmt::Let(let_stmt) => {
                self.validate_spawn_ipc_fd_usage_expr(&let_stmt.value, state)?;
                self.bind_spawn_ipc_fd_pattern(let_stmt, state);
            }
            Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                self.validate_spawn_ipc_fd_usage_expr(expr, state)?;
            }
            Stmt::Return(ReturnStmt { value: None, .. }) => {}
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::If(if_stmt) => {
                self.validate_spawn_ipc_fd_usage_expr(&if_stmt.condition, state)?;
                let mut then_state = state.clone();
                self.validate_spawn_ipc_fd_usage_statements(&if_stmt.then_branch, &mut then_state)?;
                if let Some(else_branch) = &if_stmt.else_branch {
                    let mut else_state = state.clone();
                    self.validate_spawn_ipc_fd_usage_statements(else_branch, &mut else_state)?;
                    let closed_on_both_paths: Vec<String> = then_state
                        .closed
                        .intersection(&else_state.closed)
                        .filter(|fd_key| !state.closed.contains(*fd_key))
                        .cloned()
                        .collect();
                    state.closed.extend(closed_on_both_paths);
                }
            }
            Stmt::For(for_stmt) => {
                self.validate_spawn_ipc_fd_usage_expr(&for_stmt.iterable, state)?;
                let mut loop_state = state.clone();
                self.validate_spawn_ipc_fd_usage_statements(&for_stmt.body, &mut loop_state)?;
            }
            Stmt::While(while_stmt) => {
                self.validate_spawn_ipc_fd_usage_expr(&while_stmt.condition, state)?;
                let mut loop_state = state.clone();
                self.validate_spawn_ipc_fd_usage_statements(&while_stmt.body, &mut loop_state)?;
            }
            Stmt::Borrow(borrow_stmt) => self.validate_spawn_ipc_fd_usage_statements(&borrow_stmt.body, state)?,
        }
        Ok(())
    }

    fn validate_spawn_ipc_fd_usage_expr(&self, expr: &Expr, state: &mut SpawnIpcFdState) -> Result<()> {
        match expr {
            Expr::Call(call) => {
                for arg in &call.args {
                    self.validate_spawn_ipc_fd_usage_expr(arg, state)?;
                }
                if let Some(name) = direct_call_name(call) {
                    match name {
                        "pipe_read" => self.require_open_spawn_ipc_fd(call.args.first(), state, "pipe_read", call.span)?,
                        "pipe_write" => self.require_open_spawn_ipc_fd(call.args.first(), state, "pipe_write", call.span)?,
                        "close" => {
                            let Some(fd_key) = call.args.first().and_then(|arg| self.spawn_ipc_fd_key(arg, state)) else {
                                return Ok(());
                            };
                            if state.closed.contains(&fd_key) {
                                return Err(CompileError::new(
                                    "close uses a Spawn/IPC file descriptor after it was already closed",
                                    call.span,
                                ));
                            }
                            state.closed.insert(fd_key);
                        }
                        _ => {}
                    }
                }
            }
            Expr::Assign(assign) => {
                self.validate_spawn_ipc_fd_usage_expr(&assign.target, state)?;
                self.validate_spawn_ipc_fd_usage_expr(&assign.value, state)?;
            }
            Expr::Binary(binary) => {
                self.validate_spawn_ipc_fd_usage_expr(&binary.left, state)?;
                self.validate_spawn_ipc_fd_usage_expr(&binary.right, state)?;
            }
            Expr::Unary(unary) => self.validate_spawn_ipc_fd_usage_expr(&unary.expr, state)?,
            Expr::FieldAccess(field) => self.validate_spawn_ipc_fd_usage_expr(&field.expr, state)?,
            Expr::Index(index) => {
                self.validate_spawn_ipc_fd_usage_expr(&index.expr, state)?;
                self.validate_spawn_ipc_fd_usage_expr(&index.index, state)?;
            }
            Expr::Create(create) => {
                for (_, value) in &create.fields {
                    self.validate_spawn_ipc_fd_usage_expr(value, state)?;
                }
                if let Some(lock) = &create.lock {
                    self.validate_spawn_ipc_fd_usage_expr(lock, state)?;
                }
            }
            Expr::Consume(consume) => self.validate_spawn_ipc_fd_usage_expr(&consume.expr, state)?,
            Expr::Destroy(destroy) => self.validate_spawn_ipc_fd_usage_expr(&destroy.expr, state)?,
            Expr::Claim(claim) => self.validate_spawn_ipc_fd_usage_expr(&claim.receipt, state)?,
            Expr::Settle(settle) => self.validate_spawn_ipc_fd_usage_expr(&settle.expr, state)?,
            Expr::CreateUnique(_) | Expr::ReplaceUnique(_) => {}
            Expr::Assert(assert_expr) => {
                self.validate_spawn_ipc_fd_usage_expr(&assert_expr.condition, state)?;
                self.validate_spawn_ipc_fd_usage_expr(&assert_expr.message, state)?;
            }
            Expr::Require(require_expr) => {
                self.validate_spawn_ipc_fd_usage_expr(&require_expr.condition, state)?;
                if let Some(message) = &require_expr.message {
                    self.validate_spawn_ipc_fd_usage_expr(message, state)?;
                }
            }
            Expr::Block(stmts) => {
                let mut block_state = state.clone();
                self.validate_spawn_ipc_fd_usage_statements(stmts, &mut block_state)?;
                *state = block_state;
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.validate_spawn_ipc_fd_usage_expr(item, state)?;
                }
            }
            Expr::If(if_expr) => {
                self.validate_spawn_ipc_fd_usage_expr(&if_expr.condition, state)?;
                let mut then_state = state.clone();
                self.validate_spawn_ipc_fd_usage_expr(&if_expr.then_branch, &mut then_state)?;
                let mut else_state = state.clone();
                self.validate_spawn_ipc_fd_usage_expr(&if_expr.else_branch, &mut else_state)?;
                let closed_on_both_paths: Vec<String> = then_state
                    .closed
                    .intersection(&else_state.closed)
                    .filter(|fd_key| !state.closed.contains(*fd_key))
                    .cloned()
                    .collect();
                state.closed.extend(closed_on_both_paths);
            }
            Expr::Cast(cast) => self.validate_spawn_ipc_fd_usage_expr(&cast.expr, state)?,
            Expr::Range(range) => {
                self.validate_spawn_ipc_fd_usage_expr(&range.start, state)?;
                self.validate_spawn_ipc_fd_usage_expr(&range.end, state)?;
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    self.validate_spawn_ipc_fd_usage_expr(value, state)?;
                }
            }
            Expr::Match(match_expr) => {
                self.validate_spawn_ipc_fd_usage_expr(&match_expr.expr, state)?;
                let mut shared_closed: Option<HashSet<String>> = None;
                for arm in &match_expr.arms {
                    let mut arm_state = state.clone();
                    self.validate_spawn_ipc_fd_usage_expr(&arm.value, &mut arm_state)?;
                    shared_closed = Some(match shared_closed {
                        Some(previous) => previous.intersection(&arm_state.closed).cloned().collect(),
                        None => arm_state.closed,
                    });
                }
                if let Some(shared_closed) = shared_closed {
                    let closed_on_all_arms: Vec<String> =
                        shared_closed.into_iter().filter(|fd_key| !state.closed.contains(fd_key)).collect();
                    state.closed.extend(closed_on_all_arms);
                }
            }
            Expr::StdlibCall(call) => {
                for arg in &call.args {
                    self.validate_spawn_ipc_fd_usage_expr(arg, state)?;
                }
            }
            Expr::RequireBlock(require_block) => {
                for expr in &require_block.expressions {
                    self.validate_spawn_ipc_fd_usage_expr(expr, state)?;
                }
            }
            Expr::Preserve(_)
            | Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::ByteString(_)
            | Expr::Identifier(_)
            | Expr::ReadRef(_) => {}
            Expr::ReplaceRelation(relation) => {
                if let ReplaceLockTreatment::Exact(lock) | ReplaceLockTreatment::ExactHash(lock) = &relation.lock {
                    self.validate_spawn_ipc_fd_usage_expr(lock, state)?;
                }
                match &relation.data {
                    ReplaceDataTreatment::Fields(treatments) => {
                        for treatment in treatments {
                            if let ReplaceFieldTreatment::Assign(_, value) = treatment {
                                self.validate_spawn_ipc_fd_usage_expr(value, state)?;
                            }
                        }
                    }
                    ReplaceDataTreatment::SameExcept(assigned) => {
                        for (_, value) in assigned {
                            self.validate_spawn_ipc_fd_usage_expr(value, state)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn bind_spawn_ipc_fd_pattern(&self, let_stmt: &LetStmt, state: &mut SpawnIpcFdState) {
        if is_direct_call(&let_stmt.value, "pipe") {
            match &let_stmt.pattern {
                BindingPattern::Name(name) => {
                    state.pipe_tuples.insert(name.clone(), (format!("{}.0", name), format!("{}.1", name)));
                }
                BindingPattern::Tuple(items) if items.len() == 2 => {
                    if let Some(read_name) = binding_pattern_name(&items[0]) {
                        self.register_spawn_ipc_fd_alias(read_name, read_name.to_string(), state);
                    }
                    if let Some(write_name) = binding_pattern_name(&items[1]) {
                        self.register_spawn_ipc_fd_alias(write_name, write_name.to_string(), state);
                    }
                }
                _ => {}
            }
            return;
        }

        if is_direct_call(&let_stmt.value, "inherited_fd") {
            if let BindingPattern::Name(name) = &let_stmt.pattern {
                self.register_spawn_ipc_fd_alias(name, name.clone(), state);
            }
            return;
        }

        if let BindingPattern::Name(name) = &let_stmt.pattern
            && let Some(fd_key) = self.spawn_ipc_fd_key(&let_stmt.value, state)
        {
            self.register_spawn_ipc_fd_alias(name, fd_key, state);
        }
    }

    fn register_spawn_ipc_fd_alias(&self, name: &str, fd_key: String, state: &mut SpawnIpcFdState) {
        state.aliases.insert(name.to_string(), fd_key);
    }

    fn require_open_spawn_ipc_fd(&self, arg: Option<&Expr>, state: &SpawnIpcFdState, operation: &str, span: Span) -> Result<()> {
        let Some(fd_key) = arg.and_then(|expr| self.spawn_ipc_fd_key(expr, state)) else {
            return Ok(());
        };
        if state.closed.contains(&fd_key) {
            return Err(CompileError::new(format!("{} uses a Spawn/IPC file descriptor after close", operation), span));
        }
        Ok(())
    }

    fn spawn_ipc_fd_key(&self, expr: &Expr, state: &SpawnIpcFdState) -> Option<String> {
        match expr {
            Expr::Identifier(name) => state.aliases.get(name).cloned(),
            Expr::FieldAccess(field) => {
                let Expr::Identifier(base) = field.expr.as_ref() else {
                    return None;
                };
                let (read_fd, write_fd) = state.pipe_tuples.get(base)?;
                match field.field.as_str() {
                    "0" => Some(read_fd.clone()),
                    "1" => Some(write_fd.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn reject_unclosed_spawn_ipc_fds(&self, state: &SpawnIpcFdState) -> Result<()> {
        let mut open_fds: Vec<String> = state
            .aliases
            .values()
            .chain(state.pipe_tuples.values().flat_map(|(read_fd, write_fd)| [read_fd, write_fd]))
            .filter(|fd_key| !state.closed.contains(*fd_key))
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        open_fds.sort();
        if let Some(fd_key) = open_fds.first() {
            return Err(CompileError::new(
                format!(
                    "Spawn/IPC file descriptor '{}' is not closed before callable exit; close every pipe or inherited fd on all static paths",
                    fd_key
                ),
                Span::default(),
            ));
        }
        Ok(())
    }
}
