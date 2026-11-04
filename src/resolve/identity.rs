//! Semantic keys for concrete nominal values. Labels and layouts remain
//! separate: a specialization is its template owner plus resolved arguments.
use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTypeIdentity {
    /// Only non-composite primitive AST variants enter this leaf.
    Primitive(Type),
    /// The closed compiler prelude family has no user declaration owner.
    PreludeOption(Box<Self>),
    /// Its argument names a module interface, not a nominal value declaration.
    /// Source and independent artifact validation still establish admission.
    OpenHandle(OpenHandleType),
    Nominal {
        declaration: DeclarationIdentity,
        arguments: Vec<Self>,
    },
    Application {
        constructor: String,
        arguments: Vec<TypeArgument<Self>>,
        packed_domain: Option<String>,
    },
    Array(Box<Self>, usize),
    Tuple(Vec<Self>),
    Reference {
        mutable: bool,
        inner: Box<Self>,
    },
}

/// Resolution does not admit a value, establish abilities or prove a layout.
/// Missing nominal bindings, cycles and unbounded origin graphs cannot supply
/// nominal equality. Interface designations retain their separate source checks.
pub(crate) fn resolved_type_identity(
    root: &Module,
    resolver: Option<&ModuleResolver>,
    scope: &str,
    ty: &Type,
) -> Option<ResolvedTypeIdentity> {
    struct Context<'a> {
        root: &'a Module,
        resolver: Option<&'a ModuleResolver>,
        visiting: HashSet<DeclarationIdentity>,
        remaining: usize,
    }
    impl Context<'_> {
        fn binding(&self, scope: &str, name: &str) -> Option<DeclarationIdentity> {
            if let Some(resolver) = self.resolver {
                return resolver.resolve_type_identity(scope, name);
            }
            let local = name.strip_prefix(&format!("{}::", self.root.name)).unwrap_or(name);
            (scope == self.root.name
                && self.root.items.iter().any(|item| {
                    matches!(item, Item::Resource(_) | Item::Shared(_) | Item::Receipt(_) | Item::Struct(_) | Item::Enum(_))
                        && item.name() == Some(local)
                }))
            .then(|| DeclarationIdentity { module: scope.into(), name: local.into() })
        }

        fn resolve(&mut self, scope: &str, ty: &Type, depth: usize) -> Option<ResolvedTypeIdentity> {
            if depth > 32 || self.remaining == 0 {
                return None;
            }
            self.remaining -= 1;
            if let Some(handle) = ty.as_open_handle() {
                return handle.has_bounded_module_path().then_some(ResolvedTypeIdentity::OpenHandle(handle));
            }
            let key = match ty {
                Type::Named(name) => {
                    let declaration = self.binding(scope, name)?;
                    if !self.visiting.insert(declaration.clone()) {
                        return None;
                    }
                    let module =
                        if declaration.module == self.root.name { self.root } else { self.resolver?.module(&declaration.module)? };
                    let origin = module.specializations.get(&declaration.name);
                    let (owner, arguments) = match origin {
                        Some(origin) if origin.kind != SpecializationKind::Function => {
                            let arguments = origin
                                .arguments
                                .iter()
                                .map(|argument| self.resolve(&declaration.module, argument, depth + 1))
                                .collect::<Option<Vec<_>>>()?;
                            (origin.template.clone(), arguments)
                        }
                        Some(_) => return None,
                        None => (declaration.clone(), Vec::new()),
                    };
                    self.visiting.remove(&declaration);
                    if crate::generics::is_builtin_option_specialization(module, &declaration.name) {
                        let [argument] = arguments.as_slice() else { return None };
                        ResolvedTypeIdentity::PreludeOption(Box::new(argument.clone()))
                    } else {
                        ResolvedTypeIdentity::Nominal { declaration: owner, arguments }
                    }
                }
                Type::Applied(application) => {
                    let arguments = application
                        .arguments
                        .iter()
                        .map(|argument| match argument {
                            TypeArgument::Type(ty) => self.resolve(scope, ty, depth + 1).map(TypeArgument::Type),
                            TypeArgument::UnsignedConstant(value) => Some(TypeArgument::UnsignedConstant(*value)),
                        })
                        .collect::<Option<Vec<_>>>()?;
                    let packed_domain =
                        application.unary_type("Commitment").or_else(|| application.unary_type("Opening")).map(Type::source_text);
                    ResolvedTypeIdentity::Application { constructor: application.constructor.clone(), arguments, packed_domain }
                }
                Type::Array(inner, length) => ResolvedTypeIdentity::Array(Box::new(self.resolve(scope, inner, depth + 1)?), *length),
                Type::Tuple(items) if items.is_empty() => ResolvedTypeIdentity::Primitive(Type::Unit),
                Type::Tuple(items) => ResolvedTypeIdentity::Tuple(
                    items.iter().map(|item| self.resolve(scope, item, depth + 1)).collect::<Option<Vec<_>>>()?,
                ),
                Type::Ref(inner) | Type::MutRef(inner) => ResolvedTypeIdentity::Reference {
                    mutable: matches!(ty, Type::MutRef(_)),
                    inner: Box::new(self.resolve(scope, inner, depth + 1)?),
                },
                Type::Selection(_) => return None,
                primitive => ResolvedTypeIdentity::Primitive(primitive.clone()),
            };
            Some(key)
        }
    }
    let mut context = Context { root, resolver, visiting: HashSet::new(), remaining: 4096 };
    context.resolve(scope, ty, 0)
}

pub(crate) fn nominal_identities(root: &Module, resolver: Option<&ModuleResolver>) -> BTreeMap<String, Option<ResolvedTypeIdentity>> {
    let mut names = std::collections::BTreeSet::new();
    for item in &root.items {
        match item {
            Item::Resource(_) | Item::Shared(_) | Item::Receipt(_) | Item::Struct(_) | Item::Enum(_) => {
                if let Some(name) = item.name() {
                    names.insert(name.to_owned());
                    names.insert(format!("{}::{name}", root.name));
                }
            }
            Item::Use(imports) => {
                for import in &imports.imports {
                    let local = import.alias.as_ref().unwrap_or(&import.name);
                    if resolver.is_some_and(|resolver| resolver.resolve_type_identity(&root.name, local).is_some()) {
                        names.insert(local.clone());
                        names.insert(format!("{}::{}", imports.module_path.join("::"), import.name));
                    }
                }
            }
            _ => {}
        }
    }
    names
        .into_iter()
        .map(|name| {
            let identity = resolved_type_identity(root, resolver, &root.name, &Type::Named(name.clone()));
            (name, identity)
        })
        .collect()
}
