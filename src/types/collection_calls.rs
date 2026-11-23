use super::{expr_span, type_repr, CallableKind, TypeChecker, TypeEnv};
use crate::ast::{parse_bounded_collection_type, AssignOp, BoundedCollectionKind, CallExpr, Expr, Stmt, Type};
use crate::error::{CompileError, Result};

impl TypeChecker<'_> {
    pub(super) fn infer_bounded_collection_each(&mut self, env: &mut TypeEnv, call: &CallExpr) -> Result<Option<Type>> {
        let Expr::Identifier(operation) = call.func.as_ref() else {
            return Ok(None);
        };
        let kind = match operation.as_str() {
            "__cellscript_consume_each" => BoundedCollectionKind::CellSet,
            "__cellscript_create_each" => BoundedCollectionKind::List,
            _ => return Ok(None),
        };
        if !matches!(self.current_callable, Some(CallableKind::Action)) {
            return Err(CompileError::new(
                format!("{} is an action-only bounded lifecycle construct", operation.trim_start_matches("__cellscript_")),
                call.span,
            ));
        }
        let [Expr::Identifier(binding), Expr::Identifier(collection_binding), Expr::Block(body)] = call.args.as_slice() else {
            return Err(CompileError::new(
                format!("{} has an invalid internal bounded-collection shape", operation.trim_start_matches("__cellscript_")),
                call.span,
            ));
        };
        let collection_ty = env
            .lookup(collection_binding)
            .cloned()
            .ok_or_else(|| CompileError::new(format!("unknown bounded collection '{}'", collection_binding), call.span))?;
        let collection = parse_bounded_collection_type(&collection_ty).ok_or_else(|| {
            CompileError::new(
                format!(
                    "{} expects {}, found {}",
                    operation.trim_start_matches("__cellscript_"),
                    if kind == BoundedCollectionKind::CellSet { "BoundedCellSet<Resource, N>" } else { "BoundedList<Plan, N>" },
                    type_repr(&collection_ty)
                ),
                call.span,
            )
        })?;
        if collection.kind != kind {
            return Err(CompileError::new(
                format!(
                    "{} expects {}, found {}",
                    operation.trim_start_matches("__cellscript_"),
                    if kind == BoundedCollectionKind::CellSet { "BoundedCellSet" } else { "BoundedList" },
                    if collection.kind == BoundedCollectionKind::CellSet { "BoundedCellSet" } else { "BoundedList" }
                ),
                call.span,
            ));
        }

        let element_ty = collection.element_type;
        let mut body_env = env.child();
        body_env.bind_new(binding.clone(), element_ty, false, false, call.span)?;
        let mut created_types = Vec::new();
        for stmt in body {
            let (Stmt::Expr(expr) | Stmt::Discard(expr)) = stmt else {
                return Err(CompileError::new(
                    format!(
                        "{} body only accepts require predicates{}",
                        operation.trim_start_matches("__cellscript_"),
                        if kind == BoundedCollectionKind::List { " and one create template" } else { "" }
                    ),
                    call.span,
                ));
            };
            match (kind, expr) {
                (_, Expr::Require(_) | Expr::RequireBlock(_)) => {
                    let previous_callable = self.current_callable.replace(CallableKind::Invariant);
                    let result = self.infer_expr(&mut body_env, expr);
                    self.current_callable = previous_callable;
                    result?;
                }
                (BoundedCollectionKind::List, Expr::Create(create)) => {
                    self.infer_expr(&mut body_env, expr)?;
                    created_types.push(create.ty.clone());
                }
                (_, Expr::Assign(assign)) => {
                    let Expr::Identifier(accumulator) = assign.target.as_ref() else {
                        return Err(CompileError::new(
                            format!(
                                "{} only permits '+=' updates to named outer numeric accumulators",
                                operation.trim_start_matches("__cellscript_")
                            ),
                            assign.span,
                        ));
                    };
                    let Some(accumulator_ty) = env.lookup(accumulator).cloned() else {
                        return Err(CompileError::new(
                            format!(
                                "{} accumulator '{}' must be declared outside the bounded body",
                                operation.trim_start_matches("__cellscript_"),
                                accumulator
                            ),
                            assign.span,
                        ));
                    };
                    if assign.op != AssignOp::AddAssign || !env.is_mutable(accumulator) || !self.is_numeric_type(&accumulator_ty) {
                        return Err(CompileError::new(
                            format!(
                                "{} only permits '+=' updates to mutable outer numeric accumulators",
                                operation.trim_start_matches("__cellscript_")
                            ),
                            assign.span,
                        ));
                    }
                    let previous_callable = self.current_callable.replace(CallableKind::Invariant);
                    let mut purity_env = body_env.clone();
                    let result =
                        self.infer_expr_with_expected_type(&mut purity_env, &assign.value, &accumulator_ty, expr_span(&assign.value));
                    self.current_callable = previous_callable;
                    result?;
                    self.infer_expr(&mut body_env, expr)?;
                }
                _ => {
                    return Err(CompileError::new(
                        format!(
                            "{} body contains unsupported statement; {}",
                            operation.trim_start_matches("__cellscript_"),
                            if kind == BoundedCollectionKind::CellSet {
                                "consume_each owns the per-element consume and permits require predicates plus outer numeric '+=' accumulators"
                            } else {
                                "create_each permits require predicates, outer numeric '+=' accumulators, and exactly one create template"
                            }
                        ),
                        expr.span(),
                    ));
                }
            }
        }
        if kind == BoundedCollectionKind::CellSet {
            env.consume(collection_binding)?;
        } else if created_types.len() != 1 {
            return Err(CompileError::new(
                format!("create_each requires exactly one create template per bounded plan element, found {}", created_types.len()),
                call.span,
            ));
        }
        Ok(Some(Type::Unit))
    }
}
