//! Retain symbolic generic signatures independently of emitted instances.
//! Availability and machine effects still come from actual typed entries.
use crate::ast::{Item, Module, TypeParam};
use cellscript_artifact_checker::{
    GenericDeclarationCatalog, GenericDeclarationContract, TypedSemanticGenericDeclaration as Declaration, TypedSemanticGenericField,
    TypedSemanticGenericParameter, TypedSemanticGenericValueParameter, TypedSemanticGenericVariant,
    GENERIC_DECLARATION_CATALOG_SCHEMA,
};

pub(crate) fn generic_catalog(module: &Module) -> Option<GenericDeclarationCatalog> {
    let mut declarations = module
        .interface_templates
        .iter()
        .filter_map(|item| {
            let (name, kind, parameters, declaration) = match item {
                Item::Struct(definition) => (
                    &definition.name,
                    "struct",
                    &definition.type_params,
                    Declaration::Struct {
                        fields: definition
                            .fields
                            .iter()
                            .map(|field| TypedSemanticGenericField {
                                name: field.name.clone(),
                                ty: crate::generics::render_source_type(&field.ty),
                            })
                            .collect(),
                        abilities: definition.abilities.iter().map(|ability| ability.as_str().to_string()).collect(),
                    },
                ),
                Item::Enum(definition) => (
                    &definition.name,
                    "enum",
                    &definition.type_params,
                    Declaration::Enum {
                        variants: definition
                            .variants
                            .iter()
                            .map(|variant| TypedSemanticGenericVariant {
                                name: variant.name.clone(),
                                fields: variant.fields.iter().map(crate::generics::render_source_type).collect(),
                            })
                            .collect(),
                        abilities: definition.abilities.iter().map(|ability| ability.as_str().to_string()).collect(),
                    },
                ),
                Item::Function(definition) => (
                    &definition.name,
                    "function",
                    &definition.type_params,
                    Declaration::Function {
                        params: definition
                            .params
                            .iter()
                            .map(|parameter| TypedSemanticGenericValueParameter {
                                name: parameter.name.clone(),
                                ty: crate::generics::render_source_type(&parameter.ty),
                                source: format!("{:?}", parameter.source).to_ascii_lowercase(),
                                mutable: parameter.is_mut,
                                reference: parameter.is_ref || parameter.is_read_ref,
                            })
                            .collect(),
                        return_type: definition
                            .return_type
                            .as_ref()
                            .map(crate::generics::render_source_type)
                            .unwrap_or_else(|| "unit".to_string()),
                    },
                ),
                _ => return None,
            };
            if parameters.is_empty() {
                return None;
            }
            Some(GenericDeclarationContract {
                module: module.name.clone(),
                name: name.clone(),
                kind: kind.to_string(),
                visibility: module.visibility_of(name).as_str().to_string(),
                parameters: parameters.iter().map(parameter_contract).collect(),
                declaration,
            })
        })
        .collect::<Vec<_>>();
    declarations.sort_by(|left, right| (&left.module, &left.name, &left.kind).cmp(&(&right.module, &right.name, &right.kind)));
    (!declarations.is_empty() && bounded(&declarations))
        .then_some(GenericDeclarationCatalog { schema: GENERIC_DECLARATION_CATALOG_SCHEMA.to_string(), declarations })
}

// This optional receipt prerequisite must not narrow the existing generic
// language. Oversized declarations keep their historical declaration-only
// interface; bounded open admission must reject missing catalog evidence.
pub(super) fn bounded(declarations: &[GenericDeclarationContract]) -> bool {
    if declarations.len() > 384 {
        return false;
    }
    let mut nodes = 0usize;
    for contract in declarations {
        if contract.module.len() > 512
            || contract.name.len() > 512
            || contract.parameters.len() > 8
            || contract.parameters.iter().any(|parameter| parameter.name.len() > 512)
        {
            return false;
        }
        nodes = nodes.saturating_add(1).saturating_add(contract.parameters.len());
        let types: Vec<&str> = match &contract.declaration {
            Declaration::Struct { fields, .. } => {
                if fields.len() > 64 || fields.iter().any(|field| field.name.len() > 512) {
                    return false;
                }
                nodes = nodes.saturating_add(fields.len());
                fields.iter().map(|field| field.ty.as_str()).collect()
            }
            Declaration::Enum { variants, .. } => {
                if variants.len() > 32 || variants.iter().any(|variant| variant.name.len() > 512 || variant.fields.len() > 64) {
                    return false;
                }
                nodes = nodes.saturating_add(variants.len());
                let fields = variants.iter().flat_map(|variant| variant.fields.iter().map(String::as_str)).collect::<Vec<_>>();
                nodes = nodes.saturating_add(fields.len());
                fields
            }
            Declaration::Function { params, return_type } => {
                if params.len() > 64 || params.iter().any(|parameter| parameter.name.len() > 512) {
                    return false;
                }
                nodes = nodes.saturating_add(params.len()).saturating_add(1);
                params.iter().map(|parameter| parameter.ty.as_str()).chain(std::iter::once(return_type.as_str())).collect()
            }
            Declaration::Unavailable => return false,
        };
        if nodes > 4096 || types.iter().any(|ty| !bounded_type(ty)) {
            return false;
        }
    }
    true
}

pub(super) fn bounded_type(ty: &str) -> bool {
    let scope = cellscript_artifact_checker::NominalDeclarationScope { module: "preflight".into(), bindings: Vec::new() };
    cellscript_artifact_checker::interface::qualified_source_type(ty, &scope).is_ok()
}

fn parameter_contract(parameter: &TypeParam) -> TypedSemanticGenericParameter {
    TypedSemanticGenericParameter {
        name: parameter.name.clone(),
        constraints: parameter.constraints.iter().map(|ability| ability.as_str().to_string()).collect(),
        phantom: parameter.phantom,
    }
}
