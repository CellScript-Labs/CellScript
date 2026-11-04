//! Source-only scalar boundaries for collection, Cell and nested-view bodies.
//! They preserve executable bytes and do not claim complete source provenance.
use super::*;

fn scalar(ty: &IrType) -> bool {
    matches!(ty, IrType::Bool | IrType::U8 | IrType::U16 | IrType::U32 | IrType::I32 | IrType::U64)
}

pub(super) fn scalar_operand(value: &IrOperand) -> bool {
    match value {
        IrOperand::Var(var) => scalar(&var.ty),
        IrOperand::Const(value) => {
            matches!(value, IrConst::Bool(_) | IrConst::U8(_) | IrConst::U16(_) | IrConst::U32(_) | IrConst::U64(_))
        }
    }
}

/// A direct collection read has already published its copied value pointer.
/// Bind its register return separately from general aggregate computation.
pub(super) fn return_operand(block: &IrBlock, value: &IrOperand) -> bool {
    if scalar_operand(value) {
        return true;
    }
    let IrOperand::Var(var) = value else { return false };
    let copied_bytes =
        var.ty == IrType::U128 || matches!(&var.ty, IrType::Array(inner, count) if **inner == IrType::U8 && (1..=8).contains(count));
    copied_bytes
        && block.instructions.iter().any(|instruction| {
            matches!(instruction, IrInstruction::AllocatedValue(operation)
            if operation.dest.as_ref().is_some_and(|dest| dest.id == var.id))
        })
}

fn bounded_type(ty: &IrType) -> bool {
    match ty {
        IrType::Ref(inner) | IrType::MutRef(inner) => bounded_type(inner),
        IrType::Nested(_) => true,
        IrType::Context(view) => matches!(
            view,
            crate::ast::ContextView::Input(_) | crate::ast::ContextView::Output(_) | crate::ast::ContextView::CellData(_)
        ),
        IrType::Selection(_) => true,
        _ => false,
    }
}

fn bounded_operand(value: &IrOperand) -> bool {
    matches!(value, IrOperand::Var(var) if bounded_type(&var.ty))
}

pub(super) fn covers_body(body: &IrBody, params: &[IrParam]) -> bool {
    params.iter().any(|param| bounded_type(&param.ty))
        || body.blocks.iter().flat_map(|block| &block.instructions).any(|instruction| match instruction {
            IrInstruction::AllocatedValue(_)
            | IrInstruction::VerifyExactCellRoles { .. }
            | IrInstruction::ReadExactCellRole { .. } => true,
            IrInstruction::Call { dest, args, .. } => {
                dest.as_ref().is_some_and(|var| bounded_type(&var.ty)) || args.iter().any(bounded_operand)
            }
            IrInstruction::Move { dest, src } => bounded_type(&dest.ty) || bounded_operand(src),
            IrInstruction::FieldAccess { dest, obj, .. } => bounded_type(&dest.ty) || bounded_operand(obj),
            IrInstruction::Index { dest, arr, .. } => bounded_type(&dest.ty) || bounded_operand(arr),
            IrInstruction::Length { operand, .. } => bounded_operand(operand),
            _ => false,
        })
}

impl CodeGenerator {
    pub(super) fn begin_scalar_value(&mut self, instruction: &IrInstruction, block: usize, index: usize) -> Option<String> {
        if !self.checked_scalar_values {
            return None;
        }
        let covered = match instruction {
            IrInstruction::LoadConst { dest, value } => scalar(&dest.ty) && scalar_operand(&IrOperand::Const(value.clone())),
            IrInstruction::Move { dest, src } => scalar(&dest.ty) && scalar_operand(src),
            IrInstruction::Unary { dest, op, operand } => {
                scalar(&dest.ty) && scalar_operand(operand) && matches!(op, UnaryOp::Neg | UnaryOp::Not)
            }
            IrInstruction::Binary { dest, left, right, .. } => scalar(&dest.ty) && scalar_operand(left) && scalar_operand(right),
            IrInstruction::Call { dest: Some(dest), func, args } => {
                ((dest.ty == IrType::U64
                    && matches!(
                        func.runtime_helper(),
                        Some(
                            cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellCapacity
                                | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellOccupiedCapacity
                                | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellUnoccupiedCapacity
                                | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataSize
                                | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellOutputIndex
                        )
                    ))
                    || (dest.ty == IrType::Hash
                        && matches!(
                            func.runtime_helper(),
                            Some(
                                cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellDataHashField
                                    | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellLockHash
                                    | cellscript_artifact_checker::runtime_contract::RuntimeHelper::CkbCellTypeHash
                            )
                        ))
                    || func.runtime_helper().is_some_and(|helper| crate::ir::is_ckb_script_hash_property_result(helper, &dest.ty)))
                    && matches!(args.as_slice(), [IrOperand::Var(var)] if matches!(&var.ty,
                        IrType::Context(crate::ast::ContextView::Input(_) | crate::ast::ContextView::Output(_))))
            }
            _ => false,
        };
        if !covered {
            return None;
        }
        let marker = self.fresh_label(&format!("allocated_scalar_{block}_{index}"));
        self.emit_label(&format!("{marker}_begin"));
        Some(marker)
    }
}
