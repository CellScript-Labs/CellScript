use super::{
    fixed_byte_width_for_script_args_operand, fixed_byte_width_for_script_args_var, BlockId, IrBlock, IrConst, IrGenerator,
    IrInstruction, IrOperand, IrType, IrVar, LoweredExpr, CKB_SCRIPT_ARGS_TYPE, CKB_SCRIPT_HASH_TYPE, CKB_SCRIPT_VALUE_TYPE,
};
use crate::ast::{CallExpr, Expr};
use crate::error::Span;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_script_args_empty(&mut self, current: BlockId, blocks: &mut [IrBlock]) -> LoweredExpr {
        let aggregate = self.new_var("script_args", IrType::Named(CKB_SCRIPT_ARGS_TYPE.to_string()));
        let bytes = self.new_var("script_args_bytes", IrType::Array(Box::new(IrType::U8), 0));
        let len = self.new_var("script_args_len", IrType::U64);
        let is_empty = self.new_var("script_args_is_empty", IrType::Bool);
        let block = self.block_mut(blocks, current);
        block.instructions.push(IrInstruction::LoadConst { dest: bytes.clone(), value: IrConst::Array(Vec::new()) });
        block.instructions.push(IrInstruction::LoadConst { dest: len.clone(), value: IrConst::U64(0) });
        block.instructions.push(IrInstruction::LoadConst { dest: is_empty.clone(), value: IrConst::Bool(true) });
        block.instructions.push(IrInstruction::Tuple {
            dest: aggregate.clone(),
            fields: vec![IrOperand::Var(bytes.clone()), IrOperand::Var(len.clone()), IrOperand::Var(is_empty.clone())],
        });
        self.aggregate_fields.insert(
            aggregate.id,
            HashMap::from([("bytes".to_string(), bytes), ("len".to_string(), len), ("is_empty".to_string(), is_empty)]),
        );
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(current) }
    }

    pub(super) fn lower_hash_from_bytes(
        &mut self,
        raw: &Expr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> LoweredExpr {
        if let Expr::ByteString(bytes) = raw {
            if bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(bytes);
                return LoweredExpr { operand: IrOperand::Const(IrConst::Hash(hash)), current: Some(current) };
            }
            self.record_error("Hash::from_bytes expects exactly 32 bytes ([u8; 32])", span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(current) };
        }

        let lowered = self.lower_expr(raw, current, blocks, vars);
        let Some(active) = lowered.current else {
            return lowered;
        };
        let raw_ty = self.operand_type(&lowered.operand);
        if !matches!(raw_ty, IrType::Array(inner, 32) if matches!(inner.as_ref(), IrType::U8)) {
            self.record_error("Hash::from_bytes expects exactly 32 bytes ([u8; 32])", span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        }

        let hash = self.new_var("hash_from_bytes", IrType::Hash);
        // The explicit constructor wraps one complete byte value. A Move would
        // erase the conversion and incorrectly claim implicit assignability.
        self.block_mut(blocks, active)
            .instructions
            .push(IrInstruction::Tuple { dest: hash.clone(), fields: vec![lowered.operand.clone()] });
        self.copy_aggregate_metadata(&lowered.operand, hash.id);
        LoweredExpr { operand: IrOperand::Var(hash), current: Some(active) }
    }

    pub(super) fn lower_script_args(
        &mut self,
        raw: &Expr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let lowered = match raw {
            Expr::ByteString(bytes) => LoweredExpr {
                operand: IrOperand::Const(IrConst::Array(bytes.iter().copied().map(IrConst::U8).collect())),
                current: Some(current),
            },
            _ => self.lower_expr(raw, current, blocks, vars),
        };
        let Some(active) = lowered.current else {
            return lowered;
        };
        let raw_ty = self.operand_type(&lowered.operand);
        let len_value = fixed_byte_width_for_script_args_operand(&lowered.operand, &raw_ty).unwrap_or(0) as u64;

        let aggregate = self.new_var("script_args", IrType::Named(CKB_SCRIPT_ARGS_TYPE.to_string()));
        let bytes = self.new_var("script_args_bytes", raw_ty);
        let len = self.new_var("script_args_len", IrType::U64);
        let is_empty = self.new_var("script_args_is_empty", IrType::Bool);
        let block = self.block_mut(blocks, active);
        block.instructions.push(IrInstruction::Move { dest: bytes.clone(), src: lowered.operand.clone() });
        block.instructions.push(IrInstruction::LoadConst { dest: len.clone(), value: IrConst::U64(len_value) });
        block.instructions.push(IrInstruction::LoadConst { dest: is_empty.clone(), value: IrConst::Bool(len_value == 0) });
        block.instructions.push(IrInstruction::Tuple {
            dest: aggregate.clone(),
            fields: vec![IrOperand::Var(bytes.clone()), IrOperand::Var(len.clone()), IrOperand::Var(is_empty.clone())],
        });
        self.copy_aggregate_metadata(&lowered.operand, bytes.id);
        self.aggregate_fields.insert(
            aggregate.id,
            HashMap::from([("bytes".to_string(), bytes), ("len".to_string(), len), ("is_empty".to_string(), is_empty)]),
        );
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    pub(super) fn lower_script_value(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let mut active = current;
        let code_hash = self.lower_expr(&call.args[0], active, blocks, vars);
        active = match code_hash.current {
            Some(next) => next,
            None => return code_hash,
        };
        let hash_type = self.lower_expr(&call.args[1], active, blocks, vars);
        active = match hash_type.current {
            Some(next) => next,
            None => return hash_type,
        };
        let args = self.lower_expr(&call.args[2], active, blocks, vars);
        active = match args.current {
            Some(next) => next,
            None => return args,
        };

        let aggregate = self.new_var("script_value", IrType::Named(CKB_SCRIPT_VALUE_TYPE.to_string()));
        let code_hash_field = self.new_var("script_code_hash", IrType::Hash);
        let hash_type_field = self.new_var("script_hash_type", IrType::U64);
        let args_field = self.new_var("script_args", IrType::Named(CKB_SCRIPT_ARGS_TYPE.to_string()));
        let block = self.block_mut(blocks, active);
        block.instructions.push(IrInstruction::Move { dest: code_hash_field.clone(), src: code_hash.operand.clone() });
        block.instructions.push(IrInstruction::Move { dest: hash_type_field.clone(), src: hash_type.operand.clone() });
        block.instructions.push(IrInstruction::Move { dest: args_field.clone(), src: args.operand.clone() });
        block.instructions.push(IrInstruction::Tuple {
            dest: aggregate.clone(),
            fields: vec![
                IrOperand::Var(code_hash_field.clone()),
                IrOperand::Var(hash_type_field.clone()),
                IrOperand::Var(args_field.clone()),
            ],
        });
        self.copy_aggregate_metadata(&args.operand, args_field.id);
        self.aggregate_fields.insert(
            aggregate.id,
            HashMap::from([
                ("code_hash".to_string(), code_hash_field),
                ("hash_type".to_string(), hash_type_field),
                ("args".to_string(), args_field),
            ]),
        );
        LoweredExpr { operand: IrOperand::Var(aggregate), current: Some(active) }
    }

    pub(super) fn lower_script_match_requirement(
        &mut self,
        call: &CallExpr,
        lock_script: bool,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let source = self.lower_expr(&call.args[0], current, blocks, vars);
        let Some(active) = source.current else {
            return source;
        };
        let script = self.lower_expr(&call.args[1], active, blocks, vars);
        let Some(active) = script.current else {
            return script;
        };
        let Some(script_var) = (match &script.operand {
            IrOperand::Var(var) => Some(var.clone()),
            IrOperand::Const(_) => None,
        }) else {
            self.record_error("script::require_cell_*_matches requires a constructed Script value", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) };
        };
        let Some(fields) = self.aggregate_fields.get(&script_var.id).cloned() else {
            self.record_error(
                "script::require_cell_*_matches requires a Script constructed by script::new in this verifier path",
                call.span,
            );
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) };
        };
        let Some(code_hash) = fields.get("code_hash").cloned() else {
            self.record_error("constructed Script is missing code_hash", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) };
        };
        let Some(hash_type) = fields.get("hash_type").cloned() else {
            self.record_error("constructed Script is missing hash_type", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) };
        };
        let Some(args) = fields.get("args").cloned() else {
            self.record_error("constructed Script is missing args", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) };
        };
        let Some(args_fields) = self.aggregate_fields.get(&args.id).cloned() else {
            self.record_error("constructed Script args must come from script::args or script::args_empty", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) };
        };
        let Some(args_bytes) = args_fields.get("bytes").cloned() else {
            self.record_error("constructed ScriptArgs is missing bytes", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) };
        };
        let args_width = fixed_byte_width_for_script_args_var(&args_bytes).unwrap_or(usize::MAX);

        let identity_helper =
            if lock_script { "__ckb_require_cell_lock_script_hash_type" } else { "__ckb_require_cell_type_script_hash_type" };
        let block = self.block_mut(blocks, active);
        block.instructions.push(IrInstruction::Call {
            dest: None,
            func: crate::ir::IrCallTarget::runtime_symbol(identity_helper),
            args: vec![source.operand.clone(), IrOperand::Var(code_hash), IrOperand::Var(hash_type)],
        });
        if args_width == 0 {
            block.instructions.push(IrInstruction::Call {
                dest: None,
                func: crate::ir::IrCallTarget::runtime_symbol(
                    &(if lock_script {
                        "__ckb_require_cell_lock_args_empty".to_string()
                    } else {
                        "__ckb_require_cell_type_args_empty".to_string()
                    }),
                ),
                args: vec![source.operand],
            });
        } else {
            block.instructions.push(IrInstruction::Call {
                dest: None,
                func: crate::ir::IrCallTarget::runtime_symbol(
                    &(if lock_script {
                        "__ckb_require_cell_lock_args_exact".to_string()
                    } else {
                        "__ckb_require_cell_type_args_exact".to_string()
                    }),
                ),
                args: vec![source.operand, IrOperand::Var(args_bytes)],
            });
        }
        LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) }
    }

    pub(super) fn lower_script_hash(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> LoweredExpr {
        let script = self.lower_expr(&call.args[0], current, blocks, vars);
        let Some(active) = script.current else {
            return script;
        };
        let Some(script_var) = (match &script.operand {
            IrOperand::Var(var) => Some(var.clone()),
            IrOperand::Const(_) => None,
        }) else {
            self.record_error("script::hash requires a constructed Script value", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        };
        let Some(fields) = self.aggregate_fields.get(&script_var.id).cloned() else {
            self.record_error("script::hash requires a Script constructed by script::new in this verifier path", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        };
        let Some(code_hash) = fields.get("code_hash").cloned() else {
            self.record_error("constructed Script is missing code_hash", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        };
        let Some(hash_type) = fields.get("hash_type").cloned() else {
            self.record_error("constructed Script is missing hash_type", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        };
        let Some(args) = fields.get("args").cloned() else {
            self.record_error("constructed Script is missing args", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        };
        let Some(args_fields) = self.aggregate_fields.get(&args.id).cloned() else {
            self.record_error("constructed Script args must come from script::args or script::args_empty", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        };
        let Some(args_bytes) = args_fields.get("bytes").cloned() else {
            self.record_error("constructed ScriptArgs is missing bytes", call.span);
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        };
        let args_width = fixed_byte_width_for_script_args_var(&args_bytes).unwrap_or(usize::MAX);
        if args_width > crate::CKB_SCRIPT_HASH_MAX_ARGS_BYTES {
            self.record_error(
                format!(
                    "script::hash args length {} exceeds the bounded maximum of {} bytes",
                    args_width,
                    crate::CKB_SCRIPT_HASH_MAX_ARGS_BYTES
                ),
                call.span,
            );
            return LoweredExpr { operand: IrOperand::Const(IrConst::Hash([0; 32])), current: Some(active) };
        }

        let dest = self.new_var("script_hash_bytes", IrType::Hash);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: Some(dest.clone()),
            func: crate::ir::IrCallTarget::Runtime(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbScriptHash),
            args: vec![IrOperand::Var(code_hash), IrOperand::Var(hash_type), IrOperand::Var(args_bytes)],
        });
        let semantic = self.new_var("script_hash", IrType::Named(CKB_SCRIPT_HASH_TYPE.to_string()));
        self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: semantic.clone(), src: IrOperand::Var(dest) });
        LoweredExpr { operand: IrOperand::Var(semantic), current: Some(active) }
    }
}
