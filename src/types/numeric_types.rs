use super::{
    type_repr, TypeChecker, CKB_ABSOLUTE_BLOCK_SINCE_TYPE, CKB_ABSOLUTE_EPOCH_SINCE_TYPE, CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE,
    CKB_BLOCK_NUMBER_TYPE, CKB_DECODED_SINCE_TYPE, CKB_ENCODED_SINCE_TYPE, CKB_EPOCH_DURATION_TYPE, CKB_EPOCH_LENGTH_TYPE,
    CKB_EPOCH_NUMBER_TYPE, CKB_RELATIVE_BLOCK_SINCE_TYPE, CKB_RELATIVE_EPOCH_SINCE_TYPE, CKB_RELATIVE_TIMESTAMP_SINCE_TYPE,
    CKB_TIMESTAMP_MILLIS_TYPE,
};
use crate::ast::{Expr, Type};
use crate::error::{CompileError, Result, Span};

impl TypeChecker<'_> {
    pub(super) fn integer_literal_type_for_expected(value: u128, expected_ty: &Type, span: Span) -> Result<Option<Type>> {
        if !Self::is_integer_literal_target_type(expected_ty) {
            return Ok(None);
        }
        if Self::integer_literal_fits_expected_type(value, expected_ty) {
            Ok(Some(expected_ty.clone()))
        } else {
            Err(crate::error::SourceDiagnosticCode::IntegerLiteralOutOfRange
                .error(format!("integer literal {} does not fit expected type {}", value, type_repr(expected_ty)), span))
        }
    }

    fn is_integer_literal_target_type(ty: &Type) -> bool {
        match ty {
            Type::U8 | Type::U16 | Type::U32 | Type::I32 | Type::U64 | Type::U128 => true,
            Type::Named(name) => name == "usize" || name == "isize",
            _ => false,
        }
    }

    fn integer_literal_fits_expected_type(value: u128, expected_ty: &Type) -> bool {
        match expected_ty {
            Type::U8 => value <= u8::MAX as u128,
            Type::U16 => value <= u16::MAX as u128,
            Type::U32 => value <= u32::MAX as u128,
            Type::I32 => value <= i32::MAX as u128,
            Type::U64 => value <= u64::MAX as u128,
            Type::U128 => true,
            Type::Named(name) if name == "usize" => value <= u64::MAX as u128,
            Type::Named(name) if name == "isize" => value <= i64::MAX as u128,
            _ => false,
        }
    }

    fn unsigned_widening_rank(ty: &Type) -> Option<u8> {
        match ty {
            Type::U8 => Some(0),
            Type::U16 => Some(1),
            Type::U32 => Some(2),
            Type::U64 => Some(3),
            Type::U128 => Some(4),
            Type::Named(name) if name == "usize" => Some(3),
            _ => None,
        }
    }

    fn unsigned_type_for_rank(rank: u8) -> Type {
        match rank {
            0 => Type::U8,
            1 => Type::U16,
            2 => Type::U32,
            3 => Type::U64,
            _ => Type::U128,
        }
    }

    fn unsigned_widening_result_type(left_ty: &Type, right_ty: &Type) -> Option<Type> {
        let left_rank = Self::unsigned_widening_rank(left_ty)?;
        let right_rank = Self::unsigned_widening_rank(right_ty)?;
        Some(Self::unsigned_type_for_rank(left_rank.max(right_rank)))
    }

    pub(super) fn expr_type_compatible_with_expected(
        &self,
        expr: &Expr,
        actual_ty: &Type,
        expected_ty: &Type,
        span: Span,
    ) -> Result<bool> {
        self.ensure_expr_has_value(expr, actual_ty, span)?;
        if self.types_equal(actual_ty, expected_ty) {
            return Ok(true);
        }
        if let Expr::Integer(value) = expr {
            return Self::integer_literal_type_for_expected(*value, expected_ty, span).map(|ty| ty.is_some());
        }
        Ok(false)
    }

    pub(super) fn binary_operand_types_compatible(
        &self,
        left: &Expr,
        left_ty: &Type,
        right: &Expr,
        right_ty: &Type,
        span: Span,
    ) -> Result<bool> {
        if self.types_equal(left_ty, right_ty) {
            return Ok(true);
        }
        if let Expr::Integer(value) = left
            && Self::is_integer_literal_target_type(right_ty)
        {
            return Self::integer_literal_type_for_expected(*value, right_ty, span).map(|ty| ty.is_some());
        }
        if let Expr::Integer(value) = right
            && Self::is_integer_literal_target_type(left_ty)
        {
            return Self::integer_literal_type_for_expected(*value, left_ty, span).map(|ty| ty.is_some());
        }
        Ok(false)
    }

    pub(super) fn numeric_binary_result_type(
        &self,
        left: &Expr,
        left_ty: &Type,
        right: &Expr,
        right_ty: &Type,
        span: Span,
    ) -> Result<Type> {
        if self.types_equal(left_ty, right_ty) {
            return Ok(left_ty.clone());
        }
        if let Expr::Integer(value) = left
            && Self::is_integer_literal_target_type(right_ty)
        {
            Self::integer_literal_type_for_expected(*value, right_ty, span)?;
            return Ok(right_ty.clone());
        }
        if let Expr::Integer(value) = right
            && Self::is_integer_literal_target_type(left_ty)
        {
            Self::integer_literal_type_for_expected(*value, left_ty, span)?;
            return Ok(left_ty.clone());
        }
        if let Some(ty) = Self::unsigned_widening_result_type(left_ty, right_ty) {
            return Ok(ty);
        }
        Err(CompileError::new("arithmetic operations require matching numeric types", span))
    }

    pub(super) fn is_numeric_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::I32 | Type::U64 | Type::U128)
            || matches!(ty, Type::Named(name) if name == "usize" || name == "isize")
    }

    pub(super) fn is_ordered_ckb_temporal_type(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Named(name)
                if matches!(
                    name.as_str(),
                    CKB_EPOCH_NUMBER_TYPE
                        | CKB_EPOCH_DURATION_TYPE
                        | CKB_BLOCK_NUMBER_TYPE
                        | CKB_EPOCH_LENGTH_TYPE
                        | CKB_TIMESTAMP_MILLIS_TYPE
                        | CKB_ABSOLUTE_BLOCK_SINCE_TYPE
                        | CKB_ABSOLUTE_EPOCH_SINCE_TYPE
                        | CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE
                        | CKB_RELATIVE_BLOCK_SINCE_TYPE
                        | CKB_RELATIVE_EPOCH_SINCE_TYPE
                        | CKB_RELATIVE_TIMESTAMP_SINCE_TYPE
                )
        )
    }

    pub(super) fn is_ckb_since_type(ty: &Type) -> bool {
        matches!(
            ty,
            Type::Named(name)
                if name == CKB_ENCODED_SINCE_TYPE
                    || name == CKB_DECODED_SINCE_TYPE
                    || name == CKB_ABSOLUTE_BLOCK_SINCE_TYPE
                    || name == CKB_ABSOLUTE_EPOCH_SINCE_TYPE
                    || name == CKB_ABSOLUTE_TIMESTAMP_SINCE_TYPE
                    || name == CKB_RELATIVE_BLOCK_SINCE_TYPE
                    || name == CKB_RELATIVE_EPOCH_SINCE_TYPE
                    || name == CKB_RELATIVE_TIMESTAMP_SINCE_TYPE
                    || name.starts_with("Since<Absolute, ")
                    || name.starts_with("Since<Relative, ")
        )
    }

    pub(super) fn is_unsigned_shift_count_type(ty: &Type) -> bool {
        matches!(ty, Type::U8 | Type::U16 | Type::U32 | Type::U64) || matches!(ty, Type::Named(name) if name == "usize")
    }

    pub(super) fn integer_type_width_bits(ty: &Type) -> Option<u16> {
        match ty {
            Type::U8 => Some(8),
            Type::U16 => Some(16),
            Type::U32 | Type::I32 => Some(32),
            Type::U64 | Type::Named(_) => Some(64),
            Type::U128 => Some(128),
            _ => None,
        }
    }

    pub(super) fn validate_checked_cast(&self, expr: &Expr, source: &Type, target: &Type, span: Span) -> Result<()> {
        if self.types_equal(source, target) {
            return Ok(());
        }

        if let Expr::Integer(value) = expr
            && Self::integer_literal_fits_expected_type(*value, target)
        {
            return Ok(());
        }

        if self.is_numeric_type(source) && self.is_numeric_type(target) {
            let source_signed = matches!(source, Type::I32) || matches!(source, Type::Named(name) if name == "isize");
            let target_signed = matches!(target, Type::I32) || matches!(target, Type::Named(name) if name == "isize");
            if source_signed || target_signed {
                return Err(CompileError::new(
                    format!(
                        "unsupported signed numeric cast from {} to {}; 0.22 casts must preserve signedness or use a checked conversion",
                        type_repr(source),
                        type_repr(target)
                    ),
                    span,
                ));
            }
            if matches!(source, Type::U128) && !matches!(target, Type::U128) {
                return Err(CompileError::new(
                    format!(
                        "unsupported full-width narrowing cast from {} to {}; use an explicit checked conversion once u128 narrowing is available",
                        type_repr(source),
                        type_repr(target)
                    ),
                    span,
                ));
            }
            return Ok(());
        }

        Err(CompileError::new(
            format!(
                "unsupported cast from {} to {}; casts cannot erase Cell identity, linear ownership, reference, or capability boundaries",
                type_repr(source),
                type_repr(target)
            ),
            span,
        ))
    }

    pub(super) fn is_bool_type(&self, ty: &Type) -> bool {
        matches!(ty, Type::Bool)
    }
}
