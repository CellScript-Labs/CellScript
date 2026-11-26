use super::{
    ir_type_display, BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrTerminator, IrType, IrVar, LoweredExpr,
};
use crate::ast::{BinaryOp, IfExpr, MatchExpr, MatchPattern};
use crate::error::Span;
use crate::runtime_errors::CellScriptRuntimeError;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_if_expr(
        &mut self,
        if_expr: &IfExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&if_expr.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let Some(current) = lowered_cond.current else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        self.enter_branch_scope();
        let then_lowered = self.lower_expr(&if_expr.then_branch, then_block, blocks, &mut then_vars);
        self.leave_branch_scope();
        let mut else_vars = vars.clone();
        self.enter_branch_scope();
        let else_lowered = self.lower_expr(&if_expr.else_branch, else_block, blocks, &mut else_vars);

        self.leave_branch_scope();
        if then_lowered.current.is_none() && else_lowered.current.is_none() {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        }

        let result_ty = match (then_lowered.current.is_some(), else_lowered.current.is_some()) {
            (true, _) => self.operand_type(&then_lowered.operand),
            (false, true) => self.operand_type(&else_lowered.operand),
            (false, false) => return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None },
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

    pub(super) fn lower_if_expr_with_expected_type(
        &mut self,
        if_expr: &IfExpr,
        expected_ty: IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_cond = self.lower_expr(&if_expr.condition, current, blocks, vars);
        let cond = lowered_cond.operand;
        let Some(current) = lowered_cond.current else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        let then_block = self.push_block(blocks);
        let else_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Branch { cond, then_block, else_block };

        let mut then_vars = vars.clone();
        self.enter_branch_scope();
        let then_lowered = self.lower_expr_with_expected_type(&if_expr.then_branch, &expected_ty, then_block, blocks, &mut then_vars);
        self.leave_branch_scope();
        let mut else_vars = vars.clone();
        self.enter_branch_scope();
        let else_lowered = self.lower_expr_with_expected_type(&if_expr.else_branch, &expected_ty, else_block, blocks, &mut else_vars);

        self.leave_branch_scope();
        if then_lowered.current.is_none() && else_lowered.current.is_none() {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        }

        let dest = self.new_var("if_tmp", expected_ty);
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

    pub(super) fn lower_match_expr(
        &mut self,
        match_expr: &MatchExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_scrutinee = self.lower_expr(&match_expr.expr, current, blocks, vars);
        let Some(mut check_block) = lowered_scrutinee.current else {
            return lowered_scrutinee;
        };

        if match_expr.arms.is_empty() {
            self.record_error("match expression reached IR lowering without arms", match_expr.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(check_block) };
        }

        let mut arm_entries = Vec::with_capacity(match_expr.arms.len());
        for _ in &match_expr.arms {
            arm_entries.push(self.push_block(blocks));
        }
        let join = self.push_block(blocks);
        let mut result_dest: Option<IrVar> = None;

        for (index, arm) in match_expr.arms.iter().enumerate() {
            let arm_entry = arm_entries[index];
            let condition =
                self.lower_match_pattern_condition(&arm.pattern, lowered_scrutinee.operand.clone(), check_block, blocks, arm.span);
            let Some(condition) = condition else {
                return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(check_block) };
            };
            if matches!(condition, IrOperand::Const(IrConst::Bool(true))) {
                self.block_mut(blocks, check_block).terminator = IrTerminator::Jump(arm_entry);
            } else {
                let else_block = if index + 1 < match_expr.arms.len() {
                    self.push_block(blocks)
                } else {
                    let fail_block = self.push_block(blocks);
                    let fail = self.block_mut(blocks, fail_block);
                    fail.terminator =
                        IrTerminator::Return(Some(IrOperand::Const(IrConst::U64(CellScriptRuntimeError::FlowNewStateInvalid.code()))));
                    fail.runtime_error = Some(CellScriptRuntimeError::FlowNewStateInvalid);
                    fail_block
                };
                self.block_mut(blocks, check_block).terminator =
                    IrTerminator::Branch { cond: condition, then_block: arm_entry, else_block };
                check_block = else_block;
            }

            let mut arm_vars = vars.clone();
            self.enter_branch_scope();
            if !self.bind_match_pattern(&arm.pattern, lowered_scrutinee.operand.clone(), arm_entry, blocks, &mut arm_vars, arm.span) {
                self.leave_branch_scope();
                return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(arm_entry) };
            }
            let lowered_value = self.lower_expr(&arm.value, arm_entry, blocks, &mut arm_vars);
            self.leave_branch_scope();
            let Some(arm_exit) = lowered_value.current else {
                continue;
            };

            if result_dest.is_none() {
                let ty = self.operand_type(&lowered_value.operand);
                result_dest = Some(self.new_var("match_tmp", ty));
            }
            let dest = result_dest.as_ref().expect("match result destination must be initialized");
            let block = self.block_mut(blocks, arm_exit);
            block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: lowered_value.operand });
            block.terminator = IrTerminator::Jump(join);
        }

        let Some(dest) = result_dest else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(join) };
        };
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(join) }
    }

    fn lower_match_pattern_condition(
        &mut self,
        pattern: &MatchPattern,
        operand: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        span: Span,
    ) -> Option<IrOperand> {
        match pattern {
            MatchPattern::Wildcard | MatchPattern::Binding(_) => Some(IrOperand::Const(IrConst::Bool(true))),
            MatchPattern::Tuple(items) => {
                let conditions = items
                    .iter()
                    .enumerate()
                    .map(|(index, pattern)| {
                        let field = self.lower_match_field(&operand, &index.to_string(), current, blocks, span)?;
                        self.lower_match_pattern_condition(pattern, field, current, blocks, span)
                    })
                    .collect::<Option<Vec<_>>>()?;
                self.combine_match_conditions(conditions, BinaryOp::And, current, blocks)
            }
            MatchPattern::Struct { fields, .. } => {
                let conditions = fields
                    .iter()
                    .map(|(field, pattern)| {
                        let field = self.lower_match_field(&operand, field, current, blocks, span)?;
                        self.lower_match_pattern_condition(pattern, field, current, blocks, span)
                    })
                    .collect::<Option<Vec<_>>>()?;
                self.combine_match_conditions(conditions, BinaryOp::And, current, blocks)
            }
            MatchPattern::Variant { path, fields } => {
                let IrType::Named(enum_name) = self.operand_type(&operand) else {
                    self.record_error(format!("enum pattern '{}' reached IR with a non-enum operand", path), span);
                    return None;
                };
                let variant_name = path.rsplit_once("::").map_or(path.as_str(), |(_, variant)| variant);
                let layout = self.module.enum_layouts.get(&enum_name).cloned();
                let tag_condition = if let Some(layout) = &layout {
                    let Some(variant) = layout.variants.iter().find(|variant| variant.name == variant_name) else {
                        self.record_error(format!("unknown match variant '{}'", path), span);
                        return None;
                    };
                    if layout.has_payload() {
                        let tag = self.new_var("match_enum_tag", IrType::U8);
                        self.block_mut(blocks, current).instructions.push(IrInstruction::EnumTag {
                            dest: tag.clone(),
                            operand: operand.clone(),
                            enum_name: enum_name.clone(),
                        });
                        self.lower_match_equality(IrOperand::Var(tag), IrOperand::Const(IrConst::U8(variant.tag)), current, blocks)
                    } else {
                        let pattern_operand = self.lower_match_pattern_operand(path, span)?;
                        self.lower_match_equality(operand.clone(), pattern_operand, current, blocks)
                    }
                } else {
                    let pattern_operand = self.lower_match_pattern_operand(path, span)?;
                    self.lower_match_equality(operand.clone(), pattern_operand, current, blocks)
                };
                let mut conditions = vec![tag_condition];
                if !fields.is_empty() {
                    let Some(layout) = layout.filter(|layout| layout.has_payload()) else {
                        self.record_error(format!("enum pattern '{}' has payload fields but no payload layout", path), span);
                        return None;
                    };
                    let Some(variant) = layout.variants.iter().find(|variant| variant.name == variant_name) else {
                        self.record_error(format!("unknown match variant '{}'", path), span);
                        return None;
                    };
                    for (pattern, field) in fields.iter().zip(&variant.fields) {
                        let dest = self.new_var(format!("match_{}_field_{}", variant_name, field.index), field.ty.clone());
                        self.block_mut(blocks, current).instructions.push(IrInstruction::EnumPayload {
                            dest: dest.clone(),
                            operand: operand.clone(),
                            enum_name: enum_name.clone(),
                            variant: variant_name.to_string(),
                            field_index: field.index,
                        });
                        conditions.push(self.lower_match_pattern_condition(pattern, IrOperand::Var(dest), current, blocks, span)?);
                    }
                }
                self.combine_match_conditions(conditions, BinaryOp::And, current, blocks)
            }
            MatchPattern::Or(patterns) => {
                let conditions = patterns
                    .iter()
                    .map(|pattern| self.lower_match_pattern_condition(pattern, operand.clone(), current, blocks, span))
                    .collect::<Option<Vec<_>>>()?;
                self.combine_match_conditions(conditions, BinaryOp::Or, current, blocks)
            }
        }
    }

    fn lower_match_equality(&mut self, left: IrOperand, right: IrOperand, current: BlockId, blocks: &mut [IrBlock]) -> IrOperand {
        let dest = self.new_var("match_cond", IrType::Bool);
        self.block_mut(blocks, current).instructions.push(IrInstruction::Binary { dest: dest.clone(), op: BinaryOp::Eq, left, right });
        IrOperand::Var(dest)
    }

    fn combine_match_conditions(
        &mut self,
        conditions: Vec<IrOperand>,
        op: BinaryOp,
        current: BlockId,
        blocks: &mut [IrBlock],
    ) -> Option<IrOperand> {
        let mut conditions = conditions.into_iter();
        let mut combined = conditions.next().unwrap_or(IrOperand::Const(IrConst::Bool(true)));
        for condition in conditions {
            if matches!(combined, IrOperand::Const(IrConst::Bool(true))) && op == BinaryOp::And {
                combined = condition;
                continue;
            }
            let dest = self.new_var("match_pattern_cond", IrType::Bool);
            self.block_mut(blocks, current).instructions.push(IrInstruction::Binary {
                dest: dest.clone(),
                op,
                left: combined,
                right: condition,
            });
            combined = IrOperand::Var(dest);
        }
        Some(combined)
    }

    fn lower_match_field(
        &mut self,
        operand: &IrOperand,
        field: &str,
        current: BlockId,
        blocks: &mut [IrBlock],
        span: Span,
    ) -> Option<IrOperand> {
        let base = match operand {
            IrOperand::Var(var) => var.clone(),
            IrOperand::Const(value) => {
                let var = self.new_var("match_value", self.const_type(value));
                self.block_mut(blocks, current)
                    .instructions
                    .push(IrInstruction::LoadConst { dest: var.clone(), value: value.clone() });
                var
            }
        };
        if !self.is_mutable_value_aggregate(&base)
            && let Some(projected) = self.aggregate_fields.get(&base.id).and_then(|fields| fields.get(field)).cloned()
            && self.aggregate_field_read_dominates(projected.id)
        {
            return Some(IrOperand::Var(projected));
        }
        let Some(field_ty) = self.lookup_field_ir_type(&base.ty, field) else {
            self.record_error(format!("pattern field '{}' has no IR layout on type '{}'", field, ir_type_display(&base.ty)), span);
            return None;
        };
        let dest = self.new_var(format!("match_{}_{}", base.name, field), field_ty);
        self.block_mut(blocks, current).instructions.push(IrInstruction::FieldAccess {
            dest: dest.clone(),
            obj: IrOperand::Var(base),
            field: field.to_string(),
        });
        Some(IrOperand::Var(dest))
    }

    fn bind_match_pattern(
        &mut self,
        pattern: &MatchPattern,
        operand: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> bool {
        match pattern {
            MatchPattern::Wildcard => true,
            MatchPattern::Binding(name) => {
                let var = match operand {
                    IrOperand::Var(var) => var,
                    IrOperand::Const(value) => {
                        let var = self.new_var(name, self.const_type(&value));
                        self.block_mut(blocks, current).instructions.push(IrInstruction::LoadConst { dest: var.clone(), value });
                        var
                    }
                };
                vars.insert(name.clone(), var);
                self.record_source_pattern(&crate::ast::BindingPattern::Name(name.clone()), span, vars);
                true
            }
            MatchPattern::Tuple(items) => items.iter().enumerate().all(|(index, pattern)| {
                self.lower_match_field(&operand, &index.to_string(), current, blocks, span)
                    .is_some_and(|field| self.bind_match_pattern(pattern, field, current, blocks, vars, span))
            }),
            MatchPattern::Struct { fields, .. } => fields.iter().all(|(field, pattern)| {
                self.lower_match_field(&operand, field, current, blocks, span)
                    .is_some_and(|field| self.bind_match_pattern(pattern, field, current, blocks, vars, span))
            }),
            MatchPattern::Variant { path, fields } => {
                if fields.is_empty() {
                    return true;
                }
                let IrType::Named(enum_name) = self.operand_type(&operand) else {
                    self.record_error(format!("enum pattern '{}' reached binding with a non-enum operand", path), span);
                    return false;
                };
                let variant_name = path.rsplit_once("::").map_or(path.as_str(), |(_, variant)| variant);
                let Some(layout) = self.module.enum_layouts.get(&enum_name).cloned() else {
                    self.record_error(format!("enum pattern '{}' has no IR payload layout", path), span);
                    return false;
                };
                let Some(variant) = layout.variants.iter().find(|variant| variant.name == variant_name) else {
                    self.record_error(format!("unknown match variant '{}'", path), span);
                    return false;
                };
                fields.iter().zip(&variant.fields).all(|(pattern, field)| {
                    let dest = self.new_var(format!("match_{}_field_{}", variant_name, field.index), field.ty.clone());
                    self.block_mut(blocks, current).instructions.push(IrInstruction::EnumPayload {
                        dest: dest.clone(),
                        operand: operand.clone(),
                        enum_name: enum_name.clone(),
                        variant: variant_name.to_string(),
                        field_index: field.index,
                    });
                    self.bind_match_pattern(pattern, IrOperand::Var(dest), current, blocks, vars, span)
                })
            }
            MatchPattern::Or(_) => true,
        }
    }
}
