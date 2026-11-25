use super::{
    BlockId, CellMetadataField, IrBlock, IrConst, IrEnforcedClaim, IrGenerator, IrInstruction, IrOperand, IrSourceLockTreatment,
    IrSuccessorRelation, IrTerminator, IrVar, LoweredExpr,
};
use crate::ast::{
    BinaryExpr, BinaryOp, CreateExpr, Expr, FieldAccessExpr, PreserveExpr, ReplaceDataTreatment, ReplaceFieldTreatment,
    ReplaceLockTreatment, ReplaceRelation, RequireBlockExpr, RequireExpr, StdlibCallExpr,
};
use crate::runtime_errors::CellScriptRuntimeError;
use std::collections::HashMap;

impl IrGenerator<'_> {
    /// Lower `require { expr1, expr2, ... }` — desugar into independent atomic `require` statements.
    pub(super) fn lower_require_block_expr(
        &mut self,
        require_block: &RequireBlockExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let mut active = current;
        for expr in &require_block.expressions {
            // Each expression in a require block is treated as a require condition:
            // require_block { expr1, expr2 } desugars to require expr1; require expr2;
            let lowered = self.lower_expr(expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            let cond = lowered.operand;

            let ok_block = self.push_block(blocks);
            let fail_block = self.push_block(blocks);
            self.block_mut(blocks, next).terminator =
                IrTerminator::Branch { cond: cond.clone(), then_block: ok_block, else_block: fail_block };
            self.block_mut(blocks, fail_block).terminator = IrTerminator::Return(Some(self.fail_closed_return_operand()));
            self.block_mut(blocks, fail_block).runtime_error = Some(CellScriptRuntimeError::AssertionFailed);
            self.enforced_claims.push(IrEnforcedClaim {
                statement: crate::fmt::format_expression(expr),
                condition_block: next,
                success_block: ok_block,
                failure_block: fail_block,
                span: if expr.span().end > expr.span().start { expr.span() } else { require_block.span },
            });
            active = ok_block;
        }
        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
    }

    /// Lower `preserve output from input { field1, field2, ... }` — desugar into
    /// `require output.field1 == input.field1; require output.field2 == input.field2; ...`
    pub(super) fn lower_preserve_expr(
        &mut self,
        preserve: &PreserveExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let mut active = current;
        for field_name in &preserve.fields {
            let output_field = Expr::FieldAccess(FieldAccessExpr {
                expr: Box::new(Expr::Identifier(preserve.output_name.clone())),
                field: field_name.clone(),
                span: preserve.span,
            });
            let input_field = Expr::FieldAccess(FieldAccessExpr {
                expr: Box::new(Expr::Identifier(preserve.input_name.clone())),
                field: field_name.clone(),
                span: preserve.span,
            });
            let equality = Expr::Binary(BinaryExpr {
                op: BinaryOp::Eq,
                left: Box::new(output_field),
                right: Box::new(input_field),
                span: preserve.span,
            });
            let require_expr = RequireExpr { condition: Box::new(equality), message: None, span: preserve.span };
            let lowered = self.lower_require_expr(&require_expr, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
        }
        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
    }

    /// Elaborate an authoring successor relation into the same checked
    /// instructions as the spelled-out lifecycle operations: the predecessor is
    /// consumed exactly once, every concrete schema field becomes an explicit
    /// successor equality (or is bound by the create in the exact-lock form),
    /// and the identity, capacity and lock treatments lower to the canonical
    /// type-hash, capacity and lock-hash preservation checks.
    pub(super) fn lower_replace_relation(
        &mut self,
        relation: &ReplaceRelation,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let span = relation.span;
        let mut active = current;

        let resource_ty = vars
            .get(&relation.after)
            .and_then(|var| Self::named_type_name_from_ir_type(&var.ty).map(str::to_string))
            .unwrap_or_else(|| relation.after.clone());

        // 1. Account the predecessor through the ordinary consume kernel.
        let predecessor = Expr::Identifier(relation.before.clone());
        let lowered = self.lower_consumed_cell(&predecessor, active, blocks, vars);
        let Some(next) = lowered.current else {
            return lowered;
        };
        active = next;
        let consume_block = active;
        let input_local_id = match &lowered.operand {
            IrOperand::Var(var) => var.id,
            _ => {
                self.record_error("replace predecessor must retain its bound input local", span);
                return lowered;
            }
        };

        // 2. Resolve the data treatment into explicit successor field values.
        let before_expr = Expr::Identifier(relation.before.clone());
        let after_expr = Expr::Identifier(relation.after.clone());
        let mut field_values: Vec<(String, Expr)> = Vec::new();
        match &relation.data {
            ReplaceDataTreatment::Fields(treatments) => {
                for treatment in treatments {
                    match treatment {
                        ReplaceFieldTreatment::Same(field) => field_values.push((
                            field.clone(),
                            Expr::FieldAccess(FieldAccessExpr { expr: Box::new(before_expr.clone()), field: field.clone(), span }),
                        )),
                        ReplaceFieldTreatment::Assign(field, value) => {
                            field_values.push((field.clone(), value.clone()));
                        }
                    }
                }
            }
            ReplaceDataTreatment::SameExcept(assigned) => {
                // Sorted for deterministic expansion regardless of the
                // field map's iteration order.
                let mut schema_fields =
                    self.type_fields.get(&resource_ty).map(|fields| fields.keys().cloned().collect::<Vec<_>>()).unwrap_or_default();
                schema_fields.sort();
                for field in schema_fields {
                    if let Some((_, value)) = assigned.iter().find(|(name, _)| name == &field) {
                        field_values.push((field, value.clone()));
                    } else {
                        field_values.push((
                            field.clone(),
                            Expr::FieldAccess(FieldAccessExpr { expr: Box::new(before_expr.clone()), field, span }),
                        ));
                    }
                }
                for (field, _) in assigned {
                    if !field_values.iter().any(|(name, _)| name == field) {
                        self.record_error(
                            format!("replace relation `same except` field '{field}' does not exist on resource '{resource_ty}'"),
                            span,
                        );
                    }
                }
            }
        }

        // 3. Bind the successor. Both lock treatments go through the create
        //    kernel so resource conservation is enforced the same way: the
        //    exact form binds the lock target, the same form leaves the lock
        //    unconstrained by create and pins it with the lock-hash
        //    preservation check instead.
        let create = CreateExpr {
            target: Some(relation.after.clone()),
            ty: resource_ty.clone(),
            fields: field_values.clone(),
            lock: match &relation.lock {
                ReplaceLockTreatment::Exact(lock) | ReplaceLockTreatment::ExactHash(lock) => Some(lock.clone()),
                ReplaceLockTreatment::Same => None,
            },
            span,
        };
        let lowered = self.lower_create_expr(&create, active, blocks, vars);
        let Some(next) = lowered.current else {
            return lowered;
        };
        active = next;
        let create_block = active;
        let output_local_id = match &lowered.operand {
            IrOperand::Var(var) => var.id,
            _ => {
                self.record_error("replace successor must retain its bound output local", span);
                return lowered;
            }
        };

        if let ReplaceLockTreatment::Same = &relation.lock {
            // The lock is not bound by the create; pin it to the
            // predecessor's complete Lock Script hash instead. Conservation
            // now recognizes the updated-successor shape (verbatim aliases
            // plus verifier-checked u64 updates rooted in the consumed
            // input), so this stays executable under DenyFailClosed.
            let same_lock = StdlibCallExpr {
                namespace: "cell".to_string(),
                name: "same_lock".to_string(),
                args: vec![after_expr.clone(), before_expr.clone()],
                preserve_fields: Vec::new(),
                span,
            };
            let lowered = self.lower_stdlib_call(&same_lock, active, blocks, vars);
            let Some(next) = lowered.current else {
                return lowered;
            };
            active = next;
        }

        // 4. Complete Type Script hash equality is independent of a business
        // object's declared logical identity policy.
        let same_type = StdlibCallExpr {
            namespace: "cell".to_string(),
            name: "same_type".to_string(),
            args: vec![after_expr.clone(), before_expr.clone()],
            preserve_fields: Vec::new(),
            span,
        };
        let lowered = self.lower_stdlib_call(&same_type, active, blocks, vars);
        let Some(next) = lowered.current else {
            return lowered;
        };
        active = next;

        // 5. Capacity: exact equality with the predecessor.
        let preserve_capacity = StdlibCallExpr {
            namespace: "cell".to_string(),
            name: "preserve_capacity".to_string(),
            args: vec![after_expr, before_expr],
            preserve_fields: Vec::new(),
            span,
        };
        let lowered = self.lower_stdlib_call(&preserve_capacity, active, blocks, vars);
        let Some(next) = lowered.current else {
            return lowered;
        };
        active = next;

        let data_fields = field_values
            .iter()
            .map(|(field, _)| {
                let treatment = match &relation.data {
                    ReplaceDataTreatment::Fields(fields) => fields
                        .iter()
                        .find_map(|treatment| match treatment {
                            ReplaceFieldTreatment::Same(name) if name == field => Some("preserve".to_string()),
                            ReplaceFieldTreatment::Assign(name, value) if name == field => {
                                Some(format!("set-from-expression:{}", crate::fmt::format_expression(value)))
                            }
                            _ => None,
                        })
                        .unwrap_or_default(),
                    ReplaceDataTreatment::SameExcept(assigned) => assigned
                        .iter()
                        .find(|(name, _)| name == field)
                        .map(|(_, value)| format!("set-from-expression:{}", crate::fmt::format_expression(value)))
                        .unwrap_or_else(|| "preserve".to_string()),
                };
                (field.clone(), treatment)
            })
            .collect();
        self.successor_relations.push(IrSuccessorRelation {
            occurrence: self.successor_relations.len(),
            input_binding: relation.before.clone(),
            output_binding: relation.after.clone(),
            input_local_id,
            output_local_id,
            data_fields,
            lock_treatment: IrSourceLockTreatment::from_source(&relation.lock),
            lock_expression: relation.lock_expr().map(crate::fmt::format_expression),
            consume_block,
            create_block,
            success_block: active,
            span,
        });
        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
    }

    pub(super) fn lower_cell_metadata_equality_call(
        &mut self,
        qualified: &str,
        call: &StdlibCallExpr,
        field: CellMetadataField,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        if call.args.len() != 2 {
            self.record_error(format!("{} expects 2 arguments (output, input), got {}", qualified, call.args.len()), call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(current) };
        }

        let left = self.lower_expr(&call.args[0], current, blocks, vars);
        let Some(active) = left.current else {
            return left;
        };
        let right = self.lower_expr(&call.args[1], active, blocks, vars);
        let Some(active) = right.current else {
            return right;
        };

        self.block_mut(blocks, active).instructions.push(IrInstruction::CellMetadataEquality {
            left: left.operand,
            right: right.operand,
            field,
        });
        LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
    }

    pub(super) fn lower_stdlib_output_create_and_preserve(
        &mut self,
        qualified: &str,
        input_role: &str,
        input: &Expr,
        output: &Expr,
        lock: Option<&Expr>,
        preserve_fields: &[String],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<BlockId> {
        let output_name = match output {
            Expr::Identifier(name) => name.clone(),
            _ => {
                self.record_error(format!("{} output must be a named Cell output binding", qualified), output.span());
                return None;
            }
        };
        let input_name = match input {
            Expr::Identifier(name) => name.clone(),
            _ => {
                self.record_error(format!("{} {} must be a named Cell input binding", qualified, input_role), input.span());
                return None;
            }
        };
        let Some(output_ty) = vars.get(&output_name).and_then(|var| Self::named_type_name_from_ir_type(&var.ty).map(str::to_string))
        else {
            self.record_error(format!("{} output must be a named Cell output binding", qualified), output.span());
            return None;
        };

        let create_fields = preserve_fields
            .iter()
            .map(|field| {
                (
                    field.clone(),
                    Expr::FieldAccess(FieldAccessExpr { expr: Box::new(input.clone()), field: field.clone(), span: input.span() }),
                )
            })
            .collect::<Vec<_>>();
        let create_expr = CreateExpr {
            target: Some(output_name.clone()),
            ty: output_ty,
            fields: create_fields,
            lock: lock.cloned().map(Box::new),
            span: output.span(),
        };
        let lowered = self.lower_create_expr(&create_expr, current, blocks, vars);
        let mut active = lowered.current?;

        if !preserve_fields.is_empty() {
            let preserve = PreserveExpr { output_name, input_name, fields: preserve_fields.to_vec(), span: input.span() };
            let lowered = self.lower_preserve_expr(&preserve, active, blocks, vars);
            active = lowered.current?;
        }

        Some(active)
    }
}
