use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
pub struct ModuleResolver {
    modules: HashMap<String, Module>,
    module_packages: HashMap<String, String>,
    symbol_tables: HashMap<String, SymbolTable>,
    imports: HashMap<String, Vec<ImportItem>>,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    types: HashMap<String, TypeDef>,
    functions: HashMap<String, FunctionDef>,
    constants: HashMap<String, ConstantDef>,
    imported: HashMap<String, String>,
    /// Reserve module-local policy names without treating them as value types.
    cell_role_catalogs: HashSet<String>,
    data_shapes: HashSet<String>,
    collection_pools: HashSet<String>,
}

#[derive(Debug, Clone)]
pub enum TypeDef {
    Resource(ResourceDef),
    Shared(SharedDef),
    Receipt(ReceiptDef),
    Struct(StructDef),
    Enum(EnumDef),
}

#[derive(Debug, Clone)]
pub enum FunctionDef {
    Action(ActionDef),
    Function(FnDef),
    Lock(LockDef),
}

#[derive(Debug, Clone)]
pub struct ConstantDef {
    pub name: String,
    pub ty: Type,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub module_path: Vec<String>,
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

impl Default for ModuleResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ModuleResolver {
    pub fn new() -> Self {
        Self { modules: HashMap::new(), module_packages: HashMap::new(), symbol_tables: HashMap::new(), imports: HashMap::new() }
    }

    pub fn register_module(&mut self, module: Module) -> Result<()> {
        self.register_module_in_package(module, "__cellscript_single_package__")
    }

    pub fn register_module_in_package(&mut self, module: Module, package_id: impl Into<String>) -> Result<()> {
        let name = module.name.clone();
        if self.modules.contains_key(&name) {
            return Err(CompileError::new(format!("duplicate module '{}'", name), module.span));
        }

        let mut symbol_table = SymbolTable::default();

        for item in module.items.iter().chain(module.interface_templates.iter()) {
            match item {
                Item::Resource(r) => {
                    Self::insert_type_symbol(&mut symbol_table, &r.name, TypeDef::Resource(r.clone()), r.span)?;
                }
                Item::Shared(s) => {
                    Self::insert_type_symbol(&mut symbol_table, &s.name, TypeDef::Shared(s.clone()), s.span)?;
                }
                Item::Receipt(r) => {
                    Self::insert_type_symbol(&mut symbol_table, &r.name, TypeDef::Receipt(r.clone()), r.span)?;
                }
                Item::Struct(s) => {
                    Self::insert_type_symbol(&mut symbol_table, &s.name, TypeDef::Struct(s.clone()), s.span)?;
                }
                Item::Collections(def) => {
                    Self::ensure_symbol_available(&symbol_table, &def.name, def.span)?;
                    symbol_table.collection_pools.insert(def.name.clone());
                }
                Item::DataShape(def) => {
                    Self::ensure_symbol_available(&symbol_table, &def.name, def.span)?;
                    symbol_table.data_shapes.insert(def.name.clone());
                }
                Item::CellRoles(def) => {
                    Self::ensure_symbol_available(&symbol_table, &def.name, def.span)?;
                    symbol_table.cell_role_catalogs.insert(def.name.clone());
                }
                Item::Flow(_) => {}
                Item::Enum(e) => {
                    Self::insert_type_symbol(&mut symbol_table, &e.name, TypeDef::Enum(e.clone()), e.span)?;
                }
                Item::Const(c) => {
                    Self::insert_constant_symbol(
                        &mut symbol_table,
                        &c.name,
                        ConstantDef { name: c.name.clone(), ty: c.ty.clone(), value: c.value.clone() },
                        c.span,
                    )?;
                }
                Item::Action(a) => {
                    Self::insert_function_symbol(&mut symbol_table, &a.name, FunctionDef::Action(a.clone()), a.span)?;
                }
                Item::Function(f) => {
                    Self::insert_function_symbol(&mut symbol_table, &f.name, FunctionDef::Function(f.clone()), f.span)?;
                }
                Item::Lock(l) => {
                    Self::insert_function_symbol(&mut symbol_table, &l.name, FunctionDef::Lock(l.clone()), l.span)?;
                }
                Item::Invariant(_) => {}
                Item::Use(u) => {
                    for import in &u.imports {
                        let import_item = ImportItem {
                            module_path: u.module_path.clone(),
                            name: import.name.clone(),
                            alias: import.alias.clone(),
                            span: u.span,
                        };

                        self.process_import(&mut symbol_table, &import_item)?;
                        self.imports.entry(name.clone()).or_default().push(import_item);
                    }
                }
            }
        }

        self.symbol_tables.insert(name.clone(), symbol_table);
        self.module_packages.insert(name.clone(), package_id.into());
        self.modules.insert(name, module);

        Ok(())
    }

    fn insert_type_symbol(symbol_table: &mut SymbolTable, name: &str, ty: TypeDef, span: Span) -> Result<()> {
        Self::ensure_symbol_available(symbol_table, name, span)?;
        symbol_table.types.insert(name.to_string(), ty);
        Ok(())
    }

    fn insert_function_symbol(symbol_table: &mut SymbolTable, name: &str, function: FunctionDef, span: Span) -> Result<()> {
        Self::ensure_symbol_available(symbol_table, name, span)?;
        symbol_table.functions.insert(name.to_string(), function);
        Ok(())
    }

    fn insert_constant_symbol(symbol_table: &mut SymbolTable, name: &str, constant: ConstantDef, span: Span) -> Result<()> {
        Self::ensure_symbol_available(symbol_table, name, span)?;
        symbol_table.constants.insert(name.to_string(), constant);
        Ok(())
    }

    fn ensure_symbol_available(symbol_table: &SymbolTable, name: &str, span: Span) -> Result<()> {
        if symbol_table.types.contains_key(name)
            || symbol_table.functions.contains_key(name)
            || symbol_table.constants.contains_key(name)
            || symbol_table.imported.contains_key(name)
            || symbol_table.cell_role_catalogs.contains(name)
            || symbol_table.data_shapes.contains(name)
            || symbol_table.collection_pools.contains(name)
        {
            Err(CompileError::new(format!("duplicate symbol '{}'", name), span))
        } else {
            Ok(())
        }
    }

    fn process_import(&mut self, symbol_table: &mut SymbolTable, import: &ImportItem) -> Result<()> {
        if import.module_path.is_empty() || import.name.is_empty() {
            return Err(CompileError::new("empty import path", import.span));
        }

        let full_path = import.module_path.iter().chain(std::iter::once(&import.name)).cloned().collect::<Vec<_>>().join("::");
        let local_name = import.alias.clone().unwrap_or_else(|| import.name.clone());

        Self::ensure_symbol_available(symbol_table, &local_name, import.span)?;
        symbol_table.imported.insert(local_name, full_path);

        Ok(())
    }

    fn localize_type(&self, requester: &str, owner: &str, ty: &Type) -> Type {
        match ty {
            Type::Array(inner, len) => Type::Array(Box::new(self.localize_type(requester, owner, inner)), *len),
            Type::Tuple(items) => Type::Tuple(items.iter().map(|item| self.localize_type(requester, owner, item)).collect()),
            Type::Ref(inner) => Type::Ref(Box::new(self.localize_type(requester, owner, inner))),
            Type::MutRef(inner) => Type::MutRef(Box::new(self.localize_type(requester, owner, inner))),
            Type::Named(name) => {
                let expected = format!("{owner}::{name}");
                let local = self.symbol_tables.get(requester).and_then(|table| {
                    table.imported.iter().find_map(|(local, imported)| (imported == &expected).then(|| local.clone()))
                });
                Type::Named(local.unwrap_or_else(|| name.clone()))
            }
            primitive => primitive.clone(),
        }
    }

    fn localize_type_def(&self, requester: &str, owner: &str, mut ty: TypeDef) -> TypeDef {
        let localize_fields = |fields: &mut [Field]| {
            for field in fields {
                field.ty = self.localize_type(requester, owner, &field.ty);
            }
        };
        match &mut ty {
            TypeDef::Resource(def) => localize_fields(&mut def.fields),
            TypeDef::Shared(def) => localize_fields(&mut def.fields),
            TypeDef::Receipt(def) => {
                localize_fields(&mut def.fields);
                def.claim_output = def.claim_output.as_ref().map(|ty| self.localize_type(requester, owner, ty));
            }
            TypeDef::Struct(def) => localize_fields(&mut def.fields),
            TypeDef::Enum(def) => {
                for variant in &mut def.variants {
                    for field in &mut variant.fields {
                        *field = self.localize_type(requester, owner, field);
                    }
                }
            }
        }
        ty
    }

    fn localize_function_def(&self, requester: &str, owner: &str, mut function: FunctionDef) -> FunctionDef {
        let localize_params = |params: &mut [Param]| {
            for param in params {
                param.ty = self.localize_type(requester, owner, &param.ty);
            }
        };
        match &mut function {
            FunctionDef::Action(def) => {
                localize_params(&mut def.params);
                def.return_type = def.return_type.as_ref().map(|ty| self.localize_type(requester, owner, ty));
                for output in &mut def.outputs {
                    output.ty = self.localize_type(requester, owner, &output.ty);
                }
            }
            FunctionDef::Function(def) => {
                localize_params(&mut def.params);
                def.return_type = def.return_type.as_ref().map(|ty| self.localize_type(requester, owner, ty));
            }
            FunctionDef::Lock(def) => {
                localize_params(&mut def.params);
                def.return_type = self.localize_type(requester, owner, &def.return_type);
            }
        }
        function
    }

    pub fn resolve_type(&self, module: &str, name: &str) -> Option<TypeDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if !self.symbol_accessible(module, target_module, symbol) {
                return None;
            }
            return self
                .symbol_tables
                .get(target_module)
                .and_then(|table| table.types.get(symbol).cloned())
                .map(|ty| self.localize_type_def(module, target_module, ty));
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(ty) = table.types.get(name) {
                return Some(ty.clone());
            }

            if let Some(full_path) = table.imported.get(name)
                && let Some((target_module, symbol)) = full_path.rsplit_once("::")
            {
                if !self.symbol_accessible(module, target_module, symbol) {
                    return None;
                }
                return self
                    .symbol_tables
                    .get(target_module)
                    .and_then(|target_table| target_table.types.get(symbol).cloned())
                    .map(|ty| self.localize_type_def(module, target_module, ty));
            }
        }

        None
    }

    pub fn resolve_function(&self, module: &str, name: &str) -> Option<FunctionDef> {
        self.resolve_function_with_module(module, name).map(|(_, function)| function)
    }

    pub fn resolve_function_with_module(&self, module: &str, name: &str) -> Option<(String, FunctionDef)> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if !self.symbol_accessible(module, target_module, symbol) {
                return None;
            }
            return self
                .symbol_tables
                .get(target_module)
                .and_then(|table| table.functions.get(symbol).cloned())
                .map(|function| (target_module.to_string(), self.localize_function_def(module, target_module, function)));
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(func) = table.functions.get(name) {
                return Some((module.to_string(), func.clone()));
            }

            if let Some(full_path) = table.imported.get(name)
                && let Some((target_module, symbol)) = full_path.rsplit_once("::")
            {
                if !self.symbol_accessible(module, target_module, symbol) {
                    return None;
                }
                if let Some(target_table) = self.symbol_tables.get(target_module) {
                    return target_table
                        .functions
                        .get(symbol)
                        .cloned()
                        .map(|function| (target_module.to_string(), self.localize_function_def(module, target_module, function)));
                }
            }
        }

        None
    }

    pub fn resolve_constant(&self, module: &str, name: &str) -> Option<ConstantDef> {
        self.resolve_constant_with_module(module, name).map(|(_, constant)| constant)
    }

    pub fn resolve_constant_with_module(&self, module: &str, name: &str) -> Option<(String, ConstantDef)> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if !self.symbol_accessible(module, target_module, symbol) {
                return None;
            }
            return self
                .symbol_tables
                .get(target_module)
                .and_then(|table| table.constants.get(symbol).cloned().map(|constant| (target_module.to_string(), constant)));
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(constant) = table.constants.get(name) {
                return Some((module.to_string(), constant.clone()));
            }

            if let Some(full_path) = table.imported.get(name)
                && let Some((target_module, symbol)) = full_path.rsplit_once("::")
            {
                if !self.symbol_accessible(module, target_module, symbol) {
                    return None;
                }
                if let Some(target_table) = self.symbol_tables.get(target_module) {
                    return target_table.constants.get(symbol).cloned().map(|constant| (target_module.to_string(), constant));
                }
            }
        }

        None
    }

    pub fn imports_for_module(&self, module: &str) -> Vec<ImportItem> {
        self.imports.get(module).cloned().unwrap_or_default()
    }

    pub fn module(&self, module: &str) -> Option<&Module> {
        self.modules.get(module)
    }

    pub fn module_has_symbol(&self, module: &str, name: &str) -> bool {
        self.symbol_tables.get(module).is_some_and(|table| {
            table.types.contains_key(name) || table.functions.contains_key(name) || table.constants.contains_key(name)
        })
    }

    pub fn type_is_linear(&self, module: &str, name: &str) -> bool {
        matches!(self.resolve_type(module, name), Some(TypeDef::Resource(_)) | Some(TypeDef::Shared(_)) | Some(TypeDef::Receipt(_)))
    }

    pub fn type_fields(&self, module: &str, name: &str) -> Option<Vec<(String, Type)>> {
        match self.resolve_type(module, name)? {
            TypeDef::Resource(resource) => Some(resource.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Shared(shared) => Some(shared.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Receipt(receipt) => Some(receipt.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Struct(struct_def) => Some(struct_def.fields.into_iter().map(|field| (field.name, field.ty)).collect()),
            TypeDef::Enum(_) => None,
        }
    }

    pub fn get_public_symbols(&self, module: &str) -> Vec<String> {
        let mut symbols = Vec::new();

        if let Some(table) = self.symbol_tables.get(module) {
            for name in table.types.keys().filter(|name| self.symbol_is_exported(module, name)) {
                symbols.push(name.clone());
            }
            for name in table.functions.keys().filter(|name| self.symbol_is_exported(module, name)) {
                symbols.push(name.clone());
            }
        }

        symbols
    }

    pub fn check_circular_deps(&self) -> Result<()> {
        self.validate_imports()
    }

    pub fn validate_imports(&self) -> Result<()> {
        for (module, imports) in &self.imports {
            for import in imports {
                let target_module = import.module_path.join("::");
                if !self.symbol_tables.contains_key(&target_module) {
                    return Err(CompileError::new(
                        format!("module '{}' imported by '{}' not found", target_module, module),
                        import.span,
                    ));
                };
                if !self.module_has_symbol(&target_module, &import.name) {
                    return Err(CompileError::new(
                        format!("symbol '{}' imported by '{}' not found in module '{}'", import.name, module, target_module),
                        import.span,
                    ));
                }
                if !self.symbol_accessible(module, &target_module, &import.name) {
                    let visibility = self.modules[&target_module].visibility_of(&import.name);
                    return Err(CompileError::new(
                        format!(
                            "symbol '{}' imported by '{}' is {} in module '{}'",
                            import.name,
                            module,
                            visibility.as_str(),
                            target_module
                        ),
                        import.span,
                    ));
                }
            }
        }

        Ok(())
    }

    pub(crate) fn symbol_accessible(&self, requester: &str, owner: &str, symbol: &str) -> bool {
        if requester == owner {
            return true;
        }
        if crate::generics::decode_monomorph_name(symbol).is_some() {
            let expected = format!("{owner}::{symbol}");
            if !self.symbol_tables.get(requester).is_some_and(|table| table.imported.values().any(|imported| imported == &expected)) {
                return false;
            }
        }
        let Some(owner_module) = self.modules.get(owner) else {
            return false;
        };
        match owner_module.visibility_of(symbol) {
            Visibility::Private => false,
            Visibility::Package => {
                self.module_packages.contains_key(requester) && self.module_packages.get(requester) == self.module_packages.get(owner)
            }
            Visibility::LegacyPublic | Visibility::Public => true,
        }
    }

    fn symbol_is_exported(&self, owner: &str, symbol: &str) -> bool {
        self.modules.get(owner).is_some_and(|module| module.visibility_of(symbol).is_exported())
    }

    pub fn resolve_qualified_name(&self, path: &[String]) -> Option<ResolvedName> {
        if path.is_empty() {
            return None;
        }

        let full_path = path.join("::");
        if self.modules.contains_key(&full_path) {
            return Some(ResolvedName::Module(full_path));
        }

        let (module_name, symbol_name) = if path.len() == 1 {
            (path[0].clone(), None)
        } else {
            let (module_path, symbol) = path.split_at(path.len() - 1);
            (module_path.join("::"), symbol.first().map(String::as_str))
        };

        if let Some(table) = self.symbol_tables.get(&module_name) {
            let Some(symbol_name) = symbol_name else {
                return Some(ResolvedName::Module(module_name));
            };

            if let Some(ty) = table.types.get(symbol_name) {
                return Some(ResolvedName::Type(module_name.clone(), symbol_name.to_string(), ty.clone()));
            }

            if let Some(func) = table.functions.get(symbol_name) {
                return Some(ResolvedName::Function(module_name, symbol_name.to_string(), func.clone()));
            }
        }

        None
    }
}

#[derive(Debug, Clone)]
pub enum ResolvedName {
    Module(String),
    Type(String, String, TypeDef),
    Function(String, String, FunctionDef),
}

pub struct PathResolver;

impl PathResolver {
    pub fn parse_path(path: &str) -> Vec<String> {
        path.split("::").map(|s| s.to_string()).collect()
    }

    pub fn build_qualified_name(module: &str, name: &str) -> String {
        format!("{}::{}", module, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn source_module(source: &str) -> Module {
        parser::parse(&lexer::lex(source).unwrap()).unwrap()
    }

    #[test]
    fn private_symbols_are_not_importable_but_package_symbols_are() {
        let mut resolver = ModuleResolver::new();
        resolver
            .register_module(source_module(
                "module library\nprivate struct Secret { value: u64, }\npublic(package) struct Shared { value: u64, }\n",
            ))
            .unwrap();
        resolver.register_module(source_module("module consumer\nuse library::Secret\nuse library::Shared\n")).unwrap();
        let error = resolver.validate_imports().unwrap_err();
        assert!(error.message.contains("Secret") && error.message.contains("private"));
    }

    #[test]
    fn package_symbols_are_restricted_to_the_owning_package() {
        let mut resolver = ModuleResolver::new();
        resolver
            .register_module_in_package(
                source_module(
                    "module library\npublic(package) struct Shared { value: u64, }\npublic struct Exported { value: u64, }\n",
                ),
                "dependency",
            )
            .unwrap();
        resolver.register_module_in_package(source_module("module consumer\nuse library::Shared\n"), "application").unwrap();

        let error = resolver.validate_imports().unwrap_err();
        assert!(error.message.contains("Shared") && error.message.contains("public(package)"));
        assert!(resolver.resolve_type("consumer", "Shared").is_none());
        assert!(resolver.resolve_type("consumer", "library::Shared").is_none());
        assert!(resolver.resolve_type("consumer", "library::Exported").is_some());
    }

    #[test]
    fn package_symbols_remain_visible_across_modules_in_one_package() {
        let mut resolver = ModuleResolver::new();
        resolver
            .register_module_in_package(
                source_module("module library\npublic(package) struct Shared { value: u64, }\n"),
                "application",
            )
            .unwrap();
        resolver.register_module_in_package(source_module("module consumer\nuse library::Shared\n"), "application").unwrap();

        resolver.validate_imports().unwrap();
        assert!(resolver.resolve_type("consumer", "Shared").is_some());
    }

    #[test]
    fn test_module_resolver() {
        let mut resolver = ModuleResolver::new();

        let module = Module {
            name: "test".to_string(),
            interface_templates: Vec::new(),
            visibilities: Default::default(),
            items: vec![Item::Resource(ResourceDef {
                name: "Token".to_string(),
                type_id: None,
                default_hash_type: None,
                capacity_floor: None,
                capabilities: vec![Capability::Store],
                identity: IdentityPolicy::default(),
                fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                validity: None,
                span: Span::default(),
            })],
            span: Span::default(),
        };

        resolver.register_module(module).unwrap();

        let ty = resolver.resolve_type("test", "Token");
        assert!(ty.is_some());
    }

    #[test]
    fn test_grouped_use_resolves_multiple_symbols() {
        let mut resolver = ModuleResolver::new();

        resolver
            .register_module(Module {
                name: "cellscript::fungible_token".to_string(),
                interface_templates: Vec::new(),
                visibilities: Default::default(),
                items: vec![
                    Item::Resource(ResourceDef {
                        name: "Token".to_string(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        capabilities: vec![Capability::Store],
                        identity: IdentityPolicy::default(),
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                        validity: None,
                        span: Span::default(),
                    }),
                    Item::Resource(ResourceDef {
                        name: "MintAuthority".to_string(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        capabilities: vec![Capability::Store],
                        identity: IdentityPolicy::default(),
                        fields: vec![Field { name: "max_supply".to_string(), ty: Type::U64, span: Span::default() }],
                        validity: None,
                        span: Span::default(),
                    }),
                ],
                span: Span::default(),
            })
            .unwrap();

        resolver
            .register_module(Module {
                name: "cellscript::launch".to_string(),
                interface_templates: Vec::new(),
                visibilities: Default::default(),
                items: vec![Item::Use(UseStmt {
                    module_path: vec!["cellscript".to_string(), "fungible_token".to_string()],
                    imports: vec![
                        UseImport { name: "Token".to_string(), alias: None },
                        UseImport { name: "MintAuthority".to_string(), alias: None },
                    ],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();

        assert!(matches!(resolver.resolve_type("cellscript::launch", "Token"), Some(TypeDef::Resource(_))));
        assert!(matches!(resolver.resolve_type("cellscript::launch", "MintAuthority"), Some(TypeDef::Resource(_))));
    }

    #[test]
    fn test_rejects_duplicate_local_symbols() {
        let mut resolver = ModuleResolver::new();
        let err = resolver
            .register_module(Module {
                name: "test".to_string(),
                interface_templates: Vec::new(),
                visibilities: Default::default(),
                items: vec![
                    Item::Resource(ResourceDef {
                        name: "Token".to_string(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        capabilities: vec![Capability::Store],
                        identity: IdentityPolicy::default(),
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                        validity: None,
                        span: Span::default(),
                    }),
                    Item::Action(ActionDef {
                        name: "Token".to_string(),
                        params: Vec::new(),
                        return_type: Some(Type::U64),
                        outputs: Vec::new(),
                        state_edges: Vec::new(),
                        body: vec![Stmt::Return(ReturnStmt { value: Some(Expr::Integer(0)), span: Span::default() })],
                        effect: EffectClass::Pure,
                        effect_declared: false,
                        scheduler_hint: None,
                        next_surface: None,
                        doc_comment: None,
                        span: Span::default(),
                    }),
                ],
                span: Span::default(),
            })
            .unwrap_err();

        assert!(err.message.contains("duplicate symbol 'Token'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_rejects_import_alias_collisions() {
        let mut resolver = ModuleResolver::new();
        resolver
            .register_module(Module {
                name: "cellscript::token".to_string(),
                interface_templates: Vec::new(),
                visibilities: Default::default(),
                items: vec![Item::Resource(ResourceDef {
                    name: "Token".to_string(),
                    type_id: None,
                    default_hash_type: None,
                    capacity_floor: None,
                    capabilities: vec![Capability::Store],
                    identity: IdentityPolicy::default(),
                    fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                    validity: None,
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();

        let err = resolver
            .register_module(Module {
                name: "app".to_string(),
                interface_templates: Vec::new(),
                visibilities: Default::default(),
                items: vec![
                    Item::Use(UseStmt {
                        module_path: vec!["cellscript".to_string(), "token".to_string()],
                        imports: vec![UseImport { name: "Token".to_string(), alias: None }],
                        span: Span::default(),
                    }),
                    Item::Struct(StructDef {
                        name: "Token".to_string(),
                        type_params: Vec::new(),
                        abilities: Vec::new(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                        validity: None,
                        span: Span::default(),
                    }),
                ],
                span: Span::default(),
            })
            .unwrap_err();

        assert!(err.message.contains("duplicate symbol 'Token'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_path_resolver() {
        let path = PathResolver::parse_path("cellscript::fungible_token::Token");
        assert_eq!(path, vec!["cellscript", "fungible_token", "Token"]);

        let qualified = PathResolver::build_qualified_name("cellscript", "Token");
        assert_eq!(qualified, "cellscript::Token");
    }
}
