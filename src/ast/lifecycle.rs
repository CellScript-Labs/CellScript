//! Single-owner source lifecycle statements and their transient kernel elaboration.
//!
//! Source records remain in the shared callable body. Type checking and IR
//! lowering request an elaboration at that occurrence; no second mutable tree
//! is retained in an action's surface index.

use super::*;
use crate::error::{CompileError, Result, SourceDiagnosticCode};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub enum LifecycleStmt {
    Pool(NextPool),
    Retire(NextRetirement),
    Fresh(NextFreshOutput),
}

impl LifecycleStmt {
    pub fn span(&self) -> Span {
        match self {
            Self::Pool(value) => value.span,
            Self::Retire(value) => value.span,
            Self::Fresh(value) => value.span,
        }
    }

    pub(crate) fn input_bindings(&self) -> &[String] {
        match self {
            Self::Pool(value) => &value.inputs,
            Self::Retire(value) => std::slice::from_ref(&value.input),
            Self::Fresh(_) => &[],
        }
    }

    pub(crate) fn output_bindings(&self) -> &[String] {
        match self {
            Self::Pool(value) => &value.outputs,
            Self::Fresh(value) => std::slice::from_ref(&value.output),
            Self::Retire(_) => &[],
        }
    }

    pub(crate) fn cell_type(&self) -> Option<&str> {
        match self {
            Self::Pool(value) => Some(&value.ty),
            Self::Fresh(value) => Some(&value.ty),
            Self::Retire(_) => None,
        }
    }

    pub(crate) fn cell_type_mut(&mut self) -> Option<&mut String> {
        match self {
            Self::Pool(value) => Some(&mut value.ty),
            Self::Fresh(value) => Some(&mut value.ty),
            Self::Retire(_) => None,
        }
    }

    pub(crate) fn resolved(&self) -> ResolvedDisposition<'_> {
        match self {
            Self::Pool(value) => ResolvedDisposition::Pool(value),
            Self::Retire(value) => ResolvedDisposition::Retire(value),
            Self::Fresh(value) => ResolvedDisposition::Fresh(value),
        }
    }

    pub(crate) fn value_exprs(&self) -> Vec<&Expr> {
        match self {
            Self::Pool(value) => value
                .data_fields
                .iter()
                .flat_map(|field| match &field.treatment {
                    NextPoolFieldTreatment::Set(values) => values.iter().map(|(_, value)| value).collect(),
                    NextPoolFieldTreatment::Conserve => Vec::new(),
                })
                .chain(value.output_locks.iter().filter_map(|(_, lock)| exact_lock_expression(lock)))
                .collect(),
            Self::Fresh(value) => value.data_fields.iter().map(|(_, value)| value).chain(exact_lock_expression(&value.lock)).collect(),
            Self::Retire(_) => Vec::new(),
        }
    }

    pub(crate) fn value_exprs_mut(&mut self) -> Vec<&mut Expr> {
        fn lock_value(lock: &mut ReplaceLockTreatment) -> Option<&mut Expr> {
            match lock {
                ReplaceLockTreatment::Exact(value) | ReplaceLockTreatment::ExactHash(value) => Some(value),
                ReplaceLockTreatment::Same => None,
            }
        }
        match self {
            Self::Pool(value) => value
                .data_fields
                .iter_mut()
                .flat_map(|field| match &mut field.treatment {
                    NextPoolFieldTreatment::Set(values) => values.iter_mut().map(|(_, value)| value).collect(),
                    NextPoolFieldTreatment::Conserve => Vec::new(),
                })
                .chain(value.output_locks.iter_mut().filter_map(|(_, lock)| lock_value(lock)))
                .collect(),
            Self::Fresh(value) => value.data_fields.iter_mut().map(|(_, value)| value).chain(lock_value(&mut value.lock)).collect(),
            Self::Retire(_) => Vec::new(),
        }
    }

    /// Executable elaboration requires an already resolved Fresh policy. The
    /// parser's ownership walk does not have declaration evidence yet.
    pub(crate) fn kernel(&self, fresh_identity: Option<&IdentityPolicy>) -> Result<Vec<Stmt>> {
        match self {
            Self::Pool(value) => lower_pool_body(value),
            Self::Retire(value) => Ok(vec![Stmt::Discard(Expr::Destroy(DestroyExpr {
                expr: Box::new(Expr::Identifier(value.input.clone())),
                policy: value.absence_policy.clone(),
                span: value.span,
            }))]),
            Self::Fresh(value) => {
                let identity = fresh_identity.ok_or_else(|| {
                    CompileError::new("fresh identity must resolve from its Cell declaration before kernel elaboration", value.span)
                })?;
                let lock = exact_lock_expression(&value.lock).cloned().map(Box::new);
                let expression = if *identity == IdentityPolicy::None {
                    Expr::Create(CreateExpr {
                        target: Some(value.output.clone()),
                        ty: value.ty.clone(),
                        fields: value.data_fields.clone(),
                        lock,
                        span: value.span,
                    })
                } else {
                    Expr::CreateUnique(CreateUniqueExpr {
                        target: Some(value.output.clone()),
                        ty: value.ty.clone(),
                        type_args: Vec::new(),
                        fields: value.data_fields.clone(),
                        lock,
                        identity: Some(identity.clone()),
                        span: value.span,
                    })
                };
                Ok(vec![Stmt::Discard(expression)])
            }
        }
    }
}

fn exact_lock_expression(treatment: &ReplaceLockTreatment) -> Option<&Expr> {
    match treatment {
        ReplaceLockTreatment::Exact(value) | ReplaceLockTreatment::ExactHash(value) => Some(value),
        ReplaceLockTreatment::Same => None,
    }
}

pub(crate) fn validate_pool_assignments(pool: &NextPool) -> Result<()> {
    if pool.inputs.is_empty() || pool.outputs.is_empty() {
        return Err(SourceDiagnosticCode::PoolRoles.error("pool requires nonempty input and output roles", pool.span));
    }
    if pool.inputs.iter().collect::<BTreeSet<_>>().len() != pool.inputs.len()
        || pool.outputs.iter().collect::<BTreeSet<_>>().len() != pool.outputs.len()
    {
        return Err(SourceDiagnosticCode::PoolRoles.error("pool input and output roles must each be distinct", pool.span));
    }
    for field in &pool.data_fields {
        if let NextPoolFieldTreatment::Set(assignments) = &field.treatment
            && !assignments.iter().map(|(output, _)| output).eq(pool.outputs.iter())
        {
            return Err(SourceDiagnosticCode::PoolAssignments.error(
                format!(
                    "pool field '{}' must assign every pool output exactly once in declared order: {}",
                    field.field,
                    pool.outputs.join(", ")
                ),
                field.span,
            ));
        }
    }
    Ok(())
}

fn lower_pool_body(pool: &NextPool) -> Result<Vec<Stmt>> {
    validate_pool_assignments(pool)?;
    let mut body = Vec::new();
    for field in &pool.data_fields {
        if matches!(field.treatment, NextPoolFieldTreatment::Conserve) {
            let left = sum_pool_field(&pool.inputs, &field.field, pool.span)?;
            let right = sum_pool_field(&pool.outputs, &field.field, pool.span)?;
            body.push(Stmt::Discard(Expr::Require(RequireExpr {
                condition: Box::new(Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(left),
                    right: Box::new(right),
                    span: pool.span,
                })),
                message: None,
                span: pool.span,
            })));
        }
    }
    for output in &pool.outputs {
        let fields = pool
            .data_fields
            .iter()
            .map(|field| {
                let value = match &field.treatment {
                    NextPoolFieldTreatment::Conserve => pool_field(output, &field.field, pool.span),
                    NextPoolFieldTreatment::Set(assignments) => assignments
                        .iter()
                        .find(|(candidate, _)| candidate == output)
                        .map(|(_, expression)| expression.clone())
                        .ok_or_else(|| {
                            SourceDiagnosticCode::PoolAssignments
                                .error(format!("pool field '{}' has no assignment for output '{output}'", field.field), field.span)
                        })?,
                };
                Ok((field.field.clone(), value))
            })
            .collect::<Result<Vec<_>>>()?;
        let lock = pool
            .output_locks
            .iter()
            .find(|(candidate, _)| candidate == output)
            .and_then(|(_, treatment)| exact_lock_expression(treatment))
            .cloned()
            .map(Box::new);
        body.push(Stmt::Discard(Expr::Create(CreateExpr {
            target: Some(output.clone()),
            ty: pool.ty.clone(),
            fields,
            lock,
            span: pool.span,
        })));
    }
    body.extend(pool.inputs.iter().map(|input| {
        Stmt::Discard(Expr::Consume(crate::ast::ConsumeExpr { expr: Box::new(Expr::Identifier(input.clone())), span: pool.span }))
    }));
    Ok(body)
}

fn sum_pool_field(bindings: &[String], field: &str, span: Span) -> Result<Expr> {
    bindings
        .iter()
        .map(|binding| Expr::Cast(CastExpr { expr: Box::new(pool_field(binding, field, span)), ty: Type::U128, span }))
        .reduce(|left, right| Expr::Binary(BinaryExpr { op: BinaryOp::Add, left: Box::new(left), right: Box::new(right), span }))
        .ok_or_else(|| SourceDiagnosticCode::PoolRoles.error("pool requires nonempty input and output roles", span))
}

fn pool_field(binding: &str, field: &str, span: Span) -> Expr {
    Expr::FieldAccess(FieldAccessExpr { expr: Box::new(Expr::Identifier(binding.to_string())), field: field.to_string(), span })
}
