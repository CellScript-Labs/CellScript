//! Bounded symbolic declarations, including templates with no machine instance.
//! This checks shape and instance agreement; universal abilities are evaluated
//! separately by receipt admission, never inferred from one concrete instance.
use super::{canonical_type, contains_parameter, invalid, unique_names};
use crate::{
    CheckerError, GenericDeclarationContract, TypedSemanticGenericDeclaration as Declaration, TypedSemanticRecord,
    GENERIC_DECLARATION_CATALOG_SCHEMA,
};
use std::collections::BTreeSet;

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value
            .chars()
            .enumerate()
            .all(|(index, character)| character == '_' || character.is_alphabetic() || (index > 0 && character.is_alphanumeric()))
}

fn text(value: &str) -> Result<(), CheckerError> {
    if value.is_empty() || value.len() > 512 {
        return Err(invalid("generic declaration type spelling exceeds its byte bound"));
    }
    crate::interface::parse_source_type(value)?;
    Ok(())
}

fn fields(contract: &GenericDeclarationContract) -> Result<Vec<&str>, CheckerError> {
    let values = match &contract.declaration {
        Declaration::Struct { fields, abilities } if contract.kind == "struct" => {
            if fields.len() > 64
                || !unique_names(fields.iter().map(|field| field.name.as_str()))
                || fields.iter().any(|field| !identifier(&field.name))
            {
                return Err(invalid("generic declaration has an invalid bounded field set"));
            }
            crate::value_abilities::declared(abilities)?;
            fields.iter().map(|field| field.ty.as_str()).collect()
        }
        Declaration::Enum { variants, abilities } if contract.kind == "enum" => {
            if variants.len() > 32
                || !unique_names(variants.iter().map(|variant| variant.name.as_str()))
                || variants.iter().any(|variant| !identifier(&variant.name) || variant.fields.len() > 64)
            {
                return Err(invalid("generic declaration has an invalid bounded variant set"));
            }
            crate::value_abilities::declared(abilities)?;
            variants.iter().flat_map(|variant| variant.fields.iter().map(String::as_str)).collect()
        }
        Declaration::Function { params, return_type } if contract.kind == "function" => {
            if params.len() > 64
                || !unique_names(params.iter().map(|parameter| parameter.name.as_str()))
                || params.iter().any(|parameter| {
                    !identifier(&parameter.name)
                        || !matches!(parameter.source.as_str(), "default" | "input" | "output" | "protected" | "witness" | "lockargs")
                })
            {
                return Err(invalid("generic declaration has an invalid bounded callable parameter set"));
            }
            params.iter().map(|parameter| parameter.ty.as_str()).chain(std::iter::once(return_type.as_str())).collect()
        }
        _ => return Err(invalid("generic declaration kind differs from its symbolic shape")),
    };
    Ok(values)
}

pub(super) fn verify(record: &TypedSemanticRecord) -> Result<(), CheckerError> {
    let Some(catalog) = &record.generic_declarations else { return Ok(()) };
    if catalog.schema != GENERIC_DECLARATION_CATALOG_SCHEMA || catalog.declarations.is_empty() || catalog.declarations.len() > 384 {
        return Err(invalid("unsupported or unbounded generic declaration catalog"));
    }
    // Count all traversed shape nodes before resolving any declaration.
    let mut count = 0usize;
    for contract in &catalog.declarations {
        let shape = match &contract.declaration {
            Declaration::Struct { fields, .. } => fields.len(),
            Declaration::Enum { variants, .. } => variants.len().saturating_add(variants.iter().map(|v| v.fields.len()).sum()),
            Declaration::Function { params, .. } => params.len().saturating_add(1),
            Declaration::Unavailable => 0,
        };
        count = count.saturating_add(1).saturating_add(contract.parameters.len()).saturating_add(shape);
    }
    if count > 4096 {
        return Err(invalid("generic declaration catalog exceeds its traversal bound"));
    }
    let mut keys = BTreeSet::new();
    for contract in &catalog.declarations {
        if contract.module.len() > 512
            || !contract.module.split("::").all(identifier)
            || !identifier(&contract.name)
            || !matches!(contract.visibility.as_str(), "legacy-public" | "public" | "public(package)" | "private")
            || contract.name.contains("__mono__")
            || !keys.insert((&contract.module, &contract.name))
            || contract.parameters.is_empty()
            || contract.parameters.len() > 8
            || !unique_names(contract.parameters.iter().map(|parameter| parameter.name.as_str()))
        {
            return Err(invalid("generic declaration identity or parameters are ambiguous or unbounded"));
        }
        let fields = fields(contract)?;
        for field in &fields {
            text(field)?;
        }
        for parameter in &contract.parameters {
            if !identifier(&parameter.name) || (contract.kind == "function" && parameter.phantom) {
                return Err(invalid("generic declaration parameter name or phantom use is invalid"));
            }
            crate::value_abilities::declared(&parameter.constraints)?;
            if contract.kind != "function" {
                let used = fields.iter().any(|field| contains_parameter(field, &parameter.name));
                if parameter.phantom == used {
                    return Err(invalid("generic declaration phantom parameter disagrees with its symbolic layout"));
                }
            }
        }
        for instance in record.instantiations.iter().filter(|instance| {
            instance.module == contract.module && instance.template == contract.name && instance.kind == contract.kind
        }) {
            if instance.parameters != contract.parameters || !same_shape(&instance.declaration, &contract.declaration) {
                return Err(invalid("generic instance differs from its retained declaration catalog"));
            }
        }
    }
    Ok(())
}

fn same_shape(left: &Declaration, right: &Declaration) -> bool {
    let normalize = |shape: &Declaration| {
        let mut shape = shape.clone();
        match &mut shape {
            Declaration::Struct { fields, .. } => fields.iter_mut().for_each(|field| field.ty = canonical_type(&field.ty)),
            Declaration::Enum { variants, .. } => {
                variants.iter_mut().flat_map(|v| &mut v.fields).for_each(|ty| *ty = canonical_type(ty))
            }
            Declaration::Function { params, return_type } => {
                params.iter_mut().for_each(|parameter| parameter.ty = canonical_type(&parameter.ty));
                *return_type = canonical_type(return_type);
            }
            Declaration::Unavailable => {}
        }
        shape
    };
    normalize(left) == normalize(right)
}
