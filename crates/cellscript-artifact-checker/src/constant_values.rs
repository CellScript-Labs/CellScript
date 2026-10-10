//! Independent bounded evaluation of public constant values.
//!
//! The producer records each public constant's initializer as a closed
//! expression tree plus its folded value. This module re-evaluates the tree
//! under explicit depth and step budgets with checked operations and compares
//! against the recorded value; it never trusts the producer's arithmetic. The
//! catalog is optional evidence: constants without a proven contract stay
//! fail-closed at the finite external codec.

use crate::{ConstantExpression, ConstantValueContract, TypedSemanticRecord, CONSTANT_VALUE_CATALOG_SCHEMA};
use std::collections::BTreeMap;

const MAX_VALUES: usize = 256;
const MAX_DEPTH: usize = 16;
const MAX_STEPS: usize = 1024;
const MAX_TEXT: usize = 512;

/// A proven constant keyed by its declaring module and name.
pub struct CheckedConstantValues {
    proven: BTreeMap<(String, String), (String, String)>,
}

impl CheckedConstantValues {
    /// Canonical type and proven value of `(module, name)`.
    pub fn proven(&self, module: &str, name: &str) -> Option<&(String, String)> {
        self.proven.get(&(module.to_string(), name.to_string()))
    }

    pub fn len(&self) -> usize {
        self.proven.len()
    }

    pub fn is_empty(&self) -> bool {
        self.proven.is_empty()
    }
}

fn invalid(message: impl Into<String>) -> crate::CheckerError {
    crate::CheckerError::new(
        crate::CheckerRejectionCode::V2419TypedSemanticsInvalid,
        format!("constant value contract: {}", message.into()),
    )
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TEXT
        && value
            .chars()
            .enumerate()
            .all(|(index, character)| character == '_' || character.is_alphabetic() || (index > 0 && character.is_numeric()))
}

fn canonical_value(ty: &str, value: &str) -> Result<u64, crate::CheckerError> {
    match ty {
        "u64" => value.parse::<u64>().map_err(|_| invalid("u64 constant value is not canonical decimal")),
        "bool" => match value {
            "true" => Ok(1),
            "false" => Ok(0),
            _ => Err(invalid("bool constant value is not a canonical literal")),
        },
        _ => Err(invalid("constant type is outside the proven scalar profile")),
    }
}

/// Evaluate one expression node independently. Every operation is checked:
/// overflow, division or remainder by zero, undefined shifts, unknown
/// operators and budget exhaustion reject the whole catalog.
fn evaluate(expression: &ConstantExpression, ty: &str, depth: usize, steps: &mut usize) -> Result<u64, crate::CheckerError> {
    *steps += 1;
    if depth > MAX_DEPTH || *steps > MAX_STEPS {
        return Err(invalid("constant expression exceeds its evaluation budget"));
    }
    match expression {
        ConstantExpression::Literal { value } => {
            let literal = value.parse::<u64>().map_err(|_| invalid("constant literal is not a decimal u64"))?;
            if ty == "bool" && literal > 1 {
                return Err(invalid("bool constant literal is outside its domain"));
            }
            Ok(literal)
        }
        ConstantExpression::Binary { op, left, right } => {
            let (left, right) = (evaluate(left, ty, depth + 1, steps)?, evaluate(right, ty, depth + 1, steps)?);
            let checked = |value: Option<u64>| value.ok_or_else(|| invalid("constant arithmetic overflows its checked domain"));
            match op.as_str() {
                "add" => checked(left.checked_add(right)),
                "sub" => checked(left.checked_sub(right)),
                "mul" => checked(left.checked_mul(right)),
                "div" => (right != 0).then(|| left / right).ok_or_else(|| invalid("constant division by zero")),
                "rem" => (right != 0).then(|| left % right).ok_or_else(|| invalid("constant remainder by zero")),
                "and" => Ok(left & right),
                "or" => Ok(left | right),
                "xor" => Ok(left ^ right),
                "shl" => (right < 64).then(|| left << right).ok_or_else(|| invalid("constant shift exceeds its width")),
                "shr" => (right < 64).then(|| left >> right).ok_or_else(|| invalid("constant shift exceeds its width")),
                _ => Err(invalid("constant operator is outside the closed grammar")),
            }
        }
    }
}

/// Verify the whole catalog against itself and the declared public constants.
/// The declared set must exactly equal the proven set: an entry for an
/// undeclared name, or a declared constant without proven evidence, rejects.
pub fn check_constant_values(
    record: &TypedSemanticRecord,
    declared: &[(String, String, String)],
) -> Result<CheckedConstantValues, crate::CheckerError> {
    let catalog = record.constant_values.as_ref().ok_or_else(|| invalid("public constants lack their value evidence catalog"))?;
    if catalog.schema != CONSTANT_VALUE_CATALOG_SCHEMA || catalog.values.len() > MAX_VALUES {
        return Err(invalid("unsupported or unbounded constant value catalog"));
    }
    let mut proven = BTreeMap::new();
    for contract in &catalog.values {
        verify_contract(contract)?;
        let mut steps = 0usize;
        let evaluated = evaluate(&contract.expression, &contract.ty, 0, &mut steps)?;
        if evaluated != canonical_value(&contract.ty, &contract.value)? {
            return Err(invalid("recorded constant value differs from its independently evaluated expression"));
        }
        if proven.insert((contract.module.clone(), contract.name.clone()), (contract.ty.clone(), contract.value.clone())).is_some() {
            return Err(invalid("constant value catalog duplicates a contract"));
        }
    }
    let declared = declared.iter().map(|(module, name, _)| (module.clone(), name.clone())).collect::<std::collections::BTreeSet<_>>();
    if proven.len() != declared.len() || declared.iter().any(|key| !proven.contains_key(key)) {
        return Err(invalid("proven constant set differs from the declared public constants"));
    }
    Ok(CheckedConstantValues { proven })
}

fn verify_contract(contract: &ConstantValueContract) -> Result<(), crate::CheckerError> {
    if !identifier(&contract.module)
        || contract.module.split("::").any(|segment| !identifier(segment))
        || !identifier(&contract.name)
        || contract.ty.len() > 16
        || contract.value.len() > 40
    {
        return Err(invalid("constant contract identity or spelling exceeds its bounds"));
    }
    canonical_value(&contract.ty, &contract.value)?;
    Ok(())
}
