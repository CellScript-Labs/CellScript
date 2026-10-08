//! Source declaration owners and import scopes survive pruning. These are
//! symbolic contracts; executable availability and effects belong to checked IR.
use crate::ast::{Capability, Field, IdentityPolicy, Item, Module, TypeIdentity, ValueAbility};
use crate::ModuleResolver;
use cellscript_artifact_checker::{
    interface::{InterfaceParam, InterfaceTypeParameter},
    GenericDeclarationCatalog, NominalDeclarationBinding, NominalDeclarationCatalog, NominalDeclarationContract,
    NominalDeclarationScope, NominalLayoutBinding, SourceCallableContract, SourceConstantContract, TypedSemanticGenericField,
    TypedSemanticGenericVariant, NOMINAL_DECLARATION_CATALOG_SCHEMA,
};
use std::collections::{BTreeMap, BTreeSet};

fn modules<'a>(root: &'a Module, resolver: Option<&'a ModuleResolver>) -> Vec<&'a Module> {
    let mut pending = vec![root];
    let mut result = BTreeMap::new();
    while let Some(module) = pending.pop() {
        if result.insert(module.name.as_str(), module).is_some() {
            continue;
        }
        if let Some(resolver) = resolver {
            for item in &module.items {
                let Item::Use(import) = item else { continue };
                if let Some(owner) = resolver.module(&import.module_path.join("::"))
                    && !result.contains_key(owner.name.as_str())
                {
                    pending.push(owner);
                }
            }
        }
    }
    result.into_values().collect()
}

pub(crate) fn generic_catalog_for_context(root: &Module, resolver: Option<&ModuleResolver>) -> Option<GenericDeclarationCatalog> {
    let mut declarations = Vec::new();
    for module in modules(root, resolver) {
        if let Some(catalog) = super::declarations::generic_catalog(module) {
            declarations.extend(catalog.declarations);
        } else if module.interface_templates.iter().any(|item| match item {
            Item::Struct(def) => !def.type_params.is_empty(),
            Item::Enum(def) => !def.type_params.is_empty(),
            Item::Function(def) => !def.type_params.is_empty(),
            _ => false,
        }) {
            return None;
        }
    }
    if declarations.is_empty() || !super::declarations::bounded(&declarations) {
        return None;
    }
    declarations.sort_by(|left, right| (&left.module, &left.name, &left.kind).cmp(&(&right.module, &right.name, &right.kind)));
    Some(GenericDeclarationCatalog { schema: cellscript_artifact_checker::GENERIC_DECLARATION_CATALOG_SCHEMA.into(), declarations })
}

pub(crate) fn nominal_catalog(
    root: &Module,
    resolver: Option<&ModuleResolver>,
    ir: &crate::ir::IrModule,
    typed: &cellscript_artifact_checker::TypedSemanticRecord,
) -> Option<NominalDeclarationCatalog> {
    let mut catalog = NominalDeclarationCatalog { schema: NOMINAL_DECLARATION_CATALOG_SCHEMA.into(), ..Default::default() };
    for module in modules(root, resolver) {
        let mut bindings = BTreeMap::new();
        for item in module.items.iter().chain(&module.interface_templates) {
            if let Some(name) = item.name() {
                if crate::generics::decode_monomorph_name(name).is_some() {
                    continue;
                }
                if matches!(item, Item::Resource(_) | Item::Shared(_) | Item::Receipt(_) | Item::Struct(_) | Item::Enum(_)) {
                    bindings.insert(
                        name.to_string(),
                        NominalDeclarationBinding {
                            local_name: name.to_string(),
                            owner_module: module.name.clone(),
                            source_name: name.to_string(),
                        },
                    );
                }
                if let Some(declaration) = nominal(module, item) {
                    catalog.declarations.push(declaration);
                }
                if let Some(callable) = callable(module, item) {
                    catalog.callables.push(callable);
                }
                if let Item::Const(constant) = item {
                    catalog.constants.push(SourceConstantContract {
                        module: module.name.clone(),
                        name: name.to_string(),
                        visibility: module.visibility_of(name).as_str().into(),
                        ty: crate::generics::render_source_type(&constant.ty),
                    });
                }
            }
            let Item::Use(import) = item else { continue };
            for symbol in &import.imports {
                let local = symbol.alias.as_ref().unwrap_or(&symbol.name);
                let owner = import.module_path.join("::");
                if resolver.is_some_and(|resolver| resolver.resolve_type(&module.name, local).is_none()) {
                    continue;
                }
                let source =
                    crate::generics::decode_monomorph_name(&symbol.name).map(|(name, _)| name).unwrap_or_else(|| symbol.name.clone());
                let local = crate::generics::decode_monomorph_name(local).map(|(name, _)| name).unwrap_or_else(|| local.clone());
                let binding = NominalDeclarationBinding { local_name: local.clone(), owner_module: owner, source_name: source };
                if bindings.get(&local).is_some_and(|previous| previous != &binding) {
                    return None;
                }
                bindings.insert(local, binding);
            }
        }
        catalog.scopes.push(NominalDeclarationScope { module: module.name.clone(), bindings: bindings.into_values().collect() });
    }
    let generic_names = typed
        .instantiations
        .iter()
        .filter(|instance| matches!(instance.kind.as_str(), "struct" | "enum"))
        .flat_map(|instance| &instance.lowered_names)
        .collect::<BTreeSet<_>>();
    for ty in &typed.types {
        if generic_names.contains(&ty.name) {
            continue;
        }
        let origin = ir.source_type_origins.get(&ty.name)?.as_ref()?;
        catalog.layout_bindings.push(NominalLayoutBinding {
            owner_module: origin.module.clone(),
            source_name: origin.name.clone(),
            lowered_name: ty.name.clone(),
        });
    }
    if !bounded(&catalog) {
        return None;
    }
    let mut targets =
        catalog.declarations.iter().map(|declaration| (declaration.module.clone(), declaration.name.clone())).collect::<BTreeSet<_>>();
    if let Some(generic) = generic_catalog_for_context(root, resolver) {
        targets.extend(
            generic
                .declarations
                .iter()
                .filter(|declaration| matches!(declaration.kind.as_str(), "struct" | "enum"))
                .map(|declaration| (declaration.module.clone(), declaration.name.clone())),
        );
    }
    if catalog
        .scopes
        .iter()
        .flat_map(|scope| &scope.bindings)
        .any(|binding| !targets.contains(&(binding.owner_module.clone(), binding.source_name.clone())))
    {
        return None;
    }
    for scope in &catalog.scopes {
        let types = catalog
            .declarations
            .iter()
            .filter(|declaration| declaration.module == scope.module)
            .flat_map(|declaration| {
                declaration
                    .fields
                    .iter()
                    .map(|field| field.ty.as_str())
                    .chain(declaration.variants.iter().flat_map(|variant| variant.fields.iter().map(String::as_str)))
            })
            .chain(catalog.callables.iter().filter(|callable| callable.module == scope.module).flat_map(|callable| {
                callable
                    .params
                    .iter()
                    .chain(&callable.outputs)
                    .map(|parameter| parameter.r#type.as_str())
                    .chain(callable.return_type.iter().map(String::as_str))
            }))
            .chain(catalog.constants.iter().filter(|constant| constant.module == scope.module).map(|constant| constant.ty.as_str()));
        if types.into_iter().any(|ty| cellscript_artifact_checker::interface::qualified_source_type(ty, scope).is_err()) {
            return None;
        }
    }
    Some(catalog)
}

fn nominal(module: &Module, item: &Item) -> Option<NominalDeclarationContract> {
    let (name, kind, fields, abilities, capabilities, identity, type_identity) = match item {
        Item::Resource(def) => {
            (&def.name, "resource", &def.fields, &[][..], &def.capabilities[..], Some(&def.identity), def.type_id.as_ref())
        }
        Item::Shared(def) => {
            (&def.name, "shared", &def.fields, &[][..], &def.capabilities[..], Some(&def.identity), def.type_id.as_ref())
        }
        Item::Receipt(def) => {
            (&def.name, "receipt", &def.fields, &[][..], &def.capabilities[..], Some(&def.identity), def.type_id.as_ref())
        }
        Item::Struct(def) if def.type_params.is_empty() => {
            (&def.name, "struct", &def.fields, &def.abilities[..], &[][..], None, def.type_id.as_ref())
        }
        Item::Enum(def) if def.type_params.is_empty() => {
            return Some(NominalDeclarationContract {
                module: module.name.clone(),
                name: def.name.clone(),
                kind: "enum".into(),
                visibility: module.visibility_of(&def.name).as_str().into(),
                variants: def
                    .variants
                    .iter()
                    .map(|variant| TypedSemanticGenericVariant {
                        name: variant.name.clone(),
                        fields: variant.fields.iter().map(crate::generics::render_source_type).collect(),
                    })
                    .collect(),
                abilities: def.abilities.iter().map(|ability| ability.as_str().into()).collect(),
                identity_policy: "none".into(),
                ..Default::default()
            });
        }
        _ => return None,
    };
    Some(structural(module, name, kind, fields, abilities, capabilities, identity, type_identity))
}

#[allow(clippy::too_many_arguments)] // Each source type dimension stays explicit in the wire record.
fn structural(
    module: &Module,
    name: &str,
    kind: &str,
    fields: &[Field],
    abilities: &[ValueAbility],
    capabilities: &[Capability],
    identity: Option<&IdentityPolicy>,
    type_identity: Option<&TypeIdentity>,
) -> NominalDeclarationContract {
    NominalDeclarationContract {
        module: module.name.clone(),
        name: name.into(),
        kind: kind.into(),
        visibility: module.visibility_of(name).as_str().into(),
        fields: fields
            .iter()
            .map(|field| TypedSemanticGenericField { name: field.name.clone(), ty: crate::generics::render_source_type(&field.ty) })
            .collect(),
        variants: vec![],
        abilities: abilities.iter().map(|ability| ability.as_str().into()).collect(),
        capabilities: capabilities.iter().map(|capability| capability.as_str().into()).collect(),
        identity_policy: match identity.unwrap_or(&IdentityPolicy::None) {
            IdentityPolicy::None => "none".into(),
            IdentityPolicy::CkbTypeId => "ckb-type-id".into(),
            IdentityPolicy::Field(field) => format!("field:{field}"),
            IdentityPolicy::ScriptArgs => "script-args".into(),
            IdentityPolicy::SingletonType => "singleton-type".into(),
        },
        type_identity: type_identity.map(|identity| identity.value.clone()),
    }
}

fn callable(module: &Module, item: &Item) -> Option<SourceCallableContract> {
    let (name, kind, parameters, params, outputs, return_type, effect) = match item {
        Item::Action(def) => (
            &def.name,
            "action",
            vec![],
            params(&def.params),
            def.outputs
                .iter()
                .map(|output| InterfaceParam {
                    name: output.name.clone(),
                    r#type: crate::generics::render_source_type(&output.ty),
                    source: "output".into(),
                    mutable: false,
                    reference: false,
                })
                .collect(),
            def.return_type.as_ref().map(crate::generics::render_source_type),
            def.effect.as_str(),
        ),
        Item::Function(def) => (
            &def.name,
            "function",
            def.type_params
                .iter()
                .map(|parameter| InterfaceTypeParameter {
                    name: parameter.name.clone(),
                    phantom: parameter.phantom,
                    constraints: parameter.constraints.iter().map(|ability| ability.as_str().into()).collect(),
                })
                .collect(),
            params(&def.params),
            vec![],
            def.return_type.as_ref().map(crate::generics::render_source_type),
            def.effect.as_str(),
        ),
        Item::Lock(def) => (
            &def.name,
            "lock",
            vec![],
            params(&def.params),
            vec![],
            Some(crate::generics::render_source_type(&def.return_type)),
            "ReadOnly",
        ),
        _ => return None,
    };
    Some(SourceCallableContract {
        module: module.name.clone(),
        name: name.clone(),
        kind: kind.into(),
        visibility: module.visibility_of(name).as_str().into(),
        type_parameters: parameters,
        params,
        outputs,
        return_type,
        declared_effect: effect.into(),
    })
}

fn params(parameters: &[crate::ast::Param]) -> Vec<InterfaceParam> {
    parameters
        .iter()
        .map(|parameter| InterfaceParam {
            name: parameter.name.clone(),
            r#type: crate::generics::render_source_type(&parameter.ty),
            source: match parameter.source {
                crate::ast::ParamSource::LockArgs => "lock_args".into(),
                source => format!("{source:?}").to_ascii_lowercase(),
            },
            mutable: parameter.is_mut,
            reference: parameter.is_ref || parameter.is_read_ref,
        })
        .collect()
}

fn bounded(catalog: &NominalDeclarationCatalog) -> bool {
    if catalog.declarations.len() > 256
        || catalog.scopes.len() > 256
        || catalog.callables.len() > 128
        || catalog.constants.len() > 256
        || catalog.layout_bindings.len() > 256
    {
        return false;
    }
    let mut nodes = catalog.layout_bindings.len();
    if catalog
        .layout_bindings
        .iter()
        .any(|binding| [&binding.owner_module, &binding.source_name, &binding.lowered_name].iter().any(|name| name.len() > 512))
    {
        return false;
    }
    let mut names = BTreeSet::new();
    for scope in &catalog.scopes {
        if scope.module.len() > 512 || scope.bindings.len() > 256 {
            return false;
        }
        nodes = nodes.saturating_add(1).saturating_add(scope.bindings.len());
        if scope
            .bindings
            .iter()
            .any(|binding| [&binding.local_name, &binding.owner_module, &binding.source_name].iter().any(|name| name.len() > 512))
        {
            return false;
        }
    }
    for declaration in &catalog.declarations {
        if !names.insert((&declaration.module, &declaration.name))
            || declaration.module.len() > 512
            || declaration.name.len() > 512
            || declaration.fields.len() > 64
            || declaration.variants.len() > 32
            || declaration.type_identity.as_ref().is_some_and(|identity| identity.len() > 512)
        {
            return false;
        }
        nodes = nodes.saturating_add(1).saturating_add(declaration.fields.len()).saturating_add(declaration.variants.len());
        if declaration.fields.iter().any(|field| field.name.len() > 512 || !super::declarations::bounded_type(&field.ty)) {
            return false;
        }
        for variant in &declaration.variants {
            nodes = nodes.saturating_add(variant.fields.len());
            if variant.name.len() > 512
                || variant.fields.len() > 64
                || variant.fields.iter().any(|ty| !super::declarations::bounded_type(ty))
            {
                return false;
            }
        }
    }
    for callable in &catalog.callables {
        nodes = nodes
            .saturating_add(1)
            .saturating_add(callable.params.len())
            .saturating_add(callable.outputs.len())
            .saturating_add(callable.type_parameters.len());
        if callable.module.len() > 512
            || callable.name.len() > 512
            || callable.params.len() > 64
            || callable.outputs.len() > 64
            || callable.type_parameters.len() > 8
            || callable
                .params
                .iter()
                .chain(&callable.outputs)
                .any(|parameter| parameter.name.len() > 512 || !super::declarations::bounded_type(&parameter.r#type))
            || callable.return_type.as_ref().is_some_and(|ty| !super::declarations::bounded_type(ty))
        {
            return false;
        }
    }
    for constant in &catalog.constants {
        nodes = nodes.saturating_add(1);
        if constant.module.len() > 512 || constant.name.len() > 512 || !super::declarations::bounded_type(&constant.ty) {
            return false;
        }
    }
    nodes <= 4096
}
