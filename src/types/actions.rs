use super::{
    action_core_evidence_binding_names, action_output_binding_names, collect_required_output_fields, native_lock_domains,
    push_diagnostic, type_repr, ActionOutputBinding, CallableKind, OutputFieldObligation, TypeChecker, TypeEnv,
};
use crate::ast::{
    expression_fails_closed, statement_fails_closed, statements_fail_closed, ActionDef, Expr, NextAudit, ParamSource,
    ReplaceDataTreatment, ReplaceFieldTreatment, ReturnStmt, Stmt, Type,
};
use crate::error::{CompileError, Result, Span};
use std::collections::{HashMap, HashSet};

impl TypeChecker<'_> {
    pub(super) fn check_action(&mut self, action: &ActionDef) -> Result<()> {
        let previous_lock_domains = std::mem::replace(&mut self.native_lock_domains, native_lock_domains(action));
        let previous_callable = self.current_callable.replace(CallableKind::Action);
        let previous_return_type = self.current_return_type.replace(action.return_type.clone());
        let result = (|| {
            let mut env = self.env.child();
            let core_evidence_bindings = action_core_evidence_binding_names(action);

            self.bind_callable_params_with_non_linear(&mut env, &action.params, "action", &action.name, &core_evidence_bindings)?;
            self.bind_action_outputs(&mut env, action)?;
            if let Some(surface) = &action.next_surface {
                self.check_next_audits(&env, &surface.audits)?;
            }
            self.validate_action_state_edges(action, &env)?;
            self.validate_action_create_targets(action)?;
            self.validate_action_branch_obligations(action)?;
            if let Some(return_type) = &action.return_type {
                self.validate_callable_return_type("action", &action.name, return_type, action.span)?;
            }
            let return_env = env.clone();
            self.check_no_unreachable_stmts(&action.body)?;
            self.validate_spawn_ipc_fd_usage(&action.body)?;

            let tail = self.check_body_statements(&mut env, &action.body)?;

            if let Some(return_type) = &action.return_type {
                self.check_body_returns_or_tail_expr("action", &action.name, &action.body, return_type, action.span, &return_env)?;
            }

            if let Some((tail_base, stmt)) = tail {
                self.mark_stmt_as_returned(&mut env, &tail_base, stmt)?;
            }

            if statements_fail_closed(&action.body) {
                Ok(())
            } else {
                env.check_linear_complete()
            }
        })();
        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        self.native_lock_domains = previous_lock_domains;
        result
    }

    pub(super) fn check_action_diagnostics(&mut self, action: &ActionDef) -> Vec<CompileError> {
        let previous_lock_domains = std::mem::replace(&mut self.native_lock_domains, native_lock_domains(action));
        let previous_callable = self.current_callable.replace(CallableKind::Action);
        let previous_return_type = self.current_return_type.replace(action.return_type.clone());
        let mut diagnostics = Vec::new();
        let mut env = self.env.child();
        let core_evidence_bindings = action_core_evidence_binding_names(action);

        push_diagnostic(
            &mut diagnostics,
            self.bind_callable_params_with_non_linear(&mut env, &action.params, "action", &action.name, &core_evidence_bindings),
        );
        push_diagnostic(&mut diagnostics, self.bind_action_outputs(&mut env, action));
        if let Some(surface) = &action.next_surface {
            push_diagnostic(&mut diagnostics, self.check_next_audits(&env, &surface.audits));
        }
        push_diagnostic(&mut diagnostics, self.validate_action_state_edges(action, &env));
        push_diagnostic(&mut diagnostics, self.validate_action_create_targets(action));
        push_diagnostic(&mut diagnostics, self.validate_action_branch_obligations(action));
        if let Some(return_type) = &action.return_type {
            push_diagnostic(&mut diagnostics, self.validate_callable_return_type("action", &action.name, return_type, action.span));
        }
        let return_env = env.clone();
        push_diagnostic(&mut diagnostics, self.check_no_unreachable_stmts(&action.body));
        push_diagnostic(&mut diagnostics, self.validate_spawn_ipc_fd_usage(&action.body));

        let body_error_start = diagnostics.len();
        let tail = self.check_body_statements_diagnostics(&mut env, &action.body, &mut diagnostics);
        let body_had_errors = diagnostics.len() > body_error_start;

        if !body_had_errors {
            if let Some(return_type) = &action.return_type {
                push_diagnostic(
                    &mut diagnostics,
                    self.check_body_returns_or_tail_expr("action", &action.name, &action.body, return_type, action.span, &return_env),
                );
            }
            if let Some((tail_base, stmt)) = tail {
                push_diagnostic(&mut diagnostics, self.mark_stmt_as_returned(&mut env, &tail_base, stmt));
            }
            if !statements_fail_closed(&action.body) {
                push_diagnostic(&mut diagnostics, env.check_linear_complete());
            }
        }

        self.current_callable = previous_callable;
        self.current_return_type = previous_return_type;
        self.native_lock_domains = previous_lock_domains;
        diagnostics
    }

    pub(super) fn bind_action_outputs(&self, env: &mut TypeEnv, action: &ActionDef) -> Result<()> {
        let mut seen = action.params.iter().map(|param| param.name.clone()).collect::<HashSet<_>>();
        for output in &action.outputs {
            if output.name == "_" {
                return Err(CompileError::new(
                    format!("action '{}' output binding must have a stable name", action.name),
                    output.span,
                ));
            }
            if !seen.insert(output.name.clone()) {
                return Err(CompileError::new(
                    format!("duplicate action binding '{}' in action '{}'", output.name, action.name),
                    output.span,
                ));
            }
            self.validate_type(&output.ty)?;
            let Some(type_name) = Self::base_type_name(&output.ty) else {
                return Err(CompileError::new(
                    format!("action output '{}' must name a Cell-backed resource, shared cell, or receipt type", output.name),
                    output.span,
                ));
            };
            if self.resolve_cell_type_kind(type_name).is_none() {
                return Err(CompileError::new(
                    format!(
                        "action output '{}' references non-Cell type {}; action outputs are proposed transaction output Cells",
                        output.name,
                        type_repr(&output.ty)
                    ),
                    output.span,
                ));
            }
            env.bind_new(output.name.clone(), output.ty.clone(), false, false, output.span)?;
        }
        Ok(())
    }

    pub(super) fn check_next_audits(&mut self, env: &TypeEnv, audits: &[NextAudit]) -> Result<()> {
        for audit in audits {
            Self::validate_require_condition_is_pure(&audit.subject, "audit evidence subject")?;
            let mut audit_env = env.clone();
            let subject_ty = self.infer_expr(&mut audit_env, &audit.subject)?;
            if self.is_linear_type(&subject_ty) {
                return Err(CompileError::new(
                    format!("audit '{}' cannot capture a Cell-backed linear value; select immutable evidence fields", audit.name),
                    audit.span,
                ));
            }
        }
        Ok(())
    }

    fn validate_action_create_targets(&self, action: &ActionDef) -> Result<()> {
        let outputs = action_output_binding_names(action);
        self.validate_create_targets_in_stmts(&action.body, &outputs)
    }

    fn validate_action_branch_obligations(&self, action: &ActionDef) -> Result<()> {
        let outputs = action_output_binding_names(action);
        if outputs.is_empty() {
            return Ok(());
        }

        self.validate_branch_obligations_in_stmts(&action.body, &outputs, HashSet::new())?;
        Ok(())
    }

    fn validate_branch_obligations_in_stmts(
        &self,
        stmts: &[Stmt],
        outputs: &HashMap<String, ActionOutputBinding>,
        mut guaranteed: HashSet<OutputFieldObligation>,
    ) -> Result<HashSet<OutputFieldObligation>> {
        for stmt in stmts {
            match stmt {
                Stmt::Lifecycle(value) => {
                    if let crate::ast::LifecycleStmt::Fresh(fresh) = value.as_ref() {
                        for expression in value.value_exprs() {
                            guaranteed = self.validate_branch_obligations_in_expr(expression, outputs, guaranteed)?;
                        }
                        if outputs.contains_key(&fresh.output) {
                            guaranteed.extend(
                                fresh
                                    .data_fields
                                    .iter()
                                    .map(|(field, _)| OutputFieldObligation { output: fresh.output.clone(), field: field.clone() }),
                            );
                        }
                    } else {
                        guaranteed = self.validate_branch_obligations_in_stmts(&value.kernel(None)?, outputs, guaranteed)?;
                    }
                }
                Stmt::Let(let_stmt) => {
                    guaranteed = self.validate_branch_obligations_in_expr(&let_stmt.value, outputs, guaranteed)?;
                }
                Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                    guaranteed = self.validate_branch_obligations_in_expr(expr, outputs, guaranteed)?;
                }
                Stmt::Return(ReturnStmt { value: None, .. }) => {}
                Stmt::Break(_) | Stmt::Continue(_) => {}
                Stmt::If(if_stmt) => {
                    guaranteed = self.validate_branch_obligations_in_expr(&if_stmt.condition, outputs, guaranteed)?;
                    let then_guaranteed =
                        self.validate_branch_obligations_in_stmts(&if_stmt.then_branch, outputs, guaranteed.clone())?;
                    let else_guaranteed = if let Some(else_branch) = &if_stmt.else_branch {
                        self.validate_branch_obligations_in_stmts(else_branch, outputs, guaranteed.clone())?
                    } else {
                        guaranteed.clone()
                    };
                    guaranteed = self.merge_accepting_field_obligations(
                        &guaranteed,
                        vec![
                            (then_guaranteed, statements_fail_closed(&if_stmt.then_branch)),
                            (else_guaranteed, if_stmt.else_branch.as_ref().is_some_and(|body| statements_fail_closed(body))),
                        ],
                        if_stmt.span,
                    )?;
                }
                Stmt::For(for_stmt) => {
                    self.validate_branch_obligations_in_expr(&for_stmt.iterable, outputs, guaranteed.clone())?;
                    self.validate_branch_obligations_in_stmts(&for_stmt.body, outputs, guaranteed.clone())?;
                }
                Stmt::While(while_stmt) => {
                    self.validate_branch_obligations_in_expr(&while_stmt.condition, outputs, guaranteed.clone())?;
                    self.validate_branch_obligations_in_stmts(&while_stmt.body, outputs, guaranteed.clone())?;
                }
                Stmt::Borrow(borrow_stmt) => {
                    guaranteed = self.validate_branch_obligations_in_stmts(&borrow_stmt.body, outputs, guaranteed)?;
                }
            }
            if statement_fails_closed(stmt) || matches!(stmt, Stmt::Return(_)) {
                break;
            }
        }

        Ok(guaranteed)
    }

    fn validate_branch_obligations_in_expr(
        &self,
        expr: &Expr,
        outputs: &HashMap<String, ActionOutputBinding>,
        mut guaranteed: HashSet<OutputFieldObligation>,
    ) -> Result<HashSet<OutputFieldObligation>> {
        match expr {
            Expr::Require(require_expr) => {
                collect_required_output_fields(&require_expr.condition, outputs, &mut guaranteed);
                self.validate_branch_obligations_in_expr(&require_expr.condition, outputs, guaranteed.clone())?;
                if let Some(message) = &require_expr.message {
                    self.validate_branch_obligations_in_expr(message, outputs, guaranteed.clone())?;
                }
                Ok(guaranteed)
            }
            Expr::If(if_expr) => {
                guaranteed = self.validate_branch_obligations_in_expr(&if_expr.condition, outputs, guaranteed)?;
                let then_guaranteed = self.validate_branch_obligations_in_expr(&if_expr.then_branch, outputs, guaranteed.clone())?;
                let else_guaranteed = self.validate_branch_obligations_in_expr(&if_expr.else_branch, outputs, guaranteed.clone())?;
                self.merge_accepting_field_obligations(
                    &guaranteed,
                    vec![
                        (then_guaranteed, expression_fails_closed(&if_expr.then_branch)),
                        (else_guaranteed, expression_fails_closed(&if_expr.else_branch)),
                    ],
                    if_expr.span,
                )
            }
            Expr::Match(match_expr) => {
                guaranteed = self.validate_branch_obligations_in_expr(&match_expr.expr, outputs, guaranteed)?;
                let mut arm_sets = Vec::with_capacity(match_expr.arms.len());
                for arm in &match_expr.arms {
                    arm_sets.push((
                        self.validate_branch_obligations_in_expr(&arm.value, outputs, guaranteed.clone())?,
                        expression_fails_closed(&arm.value),
                    ));
                }
                self.merge_accepting_field_obligations(&guaranteed, arm_sets, match_expr.span)
            }
            Expr::Block(stmts) => self.validate_branch_obligations_in_stmts(stmts, outputs, guaranteed),
            Expr::Assign(assign) => {
                guaranteed = self.validate_branch_obligations_in_expr(&assign.value, outputs, guaranteed)?;
                self.validate_branch_obligations_in_expr(&assign.target, outputs, guaranteed)
            }
            Expr::Binary(binary) => {
                guaranteed = self.validate_branch_obligations_in_expr(&binary.left, outputs, guaranteed)?;
                self.validate_branch_obligations_in_expr(&binary.right, outputs, guaranteed)
            }
            Expr::Unary(unary) => self.validate_branch_obligations_in_expr(&unary.expr, outputs, guaranteed),
            Expr::Call(call) => {
                guaranteed = self.validate_branch_obligations_in_expr(&call.func, outputs, guaranteed)?;
                for arg in &call.args {
                    guaranteed = self.validate_branch_obligations_in_expr(arg, outputs, guaranteed)?;
                }
                Ok(guaranteed)
            }
            Expr::FieldAccess(field) => self.validate_branch_obligations_in_expr(&field.expr, outputs, guaranteed),
            Expr::Index(index) => {
                guaranteed = self.validate_branch_obligations_in_expr(&index.expr, outputs, guaranteed)?;
                self.validate_branch_obligations_in_expr(&index.index, outputs, guaranteed)
            }
            Expr::Create(create) => {
                for (_, value) in &create.fields {
                    guaranteed = self.validate_branch_obligations_in_expr(value, outputs, guaranteed)?;
                }
                if let Some(lock) = &create.lock {
                    guaranteed = self.validate_branch_obligations_in_expr(lock, outputs, guaranteed)?;
                }
                if let Some(output) = &create.target
                    && outputs.contains_key(output)
                {
                    guaranteed.extend(
                        create.fields.iter().map(|(field, _)| OutputFieldObligation { output: output.clone(), field: field.clone() }),
                    );
                }
                Ok(guaranteed)
            }
            Expr::Consume(consume) => self.validate_branch_obligations_in_expr(&consume.expr, outputs, guaranteed),
            Expr::Destroy(destroy) => self.validate_branch_obligations_in_expr(&destroy.expr, outputs, guaranteed),
            Expr::Claim(claim) => self.validate_branch_obligations_in_expr(&claim.receipt, outputs, guaranteed),
            Expr::Settle(settle) => self.validate_branch_obligations_in_expr(&settle.expr, outputs, guaranteed),
            Expr::CreateUnique(create) => {
                for (_, value) in &create.fields {
                    guaranteed = self.validate_branch_obligations_in_expr(value, outputs, guaranteed)?;
                }
                if let Some(lock) = &create.lock {
                    guaranteed = self.validate_branch_obligations_in_expr(lock, outputs, guaranteed)?;
                }
                if let Some(output) = &create.target
                    && outputs.contains_key(output)
                {
                    guaranteed.extend(
                        create.fields.iter().map(|(field, _)| OutputFieldObligation { output: output.clone(), field: field.clone() }),
                    );
                }
                Ok(guaranteed)
            }
            Expr::ReplaceUnique(replace) => {
                guaranteed = self.validate_branch_obligations_in_expr(&replace.expr, outputs, guaranteed)?;
                for (_, value) in &replace.fields {
                    guaranteed = self.validate_branch_obligations_in_expr(value, outputs, guaranteed)?;
                }
                Ok(guaranteed)
            }
            Expr::Assert(assert_expr) => {
                guaranteed = self.validate_branch_obligations_in_expr(&assert_expr.condition, outputs, guaranteed)?;
                self.validate_branch_obligations_in_expr(&assert_expr.message, outputs, guaranteed)
            }
            Expr::Tuple(elems) | Expr::Array(elems) => {
                for elem in elems {
                    guaranteed = self.validate_branch_obligations_in_expr(elem, outputs, guaranteed)?;
                }
                Ok(guaranteed)
            }
            Expr::Cast(cast) => self.validate_branch_obligations_in_expr(&cast.expr, outputs, guaranteed),
            Expr::Range(range) => {
                guaranteed = self.validate_branch_obligations_in_expr(&range.start, outputs, guaranteed)?;
                self.validate_branch_obligations_in_expr(&range.end, outputs, guaranteed)
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    guaranteed = self.validate_branch_obligations_in_expr(value, outputs, guaranteed)?;
                }
                Ok(guaranteed)
            }
            Expr::StdlibCall(call) => {
                for argument in &call.args {
                    guaranteed = self.validate_branch_obligations_in_expr(argument, outputs, guaranteed)?;
                }
                if matches!((call.namespace.as_str(), call.name.as_str()), ("lifecycle", "transfer" | "settle") | ("receipt", "claim"))
                    && let Some(Expr::Identifier(output)) = call.args.get(1)
                    && outputs.contains_key(output)
                {
                    guaranteed.extend(
                        call.preserve_fields
                            .iter()
                            .map(|field| OutputFieldObligation { output: output.clone(), field: field.clone() }),
                    );
                }
                Ok(guaranteed)
            }
            Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) | Expr::ReadRef(_) => {
                Ok(guaranteed)
            }
            Expr::RequireBlock(require_block) => {
                let mut current = guaranteed;
                for expr in &require_block.expressions {
                    collect_required_output_fields(expr, outputs, &mut current);
                    current = self.validate_branch_obligations_in_expr(expr, outputs, current)?;
                }
                Ok(current)
            }
            Expr::Preserve(preserve) => {
                for field in &preserve.fields {
                    guaranteed.insert(OutputFieldObligation { output: preserve.output_name.clone(), field: field.clone() });
                }
                Ok(guaranteed)
            }
            Expr::ReplaceRelation(relation) => {
                match &relation.data {
                    ReplaceDataTreatment::Fields(treatments) => {
                        for treatment in treatments {
                            let field = match treatment {
                                ReplaceFieldTreatment::Same(field) => field,
                                ReplaceFieldTreatment::Assign(field, value) => {
                                    guaranteed = self.validate_branch_obligations_in_expr(value, outputs, guaranteed)?;
                                    field
                                }
                            };
                            guaranteed.insert(OutputFieldObligation { output: relation.after.clone(), field: field.clone() });
                        }
                    }
                    ReplaceDataTreatment::SameExcept(assignments) => {
                        for (_, value) in assignments {
                            guaranteed = self.validate_branch_obligations_in_expr(value, outputs, guaranteed)?;
                        }
                        if let Some(output) = outputs.get(&relation.after)
                            && let Some(fields) = self.resolve_named_type_fields(&output.type_name)
                        {
                            guaranteed.extend(
                                fields
                                    .keys()
                                    .map(|field| OutputFieldObligation { output: relation.after.clone(), field: field.clone() }),
                            );
                        }
                    }
                }
                if let Some(lock) = relation.lock_expr() {
                    guaranteed = self.validate_branch_obligations_in_expr(lock, outputs, guaranteed)?;
                }
                Ok(guaranteed)
            }
        }
    }

    fn merge_accepting_field_obligations(
        &self,
        base: &HashSet<OutputFieldObligation>,
        branches: Vec<(HashSet<OutputFieldObligation>, bool)>,
        span: Span,
    ) -> Result<HashSet<OutputFieldObligation>> {
        let branches = branches.into_iter().filter_map(|(fields, fails)| (!fails).then_some(fields)).collect::<Vec<_>>();
        self.reject_asymmetric_branch_constraints(base, &branches, span)?;
        let mut branches = branches.into_iter();
        let Some(first) = branches.next() else {
            return Ok(base.clone());
        };
        Ok(branches.fold(first, |fields, branch| fields.intersection(&branch).cloned().collect()))
    }

    fn reject_asymmetric_branch_constraints(
        &self,
        base: &HashSet<OutputFieldObligation>,
        branches: &[HashSet<OutputFieldObligation>],
        span: Span,
    ) -> Result<()> {
        if branches.len() < 2 {
            return Ok(());
        }

        let mut union = HashSet::new();
        for branch in branches {
            for field in branch {
                if !base.contains(field) {
                    union.insert(field.clone());
                }
            }
        }

        let mut asymmetric = union
            .into_iter()
            .filter(|field| {
                branches.iter().any(|branch| branch.contains(field)) && branches.iter().any(|branch| !branch.contains(field))
            })
            .collect::<Vec<_>>();
        asymmetric.sort();

        if let Some(field) = asymmetric.into_iter().next() {
            return Err(CompileError::new(
                format!(
                    "incomplete branch constraints: field '{}' of output '{}' is constrained in one branch but not all branches",
                    field.field, field.output
                ),
                span,
            ));
        }

        Ok(())
    }

    fn validate_create_targets_in_stmts(&self, stmts: &[Stmt], outputs: &HashMap<String, ActionOutputBinding>) -> Result<()> {
        for stmt in stmts {
            match stmt {
                Stmt::Lifecycle(value) => {
                    if let crate::ast::LifecycleStmt::Fresh(fresh) = value.as_ref() {
                        let Some(binding) = outputs.get(&fresh.output) else {
                            return Err(CompileError::new(
                                format!("fresh output '{}' must name a declared action output binding", fresh.output),
                                fresh.span,
                            ));
                        };
                        if binding.type_name != fresh.ty {
                            return Err(CompileError::new(
                                format!(
                                    "fresh output '{}' has type '{}', but its initializer constructs '{}'",
                                    fresh.output, binding.type_name, fresh.ty
                                ),
                                fresh.span,
                            ));
                        }
                        for expression in value.value_exprs() {
                            self.validate_create_targets_in_expr(expression, outputs)?;
                        }
                    } else {
                        self.validate_create_targets_in_stmts(&value.kernel(None)?, outputs)?;
                    }
                }
                Stmt::Let(let_stmt) => self.validate_create_targets_in_expr(&let_stmt.value, outputs)?,
                Stmt::Expr(expr) | Stmt::Discard(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                    self.validate_create_targets_in_expr(expr, outputs)?
                }
                Stmt::Return(ReturnStmt { value: None, .. }) => {}
                Stmt::Break(_) | Stmt::Continue(_) => {}
                Stmt::If(if_stmt) => {
                    self.validate_create_targets_in_expr(&if_stmt.condition, outputs)?;
                    self.validate_create_targets_in_stmts(&if_stmt.then_branch, outputs)?;
                    if let Some(else_branch) = &if_stmt.else_branch {
                        self.validate_create_targets_in_stmts(else_branch, outputs)?;
                    }
                }
                Stmt::For(for_stmt) => {
                    self.validate_create_targets_in_expr(&for_stmt.iterable, outputs)?;
                    self.validate_create_targets_in_stmts(&for_stmt.body, outputs)?;
                }
                Stmt::While(while_stmt) => {
                    self.validate_create_targets_in_expr(&while_stmt.condition, outputs)?;
                    self.validate_create_targets_in_stmts(&while_stmt.body, outputs)?;
                }
                Stmt::Borrow(borrow_stmt) => self.validate_create_targets_in_stmts(&borrow_stmt.body, outputs)?,
            }
        }
        Ok(())
    }

    fn validate_create_targets_in_expr(&self, expr: &Expr, outputs: &HashMap<String, ActionOutputBinding>) -> Result<()> {
        match expr {
            Expr::ReplaceRelation(relation) => {
                // Every relation binds its successor through the create
                // kernel, so the successor must be an action output binding.
                if !outputs.contains_key(&relation.after) {
                    return Err(CompileError::new(
                        format!("replace relation successor '{}' must be declared as an action output binding", relation.after),
                        relation.span,
                    ));
                }
                for value in relation.value_exprs() {
                    self.validate_create_targets_in_expr(value, outputs)?;
                }
            }
            Expr::Create(create) => {
                if let Some(target) = &create.target {
                    let Some(binding) = outputs.get(target) else {
                        return Err(CompileError::new(
                            format!("create target '{}' must be declared as an action output binding", target),
                            create.span,
                        ));
                    };
                    if binding.type_name != create.ty {
                        return Err(CompileError::new(
                            format!(
                                "create target '{}' has type '{}', but initializer constructs '{}'",
                                target, binding.type_name, create.ty
                            ),
                            create.span,
                        ));
                    }
                }
                for (_, value) in &create.fields {
                    self.validate_create_targets_in_expr(value, outputs)?;
                }
                if let Some(lock) = &create.lock {
                    self.validate_create_targets_in_expr(lock, outputs)?;
                }
            }
            Expr::Assign(assign) => {
                self.validate_create_targets_in_expr(&assign.target, outputs)?;
                self.validate_create_targets_in_expr(&assign.value, outputs)?;
            }
            Expr::Binary(binary) => {
                self.validate_create_targets_in_expr(&binary.left, outputs)?;
                self.validate_create_targets_in_expr(&binary.right, outputs)?;
            }
            Expr::Unary(unary) => self.validate_create_targets_in_expr(&unary.expr, outputs)?,
            Expr::Call(call) => {
                self.validate_create_targets_in_expr(&call.func, outputs)?;
                for arg in &call.args {
                    self.validate_create_targets_in_expr(arg, outputs)?;
                }
            }
            Expr::FieldAccess(field) => self.validate_create_targets_in_expr(&field.expr, outputs)?,
            Expr::Index(index) => {
                self.validate_create_targets_in_expr(&index.expr, outputs)?;
                self.validate_create_targets_in_expr(&index.index, outputs)?;
            }
            Expr::Consume(consume) => self.validate_create_targets_in_expr(&consume.expr, outputs)?,
            Expr::Destroy(destroy) => self.validate_create_targets_in_expr(&destroy.expr, outputs)?,
            Expr::Claim(claim) => self.validate_create_targets_in_expr(&claim.receipt, outputs)?,
            Expr::Settle(settle) => self.validate_create_targets_in_expr(&settle.expr, outputs)?,
            Expr::CreateUnique(create) => {
                if let Some(target) = &create.target {
                    let Some(binding) = outputs.get(target) else {
                        return Err(CompileError::new(
                            format!("create_unique target '{}' must be declared as an action output binding", target),
                            create.span,
                        ));
                    };
                    if binding.type_name != create.ty {
                        return Err(CompileError::new(
                            format!(
                                "create_unique target '{}' has type '{}', but initializer constructs '{}'",
                                target, binding.type_name, create.ty
                            ),
                            create.span,
                        ));
                    }
                }
                for (_, value) in &create.fields {
                    self.validate_create_targets_in_expr(value, outputs)?;
                }
                if let Some(lock) = &create.lock {
                    self.validate_create_targets_in_expr(lock, outputs)?;
                }
            }
            Expr::ReplaceUnique(replace) => {
                self.validate_create_targets_in_expr(&replace.expr, outputs)?;
                for (_, value) in &replace.fields {
                    self.validate_create_targets_in_expr(value, outputs)?;
                }
            }
            Expr::Assert(assert_expr) => {
                self.validate_create_targets_in_expr(&assert_expr.condition, outputs)?;
                self.validate_create_targets_in_expr(&assert_expr.message, outputs)?;
            }
            Expr::Require(require_expr) => {
                self.validate_create_targets_in_expr(&require_expr.condition, outputs)?;
                if let Some(message) = &require_expr.message {
                    self.validate_create_targets_in_expr(message, outputs)?;
                }
            }
            Expr::Block(stmts) => self.validate_create_targets_in_stmts(stmts, outputs)?,
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    self.validate_create_targets_in_expr(item, outputs)?;
                }
            }
            Expr::If(if_expr) => {
                self.validate_create_targets_in_expr(&if_expr.condition, outputs)?;
                self.validate_create_targets_in_expr(&if_expr.then_branch, outputs)?;
                self.validate_create_targets_in_expr(&if_expr.else_branch, outputs)?;
            }
            Expr::Cast(cast) => self.validate_create_targets_in_expr(&cast.expr, outputs)?,
            Expr::Range(range) => {
                self.validate_create_targets_in_expr(&range.start, outputs)?;
                self.validate_create_targets_in_expr(&range.end, outputs)?;
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    self.validate_create_targets_in_expr(value, outputs)?;
                }
            }
            Expr::Match(match_expr) => {
                self.validate_create_targets_in_expr(&match_expr.expr, outputs)?;
                for arm in &match_expr.arms {
                    self.validate_create_targets_in_expr(&arm.value, outputs)?;
                }
            }
            Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::ByteString(_)
            | Expr::Identifier(_)
            | Expr::ReadRef(_)
            | Expr::StdlibCall(_) => {}
            Expr::RequireBlock(require_block) => {
                for expr in &require_block.expressions {
                    self.validate_create_targets_in_expr(expr, outputs)?;
                }
            }
            Expr::Preserve(_) => {}
        }
        Ok(())
    }

    fn validate_action_state_edges(&self, action: &ActionDef, env: &TypeEnv) -> Result<()> {
        let output_bindings = action_output_binding_names(action);
        let mut lineage_inputs = HashMap::new();
        let mut lineage_outputs = HashMap::new();
        for state_edge in &action.state_edges {
            let from_path = &state_edge.path;
            let to_path = &state_edge.to_path;
            if let Some(previous_output) = lineage_inputs.insert(from_path.base.clone(), to_path.base.clone())
                && previous_output != to_path.base
            {
                return Err(CompileError::new(
                    format!(
                        "state transition binding '{}' points to both '{}' and '{}'; split/merge lineage is not supported",
                        from_path.base, previous_output, to_path.base
                    ),
                    state_edge.span,
                ));
            }
            if let Some(previous_input) = lineage_outputs.insert(to_path.base.clone(), from_path.base.clone())
                && previous_input != from_path.base
            {
                return Err(CompileError::new(
                    format!(
                        "state transition output '{}' is reached from both '{}' and '{}'; split/merge lineage is not supported",
                        to_path.base, previous_input, from_path.base
                    ),
                    state_edge.span,
                ));
            }
        }

        for state_edge in &action.state_edges {
            let path = &state_edge.path;
            let to_path = &state_edge.to_path;
            let ty = env
                .lookup(&path.base)
                .ok_or_else(|| CompileError::new(format!("unknown state transition binding '{}'", path.base), path.span))?;
            let Some(param) = action.params.iter().find(|param| param.name == path.base) else {
                return Err(CompileError::new(
                    format!("state transition binding '{}' must name an action input parameter", path.base),
                    path.span,
                ));
            };
            if !matches!(param.source, ParamSource::Default | ParamSource::Input)
                || param.is_read_ref
                || !matches!(param.ty, Type::Named(_))
            {
                return Err(CompileError::new(
                    format!(
                        "state transition binding '{}' must be an owned Cell input parameter, not a reference, witness, lock_args, protected, output, or read parameter",
                        path.base
                    ),
                    path.span,
                ));
            }
            let type_name = Self::base_type_name(ty)
                .ok_or_else(|| {
                    CompileError::new(format!("state transition binding '{}' is not a named state type", path.base), path.span)
                })?
                .to_string();
            let Some(output_binding) = output_bindings.get(&to_path.base) else {
                return Err(CompileError::new(
                    format!("state transition output binding '{}' must be a named action return", to_path.base),
                    to_path.span,
                ));
            };
            if output_binding.type_name != type_name {
                return Err(CompileError::new(
                    format!(
                        "state transition input '{}' has type '{}', but output '{}' has type '{}'",
                        path.base, type_name, to_path.base, output_binding.type_name
                    ),
                    state_edge.span,
                ));
            }
            if path.field.is_empty() && to_path.field.is_empty() && state_edge.from.is_empty() && state_edge.to.is_empty() {
                continue;
            }
            let spec = self
                .flows
                .get(&type_name)
                .ok_or_else(|| CompileError::new(format!("type '{}' has no declared flow", type_name), path.span))?;
            if spec.field_name != path.field {
                return Err(CompileError::new(
                    format!(
                        "state transition field '{}.{}' does not match declared flow field '{}.{}'",
                        path.base, path.field, type_name, spec.field_name
                    ),
                    path.span,
                ));
            }
            if spec.field_name != to_path.field {
                return Err(CompileError::new(
                    format!(
                        "state transition output field '{}.{}' does not match declared flow field '{}.{}'",
                        to_path.base, to_path.field, type_name, spec.field_name
                    ),
                    to_path.span,
                ));
            }

            let states = self
                .flow_states
                .get(&type_name)
                .ok_or_else(|| CompileError::new(format!("type '{}' has no declared flow", type_name), state_edge.span))?;
            let from = self.canonical_state_name_for_flow(
                &type_name,
                spec.field_enum_type.as_deref(),
                states,
                &state_edge.from,
                state_edge.span,
            )?;
            let to = self.canonical_state_name_for_flow(
                &type_name,
                spec.field_enum_type.as_deref(),
                states,
                &state_edge.to,
                state_edge.span,
            )?;
            if !spec.transitions.iter().any(|transition| transition.from == from && transition.to == to) {
                return Err(CompileError::new(
                    format!("state transition '{}.{} {} -> {}' is not declared", type_name, spec.field_name, from, to),
                    state_edge.span,
                ));
            }
        }

        Ok(())
    }
}
