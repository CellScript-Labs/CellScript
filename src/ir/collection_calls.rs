use super::{
    direct_bounded_plan_field_expr, BlockId, CreatePattern, IrBlock, IrConst, IrGenerator, IrIdentityPolicy, IrInstruction, IrOperand,
    IrTerminator, IrType, IrTypeKind, IrVar, LoweredExpr, BOUNDED_CELL_RUNTIME_MAX_DATA_BYTES, BOUNDED_OUTPUT_PLAN_MAX_BYTES,
};
use crate::ast::{BinaryOp, BoundedCollectionKind, CallExpr, CreateExpr, Expr, ParamSource, Stmt};
use std::collections::HashMap;

impl IrGenerator<'_> {
    // Inspect without emitting instructions: non-selection methods must retain
    // their normal lowering path and evaluate their receiver exactly once.
    pub(super) fn selection_receiver_type(
        &self,
        expr: &Expr,
        vars: &HashMap<String, IrVar>,
    ) -> Option<crate::cell_selection::SelectionType> {
        match expr {
            Expr::Identifier(name) => match &vars.get(name)?.ty {
                IrType::Selection(selection) => Some((**selection).clone()),
                _ => None,
            },
            Expr::FieldAccess(field) => self.selection_receiver_type(&field.expr, vars)?.project_role(&field.field),
            Expr::Call(call) if matches!(call.func.as_ref(), Expr::Identifier(name) if name == crate::cell_selection::SELECT) => {
                Some(crate::cell_selection::SelectionType::from_contract(
                    &crate::cell_selection::resolve_call(call, &self.cell_role_catalogs).ok()?,
                ))
            }
            _ => None,
        }
    }

    pub(super) fn lower_bounded_consume_each(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let [Expr::Identifier(binding), Expr::Identifier(collection_binding), Expr::Block(body)] = call.args.as_slice() else {
            return None;
        };
        let collection_var = vars.get(collection_binding)?.clone();
        let IrType::Bounded(collection) = &collection_var.ty else {
            return None;
        };
        let collection = collection.as_ref().clone();
        let IrType::Named(element_name) = &collection.element_type else { return None };
        if collection.kind != BoundedCollectionKind::CellSet {
            return None;
        }
        let element_width =
            self.bounded_consume_runtime_width(&collection.element_type, ParamSource::Input, collection.max_elements)?;

        let index_var = self.new_var("bounded_cell_index", IrType::U64);
        self.block_mut(blocks, current)
            .instructions
            .push(IrInstruction::Move { dest: index_var.clone(), src: IrOperand::Const(IrConst::U64(0)) });

        let scan_block = self.push_block(blocks);
        let body_block = self.push_block(blocks);
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(scan_block);

        let element_var = self.new_var(binding.clone(), collection.element_type.clone());
        let found_var = self.new_var("bounded_cell_found", IrType::Bool);
        self.block_mut(blocks, scan_block).instructions.push(IrInstruction::BoundedCellLoad {
            dest: element_var.clone(),
            found: found_var.clone(),
            index: IrOperand::Var(index_var.clone()),
            max_elements: collection.max_elements,
            element_type: element_name.clone(),
            element_width,
        });
        self.block_mut(blocks, scan_block).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(found_var), then_block: body_block, else_block: exit_block };

        let mut body_vars = vars.clone();
        body_vars.insert(binding.clone(), element_var);
        let body_exit = self.lower_stmts(body, body_block, blocks, &mut body_vars, None, false);
        if let Some(body_exit) = body_exit {
            let next_index = self.new_var("bounded_cell_next", IrType::U64);
            let block = self.block_mut(blocks, body_exit);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var, src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(scan_block);
        }

        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(exit_block) })
    }

    pub(super) fn lower_bounded_create_each(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let [Expr::Identifier(binding), Expr::Identifier(collection_binding), Expr::Block(body)] = call.args.as_slice() else {
            return None;
        };
        let collection_var = vars.get(collection_binding)?.clone();
        let IrType::Bounded(collection) = &collection_var.ty else {
            return None;
        };
        let collection = collection.as_ref().clone();
        let IrType::Named(element_name) = &collection.element_type else { return None };
        if collection.kind != BoundedCollectionKind::List {
            return None;
        }
        let create = body.iter().find_map(|stmt| match stmt {
            Stmt::Expr(Expr::Create(create)) | Stmt::Discard(Expr::Create(create)) => Some(create),
            _ => None,
        })?;
        let (element_width, capacity_floor_shannons) =
            self.bounded_create_call_runtime_shape(binding, element_name, collection.max_elements, create)?;

        let index_var = self.new_var("bounded_plan_index", IrType::U64);
        self.block_mut(blocks, current)
            .instructions
            .push(IrInstruction::Move { dest: index_var.clone(), src: IrOperand::Const(IrConst::U64(0)) });

        let scan_block = self.push_block(blocks);
        let body_block = self.push_block(blocks);
        let exit_block = self.push_block(blocks);
        self.block_mut(blocks, current).terminator = IrTerminator::Jump(scan_block);

        let element_var = self.new_var(binding.clone(), collection.element_type.clone());
        let found_var = self.new_var("bounded_plan_found", IrType::Bool);
        self.block_mut(blocks, scan_block).instructions.push(IrInstruction::BoundedPlanLoad {
            dest: element_var.clone(),
            found: found_var.clone(),
            plan: IrOperand::Var(collection_var),
            index: IrOperand::Var(index_var.clone()),
            max_elements: collection.max_elements,
            element_type: element_name.clone(),
            element_width,
        });
        self.block_mut(blocks, scan_block).terminator =
            IrTerminator::Branch { cond: IrOperand::Var(found_var), then_block: body_block, else_block: exit_block };

        let mut body_vars = vars.clone();
        body_vars.insert(binding.clone(), element_var);
        let mut active = Some(body_block);
        for stmt in body {
            let Some(current_body) = active else { break };
            if let Stmt::Expr(Expr::Create(create)) | Stmt::Discard(Expr::Create(create)) = stmt {
                let (next, pattern) = self.lower_bounded_create_pattern(create, current_body, blocks, &mut body_vars)?;
                self.block_mut(blocks, next).instructions.push(IrInstruction::BoundedOutputVerify {
                    index: IrOperand::Var(index_var.clone()),
                    pattern,
                    capacity_floor_shannons,
                });
                active = Some(next);
            } else {
                active = self.lower_stmt(stmt, current_body, blocks, &mut body_vars, None);
            }
        }
        if let Some(body_exit) = active {
            let next_index = self.new_var("bounded_plan_next", IrType::U64);
            let block = self.block_mut(blocks, body_exit);
            block.instructions.push(IrInstruction::Binary {
                dest: next_index.clone(),
                op: BinaryOp::Add,
                left: IrOperand::Var(index_var.clone()),
                right: IrOperand::Const(IrConst::U64(1)),
            });
            block.instructions.push(IrInstruction::Move { dest: index_var.clone(), src: IrOperand::Var(next_index) });
            block.terminator = IrTerminator::Jump(scan_block);
        }

        self.block_mut(blocks, exit_block).instructions.push(IrInstruction::BoundedOutputEnd { index: IrOperand::Var(index_var) });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(exit_block) })
    }

    fn bounded_create_call_runtime_shape(
        &self,
        binding: &str,
        element_type: &str,
        max_elements: usize,
        create: &CreateExpr,
    ) -> Option<(usize, u64)> {
        if !(1..=1024).contains(&max_elements) || self.type_kinds.get(element_type) != Some(&IrTypeKind::Struct) {
            return None;
        }
        let plan_width = self.fixed_encoded_size(&IrType::Named(element_type.to_string()))?;
        let encoded_max = max_elements.checked_mul(plan_width)?.checked_add(12)?;
        if plan_width == 0 || encoded_max > BOUNDED_OUTPUT_PLAN_MAX_BYTES {
            return None;
        }
        if self.type_kinds.get(&create.ty) != Some(&IrTypeKind::Resource)
            || self
                .fixed_encoded_size(&IrType::Named(create.ty.clone()))
                .is_none_or(|width| width == 0 || width > BOUNDED_CELL_RUNTIME_MAX_DATA_BYTES)
            || self.type_identities.get(&create.ty) != Some(&IrIdentityPolicy::None)
            || create.lock.is_none()
        {
            return None;
        }
        let fields = self.type_fields.get(&create.ty)?;
        let plan_fields = self.type_fields.get(element_type)?;
        if create.fields.len() != fields.len() || !fields.keys().all(|field| create.fields.iter().any(|(name, _)| name == field)) {
            return None;
        }
        for (output_field, value) in &create.fields {
            let plan_field = direct_bounded_plan_field_expr(value, binding)?;
            if fields.get(output_field) != plan_fields.get(plan_field) {
                return None;
            }
        }
        let lock_field = direct_bounded_plan_field_expr(create.lock.as_deref()?, binding)?;
        if self.fixed_encoded_size(plan_fields.get(lock_field)?) != Some(32) {
            return None;
        }
        Some((plan_width, self.type_capacity_floors.get(&create.ty).copied().filter(|floor| *floor > 0)?))
    }

    fn lower_bounded_create_pattern(
        &mut self,
        create: &CreateExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<(BlockId, CreatePattern)> {
        let mut active = current;
        let mut lowered_fields = Vec::with_capacity(create.fields.len());
        let mut field_vars = HashMap::new();
        for (field_name, field_expr) in &create.fields {
            let expected_ty = self.type_fields.get(&create.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(state_operand) = self.lower_flow_state_initializer(&create.ty, field_name, field_expr) {
                LoweredExpr { operand: state_operand, current: Some(active) }
            } else if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            active = lowered.current?;
            lowered_fields.push((field_name.clone(), lowered.operand.clone()));
            let field_ty = self.operand_type(&lowered.operand);
            let field_var = self.new_var(format!("{}_{}", create.ty, field_name), field_ty);
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_vars.insert(field_name.clone(), field_var);
        }
        let lock = if let Some(lock_expr) = &create.lock {
            let lowered = self.lower_expr(lock_expr, active, blocks, vars);
            active = lowered.current?;
            Some(lowered.operand)
        } else {
            None
        };
        active = self.lower_type_validity_checks(&create.ty, &field_vars, active, blocks, vars)?;
        Some((
            active,
            CreatePattern {
                operation: "bounded-create".to_string(),
                ty: create.ty.clone(),
                binding: format!("bounded_create_{}", create.ty),
                fields: lowered_fields,
                lock,
                identity: IrIdentityPolicy::None,
            },
        ))
    }
}
