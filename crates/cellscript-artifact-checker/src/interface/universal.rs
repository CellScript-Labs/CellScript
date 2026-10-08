//! Universal declaration guarantees use parameter minima, never one instance.
//! This proves local type/ability contracts, not generic function behavior.
use super::{
    source_types::{SourceArgument, SourceType},
    InterfaceInspection,
};
use crate::{
    value_abilities::{declared, CELL, COPY, DROP, FIXED, NON_LINEAR, PLAIN, SERIALIZABLE},
    CheckerError, CheckerRejectionCode, GenericDeclarationContract, NominalDeclarationCatalog, NominalDeclarationContract,
    TypedSemanticGenericDeclaration as Shape,
};
use std::collections::{BTreeMap, BTreeSet};

fn invalid(message: &str) -> CheckerError {
    CheckerError::new(CheckerRejectionCode::V2419TypedSemanticsInvalid, message)
}

struct Contracts<'a> {
    catalog: &'a NominalDeclarationCatalog,
    nominals: BTreeMap<String, &'a NominalDeclarationContract>,
    generics: BTreeMap<String, &'a GenericDeclarationContract>,
    active: BTreeSet<String>,
    visits: usize,
}

impl Contracts<'_> {
    fn source(&mut self, value: &str, module: &str, parameters: &BTreeMap<String, u8>, depth: usize) -> Result<u8, CheckerError> {
        let scope = self
            .catalog
            .scopes
            .iter()
            .find(|scope| scope.module == module)
            .ok_or_else(|| invalid("universal declaration has no defining scope"))?;
        let names = parameters.keys().map(String::as_str).collect::<Vec<_>>();
        let ty = super::qualified_source_type_with_parameters(value, scope, &names)?;
        self.ty(&SourceType::parse(&ty)?, parameters, depth)
    }

    fn ty(&mut self, ty: &SourceType, parameters: &BTreeMap<String, u8>, depth: usize) -> Result<u8, CheckerError> {
        self.visits += 1;
        if depth > 16 || self.visits > 4096 {
            return Err(invalid("universal declaration expansion exceeds its traversal bound"));
        }
        match ty {
            SourceType::Array(inner, _) => self.ty(inner, parameters, depth + 1),
            SourceType::Tuple(fields) => {
                let mut supported = PLAIN | CELL;
                let mut cell = false;
                for field in fields {
                    let abilities = self.ty(field, parameters, depth + 1)?;
                    supported &= abilities;
                    cell |= abilities & CELL != 0;
                }
                Ok(if cell { (supported & (FIXED | SERIALIZABLE)) | CELL } else { supported & !CELL })
            }
            SourceType::Reference { inner, .. } => {
                self.ty(inner, parameters, depth + 1)?;
                Ok(COPY | DROP | NON_LINEAR)
            }
            SourceType::Named(name, arguments) => {
                if let Some(bits) = parameters.get(name) {
                    return if arguments.is_empty() { Ok(*bits) } else { Err(invalid("generic parameter is applied as a template")) };
                }
                if matches!(
                    name.as_str(),
                    "u8" | "u16" | "u32" | "i32" | "u64" | "u128" | "bool" | "unit" | "address" | "hash" | "usize" | "isize"
                ) {
                    return if arguments.is_empty() { Ok(PLAIN) } else { Err(invalid("primitive type has template arguments")) };
                }
                if name == "string" && arguments.is_empty() {
                    return Ok(PLAIN & !FIXED);
                }
                if name == "Vec" {
                    if let [SourceArgument::Type(inner)] = arguments.as_slice() {
                        let bits = self.ty(inner, parameters, depth + 1)?;
                        if bits & CELL == 0 && bits & (FIXED | SERIALIZABLE | NON_LINEAR) == FIXED | SERIALIZABLE | NON_LINEAR {
                            return Ok(PLAIN & !FIXED);
                        }
                    }
                    return Err(invalid("universal Vec item lacks its fixed owned value contract"));
                }
                if let Some(declaration) = self.nominals.get(name).copied() {
                    if !arguments.is_empty() {
                        return Err(invalid("non-generic nominal type has template arguments"));
                    }
                    self.nominal(declaration, depth + 1)
                } else if let Some(declaration) = self.generics.get(name).copied() {
                    if arguments.len() != declaration.parameters.len() {
                        return Err(invalid("universal generic application has incorrect arity"));
                    }
                    let mut substitutions = BTreeMap::new();
                    for (parameter, argument) in declaration.parameters.iter().zip(arguments) {
                        let SourceArgument::Type(argument) = argument else {
                            return Err(invalid("value generic application contains a count argument"));
                        };
                        let actual = self.ty(argument, parameters, depth + 1)?;
                        let required = declared(&parameter.constraints)?;
                        let layout = !parameter.phantom;
                        let fixed_value = FIXED | SERIALIZABLE | NON_LINEAR;
                        if required & !actual != 0 || (layout && (actual & CELL != 0 || actual & fixed_value != fixed_value)) {
                            return Err(invalid("universal generic application strengthens an unproven parameter guarantee"));
                        }
                        substitutions.insert(parameter.name.clone(), actual);
                    }
                    self.generic(declaration, &substitutions, depth + 1)
                } else {
                    Err(invalid("universal nominal type lacks a complete defining declaration"))
                }
            }
        }
    }

    fn fields<'a>(
        &mut self,
        fields: impl Iterator<Item = &'a str>,
        module: &str,
        parameters: &BTreeMap<String, u8>,
        depth: usize,
    ) -> Result<(u8, bool), CheckerError> {
        let mut supported = PLAIN | CELL;
        let mut cell = false;
        for field in fields {
            let abilities = self.source(field, module, parameters, depth + 1)?;
            supported &= abilities;
            cell |= abilities & CELL != 0;
        }
        Ok((supported, cell))
    }

    fn nominal(&mut self, declaration: &NominalDeclarationContract, depth: usize) -> Result<u8, CheckerError> {
        let identity = format!("{}::{}", declaration.module, declaration.name);
        if depth > 16 || !self.active.insert(identity.clone()) {
            return Err(invalid("universal nominal declarations form a recursive or unbounded graph"));
        }
        let fields = declaration
            .fields
            .iter()
            .map(|field| field.ty.as_str())
            .chain(declaration.variants.iter().flat_map(|variant| variant.fields.iter().map(String::as_str)));
        let (supported, cell) = self.fields(fields, &declaration.module, &BTreeMap::new(), depth)?;
        let abilities = declared(&declaration.abilities)?;
        if abilities & !supported != 0 || abilities & CELL != 0 {
            return Err(invalid("universal nominal ability is unsupported by its declared fields"));
        }
        self.active.remove(&identity);
        Ok(if matches!(declaration.kind.as_str(), "resource" | "shared" | "receipt") {
            if supported & (FIXED | SERIALIZABLE) != FIXED | SERIALIZABLE {
                return Err(invalid("universal Cell layout lacks fixed serializable fields"));
            }
            CELL | FIXED | SERIALIZABLE
        } else if cell {
            (abilities & (FIXED | SERIALIZABLE)) | CELL
        } else {
            abilities | NON_LINEAR
        })
    }

    fn generic(
        &mut self,
        declaration: &GenericDeclarationContract,
        parameters: &BTreeMap<String, u8>,
        depth: usize,
    ) -> Result<u8, CheckerError> {
        let identity = format!("{}::{}", declaration.module, declaration.name);
        if depth > 16 || !self.active.insert(identity.clone()) {
            return Err(invalid("universal generic declarations form a recursive or unbounded graph"));
        }
        let (fields, abilities): (Vec<&str>, _) = match &declaration.declaration {
            Shape::Struct { fields, abilities } => (fields.iter().map(|field| field.ty.as_str()).collect(), abilities),
            Shape::Enum { variants, abilities } => {
                (variants.iter().flat_map(|variant| variant.fields.iter().map(String::as_str)).collect(), abilities)
            }
            _ => return Err(invalid("universal value template lacks a structural declaration")),
        };
        let (supported, cell) = self.fields(fields.into_iter(), &declaration.module, parameters, depth)?;
        let abilities = declared(abilities)?;
        if cell || abilities & CELL != 0 || abilities & !supported != 0 {
            return Err(invalid("universal generic ability is unsupported by parameter minima and symbolic fields"));
        }
        self.active.remove(&identity);
        Ok(abilities | NON_LINEAR)
    }
}

pub(super) fn verify(inspection: &InterfaceInspection) -> Result<(), CheckerError> {
    let record = inspection.effective();
    let catalog = record
        .nominal_declarations
        .as_ref()
        .ok_or_else(|| invalid("open interface requires a bounded nominal declaration catalog"))?;
    let has_generic = !record.instantiations.is_empty()
        || inspection.declared().types.iter().any(|ty| !ty.type_parameters.is_empty())
        || inspection.declared().callables.iter().any(|callable| !callable.type_parameters.is_empty());
    if has_generic && record.generic_declarations.is_none() {
        return Err(invalid("open interface requires a bounded generic declaration catalog"));
    }
    let mut contracts = Contracts {
        catalog,
        nominals: catalog
            .declarations
            .iter()
            .map(|declaration| (format!("{}::{}", declaration.module, declaration.name), declaration))
            .collect(),
        generics: record
            .generic_declarations
            .iter()
            .flat_map(|catalog| &catalog.declarations)
            .filter(|declaration| matches!(declaration.kind.as_str(), "struct" | "enum"))
            .map(|declaration| (format!("{}::{}", declaration.module, declaration.name), declaration))
            .collect(),
        active: BTreeSet::new(),
        visits: 0,
    };
    for ty in &inspection.declared().types {
        let identity = format!("{}::{}", record.module, ty.name);
        if let Some(declaration) = contracts.nominals.get(&identity).copied() {
            contracts.nominal(declaration, 0)?;
        } else if let Some(declaration) = contracts.generics.get(&identity).copied() {
            let mut parameters = BTreeMap::new();
            for parameter in &declaration.parameters {
                let bits = declared(&parameter.constraints)?;
                if !parameter.phantom && bits & (FIXED | SERIALIZABLE | NON_LINEAR) != FIXED | SERIALIZABLE | NON_LINEAR {
                    return Err(invalid("public universal layout parameter lacks the fixed value boundary"));
                }
                parameters.insert(parameter.name.clone(), bits);
            }
            contracts.generic(declaration, &parameters, 0)?;
        } else {
            return Err(invalid("public type lacks its universal declaration contract"));
        }
    }
    for callable in &inspection.declared().callables {
        let parameters = callable
            .type_parameters
            .iter()
            .map(|parameter| Ok((parameter.name.clone(), declared(&parameter.constraints)?)))
            .collect::<Result<BTreeMap<_, _>, CheckerError>>()?;
        for parameter in callable.params.iter().chain(&callable.outputs) {
            contracts.source(&parameter.r#type, &record.module, &parameters, 0)?;
        }
        if let Some(ty) = &callable.return_type {
            contracts.source(ty, &record.module, &parameters, 0)?;
        }
    }
    for constant in &inspection.declared().constants {
        contracts.source(&constant.r#type, &record.module, &BTreeMap::new(), 0)?;
    }
    Ok(())
}
