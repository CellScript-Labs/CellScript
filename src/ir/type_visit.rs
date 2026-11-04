//! Complete compiler-owned type traversal for IR admission. New instructions
//! must choose how their types and operands participate in this boundary.
use super::{ast_type_to_ir, IrInstruction, IrOperand, IrType};

fn operand<E>(value: &IrOperand, visit: &mut impl FnMut(&IrType) -> Result<(), E>) -> Result<(), E> {
    if let IrOperand::Var(value) = value {
        visit(&value.ty)?;
    }
    Ok(())
}

pub(crate) fn visit_instruction_types<E>(
    instruction: &IrInstruction,
    mut visit: impl FnMut(&IrType) -> Result<(), E>,
) -> Result<(), E> {
    use IrInstruction::*;
    match instruction {
        AllocatedValue(operation) => {
            if let Some(dest) = &operation.dest {
                visit(&dest.ty)?;
            }
            for value in operation.owner.iter().chain(&operation.args) {
                operand(value, &mut visit)?;
            }
            for member in &operation.declaration.members {
                use crate::ast::ValueCollectionKind;
                match &member.kind {
                    ValueCollectionKind::Vec(ty) | ValueCollectionKind::OrderedSet(ty) => visit(&ast_type_to_ir(ty))?,
                    ValueCollectionKind::OrderedMap { key, value } => {
                        visit(&ast_type_to_ir(key))?;
                        visit(&ast_type_to_ir(value))?;
                    }
                }
            }
        }
        LoadConst { dest, .. } | LoadVar { dest, .. } => visit(&dest.ty)?,
        StoreVar { src, .. } => operand(src, &mut visit)?,
        Binary { dest, left, right, .. } => {
            visit(&dest.ty)?;
            operand(left, &mut visit)?;
            operand(right, &mut visit)?;
        }
        Unary { dest, operand: value, .. }
        | FieldAccess { dest, obj: value, .. }
        | Length { dest, operand: value }
        | TypeHash { dest, operand: value }
        | Move { dest, src: value }
        | Settle { dest, operand: value }
        | Claim { dest, receipt: value } => {
            visit(&dest.ty)?;
            operand(value, &mut visit)?;
        }
        Index { dest, arr, idx } => {
            visit(&dest.ty)?;
            operand(arr, &mut visit)?;
            operand(idx, &mut visit)?;
        }
        CollectionNew { dest, ty, capacity } => {
            visit(&dest.ty)?;
            visit(ty)?;
            if let Some(value) = capacity {
                operand(value, &mut visit)?;
            }
        }
        CollectionCapacity { dest, collection } | CollectionPop { dest, collection } => {
            visit(&dest.ty)?;
            operand(collection, &mut visit)?;
        }
        CollectionContains { dest, collection, value } => {
            visit(&dest.ty)?;
            operand(collection, &mut visit)?;
            operand(value, &mut visit)?;
        }
        CollectionRemove { dest, collection, index } => {
            visit(&dest.ty)?;
            operand(collection, &mut visit)?;
            operand(index, &mut visit)?;
        }
        CollectionPush { collection, value }
        | CollectionExtend { collection, slice: value }
        | CollectionTruncate { collection, len: value } => {
            operand(collection, &mut visit)?;
            operand(value, &mut visit)?;
        }
        CollectionClear { collection } | CollectionReverse { collection } => operand(collection, &mut visit)?,
        CollectionSwap { collection, left, right } => {
            operand(collection, &mut visit)?;
            operand(left, &mut visit)?;
            operand(right, &mut visit)?;
        }
        CollectionInsert { collection, index, value } | CollectionSet { collection, index, value } => {
            operand(collection, &mut visit)?;
            operand(index, &mut visit)?;
            operand(value, &mut visit)?;
        }
        BoundedCellLoad { dest, found, index, element_type, .. } => {
            visit(&dest.ty)?;
            visit(&found.ty)?;
            visit(&IrType::Named(element_type.clone()))?;
            operand(index, &mut visit)?;
        }
        BoundedPlanLoad { dest, found, plan, index, element_type, .. } => {
            visit(&dest.ty)?;
            visit(&found.ty)?;
            visit(&IrType::Named(element_type.clone()))?;
            operand(plan, &mut visit)?;
            operand(index, &mut visit)?;
        }
        BoundedOutputVerify { index, pattern, .. } => {
            operand(index, &mut visit)?;
            visit(&IrType::Named(pattern.ty.clone()))?;
            for (_, value) in &pattern.fields {
                operand(value, &mut visit)?;
            }
            if let Some(value) = &pattern.lock {
                operand(value, &mut visit)?;
            }
        }
        BoundedOutputEnd { index } => operand(index, &mut visit)?,
        VerifyExactCellRoles { dest, contract } => {
            if let Some(dest) = dest {
                visit(&dest.ty)?;
            }
            for role in &contract.roles {
                visit(&IrType::Named(role.type_name.clone()))?;
            }
        }
        ReadExactCellRole { dest, selection, index, .. } => {
            visit(&dest.ty)?;
            operand(selection, &mut visit)?;
            if let Some(value) = index {
                operand(value, &mut visit)?;
            }
        }
        Call { dest, args, .. } => {
            if let Some(dest) = dest {
                visit(&dest.ty)?;
            }
            for value in args {
                operand(value, &mut visit)?;
            }
        }
        ReadRef { dest, ty } => {
            visit(&dest.ty)?;
            visit(&IrType::Named(ty.clone()))?;
        }
        Tuple { dest, fields } => {
            visit(&dest.ty)?;
            for value in fields {
                operand(value, &mut visit)?;
            }
        }
        EnumConstruct { dest, enum_name, fields, .. } => {
            visit(&dest.ty)?;
            visit(&IrType::Named(enum_name.clone()))?;
            for value in fields {
                operand(value, &mut visit)?;
            }
        }
        EnumTag { dest, operand: value, enum_name } | EnumPayload { dest, operand: value, enum_name, .. } => {
            visit(&dest.ty)?;
            visit(&IrType::Named(enum_name.clone()))?;
            operand(value, &mut visit)?;
        }
        Consume { operand: value } | Destroy { operand: value, .. } => operand(value, &mut visit)?,
        Transfer { dest, operand: value, to } => {
            visit(&dest.ty)?;
            operand(value, &mut visit)?;
            operand(to, &mut visit)?;
        }
        Create { dest, pattern } | CreateUnique { dest, pattern, .. } => {
            visit(&dest.ty)?;
            visit(&IrType::Named(pattern.ty.clone()))?;
            for (_, value) in &pattern.fields {
                operand(value, &mut visit)?;
            }
            if let Some(value) = &pattern.lock {
                operand(value, &mut visit)?;
            }
        }
        ReplaceUnique { dest, operand: value, pattern, .. } => {
            visit(&dest.ty)?;
            operand(value, &mut visit)?;
            visit(&IrType::Named(pattern.ty.clone()))?;
            for (_, value) in &pattern.fields {
                operand(value, &mut visit)?;
            }
            if let Some(value) = &pattern.lock {
                operand(value, &mut visit)?;
            }
        }
        CellMetadataEquality { left, right, .. } => {
            operand(left, &mut visit)?;
            operand(right, &mut visit)?;
        }
    }
    Ok(())
}
