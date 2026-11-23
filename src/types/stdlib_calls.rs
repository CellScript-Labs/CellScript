use super::{
    expr_span, type_repr, CellTypeKind, LockTargetDomain, TypeChecker, TypeEnv, CKB_SCRIPT_ARGS_TYPE, CKB_SCRIPT_HASH_TYPE,
    CKB_SCRIPT_VALUE_TYPE,
};
use crate::ast::{Expr, StdlibCallExpr, Type};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn validate_require_condition_is_pure(expr: &Expr, context: &str) -> Result<()> {
        match expr {
            Expr::Create(_)
            | Expr::Consume(_)
            | Expr::ReplaceRelation(_)
            | Expr::Destroy(_)
            | Expr::ReadRef(_)
            | Expr::Claim(_)
            | Expr::Settle(_)
            | Expr::CreateUnique(_)
            | Expr::ReplaceUnique(_) => {
                return Err(CompileError::new(
                    format!(
                        "{} contains cell/runtime operation; move state transition logic into a separate action statement",
                        context
                    ),
                    expr_span(expr),
                )
                .with_code(crate::error::DiagnosticCode::E1005));
            }
            Expr::Require(_) | Expr::RequireBlock(_) | Expr::Preserve(_) | Expr::StdlibCall(_) => {
                return Err(CompileError::new(
                    format!("{} contains verifier-boundary syntax; require expressions must be pure boolean constraints", context),
                    expr_span(expr),
                )
                .with_code(crate::error::DiagnosticCode::E1005));
            }
            Expr::If(_) | Expr::Match(_) | Expr::Block(_) => {
                return Err(CompileError::new(
                    format!("{} contains control flow; require expressions must be pure boolean constraints", context),
                    expr_span(expr),
                )
                .with_code(crate::error::DiagnosticCode::E1006));
            }
            Expr::Assign(_) => {
                return Err(CompileError::new(
                    format!("{} contains assignment; require expressions must be pure boolean constraints", context),
                    expr_span(expr),
                )
                .with_code(crate::error::DiagnosticCode::E1005));
            }
            Expr::Binary(binary) => {
                Self::validate_require_condition_is_pure(&binary.left, context)?;
                Self::validate_require_condition_is_pure(&binary.right, context)?;
            }
            Expr::Unary(unary) => Self::validate_require_condition_is_pure(&unary.expr, context)?,
            Expr::Call(call) => {
                Self::validate_require_condition_is_pure(&call.func, context)?;
                for arg in &call.args {
                    Self::validate_require_condition_is_pure(arg, context)?;
                }
            }
            Expr::FieldAccess(field) => Self::validate_require_condition_is_pure(&field.expr, context)?,
            Expr::Index(index) => {
                Self::validate_require_condition_is_pure(&index.expr, context)?;
                Self::validate_require_condition_is_pure(&index.index, context)?;
            }
            Expr::Assert(assert_expr) => {
                Self::validate_require_condition_is_pure(&assert_expr.condition, context)?;
                Self::validate_require_condition_is_pure(&assert_expr.message, context)?;
            }
            Expr::Tuple(items) | Expr::Array(items) => {
                for item in items {
                    Self::validate_require_condition_is_pure(item, context)?;
                }
            }
            Expr::Cast(cast) => Self::validate_require_condition_is_pure(&cast.expr, context)?,
            Expr::Range(range) => {
                Self::validate_require_condition_is_pure(&range.start, context)?;
                Self::validate_require_condition_is_pure(&range.end, context)?;
            }
            Expr::StructInit(init) => {
                for (_, value) in &init.fields {
                    Self::validate_require_condition_is_pure(value, context)?;
                }
            }
            Expr::Integer(_) | Expr::Bool(_) | Expr::String(_) | Expr::ByteString(_) | Expr::Identifier(_) => {}
        }
        Ok(())
    }

    pub(super) fn infer_stdlib_call(&mut self, env: &mut TypeEnv, call: &StdlibCallExpr) -> Result<Type> {
        let qualified = format!("std::{}::{}", call.namespace, call.name);
        let signature = crate::stdlib::signature::lookup(&call.namespace, &call.name).ok_or_else(|| {
            CompileError::new(
                format!("unknown stdlib pattern '{}' — each stdlib primitive must have a canonical signature", qualified),
                call.span,
            )
        })?;
        self.validate_stdlib_borrow_call(&qualified, call)?;
        self.validate_stdlib_arity(&qualified, call, signature.arity)?;
        if !signature.allows_preserve_fields && !call.preserve_fields.is_empty() {
            return Err(CompileError::new(format!("{} does not accept a preserve-field block", qualified), call.span));
        }
        match qualified.as_str() {
            "std::cell::same_lock" | "std::cell::preserve_lock" | "std::cell::preserve_capacity" => {
                self.validate_stdlib_arity(&qualified, call, 2)?;
                self.require_named_cell_identifier(env, &call.args[0], &qualified, "output")?;
                self.require_named_cell_identifier(env, &call.args[1], &qualified, "input")?;
                Ok(Type::Bool)
            }
            "std::cell::same_type" | "std::cell::preserve_type" | "std::accounting::conserved" => {
                self.validate_stdlib_arity(&qualified, call, 2)?;
                let left_ty = self.infer_expr(env, &call.args[0])?;
                let right_ty = self.infer_expr(env, &call.args[1])?;
                if qualified == "std::accounting::conserved" {
                    let left_amount = self.lookup_field_type(&left_ty, "amount", call.span)?;
                    let right_amount = self.lookup_field_type(&right_ty, "amount", call.span)?;
                    if !self.types_equal(&left_amount, &right_amount) {
                        return Err(CompileError::new("std::accounting::conserved requires matching amount field types", call.span));
                    }
                }
                Ok(Type::Bool)
            }
            "std::lifecycle::transfer" => {
                self.validate_stdlib_arity(&qualified, call, 3)?;
                let (input_ty, input_name) = self.require_named_linear_cell_operand(env, &call.args[0], &qualified, call.span)?;
                let output_ty = self.require_named_cell_identifier(env, &call.args[1], &qualified, "output")?;
                let lock_ty = self.infer_expr(env, &call.args[2])?;
                if !Self::is_lock_hash_type(&lock_ty) {
                    return Err(CompileError::new(
                        "std::lifecycle::transfer lock target must be ScriptHash, Address, or Hash",
                        call.span,
                    ));
                }
                if !self.types_equal(&input_ty, &output_ty) {
                    return Err(CompileError::new(
                        "std::lifecycle::transfer input and output must have the same Cell type",
                        call.span,
                    ));
                }
                self.validate_preserve_field_types(&output_ty, &input_ty, &call.preserve_fields, call.span)?;
                self.validate_stdlib_output_field_coverage(&qualified, &output_ty, &call.preserve_fields, call.span)?;
                env.consume(&input_name)?;
                Ok(Type::Bool)
            }
            "std::receipt::claim" => {
                self.validate_stdlib_arity(&qualified, call, 3)?;
                let (input_ty, input_name) = self.require_named_linear_cell_operand(env, &call.args[0], &qualified, call.span)?;
                let output_ty = self.require_named_cell_identifier(env, &call.args[1], &qualified, "output")?;
                self.validate_stdlib_lock_arg(&qualified, &call.args[2], env)?;
                self.validate_preserve_field_types(&output_ty, &input_ty, &call.preserve_fields, call.span)?;
                self.validate_stdlib_output_field_coverage(&qualified, &output_ty, &call.preserve_fields, call.span)?;
                self.validate_receipt_claim_pattern(&input_ty, &output_ty, call.span)?;
                env.consume(&input_name)?;
                Ok(Type::Bool)
            }
            "std::lifecycle::settle" => {
                self.validate_stdlib_arity(&qualified, call, 3)?;
                let (input_ty, input_name) = self.require_named_linear_cell_operand(env, &call.args[0], &qualified, call.span)?;
                let output_ty = self.require_named_cell_identifier(env, &call.args[1], &qualified, "output")?;
                self.validate_stdlib_lock_arg(&qualified, &call.args[2], env)?;
                self.validate_preserve_field_types(&output_ty, &input_ty, &call.preserve_fields, call.span)?;
                self.validate_stdlib_output_field_coverage(&qualified, &output_ty, &call.preserve_fields, call.span)?;
                env.consume(&input_name)?;
                Ok(Type::Bool)
            }
            _ => Err(CompileError::new(
                format!("unknown stdlib pattern '{}' — each stdlib primitive must have a canonical expansion", qualified),
                call.span,
            )),
        }
    }

    fn validate_stdlib_arity(&self, qualified: &str, call: &StdlibCallExpr, expected: usize) -> Result<()> {
        if call.args.len() == expected {
            Ok(())
        } else {
            Err(CompileError::new(format!("{} expects {} arguments, got {}", qualified, expected, call.args.len()), call.span))
        }
    }

    fn validate_stdlib_borrow_call(&self, qualified: &str, call: &StdlibCallExpr) -> Result<()> {
        let read_only = matches!(
            (call.namespace.as_str(), call.name.as_str()),
            ("cell", "same_lock" | "preserve_lock" | "preserve_capacity" | "same_type" | "preserve_type")
                | ("accounting", "conserved")
        );
        for arg in &call.args {
            let Some(borrow) = self.active_borrow_used_by_expr(arg) else {
                continue;
            };
            if !read_only {
                return Err(CompileError::new(
                    format!(
                        "function '{}' performs lifecycle operations and cannot receive borrowed linear view '{}'",
                        qualified, borrow.binding
                    ),
                    call.span,
                ));
            }
            if !matches!(arg, Expr::Identifier(name) if name == &borrow.binding) {
                return Err(CompileError::new(
                    format!("borrow '{}' cannot escape borrow block rooted at linear value '{}'", borrow.binding, borrow.root),
                    call.span,
                ));
            }
        }
        Ok(())
    }

    pub(super) fn is_script_args_payload_type(ty: &Type) -> bool {
        match ty {
            Type::Hash => true,
            Type::Array(inner, _) => matches!(inner.as_ref(), Type::U8),
            _ => false,
        }
    }

    pub(super) fn is_hash_bytes_type(ty: &Type) -> bool {
        matches!(ty, Type::Array(inner, 32) if matches!(inner.as_ref(), Type::U8))
    }

    pub(super) fn is_script_args_type(ty: &Type) -> bool {
        matches!(ty, Type::Named(name) if name.as_str() == CKB_SCRIPT_ARGS_TYPE)
    }

    pub(super) fn is_script_value_type(ty: &Type) -> bool {
        matches!(ty, Type::Named(name) if name.as_str() == CKB_SCRIPT_VALUE_TYPE)
    }

    pub(super) fn validate_script_hash_type_literal(value: u128, span: Span) -> Result<()> {
        match value {
            0 | 1 | 2 | 4 => Ok(()),
            _ => Err(CompileError::new("script hash_type must be one of data(0), type(1), data1(2), or data2(4)", span)),
        }
    }

    fn require_named_cell_identifier(&mut self, env: &mut TypeEnv, expr: &Expr, operation: &str, role: &str) -> Result<Type> {
        let ty = self.infer_expr(env, expr)?;
        if !self.is_linear_type(&ty) || self.contains_collection_owner(&ty) {
            return Err(CompileError::new(format!("{} {} must be a cell-backed value", operation, role), expr_span(expr)));
        }
        if !matches!(expr, Expr::Identifier(_)) {
            return Err(CompileError::new(format!("{} {} must be a named cell-backed binding", operation, role), expr_span(expr)));
        }
        Ok(ty)
    }

    fn validate_preserve_field_types(&self, output_ty: &Type, input_ty: &Type, fields: &[String], span: Span) -> Result<()> {
        for field in fields {
            let output_field_ty = self.lookup_field_type(output_ty, field, span)?;
            let input_field_ty = self.lookup_field_type(input_ty, field, span)?;
            if !self.types_equal(&output_field_ty, &input_field_ty) {
                return Err(CompileError::new(
                    format!(
                        "preserve field '{}' type mismatch: output has {}, input has {}",
                        field,
                        type_repr(&output_field_ty),
                        type_repr(&input_field_ty)
                    ),
                    span,
                ));
            }
        }
        Ok(())
    }

    fn validate_stdlib_output_field_coverage(&self, qualified: &str, output_ty: &Type, fields: &[String], span: Span) -> Result<()> {
        let Some(output_type_name) = Self::base_type_name(output_ty) else {
            return Err(CompileError::new(format!("{} output must be a named cell-backed type", qualified), span));
        };
        let Some(output_fields) = self.resolve_named_type_fields(output_type_name) else {
            return Err(CompileError::new(format!("{} output type '{}' has no known fields", qualified, output_type_name), span));
        };
        let preserved = fields.iter().map(String::as_str).collect::<HashSet<_>>();
        let missing = output_fields.keys().filter(|field| !preserved.contains(field.as_str())).cloned().collect::<Vec<_>>();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(CompileError::new(
                format!(
                    "{} output construction must cover every '{}' field; missing {}",
                    qualified,
                    output_type_name,
                    missing.join(", ")
                ),
                span,
            ))
        }
    }

    // Low-level lifecycle helpers retain their explicit value contracts;
    // relation/fresh/pool exact_hash clauses require the ScriptHash domain.
    fn is_lock_hash_type(ty: &Type) -> bool {
        matches!(ty, Type::Address | Type::Hash) || matches!(ty, Type::Named(name) if name == CKB_SCRIPT_HASH_TYPE)
    }

    pub(super) fn validate_creation_lock_arg(
        &mut self,
        operation: &str,
        lock: &Expr,
        target: Option<&str>,
        span: Span,
        env: &mut TypeEnv,
    ) -> Result<()> {
        let domain = target.and_then(|target| self.native_lock_domains.get(&(span.start, span.end, target.to_string()))).copied();
        let Some(domain) = domain else {
            return self.validate_stdlib_lock_arg(operation, lock, env);
        };
        let actual = self.infer_expr(env, lock)?;
        let (spelling, expected) = match domain {
            LockTargetDomain::Address => ("exact", Type::Address),
            LockTargetDomain::ScriptHash => ("exact_hash", Type::Named(CKB_SCRIPT_HASH_TYPE.to_string())),
        };
        if self.types_equal(&actual, &expected) {
            Ok(())
        } else {
            Err(CompileError::new(
                format!("{operation} lock `{spelling}` expects {}, found {}", type_repr(&expected), type_repr(&actual)),
                span,
            ))
        }
    }

    fn validate_stdlib_lock_arg(&mut self, qualified: &str, lock: &Expr, env: &mut TypeEnv) -> Result<()> {
        let lock_ty = self.infer_expr(env, lock)?;
        if Self::is_lock_hash_type(&lock_ty) {
            Ok(())
        } else {
            Err(CompileError::new(format!("{} lock target must be ScriptHash, Address, or Hash", qualified), expr_span(lock)))
        }
    }

    fn validate_receipt_claim_pattern(&self, receipt_ty: &Type, output_ty: &Type, span: Span) -> Result<()> {
        let Some(receipt_name) = Self::base_type_name(receipt_ty) else {
            return Err(CompileError::new("std::receipt::claim requires a receipt cell input", span));
        };
        if self.resolve_cell_type_kind(receipt_name) != Some(CellTypeKind::Receipt) {
            return Err(CompileError::new("std::receipt::claim requires a receipt cell input", span));
        }
        let Some(Some(declared_output)) = self.resolve_receipt_claim_output(receipt_name) else {
            return Err(CompileError::new(
                format!("std::receipt::claim with an output requires receipt '{}' to declare a claim output type", receipt_name),
                span,
            ));
        };
        if self.types_equal(output_ty, &declared_output) {
            Ok(())
        } else {
            Err(CompileError::new(
                format!(
                    "std::receipt::claim output type mismatch: receipt '{}' declares {}, got {}",
                    receipt_name,
                    type_repr(&declared_output),
                    type_repr(output_ty)
                ),
                span,
            ))
        }
    }
}
