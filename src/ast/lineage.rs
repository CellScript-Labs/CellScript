//! Source-level Cell lineage and explicit consumption facts.
use super::*;
use std::collections::{HashMap, HashSet};

pub(crate) fn action_core_input_binding_names(action: &ActionDef) -> HashSet<String> {
    action_inferred_lineage_bindings(action).keys().cloned().collect()
}

pub(crate) fn action_inferred_lineage_bindings(action: &ActionDef) -> HashMap<String, String> {
    let mut bindings = HashMap::new();
    let consumed = action_consumed_bindings(action);
    for state_edge in &action.state_edges {
        if consumed.contains(&state_edge.path.base) {
            continue;
        }
        bindings.entry(state_edge.path.base.clone()).or_insert_with(|| state_edge.to_path.base.clone());
    }

    let mut outputs_by_type: HashMap<String, Vec<String>> = HashMap::new();
    for (name, type_name) in action_output_binding_types(action) {
        if bindings.values().any(|bound_output| bound_output == &name) {
            continue;
        }
        outputs_by_type.entry(type_name).or_default().push(name);
    }

    let mut inputs_by_type: HashMap<String, Vec<String>> = HashMap::new();
    for param in &action.params {
        if consumed.contains(&param.name) || bindings.contains_key(&param.name) {
            continue;
        }
        let Some(type_name) = ast_named_cell_type_name(&param.ty) else {
            continue;
        };
        if param.source != ParamSource::Default && param.source != ParamSource::Input {
            continue;
        }
        if param.is_ref || param.is_mut || param.is_read_ref {
            continue;
        }
        inputs_by_type.entry(type_name.to_string()).or_default().push(param.name.clone());
    }

    for (type_name, inputs) in inputs_by_type {
        let Some(outputs) = outputs_by_type.get(&type_name) else {
            continue;
        };
        if inputs.len() == 1 && outputs.len() == 1 {
            bindings.insert(inputs[0].clone(), outputs[0].clone());
        }
    }

    bindings
}

pub(crate) fn action_output_binding_types(action: &ActionDef) -> HashMap<String, String> {
    let mut bindings = HashMap::new();
    for output in &action.outputs {
        if let Some(type_name) = ast_named_cell_type_name(&output.ty) {
            bindings.insert(output.name.clone(), type_name.to_string());
        }
    }
    bindings
}

pub(crate) fn ast_named_cell_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Named(name) => Some(name.split('<').next().unwrap_or(name.as_str())),
        _ => None,
    }
}

pub(crate) fn action_consumed_bindings(action: &ActionDef) -> HashSet<String> {
    let mut bindings = HashSet::new();
    collect_consumed_bindings_from_stmts(&action.body, &mut bindings);
    bindings
}

fn collect_consumed_bindings_from_stmts(stmts: &[Stmt], bindings: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Let(let_stmt) => collect_consumed_bindings_from_expr(&let_stmt.value, bindings),
            Stmt::Expr(expr) | Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                collect_consumed_bindings_from_expr(expr, bindings)
            }
            Stmt::Return(ReturnStmt { value: None, .. }) => {}
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::If(if_stmt) => {
                collect_consumed_bindings_from_expr(&if_stmt.condition, bindings);
                collect_consumed_bindings_from_stmts(&if_stmt.then_branch, bindings);
                if let Some(else_branch) = &if_stmt.else_branch {
                    collect_consumed_bindings_from_stmts(else_branch, bindings);
                }
            }
            Stmt::For(for_stmt) => {
                collect_consumed_bindings_from_expr(&for_stmt.iterable, bindings);
                collect_consumed_bindings_from_stmts(&for_stmt.body, bindings);
            }
            Stmt::While(while_stmt) => {
                collect_consumed_bindings_from_expr(&while_stmt.condition, bindings);
                collect_consumed_bindings_from_stmts(&while_stmt.body, bindings);
            }
            Stmt::Borrow(borrow_stmt) => collect_consumed_bindings_from_stmts(&borrow_stmt.body, bindings),
        }
    }
}

fn collect_consumed_bindings_from_expr(expr: &Expr, bindings: &mut HashSet<String>) {
    match expr {
        Expr::ReplaceRelation(relation) => {
            bindings.insert(relation.before.clone());
        }
        Expr::Consume(consume) => {
            if let Expr::Identifier(name) = consume.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&consume.expr, bindings);
        }
        Expr::Assign(assign) => {
            collect_consumed_bindings_from_expr(&assign.target, bindings);
            collect_consumed_bindings_from_expr(&assign.value, bindings);
        }
        Expr::Binary(binary) => {
            collect_consumed_bindings_from_expr(&binary.left, bindings);
            collect_consumed_bindings_from_expr(&binary.right, bindings);
        }
        Expr::Unary(unary) => collect_consumed_bindings_from_expr(&unary.expr, bindings),
        Expr::Call(call) => {
            collect_consumed_bindings_from_expr(&call.func, bindings);
            for arg in &call.args {
                collect_consumed_bindings_from_expr(arg, bindings);
            }
        }
        Expr::FieldAccess(field) => collect_consumed_bindings_from_expr(&field.expr, bindings),
        Expr::Index(index) => {
            collect_consumed_bindings_from_expr(&index.expr, bindings);
            collect_consumed_bindings_from_expr(&index.index, bindings);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
            if let Some(lock) = &create.lock {
                collect_consumed_bindings_from_expr(lock, bindings);
            }
        }
        Expr::Destroy(destroy) => {
            if let Expr::Identifier(name) = destroy.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&destroy.expr, bindings);
        }
        Expr::Claim(claim) => {
            if let Expr::Identifier(name) = claim.receipt.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&claim.receipt, bindings);
        }
        Expr::Settle(settle) => {
            if let Expr::Identifier(name) = settle.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&settle.expr, bindings);
        }
        Expr::CreateUnique(create) => {
            for (_, value) in &create.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
            if let Some(lock) = &create.lock {
                collect_consumed_bindings_from_expr(lock, bindings);
            }
        }
        Expr::ReplaceUnique(replace) => {
            if let Expr::Identifier(name) = replace.expr.as_ref() {
                bindings.insert(name.clone());
            }
            collect_consumed_bindings_from_expr(&replace.expr, bindings);
            for (_, value) in &replace.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
        }
        Expr::ReadRef(_) => {}
        Expr::Assert(assert_expr) => {
            collect_consumed_bindings_from_expr(&assert_expr.condition, bindings);
            collect_consumed_bindings_from_expr(&assert_expr.message, bindings);
        }
        Expr::Require(require_expr) => {
            collect_consumed_bindings_from_expr(&require_expr.condition, bindings);
            if let Some(message) = &require_expr.message {
                collect_consumed_bindings_from_expr(message, bindings);
            }
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_consumed_bindings_from_expr(expr, bindings);
            }
        }
        Expr::Preserve(_) => {}
        Expr::Block(stmts) => collect_consumed_bindings_from_stmts(stmts, bindings),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_consumed_bindings_from_expr(item, bindings);
            }
        }
        Expr::If(if_expr) => {
            collect_consumed_bindings_from_expr(&if_expr.condition, bindings);
            collect_consumed_bindings_from_expr(&if_expr.then_branch, bindings);
            collect_consumed_bindings_from_expr(&if_expr.else_branch, bindings);
        }
        Expr::Cast(cast) => collect_consumed_bindings_from_expr(&cast.expr, bindings),
        Expr::Range(range) => {
            collect_consumed_bindings_from_expr(&range.start, bindings);
            collect_consumed_bindings_from_expr(&range.end, bindings);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_consumed_bindings_from_expr(value, bindings);
            }
        }
        Expr::Match(match_expr) => {
            collect_consumed_bindings_from_expr(&match_expr.expr, bindings);
            for arm in &match_expr.arms {
                collect_consumed_bindings_from_expr(&arm.value, bindings);
            }
        }
        Expr::StdlibCall(call) => {
            if let Some(name) = stdlib_lifecycle_consumed_binding(call) {
                bindings.insert(name);
            }
        }
        Expr::Identifier(_) | Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) => {}
    }
}

pub(crate) fn stdlib_lifecycle_consumed_binding(call: &StdlibCallExpr) -> Option<String> {
    let qualified = format!("std::{}::{}", call.namespace, call.name);
    match qualified.as_str() {
        "std::lifecycle::transfer" | "std::receipt::claim" | "std::lifecycle::settle" => match call.args.first() {
            Some(Expr::Identifier(name)) => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}
