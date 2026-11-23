use super::{BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrVar, LoweredExpr};
use crate::ast::{AssignExpr, AssignOp, BinaryOp, Expr};
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_assign_expr(
        &mut self,
        assign: &AssignExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        if let Some(lowered) = self.lower_local_projection_assign(assign, current, blocks, vars) {
            return lowered;
        }
        match assign.target.as_ref() {
            Expr::Identifier(name) => {
                let Some(target_var) = vars.get(name).cloned() else {
                    self.record_error(format!("assignment target '{}' is not bound in IR lowering", name), assign.span);
                    return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
                };
                let lowered_value = self.lower_expr_with_expected_type(&assign.value, &target_var.ty, current, blocks, vars);
                let Some(active) = lowered_value.current else {
                    return lowered_value;
                };
                match assign.op {
                    AssignOp::Assign => {
                        self.block_mut(blocks, active)
                            .instructions
                            .push(IrInstruction::Move { dest: target_var.clone(), src: lowered_value.operand });
                    }
                    AssignOp::AddAssign => {
                        let tmp = self.new_var("assign_tmp", target_var.ty.clone());
                        let block = self.block_mut(blocks, active);
                        block.instructions.push(IrInstruction::Binary {
                            dest: tmp.clone(),
                            op: BinaryOp::Add,
                            left: IrOperand::Var(target_var.clone()),
                            right: lowered_value.operand,
                        });
                        block.instructions.push(IrInstruction::Move { dest: target_var.clone(), src: IrOperand::Var(tmp) });
                    }
                }
                if self.is_mutable_value_aggregate(&target_var) {
                    // Cached projections describe the previous value. Keep
                    // already materialized snapshots, but re-read this local
                    // after assignment instead of aliasing its old fields.
                    self.invalidate_local_value_projections(target_var.id);
                }
                LoweredExpr { operand: IrOperand::Var(target_var), current: Some(active) }
            }
            Expr::FieldAccess(field) => {
                let lowered_value = self.lower_expr(&assign.value, current, blocks, vars);
                let Some(active) = lowered_value.current else {
                    return lowered_value;
                };
                self.lower_field_assign(field, assign.op, &assign.value, lowered_value.operand, active, blocks, vars)
            }
            Expr::Index(index) => {
                let lowered_value = self.lower_expr(&assign.value, current, blocks, vars);
                let Some(active) = lowered_value.current else {
                    return lowered_value;
                };
                self.lower_index_assign(index, assign.op, lowered_value.operand, active, blocks, vars)
            }
            _ => {
                self.record_error("invalid assignment target reached IR lowering", assign.span);
                LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
            }
        }
    }
}
