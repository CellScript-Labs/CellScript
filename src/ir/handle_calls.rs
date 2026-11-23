use super::{BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrTrustedExternalCall, IrType, IrVar, LoweredExpr};
use crate::ast::Expr;
use crate::error::Span;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_trusted_external_call(
        &mut self,
        operation: &str,
        runtime_helper: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.lower_expr(arg, active, blocks, vars);
            active = value.current?;
            lowered.push(value.operand);
        }
        let code_hash = match lowered.get(1) {
            Some(IrOperand::Const(IrConst::Hash(hash))) => *hash,
            _ => {
                self.record_error("trusted external verifier code_hash must be a compile-time Hash literal", span);
                [0; 32]
            }
        };
        let source = self.new_var("trusted_external_cell_dep", IrType::context(crate::ast::ContextView::CellDep));
        self.block_mut(blocks, active).instructions.extend([
            IrInstruction::Call {
                dest: Some(source.clone()),
                func: crate::ir::IrCallTarget::Runtime(cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbSourceCellDep),
                args: vec![lowered[0].clone()],
            },
            IrInstruction::Call {
                dest: None,
                func: crate::ir::IrCallTarget::Runtime(
                    cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbRequireCellDataHash,
                ),
                args: vec![IrOperand::Var(source), lowered[1].clone()],
            },
            IrInstruction::Call {
                dest: None,
                func: crate::ir::IrCallTarget::runtime_symbol(runtime_helper),
                args: std::iter::once(lowered[0].clone()).chain(lowered[2..].iter().cloned()).collect(),
            },
        ]);
        let adapter = match runtime_helper {
            "__ckb_exec_cell_dep_u8_args" => "u8-args-v1",
            "__ckb_exec_cell_dep_hex4" | "__ckb_spawn_wait_cell_dep_hex4" => "hex4-v1",
            _ => unreachable!("trusted external calls use a closed runtime-helper set"),
        };
        self.trusted_external_calls.push(IrTrustedExternalCall {
            operation: operation.to_string(),
            adapter: adapter.to_string(),
            code_hash,
        });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
    }

    pub(super) fn lower_exact_script_handle_requirement(
        &mut self,
        runtime_helper: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.lower_expr(arg, active, blocks, vars);
            active = value.current?;
            lowered.push(value.operand);
        }
        if !matches!(lowered.get(2), Some(IrOperand::Const(IrConst::Hash(_)))) {
            self.record_error("exact Script handle handle_hash must be a compile-time Hash literal", span);
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: None,
            func: crate::ir::IrCallTarget::runtime_symbol(runtime_helper),
            args: lowered,
        });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
    }

    pub(super) fn lower_open_handle_from_args_requirement(
        &mut self,
        runtime_helper: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.lower_expr(arg, active, blocks, vars);
            active = value.current?;
            lowered.push(value.operand);
        }
        let participant = matches!(
            runtime_helper,
            "__ckb_require_open_script_participant_from_args" | "__ckb_require_instance_script_participant_from_args"
        );
        if lowered.len() != if participant { 3 } else { 2 } {
            self.record_error("open handle from-args requirement has an invalid operand count", span);
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: None,
            func: crate::ir::IrCallTarget::runtime_symbol(runtime_helper),
            args: lowered,
        });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
    }

    pub(super) fn lower_open_handle_requirement(
        &mut self,
        runtime_helper: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.lower_expr(arg, active, blocks, vars);
            active = value.current?;
            lowered.push(value.operand);
        }
        if !matches!(lowered.get(2), Some(IrOperand::Const(IrConst::Hash(_)))) {
            self.record_error("open handle expected_root must be a compile-time Hash literal", span);
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: None,
            func: crate::ir::IrCallTarget::runtime_symbol(runtime_helper),
            args: lowered,
        });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
    }

    pub(super) fn lower_deployment_line_handle_requirement(
        &mut self,
        runtime_helper: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
        span: Span,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered = Vec::with_capacity(args.len());
        for arg in args {
            let value = self.lower_expr(arg, active, blocks, vars);
            active = value.current?;
            lowered.push(value.operand);
        }
        if !matches!(lowered.last(), Some(IrOperand::Const(IrConst::Hash(_)))) {
            self.record_error("deployment line handle_hash must be a compile-time Hash literal", span);
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: None,
            func: crate::ir::IrCallTarget::runtime_symbol(runtime_helper),
            args: lowered,
        });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
    }
}
