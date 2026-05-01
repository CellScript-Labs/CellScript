use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use std::collections::{HashMap, HashSet};

pub const LIFECYCLE_STATE_FIELD_NAME: &str = "state";

#[derive(Debug, Clone)]
struct LifecycleSpec {
    states: Vec<String>,
    state_field_name: String,
    state_field_span: Option<Span>,
}

#[derive(Debug, Clone, Default)]
struct ActionLifecycleContext {
    variable_lifecycle_types: HashMap<String, String>,
    consumed_lifecycle_types: HashSet<String>,
    integer_aliases: HashMap<String, u64>,
}

/// Validate declared state-machine transitions and statically check
/// lifecycle-aware creates that can be decided from source.
pub fn check(module: &Module) -> Result<()> {
    let mut specs = HashMap::new();

    for item in &module.items {
        let Item::StateMachine(machine) = item else {
            continue;
        };
        let mut states = Vec::new();
        for transition in &machine.transitions {
            for raw in [&transition.from, &transition.to] {
                let state = raw.rsplit_once("::").map_or(raw.as_str(), |(_, state)| state).to_string();
                if !states.iter().any(|existing| existing == &state) {
                    states.push(state);
                }
            }
        }
        if states.len() < 2 {
            return Err(CompileError::new("state machine must mention at least two states", machine.span));
        }
        specs.insert(
            machine.target.base.clone(),
            LifecycleSpec { states, state_field_name: machine.target.field.clone(), state_field_span: Some(machine.target.span) },
        );
    }

    for item in &module.items {
        match item {
            Item::Action(action) => {
                let context = action_lifecycle_context(&specs, action);
                validate_stmt_list(&specs, &context, &action.body)?;
            }
            Item::Function(function) => {
                validate_stmt_list(&specs, &ActionLifecycleContext::default(), &function.body)?;
            }
            Item::Lock(lock) => validate_stmt_list(&specs, &ActionLifecycleContext::default(), &lock.body)?,
            _ => {}
        }
    }

    Ok(())
}

fn action_lifecycle_context(specs: &HashMap<String, LifecycleSpec>, action: &ActionDef) -> ActionLifecycleContext {
    let mut context = ActionLifecycleContext::default();

    for param in &action.params {
        if let Type::Named(ty) = &param.ty {
            if specs.contains_key(ty) {
                context.variable_lifecycle_types.insert(param.name.clone(), ty.clone());
            }
        }
    }

    collect_lifecycle_stmt_context(specs, &mut context, &action.body);
    context
}

fn collect_lifecycle_stmt_context(specs: &HashMap<String, LifecycleSpec>, context: &mut ActionLifecycleContext, stmts: &[Stmt]) {
    for stmt in stmts {
        match stmt {
            Stmt::Let(let_stmt) => {
                if let BindingPattern::Name(name) = &let_stmt.pattern {
                    if let Some(value) = integer_literal(&let_stmt.value) {
                        context.integer_aliases.insert(name.clone(), value);
                    }
                    if let Some(ty) = lifecycle_expr_type(specs, context, &let_stmt.value) {
                        context.variable_lifecycle_types.insert(name.clone(), ty);
                    } else if let Some(Type::Named(ty)) = &let_stmt.ty {
                        if specs.contains_key(ty) {
                            context.variable_lifecycle_types.insert(name.clone(), ty.clone());
                        }
                    }
                }
                collect_lifecycle_expr_context(specs, context, &let_stmt.value);
            }
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => collect_lifecycle_expr_context(specs, context, expr),
            Stmt::Return(None) => {}
            Stmt::If(if_stmt) => {
                collect_lifecycle_expr_context(specs, context, &if_stmt.condition);
                collect_lifecycle_stmt_context(specs, context, &if_stmt.then_branch);
                if let Some(else_branch) = &if_stmt.else_branch {
                    collect_lifecycle_stmt_context(specs, context, else_branch);
                }
            }
            Stmt::For(for_stmt) => {
                collect_lifecycle_expr_context(specs, context, &for_stmt.iterable);
                collect_lifecycle_stmt_context(specs, context, &for_stmt.body);
            }
            Stmt::While(while_stmt) => {
                collect_lifecycle_expr_context(specs, context, &while_stmt.condition);
                collect_lifecycle_stmt_context(specs, context, &while_stmt.body);
            }
        }
    }
}

fn collect_lifecycle_expr_context(specs: &HashMap<String, LifecycleSpec>, context: &mut ActionLifecycleContext, expr: &Expr) {
    match expr {
        Expr::Consume(consume) => {
            if let Expr::Identifier(name) = consume.expr.as_ref() {
                if let Some(ty) = context.variable_lifecycle_types.get(name) {
                    context.consumed_lifecycle_types.insert(ty.clone());
                }
            }
            collect_lifecycle_expr_context(specs, context, &consume.expr);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_lifecycle_expr_context(specs, context, value);
            }
            if let Some(lock) = &create.lock {
                collect_lifecycle_expr_context(specs, context, lock);
            }
        }
        Expr::Assign(assign) => {
            collect_lifecycle_expr_context(specs, context, &assign.target);
            collect_lifecycle_expr_context(specs, context, &assign.value);
        }
        Expr::Binary(bin) => {
            collect_lifecycle_expr_context(specs, context, &bin.left);
            collect_lifecycle_expr_context(specs, context, &bin.right);
        }
        Expr::Unary(unary) => collect_lifecycle_expr_context(specs, context, &unary.expr),
        Expr::Call(call) => {
            collect_lifecycle_expr_context(specs, context, &call.func);
            for arg in &call.args {
                collect_lifecycle_expr_context(specs, context, arg);
            }
        }
        Expr::FieldAccess(field) => collect_lifecycle_expr_context(specs, context, &field.expr),
        Expr::Index(index) => {
            collect_lifecycle_expr_context(specs, context, &index.expr);
            collect_lifecycle_expr_context(specs, context, &index.index);
        }
        Expr::Transfer(transfer) => {
            collect_lifecycle_expr_context(specs, context, &transfer.expr);
            collect_lifecycle_expr_context(specs, context, &transfer.to);
        }
        Expr::Destroy(destroy) => collect_lifecycle_expr_context(specs, context, &destroy.expr),
        Expr::Claim(claim) => collect_lifecycle_expr_context(specs, context, &claim.receipt),
        Expr::Settle(settle) => collect_lifecycle_expr_context(specs, context, &settle.expr),
        Expr::Assert(assert_expr) => {
            collect_lifecycle_expr_context(specs, context, &assert_expr.condition);
            collect_lifecycle_expr_context(specs, context, &assert_expr.message);
        }
        Expr::Require(require_expr) => {
            collect_lifecycle_expr_context(specs, context, &require_expr.condition);
        }
        Expr::Block(stmts) => collect_lifecycle_stmt_context(specs, context, stmts),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                collect_lifecycle_expr_context(specs, context, item);
            }
        }
        Expr::If(if_expr) => {
            collect_lifecycle_expr_context(specs, context, &if_expr.condition);
            collect_lifecycle_expr_context(specs, context, &if_expr.then_branch);
            collect_lifecycle_expr_context(specs, context, &if_expr.else_branch);
        }
        Expr::Cast(cast) => collect_lifecycle_expr_context(specs, context, &cast.expr),
        Expr::Range(range) => {
            collect_lifecycle_expr_context(specs, context, &range.start);
            collect_lifecycle_expr_context(specs, context, &range.end);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_lifecycle_expr_context(specs, context, value);
            }
        }
        Expr::Match(match_expr) => {
            collect_lifecycle_expr_context(specs, context, &match_expr.expr);
            for arm in &match_expr.arms {
                collect_lifecycle_expr_context(specs, context, &arm.value);
            }
        }
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) | Expr::ReadRef(_) => {}
    }
}

fn lifecycle_expr_type(specs: &HashMap<String, LifecycleSpec>, context: &ActionLifecycleContext, expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(name) => context.variable_lifecycle_types.get(name).cloned(),
        Expr::Create(create) if specs.contains_key(&create.ty) => Some(create.ty.clone()),
        Expr::Cast(cast) => lifecycle_expr_type(specs, context, &cast.expr),
        _ => None,
    }
}

fn validate_stmt_list(specs: &HashMap<String, LifecycleSpec>, context: &ActionLifecycleContext, stmts: &[Stmt]) -> Result<()> {
    for stmt in stmts {
        validate_lifecycle_stmt(specs, context, stmt)?;
    }
    Ok(())
}

fn validate_lifecycle_stmt(specs: &HashMap<String, LifecycleSpec>, context: &ActionLifecycleContext, stmt: &Stmt) -> Result<()> {
    match stmt {
        Stmt::Let(let_stmt) => validate_lifecycle_expr(specs, context, &let_stmt.value),
        Stmt::Expr(expr) => validate_lifecycle_expr(specs, context, expr),
        Stmt::Return(Some(expr)) => validate_lifecycle_expr(specs, context, expr),
        Stmt::Return(None) => Ok(()),
        Stmt::If(if_stmt) => {
            validate_lifecycle_expr(specs, context, &if_stmt.condition)?;
            validate_stmt_list(specs, context, &if_stmt.then_branch)?;
            if let Some(else_branch) = &if_stmt.else_branch {
                validate_stmt_list(specs, context, else_branch)?;
            }
            Ok(())
        }
        Stmt::For(for_stmt) => {
            validate_lifecycle_expr(specs, context, &for_stmt.iterable)?;
            validate_stmt_list(specs, context, &for_stmt.body)
        }
        Stmt::While(while_stmt) => {
            validate_lifecycle_expr(specs, context, &while_stmt.condition)?;
            validate_stmt_list(specs, context, &while_stmt.body)
        }
    }
}

fn validate_lifecycle_expr(specs: &HashMap<String, LifecycleSpec>, context: &ActionLifecycleContext, expr: &Expr) -> Result<()> {
    match expr {
        Expr::Create(create) => {
            validate_lifecycle_create(specs, context, create)?;
            for (_, value) in &create.fields {
                validate_lifecycle_expr(specs, context, value)?;
            }
            if let Some(lock) = &create.lock {
                validate_lifecycle_expr(specs, context, lock)?;
            }
            Ok(())
        }
        Expr::Assign(assign) => {
            validate_lifecycle_expr(specs, context, &assign.target)?;
            validate_lifecycle_expr(specs, context, &assign.value)
        }
        Expr::Binary(bin) => {
            validate_lifecycle_expr(specs, context, &bin.left)?;
            validate_lifecycle_expr(specs, context, &bin.right)
        }
        Expr::Unary(unary) => validate_lifecycle_expr(specs, context, &unary.expr),
        Expr::Call(call) => {
            validate_lifecycle_expr(specs, context, &call.func)?;
            for arg in &call.args {
                validate_lifecycle_expr(specs, context, arg)?;
            }
            Ok(())
        }
        Expr::FieldAccess(field) => validate_lifecycle_expr(specs, context, &field.expr),
        Expr::Index(index) => {
            validate_lifecycle_expr(specs, context, &index.expr)?;
            validate_lifecycle_expr(specs, context, &index.index)
        }
        Expr::Consume(consume) => validate_lifecycle_expr(specs, context, &consume.expr),
        Expr::Transfer(transfer) => {
            validate_lifecycle_expr(specs, context, &transfer.expr)?;
            validate_lifecycle_expr(specs, context, &transfer.to)
        }
        Expr::Destroy(destroy) => validate_lifecycle_expr(specs, context, &destroy.expr),
        Expr::Claim(claim) => validate_lifecycle_expr(specs, context, &claim.receipt),
        Expr::Settle(settle) => validate_lifecycle_expr(specs, context, &settle.expr),
        Expr::Assert(assert_expr) => {
            validate_lifecycle_expr(specs, context, &assert_expr.condition)?;
            validate_lifecycle_expr(specs, context, &assert_expr.message)
        }
        Expr::Require(require_expr) => validate_lifecycle_expr(specs, context, &require_expr.condition),
        Expr::Block(stmts) => validate_stmt_list(specs, context, stmts),
        Expr::Tuple(items) | Expr::Array(items) => {
            for item in items {
                validate_lifecycle_expr(specs, context, item)?;
            }
            Ok(())
        }
        Expr::If(if_expr) => {
            validate_lifecycle_expr(specs, context, &if_expr.condition)?;
            validate_lifecycle_expr(specs, context, &if_expr.then_branch)?;
            validate_lifecycle_expr(specs, context, &if_expr.else_branch)
        }
        Expr::Cast(cast) => validate_lifecycle_expr(specs, context, &cast.expr),
        Expr::Range(range) => {
            validate_lifecycle_expr(specs, context, &range.start)?;
            validate_lifecycle_expr(specs, context, &range.end)
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                validate_lifecycle_expr(specs, context, value)?;
            }
            Ok(())
        }
        Expr::Match(match_expr) => {
            validate_lifecycle_expr(specs, context, &match_expr.expr)?;
            for arm in &match_expr.arms {
                validate_lifecycle_expr(specs, context, &arm.value)?;
            }
            Ok(())
        }
        Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) | Expr::ReadRef(_) => Ok(()),
    }
}

fn validate_lifecycle_create(
    specs: &HashMap<String, LifecycleSpec>,
    context: &ActionLifecycleContext,
    create: &CreateExpr,
) -> Result<()> {
    let Some(spec) = specs.get(&create.ty) else {
        return Ok(());
    };

    if spec.state_field_span.is_none() {
        return Ok(());
    }

    let Some((_, state_expr)) = create.fields.iter().find(|(name, _)| name == &spec.state_field_name) else {
        return Err(CompileError::new(format!("create of state-machine type '{}' must set its state field", create.ty), create.span));
    };

    let updates_existing = context.consumed_lifecycle_types.contains(&create.ty);
    let Some(state_index) = static_lifecycle_state_value(state_expr, context, &create.ty, &spec.states) else {
        if !updates_existing {
            return Err(CompileError::new(
                format!("initial create of state-machine type '{}' must use a statically known declared state", create.ty),
                create.span,
            ));
        }
        return Ok(());
    };

    if state_index as usize >= spec.states.len() {
        return Err(CompileError::new(
            format!("state-machine state index {} is out of range for '{}' with {} states", state_index, create.ty, spec.states.len()),
            create.span,
        ));
    }

    Ok(())
}

fn integer_literal(expr: &Expr) -> Option<u64> {
    match expr {
        Expr::Integer(value) => Some(*value),
        Expr::Cast(cast) => integer_literal(&cast.expr),
        _ => None,
    }
}

fn static_integer_value(expr: &Expr, context: &ActionLifecycleContext) -> Option<u64> {
    match expr {
        Expr::Identifier(name) => context.integer_aliases.get(name).copied(),
        _ => integer_literal(expr),
    }
}

fn static_lifecycle_state_value(expr: &Expr, context: &ActionLifecycleContext, _type_name: &str, states: &[String]) -> Option<u64> {
    static_integer_value(expr, context).or_else(|| match expr {
        Expr::Identifier(name) => {
            let state_name = if let Some((qualified_type, state_name)) = name.rsplit_once("::") {
                let _ = qualified_type;
                state_name
            } else {
                name.as_str()
            };
            states.iter().position(|state| state == state_name).map(|index| index as u64)
        }
        _ => None,
    })
}
