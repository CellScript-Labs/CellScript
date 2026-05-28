use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use std::collections::{HashMap, HashSet};

pub const FLOW_STATE_FIELD_NAME: &str = "state";

#[derive(Debug, Clone)]
struct FlowSpec {
    states: Vec<String>,
    state_field_name: String,
    state_field_span: Option<Span>,
}

#[derive(Debug, Clone, Default)]
struct ActionStateContext {
    variable_flow_types: HashMap<String, String>,
    consumed_flow_types: HashSet<String>,
    integer_aliases: HashMap<String, u64>,
}

/// Validate declared flow transitions and statically check
/// flow-aware creates that can be decided from source.
pub fn check(module: &Module) -> Result<()> {
    let mut specs = HashMap::new();

    for item in &module.items {
        let Item::Flow(machine) = item else {
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
            return Err(CompileError::new("flow must mention at least two states", machine.span));
        }
        specs.insert(
            machine.target.base.clone(),
            FlowSpec { states, state_field_name: machine.target.field.clone(), state_field_span: Some(machine.target.span) },
        );
    }

    for item in &module.items {
        match item {
            Item::Action(action) => {
                let context = action_state_context(&specs, action);
                validate_stmt_list(&specs, &context, &action.body)?;
            }
            Item::Function(function) => {
                validate_stmt_list(&specs, &ActionStateContext::default(), &function.body)?;
            }
            Item::Lock(lock) => validate_stmt_list(&specs, &ActionStateContext::default(), &lock.body)?,
            _ => {}
        }
    }

    Ok(())
}

fn action_state_context(specs: &HashMap<String, FlowSpec>, action: &ActionDef) -> ActionStateContext {
    let mut context = ActionStateContext::default();

    for param in &action.params {
        if let Type::Named(ty) = &param.ty {
            if specs.contains_key(ty) {
                context.variable_flow_types.insert(param.name.clone(), ty.clone());
            }
        }
    }

    collect_state_context_from_stmts(specs, &mut context, &action.body);
    context
}

fn collect_state_context_from_stmts(specs: &HashMap<String, FlowSpec>, context: &mut ActionStateContext, stmts: &[Stmt]) {
    for stmt in stmts {
        match stmt {
            Stmt::Let(let_stmt) => {
                if let BindingPattern::Name(name) = &let_stmt.pattern {
                    if let Some(value) = integer_literal(&let_stmt.value) {
                        context.integer_aliases.insert(name.clone(), value);
                    }
                    if let Some(ty) = flow_expr_type(specs, context, &let_stmt.value) {
                        context.variable_flow_types.insert(name.clone(), ty);
                    } else if let Some(Type::Named(ty)) = &let_stmt.ty {
                        if specs.contains_key(ty) {
                            context.variable_flow_types.insert(name.clone(), ty.clone());
                        }
                    }
                }
                collect_state_context_from_expr(specs, context, &let_stmt.value);
            }
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => collect_state_context_from_expr(specs, context, expr),
            Stmt::Return(None) => {}
            Stmt::If(if_stmt) => {
                collect_state_context_from_expr(specs, context, &if_stmt.condition);
                collect_state_context_from_stmts(specs, context, &if_stmt.then_branch);
                if let Some(else_branch) = &if_stmt.else_branch {
                    collect_state_context_from_stmts(specs, context, else_branch);
                }
            }
            Stmt::For(for_stmt) => {
                collect_state_context_from_expr(specs, context, &for_stmt.iterable);
                collect_state_context_from_stmts(specs, context, &for_stmt.body);
            }
            Stmt::While(while_stmt) => {
                collect_state_context_from_expr(specs, context, &while_stmt.condition);
                collect_state_context_from_stmts(specs, context, &while_stmt.body);
            }
        }
    }
}

fn collect_state_context_from_expr(specs: &HashMap<String, FlowSpec>, context: &mut ActionStateContext, expr: &Expr) {
    match expr {
        Expr::Consume(consume) => {
            mark_consumed_flow_expr(specs, context, &consume.expr);
            collect_state_context_from_expr(specs, context, &consume.expr);
        }
        Expr::Transfer(transfer) => {
            mark_consumed_flow_expr(specs, context, &transfer.expr);
            collect_state_context_from_expr(specs, context, &transfer.expr);
            collect_state_context_from_expr(specs, context, &transfer.to);
        }
        Expr::Create(create) => {
            for (_, value) in &create.fields {
                collect_state_context_from_expr(specs, context, value);
            }
            if let Some(lock) = &create.lock {
                collect_state_context_from_expr(specs, context, lock);
            }
        }
        Expr::CreateUnique(create) => {
            for (_, value) in &create.fields {
                collect_state_context_from_expr(specs, context, value);
            }
            if let Some(lock) = &create.lock {
                collect_state_context_from_expr(specs, context, lock);
            }
        }
        Expr::ReplaceUnique(replace) => {
            collect_state_context_from_expr(specs, context, &replace.expr);
            for (_, value) in &replace.fields {
                collect_state_context_from_expr(specs, context, value);
            }
        }
        Expr::Assign(assign) => {
            collect_state_context_from_expr(specs, context, &assign.target);
            collect_state_context_from_expr(specs, context, &assign.value);
        }
        Expr::Binary(bin) => {
            collect_state_context_from_expr(specs, context, &bin.left);
            collect_state_context_from_expr(specs, context, &bin.right);
        }
        Expr::Unary(unary) => collect_state_context_from_expr(specs, context, &unary.expr),
        Expr::Call(call) => {
            collect_state_context_from_expr(specs, context, &call.func);
            for arg in &call.args {
                collect_state_context_from_expr(specs, context, arg);
            }
        }
        Expr::FieldAccess(field) => collect_state_context_from_expr(specs, context, &field.expr),
        Expr::Index(index) => {
            collect_state_context_from_expr(specs, context, &index.expr);
            collect_state_context_from_expr(specs, context, &index.index);
        }
        Expr::Destroy(destroy) => collect_state_context_from_expr(specs, context, &destroy.expr),
        Expr::Claim(claim) => collect_state_context_from_expr(specs, context, &claim.receipt),
        Expr::Settle(settle) => collect_state_context_from_expr(specs, context, &settle.expr),
        Expr::Assert(assert_expr) => {
            collect_state_context_from_expr(specs, context, &assert_expr.condition);
            collect_state_context_from_expr(specs, context, &assert_expr.message);
        }
        Expr::Require(require_expr) => {
            collect_state_context_from_expr(specs, context, &require_expr.condition);
            if let Some(message) = &require_expr.message {
                collect_state_context_from_expr(specs, context, message);
            }
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                collect_state_context_from_expr(specs, context, expr);
            }
        }
        Expr::Preserve(_) => {}
        Expr::Block(stmts, _) => collect_state_context_from_stmts(specs, context, stmts),
        Expr::Tuple(items, _) | Expr::Array(items, _) => {
            for item in items {
                collect_state_context_from_expr(specs, context, item);
            }
        }
        Expr::If(if_expr) => {
            collect_state_context_from_expr(specs, context, &if_expr.condition);
            collect_state_context_from_expr(specs, context, &if_expr.then_branch);
            collect_state_context_from_expr(specs, context, &if_expr.else_branch);
        }
        Expr::Cast(cast) => collect_state_context_from_expr(specs, context, &cast.expr),
        Expr::Range(range) => {
            collect_state_context_from_expr(specs, context, &range.start);
            collect_state_context_from_expr(specs, context, &range.end);
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                collect_state_context_from_expr(specs, context, value);
            }
        }
        Expr::Match(match_expr) => {
            collect_state_context_from_expr(specs, context, &match_expr.expr);
            for arm in &match_expr.arms {
                collect_state_context_from_expr(specs, context, &arm.value);
            }
        }
        Expr::Integer(..)
        | Expr::Bool(..)
        | Expr::String(..)
        | Expr::ByteString(..)
        | Expr::Identifier(..)
        | Expr::ReadRef(_)
        | Expr::StdlibCall(_) => {}
    }
}

fn mark_consumed_flow_expr(specs: &HashMap<String, FlowSpec>, context: &mut ActionStateContext, expr: &Expr) {
    if let Some(ty) = flow_expr_type(specs, context, expr) {
        context.consumed_flow_types.insert(ty);
    }
}

fn flow_expr_type(specs: &HashMap<String, FlowSpec>, context: &ActionStateContext, expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(name, _) => context.variable_flow_types.get(name).cloned(),
        Expr::Create(create) if specs.contains_key(&create.ty) => Some(create.ty.clone()),
        Expr::Cast(cast) => flow_expr_type(specs, context, &cast.expr),
        _ => None,
    }
}

fn validate_stmt_list(specs: &HashMap<String, FlowSpec>, context: &ActionStateContext, stmts: &[Stmt]) -> Result<()> {
    for stmt in stmts {
        validate_state_transition_stmt(specs, context, stmt)?;
    }
    Ok(())
}

fn validate_state_transition_stmt(specs: &HashMap<String, FlowSpec>, context: &ActionStateContext, stmt: &Stmt) -> Result<()> {
    match stmt {
        Stmt::Let(let_stmt) => validate_state_transition_expr(specs, context, &let_stmt.value),
        Stmt::Expr(expr) => validate_state_transition_expr(specs, context, expr),
        Stmt::Return(Some(expr)) => validate_state_transition_expr(specs, context, expr),
        Stmt::Return(None) => Ok(()),
        Stmt::If(if_stmt) => {
            validate_state_transition_expr(specs, context, &if_stmt.condition)?;
            validate_stmt_list(specs, context, &if_stmt.then_branch)?;
            if let Some(else_branch) = &if_stmt.else_branch {
                validate_stmt_list(specs, context, else_branch)?;
            }
            Ok(())
        }
        Stmt::For(for_stmt) => {
            validate_state_transition_expr(specs, context, &for_stmt.iterable)?;
            validate_stmt_list(specs, context, &for_stmt.body)
        }
        Stmt::While(while_stmt) => {
            validate_state_transition_expr(specs, context, &while_stmt.condition)?;
            validate_stmt_list(specs, context, &while_stmt.body)
        }
    }
}

fn validate_state_transition_expr(specs: &HashMap<String, FlowSpec>, context: &ActionStateContext, expr: &Expr) -> Result<()> {
    match expr {
        Expr::Create(create) => {
            validate_state_transition_create(specs, context, create)?;
            for (_, value) in &create.fields {
                validate_state_transition_expr(specs, context, value)?;
            }
            if let Some(lock) = &create.lock {
                validate_state_transition_expr(specs, context, lock)?;
            }
            Ok(())
        }
        Expr::Assign(assign) => {
            validate_state_transition_expr(specs, context, &assign.target)?;
            validate_state_transition_expr(specs, context, &assign.value)
        }
        Expr::Binary(bin) => {
            validate_state_transition_expr(specs, context, &bin.left)?;
            validate_state_transition_expr(specs, context, &bin.right)
        }
        Expr::Unary(unary) => validate_state_transition_expr(specs, context, &unary.expr),
        Expr::Call(call) => {
            validate_state_transition_expr(specs, context, &call.func)?;
            for arg in &call.args {
                validate_state_transition_expr(specs, context, arg)?;
            }
            Ok(())
        }
        Expr::FieldAccess(field) => validate_state_transition_expr(specs, context, &field.expr),
        Expr::Index(index) => {
            validate_state_transition_expr(specs, context, &index.expr)?;
            validate_state_transition_expr(specs, context, &index.index)
        }
        Expr::Consume(consume) => validate_state_transition_expr(specs, context, &consume.expr),
        Expr::Transfer(transfer) => {
            validate_state_transition_expr(specs, context, &transfer.expr)?;
            validate_state_transition_expr(specs, context, &transfer.to)
        }
        Expr::Destroy(destroy) => validate_state_transition_expr(specs, context, &destroy.expr),
        Expr::Claim(claim) => validate_state_transition_expr(specs, context, &claim.receipt),
        Expr::Settle(settle) => validate_state_transition_expr(specs, context, &settle.expr),
        Expr::CreateUnique(create) => {
            for (_, value) in &create.fields {
                validate_state_transition_expr(specs, context, value)?;
            }
            if let Some(lock) = &create.lock {
                validate_state_transition_expr(specs, context, lock)?;
            }
            Ok(())
        }
        Expr::ReplaceUnique(replace) => {
            validate_state_transition_expr(specs, context, &replace.expr)?;
            for (_, value) in &replace.fields {
                validate_state_transition_expr(specs, context, value)?;
            }
            Ok(())
        }
        Expr::Assert(assert_expr) => {
            validate_state_transition_expr(specs, context, &assert_expr.condition)?;
            validate_state_transition_expr(specs, context, &assert_expr.message)
        }
        Expr::Require(require_expr) => {
            validate_state_transition_expr(specs, context, &require_expr.condition)?;
            if let Some(message) = &require_expr.message {
                validate_state_transition_expr(specs, context, message)?;
            }
            Ok(())
        }
        Expr::RequireBlock(require_block) => {
            for expr in &require_block.expressions {
                validate_state_transition_expr(specs, context, expr)?;
            }
            Ok(())
        }
        Expr::Preserve(_) => Ok(()),
        Expr::Block(stmts, _) => validate_stmt_list(specs, context, stmts),
        Expr::Tuple(items, _) | Expr::Array(items, _) => {
            for item in items {
                validate_state_transition_expr(specs, context, item)?;
            }
            Ok(())
        }
        Expr::If(if_expr) => {
            validate_state_transition_expr(specs, context, &if_expr.condition)?;
            validate_state_transition_expr(specs, context, &if_expr.then_branch)?;
            validate_state_transition_expr(specs, context, &if_expr.else_branch)
        }
        Expr::Cast(cast) => validate_state_transition_expr(specs, context, &cast.expr),
        Expr::Range(range) => {
            validate_state_transition_expr(specs, context, &range.start)?;
            validate_state_transition_expr(specs, context, &range.end)
        }
        Expr::StructInit(init) => {
            for (_, value) in &init.fields {
                validate_state_transition_expr(specs, context, value)?;
            }
            Ok(())
        }
        Expr::Match(match_expr) => {
            validate_state_transition_expr(specs, context, &match_expr.expr)?;
            for arm in &match_expr.arms {
                validate_state_transition_expr(specs, context, &arm.value)?;
            }
            Ok(())
        }
        Expr::Integer(..)
        | Expr::Bool(..)
        | Expr::String(..)
        | Expr::ByteString(..)
        | Expr::Identifier(..)
        | Expr::ReadRef(_)
        | Expr::StdlibCall(_) => Ok(()),
    }
}

fn validate_state_transition_create(
    specs: &HashMap<String, FlowSpec>,
    context: &ActionStateContext,
    create: &CreateExpr,
) -> Result<()> {
    let Some(spec) = specs.get(&create.ty) else {
        return Ok(());
    };

    if spec.state_field_span.is_none() {
        return Ok(());
    }

    let Some((_, state_expr)) = create.fields.iter().find(|(name, _)| name == &spec.state_field_name) else {
        return Err(CompileError::new(format!("create of flow type '{}' must set its state field", create.ty), create.span));
    };

    let updates_existing = context.consumed_flow_types.contains(&create.ty);
    let Some(state_index) = static_flow_state_value(state_expr, context, &create.ty, &spec.states) else {
        if !updates_existing {
            return Err(CompileError::new(
                format!("initial create of flow type '{}' must use a statically known declared state", create.ty),
                create.span,
            ));
        }
        return Ok(());
    };

    if state_index as usize >= spec.states.len() {
        return Err(CompileError::new(
            format!("flow state index {} is out of range for '{}' with {} states", state_index, create.ty, spec.states.len()),
            create.span,
        ));
    }

    Ok(())
}

fn integer_literal(expr: &Expr) -> Option<u64> {
    match expr {
        Expr::Integer(value, _) => Some(*value),
        Expr::Cast(cast) => integer_literal(&cast.expr),
        _ => None,
    }
}

fn static_integer_value(expr: &Expr, context: &ActionStateContext) -> Option<u64> {
    match expr {
        Expr::Identifier(name, _) => context.integer_aliases.get(name).copied(),
        _ => integer_literal(expr),
    }
}

fn static_flow_state_value(expr: &Expr, context: &ActionStateContext, _type_name: &str, states: &[String]) -> Option<u64> {
    static_integer_value(expr, context).or_else(|| match expr {
        Expr::Identifier(name, _) => {
            let state_name = if let Some((_qualified_type, state_name)) = name.rsplit_once("::") {
                // The qualified prefix is the enum namespace (e.g. OfferState in
                // OfferState::Filled), not the flow type name (e.g. Offer).
                // The type checker already validates that the enum type matches
                // the state field's declared type, so stripping the prefix here
                // is safe: a mismatched qualifier would have been rejected earlier.
                state_name
            } else {
                name.as_str()
            };
            states.iter().position(|state| state == state_name).map(|index| index as u64)
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_flow_tracking_follows_expression_aliases() {
        let mut specs = HashMap::new();
        specs.insert(
            "Offer".to_string(),
            FlowSpec {
                states: vec!["Open".to_string(), "Filled".to_string()],
                state_field_name: FLOW_STATE_FIELD_NAME.to_string(),
                state_field_span: Some(Span::default()),
            },
        );
        let mut context = ActionStateContext::default();
        context.variable_flow_types.insert("offer".to_string(), "Offer".to_string());
        let expr = Expr::Cast(CastExpr {
            expr: Box::new(Expr::Identifier("offer".to_string(), Span::default())),
            ty: Type::Named("Offer".to_string()),
            span: Span::default(),
        });

        mark_consumed_flow_expr(&specs, &mut context, &expr);

        assert!(context.consumed_flow_types.contains("Offer"));
    }
}
