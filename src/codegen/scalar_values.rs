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
        IrType::Named(name) => {
            crate::nested_values::is_nested_name(name)
                || ["InputView<", "OutputView<", "CellDataView<", "ExactCellSelection<"].iter().any(|prefix| name.starts_with(prefix))
        }
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
                        func.as_str(),
                        "__ckb_cell_capacity"
                            | "__ckb_cell_occupied_capacity"
                            | "__ckb_cell_unoccupied_capacity"
                            | "__ckb_cell_data_size"
                            | "__ckb_cell_output_index"
                    ))
                    || (dest.ty == IrType::Hash
                        && matches!(func.as_str(), "__ckb_cell_data_hash_field" | "__ckb_cell_lock_hash" | "__ckb_cell_type_hash")))
                    && matches!(args.as_slice(), [IrOperand::Var(var)] if matches!(&var.ty,
                        IrType::Named(name) if name.starts_with("InputView<") || name.starts_with("OutputView<")))
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
