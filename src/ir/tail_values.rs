use super::{BlockId, IrBlock, IrConst, IrGenerator, IrOperand, IrType, IrVar, LoweredExpr};
use crate::ast::Stmt;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_tail_block_value(
        &mut self,
        stmts: &[Stmt],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let Some((last, prefix)) = stmts.split_last() else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) };
        };

        let mut active = current;
        for stmt in prefix {
            let Some(next) = self.lower_stmt(stmt, active, blocks, vars, None) else {
                return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
            };
            active = next;
        }

        match last {
            Stmt::Expr(expr) => self.lower_expr(expr, active, blocks, vars),
            Stmt::If(if_stmt) if if_stmt.else_branch.is_some() => self.lower_if_stmt_value(if_stmt, active, blocks, vars),
            stmt => {
                let next = self.lower_stmt(stmt, active, blocks, vars, None);
                LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: next }
            }
        }
    }

    pub(super) fn lower_tail_block_value_with_expected_type(
        &mut self,
        stmts: &[Stmt],
        expected_ty: &IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let Some((last, prefix)) = stmts.split_last() else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) };
        };

        let mut active = current;
        for stmt in prefix {
            let Some(next) = self.lower_stmt(stmt, active, blocks, vars, None) else {
                return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
            };
            active = next;
        }

        match last {
            Stmt::Expr(expr) => self.lower_expr_with_expected_type(expr, expected_ty, active, blocks, vars),
            Stmt::If(if_stmt) if if_stmt.else_branch.is_some() => {
                self.lower_if_stmt_value_with_expected_type(if_stmt, expected_ty, active, blocks, vars)
            }
            stmt => {
                let next = self.lower_stmt(stmt, active, blocks, vars, None);
                LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: next }
            }
        }
    }
}
