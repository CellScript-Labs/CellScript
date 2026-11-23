use super::{witness_bytes_view_parts, BlockId, IrBlock, IrConst, IrGenerator, IrInstruction, IrOperand, IrType, IrVar, LoweredExpr};
use crate::ast::{CallExpr, Expr};
use std::collections::HashMap;

impl IrGenerator<'_> {
    pub(super) fn lower_bounded_witness_view(
        &mut self,
        name: &str,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        use cellscript_artifact_checker::view_types::{WitnessBytesOwner, WitnessBytesViewType};
        let owner = match name {
            "witness::bounded_raw" => WitnessBytesOwner::Raw,
            "witness::bounded_lock" => WitnessBytesOwner::Lock,
            "witness::bounded_entry" => WitnessBytesOwner::Entry,
            "witness::bounded_output_type" => WitnessBytesOwner::OutputType,
            _ => return None,
        };
        let Expr::Integer(maximum) = call.args.get(1)? else {
            return None;
        };
        let lowered = self.lower_expr(&call.args[0], current, blocks, vars);
        let active = lowered.current?;
        let maximum = u64::try_from(*maximum).ok()?;
        let dest =
            self.new_var(format!("bounded_witness_{}", owner.as_str()), IrType::WitnessBytes(WitnessBytesViewType { owner, maximum }));
        self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: dest.clone(), src: lowered.operand });
        Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
    }

    pub(super) fn lower_witness_scalar_read(
        &mut self,
        name: &str,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let lowered_view = self.lower_expr(&call.args[0], current, blocks, vars);
        let active = lowered_view.current?;
        let view_type = self.operand_type(&lowered_view.operand);
        let lowered_offset = self.lower_expr(&call.args[1], active, blocks, vars);
        let active = lowered_offset.current?;
        let (legacy_func, dest_name) = match name {
            "witness::byte" => ("__ckb_witness_u8", "witness_u8"),
            "witness::u32_le" => ("__ckb_witness_u32_le", "witness_u32_le"),
            _ => ("__ckb_witness_u64_le", "witness_u64_le"),
        };
        let (func, args) = if let Some((_, owner, maximum)) = witness_bytes_view_parts(&view_type) {
            let func = match name {
                "witness::byte" => "__ckb_witness_bounded_u8",
                "witness::u32_le" => "__ckb_witness_bounded_u32_le",
                _ => "__ckb_witness_bounded_u64_le",
            };
            (
                func,
                vec![
                    lowered_view.operand,
                    IrOperand::Const(IrConst::U64(owner)),
                    IrOperand::Const(IrConst::U64(maximum)),
                    lowered_offset.operand,
                ],
            )
        } else {
            (legacy_func, vec![lowered_view.operand, lowered_offset.operand])
        };
        let dest = self.new_var(dest_name, IrType::U64);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: Some(dest.clone()),
            func: crate::ir::IrCallTarget::runtime_symbol(func),
            args,
        });
        Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
    }

    pub(super) fn lower_bounded_witness_blake2b(
        &mut self,
        call: &CallExpr,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let lowered = self.lower_expr(&call.args[0], current, blocks, vars);
        let active = lowered.current?;
        let (_, owner, maximum) = witness_bytes_view_parts(&self.operand_type(&lowered.operand))?;
        let dest = self.new_var("bounded_witness_blake2b", IrType::Hash);
        self.block_mut(blocks, active).instructions.push(IrInstruction::Call {
            dest: Some(dest.clone()),
            func: crate::ir::IrCallTarget::Runtime(
                cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbWitnessBoundedBlake2b,
            ),
            args: vec![lowered.operand, IrOperand::Const(IrConst::U64(owner)), IrOperand::Const(IrConst::U64(maximum))],
        });
        Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
    }

    pub(super) fn lower_transparent_view_wrapper(
        &mut self,
        call: &CallExpr,
        wrapper_type: &str,
        current: BlockId,
        blocks: &mut Vec<IrBlock>,
        vars: &mut HashMap<String, IrVar>,
    ) -> Option<LoweredExpr> {
        let lowered = self.lower_expr(&call.args[0], current, blocks, vars);
        let active = lowered.current?;
        let dest = self.new_var("typed_transaction_view", IrType::Named(wrapper_type.to_string()));
        self.block_mut(blocks, active).instructions.push(IrInstruction::Move { dest: dest.clone(), src: lowered.operand });
        Some(LoweredExpr { operand: IrOperand::Var(dest), current: Some(active) })
    }
}
