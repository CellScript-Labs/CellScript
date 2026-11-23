use super::{BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrType, IrVar, LoweredExpr};
use crate::ast::Expr;
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_simple_runtime_call(
        &mut self,
        func: &str,
        dest_name: &str,
        return_ty: IrType,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered_args = Vec::with_capacity(args.len());
        for arg in args {
            let lowered = self.lower_expr(arg, active, blocks, vars);
            active = lowered.current?;
            lowered_args.push(lowered.operand);
        }
        let dest = self.new_var(dest_name, return_ty);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: Some(dest.clone()),
            func: crate::ir::IrCallTarget::runtime_symbol(func),
            args: lowered_args,
        });
        Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
    }

    pub(super) fn lower_void_runtime_call(
        &mut self,
        func: &str,
        args: &[Expr],
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let mut active = current;
        let mut lowered_args = Vec::with_capacity(args.len());
        for arg in args {
            let lowered = self.lower_expr(arg, active, blocks, vars);
            active = lowered.current?;
            lowered_args.push(lowered.operand);
        }
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: None,
            func: crate::ir::IrCallTarget::runtime_symbol(func),
            args: lowered_args,
        });
        Some(LoweredExpr { operand: IrOperand::Const(IrConst::Unit), current: Some(active) })
    }

    pub(super) fn lower_call_target_name(&self, name: &str) -> String {
        if let Some(label) = self.call_target_labels.get(name) {
            return label.clone();
        }
        if let Some((module, symbol)) = name.rsplit_once("::")
            && module == self.module.name
        {
            return symbol.to_string();
        }
        name.to_string()
    }

    pub(super) fn call_param_types(&self, source_name: &str, lowered_name: &str) -> Option<Vec<IrType>> {
        self.function_param_types
            .get(source_name)
            .or_else(|| self.function_param_types.get(lowered_name))
            .or_else(|| self.external_function_param_types.get(source_name))
            .or_else(|| self.external_function_param_types.get(lowered_name))
            .cloned()
    }

    pub(super) fn call_return_type(&self, source_name: &str, lowered_name: &str) -> Option<Option<IrType>> {
        self.function_return_types
            .get(source_name)
            .or_else(|| self.function_return_types.get(lowered_name))
            .or_else(|| self.external_function_return_types.get(source_name))
            .or_else(|| self.external_function_return_types.get(lowered_name))
            .cloned()
    }
}
