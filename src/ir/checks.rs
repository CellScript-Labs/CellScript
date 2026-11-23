use super::{BlockId, IrBlock, IrConst, IrEnforcedClaim, IrGenerator, IrOperand, IrTerminator, IrVar, LoweredExpr};
use crate::ast::{AssertExpr, RequireExpr};
use crate::error::Span;
use crate::runtime_errors::CellScriptRuntimeError;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_assert_expr(
        &mut self,
        assert_expr: &AssertExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&assert_expr.condition, current, blocks, vars);
        let Some(active) = lowered_cond.current else {
            return lowered_cond;
        };
        let cond = lowered_cond.operand;

        let ok_block = self.push_block(blocks);
        let fail_block = self.push_block(blocks);
        self.block_mut(blocks, active).terminator = IrTerminator::Branch { cond, then_block: ok_block, else_block: fail_block };
        self.block_mut(blocks, fail_block).terminator = IrTerminator::Return(Some(self.fail_closed_return_operand()));
        self.block_mut(blocks, fail_block).runtime_error = Some(CellScriptRuntimeError::AssertionFailed);

        LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(ok_block) }
    }

    pub(super) fn lower_require_expr(
        &mut self,
        require_expr: &RequireExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&require_expr.condition, current, blocks, vars);
        let Some(active) = lowered_cond.current else {
            return lowered_cond;
        };
        let cond = lowered_cond.operand;

        let ok_block = self.push_block(blocks);
        let fail_block = self.push_block(blocks);
        self.block_mut(blocks, active).terminator =
            IrTerminator::Branch { cond: cond.clone(), then_block: ok_block, else_block: fail_block };
        self.block_mut(blocks, fail_block).terminator = IrTerminator::Return(Some(self.fail_closed_return_operand()));
        self.block_mut(blocks, fail_block).runtime_error = Some(CellScriptRuntimeError::AssertionFailed);
        let condition_span = require_expr.condition.span();
        let claim_span = if condition_span.end > require_expr.span.start {
            Span::new(require_expr.span.start, condition_span.end, require_expr.span.line, require_expr.span.column)
        } else {
            require_expr.span
        };
        self.enforced_claims.push(IrEnforcedClaim {
            statement: crate::fmt::format_expression(&require_expr.condition),
            condition_block: active,
            success_block: ok_block,
            failure_block: fail_block,
            span: claim_span,
        });

        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(ok_block) }
    }

    pub(super) fn fail_closed_return_operand(&self) -> IrOperand {
        self.fail_closed_runtime_return_operand(CellScriptRuntimeError::AssertionFailed)
    }

    pub(super) fn fail_closed_runtime_return_operand(&self, error: CellScriptRuntimeError) -> IrOperand {
        if self.lowering_lock_entry {
            IrOperand::Const(IrConst::Bool(false))
        } else {
            IrOperand::Const(IrConst::U64(error.code()))
        }
    }
}
