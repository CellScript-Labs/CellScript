//! V0 commitment domains contain source spelling. Import localization may
//! rename ordinary values, but cannot rewrite a cryptographic domain.
use super::*;

impl ModuleResolver {
    pub(super) fn validate_packed_import(&self, requester: &str, owner: &str, name: &str, span: Span) -> Result<()> {
        fn same_bindings(resolver: &ModuleResolver, requester: &str, owner: &str, ty: &Type, parameters: &[TypeParam]) -> bool {
            match ty {
                Type::Named(name) if parameters.iter().any(|parameter| parameter.name == *name) => true,
                Type::Named(name) => resolver.resolve_type_identity(requester, name) == resolver.resolve_type_identity(owner, name),
                Type::Applied(application) => application
                    .arguments
                    .iter()
                    .filter_map(TypeArgument::as_type)
                    .all(|ty| same_bindings(resolver, requester, owner, ty, parameters)),
                Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => {
                    same_bindings(resolver, requester, owner, inner, parameters)
                }
                Type::Tuple(items) => items.iter().all(|ty| same_bindings(resolver, requester, owner, ty, parameters)),
                _ => true,
            }
        }
        fn check(
            resolver: &ModuleResolver,
            requester: &str,
            owner: &str,
            ty: &Type,
            parameters: &[TypeParam],
            span: Span,
        ) -> Result<()> {
            match ty {
                Type::Applied(application) => {
                    if let Some(inner) = application.unary_type("Commitment").or_else(|| application.unary_type("Opening"))
                        && !same_bindings(resolver, requester, owner, inner, parameters)
                    {
                        return Err(CompileError::new(
                            format!("imported {} requires the same V0 packed type spelling and owner in both scopes; import its value types under their original names", ty.source_text()),
                            span,
                        ).with_code(crate::error::DiagnosticCode::E2111));
                    }
                    for ty in application.arguments.iter().filter_map(TypeArgument::as_type) {
                        check(resolver, requester, owner, ty, parameters, span)?;
                    }
                }
                Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => {
                    check(resolver, requester, owner, inner, parameters, span)?
                }
                Type::Tuple(items) => {
                    for ty in items {
                        check(resolver, requester, owner, ty, parameters, span)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        let table = &self.symbol_tables[owner];
        let parameters = match (table.types.get(name), table.functions.get(name)) {
            (Some(TypeDef::Struct(def)), _) => def.type_params.as_slice(),
            (Some(TypeDef::Enum(def)), _) => def.type_params.as_slice(),
            (_, Some(FunctionDef::Function(def))) => def.type_params.as_slice(),
            _ => &[],
        };
        let mut types = Vec::new();
        if let Some(ty) = table.types.get(name) {
            match ty {
                TypeDef::Resource(def) => types.extend(def.fields.iter().map(|field| &field.ty)),
                TypeDef::Shared(def) => types.extend(def.fields.iter().map(|field| &field.ty)),
                TypeDef::Receipt(def) => {
                    types.extend(def.fields.iter().map(|field| &field.ty));
                    types.extend(def.claim_output.iter());
                }
                TypeDef::Struct(def) => types.extend(def.fields.iter().map(|field| &field.ty)),
                TypeDef::Enum(def) => types.extend(def.variants.iter().flat_map(|variant| &variant.fields)),
            }
        }
        if let Some(function) = table.functions.get(name) {
            match function {
                FunctionDef::Action(def) => {
                    types.extend(def.params.iter().map(|param| &param.ty));
                    types.extend(def.return_type.iter());
                    types.extend(def.outputs.iter().map(|output| &output.ty));
                }
                FunctionDef::Function(def) => {
                    types.extend(def.params.iter().map(|param| &param.ty));
                    types.extend(def.return_type.iter());
                }
                FunctionDef::Lock(def) => {
                    types.extend(def.params.iter().map(|param| &param.ty));
                    types.push(&def.return_type);
                }
            }
        }
        if let Some(constant) = table.constants.get(name) {
            types.push(&constant.ty);
        }
        for ty in types {
            check(self, requester, owner, ty, parameters, span)?;
        }
        Ok(())
    }
}
