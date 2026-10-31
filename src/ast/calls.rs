//! Complete source call traversal, shared by inference and lowering.
use super::*;
use std::collections::HashSet;

pub(crate) fn replace_relation_inner_exprs(relation: &ReplaceRelation) -> Vec<&Expr> {
    let mut inner = Vec::new();
    if let ReplaceLockTreatment::Exact(lock) | ReplaceLockTreatment::ExactHash(lock) = &relation.lock {
        inner.push(lock.as_ref());
    }
    match &relation.data {
        ReplaceDataTreatment::Fields(treatments) => {
            for treatment in treatments {
                if let ReplaceFieldTreatment::Assign(_, value) = treatment {
                    inner.push(value);
                }
            }
        }
        ReplaceDataTreatment::SameExcept(assigned) => {
            for (_, value) in assigned {
                inner.push(value);
            }
        }
    }
    inner
}

pub(crate) fn collect_call_names_from_stmts(stmts: &[Stmt], names: &mut HashSet<String>) {
    for stmt in stmts {
        collect_call_names_from_stmt(stmt, names);
    }
}

pub(crate) fn collect_call_names_from_stmt(stmt: &Stmt, names: &mut HashSet<String>) {
    match stmt {
        Stmt::Expr(expr) | Stmt::Let(LetStmt { value: expr, .. }) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
            collect_call_names_from_expr(expr, names);
        }
        Stmt::If(if_stmt) => {
            collect_call_names_from_expr(&if_stmt.condition, names);
            collect_call_names_from_stmts(&if_stmt.then_branch, names);
            if let Some(else_branch) = &if_stmt.else_branch {
                collect_call_names_from_stmts(else_branch, names);
            }
        }
        Stmt::For(for_stmt) => {
            collect_call_names_from_expr(&for_stmt.iterable, names);
            collect_call_names_from_stmts(&for_stmt.body, names);
        }
        Stmt::While(while_stmt) => {
            collect_call_names_from_expr(&while_stmt.condition, names);
            collect_call_names_from_stmts(&while_stmt.body, names);
        }
        Stmt::Borrow(borrow_stmt) => collect_call_names_from_stmts(&borrow_stmt.body, names),
        Stmt::Return(ReturnStmt { value: None, .. }) => {}
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

pub(crate) fn collect_call_names_from_expr(expr: &Expr, names: &mut HashSet<String>) {
    match expr {
        Expr::ReplaceRelation(relation) => {
            for value in replace_relation_inner_exprs(relation) {
                collect_call_names_from_expr(value, names);
            }
        }
        Expr::Call(call) => {
            if let Expr::Identifier(name) = call.func.as_ref() {
                names.insert(name.clone());
            }
            collect_call_names_from_expr(&call.func, names);
            for arg in &call.args {
                collect_call_names_from_expr(arg, names);
            }
        }
        Expr::Assign(assign) => {
            collect_call_names_from_expr(&assign.target, names);
            collect_call_names_from_expr(&assign.value, names);
        }
        Expr::Binary(binary) => {
            collect_call_names_from_expr(&binary.left, names);
            collect_call_names_from_expr(&binary.right, names);
        }
        Expr::Unary(unary) => collect_call_names_from_expr(&unary.expr, names),
        Expr::FieldAccess(field) => collect_call_names_from_expr(&field.expr, names),
        Expr::Index(index) => {
            collect_call_names_from_expr(&index.expr, names);
            collect_call_names_from_expr(&index.index, names);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_call_names_from_expr(value, names);
            }
            if let Some(lock) = &create.lock {
                collect_call_names_from_expr(lock, names);
            }
        }
        Expr::Consume(consume) => collect_call_names_from_expr(&consume.expr, names),
        Expr::Destroy(destroy) => collect_call_names_from_expr(&destroy.expr, names),
        Expr::Claim(claim) => collect_call_names_from_expr(&claim.receipt, names),
        Expr::Settle(settle) => collect_call_names_from_expr(&settle.expr, names),
        Expr::CreateUnique(cu) => {
            for (_, value) in &cu.fields {
                collect_call_names_from_expr(value, names);
            }
            if let Some(lock) = &cu.lock {
                collect_call_names_from_expr(lock, names);
            }
        }
        Expr::ReplaceUnique(ru) => {
            collect_call_names_from_expr(&ru.expr, names);
            for (_, value) in &ru.fields {
                collect_call_names_from_expr(value, names);
            }
        }
        Expr::Assert(assert_expr) => {
            collect_call_names_from_expr(&assert_expr.condition, names);
            collect_call_names_from_expr(&assert_expr.message, names);
        }
        Expr::Require(require_expr) => {
            collect_call_names_from_expr(&require_expr.condition, names);
            if let Some(message) = &require_expr.message {
                collect_call_names_from_expr(message, names);
            }
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_call_names_from_expr(expr, names);
            }
        }
        Expr::Preserve(_) => {}
        Expr::Block(stmts) => collect_call_names_from_stmts(stmts, names),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_call_names_from_expr(item, names);
            }
        }
        Expr::If(if_expr) => {
            collect_call_names_from_expr(&if_expr.condition, names);
            collect_call_names_from_expr(&if_expr.then_branch, names);
            collect_call_names_from_expr(&if_expr.else_branch, names);
        }
        Expr::Cast(cast) => collect_call_names_from_expr(&cast.expr, names),
        Expr::Range(range) => {
            collect_call_names_from_expr(&range.start, names);
            collect_call_names_from_expr(&range.end, names);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_call_names_from_expr(value, names);
            }
        }
        Expr::Match(match_expr) => {
            collect_call_names_from_expr(&match_expr.expr, names);
            for arm in &match_expr.arms {
                collect_call_names_from_expr(&arm.value, names);
            }
        }
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) | Expr::ReadRef(_) => {}
        Expr::StdlibCall(call) => {
            for arg in &call.args {
                collect_call_names_from_expr(arg, names);
            }
        }
    }
}
