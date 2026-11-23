use super::{BlockId, CellMetadataField, IrBlock, IrConst, IrGenerator, IrOperand, IrVar, LoweredExpr};
use crate::ast::{
    BinaryExpr, BinaryOp, CallExpr, ConsumeExpr, CreateExpr, Expr, FieldAccessExpr, PreserveExpr, RequireExpr, StdlibCallExpr,
};
use std::collections::HashMap;

impl IrGenerator<'_> {
    /// Lower a stdlib call expression by expanding it into core IR instructions.
    ///
    /// Constraint patterns expand into `require` constraints or canonical
    /// verifier metadata checks.
    ///
    /// Lifecycle patterns expand into `consume` plus explicit output and verifier constraints.
    pub(super) fn lower_stdlib_call(
        &mut self,
        call: &StdlibCallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let qualified = format!("std::{}::{}", call.namespace, call.name);
        let mut active = current;
        let Some(signature) = crate::stdlib::signature::lookup(&call.namespace, &call.name) else {
            self.record_error(
                format!("unknown stdlib pattern '{}' — each stdlib primitive must have a canonical signature", qualified),
                call.span,
            );
            return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
        };
        if call.args.len() != signature.arity {
            self.record_error(format!("{} expects {} arguments, got {}", qualified, signature.arity, call.args.len()), call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
        }
        if !signature.allows_preserve_fields && !call.preserve_fields.is_empty() {
            self.record_error(format!("{} does not accept a preserve-field block", qualified), call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
        }

        match qualified.as_str() {
            // Constraint patterns — expand to canonical verifier constraints
            "std::cell::same_lock" | "std::cell::preserve_lock" => {
                self.lower_cell_metadata_equality_call(&qualified, call, CellMetadataField::LockHash, active, blocks, vars)
            }
            "std::cell::same_type" | "std::cell::preserve_type" => {
                if call.args.len() != 2 {
                    self.record_error(
                        format!("{} expects 2 arguments (output, input), got {}", qualified, call.args.len()),
                        call.span,
                    );
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                }
                let output = &call.args[0];
                let input = &call.args[1];
                let output_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(output.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    type_args: Vec::new(),
                    args: vec![],
                    span: call.span,
                });
                let input_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(input.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    type_args: Vec::new(),
                    args: vec![],
                    span: call.span,
                });
                let equality = Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(output_type_hash),
                    right: Box::new(input_type_hash),
                    span: call.span,
                });
                let require_expr = RequireExpr { condition: Box::new(equality), message: None, span: call.span };
                self.lower_require_expr(&require_expr, active, blocks, vars)
            }
            "std::cell::preserve_capacity" => {
                self.lower_cell_metadata_equality_call(&qualified, call, CellMetadataField::Capacity, active, blocks, vars)
            }
            "std::accounting::conserved" => {
                if call.args.len() != 2 {
                    self.record_error(
                        format!("{} expects 2 arguments (output, input), got {}", qualified, call.args.len()),
                        call.span,
                    );
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                }
                let output = &call.args[0];
                let input = &call.args[1];
                let output_amount = Expr::FieldAccess(FieldAccessExpr {
                    expr: Box::new(output.clone()),
                    field: "amount".to_string(),
                    span: call.span,
                });
                let input_amount =
                    Expr::FieldAccess(FieldAccessExpr { expr: Box::new(input.clone()), field: "amount".to_string(), span: call.span });
                let equality = Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(output_amount),
                    right: Box::new(input_amount),
                    span: call.span,
                });
                let require_expr = RequireExpr { condition: Box::new(equality), message: None, span: call.span };
                self.lower_require_expr(&require_expr, active, blocks, vars)
            }

            // Lifecycle patterns — consume + explicit output and verifier constraints
            "std::lifecycle::transfer" => {
                if call.args.len() != 3 {
                    self.record_error(
                        format!("std::lifecycle::transfer expects 3 arguments (input, output, to), got {}", call.args.len()),
                        call.span,
                    );
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                }
                let input = &call.args[0];
                let output = &call.args[1];
                let lock = &call.args[2];

                // 1. consume input
                let consume_expr = ConsumeExpr { expr: Box::new(input.clone()), span: call.span };
                let lowered = self.lower_consume_expr(&consume_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 2. constrain the proposed output binding and lock target
                let output_name = match output {
                    Expr::Identifier(name) => name.clone(),
                    _ => {
                        self.record_error("std::lifecycle::transfer output must be a named Cell output binding", call.span);
                        return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                    }
                };
                let output_ty = vars
                    .get(&output_name)
                    .and_then(|var| Self::named_type_name_from_ir_type(&var.ty).map(str::to_string))
                    .unwrap_or_else(|| "output".to_string());
                let create_fields = call
                    .preserve_fields
                    .iter()
                    .map(|field| {
                        (
                            field.clone(),
                            Expr::FieldAccess(FieldAccessExpr {
                                expr: Box::new(input.clone()),
                                field: field.clone(),
                                span: call.span,
                            }),
                        )
                    })
                    .collect::<Vec<_>>();
                let create_expr = CreateExpr {
                    target: Some(output_name.clone()),
                    ty: output_ty,
                    fields: create_fields,
                    lock: Some(Box::new(lock.clone())),
                    span: call.span,
                };
                let lowered = self.lower_create_expr(&create_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 3. require output.type_hash == input.type_hash
                let output_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(output.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    type_args: Vec::new(),
                    args: vec![],
                    span: call.span,
                });
                let input_type_hash = Expr::Call(CallExpr {
                    func: Box::new(Expr::FieldAccess(FieldAccessExpr {
                        expr: Box::new(input.clone()),
                        field: "type_hash".to_string(),
                        span: call.span,
                    })),
                    type_args: Vec::new(),
                    args: vec![],
                    span: call.span,
                });
                let type_eq = Expr::Binary(BinaryExpr {
                    op: BinaryOp::Eq,
                    left: Box::new(output_type_hash),
                    right: Box::new(input_type_hash),
                    span: call.span,
                });
                let require_type = RequireExpr { condition: Box::new(type_eq), message: None, span: call.span };
                let lowered = self.lower_require_expr(&require_type, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 4. preserve listed fields from input to output
                if !call.preserve_fields.is_empty() {
                    let input_name = match input {
                        Expr::Identifier(name) => name.clone(),
                        _ => "input".to_string(),
                    };
                    let preserve = PreserveExpr { output_name, input_name, fields: call.preserve_fields.clone(), span: call.span };
                    let lowered = self.lower_preserve_expr(&preserve, active, blocks, vars);
                    let Some(next) = lowered.current else {
                        return lowered;
                    };
                    active = next;
                }

                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
            }
            "std::receipt::claim" => {
                if call.args.len() != 3 {
                    self.record_error(format!("std::receipt::claim expects 3 arguments, got {}", call.args.len()), call.span);
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                }
                let receipt = &call.args[0];

                // 1. consume receipt
                let consume_expr = ConsumeExpr { expr: Box::new(receipt.clone()), span: call.span };
                let lowered = self.lower_consume_expr(&consume_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 2. canonical output construction from receipt fields with an explicit lock target.
                let Some(next) = self.lower_stdlib_output_create_and_preserve(
                    "std::receipt::claim",
                    "receipt",
                    receipt,
                    &call.args[1],
                    call.args.get(2),
                    &call.preserve_fields,
                    active,
                    blocks,
                    vars,
                ) else {
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                };
                active = next;

                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
            }
            "std::lifecycle::settle" => {
                if call.args.len() != 3 {
                    self.record_error(format!("std::lifecycle::settle expects 3 arguments, got {}", call.args.len()), call.span);
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                }
                let input = &call.args[0];

                // 1. consume input
                let consume_expr = ConsumeExpr { expr: Box::new(input.clone()), span: call.span };
                let lowered = self.lower_consume_expr(&consume_expr, active, blocks, vars);
                let Some(next) = lowered.current else {
                    return lowered;
                };
                active = next;

                // 2. canonical output construction from settled fields with an explicit lock target.
                let Some(next) = self.lower_stdlib_output_create_and_preserve(
                    "std::lifecycle::settle",
                    "input",
                    input,
                    &call.args[1],
                    call.args.get(2),
                    &call.preserve_fields,
                    active,
                    blocks,
                    vars,
                ) else {
                    return LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) };
                };
                active = next;

                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(true)), current: Some(active) }
            }

            _ => {
                self.record_error(
                    format!(
                        "unknown stdlib pattern '{}' — each stdlib primitive must have a canonical expansion into core CellScript",
                        qualified
                    ),
                    call.span,
                );
                LoweredExpr { operand: IrOperand::Const(IrConst::Bool(false)), current: Some(active) }
            }
        }
    }
}
