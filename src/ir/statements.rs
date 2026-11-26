use super::{
    binding_pattern_label, ir_type_display, BlockId, IrBlock, IrBorrowRegion, IrConst, IrGenerator, IrInstruction, IrOperand,
    IrSourceBinding, IrTerminator, IrType, IrVar,
};
use crate::ast::{BindingPattern, ReturnStmt, Stmt};
use crate::error::Span;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_stmts(
        &mut self,
        stmts: &[Stmt],
        mut current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
        tail_expr_returns: bool,
    ) -> Option<BlockId> {
        for (index, stmt) in stmts.iter().enumerate() {
            if tail_expr_returns && index + 1 == stmts.len() {
                if let Stmt::Expr(expr) = stmt {
                    let lowered = if let Some(return_type) = return_type {
                        self.lower_expr_with_expected_type(expr, return_type, current, blocks, vars)
                    } else {
                        self.lower_expr(expr, current, blocks, vars)
                    };
                    let active = lowered.current?;
                    let value = self.materialize_fixed_return_constant(lowered.operand, self.block_mut(blocks, active));
                    self.block_mut(blocks, active).terminator = IrTerminator::Return(Some(value));
                    return None;
                }
                if let Stmt::If(if_stmt) = stmt {
                    return self.lower_if_stmt(if_stmt, current, blocks, vars, return_type, true);
                }
            }
            let next = self.lower_stmt(stmt, current, blocks, vars, return_type)?;
            current = next;
        }

        Some(current)
    }

    pub(super) fn record_source_pattern(&mut self, pattern: &BindingPattern, declaration: Span, vars: &HashMap<String, IrVar>) {
        if !cfg!(feature = "collection-explanation") {
            return;
        }
        match pattern {
            BindingPattern::Name(name) => {
                if let Some(value) = vars.get(name) {
                    self.source_bindings.push(IrSourceBinding { name: name.clone(), value: value.clone(), declaration });
                }
            }
            BindingPattern::Tuple(items) => {
                for item in items {
                    self.record_source_pattern(item, declaration, vars);
                }
            }
            BindingPattern::Wildcard => {}
        }
    }

    pub(super) fn lower_stmt(
        &mut self,
        stmt: &Stmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        let next = self.lower_stmt_inner(stmt, current, blocks, vars, return_type);
        if next.is_some()
            && let Stmt::Let(binding) = stmt
        {
            self.record_source_pattern(&binding.pattern, binding.span, vars);
        }
        next
    }

    fn lower_stmt_inner(
        &mut self,
        stmt: &Stmt,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        return_type: Option<&IrType>,
    ) -> Option<BlockId> {
        match stmt {
            Stmt::Lifecycle(value) => self.lower_lifecycle_statement(value, current, blocks, vars, return_type),
            Stmt::Let(let_stmt) => {
                let transition_coverable = matches!(let_stmt.pattern, BindingPattern::Name(_))
                    && self.transition_expr_is_coverable_u64(&let_stmt.value, vars);
                let lowered = match &let_stmt.ty {
                    Some(declared_ty) => {
                        self.lower_expr_with_expected_type(&let_stmt.value, &self.convert_type(declared_ty), current, blocks, vars)
                    }
                    None => self.lower_expr(&let_stmt.value, current, blocks, vars),
                };
                let active = lowered.current?;
                if let BindingPattern::Name(name) = &let_stmt.pattern
                    && matches!(&lowered.operand, IrOperand::Var(var) if matches!(&var.ty, IrType::Named(owner) if self.collection_pools.contains_key(owner)))
                {
                    let var = self.materialize_owned_operand(name, lowered.operand, self.block_mut(blocks, active));
                    vars.insert(name.clone(), var);
                    return Some(active);
                }
                let block = self.block_mut(blocks, active);
                if let (BindingPattern::Name(name), Some(declared_ty)) = (&let_stmt.pattern, &let_stmt.ty) {
                    let declared_ty = self.convert_type(declared_ty);
                    let var = if let_stmt.is_mut {
                        let owned = self.materialize_owned_operand(name, lowered.operand, block);
                        if owned.ty == declared_ty {
                            owned
                        } else {
                            self.materialize_operand_with_type(name, IrOperand::Var(owned), declared_ty, block)
                        }
                    } else {
                        self.materialize_operand_with_type(name, lowered.operand, declared_ty, block)
                    };
                    if let_stmt.is_mut {
                        self.mutable_local_ids.insert(var.id);
                    }
                    if transition_coverable && var.ty == IrType::U64 {
                        self.transition_coverable_value_ids.insert(var.id);
                    }
                    vars.insert(name.clone(), var);
                    return Some(active);
                }
                if let (BindingPattern::Name(name), true) = (&let_stmt.pattern, let_stmt.is_mut) {
                    let var = self.materialize_owned_operand(name, lowered.operand, block);
                    self.mutable_local_ids.insert(var.id);
                    if transition_coverable && var.ty == IrType::U64 {
                        self.transition_coverable_value_ids.insert(var.id);
                    }
                    vars.insert(name.clone(), var);
                    return Some(active);
                }
                let bound = self.bind_pattern(&let_stmt.pattern, lowered.operand, block, vars);
                if transition_coverable && bound.as_ref().is_some_and(|var| var.ty == IrType::U64) {
                    let bound = bound.expect("checked above");
                    self.transition_coverable_value_ids.insert(bound.id);
                }
                Some(active)
            }
            Stmt::Expr(expr) | Stmt::Discard(expr) => self.lower_expr(expr, current, blocks, vars).current,
            Stmt::Return(ReturnStmt { value: None, .. }) => {
                self.block_mut(blocks, current).terminator = IrTerminator::Return(None);
                None
            }
            Stmt::Return(ReturnStmt { value: Some(expr), .. }) => {
                let lowered = if let Some(return_type) = return_type {
                    self.lower_expr_with_expected_type(expr, return_type, current, blocks, vars)
                } else {
                    self.lower_expr(expr, current, blocks, vars)
                };
                let active = lowered.current?;
                let value = self.materialize_fixed_return_constant(lowered.operand, self.block_mut(blocks, active));
                self.block_mut(blocks, active).terminator = IrTerminator::Return(Some(value));
                None
            }
            Stmt::If(if_stmt) => self.lower_if_stmt(if_stmt, current, blocks, vars, return_type, false),
            Stmt::For(for_stmt) => self.lower_for_stmt(for_stmt, current, blocks, vars, return_type),
            Stmt::While(while_stmt) => self.lower_while_stmt(while_stmt, current, blocks, vars, return_type),
            Stmt::Break(control) | Stmt::Continue(control) => {
                let target = self
                    .loop_targets
                    .iter()
                    .rev()
                    .find(|target| control.label.as_ref().is_none_or(|label| target.label.as_ref() == Some(label)))
                    .cloned();
                let Some(target) = target else {
                    self.record_error("loop control has no matching lowered loop", control.span);
                    return Some(current);
                };
                let destination = if matches!(stmt, Stmt::Break(_)) { target.break_block } else { target.continue_block };
                self.block_mut(blocks, current).terminator = IrTerminator::Jump(destination);
                None
            }
            Stmt::Borrow(borrow_stmt) => {
                let Some(root) = vars.get(&borrow_stmt.root).cloned() else {
                    self.record_error(format!("borrow root '{}' was not lowered", borrow_stmt.root), borrow_stmt.span);
                    return Some(current);
                };
                let mut projected = root.clone();
                for field in &borrow_stmt.path {
                    let Some(next) = self.materialize_schema_field(&projected, field, current, blocks) else {
                        self.record_error(
                            format!("borrow path '{}.{}' has no lowered field", borrow_stmt.root, borrow_stmt.path.join(".")),
                            borrow_stmt.span,
                        );
                        return Some(current);
                    };
                    projected = next;
                }
                let view_target = match &projected.ty {
                    IrType::Ref(inner) | IrType::MutRef(inner) => inner.as_ref().clone(),
                    ty => ty.clone(),
                };
                let parent_region = self.borrow_regions.iter().rev().find(|region| region.binding == borrow_stmt.root).cloned();
                let canonical_root = parent_region.as_ref().map_or_else(|| borrow_stmt.root.clone(), |region| region.root.clone());
                let root_type = parent_region.as_ref().map_or_else(
                    || {
                        ir_type_display(match &root.ty {
                            IrType::Ref(inner) | IrType::MutRef(inner) => inner,
                            ty => ty,
                        })
                    },
                    |region| region.root_type.clone(),
                );
                let mut canonical_path = parent_region.map_or_else(Vec::new, |region| region.path);
                canonical_path.extend(borrow_stmt.path.iter().cloned());
                let region_index = self.borrow_regions.len();
                let start_instruction = self.block_mut(blocks, current).instructions.len();
                let view =
                    IrVar { id: projected.id, name: borrow_stmt.binding.clone(), ty: IrType::Ref(Box::new(view_target.clone())) };
                self.borrow_regions.push(IrBorrowRegion {
                    root: canonical_root,
                    path: canonical_path,
                    binding: borrow_stmt.binding.clone(),
                    root_type,
                    view_type: ir_type_display(&view_target),
                    view_binding: view.clone(),
                    start_block: current,
                    start_instruction,
                    end_block: None,
                    end_instruction: None,
                    span: borrow_stmt.span,
                });
                let previous = vars.insert(borrow_stmt.binding.clone(), view);
                self.record_source_pattern(&BindingPattern::Name(borrow_stmt.binding.clone()), borrow_stmt.span, vars);
                let exit = self.lower_stmts(&borrow_stmt.body, current, blocks, vars, return_type, false);
                if let Some(end_block) = exit {
                    self.borrow_regions[region_index].end_block = Some(end_block);
                    self.borrow_regions[region_index].end_instruction = Some(self.block_mut(blocks, end_block).instructions.len());
                }
                if let Some(previous) = previous {
                    vars.insert(borrow_stmt.binding.clone(), previous);
                } else {
                    vars.remove(&borrow_stmt.binding);
                }
                exit
            }
        }
    }

    pub(super) fn bind_pattern(
        &mut self,
        pattern: &BindingPattern,
        value: IrOperand,
        block: &mut IrBlock,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<IrVar> {
        match pattern {
            BindingPattern::Name(name) => {
                let var = self.materialize_operand(name, value, block);
                vars.insert(name.clone(), var.clone());
                Some(var)
            }
            BindingPattern::Tuple(items) => {
                let base_var = match &value {
                    IrOperand::Var(var) => Some(var.clone()),
                    IrOperand::Const(_) => None,
                };
                for (index, item) in items.iter().enumerate() {
                    if matches!(item, BindingPattern::Wildcard) {
                        continue;
                    }
                    let field = index.to_string();
                    let tuple_name = format!("{}_{}", binding_pattern_label(item), index);
                    let projected = base_var
                        .as_ref()
                        .filter(|var| !self.is_mutable_value_aggregate(var))
                        .and_then(|var| self.aggregate_fields.get(&var.id).and_then(|fields| fields.get(&field)).cloned())
                        .filter(|field| self.aggregate_field_read_dominates(field.id))
                        .map(IrOperand::Var)
                        .or_else(|| {
                            let base_var = base_var.as_ref()?;
                            let field_ty = self.lookup_field_ir_type(&base_var.ty, &field)?;
                            let field_var = self.new_var(tuple_name, field_ty);
                            block.instructions.push(IrInstruction::FieldAccess {
                                dest: field_var.clone(),
                                obj: IrOperand::Var(base_var.clone()),
                                field,
                            });
                            Some(IrOperand::Var(field_var))
                        });

                    if let Some(projected) = projected {
                        self.bind_pattern(item, projected, block, vars);
                    } else {
                        self.record_error("tuple binding requires a lowered tuple aggregate", Span::default());
                    }
                }
                base_var
            }
            BindingPattern::Wildcard => None,
        }
    }

    fn materialize_fixed_return_constant(&mut self, operand: IrOperand, block: &mut IrBlock) -> IrOperand {
        let copied = match &operand {
            IrOperand::Const(IrConst::Hash(_) | IrConst::Address(_)) => true,
            IrOperand::Const(IrConst::Array(items)) => {
                (1..=64).contains(&items.len()) && items.iter().all(|item| matches!(item, IrConst::U8(_)))
            }
            _ => false,
        };
        if copied {
            IrOperand::Var(self.materialize_operand("returned_bytes", operand, block))
        } else {
            operand
        }
    }

    fn materialize_operand(&mut self, name: &str, operand: IrOperand, block: &mut IrBlock) -> IrVar {
        match operand {
            IrOperand::Var(var) if self.mutable_local_ids.contains(&var.id) => {
                self.materialize_owned_operand(name, IrOperand::Var(var), block)
            }
            IrOperand::Var(var) => var,
            IrOperand::Const(value) => {
                let ty = self.const_type(&value);
                let var = self.new_var(name.to_string(), ty);
                block.instructions.push(IrInstruction::LoadConst { dest: var.clone(), value });
                var
            }
        }
    }

    pub(super) fn materialize_operand_with_type(&mut self, name: &str, operand: IrOperand, ty: IrType, block: &mut IrBlock) -> IrVar {
        match operand {
            IrOperand::Var(var) if var.ty == ty && !self.mutable_local_ids.contains(&var.id) => var,
            IrOperand::Var(var) => {
                let dest = self.new_var(name.to_string(), ty);
                let source = IrOperand::Var(var);
                block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: source.clone() });
                self.copy_aggregate_metadata(&source, dest.id);
                dest
            }
            IrOperand::Const(value) => {
                let dest = self.new_var(name.to_string(), ty);
                block.instructions.push(IrInstruction::LoadConst { dest: dest.clone(), value });
                dest
            }
        }
    }

    fn materialize_owned_operand(&mut self, name: &str, operand: IrOperand, block: &mut IrBlock) -> IrVar {
        match operand {
            IrOperand::Var(var) => {
                let dest = self.new_var(name.to_string(), var.ty.clone());
                let source = IrOperand::Var(var);
                block.instructions.push(IrInstruction::Move { dest: dest.clone(), src: source.clone() });
                self.copy_aggregate_metadata(&source, dest.id);
                dest
            }
            IrOperand::Const(value) => {
                let ty = self.const_type(&value);
                let dest = self.new_var(name.to_string(), ty);
                block.instructions.push(IrInstruction::LoadConst { dest: dest.clone(), value });
                dest
            }
        }
    }
}
