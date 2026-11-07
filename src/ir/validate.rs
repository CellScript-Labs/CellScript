//! Structural invariants, separate from source and independent machine checks.
//! Locals are mutable locations; this is not SSA or a dominance proof.
use super::*;
use crate::error::DiagnosticCode;

fn invalid(detail: impl std::fmt::Display) -> CompileError {
    CompileError::without_span(format!("invalid executable IR: {detail}")).with_code(DiagnosticCode::E2105)
}

pub(crate) fn validate_structure(module: &IrModule) -> Result<()> {
    let declarations = module
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::TypeDef(value) => Some(value.name.as_str()),
            _ => None,
        })
        .chain(module.external_type_defs.iter().map(|value| value.name.as_str()))
        .chain(module.source_type_origins.keys().map(String::as_str));
    for name in declarations {
        if is_reserved_builtin_type_name(name) {
            return Err(invalid(format!("declaration '{name}' conflicts with a reserved builtin representation")));
        }
    }
    for (name, identity) in &module.nominal_identities {
        if is_reserved_builtin_type_name(name) && matches!(identity, Some(crate::resolve::ResolvedTypeIdentity::Nominal { .. })) {
            return Err(invalid(format!("nominal alias '{name}' conflicts with a reserved builtin representation")));
        }
    }
    for item in &module.items {
        let (name, params, body) = match item {
            IrItem::Action(value) => (&value.name, &value.params, &value.body),
            IrItem::Function(value) => (&value.name, &value.params, &value.body),
            IrItem::Lock(value) => (&value.name, &value.params, &value.body),
            _ => continue,
        };
        validate_body(module, name, params, body)?;
    }
    Ok(())
}

fn validate_body(module: &IrModule, name: &str, params: &[IrParam], body: &IrBody) -> Result<()> {
    if body.blocks.is_empty() {
        return Err(invalid(format!("callable '{name}' has no entry block")));
    }
    let mut blocks = HashSet::new();
    for block in &body.blocks {
        if !blocks.insert(block.id) {
            return Err(invalid(format!("callable '{name}' repeats block {}", block.id.0)));
        }
    }
    let mut locals = HashMap::<usize, IrType>::new();
    let mut variable = |var: &IrVar| {
        // Borrow views share backing storage. Empty vectors may acquire an
        // element type; two concrete types must still agree by declaration.
        let ty = backing_type(&var.ty);
        match locals.get(&var.id) {
            Some(old) if matches!(old, IrType::Vector(None)) && matches!(ty, IrType::Vector(Some(_))) => {
                locals.insert(var.id, ty.clone());
            }
            Some(old) if matches!(old, IrType::Vector(Some(_))) && matches!(ty, IrType::Vector(None)) => {}
            Some(old) if !resolved_types_equal(old, ty, &module.nominal_identities) => {
                return Err(invalid(format!(
                    "callable '{name}' gives local {} conflicting types '{}' and '{}'",
                    var.id,
                    old.source_text(),
                    ty.source_text()
                )));
            }
            None => {
                locals.insert(var.id, ty.clone());
            }
            _ => {}
        }
        Ok(())
    };
    let mut param_ids = HashSet::new();
    for param in params {
        if !param_ids.insert(param.binding.id) || param.ty != param.binding.ty {
            return Err(invalid(format!("callable '{name}' has an inconsistent parameter binding '{}'", param.name)));
        }
        variable(&param.binding)?;
    }
    for block in &body.blocks {
        for instruction in &block.instructions {
            super::type_visit::visit_instruction_variables(instruction, &mut variable)?;
        }
        let target = |target: BlockId| {
            if !blocks.contains(&target) {
                return Err(invalid(format!("callable '{name}' block {} targets missing block {}", block.id.0, target.0)));
            }
            Ok(())
        };
        match &block.terminator {
            IrTerminator::Jump(id) => target(*id)?,
            IrTerminator::Branch { cond, then_block, else_block } => {
                target(*then_block)?;
                target(*else_block)?;
                match cond {
                    IrOperand::Var(var) if var.ty == IrType::Bool => variable(var)?,
                    IrOperand::Const(IrConst::Bool(_)) => {}
                    _ => return Err(invalid(format!("callable '{name}' block {} has a non-boolean branch", block.id.0))),
                }
            }
            IrTerminator::Return(Some(IrOperand::Var(var))) => variable(var)?,
            IrTerminator::Return(_) => {}
        }
    }
    Ok(())
}

fn backing_type(ty: &IrType) -> &IrType {
    match ty {
        IrType::Ref(inner) | IrType::MutRef(inner) => backing_type(inner),
        ty => ty,
    }
}

#[cfg(test)]
mod tests;
