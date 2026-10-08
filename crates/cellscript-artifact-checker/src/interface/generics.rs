//! Project instantiated public declarations onto checked concrete records.
//! This verifies actual substitutions, not the template's universal ability
//! constraints or the behavior of instances absent from the artifact.

use super::{mismatch, InterfaceTypeParameter, PackageInterface};
use crate::{CheckerError, TypedSemanticRecord};
use std::collections::BTreeMap;

fn substitutions<'a>(
    parameters: &'a [InterfaceTypeParameter],
    arguments: &[String],
) -> Result<BTreeMap<&'a str, String>, CheckerError> {
    if parameters.len() != arguments.len() {
        return Err(mismatch("interface generic arity differs from checked instantiation"));
    }
    let result = parameters
        .iter()
        .zip(arguments)
        .map(|(parameter, argument)| (parameter.name.as_str(), crate::checker::canonical_abi_type(argument)))
        .collect::<BTreeMap<_, _>>();
    if result.len() != parameters.len() {
        return Err(mismatch("interface generic parameter names are ambiguous"));
    }
    Ok(result)
}

pub(super) use crate::generic_projection::matches_type;

pub(super) fn check(
    interface: &PackageInterface,
    effective: &TypedSemanticRecord,
    instances: &BTreeMap<&str, String>,
) -> Result<(), CheckerError> {
    let abilities = crate::value_abilities::verify(effective)?;
    for instance in effective.instantiations.iter().filter(|instance| instance.module == interface.module) {
        if instance.kind == "function" {
            let Some(declared) = interface.callables.iter().find(|entry| entry.name == instance.template) else { continue };
            check_parameter_binding(&declared.type_parameters, &instance.parameters)?;
            check_function_declaration(declared, &instance.declaration)?;
            let substitutions = substitutions(&declared.type_parameters, &instance.type_arguments)?;
            check_constraints(&declared.type_parameters, &instance.type_arguments, false, &abilities)?;
            let mut entries = effective.entries.iter().filter(|entry| instance.lowered_names.contains(&entry.name));
            if let Some(entry) = entries.next() {
                if entries.next().is_some() || declared.kind != "function" {
                    return Err(mismatch("interface generic callable kind or checked entry is ambiguous"));
                }
                super::check_callable_entry(declared, entry, instances, &substitutions)?;
            }
            continue;
        }
        let Some(declared) = interface.types.iter().find(|ty| ty.name == instance.template) else { continue };
        check_parameter_binding(&declared.type_parameters, &instance.parameters)?;
        check_type_declaration(declared, &instance.declaration)?;
        let substitutions = substitutions(&declared.type_parameters, &instance.type_arguments)?;
        check_constraints(&declared.type_parameters, &instance.type_arguments, true, &abilities)?;
        if declared.kind != instance.kind {
            return Err(mismatch("interface generic kind differs from checked instantiation"));
        }
        let mut layouts = effective.types.iter().filter(|ty| instance.lowered_names.contains(&ty.name));
        let Some(checked) = layouts.next() else { continue };
        if layouts.next().is_some()
            || declared.kind != checked.kind
            || declared.fields.len() != checked.fields.len()
            || declared.variants.len() != checked.variants.len()
            || !declared.cell_capabilities.is_empty()
            || !checked.capabilities.is_empty()
            || declared.value_abilities != checked.value_abilities
        {
            return Err(mismatch("interface generic shape differs from checked concrete layout"));
        }
        // Concrete offsets and widths are checked independently by the bundle
        // checker. Templates have no offsets; their ordered substituted fields
        // must match that concrete layout, including same-width nominal types.
        let mut offset = 0u32;
        for field in &declared.fields {
            let Some(concrete) = checked.fields.iter().find(|candidate| candidate.name == field.name) else {
                return Err(mismatch("interface generic field differs from checked concrete layout"));
            };
            if concrete.offset != offset || !matches_type(&field.r#type, &concrete.ty, instances, &substitutions) {
                return Err(mismatch("interface generic field differs from checked concrete layout"));
            }
            // Zero-width fields may share an offset; the checked record sorts
            // such ties by name, while source order still determines layout.
            offset = concrete
                .width_bytes
                .and_then(|width| offset.checked_add(width))
                .ok_or_else(|| mismatch("interface generic field has no bounded checked concrete layout"))?;
        }
        for (index, (variant, concrete)) in declared.variants.iter().zip(&checked.variants).enumerate() {
            if variant.name != concrete.name || concrete.tag as usize != index || variant.fields.len() != concrete.fields.len() {
                return Err(mismatch("interface generic variant differs from checked concrete layout"));
            }
            for (field, concrete) in variant.fields.iter().zip(&concrete.fields) {
                if !matches_type(field, &concrete.ty, instances, &substitutions) {
                    return Err(mismatch("interface generic variant field differs from checked concrete layout"));
                }
            }
        }
    }
    Ok(())
}

fn check_type_declaration(
    declared: &super::InterfaceType,
    shape: &crate::TypedSemanticGenericDeclaration,
) -> Result<(), CheckerError> {
    use crate::{generic_projection::canonical_type, TypedSemanticGenericDeclaration as Declaration};
    let matches = match shape {
        Declaration::Struct { fields, abilities } => {
            declared.kind == "struct"
                && *abilities == declared.value_abilities
                && declared.variants.is_empty()
                && fields.len() == declared.fields.len()
                && declared.fields.iter().zip(fields).all(|(public, retained)| {
                    public.name == retained.name && canonical_type(&public.r#type) == canonical_type(&retained.ty)
                })
        }
        Declaration::Enum { variants, abilities } => {
            declared.kind == "enum"
                && *abilities == declared.value_abilities
                && declared.fields.is_empty()
                && variants.len() == declared.variants.len()
                && declared.variants.iter().zip(variants).all(|(public, retained)| {
                    public.name == retained.name
                        && public.fields.len() == retained.fields.len()
                        && public
                            .fields
                            .iter()
                            .zip(&retained.fields)
                            .all(|(public, retained)| canonical_type(public) == canonical_type(retained))
                })
        }
        _ => false,
    };
    if !matches {
        return Err(mismatch("public generic layout differs from its retained symbolic declaration"));
    }
    Ok(())
}

fn check_function_declaration(
    declared: &super::InterfaceCallable,
    shape: &crate::TypedSemanticGenericDeclaration,
) -> Result<(), CheckerError> {
    use crate::{generic_projection::canonical_type, TypedSemanticGenericDeclaration as Declaration};
    let Declaration::Function { params, return_type } = shape else {
        return Err(mismatch("public generic callable lacks a retained function declaration"));
    };
    if declared.kind != "function"
        || !declared.outputs.is_empty()
        || declared.params.len() != params.len()
        || canonical_type(declared.return_type.as_deref().unwrap_or("unit")) != canonical_type(return_type)
        || declared.params.iter().zip(params).any(|(public, retained)| {
            let source = if public.source == "lock_args" { "lockargs" } else { &public.source };
            public.name != retained.name
                || source != retained.source
                || public.mutable != retained.mutable
                || public.reference != retained.reference
                || canonical_type(&public.r#type) != canonical_type(&retained.ty)
        })
    {
        return Err(mismatch("public generic signature differs from its retained symbolic declaration"));
    }
    Ok(())
}

fn check_parameter_binding(
    declared: &[InterfaceTypeParameter],
    checked: &[crate::TypedSemanticGenericParameter],
) -> Result<(), CheckerError> {
    if declared.len() != checked.len()
        || declared.iter().zip(checked).any(|(declared, checked)| {
            declared.name != checked.name || declared.phantom != checked.phantom || declared.constraints != checked.constraints
        })
    {
        return Err(mismatch("interface generic parameters differ from the retained declaration contract"));
    }
    Ok(())
}

fn check_constraints(
    parameters: &[InterfaceTypeParameter],
    arguments: &[String],
    layout: bool,
    facts: &crate::value_abilities::AbilityFacts,
) -> Result<(), CheckerError> {
    use crate::value_abilities::{declared, CELL, FIXED, NON_LINEAR, SERIALIZABLE};
    let layout_required = FIXED | SERIALIZABLE | NON_LINEAR;
    for (parameter, argument) in parameters.iter().zip(arguments) {
        let required = declared(&parameter.constraints)?;
        let actual = facts.for_type(argument)?;
        if required & !actual != 0
            || (layout && !parameter.phantom && (actual & CELL != 0 || actual & layout_required != layout_required))
            || (!layout && actual & CELL != 0 && required & CELL == 0)
        {
            return Err(mismatch("interface generic constraint is not supported by checked value abilities"));
        }
    }
    Ok(())
}
