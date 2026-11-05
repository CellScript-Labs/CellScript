//! Familiar authoring grammar over the shared value/statement kernel.
//!
//! This is an independent frontend policy, not a translation into preview4
//! text. Existing declarations and expressions keep their checked meaning.
//! In particular, a legacy terminal consume is not reclassified as retirement,
//! and a parameter's source is resolved by the existing typed entry contract.
//! Multiple source entries do not imply runtime dispatch: artifact selection
//! remains a separate, explicitly verified boundary.
//!
//! The authoring route additionally enforces path-sensitive successor
//! completeness: once a Cell role is disposed anywhere in an entry (through a
//! `replace` relation, a consume, a destroy or a declared transition edge),
//! every accepting path must dispose of it exactly once. A branch that skips
//! the disposal, a path that disposes twice, or a disposal hidden inside a
//! loop is rejected at the source level instead of surfacing as a weaker
//! runtime obligation union.

use crate::ast::{Expr, Item, Module, Param, ParamSource, Stmt, Type};
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::Token;
use crate::parser;
use std::collections::HashSet;

pub(super) fn parse(tokens: &[Token]) -> Result<Module> {
    let module = parser::parse(tokens)?;
    let diagnostics = successor_completeness_diagnostics(&module);
    diagnostics.into_iter().next().map_or(Ok(module), Err)
}

pub(super) fn parse_diagnostics(tokens: &[Token]) -> std::result::Result<Module, Vec<CompileError>> {
    let module = parser::parse_diagnostics(tokens)?;
    let diagnostics = successor_completeness_diagnostics(&module);
    if diagnostics.is_empty() {
        Ok(module)
    } else {
        Err(diagnostics)
    }
}

pub(super) fn successor_completeness_diagnostics(module: &Module) -> Vec<CompileError> {
    let resources: HashSet<String> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Resource(def) => Some(def.name.clone()),
            Item::Shared(def) => Some(def.name.clone()),
            Item::Receipt(def) => Some(def.name.clone()),
            _ => None,
        })
        .collect();
    let mut errors = Vec::new();
    for item in &module.items {
        match item {
            Item::Action(action) => {
                let edge_roles: Vec<String> = action.state_edges.iter().map(|edge| edge.from.clone()).collect();
                validate_entry_successors(
                    &action.params,
                    &edge_roles,
                    &action.body,
                    action.next_surface.as_ref().map(|_| action.outputs.as_slice()),
                    "action",
                    &action.name,
                    action.span,
                    &resources,
                    &mut errors,
                );
            }
            Item::Lock(lock) => {
                validate_entry_successors(&lock.params, &[], &lock.body, None, "lock", &lock.name, lock.span, &resources, &mut errors);
            }
            _ => {}
        }
    }
    errors
}

#[allow(clippy::too_many_arguments)]
fn validate_entry_successors(
    params: &[Param],
    edge_roles: &[String],
    body: &[Stmt],
    group_outputs: Option<&[crate::ast::ActionOutput]>,
    kind: &str,
    name: &str,
    span: Span,
    resources: &HashSet<String>,
    errors: &mut Vec<CompileError>,
) {
    let mut roles: HashSet<String> = params
        .iter()
        .filter(|param| match (&param.ty, param.source) {
            (Type::Named(type_name), ParamSource::Input) => resources.contains(type_name),
            _ => false,
        })
        .map(|param| param.name.clone())
        .collect();
    if let Some(outputs) = group_outputs {
        roles.extend(outputs.iter().map(|output| output.name.clone()));
    }

    // Unconditional declared transitions account their predecessors on every
    // path before the body begins.
    let mut initial: HashSet<String> = HashSet::new();
    for role in edge_roles {
        initial.insert(role.clone());
    }

    // A role only becomes *required* when this entry actually disposes of it
    // somewhere; read-only use of an input Cell stays unconstrained.
    let mut anywhere: HashSet<String> = initial.clone();
    let mut must = Some(initial);
    let mut analyzer = PathAnalyzer { roles: &roles, anywhere: &mut anywhere, errors, in_loop: false };
    let mut exits: Vec<HashSet<String>> = Vec::new();
    analyzer.stmts(body, &mut must, &mut exits);
    if let Some(must) = must {
        exits.push(must);
    }
    if exits.is_empty() {
        return;
    }

    let required: HashSet<String> =
        if group_outputs.is_some() { roles.clone() } else { anywhere.intersection(&roles).cloned().collect() };
    if required.is_empty() {
        return;
    }
    let mut accepted = exits.pop().unwrap_or_default();
    for exit in &exits {
        accepted = accepted.intersection(exit).cloned().collect();
    }
    let missing: Vec<String> = {
        let mut missing = required.difference(&accepted).cloned().collect::<Vec<_>>();
        missing.sort();
        missing
    };
    if !missing.is_empty() {
        errors.push(CompileError::new(
            format!(
                "{kind} '{name}' disposes of {} conditionally; every accepting path must dispose of each such role exactly once",
                missing.join(", ")
            ),
            span,
        ));
    }
}

struct PathAnalyzer<'a> {
    roles: &'a HashSet<String>,
    anywhere: &'a mut HashSet<String>,
    errors: &'a mut Vec<CompileError>,
    in_loop: bool,
}

impl<'a> PathAnalyzer<'a> {
    fn stmts(&mut self, stmts: &[Stmt], must: &mut Option<HashSet<String>>, exits: &mut Vec<HashSet<String>>) {
        for stmt in stmts {
            if must.is_none() {
                break;
            }
            match stmt {
                Stmt::If(if_stmt) => {
                    self.expr(&if_stmt.condition, must, exits);
                    let mut then_must = must.clone();
                    self.stmts(&if_stmt.then_branch, &mut then_must, exits);
                    let mut else_must = must.clone();
                    if let Some(else_branch) = &if_stmt.else_branch {
                        self.stmts(else_branch, &mut else_must, exits);
                    }
                    *must = join_accepting_paths(then_must, else_must);
                }
                Stmt::For(for_stmt) => {
                    self.expr(&for_stmt.iterable, must, exits);
                    let mut nested = PathAnalyzer { roles: self.roles, anywhere: self.anywhere, errors: self.errors, in_loop: true };
                    let mut body_must = must.clone();
                    nested.stmts(&for_stmt.body, &mut body_must, &mut Vec::new());
                }
                Stmt::While(while_stmt) => {
                    // The condition is evaluated again on every back edge;
                    // it has the same single-disposal restriction as the body.
                    let mut nested = PathAnalyzer { roles: self.roles, anywhere: self.anywhere, errors: self.errors, in_loop: true };
                    nested.expr(&while_stmt.condition, must, exits);
                    let mut body_must = must.clone();
                    nested.stmts(&while_stmt.body, &mut body_must, &mut Vec::new());
                }
                Stmt::Return(return_stmt) => {
                    if let Some(value) = &return_stmt.value {
                        self.expr(value, must, exits);
                    }
                    if let Some(must) = must.take() {
                        exits.push(must);
                    }
                }
                Stmt::Let(let_stmt) => self.expr(&let_stmt.value, must, exits),
                Stmt::Borrow(borrow_stmt) => self.stmts(&borrow_stmt.body, must, exits),
                Stmt::Expr(expr) => self.expr(expr, must, exits),
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn expr(&mut self, expr: &Expr, must: &mut Option<HashSet<String>>, exits: &mut Vec<HashSet<String>>) {
        if must.is_none() {
            return;
        }
        match expr {
            Expr::Consume(consume) => {
                if let Expr::Identifier(name) = consume.expr.as_ref() {
                    self.account(name, consume.span, must);
                } else {
                    self.expr(&consume.expr, must, exits);
                }
            }
            Expr::Destroy(destroy) => {
                if let Expr::Identifier(name) = destroy.expr.as_ref() {
                    self.account(name, destroy.span, must);
                } else {
                    self.expr(&destroy.expr, must, exits);
                }
            }
            Expr::ReplaceRelation(relation) => {
                self.account(&relation.before, relation.span, must);
                match &relation.data {
                    crate::ast::ReplaceDataTreatment::Fields(fields) => {
                        for field in fields {
                            if let crate::ast::ReplaceFieldTreatment::Assign(_, value) = field {
                                self.expr(value, must, exits);
                            }
                        }
                    }
                    crate::ast::ReplaceDataTreatment::SameExcept(fields) => {
                        for (_, value) in fields {
                            self.expr(value, must, exits);
                        }
                    }
                }
                if let Some(lock) = relation.lock_expr() {
                    self.expr(lock, must, exits);
                }
                if self.roles.contains(&relation.after) {
                    self.account(&relation.after, relation.span, must);
                }
            }
            Expr::Create(create) => {
                for (_, value) in &create.fields {
                    self.expr(value, must, exits);
                }
                if let Some(lock) = &create.lock {
                    self.expr(lock, must, exits);
                }
                if let Some(target) = &create.target
                    && self.roles.contains(target)
                {
                    self.account(target, create.span, must);
                }
            }
            Expr::CreateUnique(create) => {
                for (_, value) in &create.fields {
                    self.expr(value, must, exits);
                }
                if let Some(lock) = &create.lock {
                    self.expr(lock, must, exits);
                }
                if let Some(target) = &create.target
                    && self.roles.contains(target)
                {
                    self.account(target, create.span, must);
                }
            }
            Expr::Block(stmts) => self.stmts(stmts, must, exits),
            Expr::If(if_expr) => {
                self.expr(&if_expr.condition, must, exits);
                let mut then_must = must.clone();
                self.expr(&if_expr.then_branch, &mut then_must, exits);
                let mut else_must = must.clone();
                self.expr(&if_expr.else_branch, &mut else_must, exits);
                *must = join_accepting_paths(then_must, else_must);
            }
            Expr::Match(match_expr) => {
                self.expr(&match_expr.expr, must, exits);
                let mut joined = None;
                for arm in &match_expr.arms {
                    let mut arm_must = must.clone();
                    self.expr(&arm.value, &mut arm_must, exits);
                    joined = join_accepting_paths(joined, arm_must);
                }
                *must = joined;
            }
            Expr::Assign(assign) => {
                // Assignment lowering evaluates the RHS before projecting its
                // target; both may contain checked block expressions.
                self.expr(&assign.value, must, exits);
                self.expr(&assign.target, must, exits);
            }
            Expr::Binary(binary) => {
                // The checked source kernel currently evaluates both boolean
                // operands as well as arithmetic operands; do not invent a
                // short-circuit source contract here.
                self.expr(&binary.left, must, exits);
                self.expr(&binary.right, must, exits);
            }
            Expr::Unary(unary) => self.expr(&unary.expr, must, exits),
            Expr::Call(call) => {
                self.expr(&call.func, must, exits);
                for arg in &call.args {
                    self.expr(arg, must, exits);
                }
            }
            Expr::FieldAccess(field) => self.expr(&field.expr, must, exits),
            Expr::Index(index) => {
                self.expr(&index.expr, must, exits);
                self.expr(&index.index, must, exits);
            }
            Expr::Claim(claim) => {
                self.expr(&claim.receipt, must, exits);
                self.account_operand(&claim.receipt, claim.span, must);
            }
            Expr::Settle(settle) => {
                self.expr(&settle.expr, must, exits);
                self.account_operand(&settle.expr, settle.span, must);
            }
            Expr::ReplaceUnique(replace) => {
                self.expr(&replace.expr, must, exits);
                self.account_operand(&replace.expr, replace.span, must);
                for (_, value) in &replace.fields {
                    self.expr(value, must, exits);
                }
            }
            Expr::Assert(assertion) => {
                self.expr(&assertion.condition, must, exits);
                self.expr(&assertion.message, must, exits);
                if matches!(assertion.condition.as_ref(), Expr::Bool(false)) {
                    *must = None;
                }
            }
            Expr::Require(requirement) => {
                self.expr(&requirement.condition, must, exits);
                if let Some(message) = &requirement.message {
                    self.expr(message, must, exits);
                }
                if matches!(requirement.condition.as_ref(), Expr::Bool(false)) {
                    *must = None;
                }
            }
            Expr::RequireBlock(requirements) => {
                for condition in &requirements.expressions {
                    self.expr(condition, must, exits);
                    if matches!(condition, Expr::Bool(false)) {
                        *must = None;
                    }
                }
            }
            Expr::Tuple(values) | Expr::Array(values) => {
                for value in values {
                    self.expr(value, must, exits);
                }
            }
            Expr::Cast(cast) => self.expr(&cast.expr, must, exits),
            Expr::Range(range) => {
                self.expr(&range.start, must, exits);
                self.expr(&range.end, must, exits);
            }
            Expr::StructInit(value) => {
                for (_, value) in &value.fields {
                    self.expr(value, must, exits);
                }
            }
            Expr::StdlibCall(call) => {
                for argument in &call.args {
                    self.expr(argument, must, exits);
                }
                match (call.namespace.as_str(), call.name.as_str()) {
                    ("lifecycle", "transfer" | "settle") | ("receipt", "claim") => {
                        if let Some(input) = call.args.first() {
                            self.account_operand(input, call.span, must);
                        }
                        if let Some(Expr::Identifier(output)) = call.args.get(1)
                            && self.roles.contains(output)
                        {
                            self.account(output, call.span, must);
                        }
                    }
                    _ => {}
                }
            }
            Expr::Integer(_)
            | Expr::Bool(_)
            | Expr::String(_)
            | Expr::ByteString(_)
            | Expr::Identifier(_)
            | Expr::ReadRef(_)
            | Expr::Preserve(_) => {}
        }
    }

    fn account_operand(&mut self, operand: &Expr, span: Span, must: &mut Option<HashSet<String>>) {
        if let Expr::Identifier(name) = operand {
            self.account(name, span, must);
        }
    }

    fn account(&mut self, name: &str, span: Span, must: &mut Option<HashSet<String>>) {
        let Some(must) = must else {
            return;
        };
        self.anywhere.insert(name.to_string());
        if self.in_loop && self.roles.contains(name) {
            self.errors.push(CompileError::new(
                format!("Cell role '{name}' is disposed of inside a loop; successor accounting cannot be path-checked there"),
                span,
            ));
            return;
        }
        if self.roles.contains(name) && must.contains(name) {
            self.errors.push(CompileError::new(format!("Cell role '{name}' is disposed of twice on one accepting path"), span));
            return;
        }
        must.insert(name.to_string());
    }
}

fn join_accepting_paths(left: Option<HashSet<String>>, right: Option<HashSet<String>>) -> Option<HashSet<String>> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.intersection(&right).cloned().collect()),
        (Some(path), None) | (None, Some(path)) => Some(path),
        (None, None) => None,
    }
}
