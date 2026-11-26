//! Independently validate the bounded Lock-group authorization entry contract.

use super::*;

pub(super) fn validate(typed: &TypedSemanticRecord, selected: &str) -> Result<(), CheckerError> {
    let invalid = || typed_error("Lock authorization entry has inconsistent ports or protected source");
    let mut entries = typed.entries.iter().filter(|entry| entry.id == selected);
    let Some(entry) = entries.next() else { return invalid() };
    if entries.next().is_some() || entry.kind != "lock" || entry.return_type != "bool" {
        return invalid();
    }
    let mut protected = None;
    for param in &entry.params {
        match param.source.as_str() {
            "protected" => {
                if param.mutable || param.reference {
                    return invalid();
                }
                let Some(schema) = param.ty.strip_prefix('&').map(str::trim) else { return invalid() };
                if protected.replace(param).is_some()
                    || !typed.types.iter().any(|ty| ty.name == schema && matches!(ty.kind.as_str(), "resource" | "shared" | "receipt"))
                {
                    return invalid();
                }
                let matching = entry.cell_bindings.iter().filter(|binding| binding.binding == param.name).collect::<Vec<_>>();
                let [binding] = matching.as_slice() else { return invalid() };
                if binding.role != CellBindingRole::ReadOnly
                    || binding.local_id != Some(param.binding_id)
                    || binding.ty != schema
                    || binding.source != CellBindingSource::GroupInput
                    || binding.ordinal != 0
                    || binding.membership != CellBindingMembership::CurrentLockGroup
                {
                    return invalid();
                }
            }
            "witness" if !param.reference => {}
            "lockargs" if !param.mutable && !param.reference => {}
            "default" if !param.mutable && param.reference => {
                let Some(schema) = param.ty.strip_prefix('&').map(str::trim) else { return invalid() };
                if !typed.types.iter().any(|ty| ty.name == schema && matches!(ty.kind.as_str(), "resource" | "shared" | "receipt")) {
                    return invalid();
                }
                let matching = entry.cell_bindings.iter().filter(|binding| binding.binding == param.name).collect::<Vec<_>>();
                let [binding] = matching.as_slice() else { return invalid() };
                if binding.role != CellBindingRole::ReadOnly
                    || binding.local_id != Some(param.binding_id)
                    || binding.ty != schema
                    || binding.source != CellBindingSource::CellDep
                    || binding.membership != CellBindingMembership::Unproven
                {
                    return invalid();
                }
            }
            _ => return invalid(),
        }
    }
    for binding in &entry.cell_bindings {
        if binding.source == CellBindingSource::CellDep
            && binding.role == CellBindingRole::ReadOnly
            && binding.membership == CellBindingMembership::Unproven
        {
            continue;
        }
        if !protected.is_some_and(|param| binding.binding == param.name && binding.local_id == Some(param.binding_id)) {
            return invalid();
        }
    }
    Ok(())
}
