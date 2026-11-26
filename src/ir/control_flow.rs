use super::{
    stmts_contain_loop_control, BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrTerminator, IrType, IrVar,
    LoopTarget, LoweredExpr,
};
use crate::ast::{BinaryOp, Expr, ForStmt, IfStmt, RangeExpr, UnaryOp, WhileStmt};
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_if_stmt(
        &mut self,
        if_stmt: &IfStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
        tail_expr_returns: bool,
    ) -> Option<BlockId> {
        let lowered_cond = self.lower_expr(&if_stmt.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let current = lowered_cond.current?;

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        self.enter_branch_scope();
        let then_exit = self.lower_stmts(&if_stmt.then_branch, then_block, blocks, &mut then_vars, return_type, tail_expr_returns);
        self.leave_branch_scope();

        let mut else_vars = vars.clone();
        self.enter_branch_scope();
        let else_exit = if let Some(else_branch) = &if_stmt.else_branch {
            let exit = self.lower_stmts(else_branch, else_block, blocks, &mut else_vars, return_type, tail_expr_returns);
            self.leave_branch_scope();
            exit
        } else {
            self.leave_branch_scope();
            Some(else_block)
        };

        if then_exit.is_none() && else_exit.is_none() {
            return None;
        }

        let join = self.push_block(blocks);
        if let Some(exit) = then_exit {
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }
        if let Some(exit) = else_exit {
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(join);
        }
        Some(join)
    }

    pub(super) fn lower_if_stmt_value(
        &mut self,
        if_stmt: &IfStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&if_stmt.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let Some(current) = lowered_cond.current else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
        };
        let Some(else_branch) = &if_stmt.else_branch else {
            let next = self.lower_if_stmt(if_stmt, current, blocks, vars, None, false);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: next };
        };

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        self.enter_branch_scope();
        let then_lowered = self.lower_tail_block_value(&if_stmt.then_branch, then_block, blocks, &mut then_vars);
        self.leave_branch_scope();
        let mut else_vars = vars.clone();
        self.enter_branch_scope();
        let else_lowered = self.lower_tail_block_value(else_branch, else_block, blocks, &mut else_vars);
        self.leave_branch_scope();

        if then_lowered.current.is_none() && else_lowered.current.is_none() {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
        }

        let result_ty = match (then_lowered.current.is_some(), else_lowered.current.is_some()) {
            (true, _) => self.operand_type(&then_lowered.operand),
            (false, true) => self.operand_type(&else_lowered.operand),
            (false, false) => return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None },
        };
        let dest = self.new_var("if_tmp", result_ty);
        let join = self.push_block(blocks);

        if let Some(exit) = then_lowered.current {
            let block = self.block_mut(blocks, exit);
            block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: then_lowered.operand });
            block.terminator = IrTerminator::Jump(join);
        }

        if let Some(exit) = else_lowered.current {
            let block = self.block_mut(blocks, exit);
            block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: else_lowered.operand });
            block.terminator = IrTerminator::Jump(join);
        }

        LoweredExpr { operand: IrOperand::Var(dest), current: Some(join) }
    }

    pub(super) fn lower_if_stmt_value_with_expected_type(
        &mut self,
        if_stmt: &IfStmt,
        expected_ty: &IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&if_stmt.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let Some(current) = lowered_cond.current else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
        };
        let Some(else_branch) = &if_stmt.else_branch else {
            let next = self.lower_if_stmt(if_stmt, current, blocks, vars, None, false);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: next };
        };

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        self.enter_branch_scope();
        let then_lowered =
            self.lower_tail_block_value_with_expected_type(&if_stmt.then_branch, expected_ty, then_block, blocks, &mut then_vars);
        self.leave_branch_scope();
        let mut else_vars = vars.clone();
        self.enter_branch_scope();
        let else_lowered =
            self.lower_tail_block_value_with_expected_type(else_branch, expected_ty, else_block, blocks, &mut else_vars);
        self.leave_branch_scope();

        if then_lowered.current.is_none() && else_lowered.current.is_none() {
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: None };
        }

        let dest = self.new_var("if_tmp", expected_ty.clone());
        let join = self.push_block(blocks);

        if let Some(exit) = then_lowered.current {
            let block = self.block_mut(blocks, exit);
            block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: then_lowered.operand });
            block.terminator = IrTerminator::Jump(join);
        }

        if let Some(exit) = else_lowered.current {
            let block = self.block_mut(blocks, exit);
            block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: else_lowered.operand });
            block.terminator = IrTerminator::Jump(join);
        }

        LoweredExpr { operand: IrOperand::Var(dest), current: Some(join) }
    }

    pub(super) fn lower_while_stmt(
        &mut self,
        while_stmt: &WhileStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        let cond_entry = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(cond_entry);

        let lowered_cond = self.lower_expr(&while_stmt.condition, cond_entry, blocks, vars);
        let cond = lowered_cond.operand;
        let cond_exit = lowered_cond.current?;

        let body_block = self.push_block(blocks);
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, cond_exit).terminator = IrTerminator::Branch { cond, then_block: body_block, else_block: exit_block };

        let mut body_vars = vars.clone();
        self.enter_branch_scope();
        self.loop_targets.push(LoopTarget { label: while_stmt.label.clone(), break_block: exit_block, continue_block: cond_entry });
        let body_exit = self.lower_stmts(&while_stmt.body, body_block, blocks, &mut body_vars, return_type, false);
        self.loop_targets.pop();
        self.leave_branch_scope();
        if let Some(exit) = body_exit {
            self.block_mut(blocks, exit).terminator = IrTerminator::Jump(cond_entry);
        }

        Some(exit_block)
    }

    pub(super) fn lower_for_stmt(
        &mut self,
        for_stmt: &ForStmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        if let Some(lowered) = self.lower_allocated_for(for_stmt, current, blocks, vars, return_type) {
            return lowered;
        }
        match &for_stmt.iterable {
            Expr::Range(range) => self.lower_for_range_stmt(for_stmt, range, current, blocks, vars, return_type),
            _ => {
                let lowered = self.lower_expr(&for_stmt.iterable, current, blocks, vars);
                let active = lowered.current?;
                self.lower_for_iterable_stmt(for_stmt, lowered.operand, active, blocks, vars, return_type)
            }
        }
    }

    // Borrowed iteration has the same one-layer reference type as the checked
    // source. Index produces the element value; the explicit conversion keeps
    // that distinction visible to IR validation and the independent checker.
    fn borrow_iteration_item(&mut self, value: IrVar, borrowed: bool, block: BlockId, blocks: &mut [IrBlock]) -> IrVar {
        if !borrowed {
            return value;
        }
        let reference = self.new_var("iter_ref", IrType::Ref(Box::new(value.ty.clone())));
        self.block_mut(blocks, block).instructions.push(IrInstruction::Unary {
            dest: reference.clone(),
            op: UnaryOp::Ref,
            operand: IrOperand::Var(value.clone()),
        });
        self.copy_aggregate_metadata(&IrOperand::Var(value), reference.id);
        reference
    }

    fn lower_for_iterable_stmt(
        &mut self,
        for_stmt: &ForStmt,
        iterable: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        if let IrOperand::Var(iterable_var) = &iterable {
            if !self.is_mutable_value_aggregate(iterable_var)
                && let Some(elements) = self.aggregate_elements.get(&iterable_var.id).cloned()
            {
                let borrowed = matches!(&iterable_var.ty, IrType::Ref(_));
                return self.lower_for_local_fixed_array_stmt(for_stmt, elements, borrowed, current, blocks, vars, return_type);
            }
            if let IrType::Array(_, len) = &iterable_var.ty {
                let len = *len;
                return self.lower_for_static_array_pointer_stmt(for_stmt, iterable, len, current, blocks, vars, return_type);
            }
        }

        let Some(item_ty) = self.iter_item_type(&iterable) else {
            self.record_error("for-loop iterable has no lowered item type", for_stmt.span);
            return Some(current);
        };

        let borrowed = matches!(&iterable, IrOperand::Var(var) if matches!(var.ty, IrType::Ref(_)));
        let index_var = self.new_var("iter_index", IrType::U64);
        let length_var = self.new_var("iter_len", IrType::U64);
        {
            let block = self.block_mut(blocks, current);
            block.instructions.push(IrInstruction::Move { dest: index_var.clone(), src: IrOperand::Const(IrConst::U64(0)) });
            block.instructions.push(IrInstruction::Length { dest: length_var.clone(), operand: iterable.clone() });
        }

        let cond_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(cond_block);

        let cond_var = self.new_var("iter_cond", IrType::Bool);
        {
            let block = self.block_mut(blocks, cond_block);
            block.instructions.push(IrInstruction::Binary {
                dest: cond_var.clone(),
                op: BinaryOp::Lt,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Var(length_var.clone()),
            });
        }

        let body_block = self.push_block(blocks);
        let has_loop_control = stmts_contain_loop_control(&for_stmt.body);
        let step_block = has_loop_control.then(|| self.push_block(blocks));
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, cond_block).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(cond_var), then_block: body_block, else_block: exit_block };

        let item_var = self.new_var("iter_item", item_ty);
        self.block_mut(blocks, body_block).instructions.push(IrInstruction::Index {
            dest: item_var.clone(),
            arr: iterable,
            idx: IrOperand::Var(index_var.clone()),
        });

        let item_var = self.borrow_iteration_item(item_var, borrowed, body_block, blocks);
        let mut body_vars = vars.clone();
        self.enter_branch_scope();
        self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, body_block), &mut body_vars);
        self.record_source_pattern(&for_stmt.pattern, for_stmt.span, &body_vars);
        if let Some(step_block) = step_block {
            self.loop_targets.push(LoopTarget { label: for_stmt.label.clone(), break_block: exit_block, continue_block: step_block });
        }
        let body_exit = self.lower_stmts(&for_stmt.body, body_block, blocks, &mut body_vars, return_type, false);
        self.leave_branch_scope();
        if step_block.is_some() {
            self.loop_targets.pop();
        }
        if let Some(exit) = body_exit {
            let next_index = self.new_var("iter_next", IrType::U64);
            let destination = step_block.unwrap_or(exit);
            if destination != exit {
                self.block_mut(blocks, exit).terminator = IrTerminator::Jump(destination);
            }
            let block = self.block_mut(blocks, destination);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var, src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(cond_block);
        } else if let Some(step_block) = step_block {
            let next_index = self.new_var("iter_next", IrType::U64);
            let block = self.block_mut(blocks, step_block);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var, src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(cond_block);
        }

        Some(exit_block)
    }

    fn lower_for_static_array_pointer_stmt(
        &mut self,
        for_stmt: &ForStmt,
        iterable: IrOperand,
        len: usize,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        let Some(item_ty) = self.iter_item_type(&iterable) else {
            self.record_error("for-loop iterable has no lowered item type", for_stmt.span);
            return Some(current);
        };

        let borrowed = matches!(&iterable, IrOperand::Var(var) if matches!(var.ty, IrType::Ref(_)));
        if !stmts_contain_loop_control(&for_stmt.body) {
            let mut current = current;
            for index in 0..len {
                let item_var = self.new_var(format!("iter_item_{}", index), item_ty.clone());
                self.block_mut(blocks, current).instructions.push(IrInstruction::Index {
                    dest: item_var.clone(),
                    arr: iterable.clone(),
                    idx: IrOperand::Const(IrConst::U64(index as u64)),
                });

                let item_var = self.borrow_iteration_item(item_var, borrowed, current, blocks);
                let mut body_vars = vars.clone();
                self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, current), &mut body_vars);
                self.record_source_pattern(&for_stmt.pattern, for_stmt.span, &body_vars);
                current = self.lower_stmts(&for_stmt.body, current, blocks, &mut body_vars, return_type, false)?;
            }
            return Some(current);
        }

        let exit_block = self.push_block(blocks);
        if len == 0 {
            self.block_mut(blocks, current).terminator = IrTerminator::Jump(exit_block);
            return Some(exit_block);
        }
        let iteration_blocks = (0..len).map(|_| self.push_block(blocks)).collect::<Vec<_>>();
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(iteration_blocks[0]);
        for index in 0..len {
            let iteration_block = iteration_blocks[index];
            let next_block = iteration_blocks.get(index + 1).copied().unwrap_or(exit_block);
            let item_var = self.new_var(format!("iter_item_{}", index), item_ty.clone());
            self.block_mut(blocks, iteration_block).instructions.push(IrInstruction::Index {
                dest: item_var.clone(),
                arr: iterable.clone(),
                idx: IrOperand::Const(IrConst::U64(index as u64)),
            });

            let item_var = self.borrow_iteration_item(item_var, borrowed, iteration_block, blocks);
            let mut body_vars = vars.clone();
            self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, iteration_block), &mut body_vars);
            self.record_source_pattern(&for_stmt.pattern, for_stmt.span, &body_vars);
            self.loop_targets.push(LoopTarget { label: for_stmt.label.clone(), break_block: exit_block, continue_block: next_block });
            let body_exit = self.lower_stmts(&for_stmt.body, iteration_block, blocks, &mut body_vars, return_type, false);
            self.loop_targets.pop();
            if let Some(body_exit) = body_exit {
                self.block_mut(blocks, body_exit).terminator = IrTerminator::Jump(next_block);
            }
        }

        Some(exit_block)
    }

    fn lower_for_local_fixed_array_stmt(
        &mut self,
        for_stmt: &ForStmt,
        elements: Vec<IrVar>,
        borrowed: bool,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        if !stmts_contain_loop_control(&for_stmt.body) {
            let mut current = current;
            for (index, element_var) in elements.into_iter().enumerate() {
                let item_var = self.new_var(format!("iter_item_{}", index), element_var.ty.clone());
                self.block_mut(blocks, current)
                    .instructions
                    .push(IrInstruction::Move { dest: item_var.clone(), src: IrOperand::Var(element_var.clone()) });
                self.copy_aggregate_metadata(&IrOperand::Var(element_var), item_var.id);

                let item_var = self.borrow_iteration_item(item_var, borrowed, current, blocks);
                let mut body_vars = vars.clone();
                self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, current), &mut body_vars);
                self.record_source_pattern(&for_stmt.pattern, for_stmt.span, &body_vars);
                current = self.lower_stmts(&for_stmt.body, current, blocks, &mut body_vars, return_type, false)?;
            }
            return Some(current);
        }

        let exit_block = self.push_block(blocks);
        if elements.is_empty() {
            self.block_mut(blocks, current).terminator = IrTerminator::Jump(exit_block);
            return Some(exit_block);
        }
        let iteration_blocks = (0..elements.len()).map(|_| self.push_block(blocks)).collect::<Vec<_>>();
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(iteration_blocks[0]);
        for (index, element_var) in elements.into_iter().enumerate() {
            let iteration_block = iteration_blocks[index];
            let next_block = iteration_blocks.get(index + 1).copied().unwrap_or(exit_block);
            let item_var = self.new_var(format!("iter_item_{}", index), element_var.ty.clone());
            self.block_mut(blocks, iteration_block)
                .instructions
                .push(IrInstruction::Move { dest: item_var.clone(), src: IrOperand::Var(element_var.clone()) });
            self.copy_aggregate_metadata(&IrOperand::Var(element_var), item_var.id);

            let item_var = self.borrow_iteration_item(item_var, borrowed, iteration_block, blocks);
            let mut body_vars = vars.clone();
            self.bind_pattern(&for_stmt.pattern, IrOperand::Var(item_var), self.block_mut(blocks, iteration_block), &mut body_vars);
            self.record_source_pattern(&for_stmt.pattern, for_stmt.span, &body_vars);
            self.loop_targets.push(LoopTarget { label: for_stmt.label.clone(), break_block: exit_block, continue_block: next_block });
            let body_exit = self.lower_stmts(&for_stmt.body, iteration_block, blocks, &mut body_vars, return_type, false);
            self.loop_targets.pop();
            if let Some(body_exit) = body_exit {
                self.block_mut(blocks, body_exit).terminator = IrTerminator::Jump(next_block);
            }
        }

        Some(exit_block)
    }

    fn lower_for_range_stmt(
        &mut self,
        for_stmt: &ForStmt,
        range: &RangeExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        let lowered_start = self.lower_expr(&range.start, current, blocks, vars);
        let start = lowered_start.operand;
        let active = lowered_start.current?;

        let lowered_end = self.lower_expr(&range.end, active, blocks, vars);
        let end = lowered_end.operand;
        let active = lowered_end.current?;

        let index_var = self.new_var("for_index", IrType::U64);
        let end_var = self.new_var("for_end", IrType::U64);
        {
            let block = self.block_mut(blocks, active);
            block.instructions.push(IrInstruction::Move { dest: index_var.clone(), src: start });
            block.instructions.push(IrInstruction::Move { dest: end_var.clone(), src: end });
        }

        let cond_block = self.push_block(blocks);
        self.block_mut(blocks, active).terminator = IrTerminator::Jump(cond_block);

        let cond_var = self.new_var("for_cond", IrType::Bool);
        {
            let block = self.block_mut(blocks, cond_block);
            block.instructions.push(IrInstruction::Binary {
                dest: cond_var.clone(),
                op: BinaryOp::Lt,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Var(end_var.clone()),
            });
        }

        let body_block = self.push_block(blocks);
        let has_loop_control = stmts_contain_loop_control(&for_stmt.body);
        let step_block = has_loop_control.then(|| self.push_block(blocks));
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, cond_block).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(cond_var), then_block: body_block, else_block: exit_block };

        let mut body_vars = vars.clone();
        self.bind_pattern(&for_stmt.pattern, IrOperand::Var(index_var.clone()), self.block_mut(blocks, body_block), &mut body_vars);
        self.record_source_pattern(&for_stmt.pattern, for_stmt.span, &body_vars);
        if let Some(step_block) = step_block {
            self.loop_targets.push(LoopTarget { label: for_stmt.label.clone(), break_block: exit_block, continue_block: step_block });
        }
        let body_exit = self.lower_stmts(&for_stmt.body, body_block, blocks, &mut body_vars, return_type, false);
        if step_block.is_some() {
            self.loop_targets.pop();
        }
        if let Some(exit) = body_exit {
            let next_index = self.new_var("for_next", IrType::U64);
            let destination = step_block.unwrap_or(exit);
            if destination != exit {
                self.block_mut(blocks, exit).terminator = IrTerminator::Jump(destination);
            }
            let block = self.block_mut(blocks, destination);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var, src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(cond_block);
        } else if let Some(step_block) = step_block {
            let next_index = self.new_var("for_next", IrType::U64);
            let block = self.block_mut(blocks, step_block);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var, src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(cond_block);
        }

        Some(exit_block)
    }
}
