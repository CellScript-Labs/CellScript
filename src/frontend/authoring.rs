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

fn successor_completeness_diagnostics(module: &Module) -> Vec<CompileError> {
    let resources: HashSet<String> = module
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Resource(def) => Some(def.name.clone()),
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
                    "action",
                    &action.name,
                    action.span,
                    &resources,
                    &mut errors,
                );
            }
            Item::Lock(lock) => {
                validate_entry_successors(&lock.params, &[], &lock.body, "lock", &lock.name, lock.span, &resources, &mut errors);
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
    kind: &str,
    name: &str,
    span: Span,
    resources: &HashSet<String>,
    errors: &mut Vec<CompileError>,
) {
    let roles: HashSet<String> = params
        .iter()
        .filter(|param| match (&param.ty, param.source) {
            (Type::Named(type_name), ParamSource::Input) => resources.contains(type_name),
            _ => false,
        })
        .map(|param| param.name.clone())
        .collect();

    // Unconditional declared transitions account their predecessors on every
    // path before the body begins.
    let mut must: HashSet<String> = HashSet::new();
    for role in edge_roles {
        must.insert(role.clone());
    }

    // A role only becomes *required* when this entry actually disposes of it
    // somewhere; read-only use of an input Cell stays unconstrained.
    let mut anywhere: HashSet<String> = must.clone();
    let mut analyzer = PathAnalyzer { roles: &roles, anywhere: &mut anywhere, errors, in_loop: false };
    let mut exits: Vec<HashSet<String>> = Vec::new();
    analyzer.stmts(body, &mut must, &mut exits);
    exits.push(must);

    let required: HashSet<String> = anywhere.intersection(&roles).cloned().collect();
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
    fn stmts(&mut self, stmts: &[Stmt], must: &mut HashSet<String>, exits: &mut Vec<HashSet<String>>) {
        for stmt in stmts {
            match stmt {
                Stmt::If(if_stmt) => {
                    self.expr(&if_stmt.condition, must, exits);
                    let mut then_must = must.clone();
                    self.stmts(&if_stmt.then_branch, &mut then_must, exits);
                    let mut else_must = must.clone();
                    if let Some(else_branch) = &if_stmt.else_branch {
                        self.stmts(else_branch, &mut else_must, exits);
                    }
                    *must = then_must.intersection(&else_must).cloned().collect();
                }
                Stmt::For(for_stmt) => {
                    self.expr(&for_stmt.iterable, must, exits);
                    // A disposal inside a loop cannot guarantee single-path
                    // accounting; the nested pass reports such roles directly.
                    let mut nested = PathAnalyzer { roles: self.roles, anywhere: self.anywhere, errors: self.errors, in_loop: true };
                    let mut body_must = must.clone();
                    let mut body_exits = Vec::new();
                    nested.stmts(&for_stmt.body, &mut body_must, &mut body_exits);
                }
                Stmt::While(while_stmt) => {
                    self.expr(&while_stmt.condition, must, exits);
                    let mut nested = PathAnalyzer { roles: self.roles, anywhere: self.anywhere, errors: self.errors, in_loop: true };
                    let mut body_must = must.clone();
                    let mut body_exits = Vec::new();
                    nested.stmts(&while_stmt.body, &mut body_must, &mut body_exits);
                }
                Stmt::Return(return_stmt) => {
                    if let Some(value) = &return_stmt.value {
                        self.expr(value, must, exits);
                    }
                    exits.push(must.clone());
                }
                Stmt::Let(let_stmt) => self.expr(&let_stmt.value, must, exits),
                Stmt::Borrow(borrow_stmt) => self.stmts(&borrow_stmt.body, must, exits),
                Stmt::Expr(expr) => self.expr(expr, must, exits),
                Stmt::Break(_) | Stmt::Continue(_) => {}
            }
        }
    }

    fn expr(&mut self, expr: &Expr, must: &mut HashSet<String>, exits: &mut Vec<HashSet<String>>) {
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
            Expr::ReplaceRelation(relation) => self.account(&relation.before, relation.span, must),
            Expr::Block(stmts) => self.stmts(stmts, must, exits),
            Expr::If(if_expr) => {
                self.expr(&if_expr.condition, must, exits);
                let mut then_must = must.clone();
                self.expr(&if_expr.then_branch, &mut then_must, exits);
                let mut else_must = must.clone();
                self.expr(&if_expr.else_branch, &mut else_must, exits);
                *must = then_must.intersection(&else_must).cloned().collect();
            }
            Expr::Match(match_expr) => {
                self.expr(&match_expr.expr, must, exits);
                let mut arm_results = Vec::new();
                for arm in &match_expr.arms {
                    let mut arm_must = must.clone();
                    self.expr(&arm.value, &mut arm_must, exits);
                    arm_results.push(arm_must);
                }
                if let Some(first) = arm_results.first().cloned() {
                    *must = arm_results.iter().fold(first, |acc, arm| acc.intersection(arm).cloned().collect());
                }
            }
            Expr::Binary(binary) => {
                self.expr(&binary.left, must, exits);
                self.expr(&binary.right, must, exits);
            }
            Expr::Call(call) => {
                self.expr(&call.func, must, exits);
                for arg in &call.args {
                    self.expr(arg, must, exits);
                }
            }
            _ => {}
        }
    }

    fn account(&mut self, name: &str, span: Span, must: &mut HashSet<String>) {
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
