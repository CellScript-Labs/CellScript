use super::{
    checked_cast_maximum, unsigned_ir_bits, BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrTerminator, IrType,
    IrVar, LoweredExpr,
};
use crate::ast::{BinaryOp, CastExpr, Expr, UnaryOp};
use crate::runtime_errors::CellScriptRuntimeError;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn const_type(&self, value: &IrConst) -> IrType {
        match value {
            IrConst::U8(_) => IrType::U8,
            IrConst::U16(_) => IrType::U16,
            IrConst::U32(_) => IrType::U32,
            IrConst::U64(_) => IrType::U64,
            IrConst::U128(_) => IrType::U128,
            IrConst::Unit => IrType::Unit,
            IrConst::Bool(_) => IrType::Bool,
            IrConst::Address(_) => IrType::Address,
            IrConst::Hash(_) => IrType::Hash,
            IrConst::Array(items) => IrType::Array(Box::new(IrType::U8), items.len()),
        }
    }

    pub(super) fn default_integer_const(value: u128) -> IrConst {
        u64::try_from(value).map(IrConst::U64).unwrap_or(IrConst::U128(value))
    }

    pub(super) fn integer_const_for_expected_type(value: u128, expected_ty: &IrType) -> Option<IrConst> {
        match expected_ty {
            IrType::U8 => u8::try_from(value).ok().map(IrConst::U8),
            IrType::U16 => u16::try_from(value).ok().map(IrConst::U16),
            IrType::U32 => u32::try_from(value).ok().map(IrConst::U32),
            IrType::I32 => i32::try_from(value).ok().map(|value| IrConst::U32(value as u32)),
            IrType::U64 => u64::try_from(value).ok().map(IrConst::U64),
            IrType::U128 => Some(IrConst::U128(value)),
            IrType::Named(name) if name == "usize" => u64::try_from(value).ok().map(IrConst::U64),
            IrType::Named(name) if name == "isize" => {
                i64::try_from(value).ok().and_then(|_| u64::try_from(value).ok()).map(IrConst::U64)
            }
            _ => None,
        }
    }

    pub(super) fn integer_operand_for_expected_type(
        &mut self,
        value: u128,
        expected_ty: &IrType,
        block: &mut IrBlock,
    ) -> Option<IrOperand> {
        let value = IrOperand::Const(Self::integer_const_for_expected_type(value, expected_ty)?);
        // The constant encoding has no signed variant. Preserve the contextual
        // i32 type in a LoadConst destination before assignment, arithmetic or
        // helper transport can mistake its U32 encoding for an unsigned value.
        Some(if *expected_ty == IrType::I32 {
            IrOperand::Var(self.materialize_operand_with_type("i32_literal", value, IrType::I32, block))
        } else {
            value
        })
    }

    pub(super) fn binary_result_type_for_operands(&self, op: BinaryOp, left: &IrOperand, right: &IrOperand) -> IrType {
        match op {
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => IrType::Bool,
            BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Mod
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor => {
                let left_ty = self.operand_type(left);
                let right_ty = self.operand_type(right);
                if left_ty == right_ty {
                    left_ty
                } else if let Some(ty) = Self::unsigned_widening_result_ir_type(&left_ty, &right_ty) {
                    ty
                } else {
                    IrType::U64
                }
            }
            BinaryOp::And | BinaryOp::Or => IrType::Bool,
            BinaryOp::Shl | BinaryOp::Shr => self.operand_type(left),
        }
    }

    fn unsigned_widening_rank_ir(ty: &IrType) -> Option<u8> {
        match ty {
            IrType::U8 => Some(0),
            IrType::U16 => Some(1),
            IrType::U32 => Some(2),
            IrType::U64 => Some(3),
            IrType::U128 => Some(4),
            IrType::Named(name) if name == "usize" => Some(3),
            _ => None,
        }
    }

    fn unsigned_ir_type_for_rank(rank: u8) -> IrType {
        match rank {
            0 => IrType::U8,
            1 => IrType::U16,
            2 => IrType::U32,
            3 => IrType::U64,
            _ => IrType::U128,
        }
    }

    fn unsigned_widening_result_ir_type(left_ty: &IrType, right_ty: &IrType) -> Option<IrType> {
        let left_rank = Self::unsigned_widening_rank_ir(left_ty)?;
        let right_rank = Self::unsigned_widening_rank_ir(right_ty)?;
        Some(Self::unsigned_ir_type_for_rank(left_rank.max(right_rank)))
    }

    pub(super) fn unary_result_type(&self, op: UnaryOp, operand: &IrOperand) -> Option<IrType> {
        let operand_type = match operand {
            IrOperand::Var(var) => var.ty.clone(),
            IrOperand::Const(value) => self.const_type(value),
        };
        Some(match op {
            UnaryOp::Not => IrType::Bool,
            UnaryOp::Neg => operand_type,
            UnaryOp::Ref => IrType::Ref(Box::new(operand_type)),
            UnaryOp::Deref => match operand_type {
                IrType::Ref(inner) | IrType::MutRef(inner) => *inner,
                _ => return None,
            },
        })
    }

    pub(super) fn lower_cast_expr(
        &mut self,
        cast: &CastExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        if let Expr::Integer(value) = cast.expr.as_ref() {
            let target = self.convert_type(&cast.ty);
            if let Some(operand) = self.integer_operand_for_expected_type(*value, &target, self.block_mut(blocks, current)) {
                return LoweredExpr { operand, current: Some(current) };
            }
        }

        let lowered = self.lower_expr(&cast.expr, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        let source_ty = self.operand_type(&lowered.operand);
        let target_ty = self.convert_type(&cast.ty);
        // Widening must retain the target type in typed IR even when both
        // values fit one register. u128 additionally changes representation
        // from a scalar to two stored limbs. Neither conversion needs a
        // range guard, but erasing it gives later operations the wrong type.
        if unsigned_ir_bits(&source_ty).zip(unsigned_ir_bits(&target_ty)).is_some_and(|(source, target)| target > source) {
            let cast_value = self.new_var("cast_value", target_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: cast_value.clone(), src: lowered.operand });
            return LoweredExpr { operand: IrOperand::Var(cast_value), current: Some(active) };
        }
        let Some(maximum) = checked_cast_maximum(&source_ty, &target_ty) else {
            return LoweredExpr { operand: lowered.operand, current: Some(active) };
        };

        let condition = self.new_var("cast_fits", IrType::Bool);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Binary {
            dest: condition.clone(),
            op: BinaryOp::Le,
            left: lowered.operand.clone(),
            right: IrOperand::Const(maximum),
        });

        let ok_block = self.push_block(blocks);
        let fail_block = self.push_block(blocks);
        self.block_mut(blocks, active).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(condition), then_block: ok_block, else_block: fail_block };
        self.block_mut(blocks, fail_block).terminator =
            IrTerminator::Return(Some(self.fail_closed_runtime_return_operand(CellScriptRuntimeError::NumericOrDiscriminantInvalid)));
        self.block_mut(blocks, fail_block).runtime_error = Some(CellScriptRuntimeError::NumericOrDiscriminantInvalid);

        let cast_value = self.new_var("cast_value", target_ty);
        self.block_mut(blocks, ok_block).instructions.push(IrInstruction::Move { dest: cast_value.clone(), src: lowered.operand });
        LoweredExpr { operand: IrOperand::Var(cast_value), current: Some(ok_block) }
    }
}
