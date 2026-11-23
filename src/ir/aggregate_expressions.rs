use super::nominal::resolved_types_equal;
use super::{
    call_target_is_min, collection_item_ir_type, const_usize_operand, direct_field_access_root, same_direct_field_access,
    script_ref_property_runtime_helper, typed_view_property_runtime_helper, typed_view_property_wrapper, witness_bytes_view_parts,
    BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrType, IrVar, LoweredExpr, MutateFieldTransition,
    MutateTransitionOp, CKB_LOCK_SCRIPT_REF_TYPE, CKB_TYPE_SCRIPT_REF_TYPE,
};
use crate::ast::{AssignOp, BinaryExpr, BinaryOp, Expr, FieldAccessExpr, IndexExpr, StructInitExpr};
use crate::error::{CompileError, Span};
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_index_expr(
        &mut self,
        index: &IndexExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_arr = self.lower_expr(&index.expr, current, blocks, vars);
        let Some(active) = lowered_arr.current else {
            return lowered_arr;
        };
        let lowered_idx = self.lower_expr(&index.index, active, blocks, vars);
        let Some(active) = lowered_idx.current else {
            return lowered_idx;
        };

        if let IrOperand::Var(arr_var) = &lowered_arr.operand
            && !self.is_mutable_value_aggregate(arr_var)
            && let Some(elements) = self.aggregate_elements.get(&arr_var.id)
        {
            let Some(index_value) = const_usize_operand(&lowered_idx.operand) else {
                self.record_error("local fixed-array indexing requires a compile-time constant index", index.span);
                return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
            };
            let Some(element_var) = elements.get(index_value).cloned() else {
                self.record_error(
                    format!("array index {} is out of bounds for local fixed array of length {}", index_value, elements.len()),
                    index.span,
                );
                return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
            };
            return LoweredExpr { operand: IrOperand::Var(element_var), current: Some(active) };
        }

        let Some(result_ty) = self.index_result_type(&lowered_arr.operand) else {
            self.record_error("index expression has no lowered element type", index.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
        };

        let dest = self.new_var("index_tmp", result_ty);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Index {
            dest: dest.clone(),
            arr: lowered_arr.operand,
            idx: lowered_idx.operand,
        });
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    pub(super) fn lower_array_expr(
        &mut self,
        items: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        if items.is_empty() {
            self.record_error("empty array literal reached IR lowering without a declared array type", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        }

        let mut active = current;
        let mut elements = Vec::with_capacity(items.len());
        let mut element_ty = None;

        for (index, item) in items.iter().enumerate() {
            let lowered = self.lower_expr(item, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            element_ty.get_or_insert_with(|| ty.clone());
            let element_var = self.new_var(format!("array_elem_{}", index), ty);
            self.block_mut(blocks, active)
                .instructions
                .push(IrInstruction::Move { dest: element_var.clone(), src: lowered.operand.clone() });
            self.copy_aggregate_metadata(&lowered.operand, element_var.id);
            elements.push(element_var);
        }

        let Some(element_ty) = element_ty else {
            self.record_error("non-empty array literal did not produce an element type during IR lowering", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) };
        };
        let array_ty = IrType::Array(Box::new(element_ty), items.len());
        let aggregate = self.new_var("array_tmp", array_ty);
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Tuple { dest: aggregate.clone(), fields: elements.iter().cloned().map(IrOperand::Var).collect() });
        self.aggregate_elements.insert(aggregate.id, elements);
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    pub(super) fn lower_expr_with_expected_type(
        &mut self,
        expr: &Expr,
        expected_ty: &IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        match expr {
            Expr::Tuple(items) if items.is_empty() && matches!(expected_ty, IrType::Unit) => {
                LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(current) }
            }
            Expr::Call(call)
                if collection_item_ir_type(expected_ty).is_some()
                    && matches!(call.func.as_ref(), Expr::Identifier(name) if matches!(name.as_str(), "Vec::new" | "Vec::with_capacity")) =>
            {
                self.lower_vec_constructor_expr(call, expected_ty.clone(), current, blocks, vars)
            }
            Expr::Integer(value) => {
                if let Some(operand) = self.integer_operand_for_expected_type(*value, expected_ty, self.block_mut(blocks, current)) {
                    LoweredExpr { operand, current: Some(current) }
                } else {
                    self.lower_expr(expr, current, blocks, vars)
                }
            }
            Expr::Array(items) if collection_item_ir_type(expected_ty).is_some() => {
                self.lower_vec_literal_expr(items, expected_ty.clone(), current, blocks, vars)
            }
            Expr::Array(items) if matches!(expected_ty, IrType::Array(_, _)) => {
                self.lower_array_expr_with_expected_type(items, expected_ty.clone(), current, blocks, vars)
            }
            Expr::Tuple(items) if matches!(expected_ty, IrType::Tuple(_)) => {
                let IrType::Tuple(types) = expected_ty else { unreachable!() };
                self.lower_tuple_expr_with_types(items, Some(types), current, blocks, vars)
            }
            Expr::Block(stmts) => self.lower_tail_block_value_with_expected_type(stmts, expected_ty, current, blocks, vars),
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
                ) && Self::integer_const_for_expected_type(0, expected_ty).is_some() =>
            {
                self.lower_binary_expr_with_expected_type(binary, expected_ty.clone(), current, blocks, vars)
            }
            Expr::If(if_expr) => self.lower_if_expr_with_expected_type(if_expr, expected_ty.clone(), current, blocks, vars),
            _ => {
                let lowered = self.lower_expr(expr, current, blocks, vars);
                // Evaluate first: a unit-valued call or block can still have
                // effects. Only its already evaluated zero-tuple payload is
                // represented as the canonical generic unit argument.
                if matches!(expected_ty, IrType::Unit)
                    && matches!(&lowered.operand, IrOperand::Var(var) if matches!(&var.ty, IrType::Tuple(items) if items.is_empty()))
                {
                    LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: lowered.current }
                } else if matches!(expected_ty, IrType::Tuple(items) if items.is_empty())
                    && matches!(&lowered.operand, IrOperand::Const(IrConst::Unit) | IrOperand::Var(IrVar { ty: IrType::Unit, .. }))
                {
                    if let Some(active) = lowered.current {
                        self.lower_tuple_expr(&[], active, blocks, vars)
                    } else {
                        lowered
                    }
                } else {
                    lowered
                }
            }
        }
    }

    fn lower_array_expr_with_expected_type(
        &mut self,
        items: &[Expr],
        expected_ty: IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let IrType::Array(item_ty, expected_len) = &expected_ty else {
            self.record_error("array literal requires an expected array type", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        };
        if items.len() != *expected_len {
            self.record_error(
                format!("array literal length mismatch during IR lowering: expected {}, found {}", expected_len, items.len()),
                Span::default(),
            );
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        }
        if items.is_empty() {
            return self.lower_empty_array_expr_with_ir_type(expected_ty, current, blocks);
        }

        let mut active = current;
        let mut elements = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let lowered = self.lower_expr_with_expected_type(item, item_ty, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
            let actual_ty = self.operand_type(&lowered.operand);
            if !resolved_types_equal(&actual_ty, item_ty, &self.module.nominal_identities) {
                self.record_error(
                    format!("array literal type mismatch during IR lowering: expected {:?}, found {:?}", item_ty, actual_ty),
                    Span::default(),
                );
            }
            let element_var = self.new_var(format!("array_elem_{}", index), (**item_ty).clone());
            self.block_mut(blocks, active)
                .instructions
                .push(IrInstruction::Move { dest: element_var.clone(), src: lowered.operand.clone() });
            self.copy_aggregate_metadata(&lowered.operand, element_var.id);
            elements.push(element_var);
        }

        let aggregate = self.new_var("array_tmp", expected_ty);
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Tuple { dest: aggregate.clone(), fields: elements.iter().cloned().map(IrOperand::Var).collect() });
        self.aggregate_elements.insert(aggregate.id, elements);
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    fn lower_binary_expr_with_expected_type(
        &mut self,
        binary: &BinaryExpr,
        expected_ty: IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let left = self.lower_expr_with_expected_type(&binary.left, &expected_ty, current, blocks, vars);
        let Some(active) = left.current else {
            return left;
        };
        let right = if matches!(binary.op, BinaryOp::Shl | BinaryOp::Shr) {
            self.lower_expr(&binary.right, active, blocks, vars)
        } else {
            self.lower_expr_with_expected_type(&binary.right, &expected_ty, active, blocks, vars)
        };
        let Some(active) = right.current else {
            return right;
        };
        let dest = self.new_var("tmp", expected_ty);
        let block = self.block_mut(blocks, active);
        block.instructions.push(IrInstruction::Binary { dest: dest.clone(), op: binary.op, left: left.operand, right: right.operand });
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_vec_literal_expr(
        &mut self,
        items: &[Expr],
        vec_ty: IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let Some(item_ty) = collection_item_ir_type(&vec_ty) else {
            self.record_error("Vec literal requires an expected Vec<T> type", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        };

        let dest = self.new_var("vec_literal_tmp", vec_ty);
        self.block_mut(blocks, current).instructions.push(IrInstruction::CollectionNew {
            dest: dest.clone(),
            ty: IrType::Vector(None),
            capacity: (!items.is_empty()).then_some(IrOperand::Const(IrConst::U64(items.len() as u64))),
        });

        let mut active = current;
        for item in items {
            let lowered = self.lower_expr_with_expected_type(item, &item_ty, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let actual_ty = self.operand_type(&lowered.operand);
            if !resolved_types_equal(&actual_ty, &item_ty, &self.module.nominal_identities) {
                self.record_error(
                    format!("Vec literal type mismatch: expected {:?}, found {:?}", item_ty, actual_ty),
                    Span::default(),
                );
            }

            self.block_mut(blocks, active)
                .instructions
                .push(IrInstruction::CollectionPush { collection: IrOperand::Var(dest.clone()), value: lowered.operand });
        }

        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }

    fn lower_empty_array_expr_with_ir_type(&mut self, ir_ty: IrType, current: BlockId, blocks: &mut [IrBlock]) -> LoweredExpr {
        if !matches!(ir_ty, IrType::Array(_, 0)) {
            self.record_error("empty array literal requires a zero-length declared array type", Span::default());
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) };
        }

        let aggregate = self.new_var("array_tmp", ir_ty);
        self.block_mut(blocks, current).instructions.push(IrInstruction::Tuple { dest: aggregate.clone(), fields: vec![] });
        self.aggregate_elements.insert(aggregate.id, Vec::new());
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(current) }
    }

    pub(super) fn lower_tuple_expr(
        &mut self,
        items: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        self.lower_tuple_expr_with_types(items, None, current, blocks, vars)
    }

    fn lower_tuple_expr_with_types(
        &mut self,
        items: &[Expr],
        expected: Option<&[IrType]>,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let mut active = current;
        let mut fields = HashMap::new();
        let mut types = Vec::with_capacity(items.len());

        for (index, item) in items.iter().enumerate() {
            let lowered = if let Some(ty) = expected.and_then(|types| types.get(index)) {
                self.lower_expr_with_expected_type(item, ty, active, blocks, vars)
            } else {
                self.lower_expr(item, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("tuple_{}", index), ty.clone());
            self.block_mut(blocks, active)
                .instructions
                .push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand.clone() });
            self.copy_aggregate_metadata(&lowered.operand, field_var.id);
            fields.insert(index.to_string(), field_var);
            types.push(ty);
        }

        let aggregate = self.new_var("tuple_tmp", IrType::Tuple(types));
        let fields_for_instruction =
            (0..items.len()).filter_map(|index| fields.get(&index.to_string()).cloned().map(IrOperand::Var)).collect::<Vec<_>>();
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Tuple { dest: aggregate.clone(), fields: fields_for_instruction });
        self.aggregate_fields.insert(aggregate.id, fields);
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    pub(super) fn lower_struct_init(
        &mut self,
        init: &StructInitExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let aggregate = self.new_var("struct_tmp", IrType::Named(init.ty.clone()));
        let mut field_map = HashMap::new();
        let mut field_operands = HashMap::new();
        let mut active = current;

        for (field_name, field_expr) in &init.fields {
            let expected_ty = self.type_fields.get(&init.ty).and_then(|fields| fields.get(field_name)).cloned();
            let lowered = if let Some(expected_ty) = expected_ty {
                self.lower_expr_with_expected_type(field_expr, &expected_ty, active, blocks, vars)
            } else {
                self.lower_expr(field_expr, active, blocks, vars)
            };
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;

            let field_ty = match &lowered.operand {
                IrOperand::Var(var) => var.ty.clone(),
                IrOperand::Const(value) => self.const_type(value),
            };
            let field_var = self.new_var(format!("{}_{}", init.ty, field_name), field_ty);
            field_operands.insert(field_name.clone(), lowered.operand.clone());
            self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: lowered.operand });
            field_map.insert(field_name.clone(), field_var);
        }

        let Some(active) = self.lower_type_validity_checks(&init.ty, &field_map, active, blocks, vars) else {
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };

        // Evaluate initializers in source order above, then serialize in the
        // declared layout order. Sorting by byte offset loses zero-width ties.
        let Some(order) = self.type_field_order.get(&init.ty) else {
            self.errors
                .borrow_mut()
                .push(CompileError::without_span(format!("struct '{}' lacks its declaration field order", init.ty)));
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        };
        let tuple_operands = order.iter().filter_map(|name| field_operands.remove(name)).collect::<Vec<_>>();
        if tuple_operands.len() != order.len() || !field_operands.is_empty() {
            self.errors
                .borrow_mut()
                .push(CompileError::without_span(format!("struct '{}' initializer fields differ from its declaration", init.ty)));
            return LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: None };
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Tuple { dest: aggregate.clone(), fields: tuple_operands });
        self.aggregate_fields.insert(aggregate.id, field_map);
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    pub(super) fn lower_field_access(
        &mut self,
        field: &FieldAccessExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_base = self.lower_expr(&field.expr, current, blocks, vars);
        let Some(active) = lowered_base.current else {
            return lowered_base;
        };

        if field.field == "0"
            && let IrOperand::Const(IrConst::Hash(bytes) | IrConst::Address(bytes)) = &lowered_base.operand
        {
            return LoweredExpr {
                operand: IrOperand::Const(IrConst::Array(bytes.iter().copied().map(IrConst::U8).collect())),
                current: Some(active),
            };
        }

        if let IrOperand::Var(base_var) = &lowered_base.operand {
            if let IrType::Selection(owner) = &base_var.ty
                && let Some(projected) = owner.project_role(&field.field)
            {
                let dest = self.new_var(&field.field, IrType::Selection(Box::new(projected)));
                self.block_mut(blocks, active).instructions.push(IrInstruction::FieldAccess {
                    dest: dest.clone(),
                    obj: lowered_base.operand,
                    field: field.field.clone(),
                });
                return LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) };
            }
            if matches!(base_var.ty, IrType::U64) && (field.field == "lock" || field.field == "type" || field.field == "type_script") {
                let script_ref_ty = if field.field == "lock" {
                    IrType::Named(CKB_LOCK_SCRIPT_REF_TYPE.to_string())
                } else {
                    IrType::Named(CKB_TYPE_SCRIPT_REF_TYPE.to_string())
                };
                let dest = self.new_var(format!("{}_script_ref", field.field), script_ref_ty);
                self.block_mut(blocks, active)
                    .instructions
                    .push(IrInstruction::Move { dest: dest.clone(), src: lowered_base.operand });
                return LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) };
            }

            if let Some((func, dest_name, return_ty)) = script_ref_property_runtime_helper(&base_var.ty, &field.field) {
                let dest = self.new_var(dest_name, return_ty);
                self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                    dest: Some(dest.clone()),
                    func: crate::ir::IrCallTarget::runtime_symbol(func),
                    args: vec![lowered_base.operand],
                });
                return LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) };
            }

            if field.field == "size"
                && let Some((_, owner, maximum)) = witness_bytes_view_parts(&base_var.ty)
            {
                let dest = self.new_var("bounded_witness_size", IrType::U64);
                self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                    dest: Some(dest.clone()),
                    func: crate::ir::IrCallTarget::Runtime(
                        cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBoundedSize,
                    ),
                    args: vec![lowered_base.operand, IrOperand::Const(IrConst::U64(owner)), IrOperand::Const(IrConst::U64(maximum))],
                });
                return LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) };
            }

            if let Some((func, dest_name, return_ty)) = typed_view_property_runtime_helper(&base_var.ty, &field.field) {
                let dest = self.new_var(dest_name, return_ty);
                self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                    dest: Some(dest.clone()),
                    func: crate::ir::IrCallTarget::runtime_symbol(func),
                    args: vec![lowered_base.operand],
                });
                return LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) };
            }

            if let Some(wrapper_ty) = typed_view_property_wrapper(&base_var.ty, &field.field) {
                let dest = self.new_var(format!("{}_view", field.field), IrType::Named(wrapper_ty.to_string()));
                self.block_mut(blocks, active)
                    .instructions
                    .push(IrInstruction::Move { dest: dest.clone(), src: lowered_base.operand });
                return LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) };
            }

            if !self.is_mutable_value_aggregate(base_var)
                && let Some(fields) = self.aggregate_fields.get(&base_var.id)
                && let Some(field_var) = fields.get(&field.field)
                && self.aggregate_field_read_dominates(field_var.id)
            {
                return LoweredExpr { operand: IrOperand::Var(field_var.clone()), current: Some(active) };
            }

            if let Some(field_var) = self.materialize_schema_field(base_var, &field.field, active, blocks) {
                return LoweredExpr { operand: IrOperand::Var(field_var), current: Some(active) };
            }
        }

        self.record_error(format!("field access '.{}' has no lowered schema-backed representation", field.field), field.span);
        LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) }
    }

    pub(super) fn lower_field_assign(
        &mut self,
        field: &FieldAccessExpr,
        op: AssignOp,
        value_expr: &Expr,
        value: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_base = self.lower_expr(&field.expr, current, blocks, vars);
        let Some(active) = lowered_base.current else {
            return lowered_base;
        };

        let Some(base_var) = (match lowered_base.operand {
            IrOperand::Var(var) => Some(var),
            IrOperand::Const(_) => None,
        }) else {
            return LoweredExpr { operand: value, current: Some(active) };
        };

        let field_var = self
            .aggregate_fields
            .get(&base_var.id)
            .and_then(|fields| fields.get(&field.field))
            .cloned()
            .or_else(|| self.materialize_schema_field(&base_var, &field.field, active, blocks));
        let Some(field_var) = field_var else {
            self.record_error(format!("field assignment '.{}' has no lowered schema-backed representation", field.field), field.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        self.mutated_fields.entry(base_var.id).or_default().insert(field.field.clone());
        if let Some(transition) = self.mutate_transition_from_assignment(field, op, value_expr, &value, vars) {
            self.mutated_field_transitions.entry(base_var.id).or_default().insert(field.field.clone(), transition);
        }

        match op {
            AssignOp::Assign => {
                self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: field_var.clone(), src: value });
            }
            AssignOp::AddAssign => {
                let tmp = self.new_var("field_assign_tmp", field_var.ty.clone());
                let block = self.block_mut(blocks, active);
                block.instructions.push(IrInstruction::Binary {
                    dest: tmp.clone(),
                    op: BinaryOp::Add,
                    left: IrOperand::Var(field_var.clone()),
                    right: value,
                });
                block.instructions.push(IrInstruction::Move { dest: field_var.clone(), src: IrOperand::Var(tmp) });
            }
        }

        LoweredExpr { operand: IrOperand::Var(field_var), current: Some(active) }
    }

    fn mutate_transition_from_assignment(
        &self,
        target: &FieldAccessExpr,
        assign_op: AssignOp,
        value_expr: &Expr,
        value_operand: &IrOperand,
        vars: &HashMap<String, IrVar>,
    ) -> Option<MutateFieldTransition> {
        let (target_root, target_field) = direct_field_access_root(target)?;
        let (op, operand) = match assign_op {
            AssignOp::AddAssign => (
                MutateTransitionOp::Add,
                self.transition_operand_from_expr(value_expr, vars)
                    .or_else(|| self.transition_expr_is_coverable_u64(value_expr, vars).then_some(value_operand.clone()))?,
            ),
            AssignOp::Assign => match value_expr {
                Expr::Binary(binary) if matches!(binary.op, BinaryOp::Add | BinaryOp::Sub) => {
                    let left_is_old = same_direct_field_access(&binary.left, target_root, target_field);
                    let right_is_old = same_direct_field_access(&binary.right, target_root, target_field);
                    match (binary.op, left_is_old, right_is_old) {
                        (BinaryOp::Add, true, false) => {
                            (MutateTransitionOp::Add, self.transition_operand_from_expr(&binary.right, vars)?)
                        }
                        (BinaryOp::Add, false, true) => {
                            (MutateTransitionOp::Add, self.transition_operand_from_expr(&binary.left, vars)?)
                        }
                        (BinaryOp::Sub, true, false) => {
                            (MutateTransitionOp::Sub, self.transition_operand_from_expr(&binary.right, vars)?)
                        }
                        _ => return None,
                    }
                }
                _ => (MutateTransitionOp::Set, self.transition_operand_from_expr(value_expr, vars)?),
            },
        };
        Some(MutateFieldTransition { field: target_field.to_string(), op, operand })
    }

    fn transition_operand_from_expr(&self, expr: &Expr, vars: &HashMap<String, IrVar>) -> Option<IrOperand> {
        match expr {
            Expr::Identifier(name) => vars
                .get(name)
                .filter(|var| self.transition_param_ids.contains(&var.id) || self.transition_coverable_value_ids.contains(&var.id))
                .cloned()
                .map(IrOperand::Var),
            Expr::Integer(value) => u64::try_from(*value).ok().map(|value| IrOperand::Const(IrConst::U64(value))),
            Expr::FieldAccess(field) => {
                let (root, field_name) = direct_field_access_root(field)?;
                let root_var = vars.get(root)?;
                if !self.transition_param_ids.contains(&root_var.id) {
                    return None;
                }
                self.aggregate_fields
                    .get(&root_var.id)?
                    .get(field_name)
                    .cloned()
                    .filter(|field| self.aggregate_field_read_dominates(field.id))
                    .map(IrOperand::Var)
            }
            Expr::Cast(cast) => self.transition_operand_from_expr(&cast.expr, vars),
            _ => None,
        }
    }

    pub(super) fn transition_expr_is_coverable_u64(&self, expr: &Expr, vars: &HashMap<String, IrVar>) -> bool {
        match expr {
            Expr::Identifier(name) => vars.get(name).is_some_and(|var| {
                var.ty == IrType::U64
                    && (self.transition_param_ids.contains(&var.id) || self.transition_coverable_value_ids.contains(&var.id))
            }),
            Expr::Integer(value) => u64::try_from(*value).is_ok(),
            Expr::FieldAccess(field) => {
                let Some((root, field_name)) = direct_field_access_root(field) else {
                    return false;
                };
                let Some(root_var) = vars.get(root) else {
                    return false;
                };
                self.transition_param_ids.contains(&root_var.id)
                    && self
                        .lookup_field_ir_type(&root_var.ty, field_name)
                        .is_some_and(|ty| matches!(ty, IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64))
            }
            Expr::Cast(cast) => self.transition_expr_is_coverable_u64(&cast.expr, vars),
            Expr::Call(call) if call.args.is_empty() => match call.func.as_ref() {
                Expr::FieldAccess(field) if field.field == "len" => match field.expr.as_ref() {
                    Expr::Identifier(name) => vars.get(name).is_some_and(|var| {
                        (matches!(&var.ty, IrType::Named(type_name) if type_name == "String")
                            || matches!(&var.ty, IrType::Array(_, _) | IrType::Vector(_)))
                            && (self.transition_param_ids.contains(&var.id) || self.transition_coverable_value_ids.contains(&var.id))
                    }),
                    _ => false,
                },
                _ => false,
            },
            Expr::Binary(binary) if matches!(binary.op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div) => {
                self.transition_expr_is_coverable_u64(&binary.left, vars) && self.transition_expr_is_coverable_u64(&binary.right, vars)
            }
            Expr::Call(call)
                if call.args.len() == 2
                    && call_target_is_min(call.func.as_ref())
                    && matches!(call.func.as_ref(), Expr::Identifier(name) if self.call_param_types(name, &self.lower_call_target_name(name)).is_none()) =>
            {
                call.args.iter().all(|arg| self.transition_expr_is_coverable_u64(arg, vars))
            }
            _ => false,
        }
    }

    pub(super) fn lower_index_assign(
        &mut self,
        index: &IndexExpr,
        op: AssignOp,
        value: IrOperand,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered_arr = self.lower_expr(&index.expr, current, blocks, vars);
        let Some(active) = lowered_arr.current else {
            return lowered_arr;
        };
        let lowered_idx = self.lower_expr(&index.index, active, blocks, vars);
        let Some(active) = lowered_idx.current else {
            return lowered_idx;
        };

        let Some(arr_var) = (match lowered_arr.operand {
            IrOperand::Var(var) => Some(var),
            IrOperand::Const(_) => None,
        }) else {
            self.record_error("index assignment requires a local fixed-array value", index.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        let Some(index_value) = const_usize_operand(&lowered_idx.operand) else {
            self.record_error("local fixed-array assignment requires a compile-time constant index", index.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        let Some(elements) = self.aggregate_elements.get(&arr_var.id) else {
            self.record_error("index assignment requires a local fixed-array value with lowered element slots", index.span);
            return LoweredExpr { operand: value, current: Some(active) };
        };
        let Some(element_var) = elements.get(index_value).cloned() else {
            self.record_error(
                format!("array index {} is out of bounds for local fixed array of length {}", index_value, elements.len()),
                index.span,
            );
            return LoweredExpr { operand: value, current: Some(active) };
        };

        match op {
            AssignOp::Assign => {
                self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: element_var.clone(), src: value });
            }
            AssignOp::AddAssign => {
                let tmp = self.new_var("index_assign_tmp", element_var.ty.clone());
                let block = self.block_mut(blocks, active);
                block.instructions.push(IrInstruction::Binary {
                    dest: tmp.clone(),
                    op: BinaryOp::Add,
                    left: IrOperand::Var(element_var.clone()),
                    right: value,
                });
                block.instructions.push(IrInstruction::Move { dest: element_var.clone(), src: IrOperand::Var(tmp) });
            }
        }

        LoweredExpr { operand: IrOperand::Var(element_var), current: Some(active) }
    }
}
