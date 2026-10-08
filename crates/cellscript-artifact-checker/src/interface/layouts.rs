//! Source nominal identities are bound to complete concrete checked layouts.
//! Lowered aliases never obtain identity by matching another layout's width.
use super::{nominals::scope, qualified_source_type};
use crate::{
    CheckerError, CheckerRejectionCode, NominalDeclarationBinding, NominalDeclarationCatalog, NominalDeclarationScope,
    TypedSemanticRecord,
};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message)
}

pub(super) fn checked_type(
    value: &str,
    catalog: &NominalDeclarationCatalog,
    record: &TypedSemanticRecord,
) -> Result<String, CheckerError> {
    let scope = NominalDeclarationScope {
        module: record.module.clone(),
        bindings: catalog
            .layout_bindings
            .iter()
            .filter(|binding| !binding.lowered_name.contains("::"))
            .map(|binding| NominalDeclarationBinding {
                local_name: binding.lowered_name.clone(),
                owner_module: binding.owner_module.clone(),
                source_name: binding.source_name.clone(),
            })
            .collect(),
    };
    let mut aliases = BTreeMap::new();
    for instance in record.instantiations.iter().filter(|instance| matches!(instance.kind.as_str(), "struct" | "enum")) {
        let arguments =
            instance.type_arguments.iter().map(|argument| qualified_source_type(argument, &scope)).collect::<Result<Vec<_>, _>>()?;
        let name = format!("{}::{}<{}>", instance.module, instance.template, arguments.join(","));
        if name.len() > 512 {
            return Err(invalid("qualified generic layout identity exceeds its type bound"));
        }
        for lowered in &instance.lowered_names {
            aliases.insert(lowered.as_str(), name.clone());
        }
    }
    let source = crate::generic_projection::checked_source_type(value, &aliases);
    qualified_source_type(&source, &scope)
}

pub(super) fn verify(catalog: &NominalDeclarationCatalog, record: &TypedSemanticRecord) -> Result<(), CheckerError> {
    let generic_names = record
        .instantiations
        .iter()
        .filter(|instance| matches!(instance.kind.as_str(), "struct" | "enum"))
        .flat_map(|instance| instance.lowered_names.iter().map(String::as_str))
        .collect::<BTreeSet<_>>();
    let expected =
        record.types.iter().map(|ty| ty.name.as_str()).filter(|name| !generic_names.contains(name)).collect::<BTreeSet<_>>();
    let retained = catalog.layout_bindings.iter().map(|binding| binding.lowered_name.as_str()).collect::<BTreeSet<_>>();
    if expected != retained || retained.len() != catalog.layout_bindings.len() {
        return Err(invalid("nominal layout binding set omits, duplicates or adds concrete types"));
    }
    let root = scope(catalog, &record.module)?;
    for binding in &catalog.layout_bindings {
        let source = catalog
            .declarations
            .iter()
            .find(|declaration| declaration.module == binding.owner_module && declaration.name == binding.source_name)
            .ok_or_else(|| invalid("nominal layout binding lacks its defining declaration"))?;
        let typed = record
            .types
            .iter()
            .find(|ty| ty.name == binding.lowered_name)
            .ok_or_else(|| invalid("nominal layout binding has no checked concrete type"))?;
        if let Some(alias) = root.bindings.iter().find(|alias| alias.local_name == binding.lowered_name)
            && (alias.owner_module != binding.owner_module || alias.source_name != binding.source_name)
        {
            return Err(invalid("nominal concrete alias changes its retained source owner"));
        }
        if binding.lowered_name.contains("::") && binding.lowered_name != format!("{}::{}", binding.owner_module, binding.source_name)
        {
            return Err(invalid("qualified concrete nominal name changes its retained source owner"));
        }
        let mut capabilities = source.capabilities.clone();
        capabilities.sort();
        if source.kind != typed.kind
            || source.abilities != typed.value_abilities
            || capabilities != typed.capabilities
            || source.identity_policy != typed.identity_policy
            || source.fields.len() != typed.fields.len()
            || source.variants.len() != typed.variants.len()
        {
            return Err(invalid("nominal declaration differs from its checked concrete contract"));
        }
        let owner = scope(catalog, &binding.owner_module)?;
        let mut offset = Some(0u32);
        for field in &source.fields {
            let concrete = typed
                .fields
                .iter()
                .find(|concrete| concrete.name == field.name)
                .ok_or_else(|| invalid("nominal source field is missing from its checked layout"))?;
            if offset.is_some_and(|offset| concrete.offset != offset)
                || qualified_source_type(&field.ty, owner)? != checked_type(&concrete.ty, catalog, record)?
            {
                return Err(invalid("nominal source field order or qualified type differs from its checked layout"));
            }
            // Dynamic declarations keep the existing language. A fixed-layout
            // receipt must additionally require complete widths/offsets.
            if let Some((start, width)) = offset.zip(concrete.width_bytes) {
                offset = Some(start.checked_add(width).ok_or_else(|| invalid("nominal field offsets overflow"))?);
            } else {
                offset = None;
            }
        }
        for (index, (variant, concrete)) in source.variants.iter().zip(&typed.variants).enumerate() {
            if variant.name != concrete.name || concrete.tag as usize != index || variant.fields.len() != concrete.fields.len() {
                return Err(invalid("nominal enum order or payload differs from its checked layout"));
            }
            for (field, concrete) in variant.fields.iter().zip(&concrete.fields) {
                if qualified_source_type(field, owner)? != checked_type(&concrete.ty, catalog, record)? {
                    return Err(invalid("nominal enum field changes its checked qualified type"));
                }
            }
        }
    }
    Ok(())
}
