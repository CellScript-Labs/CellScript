//! Explicit Lock-group entry binding, separate from Type Script lifecycle coverage.

use super::*;
use crate::artifact::{ArtifactContext, ArtifactDeclaration, ArtifactDispatch, EntryCellBindings, EntryCoverage};

fn invalid(message: impl Into<String>) -> CompileError {
    CompileError::without_span(format!("Lock-group entry: {}", message.into())).with_code(crate::error::DiagnosticCode::E2101)
}

pub(crate) fn bind_single_lock_entry(module: &mut IrModule, declaration: &ArtifactDeclaration) -> Result<()> {
    declaration.validate()?;
    if !matches!(declaration.context, ArtifactContext::LockGroup {}) || declaration.dispatch != ArtifactDispatch::SingleEntry {
        return Err(invalid("requires a single-entry Lock artifact"));
    }
    let name = declaration.entry.as_deref().ok_or_else(|| invalid("requires an entry"))?;
    let selected = module
        .items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            IrItem::Lock(lock) if lock.name == name => Some((index, lock)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(index, lock)] = selected.as_slice() else { return Err(invalid("entry must identify exactly one Lock declaration")) };
    let index = *index;
    let bindings = group_bindings(module, lock)?;
    let IrItem::Lock(lock) = &mut module.items[index] else { unreachable!("selected a Lock") };
    lock.body.cell_bindings = bindings;
    lock.entry_cells = Some(IrEntryCells { bindings: declaration.bindings, coverage: declaration.coverage, layout: None });
    Ok(())
}

pub(super) fn validate_entry_cells(module: &IrModule, lock: &IrLock) -> Result<()> {
    let Some(cells) = &lock.entry_cells else { return Ok(()) };
    if cells.bindings != EntryCellBindings::GroupRelative
        || cells.coverage != EntryCoverage::AuthorizationOnly
        || cells.layout.is_some()
    {
        return Err(invalid("authorization coverage requires group-relative bindings and no Type-group layout"));
    }
    if !matches!(module.resolved_entry(), Some(IrEntryPoint::Lock(entry)) if entry.name == lock.name) {
        return Err(invalid("a Lock artifact contract must belong to the selected Lock entry"));
    }
    if group_bindings(module, lock)? != lock.body.cell_bindings {
        return Err(invalid("selected Lock has a stale physical binding plan"));
    }
    Ok(())
}

fn group_bindings(module: &IrModule, lock: &IrLock) -> Result<Vec<IrCellBinding>> {
    let mut protected = Vec::new();
    for param in &lock.params {
        match param.source {
            ParamSource::Protected => {
                let IrType::Ref(inner) = &param.ty else { return Err(invalid("protected ports require a read-only Cell view")) };
                let IrType::Named(schema) = inner.as_ref() else { return Err(invalid("protected ports require a Cell schema")) };
                if !unique_cell_schema(module, schema) || param.is_mut || param.is_ref || param.is_read_ref {
                    return Err(invalid("protected ports must resolve to one immutable Cell-backed schema"));
                }
                protected.push((param, schema));
            }
            ParamSource::Witness if !param.is_ref && !param.is_read_ref => {}
            ParamSource::LockArgs if !param.is_mut && !param.is_ref && !param.is_read_ref => {}
            ParamSource::Default if param.is_read_ref && !param.is_mut && !param.is_ref => {
                let IrType::Ref(inner) = &param.ty else {
                    return Err(invalid("read ports require an immutable Cell dependency view"));
                };
                let IrType::Named(schema) = inner.as_ref() else { return Err(invalid("read ports require a Cell schema")) };
                if !unique_cell_schema(module, schema) {
                    return Err(invalid("read ports must resolve to one Cell-backed schema"));
                }
                let matching = lock.body.cell_bindings.iter().filter(|binding| binding.binding == param.name).collect::<Vec<_>>();
                let [binding] = matching.as_slice() else {
                    return Err(invalid("read ports require exactly one Cell dependency binding"));
                };
                if binding.role != IrCellBindingRole::ReadOnly
                    || binding.local_id != Some(param.binding.id)
                    || binding.ty != *schema
                    || binding.source != IrCellSource::CellDep
                    || binding.membership != IrCellMembership::Unproven
                {
                    return Err(invalid("read port disagrees with its Cell dependency binding"));
                }
            }
            _ => return Err(invalid("ports use protected, lock_args, witness, or read provenance")),
        }
    }
    if protected.len() > 1 {
        return Err(invalid("the bounded Lock contract admits at most one protected input view"));
    }
    let mut bindings = lock.body.cell_bindings.clone();
    for (param, schema) in &protected {
        let mut matching = bindings.iter_mut().filter(|binding| binding.binding == param.name);
        let binding = matching.next().ok_or_else(|| invalid("protected port has no physical binding"))?;
        if matching.next().is_some() {
            return Err(invalid("protected port must have exactly one physical binding"));
        }
        if binding.role != IrCellBindingRole::ReadOnly || binding.local_id != Some(param.binding.id) || binding.ty != schema.as_str() {
            return Err(invalid("protected binding disagrees with its checked port"));
        }
        binding.source = IrCellSource::GroupInput;
        binding.ordinal = 0;
        binding.membership = IrCellMembership::CurrentLockGroup;
    }
    for binding in &bindings {
        if binding.source == IrCellSource::CellDep
            && binding.role == IrCellBindingRole::ReadOnly
            && binding.membership == IrCellMembership::Unproven
        {
            continue;
        }
        if !protected.iter().any(|(param, _)| param.name == binding.binding)
            || binding.role != IrCellBindingRole::ReadOnly
            || binding.source != IrCellSource::GroupInput
            || binding.ordinal != 0
            || binding.membership != IrCellMembership::CurrentLockGroup
        {
            return Err(invalid("Lock authorization does not introduce input/output lifecycle roles"));
        }
    }
    Ok(bindings)
}

fn unique_cell_schema(module: &IrModule, schema: &str) -> bool {
    module
        .items
        .iter()
        .filter_map(|item| match item {
            IrItem::TypeDef(ty) => Some(ty),
            _ => None,
        })
        .chain(&module.external_type_defs)
        .filter(|ty| ty.name == schema && matches!(ty.kind, IrTypeKind::Resource | IrTypeKind::Shared | IrTypeKind::Receipt))
        .count()
        == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound() -> IrModule {
        let source = include_str!("../../tests/fixtures/authoring/lock-artifact.cell");
        let module = crate::frontend::parse(source, crate::CURRENT_EDITION).unwrap();
        crate::types::check(&module).unwrap();
        let ir = crate::ir::generate(&module).unwrap();
        crate::scope_ir_to_entry(&ir, &crate::CompileEntryScope::Artifact(ArtifactDeclaration::single_lock_group("vault", "unlock")))
            .unwrap()
    }

    #[test]
    fn lock_binding_revalidation_rejects_stale_sources_without_repairing_them() {
        let mut module = bound();
        module.validate_entry_selection().unwrap();
        let IrItem::Lock(lock) = module.items.iter_mut().find(|item| matches!(item, IrItem::Lock(_))).unwrap() else { unreachable!() };
        lock.body.cell_bindings[0].source = IrCellSource::Input;
        lock.body.cell_bindings[0].membership = IrCellMembership::Unproven;
        assert!(module.validate_entry_selection().unwrap_err().message.contains("stale physical binding"));
        let failure = crate::codegen::CodeGenerator::new(Default::default())
            .generate(&module, crate::ArtifactFormat::from_target("riscv64-elf").unwrap())
            .unwrap_err();
        assert!(failure.message.contains("stale physical binding"));
        let IrItem::Lock(lock) = module.items.iter().find(|item| matches!(item, IrItem::Lock(_))).unwrap() else { unreachable!() };
        assert_eq!(lock.body.cell_bindings[0].source, IrCellSource::Input);
    }

    #[test]
    fn lock_authorization_does_not_accept_a_type_group_layout_or_coverage() {
        let mut module = bound();
        let IrItem::Lock(lock) = module.items.iter_mut().find(|item| matches!(item, IrItem::Lock(_))).unwrap() else { unreachable!() };
        let cells = lock.entry_cells.as_mut().unwrap();
        cells.coverage = EntryCoverage::FixedGroup;
        cells.layout = Some("Vault".into());
        assert!(module.validate_entry_selection().unwrap_err().message.contains("authorization coverage"));
    }
}
