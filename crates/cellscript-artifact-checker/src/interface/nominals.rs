//! Declaration scopes are explicit; equal widths never qualify a nominal type.
use super::{mismatch, PackageInterface};
use crate::{
    CheckerError, CheckerRejectionCode, NominalDeclarationCatalog, NominalDeclarationScope, TypedSemanticRecord,
    NOMINAL_DECLARATION_CATALOG_SCHEMA,
};
use std::collections::{BTreeMap, BTreeSet};

/// Reserved compatible-open handle class names (#28 H2). The independent
/// value-ability layer validates any leaf built from them.
pub(crate) const OPEN_HANDLE_CLASSES: [&str; 2] = ["ScriptHandle", "VerifierHandle"];

fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message)
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value
            .chars()
            .enumerate()
            .all(|(index, character)| character == '_' || character.is_alphabetic() || (index > 0 && character.is_alphanumeric()))
}
fn module(value: &str) -> bool {
    value.len() <= 512 && value.split("::").all(identifier)
}
fn visibility(value: &str) -> bool {
    matches!(value, "legacy-public" | "public" | "public(package)" | "private")
}

/// Canonical bounded name resolution, not a checked receipt constructor.
/// Consumers must independently validate the scope/targets before admission.
pub fn qualified_source_type(value: &str, scope: &NominalDeclarationScope) -> Result<String, CheckerError> {
    qualified_source_type_with_parameters(value, scope, &[])
}

/// Generic binders shadow source imports and declarations with the same name.
/// The caller must supply the complete checked declaration parameter list.
pub fn qualified_source_type_with_parameters(
    value: &str,
    scope: &NominalDeclarationScope,
    parameters: &[&str],
) -> Result<String, CheckerError> {
    if value.len() > 512 || scope.bindings.len() > 256 || !module(&scope.module) {
        return Err(invalid("nominal declaration spelling or scope exceeds its bound"));
    }
    let mut bindings = BTreeMap::new();
    for binding in &scope.bindings {
        if !identifier(&binding.local_name)
            || !module(&binding.owner_module)
            || !identifier(&binding.source_name)
            || binding.owner_module.len().saturating_add(binding.source_name.len()).saturating_add(2) > 512
            || bindings.insert(binding.local_name.as_str(), binding).is_some()
        {
            return Err(invalid("nominal declaration scope has ambiguous or unbounded names"));
        }
    }
    if parameters.len() > 8
        || parameters.iter().any(|parameter| !identifier(parameter))
        || parameters.iter().copied().collect::<BTreeSet<_>>().len() != parameters.len()
    {
        return Err(invalid("nominal declaration has ambiguous or unbounded generic binders"));
    }
    super::source_types::qualify(value, scope, parameters)
}

pub(crate) fn scope<'a>(catalog: &'a NominalDeclarationCatalog, name: &str) -> Result<&'a NominalDeclarationScope, CheckerError> {
    catalog.scopes.iter().find(|scope| scope.module == name).ok_or_else(|| invalid("nominal declaration has no owner scope"))
}

pub(crate) fn verify_catalog(record: &TypedSemanticRecord) -> Result<(), CheckerError> {
    let Some(catalog) = &record.nominal_declarations else { return Ok(()) };
    if catalog.schema != NOMINAL_DECLARATION_CATALOG_SCHEMA
        || catalog.declarations.len() > 256
        || catalog.scopes.is_empty()
        || catalog.scopes.len() > 256
        || catalog.callables.len() > 128
        || catalog.constants.len() > 256
        || catalog.layout_bindings.len() > 256
    {
        return Err(invalid("unsupported or unbounded nominal declaration catalog"));
    }
    let mut nodes = catalog
        .scopes
        .len()
        .saturating_add(catalog.declarations.len())
        .saturating_add(catalog.callables.len())
        .saturating_add(catalog.constants.len());
    nodes = nodes.saturating_add(catalog.layout_bindings.len());
    for binding in &catalog.layout_bindings {
        if !module(&binding.owner_module) || !identifier(&binding.source_name) || !module(&binding.lowered_name) {
            return Err(invalid("nominal layout binding has an invalid or unbounded source identity"));
        }
    }
    for scope in &catalog.scopes {
        nodes = nodes.saturating_add(scope.bindings.len());
    }
    for declaration in &catalog.declarations {
        nodes = nodes.saturating_add(declaration.fields.len()).saturating_add(declaration.variants.len());
        for variant in &declaration.variants {
            nodes = nodes.saturating_add(variant.fields.len());
        }
    }
    for callable in &catalog.callables {
        nodes = nodes
            .saturating_add(callable.params.len())
            .saturating_add(callable.outputs.len())
            .saturating_add(callable.type_parameters.len());
    }
    if nodes > 4096 {
        return Err(invalid("nominal declaration catalog exceeds its traversal bound"));
    }
    let mut names = BTreeSet::new();
    for declaration in &catalog.declarations {
        // Reserved compatible-open handle class names (#28 H2): no user
        // declaration may manufacture the surface before it ships.
        if matches!(declaration.name.as_str(), "ScriptHandle" | "VerifierHandle") {
            return Err(invalid("nominal declaration uses a reserved open-handle class name"));
        }
        if !module(&declaration.module)
            || !identifier(&declaration.name)
            || !visibility(&declaration.visibility)
            || !names.insert((declaration.module.as_str(), declaration.name.as_str()))
            || declaration.fields.len() > 64
            || declaration.variants.len() > 32
            || !matches!(declaration.kind.as_str(), "resource" | "shared" | "receipt" | "struct" | "enum")
            || declaration.type_identity.as_ref().is_some_and(|identity| identity.is_empty() || identity.len() > 512)
        {
            return Err(invalid("nominal declaration has invalid identity, kind or bounded shape"));
        }
        crate::value_abilities::declared(&declaration.abilities)?;
        let ordinary = matches!(declaration.kind.as_str(), "struct" | "enum");
        let capabilities = declaration.capabilities.iter().collect::<BTreeSet<_>>();
        if capabilities.len() != declaration.capabilities.len()
            || (ordinary && !capabilities.is_empty())
            || (!ordinary && !declaration.abilities.is_empty())
            || declaration.capabilities.iter().any(|capability| {
                !matches!(
                    capability.as_str(),
                    "store" | "create" | "consume" | "destroy" | "replace" | "burn" | "relock" | "retarget_type" | "read_ref"
                )
            })
        {
            return Err(invalid("nominal declaration has invalid capabilities or value abilities"));
        }
        if declaration.kind == "enum" {
            if !declaration.fields.is_empty() || declaration.identity_policy != "none" || declaration.type_identity.is_some() {
                return Err(invalid("nominal enum carries a structural Cell identity"));
            }
        } else if !declaration.variants.is_empty() {
            return Err(invalid("nominal structural type carries enum variants"));
        }
        let field_names = declaration.fields.iter().map(|field| field.name.as_str()).collect::<BTreeSet<_>>();
        let variant_names = declaration.variants.iter().map(|variant| variant.name.as_str()).collect::<BTreeSet<_>>();
        if field_names.len() != declaration.fields.len()
            || variant_names.len() != declaration.variants.len()
            || field_names.iter().chain(&variant_names).any(|name| !identifier(name))
        {
            return Err(invalid("nominal declaration has duplicate or invalid fields/variants"));
        }
        if !matches!(declaration.identity_policy.as_str(), "none" | "ckb-type-id" | "script-args" | "singleton-type")
            && !declaration
                .identity_policy
                .strip_prefix("field:")
                .is_some_and(|field| field.len() <= 512 && field_names.contains(field.split('.').next().unwrap_or("")))
        {
            return Err(invalid("nominal declaration has invalid identity policy"));
        }
        let owner = scope(catalog, &declaration.module)?;
        for field in &declaration.fields {
            qualified_source_type(&field.ty, owner)?;
        }
        for variant in &declaration.variants {
            if variant.fields.len() > 64 {
                return Err(invalid("nominal enum payload exceeds 64 fields"));
            }
            for ty in &variant.fields {
                qualified_source_type(ty, owner)?;
            }
        }
    }
    if let Some(generic) = &record.generic_declarations {
        for declaration in &generic.declarations {
            if matches!(declaration.kind.as_str(), "struct" | "enum")
                && !names.insert((declaration.module.as_str(), declaration.name.as_str()))
            {
                return Err(invalid("nominal and generic declarations have conflicting identities"));
            }
        }
    }
    let mut scopes = BTreeSet::new();
    for owner in &catalog.scopes {
        if !module(&owner.module) || !scopes.insert(owner.module.as_str()) || owner.bindings.len() > 256 {
            return Err(invalid("nominal declaration owner scope is duplicated or unbounded"));
        }
        qualified_source_type("unit", owner)?;
        for binding in &owner.bindings {
            if !names.contains(&(binding.owner_module.as_str(), binding.source_name.as_str())) {
                return Err(invalid("nominal declaration import lacks its defining contract"));
            }
        }
    }
    if !scopes.contains(record.module.as_str()) {
        return Err(invalid("nominal declaration catalog omits its artifact module"));
    }
    for (module, name) in &names {
        let owner = scope(catalog, module)?;
        if !owner
            .bindings
            .iter()
            .any(|binding| binding.local_name == *name && binding.owner_module == *module && binding.source_name == *name)
        {
            return Err(invalid("nominal declaration owner is rebound through an import alias"));
        }
    }
    let mut symbols = names.clone();
    for callable in &catalog.callables {
        if !module(&callable.module)
            || !identifier(&callable.name)
            || !visibility(&callable.visibility)
            || !symbols.insert((callable.module.as_str(), callable.name.as_str()))
            || callable.params.len() > 64
            || callable.outputs.len() > 64
            || callable.type_parameters.len() > 8
            || !matches!(callable.kind.as_str(), "action" | "function" | "lock")
            || !matches!(callable.declared_effect.as_str(), "Pure" | "ReadOnly" | "Mutating" | "Creating" | "Destroying")
            || (callable.kind != "action" && !callable.outputs.is_empty())
            || (callable.kind != "function" && !callable.type_parameters.is_empty())
        {
            return Err(invalid("nominal declaration callable has invalid identity or signature shape"));
        }
        let owner = scope(catalog, &callable.module)?;
        let mut params = BTreeSet::new();
        for parameter in callable.params.iter().chain(&callable.outputs) {
            if !identifier(&parameter.name)
                || !params.insert(parameter.name.as_str())
                || !matches!(parameter.source.as_str(), "default" | "input" | "output" | "protected" | "witness" | "lock_args")
            {
                return Err(invalid("nominal declaration callable has invalid parameters"));
            }
            qualified_source_type(&parameter.r#type, owner)?;
        }
        if let Some(ty) = &callable.return_type {
            qualified_source_type(ty, owner)?;
        }
        let mut parameters = BTreeSet::new();
        for parameter in &callable.type_parameters {
            if !identifier(&parameter.name) || parameter.phantom || !parameters.insert(parameter.name.as_str()) {
                return Err(invalid("nominal declaration callable has invalid generic parameters"));
            }
            crate::value_abilities::declared(&parameter.constraints)?;
        }
    }
    for constant in &catalog.constants {
        if !module(&constant.module)
            || !identifier(&constant.name)
            || !visibility(&constant.visibility)
            || !symbols.insert((constant.module.as_str(), constant.name.as_str()))
        {
            return Err(invalid("nominal declaration constant has invalid identity"));
        }
        qualified_source_type(&constant.ty, scope(catalog, &constant.module)?)?;
    }
    super::layouts::verify(catalog, record)
}

pub(super) fn check_public(interface: &PackageInterface, record: &TypedSemanticRecord) -> Result<(), CheckerError> {
    let Some(catalog) = &record.nominal_declarations else { return Ok(()) };
    let exported = |visibility: &str| matches!(visibility, "public" | "legacy-public");
    let mut types = catalog
        .declarations
        .iter()
        .filter(|declaration| declaration.module == interface.module && exported(&declaration.visibility))
        .map(|declaration| declaration.name.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(generic) = &record.generic_declarations {
        types.extend(
            generic
                .declarations
                .iter()
                .filter(|declaration| {
                    declaration.module == interface.module
                        && matches!(declaration.kind.as_str(), "struct" | "enum")
                        && exported(&declaration.visibility)
                })
                .map(|declaration| declaration.name.as_str()),
        );
    }
    let callables = catalog
        .callables
        .iter()
        .filter(|callable| callable.module == interface.module && exported(&callable.visibility))
        .map(|callable| callable.name.as_str())
        .collect::<BTreeSet<_>>();
    let constants = catalog
        .constants
        .iter()
        .filter(|constant| constant.module == interface.module && exported(&constant.visibility))
        .map(|constant| constant.name.as_str())
        .collect::<BTreeSet<_>>();
    if types != interface.types.iter().map(|ty| ty.name.as_str()).collect()
        || types.len() != interface.types.len()
        || callables != interface.callables.iter().map(|callable| callable.name.as_str()).collect()
        || callables.len() != interface.callables.len()
        || constants != interface.constants.iter().map(|constant| constant.name.as_str()).collect()
        || constants.len() != interface.constants.len()
    {
        return Err(mismatch("public declaration set omits, duplicates or adds retained source declarations"));
    }
    for ty in interface.types.iter().filter(|ty| ty.type_parameters.is_empty()) {
        let declaration = catalog
            .declarations
            .iter()
            .find(|declaration| declaration.module == interface.module && declaration.name == ty.name)
            .ok_or_else(|| mismatch("public nominal type lacks its retained declaration contract"))?;
        let mut source_capabilities = declaration.capabilities.clone();
        let mut public_capabilities = ty.cell_capabilities.clone();
        source_capabilities.sort();
        public_capabilities.sort();
        if source_capabilities != public_capabilities {
            return Err(mismatch("public Cell capabilities differ from the retained nominal declaration"));
        }
        if declaration.abilities != ty.value_abilities {
            return Err(mismatch("public value abilities differ from the retained nominal declaration"));
        }
        if declaration.kind != ty.kind
            || declaration.visibility != ty.visibility
            || declaration.type_identity != ty.type_identity
            || declaration.abilities != ty.value_abilities
            || declaration.fields.len() != ty.fields.len()
            || declaration.variants.len() != ty.variants.len()
            || declaration.fields.iter().zip(&ty.fields).any(|(retained, public)| {
                retained.name != public.name
                    || crate::generic_projection::canonical_type(&retained.ty)
                        != crate::generic_projection::canonical_type(&public.r#type)
            })
            || declaration.variants.iter().zip(&ty.variants).any(|(retained, public)| {
                retained.name != public.name
                    || retained.fields.len() != public.fields.len()
                    || retained.fields.iter().zip(&public.fields).any(|(retained, public)| {
                        crate::generic_projection::canonical_type(retained) != crate::generic_projection::canonical_type(public)
                    })
            })
        {
            return Err(mismatch("public nominal layout differs from its retained declaration contract"));
        }
    }
    for callable in &interface.callables {
        let source = catalog
            .callables
            .iter()
            .find(|source| source.module == interface.module && source.name == callable.name)
            .ok_or_else(|| mismatch("public callable lacks its retained declaration contract"))?;
        if source.kind != callable.kind
            || source.visibility != callable.visibility
            || source.type_parameters != callable.type_parameters
            || source.params != callable.params
            || source.outputs != callable.outputs
            || source.return_type != callable.return_type
            || source.declared_effect != callable.effect
        {
            return Err(mismatch("public callable differs from its retained declaration contract"));
        }
    }
    for constant in &interface.constants {
        let source = catalog
            .constants
            .iter()
            .find(|source| source.module == interface.module && source.name == constant.name)
            .ok_or_else(|| mismatch("public constant lacks its retained declaration contract"))?;
        if source.visibility != constant.visibility || source.ty != constant.r#type {
            return Err(mismatch("public constant differs from its retained declaration contract"));
        }
    }
    Ok(())
}
