use super::{
    check_replace_assigned_field, expr_span, nested, note_replace_field, type_repr, LinearState, TypeChecker, TypeEnv,
    CKB_SCRIPT_HASH_TYPE,
};
use crate::ast::{
    expression_fails_closed, statements_fail_closed, BinaryExpr, BinaryOp, BindingPattern, CapabilityOperation, Expr, LetStmt,
    ReplaceDataTreatment, ReplaceFieldTreatment, ReplaceLockTreatment, Stmt, Type, UnaryOp,
};
use crate::error::{CompileError, Result, Span};
use std::collections::HashSet;

impl TypeChecker<'_> {
    pub(super) fn infer_let_value_type(&mut self, env: &mut TypeEnv, let_stmt: &LetStmt) -> Result<Type> {
        if let Some(declared_ty) = &let_stmt.ty {
            return self.infer_expr_with_expected_type(env, &let_stmt.value, declared_ty, let_stmt.span);
        }
        if let Expr::Array(elems) = &let_stmt.value
            && elems.is_empty()
        {
            return match &let_stmt.ty {
                Some(declared @ Type::Array(_, 0)) => Ok(declared.clone()),
                Some(Type::Array(_, size)) => Err(CompileError::new(
                    format!("empty array literal cannot initialize non-empty array of length {}", size),
                    let_stmt.span,
                )),
                Some(_) => Err(CompileError::new("empty array literal requires an array type annotation", let_stmt.span)),
                None => Err(CompileError::new("empty array literal requires an explicit array type annotation", let_stmt.span)),
            };
        }
        self.infer_expr(env, &let_stmt.value)
    }

    pub(super) fn infer_expr_with_expected_type(
        &mut self,
        env: &mut TypeEnv,
        expr: &Expr,
        expected_ty: &Type,
        span: Span,
    ) -> Result<Type> {
        let ty = match expr {
            Expr::Tuple(items) if items.is_empty() && matches!(expected_ty, Type::Unit) => Ok(Type::Unit),
            Expr::Call(call)
                if matches!(call.func.as_ref(), Expr::Identifier(name)
                if matches!(name.as_str(), "Vec::new" | "Vec::with_capacity") && self.resolve_source_signature(name, call.span)?.is_none()) =>
            {
                // Infer normally first so context cannot hide bad arity,
                // capacity types, or argument diagnostics.
                let actual = self.infer_expr(env, expr)?;
                let Some(item) = expected_ty.vector_element() else { return Ok(actual) };
                self.validate_type(expected_ty)?;
                if self.type_contains_reference(item)
                    || self.is_linear_type(item)
                    || !self.bounded_list_element_is_fixed_width(item, &mut HashSet::new())
                    || !self.payload_type_runtime_width(item, &mut HashSet::new()).is_some_and(|width| (1..=256).contains(&width))
                {
                    return Err(CompileError::new(
                        format!(
                            "local Vec constructor requires an owned fixed-width element of 1..=256 bytes, found {}",
                            type_repr(item)
                        ),
                        span,
                    ));
                }
                Ok(expected_ty.clone())
            }
            Expr::Integer(value) => {
                if let Some(ty) = Self::integer_literal_type_for_expected(*value, expected_ty, span)? {
                    Ok(ty)
                } else {
                    self.infer_expr(env, expr)
                }
            }
            Expr::Binary(binary)
                if matches!(
                    binary.op,
                    BinaryOp::Add
                        | BinaryOp::Sub
                        | BinaryOp::Mul
                        | BinaryOp::Div
                        | BinaryOp::Mod
                        | BinaryOp::BitAnd
                        | BinaryOp::BitOr
                        | BinaryOp::BitXor
                        | BinaryOp::Shl
                        | BinaryOp::Shr
                ) && self.is_numeric_type(expected_ty) =>
            {
                self.infer_numeric_binary_with_expected_type(env, binary, expected_ty, span)
            }
            Expr::Tuple(elems) if matches!(expected_ty, Type::Tuple(_)) => {
                self.reject_direct_borrow_escape(expr, span)?;
                let Type::Tuple(expected) = expected_ty else { unreachable!() };
                if elems.len() != expected.len() {
                    return Err(CompileError::new("tuple literal arity differs from its expected type", span));
                }
                for (elem, ty) in elems.iter().zip(expected) {
                    let actual = self.infer_expr_with_expected_type(env, elem, ty, expr_span(elem))?;
                    if !self.initializer_types_equal(&actual, ty) {
                        return Err(CompileError::new("tuple literal field differs from its expected type", span));
                    }
                }
                Ok(expected_ty.clone())
            }
            Expr::Array(elems) => self.infer_array_literal_with_expected_type(env, elems, expected_ty, span),
            Expr::Block(stmts) => self.infer_tail_block_value_with_expected_type(env, stmts, expected_ty, span),
            Expr::If(if_expr) => {
                let cond_ty = self.infer_expr(env, &if_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if expression condition must be boolean", if_expr.span));
                }
                let mut then_env = env.child();
                let then_ty = self.infer_expr_with_expected_type(
                    &mut then_env,
                    &if_expr.then_branch,
                    expected_ty,
                    expr_span(&if_expr.then_branch),
                )?;
                then_env.check_linear_complete()?;
                let mut else_env = env.child();
                let else_ty = self.infer_expr_with_expected_type(
                    &mut else_env,
                    &if_expr.else_branch,
                    expected_ty,
                    expr_span(&if_expr.else_branch),
                )?;
                else_env.check_linear_complete()?;
                if self.types_equal(&then_ty, &else_ty) && self.initializer_types_equal(&then_ty, expected_ty) {
                    env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_expr.span)?;
                    Ok(expected_ty.clone())
                } else {
                    Err(CompileError::new(
                        format!("if expression branches must have matching types, got {:?} and {:?}", then_ty, else_ty),
                        if_expr.span,
                    ))
                }
            }
            _ => self.infer_expr(env, expr),
        }?;
        self.ensure_expr_has_value(expr, &ty, span)?;
        Ok(ty)
    }

    /// Unit is a value; absence of a declared return is a separate callable
    /// contract. Expression statements may discard either, but value consumers
    /// must not turn a no-result call into a generic unit argument or binding.
    pub(super) fn ensure_expr_has_value(&self, expr: &Expr, ty: &Type, span: Span) -> Result<()> {
        let unit = matches!(ty, Type::Unit) || matches!(ty, Type::Tuple(items) if items.is_empty());
        if unit && !self.unit_expr_has_value(expr)? {
            return Err(CompileError::new(
                "expression does not produce a value; use an explicit result declaration or keep the operation as a statement",
                span,
            )
            .with_code(crate::error::DiagnosticCode::E2114));
        }
        Ok(())
    }

    fn unit_expr_has_value(&self, expr: &Expr) -> Result<bool> {
        Ok(match expr {
            Expr::Call(call) => match call.func.as_ref() {
                Expr::Identifier(name) => {
                    self.resolve_source_signature(name, call.span)?.is_some_and(|signature| signature.return_type.is_some())
                }
                _ => false,
            },
            Expr::Block(stmts) => match stmts.last() {
                Some(Stmt::Expr(expr)) => self.unit_expr_has_value(expr)?,
                _ => false,
            },
            Expr::If(branch) => self.unit_expr_has_value(&branch.then_branch)? && self.unit_expr_has_value(&branch.else_branch)?,
            Expr::Match(matched) => {
                let mut available = !matched.arms.is_empty();
                for arm in &matched.arms {
                    available &= self.unit_expr_has_value(&arm.value)?;
                }
                available
            }
            Expr::Identifier(_) | Expr::Tuple(_) | Expr::FieldAccess(_) | Expr::Index(_) | Expr::Cast(_) => true,
            _ => false,
        })
    }

    fn infer_value_expr(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<Type> {
        let ty = self.infer_expr(env, expr)?;
        self.ensure_expr_has_value(expr, &ty, expr_span(expr))?;
        Ok(ty)
    }

    fn infer_numeric_binary_with_expected_type(
        &mut self,
        env: &mut TypeEnv,
        binary: &BinaryExpr,
        expected_ty: &Type,
        span: Span,
    ) -> Result<Type> {
        let left_ty = if matches!(binary.left.as_ref(), Expr::Integer(_) | Expr::Binary(_)) {
            self.infer_expr_with_expected_type(env, &binary.left, expected_ty, expr_span(&binary.left))?
        } else {
            self.infer_expr(env, &binary.left)?
        };
        self.require_numeric_operand_widenable(&left_ty, expected_ty, "left", span)?;
        if matches!(binary.op, BinaryOp::Shl | BinaryOp::Shr) {
            let right_ty = self.infer_expr(env, &binary.right)?;
            if !Self::is_unsigned_shift_count_type(&right_ty) {
                return Err(CompileError::new("shift amount must have type u8, u16, u32, or u64", binary.span));
            }
            if let Expr::Integer(amount) = binary.right.as_ref() {
                let width = Self::integer_type_width_bits(expected_ty).expect("numeric types have a width");
                if *amount >= width.into() {
                    return Err(CompileError::new(
                        format!("shift amount {} is out of range for {}-bit {}", amount, width, type_repr(expected_ty)),
                        binary.span,
                    )
                    .with_code(crate::error::DiagnosticCode::E2106));
                }
            }
            return Ok(expected_ty.clone());
        }

        let right_ty = if matches!(binary.right.as_ref(), Expr::Integer(_) | Expr::Binary(_)) {
            self.infer_expr_with_expected_type(env, &binary.right, expected_ty, expr_span(&binary.right))?
        } else {
            self.infer_expr(env, &binary.right)?
        };
        self.require_numeric_operand_widenable(&right_ty, expected_ty, "right", span)?;
        Ok(expected_ty.clone())
    }

    fn require_numeric_operand_widenable(&self, actual: &Type, expected: &Type, side: &str, span: Span) -> Result<()> {
        if self.types_equal(actual, expected) {
            return Ok(());
        }
        let unsigned = |ty: &Type| matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128);
        if unsigned(actual) && unsigned(expected) && Self::integer_type_width_bits(actual) <= Self::integer_type_width_bits(expected) {
            return Ok(());
        }
        if matches!(actual, Type::I32) || matches!(expected, Type::I32) {
            return Err(CompileError::new("arithmetic operations require matching numeric types", span));
        }
        Err(CompileError::new(
            format!("binary {side} operand must have expected type {}, found {}", type_repr(expected), type_repr(actual)),
            span,
        ))
    }

    pub(super) fn infer_array_literal_with_expected_type(
        &mut self,
        env: &mut TypeEnv,
        elems: &[Expr],
        expected_ty: &Type,
        span: Span,
    ) -> Result<Type> {
        if let Some(item_ty) = expected_ty.vector_element() {
            for elem in elems {
                let actual_ty = self.infer_expr_with_expected_type(env, elem, item_ty, expr_span(elem))?;
                if self.type_contains_reference(&actual_ty) {
                    return Err(CompileError::new(
                        format!(
                            "Vec literal cannot store reference type {}; Vec<T> values must use owned non-reference items",
                            type_repr(&actual_ty)
                        ),
                        expr_span(elem),
                    ));
                }
                if !self.types_equal(&actual_ty, item_ty) {
                    return Err(CompileError::new(
                        format!("Vec literal type mismatch: expected {:?}, found {:?}", item_ty, actual_ty),
                        expr_span(elem),
                    ));
                }
            }
            return Ok(expected_ty.clone());
        }

        if let Type::Array(item_ty, expected_len) = expected_ty {
            if elems.len() != *expected_len {
                if elems.is_empty() {
                    return Err(CompileError::new(
                        format!("empty array literal cannot initialize non-empty array of length {}", expected_len),
                        span,
                    ));
                }
                return Err(CompileError::new(
                    format!("array literal length mismatch: expected {}, found {}", expected_len, elems.len()),
                    span,
                ));
            }
            for elem in elems {
                let actual_ty = self.infer_expr_with_expected_type(env, elem, item_ty, expr_span(elem))?;
                if !self.types_equal(&actual_ty, item_ty) {
                    return Err(CompileError::new(
                        format!("array literal type mismatch: expected {:?}, found {:?}", item_ty, actual_ty),
                        expr_span(elem),
                    ));
                }
            }
            return Ok(expected_ty.clone());
        }

        self.infer_expr(env, &Expr::Array(elems.to_vec()))
    }

    pub(super) fn infer_expr(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<Type> {
        let previous = self.collection_constraint;
        if matches!(expr, Expr::Require(_) | Expr::RequireBlock(_) | Expr::Assert(_)) {
            self.collection_constraint = true;
        }
        let result = self.infer_expr_inner(env, expr);
        self.collection_constraint = previous;
        if let Ok(ty) = &result
            && self.contains_collection_owner(ty)
            && !(self.collection_owner(ty).is_some() && matches!(expr, Expr::Identifier(_) | Expr::Call(_)))
        {
            return Err(CompileError::new(
                "collection owners and references cannot be wrapped or escape a local owner binding",
                expr_span(expr),
            ));
        }
        result
    }

    fn infer_expr_inner(&mut self, env: &mut TypeEnv, expr: &Expr) -> Result<Type> {
        self.validate_expr_allowed_in_current_callable(expr)?;
        match expr {
            Expr::Integer(value) => Ok(if *value <= u64::MAX as u128 { Type::U64 } else { Type::U128 }),
            Expr::Bool(_) => Ok(Type::Bool),
            Expr::String(_) => Ok(Type::Named("String".to_string())),
            Expr::ByteString(bytes) => Ok(Type::Array(Box::new(Type::U8), bytes.len())),
            Expr::Identifier(name) => {
                if let Some(ty) = env.lookup(name).cloned() {
                    if self.collection_owner(&ty).is_some() && env.linear_state(name) != Some(LinearState::Available) {
                        return Err(CompileError::new(format!("collection owner '{name}' was moved or dropped"), expr_span(expr)));
                    }
                    if ty.opening_value_type().is_some() {
                        return Err(CompileError::new(
                            format!(
                                "opening witness '{}' is opaque and may only be consumed directly as commitment::open's second argument",
                                name
                            ),
                            expr_span(expr),
                        ));
                    }
                    Ok(ty)
                } else if let Some(constant) = self.resolve_constant(name) {
                    Ok(constant.ty)
                } else if let Some(ty) = self.enum_variant_expr_type(name, expr_span(expr))? {
                    Ok(ty)
                } else if let Some(ty) = self.flow_state_expr_type(name, expr_span(expr))? {
                    Ok(ty)
                } else if let Some((prefix, _)) = name.split_once("::") {
                    if self.collection_pools.contains_key(prefix) {
                        return Err(CompileError::new("collection owner values must originate from Owner::new()", expr_span(expr)));
                    }
                    Ok(Type::Named(prefix.to_string()))
                } else {
                    Err(CompileError::new(format!("undefined variable '{}'", name), Span::default()))
                }
            }
            Expr::Assign(assign) => self.infer_assign_expr(env, assign),
            Expr::Binary(bin) => {
                self.reject_direct_borrow_escape(expr, bin.span)?;
                let left_ty = self.infer_value_expr(env, &bin.left)?;
                let right_ty = self.infer_value_expr(env, &bin.right)?;
                if self.contains_collection_owner(&left_ty) || self.contains_collection_owner(&right_ty) {
                    return Err(CompileError::new(
                        "collection owners have no pointer identity, comparison or arithmetic operations",
                        bin.span,
                    ));
                }
                if crate::cell_data_view::invocation_local(&left_ty) || crate::cell_data_view::invocation_local(&right_ty) {
                    return Err(CompileError::new(
                        "ExactCellSelection and CellDataView have no scalar, pointer-identity or arithmetic operations",
                        bin.span,
                    ));
                }

                match bin.op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                        if !self.is_numeric_type(&left_ty) || !self.is_numeric_type(&right_ty) {
                            return Err(CompileError::new("arithmetic operations require numeric types", bin.span));
                        }
                        self.numeric_binary_result_type(&bin.left, &left_ty, &bin.right, &right_ty, bin.span)
                    }
                    BinaryOp::Eq | BinaryOp::Ne => {
                        if nested::contains_nested_type(&left_ty) || nested::contains_nested_type(&right_ty) {
                            return Err(CompileError::new(
                                "nested views cannot be compared as pointers; compare their declared fields or indexed bytes",
                                bin.span,
                            ));
                        }
                        if !self.binary_operand_types_compatible(&bin.left, &left_ty, &bin.right, &right_ty, bin.span)? {
                            return Err(CompileError::new("comparison requires matching types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if Self::is_ordered_ckb_temporal_type(&left_ty) || Self::is_ordered_ckb_temporal_type(&right_ty) {
                            if left_ty != right_ty {
                                return Err(CompileError::new("ordering comparison requires matching CKB temporal domains", bin.span));
                            }
                            return Ok(Type::Bool);
                        }
                        if !self.is_numeric_type(&left_ty) || !self.is_numeric_type(&right_ty) {
                            return Err(CompileError::new("ordering comparison requires numeric types", bin.span));
                        }
                        if let Err(err) = self.numeric_binary_result_type(&bin.left, &left_ty, &bin.right, &right_ty, bin.span) {
                            if err.code == Some(crate::error::DiagnosticCode::E1030) {
                                return Err(err);
                            }
                            return Err(CompileError::new("ordering comparison requires matching numeric types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        if !self.is_bool_type(&left_ty) || !self.is_bool_type(&right_ty) {
                            return Err(CompileError::new("logical operations require boolean types", bin.span));
                        }
                        Ok(Type::Bool)
                    }
                    BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                        if !self.is_numeric_type(&left_ty) || !self.is_numeric_type(&right_ty) {
                            return Err(CompileError::new("bitwise operations require integer types", bin.span));
                        }
                        self.numeric_binary_result_type(&bin.left, &left_ty, &bin.right, &right_ty, bin.span)
                    }
                    BinaryOp::Shl | BinaryOp::Shr => {
                        if !self.is_numeric_type(&left_ty) {
                            return Err(CompileError::new("shift left operand must be an integer", bin.span));
                        }
                        if !Self::is_unsigned_shift_count_type(&right_ty) {
                            return Err(CompileError::new("shift amount must have type u8, u16, u32, or u64", bin.span));
                        }
                        if let Expr::Integer(amount) = bin.right.as_ref() {
                            let width = Self::integer_type_width_bits(&left_ty).expect("numeric types have a width");
                            if *amount >= width.into() {
                                return Err(CompileError::new(
                                    format!("shift amount {} is out of range for {}-bit {}", amount, width, type_repr(&left_ty)),
                                    bin.span,
                                )
                                .with_code(crate::error::DiagnosticCode::E2106));
                            }
                        }
                        Ok(left_ty)
                    }
                }
            }
            Expr::Unary(unary) => {
                if unary.op != UnaryOp::Deref {
                    self.reject_direct_borrow_escape(expr, unary.span)?;
                }
                let expr_ty = self.infer_expr(env, &unary.expr)?;
                match unary.op {
                    UnaryOp::Neg => {
                        if !self.is_numeric_type(&expr_ty) {
                            return Err(CompileError::new("negation requires numeric type", unary.span));
                        }
                        Ok(expr_ty)
                    }
                    UnaryOp::Not => {
                        if !self.is_bool_type(&expr_ty) {
                            return Err(CompileError::new("logical not requires boolean type", unary.span));
                        }
                        Ok(Type::Bool)
                    }
                    UnaryOp::Ref => Ok(Type::Ref(Box::new(expr_ty))),
                    UnaryOp::Deref => match expr_ty {
                        Type::Ref(inner) | Type::MutRef(inner) if !self.is_linear_type(&inner) => Ok((*inner).clone()),
                        Type::Ref(_) | Type::MutRef(_) => Err(CompileError::new(
                            "cannot materialize a Cell-backed linear value through a read-only borrow",
                            unary.span,
                        )),
                        _ => Err(CompileError::new("cannot dereference a non-reference value", unary.span)),
                    },
                }
            }
            Expr::Call(call) => {
                let source_signature = match call.func.as_ref() {
                    Expr::Identifier(name) => self.resolve_source_signature(name, call.span)?,
                    _ => None,
                };
                if let Some(ty) = self.infer_collection_helper_call(env, call, source_signature.as_ref())? {
                    return Ok(ty);
                }
                if source_signature.is_none() {
                    if let Some(ty) = self.infer_collection_call(env, call)? {
                        return Ok(ty);
                    }
                    if let Some(result) = self.infer_exact_role_verification(env, call)? {
                        return Ok(result);
                    }
                    if let Some(result) = self.infer_commitment_open_call(env, call)? {
                        return Ok(result);
                    }
                    if let Some(result) = self.infer_enum_constructor_call(env, call)? {
                        return Ok(result);
                    }
                    if let Some(result) = self.infer_bounded_collection_each(env, call)? {
                        return Ok(result);
                    }
                    self.validate_runtime_call_allowed_in_current_callable(call)?;
                }
                self.reject_forbidden_consensus_call(call)?;
                let mut arg_types = Vec::with_capacity(call.args.len());
                for arg in &call.args {
                    arg_types.push(self.infer_value_expr(env, arg)?);
                }
                if arg_types.iter().any(|ty| self.contains_collection_owner(ty)) {
                    return Err(CompileError::new(
                        "collection owners cannot cross a call boundary; pass &owner to a declared read-only helper",
                        call.span,
                    ));
                }
                if arg_types.iter().any(crate::cell_data_view::invocation_local) {
                    return Err(CompileError::new(
                        "ExactCellSelection and CellDataView cannot cross a call boundary; read their fields in the owning invocation",
                        call.span,
                    ));
                }
                if arg_types.iter().any(nested::contains_nested_type)
                    && !(arg_types.iter().filter(|ty| nested::contains_nested_type(ty)).all(nested::direct_view)
                        && matches!(call.func.as_ref(), Expr::Identifier(name) if self.nested_readonly_helper(name)))
                {
                    return Err(CompileError::new(
                        "nested values and views require an observation-only helper with the same declared bounds and no reference wrapper",
                        call.span,
                    ));
                }
                let return_type = self.infer_call_type(env, call, &arg_types, source_signature)?;
                for arg in &call.args {
                    self.mark_expr_as_moved(env, arg)?;
                }
                Ok(return_type)
            }
            Expr::FieldAccess(field) => {
                self.reject_collection_member_escape(env, expr)?;
                let expr_ty = self.infer_expr(env, &field.expr)?;
                let field_ty = self.lookup_field_type(&expr_ty, &field.field, field.span)?;
                if self.is_linear_type(&field_ty) {
                    return Err(CompileError::new(
                        "field access cannot move a linear value out of an aggregate; use destructuring to bind linear fields",
                        field.span,
                    ));
                }
                Ok(field_ty)
            }
            Expr::Index(index) => {
                if let Some(ty) = self.infer_collection_index(env, index)? {
                    return Ok(ty);
                }
                self.reject_direct_borrow_escape(expr, index.span)?;
                let expr_ty = self.infer_expr(env, &index.expr)?;
                let index_ty = self.infer_expr(env, &index.index)?;
                if nested::contains_nested_type(&expr_ty) && !matches!(index_ty, Type::U8 | Type::U16 | Type::U32 | Type::U64) {
                    return Err(CompileError::new("nested view indexes require u8, u16, u32 or u64", index.span));
                }
                if !self.is_numeric_type(&index_ty) {
                    return Err(CompileError::new("index expression requires a numeric index", index.span));
                }
                let item_ty = self.index_result_type(&expr_ty, index.span)?;
                if self.is_linear_type(&item_ty) {
                    return Err(CompileError::new(
                        "index access cannot move a linear value out of an aggregate; use destructuring or explicit iteration that handles each item",
                        index.span,
                    ));
                }
                Ok(item_ty)
            }
            Expr::Create(create) => {
                self.require_create_target_cell_backed(&create.ty, create.span)?;
                self.check_field_initializer(env, &create.ty, &create.fields, create.span, "create")?;
                if let Some(lock) = &create.lock {
                    self.validate_creation_lock_arg("create", lock, create.target.as_deref(), create.span, env)?;
                }
                if let Some(target) = &create.target {
                    let Some(target_ty) = env.lookup(target).cloned() else {
                        return Err(CompileError::new(
                            format!("create target '{}' is not declared as an action output binding", target),
                            create.span,
                        ));
                    };
                    let Type::Named(target_type_name) = target_ty else {
                        return Err(CompileError::new(
                            format!("create target '{}' must be a named Cell output binding", target),
                            create.span,
                        ));
                    };
                    if target_type_name.as_str() != create.ty {
                        return Err(CompileError::new(
                            format!(
                                "create target '{}' has type '{}', but initializer constructs '{}'",
                                target, target_type_name, create.ty
                            ),
                            create.span,
                        ));
                    }
                }
                Ok(Type::Named(create.ty.clone()))
            }
            Expr::Consume(consume) => {
                let (_consume_ty, name) = self.require_named_linear_cell_operand(env, &consume.expr, "consume", consume.span)?;
                env.consume(&name)?;
                Ok(Type::U64)
            }
            Expr::Destroy(destroy) => {
                let (destroy_ty, name) = self.require_named_linear_cell_operand(env, &destroy.expr, "destroy", destroy.span)?;
                self.require_capability_operation(&destroy_ty, CapabilityOperation::Destroy, None, destroy.span)?;
                env.destroy(&name)?;
                Ok(Type::U64)
            }
            Expr::ReadRef(read_ref) => {
                self.require_read_ref_target_cell_backed(&read_ref.ty, read_ref.span)?;
                Ok(Type::Ref(Box::new(Type::Named(read_ref.ty.clone()))))
            }
            Expr::Claim(claim) => {
                let (receipt_ty, name) = self.require_named_linear_cell_operand(env, &claim.receipt, "claim", claim.span)?;
                if !self.is_receipt_type(&receipt_ty) {
                    return Err(CompileError::new("claim requires a receipt value", claim.span));
                }
                env.consume(&name)?;
                let receipt_name = Self::base_type_name(&receipt_ty).unwrap_or_default();
                Ok(self.resolve_receipt_claim_output(receipt_name).flatten().unwrap_or(Type::U64))
            }
            Expr::Settle(settle) => {
                let (settle_ty, name) = self.require_named_linear_cell_operand(env, &settle.expr, "settle", settle.span)?;
                env.consume(&name)?;
                Ok(settle_ty)
            }
            Expr::CreateUnique(cu) => {
                self.require_create_target_cell_backed(&cu.ty, cu.span)?;
                self.check_field_initializer(env, &cu.ty, &cu.fields, cu.span, "create_unique")?;
                self.validate_unique_identity_policy(&cu.ty, &cu.identity, cu.span, "create_unique")?;
                self.require_declared_identity_match(&cu.ty, &cu.identity, cu.span, "create_unique")?;
                if let Some(target) = &cu.target {
                    let Some(target_ty) = env.lookup(target).cloned() else {
                        return Err(CompileError::new(
                            format!("create_unique target '{}' is not declared as an action output binding", target),
                            cu.span,
                        ));
                    };
                    let Type::Named(target_type_name) = target_ty else {
                        return Err(CompileError::new(
                            format!("create_unique target '{}' must be a named Cell output binding", target),
                            cu.span,
                        ));
                    };
                    if target_type_name.as_str() != cu.ty {
                        return Err(CompileError::new(
                            format!(
                                "create_unique target '{}' has type '{}', but initializer constructs '{}'",
                                target, target_type_name, cu.ty
                            ),
                            cu.span,
                        ));
                    }
                }
                if let Some(lock) = &cu.lock {
                    self.validate_creation_lock_arg("create_unique", lock, cu.target.as_deref(), cu.span, env)?;
                }
                Ok(Type::Named(cu.ty.clone()))
            }
            Expr::ReplaceUnique(ru) => {
                let (input_ty, name) = self.require_named_linear_cell_operand(env, &ru.expr, "replace_unique", ru.span)?;
                let output_ty = Type::Named(ru.ty.clone());
                if !self.types_equal(&input_ty, &output_ty) {
                    return Err(CompileError::new(
                        format!("replace_unique output type '{}' must match consumed input type {:?}", ru.ty, input_ty),
                        ru.span,
                    ));
                }
                self.check_field_initializer(env, &ru.ty, &ru.fields, ru.span, "replace_unique")?;
                self.validate_unique_identity_policy(&ru.ty, &ru.identity, ru.span, "replace_unique")?;
                self.require_capability_operation(&input_ty, CapabilityOperation::ReplaceUnique, Some(&ru.identity), ru.span)?;
                env.consume(&name)?;
                Ok(input_ty)
            }
            Expr::Assert(assert_expr) => {
                let cond_ty = self.infer_expr(env, &assert_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("assert condition must be boolean", assert_expr.span));
                }
                if !matches!(assert_expr.message.as_ref(), Expr::String(_)) {
                    return Err(CompileError::new("assert message must be a string literal", expr_span(&assert_expr.message)));
                }
                Ok(Type::Unit)
            }
            Expr::Require(require_expr) => {
                Self::validate_require_condition_is_pure(&require_expr.condition, "require condition")?;
                let cond_ty = self.infer_expr(env, &require_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("require condition must be boolean", require_expr.span));
                }
                if let Some(message) = &require_expr.message
                    && !matches!(message.as_ref(), Expr::String(_))
                {
                    return Err(CompileError::new("require message must be a string literal", expr_span(message)));
                }
                Ok(Type::Bool)
            }
            Expr::Block(stmts) => {
                self.reject_direct_borrow_escape(expr, expr_span(expr))?;
                let mut block_env = env.child();
                let last_ty = self.infer_tail_block_value(&mut block_env, stmts)?;
                if !statements_fail_closed(stmts) {
                    block_env.check_linear_complete()?;
                    env.merge_existing_linear_states_from(&block_env);
                }
                Ok(last_ty)
            }
            Expr::Tuple(elems) => {
                self.reject_direct_borrow_escape(expr, expr_span(expr))?;
                let mut types = Vec::new();
                for elem in elems {
                    types.push(self.infer_value_expr(env, elem)?);
                }
                Ok(Type::Tuple(types))
            }
            Expr::Array(elems) => {
                self.reject_direct_borrow_escape(expr, expr_span(expr))?;
                if elems.is_empty() {
                    return Err(CompileError::new("empty array literal requires an explicit array type annotation", expr_span(expr)));
                }
                let elem_ty = self.infer_value_expr(env, &elems[0])?;
                for elem in elems.iter().skip(1) {
                    let next_ty = self.infer_value_expr(env, elem)?;
                    if !self.types_equal(&elem_ty, &next_ty) {
                        return Err(CompileError::new("array elements must have matching types", expr_span(elem)));
                    }
                }
                Ok(Type::Array(Box::new(elem_ty), elems.len()))
            }
            Expr::If(if_expr) => {
                self.reject_direct_borrow_escape(expr, if_expr.span)?;
                let cond_ty = self.infer_expr(env, &if_expr.condition)?;
                if !self.is_bool_type(&cond_ty) {
                    return Err(CompileError::new("if expression condition must be boolean", if_expr.span));
                }
                let mut then_env = env.child();
                let then_ty = self.infer_expr(&mut then_env, &if_expr.then_branch)?;
                let mut else_env = env.child();
                let else_ty = self.infer_expr(&mut else_env, &if_expr.else_branch)?;
                let then_fails = expression_fails_closed(&if_expr.then_branch);
                let else_fails = expression_fails_closed(&if_expr.else_branch);
                if then_fails {
                    if !else_fails {
                        env.merge_existing_linear_states_from(&else_env);
                    }
                    Ok(else_ty)
                } else if else_fails {
                    env.merge_existing_linear_states_from(&then_env);
                    Ok(then_ty)
                } else if self.types_equal(&then_ty, &else_ty) {
                    env.merge_branch_linear_states(&then_env, false, Some(&else_env), false, if_expr.span)?;
                    Ok(then_ty)
                } else {
                    Err(CompileError::new(
                        format!("if expression branches must have matching types, got {:?} and {:?}", then_ty, else_ty),
                        if_expr.span,
                    ))
                }
            }
            Expr::Cast(cast) => {
                self.reject_direct_borrow_escape(expr, cast.span)?;
                let source_ty = self.infer_value_expr(env, &cast.expr)?;
                self.validate_checked_cast(&cast.expr, &source_ty, &cast.ty, cast.span)?;
                Ok(cast.ty.clone())
            }
            Expr::Range(range) => {
                self.infer_value_expr(env, &range.start)?;
                self.infer_value_expr(env, &range.end)?;
                Ok(Type::Named("Range".to_string()))
            }
            Expr::StructInit(init) => {
                self.reject_direct_borrow_escape(expr, init.span)?;
                self.check_field_initializer(env, &init.ty, &init.fields, init.span, "struct literal")?;
                Ok(Type::Named(init.ty.clone()))
            }
            Expr::Match(match_expr) => {
                self.reject_direct_borrow_escape(expr, match_expr.span)?;
                let scrutinee_ty = self.infer_value_expr(env, &match_expr.expr)?;
                let payloads = self.check_match_patterns(&scrutinee_ty, match_expr)?;
                if self.is_linear_type(&scrutinee_ty) {
                    self.mark_expr_as_moved(env, &match_expr.expr)?;
                }
                let mut arm_ty = None;
                let mut arm_envs = Vec::with_capacity(match_expr.arms.len());
                for (arm, payload) in match_expr.arms.iter().zip(payloads) {
                    let mut arm_env = env.child();
                    for (binding, field_ty) in payload.bindings {
                        self.bind_pattern(&mut arm_env, &BindingPattern::Name(binding), &field_ty, false, arm.span)?;
                    }
                    let ty = self.infer_expr(&mut arm_env, &arm.value)?;
                    if expression_fails_closed(&arm.value) {
                        continue;
                    }
                    if arm_ty.as_ref().is_none_or(|existing| self.types_equal(existing, &ty)) {
                        arm_ty = Some(ty);
                    } else {
                        return Err(CompileError::new("match arms must have matching types", arm.span));
                    }
                    arm_env.check_linear_complete()?;
                    arm_envs.push(arm_env);
                }
                env.merge_match_linear_states(&arm_envs, match_expr.span)?;
                if match_expr.arms.is_empty() {
                    Err(CompileError::new("match expression must contain at least one arm", match_expr.span))
                } else {
                    Ok(arm_ty.unwrap_or(Type::Unit))
                }
            }
            Expr::RequireBlock(require_block) => {
                for expr in &require_block.expressions {
                    Self::validate_require_condition_is_pure(expr, "require block")?;
                    let ty = self.infer_expr(env, expr)?;
                    if !self.is_bool_type(&ty) {
                        return Err(CompileError::new("require block expressions must be boolean", expr_span(expr))
                            .with_code(crate::error::DiagnosticCode::E1004));
                    }
                }
                Ok(Type::Unit)
            }
            Expr::ReplaceRelation(relation) => {
                let before_ty = env.lookup(&relation.before).cloned().ok_or_else(|| {
                    CompileError::new(format!("replace: undefined predecessor binding '{}'", relation.before), relation.span)
                })?;
                let after_ty = env.lookup(&relation.after).cloned().ok_or_else(|| {
                    CompileError::new(format!("replace: undefined successor binding '{}'", relation.after), relation.span)
                })?;
                let (Type::Named(before_name), Type::Named(after_name)) = (&before_ty, &after_ty) else {
                    return Err(CompileError::new(
                        "replace relations need concrete resource bindings for both the predecessor and the successor",
                        relation.span,
                    ));
                };
                if before_name != after_name {
                    return Err(CompileError::new(
                        format!("replace relation type mismatch: predecessor has {before_name}, successor has {after_name}"),
                        relation.span,
                    ));
                }
                let schema_fields = self
                    .resolve_named_type_fields(before_name.as_str())
                    .map(|fields| fields.keys().cloned().collect::<Vec<_>>())
                    .ok_or_else(|| {
                        CompileError::new(
                            format!("replace relation predecessor '{before_name}' does not resolve to a concrete schema"),
                            relation.span,
                        )
                    })?;
                let mut covered: Vec<String> = Vec::new();
                let exhaustive;
                match &relation.data {
                    ReplaceDataTreatment::Fields(treatments) => {
                        exhaustive = true;
                        for treatment in treatments {
                            let (field, value) = match treatment {
                                ReplaceFieldTreatment::Same(field) => (field, None),
                                ReplaceFieldTreatment::Assign(field, value) => (field, Some(value)),
                            };
                            note_replace_field(&schema_fields, &mut covered, field, relation.span)?;
                            if let Some(value) = value {
                                check_replace_assigned_field(self, env, &before_ty, field, value, relation.span)?;
                            }
                        }
                    }
                    // `same except` expands against the concrete schema: every
                    // unlisted field is preserved, so only the listed
                    // assignments need to exist and type-check.
                    ReplaceDataTreatment::SameExcept(assigned) => {
                        exhaustive = false;
                        for (field, value) in assigned {
                            note_replace_field(&schema_fields, &mut covered, field, relation.span)?;
                            check_replace_assigned_field(self, env, &before_ty, field, value, relation.span)?;
                        }
                    }
                }
                if exhaustive && covered.len() != schema_fields.len() {
                    let missing =
                        schema_fields.iter().filter(|field| !covered.contains(field)).cloned().collect::<Vec<_>>().join(", ");
                    return Err(CompileError::new(
                        format!(
                            "replace relation data treatment must cover every field of resource '{before_name}' exactly once; missing {missing}"
                        ),
                        relation.span,
                    ));
                }
                env.consume(&relation.before).map_err(|error| CompileError::new(error.to_string(), relation.span))?;
                match &relation.lock {
                    ReplaceLockTreatment::Exact(lock) => {
                        let lock_ty = self.infer_expr(env, lock)?;
                        if lock_ty != Type::Address {
                            return Err(CompileError::new(
                                format!("replace relation `exact` lock expects an Address, found {}", type_repr(&lock_ty)),
                                relation.span,
                            ));
                        }
                    }
                    ReplaceLockTreatment::ExactHash(lock) => {
                        let lock_ty = self.infer_expr(env, lock)?;
                        if lock_ty != Type::Named(CKB_SCRIPT_HASH_TYPE.to_string()) {
                            return Err(CompileError::new(
                                format!(
                                    "replace relation `exact_hash` lock expects a ScriptHash, found {}; convert a trusted Hash explicitly with ckb::script_hash(hash)",
                                    type_repr(&lock_ty)
                                ),
                                relation.span,
                            ));
                        }
                    }
                    ReplaceLockTreatment::Same => {}
                }
                Ok(Type::Bool)
            }
            Expr::Preserve(preserve) => {
                let output_ty = env.lookup(&preserve.output_name).cloned().ok_or_else(|| {
                    CompileError::new(format!("preserve: undefined output binding '{}'", preserve.output_name), preserve.span)
                })?;
                let input_ty = env.lookup(&preserve.input_name).cloned().ok_or_else(|| {
                    CompileError::new(format!("preserve: undefined input binding '{}'", preserve.input_name), preserve.span)
                })?;
                if preserve.fields.is_empty() {
                    return Err(CompileError::new("preserve block must list at least one field", preserve.span));
                }
                for field in &preserve.fields {
                    let output_field_ty = self.lookup_field_type(&output_ty, field, preserve.span).map_err(|_| {
                        CompileError::new(format!("field '{}' does not exist on output type '{:?}'", field, output_ty), preserve.span)
                            .with_code(crate::error::DiagnosticCode::E1002)
                    })?;
                    let input_field_ty = self.lookup_field_type(&input_ty, field, preserve.span).map_err(|_| {
                        CompileError::new(format!("field '{}' does not exist on input type '{:?}'", field, input_ty), preserve.span)
                            .with_code(crate::error::DiagnosticCode::E1003)
                    })?;
                    if !self.types_equal(&output_field_ty, &input_field_ty) {
                        return Err(CompileError::new(
                            format!(
                                "preserve field '{}' type mismatch: output has {}, input has {}",
                                field,
                                type_repr(&output_field_ty),
                                type_repr(&input_field_ty)
                            ),
                            preserve.span,
                        )
                        .with_code(crate::error::DiagnosticCode::E1004));
                    }
                }
                Ok(Type::Bool)
            }
            Expr::StdlibCall(call) => self.infer_stdlib_call(env, call),
        }
    }
}
