//! Public constant value evidence for the source-contracts profile.
//!
//! Each exported constant whose initializer is closed over the bounded
//! grammar (u64/bool literals and checked binary operations over u64) records
//! its canonical type, folded value and the expression tree. The independent
//! checker re-evaluates the tree under its own budgets; constants outside the
//! grammar are omitted and stay fail-closed at the finite codec.

use crate::ast::{self, BinaryOp, Expr};
use cellscript_artifact_checker::{ConstantExpression, ConstantValueCatalog, ConstantValueContract, CONSTANT_VALUE_CATALOG_SCHEMA};

pub(crate) fn constant_catalog(module: &ast::Module) -> Option<ConstantValueCatalog> {
    let mut values = Vec::new();
    for item in &module.items {
        let ast::Item::Const(def) = item else { continue };
        if !module.visibility_of(&def.name).is_exported() {
            continue;
        }
        let ty = match def.ty {
            ast::Type::U64 => "u64",
            ast::Type::Bool => "bool",
            _ => continue,
        };
        let Some((expression, folded)) = closed_expression(&def.value) else {
            continue;
        };
        let value = match (&def.ty, folded) {
            (ast::Type::Bool, 0) => "false".to_string(),
            (ast::Type::Bool, 1) => "true".to_string(),
            (ast::Type::Bool, _) => continue,
            _ => folded.to_string(),
        };
        values.push(ConstantValueContract {
            module: module.name.clone(),
            name: def.name.clone(),
            ty: ty.to_string(),
            value,
            expression,
        });
    }
    (!values.is_empty()).then(|| ConstantValueCatalog { schema: CONSTANT_VALUE_CATALOG_SCHEMA.to_string(), values })
}

/// Fold one initializer over the closed grammar. The arithmetic matches the
/// independent checker operation for operation: checked overflow, nonzero
/// divisors and shifts below 64; anything else has no proven value.
fn closed_expression(expr: &Expr) -> Option<(ConstantExpression, u64)> {
    match expr {
        Expr::Integer(value) => {
            let folded = u64::try_from(*value).ok()?;
            Some((ConstantExpression::Literal { value: folded.to_string() }, folded))
        }
        Expr::Bool(value) => Some((ConstantExpression::Literal { value: (*value as u64).to_string() }, *value as u64)),
        Expr::Binary(binary) => {
            let (left, left_value) = closed_expression(&binary.left)?;
            let (right, right_value) = closed_expression(&binary.right)?;
            let op = match binary.op {
                BinaryOp::Add => "add",
                BinaryOp::Sub => "sub",
                BinaryOp::Mul => "mul",
                BinaryOp::Div => "div",
                BinaryOp::Mod => "rem",
                BinaryOp::BitAnd => "and",
                BinaryOp::BitOr => "or",
                BinaryOp::BitXor => "xor",
                BinaryOp::Shl => "shl",
                BinaryOp::Shr => "shr",
                _ => return None,
            };
            let folded = match binary.op {
                BinaryOp::Add => left_value.checked_add(right_value)?,
                BinaryOp::Sub => left_value.checked_sub(right_value)?,
                BinaryOp::Mul => left_value.checked_mul(right_value)?,
                BinaryOp::Div => (right_value != 0).then(|| left_value / right_value)?,
                BinaryOp::Mod => (right_value != 0).then(|| left_value % right_value)?,
                BinaryOp::BitAnd => left_value & right_value,
                BinaryOp::BitOr => left_value | right_value,
                BinaryOp::BitXor => left_value ^ right_value,
                BinaryOp::Shl => (right_value < 64).then(|| left_value << right_value)?,
                BinaryOp::Shr => (right_value < 64).then(|| left_value >> right_value)?,
                _ => return None,
            };
            Some((ConstantExpression::Binary { op: op.to_string(), left: Box::new(left), right: Box::new(right) }, folded))
        }
        _ => None,
    }
}
