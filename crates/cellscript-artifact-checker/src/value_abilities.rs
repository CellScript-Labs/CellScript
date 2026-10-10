//! Independent value-ability registry v1 checks. Cell lifecycle authority is
//! never inferred from these local value/layout properties.

use crate::{CheckerError, CheckerRejectionCode, TypedSemanticRecord};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(crate) const COPY: u8 = 1;
pub(crate) const DROP: u8 = 2;
pub(crate) const STORE: u8 = 4;
pub(crate) const FIXED: u8 = 8;
pub(crate) const SERIALIZABLE: u8 = 16;
pub(crate) const NON_LINEAR: u8 = 32;
pub(crate) const CELL: u8 = 64;
pub(crate) const PLAIN: u8 = COPY | DROP | STORE | FIXED | SERIALIZABLE | NON_LINEAR;
const ALL: u8 = PLAIN | CELL;
const ORDER: [&str; 7] = ["copy", "drop", "store", "fixed", "serializable", "non_linear", "cell"];
const MAX_TYPE_NESTING: usize = 32;

fn invalid(message: impl Into<String>) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message)
}

pub(crate) fn declared(values: &[String]) -> Result<u8, CheckerError> {
    let mut bits = 0u8;
    let mut previous = None;
    for value in values {
        let Some(index) = ORDER.iter().position(|known| *known == value) else {
            return Err(invalid("unknown value ability"));
        };
        if previous.is_some_and(|previous| previous >= index) {
            return Err(invalid("value abilities must be unique and canonically ordered"));
        }
        bits |= 1 << index;
        previous = Some(index);
    }
    if bits & CELL != 0 && bits & NON_LINEAR != 0 {
        return Err(invalid("value abilities cannot combine cell and non_linear"));
    }
    Ok(bits)
}

/// Split only at the outer level and reject malformed/unbounded nesting.
fn split(value: &str, separator: u8) -> Result<Vec<&str>, CheckerError> {
    let mut stack = Vec::new();
    let mut start = 0;
    let mut result = Vec::new();
    for (offset, byte) in value.bytes().enumerate() {
        match byte {
            b'[' | b'(' | b'<' => {
                if stack.len() == MAX_TYPE_NESTING {
                    return Err(invalid("value-ability type nesting exceeds 32"));
                }
                stack.push(byte);
            }
            b']' | b')' | b'>' => {
                let expected = match byte {
                    b']' => b'[',
                    b')' => b'(',
                    _ => b'<',
                };
                if stack.pop() != Some(expected) {
                    return Err(invalid("unbalanced value-ability type spelling"));
                }
            }
            candidate if candidate == separator && stack.is_empty() => {
                result.push(&value[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(invalid("unbalanced value-ability type spelling"));
    }
    result.push(&value[start..]);
    Ok(result)
}

#[derive(Debug)]
enum ValueType {
    Plain,
    Leaf(String),
    Reference,
    Array(Box<ValueType>),
    Tuple(Vec<ValueType>),
}

impl ValueType {
    fn parse(value: &str, depth: usize) -> Result<Self, CheckerError> {
        if depth > MAX_TYPE_NESTING || value.is_empty() {
            return Err(invalid("empty or excessively nested value-ability type"));
        }
        // Validate the complete spelling even when an ability rule (reference
        // or Vec) does not inspect its element's abilities.
        split(value, 0)?;
        if value.starts_with('&') {
            if matches!(value, "&" | "&mut") {
                return Err(invalid("empty reference type in value-ability contract"));
            }
            return Ok(Self::Reference);
        }
        if let Some(body) = value.strip_prefix('[').and_then(|value| value.strip_suffix(']')) {
            let parts = split(body, b';')?;
            if parts.len() != 2 || parts[1].parse::<u64>().is_err() {
                return Err(invalid("invalid value-ability array type"));
            }
            return Ok(Self::Array(Box::new(Self::parse(parts[0], depth + 1)?)));
        }
        if let Some(body) = value.strip_prefix('(').and_then(|value| value.strip_suffix(')')) {
            if body.is_empty() {
                return Ok(Self::Tuple(Vec::new()));
            }
            let parts = split(body, b',')?;
            return Ok(Self::Tuple(parts.into_iter().map(|part| Self::parse(part, depth + 1)).collect::<Result<_, _>>()?));
        }
        if matches!(value, "u8" | "u16" | "u32" | "i32" | "u64" | "u128" | "bool" | "unit" | "address" | "hash") {
            return Ok(Self::Plain);
        }
        Ok(Self::Leaf(value.to_string()))
    }

    fn leaves<'a>(&'a self, output: &mut BTreeSet<&'a str>) {
        match self {
            Self::Leaf(value) => {
                output.insert(value);
            }
            Self::Array(value) => value.leaves(output),
            Self::Tuple(values) => values.iter().for_each(|value| value.leaves(output)),
            Self::Plain | Self::Reference => {}
        }
    }

    fn evaluate(&self, facts: &AbilityFacts) -> Option<u8> {
        match self {
            Self::Plain => Some(PLAIN),
            Self::Reference => Some(COPY | DROP | NON_LINEAR),
            Self::Array(value) => value.evaluate(facts),
            Self::Tuple(values) => {
                let mut values = values.iter();
                let first = values.next().map(|value| value.evaluate(facts)).unwrap_or(Some(PLAIN))?;
                values.try_fold(first, |bits, value| Some(bits & value.evaluate(facts)?))
            }
            Self::Leaf(name) => {
                let resolved = facts.aliases.get(name).unwrap_or(name);
                if let Some(bits) = facts.types.get(resolved) {
                    return *bits;
                }
                match name.as_str() {
                    "usize" | "isize" => Some(PLAIN),
                    "string" | "Vec" => Some(PLAIN & !FIXED),
                    value if value.starts_with("Vec<") && value.ends_with('>') => Some(PLAIN & !FIXED),
                    // Missing nominal evidence must not imply absence of Cell
                    // fields and thereby grant an aggregate non_linear.
                    _ => None,
                }
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct AbilityFacts {
    types: BTreeMap<String, Option<u8>>,
    aliases: BTreeMap<String, String>,
}

impl AbilityFacts {
    pub(crate) fn for_type(&self, value: &str) -> Result<u8, CheckerError> {
        ValueType::parse(&crate::checker::canonical_abi_type(value), 0)?
            .evaluate(self)
            .ok_or_else(|| invalid(format!("value abilities for '{value}' lack a complete checked contract")))
    }
}

/// Use a topological worklist rather than recursive nominal traversal. The
/// number of nodes/edges is bounded by the already checked record, and a cycle
/// cannot obtain abilities by assuming its own declarations are true.
fn open_handle_leaf(value: &str) -> Option<&'static str> {
    crate::interface::nominals::OPEN_HANDLE_CLASSES
        .iter()
        .find(|class| value.starts_with(&format!("{}<", **class)) && value.ends_with('>'))
        .copied()
}

/// An open-handle leaf's interface designation must name a module of the
/// checked source closure (a nominal-catalog scope); its identity-only
/// abilities are copy and drop.
fn register_open_handle_leaf(
    facts: &mut AbilityFacts,
    record_scope: Option<(&crate::NominalDeclarationCatalog, &crate::NominalDeclarationScope)>,
    record_module: &str,
    leaf: &str,
) -> Result<(), CheckerError> {
    let Some(class) = open_handle_leaf(leaf) else {
        return Err(invalid("open handle leaf is not a complete designation"));
    };
    let designation = leaf
        .strip_prefix(class)
        .and_then(|rest| rest.strip_prefix('<'))
        .and_then(|rest| rest.strip_suffix('>'))
        .ok_or_else(|| invalid("open handle designation is malformed"))?;
    if designation.is_empty() || designation.contains('<') || designation.split("::").any(str::is_empty) {
        return Err(invalid("open handle designation must be a plain module path"));
    }
    match record_scope {
        Some((catalog, _)) => {
            // The designation names an imported module's interface; the
            // current module's own interface is implicit and never spelled.
            if designation == record_module {
                return Err(invalid("open handle designation must name an imported module"));
            }
            if !catalog.scopes.iter().any(|scope| scope.module == designation) {
                return Err(invalid("open handle designation does not name a module of the checked source closure"));
            }
        }
        None => return Err(invalid("open handle leaf requires the declaration catalogs")),
    }
    facts.types.insert(crate::checker::canonical_abi_type(leaf), Some(COPY | DROP));
    Ok(())
}

pub(crate) fn verify(record: &TypedSemanticRecord) -> Result<AbilityFacts, CheckerError> {
    let indices = record
        .types
        .iter()
        .enumerate()
        .map(|(index, ty)| (crate::checker::canonical_abi_type(&ty.name), index))
        .collect::<BTreeMap<_, _>>();
    if indices.len() != record.types.len() {
        return Err(invalid("ambiguous type identity in value-ability graph"));
    }
    let mut facts = AbilityFacts { types: BTreeMap::new(), aliases: BTreeMap::new() };
    let mut ambiguous_aliases = BTreeSet::new();
    // Qualify alias keys and argument spellings through the record module's
    // nominal scope when the declaration catalogs exist. Two owners may export
    // the same template name; their unqualified spellings collide and stay
    // fail-closed, while qualified identities remain distinct evidence.
    let record_scope = record
        .nominal_declarations
        .as_ref()
        .map(|catalog| Ok::<_, CheckerError>((catalog, crate::interface::nominals::scope(catalog, &record.module)?)))
        .transpose()?;
    for ty in &record.types {
        let parsed = ty
            .fields
            .iter()
            .map(|field| field.ty.as_str())
            .chain(ty.variants.iter().flat_map(|variant| variant.fields.iter().map(|field| field.ty.as_str())))
            .map(|field| ValueType::parse(&crate::checker::canonical_abi_type(field), 0))
            .collect::<Result<Vec<_>, _>>()?;
        let mut leaves = BTreeSet::new();
        for field in &parsed {
            field.leaves(&mut leaves);
        }
        for leaf in leaves {
            if open_handle_leaf(leaf).is_some() {
                register_open_handle_leaf(&mut facts, record_scope, &record.module, leaf)?;
            }
        }
    }
    // Entry parameters carry the bounded handle encoding at action witness
    // positions; their designations validate like every other handle leaf.
    for entry in &record.entries {
        for param in &entry.params {
            let parsed = ValueType::parse(&crate::checker::canonical_abi_type(&param.ty), 0)?;
            let mut leaves = BTreeSet::new();
            parsed.leaves(&mut leaves);
            for leaf in leaves {
                if open_handle_leaf(leaf).is_some() {
                    register_open_handle_leaf(&mut facts, record_scope, &record.module, leaf)?;
                }
            }
        }
    }
    // Generic arguments are the identity position for handle designations;
    // register their evidence before the worklist so every consumer of the
    // returned facts resolves them.
    for instance in &record.instantiations {
        for argument in &instance.type_arguments {
            let parsed = ValueType::parse(&crate::checker::canonical_abi_type(argument), 0)?;
            let mut leaves = BTreeSet::new();
            parsed.leaves(&mut leaves);
            for leaf in leaves {
                if open_handle_leaf(leaf).is_some() {
                    register_open_handle_leaf(&mut facts, record_scope, &record.module, leaf)?;
                }
            }
        }
    }
    let insert_alias = |facts: &mut AbilityFacts, ambiguous: &mut BTreeSet<String>, alias: String, target: &str| {
        if ambiguous.contains(&alias) {
            return;
        }
        if let Some(previous) = facts.aliases.insert(alias.clone(), target.to_string())
            && previous != target
        {
            // Two declaration owners may export the same template name.
            // Keep their distinct lowered aliases; an unqualified ambiguous
            // spelling supplies no evidence for an argument constraint.
            facts.aliases.remove(&alias);
            ambiguous.insert(alias);
        }
    };
    for instance in record.instantiations.iter().filter(|instance| matches!(instance.kind.as_str(), "struct" | "enum")) {
        let target = instance.lowered_names.first().ok_or_else(|| invalid("generic instance lacks a lowered binding"))?;
        let qualified_arguments = record_scope
            .as_ref()
            .map(|(_, scope)| {
                instance
                    .type_arguments
                    .iter()
                    .map(|argument| crate::interface::nominals::qualified_source_type(argument, scope))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        for base in std::iter::once(instance.template.as_str())
            .chain(instance.lowered_names.iter().filter_map(|name| name.split_once("__mono__").map(|(base, _)| base)))
        {
            let alias = crate::checker::canonical_abi_type(&format!("{}<{}>", base, instance.type_arguments.join(",")));
            insert_alias(&mut facts, &mut ambiguous_aliases, alias, target);
            if let Some((_, scope)) = record_scope.as_ref() {
                let owner = if base == instance.template {
                    format!("{}::{}", instance.module, instance.template)
                } else {
                    match crate::interface::nominals::qualified_source_type(base, scope) {
                        Ok(qualified) => qualified,
                        Err(_) => continue,
                    }
                };
                let key = crate::checker::canonical_abi_type(&format!(
                    "{}<{}>",
                    owner,
                    qualified_arguments.as_deref().unwrap_or(&instance.type_arguments).join(",")
                ));
                insert_alias(&mut facts, &mut ambiguous_aliases, key, target);
            }
        }
    }
    let mut fields = Vec::with_capacity(record.types.len());
    let mut declared_sets = Vec::with_capacity(record.types.len());
    let mut incoming = vec![0usize; record.types.len()];
    let mut dependents = vec![Vec::new(); record.types.len()];
    let mut ready = VecDeque::new();
    for (index, ty) in record.types.iter().enumerate() {
        let declared = declared(&ty.value_abilities)?;
        declared_sets.push(declared);
        if matches!(ty.kind.as_str(), "resource" | "shared" | "receipt") {
            if declared != 0 {
                return Err(invalid("Cell kinds cannot carry ordinary value-ability declarations"));
            }
            fields.push(Vec::new());
            ready.push_back(index);
            continue;
        }
        if declared & CELL != 0 {
            return Err(invalid("ordinary value types cannot declare the Cell ability"));
        }
        let parsed = ty
            .fields
            .iter()
            .map(|field| field.ty.as_str())
            .chain(ty.variants.iter().flat_map(|variant| variant.fields.iter().map(|field| field.ty.as_str())))
            .map(|field| ValueType::parse(&crate::checker::canonical_abi_type(field), 0))
            .collect::<Result<Vec<_>, _>>()?;
        let mut leaves = BTreeSet::new();
        for field in &parsed {
            field.leaves(&mut leaves);
        }
        let dependencies = leaves
            .into_iter()
            .filter_map(|leaf| {
                let resolved = facts.aliases.get(leaf).map(String::as_str).unwrap_or(leaf);
                indices.get(resolved).copied()
            })
            .collect::<BTreeSet<_>>();
        incoming[index] = dependencies.len();
        for dependency in dependencies {
            dependents[dependency].push(index);
        }
        if incoming[index] == 0 {
            ready.push_back(index);
        }
        fields.push(parsed);
    }
    let mut completed = 0;
    while let Some(index) = ready.pop_front() {
        let ty = &record.types[index];
        let bits = if matches!(ty.kind.as_str(), "resource" | "shared" | "receipt") {
            Some(CELL | FIXED | SERIALIZABLE)
        } else {
            let mut supported = ALL;
            let mut has_cell = false;
            let mut complete = true;
            for field in &fields[index] {
                let Some(bits) = field.evaluate(&facts) else {
                    complete = false;
                    supported = 0;
                    continue;
                };
                supported &= bits;
                has_cell |= bits & CELL != 0;
            }
            if declared_sets[index] & !supported != 0 {
                return Err(invalid(format!("type '{}' declares value abilities unsupported by its fields", ty.name)));
            }
            if !complete {
                None
            } else if has_cell {
                Some((declared_sets[index] & (FIXED | SERIALIZABLE)) | CELL)
            } else {
                Some(declared_sets[index] | NON_LINEAR)
            }
        };
        facts.types.insert(crate::checker::canonical_abi_type(&ty.name), bits);
        // A concrete type's evidence must also answer its scope-qualified
        // spelling, because argument checks qualify unqualified names before
        // lookup. Unknown or lowered spellings qualify to themselves.
        if let Some((_, scope)) = record_scope.as_ref() {
            let qualified = crate::interface::nominals::qualified_source_type(&ty.name, scope)?;
            let qualified = crate::checker::canonical_abi_type(&qualified);
            if qualified != crate::checker::canonical_abi_type(&ty.name) {
                facts.types.insert(qualified, bits);
            }
        }
        completed += 1;
        for dependent in &dependents[index] {
            incoming[*dependent] -= 1;
            if incoming[*dependent] == 0 {
                ready.push_back(*dependent);
            }
        }
    }
    if completed != record.types.len() {
        return Err(invalid("cyclic value-ability dependency graph"));
    }
    verify_instantiations(record, &facts)?;
    Ok(facts)
}

fn verify_instantiations(record: &TypedSemanticRecord, facts: &AbilityFacts) -> Result<(), CheckerError> {
    let record_scope =
        record.nominal_declarations.as_ref().map(|catalog| crate::interface::nominals::scope(catalog, &record.module)).transpose()?;
    let mut bindings = BTreeSet::new();
    for instance in &record.instantiations {
        let layout = matches!(instance.kind.as_str(), "struct" | "enum");
        if instance.parameters.len() != instance.type_arguments.len()
            || instance.parameters.is_empty()
            || instance.lowered_names.is_empty()
            || !instance.lowered_names.windows(2).all(|pair| pair[0] < pair[1])
            || instance.fixed_layout_required != layout
            || instance.cell_backed_layout_rejected != layout
        {
            return Err(invalid("generic parameter contract or lowered binding is incomplete"));
        }
        let mut parameter_names = BTreeSet::new();
        for (parameter, argument) in instance.parameters.iter().zip(&instance.type_arguments) {
            if parameter.name.is_empty()
                || !parameter_names.insert(&parameter.name)
                || !parameter.name.chars().enumerate().all(|(index, character)| {
                    character == '_' || character.is_alphabetic() || (index > 0 && character.is_alphanumeric())
                })
                || (!layout && parameter.phantom)
            {
                return Err(invalid("generic parameter names or phantom declaration are invalid"));
            }
            let required = declared(&parameter.constraints)?;
            // An unqualified argument resolves through the record module's
            // scope first; without the declaration catalogs the raw spelling
            // stays fail-closed under cross-owner ambiguity.
            let checked_argument = record_scope
                .as_ref()
                .map(|scope| crate::interface::nominals::qualified_source_type(argument, scope))
                .transpose()?
                .unwrap_or_else(|| argument.clone());
            if open_handle_leaf(&checked_argument).is_some() && layout && !parameter.phantom {
                return Err(invalid("open handle designation cannot occupy a layout parameter"));
            }
            let actual = facts.for_type(&checked_argument)?;
            let fixed_value = FIXED | SERIALIZABLE | NON_LINEAR;
            if required & !actual != 0
                || (layout && !parameter.phantom && (actual & CELL != 0 || actual & fixed_value != fixed_value))
                || (!layout && actual & CELL != 0 && required & CELL == 0)
            {
                return Err(invalid("generic argument does not satisfy its retained parameter contract"));
            }
        }
        let suffix = format!("__mono__{}", crate::hex_encode(instance.type_arguments.join(",").as_bytes()));
        let mut alias_abilities = None;
        for name in &instance.lowered_names {
            if !name.ends_with(&suffix) || !bindings.insert((instance.kind.as_str(), name.as_str())) {
                return Err(invalid("generic lowered binding is ambiguous or changes its type arguments"));
            }
            if layout {
                let ty = record
                    .types
                    .iter()
                    .find(|ty| ty.name == *name && ty.kind == instance.kind)
                    .ok_or_else(|| invalid("generic layout binding is missing from checked types"))?;
                let abilities = facts.for_type(&ty.name)?;
                if alias_abilities.is_some_and(|previous| previous != abilities) {
                    return Err(invalid("generic aliases have conflicting value abilities"));
                }
                alias_abilities = Some(abilities);
            } else if !record.entries.iter().any(|entry| entry.name == *name && entry.kind == "helper") {
                return Err(invalid("generic callable binding is missing from checked entries"));
            }
        }
    }
    // The source generic kernel reserves this marker for all top-level names.
    // Every retained specialization therefore requires a declaration contract.
    for ty in &record.types {
        if ty.name.contains("__mono__") && !bindings.contains(&(ty.kind.as_str(), ty.name.as_str())) {
            return Err(invalid("specialized type lacks its generic parameter contract"));
        }
    }
    for entry in record.entries.iter().filter(|entry| entry.kind == "helper") {
        if entry.name.contains("__mono__") && !bindings.contains(&("function", entry.name.as_str())) {
            return Err(invalid("specialized callable lacks its generic parameter contract"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TypedSemanticField, TypedSemanticType};

    fn ty(name: &str, kind: &str, abilities: &[&str], fields: &[&str]) -> TypedSemanticType {
        TypedSemanticType {
            name: name.into(),
            kind: kind.into(),
            value_abilities: abilities.iter().map(|value| (*value).into()).collect(),
            fields: fields
                .iter()
                .enumerate()
                .map(|(index, value)| TypedSemanticField { name: format!("f{index}"), ty: (*value).into(), ..Default::default() })
                .collect(),
            ..Default::default()
        }
    }

    fn record(types: Vec<TypedSemanticType>) -> TypedSemanticRecord {
        TypedSemanticRecord { types, ..Default::default() }
    }

    #[test]
    fn field_validation_preserves_declared_restrictions_and_cell_identity() {
        // Reverse dependency order exercises the worklist, not input ordering.
        let facts = verify(&record(vec![
            ty("Container", "struct", &["fixed", "serializable"], &["(Token,Token)"]),
            ty("Restricted", "struct", &[], &["u64"]),
            ty("Private", "struct", &["copy", "fixed", "serializable"], &["[u64;2]"]),
            ty("Token", "resource", &[], &["u64"]),
        ]))
        .unwrap();
        assert_eq!(facts.for_type("Container").unwrap(), CELL | FIXED | SERIALIZABLE);
        assert_eq!(facts.for_type("Restricted").unwrap(), NON_LINEAR);
        assert_eq!(facts.for_type("Private").unwrap(), COPY | FIXED | SERIALIZABLE | NON_LINEAR);
        assert_eq!(facts.for_type("&Token").unwrap(), COPY | DROP | NON_LINEAR);
        assert_eq!(facts.for_type("(u64, [Hash;2], ())").unwrap(), PLAIN);
        assert_eq!(facts.for_type("Vec<Token>").unwrap(), PLAIN & !FIXED);
        assert_eq!(facts.for_type("String").unwrap(), PLAIN & !FIXED);
    }

    #[test]
    fn missing_nominal_evidence_never_grants_non_linear() {
        let facts = verify(&record(vec![
            ty("UnknownContainer", "struct", &[], &["Omitted"]),
            ty("Outer", "struct", &[], &["UnknownContainer"]),
        ]))
        .unwrap();
        for name in ["Omitted", "UnknownContainer", "Outer", "(Omitted,u64)"] {
            assert!(facts.for_type(name).is_err(), "{name}");
        }
        assert!(verify(&record(vec![ty("Forged", "struct", &["non_linear"], &["Omitted"])])).is_err());
    }

    #[test]
    fn contradictory_abilities_and_recursive_nominal_claims_reject() {
        for candidate in [
            ty("Bad", "struct", &["copy"], &["Token"]),
            ty("Bad", "struct", &["fixed"], &["String"]),
            ty("Bad", "struct", &["cell"], &[]),
            ty("Bad", "resource", &["fixed"], &[]),
            ty("Bad", "enum", &["copy", "copy"], &[]),
            ty("Bad", "enum", &["invented"], &[]),
            ty("Bad", "struct", &["drop", "copy"], &[]),
        ] {
            assert!(verify(&record(vec![candidate, ty("Token", "resource", &[], &[])])).is_err());
        }
        assert!(verify(&record(vec![ty("A", "struct", &[], &["B"]), ty("B", "struct", &[], &["A"])])).is_err());
    }

    #[test]
    fn enum_payloads_constrain_every_variant() {
        let mut choice = ty("Choice", "enum", &["copy", "fixed", "serializable"], &[]);
        choice.variants = vec![crate::TypedSemanticVariant {
            name: "Value".into(),
            fields: vec![crate::TypedSemanticVariantField { ty: "u64".into(), ..Default::default() }],
            ..Default::default()
        }];
        let mut candidate = record(vec![choice, ty("Token", "resource", &[], &[])]);
        assert_eq!(verify(&candidate).unwrap().for_type("Choice").unwrap(), COPY | FIXED | SERIALIZABLE | NON_LINEAR);
        candidate.types[0].variants.push(crate::TypedSemanticVariant {
            name: "Cell".into(),
            fields: vec![crate::TypedSemanticVariantField { ty: "Token".into(), ..Default::default() }],
            ..Default::default()
        });
        assert!(verify(&candidate).is_err());
        candidate.types[0].value_abilities = vec!["fixed".into(), "serializable".into()];
        assert_eq!(verify(&candidate).unwrap().for_type("Choice").unwrap(), FIXED | SERIALIZABLE | CELL);
    }

    #[test]
    fn parsing_is_bounded_and_rejects_malformed_compounds() {
        let facts = verify(&record(vec![])).unwrap();
        for bad in ["", "&", "&mut", "[u64]", "[u64;-1]", "(u64,)", "(u64]", "Vec<u64"] {
            assert!(facts.for_type(bad).is_err(), "{bad}");
        }
        let nested = format!("{}u64{}", "[".repeat(32), ";1]".repeat(32));
        assert_eq!(facts.for_type(&nested).unwrap(), PLAIN);
        assert!(facts.for_type(&format!("[{nested};1]")).is_err());
    }
}
