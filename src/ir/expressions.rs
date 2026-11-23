use super::call_target::IrCallTarget;
use super::{BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrType, IrVar, LoweredExpr};
use crate::ast::{BinaryOp, CallExpr, Expr};
use crate::error::Span;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_expr(
        &mut self,
        expr: &Expr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        match expr {
            Expr::Integer(value) => {
                LoweredExpr { operand: IrOperand::Const(Self::default_integer_const(*value)), current: Some(current) }
            }
            Expr::Bool(value) => LoweredExpr { operand: IrOperand::Const(IrConst::Bool(*value)), current: Some(current) },
            Expr::Identifier(name) => {
                if let Some(var) = vars.get(name).cloned() {
                    LoweredExpr { operand: IrOperand::Var(var), current: Some(current) }
                } else if let Some(constant) = self.lower_constant(name, Span::default()) {
                    let operand = if self.constants.get(name).is_some_and(|(_, ty)| *ty == IrType::I32) {
                        IrOperand::Var(self.materialize_operand_with_type(
                            name,
                            constant,
                            IrType::I32,
                            self.block_mut(blocks, current),
                        ))
                    } else {
                        constant
                    };
                    LoweredExpr { operand, current: Some(current) }
                } else if let Some(zero) = self.lower_zero_value(name) {
                    LoweredExpr { operand: zero, current: Some(current) }
                } else if let Some(lowered) = self.try_lower_enum_constructor_path(name, &[], current, blocks, vars, Span::default()) {
                    lowered
                } else if let Some((ty, enum_variant)) = self.lower_enum_variant(name) {
                    // An ordinal is not an enum identity. Keep its resolved
                    // owner on the SSA value for matches and typed consumers.
                    let var = self.materialize_operand_with_type(name, enum_variant, ty, self.block_mut(blocks, current));
                    LoweredExpr { operand: IrOperand::Var(var), current: Some(current) }
                } else if let Some(flow_state) = self.lower_flow_state_name(name) {
                    LoweredExpr { operand: flow_state, current: Some(current) }
                } else {
                    self.record_error(format!("IR lowering encountered unresolved identifier '{}'", name), Span::default());
                    LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
                }
            }
            Expr::Assign(assign) => self.lower_assign_expr(assign, current, blocks, vars),
            Expr::Binary(binary) => {
                let (left, right, active) = if let Expr::Integer(left_value) = binary.left.as_ref() {
                    let right = self.lower_expr(&binary.right, current, blocks, vars);
                    let Some(active) = right.current else {
                        return right;
                    };
                    let right_ty = self.operand_type(&right.operand);
                    let left = if matches!(binary.op, BinaryOp::Shl | BinaryOp::Shr) {
                        LoweredExpr { operand: IrOperand::Const(Self::default_integer_const(*left_value)), current: Some(active) }
                    } else {
                        self.integer_operand_for_expected_type(*left_value, &right_ty, self.block_mut(blocks, active))
                            .map(|operand| LoweredExpr { operand, current: Some(active) })
                            .unwrap_or_else(|| LoweredExpr {
                                operand: IrOperand::Const(Self::default_integer_const(*left_value)),
                                current: Some(active),
                            })
                    };
                    (left, right, active)
                } else {
                    let left = self.lower_expr(&binary.left, current, blocks, vars);
                    let Some(active) = left.current else {
                        return left;
                    };
                    let right = if let Expr::Integer(right_value) = binary.right.as_ref() {
                        let left_ty = self.operand_type(&left.operand);
                        if matches!(binary.op, BinaryOp::Shl | BinaryOp::Shr) {
                            LoweredExpr { operand: IrOperand::Const(Self::default_integer_const(*right_value)), current: Some(active) }
                        } else {
                            self.integer_operand_for_expected_type(*right_value, &left_ty, self.block_mut(blocks, active))
                                .map(|operand| LoweredExpr { operand, current: Some(active) })
                                .unwrap_or_else(|| LoweredExpr {
                                    operand: IrOperand::Const(Self::default_integer_const(*right_value)),
                                    current: Some(active),
                                })
                        }
                    } else {
                        self.lower_expr(&binary.right, active, blocks, vars)
                    };
                    let Some(active) = right.current else {
                        return right;
                    };
                    (left, right, active)
                };
                let dest = self.new_var("tmp", self.binary_result_type_for_operands(binary.op, &left.operand, &right.operand));
                let block = self.block_mut(blocks, active);
                block.instructions.push(IrInstruction::Binary {
                    dest: dest.clone(),
                    op: binary.op,
                    left: left.operand,
                    right: right.operand,
                });
                LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
            }
            Expr::Unary(unary) => {
                let operand = self.lower_expr(&unary.expr, current, blocks, vars);
                let Some(active) = operand.current else {
                    return operand;
                };
                let Some(result_type) = self.unary_result_type(unary.op, &operand.operand) else {
                    self.record_error("cannot lower dereference of a non-reference value", unary.span);
                    return operand;
                };
                let dest = self.new_var("tmp", result_type);
                let block = self.block_mut(blocks, active);
                block.instructions.push(IrInstruction::Unary { dest: dest.clone(), op: unary.op, operand: operand.operand });
                LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
            }
            Expr::Call(call) => {
                let source_binding = match call.func.as_ref() {
                    Expr::Identifier(name) => self.call_param_types(name, &self.lower_call_target_name(name)).is_some(),
                    _ => false,
                };
                if !source_binding {
                    if let Some(lowered) = self.lower_allocated_call(call, current, blocks, vars) {
                        return lowered;
                    }
                    if let Expr::Identifier(path) = call.func.as_ref()
                        && let Some(lowered) = self.try_lower_enum_constructor_path(path, &call.args, current, blocks, vars, call.span)
                    {
                        return lowered;
                    }
                    if let Some(lowered) = self.try_lower_builtin_call(call, current, blocks, vars) {
                        return lowered;
                    }
                }
                let func = match call.func.as_ref() {
                    Expr::Identifier(name) => self.lower_call_target_name(name),
                    Expr::FieldAccess(field) => field.field.clone(),
                    _ => "__expr_call".to_string(),
                };
                let source_func = match call.func.as_ref() {
                    Expr::Identifier(name) => name.as_str(),
                    Expr::FieldAccess(field) => field.field.as_str(),
                    _ => "__expr_call",
                };
                let param_types = self.call_param_types(source_func, &func);
                let mut active = current;
                let mut args = Vec::with_capacity(call.args.len());
                for (index, arg) in call.args.iter().enumerate() {
                    let lowered = if let Some(expected_ty) = param_types.as_ref().and_then(|params| params.get(index)) {
                        self.lower_expr_with_expected_type(arg, expected_ty, active, blocks, vars)
                    } else {
                        self.lower_expr(arg, active, blocks, vars)
                    };
                    let Some(next) = lowered.current else {
                        return lowered;
                    };
                    active = next;
                    args.push(lowered.operand);
                }
                match self.call_return_type(source_func, &func) {
                    Some(Some(return_type)) => {
                        let dest = self.new_var("call_tmp", return_type);
                        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                            dest: Some(dest.clone()),
                            func: IrCallTarget::Function(func),
                            args,
                        });
                        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
                    }
                    Some(None) => {
                        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
                            dest: None,
                            func: IrCallTarget::Function(func),
                            args,
                        });
                        LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) }
                    }
                    None => {
                        self.record_error(format!("call '{}' has no known return type during IR lowering", source_func), call.span);
                        LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(active) }
                    }
                }
            }
            Expr::ReadRef(read_ref) => self.lower_read_ref_expr(read_ref, current, blocks),
            Expr::Create(create) => self.lower_create_expr(create, current, blocks, vars),
            Expr::Consume(consume) => self.lower_consume_expr(consume, current, blocks, vars),
            Expr::Destroy(destroy) => self.lower_destroy_expr(destroy, current, blocks, vars),
            Expr::Claim(claim) => self.lower_claim_expr(claim, current, blocks, vars),
            Expr::Settle(settle) => self.lower_settle_expr(settle, current, blocks, vars),
            Expr::CreateUnique(cu) => self.lower_create_unique_expr(cu, current, blocks, vars),
            Expr::ReplaceUnique(ru) => self.lower_replace_unique_expr(ru, current, blocks, vars),
            Expr::Assert(assert_expr) => self.lower_assert_expr(assert_expr, current, blocks, vars),
            Expr::Require(require_expr) => self.lower_require_expr(require_expr, current, blocks, vars),
            Expr::RequireBlock(require_block) => self.lower_require_block_expr(require_block, current, blocks, vars),
            Expr::Preserve(preserve_expr) => self.lower_preserve_expr(preserve_expr, current, blocks, vars),
            Expr::ReplaceRelation(relation) => self.lower_replace_relation(relation, current, blocks, vars),
            Expr::StructInit(init) => self.lower_struct_init(init, current, blocks, vars),
            Expr::FieldAccess(field) => self.lower_field_access(field, current, blocks, vars),
            Expr::Index(index) => {
                if let Some(lowered) = self.lower_allocated_index(index, current, blocks, vars) {
                    lowered
                } else {
                    self.lower_index_expr(index, current, blocks, vars)
                }
            }
            Expr::Block(stmts) => self.lower_tail_block_value(stmts, current, blocks, vars),
            Expr::If(if_expr) => self.lower_if_expr(if_expr, current, blocks, vars),
            Expr::Match(match_expr) => self.lower_match_expr(match_expr, current, blocks, vars),
            Expr::Cast(cast) => self.lower_cast_expr(cast, current, blocks, vars),
            Expr::Array(items) => self.lower_array_expr(items, current, blocks, vars),
            Expr::Tuple(items) => self.lower_tuple_expr(items, current, blocks, vars),
            Expr::String(_) => {
                self.record_error("string literals are only supported in metadata positions such as assert messages", Span::default());
                LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
            }
            Expr::ByteString(bytes) => LoweredExpr {
                operand: IrOperand::Const(IrConst::Array(bytes.iter().copied().map(IrConst::U8).collect())),
                current: Some(current),
            },
            Expr::Range(_) => {
                self.record_error("range expressions are only supported as for-loop iterables", Span::default());
                LoweredExpr { operand: IrOperand::Const(IrConst::U64(0)), current: Some(current) }
            }
            Expr::StdlibCall(call) => self.lower_stdlib_call(call, current, blocks, vars),
        }
    }

    pub(super) fn lower_vec_constructor_expr(
        &mut self,
        call: &CallExpr,
        collection_type: IrType,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let (active, capacity) = if let Some(expr) = call.args.first() {
            let lowered = self.lower_expr(expr, current, blocks, vars);
            let Some(active) = lowered.current else { return lowered };
            (active, Some(lowered.operand))
        } else {
            (current, None)
        };
        let name = if capacity.is_some() { "vec_with_capacity_tmp" } else { "vec_new_tmp" };
        let dest = self.new_var(name, collection_type.clone());
        self.block_mut(blocks, active).instructions.push(IrInstruction::CollectionNew {
            dest: dest.clone(),
            ty: collection_type,
            capacity,
        });
        LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) }
    }
}
