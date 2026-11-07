//! Complete compiler-owned type traversal for IR admission. New instructions
//! must choose how their types and operands participate in this boundary.
use super::{ast_type_to_ir, IrInstruction, IrOperand, IrType, IrVar};

enum Node<'a> {
    Type(&'a IrType),
    Variable(&'a IrVar),
}

fn operand<E>(value: &IrOperand, visit: &mut impl FnMut(Node<'_>) -> Result<(), E>) -> Result<(), E> {
    if let IrOperand::Var(value) = value {
        visit(Node::Variable(value))?;
    }
    Ok(())
}

pub(crate) fn visit_instruction_types<E>(
    instruction: &IrInstruction,
    mut visit: impl FnMut(&IrType) -> Result<(), E>,
) -> Result<(), E> {
    visit_instruction(instruction, |node| {
        visit(match node {
            Node::Type(ty) => ty,
            Node::Variable(var) => &var.ty,
        })
    })
}

pub(super) fn visit_instruction_variables<E>(
    instruction: &IrInstruction,
    mut visit: impl FnMut(&IrVar) -> Result<(), E>,
) -> Result<(), E> {
    visit_instruction(instruction, |node| match node {
        Node::Variable(var) => visit(var),
        Node::Type(_) => Ok(()),
    })
}

fn visit_instruction<E>(instruction: &IrInstruction, mut visit: impl FnMut(Node<'_>) -> Result<(), E>) -> Result<(), E> {
    use IrInstruction::*;
    match instruction {
        AllocatedValue(operation) => {
            if let Some(dest) = &operation.dest {
                visit(Node::Variable(dest))?;
            }
            for value in operation.owner.iter().chain(&operation.args) {
                operand(value, &mut visit)?;
            }
            for member in &operation.declaration.members {
                use crate::ast::ValueCollectionKind;
                match &member.kind {
                    ValueCollectionKind::Vec(ty) | ValueCollectionKind::OrderedSet(ty) => visit(Node::Type(&ast_type_to_ir(ty)))?,
                    ValueCollectionKind::OrderedMap { key, value } => {
                        visit(Node::Type(&ast_type_to_ir(key)))?;
                        visit(Node::Type(&ast_type_to_ir(value)))?;
                    }
                }
            }
        }
        LoadConst { dest, .. } | LoadVar { dest, .. } => visit(Node::Variable(dest))?,
        StoreVar { src, .. } => operand(src, &mut visit)?,
        Binary { dest, left, right, .. } => {
            visit(Node::Variable(dest))?;
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
            visit(Node::Variable(dest))?;
            operand(value, &mut visit)?;
        }
        Index { dest, arr, idx } => {
            visit(Node::Variable(dest))?;
            operand(arr, &mut visit)?;
            operand(idx, &mut visit)?;
        }
        CollectionNew { dest, ty, capacity } => {
            visit(Node::Variable(dest))?;
            visit(Node::Type(ty))?;
            if let Some(value) = capacity {
                operand(value, &mut visit)?;
            }
        }
        CollectionCapacity { dest, collection } | CollectionPop { dest, collection } => {
            visit(Node::Variable(dest))?;
            operand(collection, &mut visit)?;
        }
        CollectionContains { dest, collection, value } => {
            visit(Node::Variable(dest))?;
            operand(collection, &mut visit)?;
            operand(value, &mut visit)?;
        }
        CollectionRemove { dest, collection, index } => {
            visit(Node::Variable(dest))?;
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
            visit(Node::Variable(dest))?;
            visit(Node::Variable(found))?;
            visit(Node::Type(&IrType::Named(element_type.clone())))?;
            operand(index, &mut visit)?;
        }
        BoundedPlanLoad { dest, found, plan, index, element_type, .. } => {
            visit(Node::Variable(dest))?;
            visit(Node::Variable(found))?;
            visit(Node::Type(&IrType::Named(element_type.clone())))?;
            operand(plan, &mut visit)?;
            operand(index, &mut visit)?;
        }
        BoundedOutputVerify { index, pattern, .. } => {
            operand(index, &mut visit)?;
            visit(Node::Type(&IrType::Named(pattern.ty.clone())))?;
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
                visit(Node::Variable(dest))?;
            }
            for role in &contract.roles {
                visit(Node::Type(&IrType::Named(role.type_name.clone())))?;
            }
        }
        ReadExactCellRole { dest, selection, index, .. } => {
            visit(Node::Variable(dest))?;
            operand(selection, &mut visit)?;
            if let Some(value) = index {
                operand(value, &mut visit)?;
            }
        }
        Call { dest, args, .. } => {
            if let Some(dest) = dest {
                visit(Node::Variable(dest))?;
            }
            for value in args {
                operand(value, &mut visit)?;
            }
        }
        ReadRef { dest, ty } => {
            visit(Node::Variable(dest))?;
            visit(Node::Type(&IrType::Named(ty.clone())))?;
        }
        Tuple { dest, fields } => {
            visit(Node::Variable(dest))?;
            for value in fields {
                operand(value, &mut visit)?;
            }
        }
        EnumConstruct { dest, enum_name, fields, .. } => {
            visit(Node::Variable(dest))?;
            visit(Node::Type(&IrType::Named(enum_name.clone())))?;
            for value in fields {
                operand(value, &mut visit)?;
            }
        }
        EnumTag { dest, operand: value, enum_name } | EnumPayload { dest, operand: value, enum_name, .. } => {
            visit(Node::Variable(dest))?;
            visit(Node::Type(&IrType::Named(enum_name.clone())))?;
            operand(value, &mut visit)?;
        }
        Consume { operand: value } | Destroy { operand: value, .. } => operand(value, &mut visit)?,
        Transfer { dest, operand: value, to } => {
            visit(Node::Variable(dest))?;
            operand(value, &mut visit)?;
            operand(to, &mut visit)?;
        }
        Create { dest, pattern } | CreateUnique { dest, pattern, .. } => {
            visit(Node::Variable(dest))?;
            visit(Node::Type(&IrType::Named(pattern.ty.clone())))?;
            for (_, value) in &pattern.fields {
                operand(value, &mut visit)?;
            }
            if let Some(value) = &pattern.lock {
                operand(value, &mut visit)?;
            }
        }
        ReplaceUnique { dest, operand: value, pattern, .. } => {
            visit(Node::Variable(dest))?;
            operand(value, &mut visit)?;
            visit(Node::Type(&IrType::Named(pattern.ty.clone())))?;
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
