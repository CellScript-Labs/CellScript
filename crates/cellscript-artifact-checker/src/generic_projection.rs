//! Independently project retained generic declarations onto concrete checked types
//! and callable signatures. This does not prove universal template bodies.

use crate::{
    CheckerError, CheckerRejectionCode, TypedSemanticGenericDeclaration as Declaration, TypedSemanticInstantiation,
    TypedSemanticRecord,
};
use std::collections::{BTreeMap, BTreeSet};
mod catalog;

/// Restore source spellings only from instantiations whose concrete-name and
/// argument identity were checked with the bundle. Replacement is a single
/// token pass: canonical arguments already use source type identities, and
/// their representation is shorter than the hex-encoded concrete name. No
/// recursive name expansion or substring substitution is permitted here.
pub(crate) fn checked_source_type(ty: &str, instances: &BTreeMap<&str, String>) -> String {
    let mut source = String::with_capacity(ty.len());
    let mut identifier = String::new();
    let flush = |source: &mut String, identifier: &mut String| {
        source.push_str(instances.get(identifier.as_str()).map(String::as_str).unwrap_or(identifier));
        identifier.clear();
    };
    // Treat a complete qualified coordinate as one token. Replacing its final
    // component alone would attach an unrelated owner to a checked alias.
    let canonical = crate::checker::canonical_abi_type(ty);
    for character in canonical.chars() {
        if character.is_alphanumeric() || character == '_' || character == ':' {
            identifier.push(character);
        } else {
            flush(&mut source, &mut identifier);
            source.push(character);
        }
    }
    flush(&mut source, &mut identifier);
    crate::checker::canonical_abi_type(&source)
}

/// Replace complete, unqualified parameter tokens once. The allocation bound
/// comes from the two input spellings, so repeated untrusted parameter tokens
/// cannot amplify a short declaration into an unbounded intermediate string.
pub(crate) fn matches_type(
    declared: &str,
    checked: &str,
    instances: &BTreeMap<&str, String>,
    substitutions: &BTreeMap<&str, String>,
) -> bool {
    let expected = checked_source_type(checked, instances);
    let limit = declared.len().saturating_add(expected.len());
    substitute_type(declared, substitutions, limit).is_some_and(|result| canonical_type(&result) == canonical_type(&expected))
}

fn substitute_type(declared: &str, substitutions: &BTreeMap<&str, String>, limit: usize) -> Option<String> {
    let mut result = String::new();
    let mut start = 0;
    while start < declared.len() {
        let mut end = start;
        for character in declared[start..].chars() {
            if !character.is_alphanumeric() && character != '_' {
                break;
            }
            end += character.len_utf8();
        }
        let replacement = if end == start {
            end += declared[start..].chars().next().expect("nonempty suffix").len_utf8();
            &declared[start..end]
        } else {
            let qualified = declared[..start].trim_end().ends_with(':') || declared[end..].trim_start().starts_with(':');
            if qualified {
                &declared[start..end]
            } else {
                substitutions.get(&declared[start..end]).map(String::as_str).unwrap_or(&declared[start..end])
            }
        };
        if replacement.len() > limit.saturating_sub(result.len()) {
            return None;
        }
        result.push_str(replacement);
        start = end;
    }
    Some(result)
}

/// Compare a declaration substitution in its own source scope against a
/// concrete artifact layout. This is a bounded projection, not an admission
/// receipt: the complete nominal catalog is independently validated before
/// typed operations can consume any resulting equivalence.
fn matches_record_type(
    declared: &str,
    checked: &str,
    instance: &TypedSemanticInstantiation,
    record: &TypedSemanticRecord,
    instances: &BTreeMap<&str, String>,
    substitutions: &BTreeMap<&str, String>,
) -> bool {
    let Some(catalog) = &record.nominal_declarations else {
        return matches_type(declared, checked, instances, substitutions);
    };
    let Some(raw) = substitute_type(declared, substitutions, 512) else { return false };
    if !crate::checker::same_packed_domains(&raw, checked) {
        return false;
    }
    let Ok(scope) = crate::interface::nominals::scope(catalog, &instance.module) else { return false };
    let parameters = instance.parameters.iter().map(|parameter| parameter.name.as_str()).collect::<Vec<_>>();
    let Ok(declared) = crate::interface::qualified_source_type_with_parameters(declared, scope, &parameters) else { return false };
    let substitutions = substitutions
        .iter()
        .map(|(parameter, argument)| {
            crate::interface::qualified_source_type(argument, scope)
                .and_then(|argument| crate::interface::checked_nominal_type(&argument, catalog, record))
                .map(|argument| (*parameter, argument))
        })
        .collect::<Result<BTreeMap<_, _>, _>>();
    let Ok(substitutions) = substitutions else { return false };
    let Some(expected) = substitute_type(&declared, &substitutions, 512) else { return false };
    crate::interface::checked_nominal_type(checked, catalog, record)
        .is_ok_and(|checked| canonical_type(&expected) == canonical_type(&checked))
}

pub(crate) fn canonical_type(value: &str) -> String {
    // Source tuples spell unit as (), while typed IR spells it as unit,
    // including inside arrays, tuple fields and nested generic arguments.
    crate::checker::canonical_abi_type(value).replace("()", "unit")
}

pub(crate) fn type_instances(record: &TypedSemanticRecord) -> BTreeMap<&str, String> {
    record
        .instantiations
        .iter()
        .filter(|instance| matches!(instance.kind.as_str(), "struct" | "enum"))
        .flat_map(|instance| {
            instance.lowered_names.iter().map(move |name| {
                let base = if is_prelude_option(&instance.template, &instance.kind, &instance.parameters, &instance.declaration) {
                    "Option"
                } else {
                    name.split_once("__mono__").map_or(instance.template.as_str(), |(base, _)| base)
                };
                (name.as_str(), format!("{}<{}>", base, instance.type_arguments.join(",")))
            })
        })
        .collect()
}

fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message)
}

/// The prelude Option family is closed. This is independent of compiler
/// provenance and must never infer builtin identity from a name or width alone.
pub(crate) fn is_prelude_option(
    name: &str,
    kind: &str,
    parameters: &[crate::TypedSemanticGenericParameter],
    declaration: &Declaration,
) -> bool {
    let fixed = |abilities: &[String]| {
        abilities.len() == 6
            && abilities.iter().map(String::as_str).collect::<BTreeSet<_>>()
                == BTreeSet::from(["copy", "drop", "store", "fixed", "serializable", "non_linear"])
    };
    let [parameter] = parameters else { return false };
    let Declaration::Enum { variants, abilities } = declaration else { return false };
    name == "Option"
        && kind == "enum"
        && parameter.name == "T"
        && !parameter.phantom
        && fixed(&parameter.constraints)
        && fixed(abilities)
        && matches!(variants.as_slice(), [none, some]
            if none.name == "None" && none.fields.is_empty()
                && some.name == "Some" && some.fields.as_slice() == ["T"])
}

/// Use only aliases belonging to one independently checked instantiation. Equal
/// layouts or unqualified template names never establish nominal equivalence.
/// Call after `verify` and value-ability/binding validation have succeeded.
pub(crate) fn nominal_aliases(record: &TypedSemanticRecord) -> BTreeMap<&str, String> {
    record
        .instantiations
        .iter()
        .filter(|instance| matches!(instance.kind.as_str(), "struct" | "enum"))
        .flat_map(|instance| {
            // A shortest existing name also bounds replacement size per token.
            let canonical = instance.lowered_names.iter().min_by_key(|name| (name.len(), *name));
            instance.lowered_names.iter().filter_map(move |name| canonical.map(|canonical| (name.as_str(), canonical.clone())))
        })
        .collect()
}

fn unique_names<'a>(names: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = BTreeSet::new();
    names.into_iter().all(|name| !name.is_empty() && seen.insert(name))
}

fn contains_parameter(value: &str, parameter: &str) -> bool {
    value.match_indices(parameter).any(|(start, _)| {
        let end = start + parameter.len();
        let word = |character: char| character.is_alphanumeric() || character == '_';
        value[..start].chars().next_back().is_none_or(|character| !word(character))
            && value[end..].chars().next().is_none_or(|character| !word(character))
            && !value[..start].trim_end().ends_with(':')
            && !value[end..].trim_start().starts_with(':')
    })
}

fn check_layout_parameters<'a>(
    instance: &TypedSemanticInstantiation,
    types: impl Iterator<Item = &'a str>,
) -> Result<(), CheckerError> {
    let fields = types.collect::<Vec<_>>();
    for parameter in &instance.parameters {
        let used = fields.iter().any(|field| contains_parameter(field, &parameter.name));
        if parameter.phantom == used {
            return Err(invalid("generic phantom parameter must be absent from layout; every non-phantom parameter must occur"));
        }
    }
    Ok(())
}

pub(crate) fn verify(record: &TypedSemanticRecord) -> Result<(), CheckerError> {
    catalog::verify(record)?;
    let instances = type_instances(record);
    let mut templates = BTreeMap::new();
    for instance in &record.instantiations {
        if instance.template == "Option"
            && !is_prelude_option(&instance.template, &instance.kind, &instance.parameters, &instance.declaration)
        {
            return Err(invalid("reserved prelude Option instance differs from its closed declaration"));
        }
        let key = (&instance.module, &instance.template, &instance.kind);
        let contract = (&instance.parameters, &instance.declaration);
        if let Some(previous) = templates.insert(key, contract)
            && previous != contract
        {
            return Err(invalid("generic instances disagree about their declaration contract"));
        }
        let substitutions = instance
            .parameters
            .iter()
            .zip(&instance.type_arguments)
            .map(|(parameter, argument)| (parameter.name.as_str(), canonical_type(argument)))
            .collect::<BTreeMap<_, _>>();
        match &instance.declaration {
            Declaration::Struct { fields, abilities } => {
                if instance.kind != "struct" || !unique_names(fields.iter().map(|field| field.name.as_str())) {
                    return Err(invalid("generic struct declaration has an invalid kind or field set"));
                }
                check_layout_parameters(instance, fields.iter().map(|field| field.ty.as_str()))?;
                for name in &instance.lowered_names {
                    let checked =
                        record.types.iter().find(|ty| ty.name == *name).ok_or_else(|| invalid("generic layout is missing"))?;
                    if checked.kind != "struct" || fields.len() != checked.fields.len() || *abilities != checked.value_abilities {
                        return Err(invalid("generic struct shape or abilities differ from its concrete layout"));
                    }
                    let fixed = checked.encoded_size.is_some() && checked.fields.iter().all(|field| field.width_bytes.is_some());
                    let mut offset = 0u32;
                    for field in fields {
                        let concrete = checked
                            .fields
                            .iter()
                            .find(|candidate| candidate.name == field.name)
                            .ok_or_else(|| invalid("generic struct field is missing from its concrete layout"))?;
                        if (fixed && concrete.offset != offset)
                            || !matches_record_type(&field.ty, &concrete.ty, instance, record, &instances, &substitutions)
                        {
                            return Err(invalid(&format!(
                                "generic struct '{}::{}' field '{}' substitution or order differs from its concrete layout: declared '{}', checked '{}', arguments {:?}",
                                instance.module, instance.template, field.name, field.ty, concrete.ty, instance.type_arguments,
                            )));
                        }
                        offset = offset
                            .checked_add(concrete.width_bytes.unwrap_or(0))
                            .ok_or_else(|| invalid("generic layout offset overflows"))?;
                    }
                }
            }
            Declaration::Enum { variants, abilities } => {
                if instance.kind != "enum" || !unique_names(variants.iter().map(|variant| variant.name.as_str())) {
                    return Err(invalid("generic enum declaration has an invalid kind or variant set"));
                }
                check_layout_parameters(instance, variants.iter().flat_map(|variant| variant.fields.iter().map(String::as_str)))?;
                for name in &instance.lowered_names {
                    let checked =
                        record.types.iter().find(|ty| ty.name == *name).ok_or_else(|| invalid("generic enum layout is missing"))?;
                    if checked.kind != "enum" || variants.len() != checked.variants.len() || *abilities != checked.value_abilities {
                        return Err(invalid("generic enum shape or abilities differ from its concrete layout"));
                    }
                    for (index, (variant, concrete)) in variants.iter().zip(&checked.variants).enumerate() {
                        if variant.name != concrete.name
                            || concrete.tag as usize != index
                            || variant.fields.len() != concrete.fields.len()
                            || variant.fields.iter().zip(&concrete.fields).any(|(field, concrete)| {
                                !matches_record_type(field, &concrete.ty, instance, record, &instances, &substitutions)
                            })
                        {
                            return Err(invalid("generic enum variant substitution or order differs from its concrete layout"));
                        }
                    }
                }
            }
            Declaration::Function { params, return_type } => {
                if instance.kind != "function" || !unique_names(params.iter().map(|param| param.name.as_str())) {
                    return Err(invalid("generic function declaration has an invalid kind or parameter set"));
                }
                for name in &instance.lowered_names {
                    let entry = record
                        .entries
                        .iter()
                        .find(|entry| entry.name == *name && entry.kind == "helper")
                        .ok_or_else(|| invalid("generic function entry is missing"))?;
                    if params.len() != entry.params.len()
                        || !matches_record_type(return_type, &entry.return_type, instance, record, &instances, &substitutions)
                        || params.iter().zip(&entry.params).any(|(param, checked)| {
                            param.name != checked.name
                                || param.source != checked.source
                                || param.mutable != checked.mutable
                                || param.reference != checked.reference
                                || !matches_record_type(&param.ty, &checked.ty, instance, record, &instances, &substitutions)
                        })
                    {
                        return Err(invalid("generic function signature substitution differs from its checked entry"));
                    }
                }
            }
            Declaration::Unavailable => return Err(invalid("generic declaration shape is unavailable")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_substitution_keeps_binders_and_free_declarations_in_their_own_scopes() {
        use crate::{
            NominalDeclarationBinding as Binding, NominalDeclarationCatalog as Catalog, NominalDeclarationScope as Scope,
            NominalLayoutBinding,
        };
        let record = TypedSemanticRecord {
            module: "consumer".into(),
            nominal_declarations: Some(Catalog {
                scopes: vec![Scope {
                    module: "declaration".into(),
                    bindings: vec![Binding { local_name: "Local".into(), owner_module: "left".into(), source_name: "Value".into() }],
                }],
                layout_bindings: [("First", "left"), ("Alias", "left"), ("Local", "right"), ("Other", "right")]
                    .into_iter()
                    .map(|(name, owner)| NominalLayoutBinding {
                        owner_module: owner.into(),
                        source_name: "Value".into(),
                        lowered_name: name.into(),
                    })
                    .collect(),
                ..Default::default()
            }),
            ..Default::default()
        };
        let instance = TypedSemanticInstantiation {
            module: "declaration".into(),
            parameters: vec![crate::TypedSemanticGenericParameter { name: "T".into(), ..Default::default() }],
            ..Default::default()
        };
        // This tests bounded comparison only. The full catalog remains a
        // separate prerequisite of artifact admission, covered by bundle tests.
        let instances = BTreeMap::new();
        let substitutions = BTreeMap::from([("T", "First".into())]);
        let matches = |declared, concrete| matches_record_type(declared, concrete, &instance, &record, &instances, &substitutions);
        assert!(matches("T", "Alias"));
        assert!(!matches("T", "Other"));
        assert!(matches("Local", "First"));
        assert!(!matches("Local", "Local"));
        assert!(matches("Commitment<T>", "Commitment<First>"));
        assert!(!matches("Commitment<T>", "Commitment<Alias>"));
    }

    #[test]
    fn nominal_aliases_preserve_distinct_declaration_owners_and_arguments() {
        let instance = |module: &str, arguments: &[&str], names: &[&str]| TypedSemanticInstantiation {
            module: module.into(),
            template: "Pair".into(),
            kind: "struct".into(),
            type_arguments: arguments.iter().map(|value| (*value).into()).collect(),
            lowered_names: names.iter().map(|value| (*value).into()).collect(),
            ..Default::default()
        };
        let record = TypedSemanticRecord {
            instantiations: vec![
                instance("owner", &["u64"], &["Duo__mono__753634", "Pair__mono__753634"]),
                instance("other", &["u64"], &["Other__mono__753634"]),
                instance("owner", &["Hash"], &["Pair__mono__48617368"]),
            ],
            ..Default::default()
        };
        let aliases = nominal_aliases(&record);
        let canonical = |ty| checked_source_type(ty, &aliases);
        assert_eq!(canonical("&Pair__mono__753634"), canonical("&Duo__mono__753634"));
        assert_eq!(canonical("[Pair__mono__753634;2]"), canonical("[Duo__mono__753634;2]"));
        assert_ne!(canonical("Pair__mono__753634"), canonical("Other__mono__753634"));
        assert_ne!(canonical("Pair__mono__753634"), canonical("Pair__mono__48617368"));
        assert_ne!(canonical("Pair__mono__753634"), canonical("αPair__mono__753634"));
    }

    #[test]
    fn source_projection_replaces_only_complete_qualified_aliases() {
        let aliases =
            BTreeMap::from([("Value__mono__753634", "Local<u64>".into()), ("provider::Value__mono__753634", "Provider<u64>".into())]);
        assert_eq!(checked_source_type("[provider :: Value__mono__753634; 2]", &aliases), "[Provider<u64>;2]");
        assert_eq!(checked_source_type("Value__mono__753634", &aliases), "Local<u64>");
        assert_eq!(checked_source_type("other::Value__mono__753634", &aliases), "other::Value__mono__753634");
        assert_eq!(checked_source_type("provider::Value__mono__753634_extra", &aliases), "provider::Value__mono__753634_extra");
    }

    #[test]
    fn phantom_occurrences_respect_complete_unqualified_type_tokens() {
        for ty in ["T", "[T;0]", "(u64,&T)", "Pair<Vec<T>>", "module::Pair<T>"] {
            assert!(contains_parameter(ty, "T"), "{ty}");
        }
        for ty in ["TT", "_T", "T_value", "module::T", "T::Member", "module :: T", "Pair<TT>", "αT", "Tα"] {
            assert!(!contains_parameter(ty, "T"), "{ty}");
        }
        assert!(contains_parameter("盒<数>", "数"));
    }

    #[test]
    fn nested_units_match_ir_without_erasing_nominal_names_or_qualified_parameters() {
        let instances = BTreeMap::new();
        let substitutions = BTreeMap::from([("T", "u64".to_string())]);
        assert!(matches_type("(T,(),[();2])", "(u64,unit,[unit;2])", &instances, &substitutions));
        assert!(!matches_type("(T,())", "(Hash,unit)", &instances, &substitutions));
        assert!(matches_type("module::T", "module::T", &instances, &substitutions));
        assert!(!matches_type("module::T", "module::u64", &instances, &substitutions));
        assert!(matches_type("αT", "αT", &instances, &substitutions));
        let large = BTreeMap::from([("T", "X".repeat(1024))]);
        assert!(!matches_type("(T,T,T)", "(u64,u64,u64)", &instances, &large));
    }
}
