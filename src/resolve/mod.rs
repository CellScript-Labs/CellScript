use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use std::collections::HashMap;

#[derive(Debug)]
pub struct ModuleResolver {
    modules: HashMap<String, Module>,
    symbol_tables: HashMap<String, SymbolTable>,
    imports: HashMap<String, Vec<ImportItem>>,
}

#[derive(Debug, Clone, Default)]
pub struct SymbolTable {
    types: HashMap<String, TypeDef>,
    functions: HashMap<String, FunctionDef>,
    constants: HashMap<String, ConstantDef>,
    imported: HashMap<String, String>,
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
        Self { modules: HashMap::new(), symbol_tables: HashMap::new(), imports: HashMap::new() }
    }

    pub fn register_module(&mut self, module: Module) -> Result<()> {
        let name = module.name.clone();
        if self.modules.contains_key(&name) {
            return Err(CompileError::new(format!("duplicate module '{}'", name), module.span));
        }

        let mut symbol_table = SymbolTable::default();

        for item in &module.items {
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

    pub fn resolve_type(&self, module: &str, name: &str) -> Option<TypeDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            return self.symbol_tables.get(target_module).and_then(|table| table.types.get(symbol).cloned());
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(ty) = table.types.get(name) {
                return Some(ty.clone());
            }

            if let Some(full_path) = table.imported.get(name) {
                if let Some((target_module, symbol)) = full_path.rsplit_once("::") {
                    return self.symbol_tables.get(target_module).and_then(|target_table| target_table.types.get(symbol).cloned());
                }
            }
        }

        None
    }

    pub fn resolve_function(&self, module: &str, name: &str) -> Option<FunctionDef> {
        self.resolve_function_with_module(module, name).map(|(_, function)| function)
    }

    pub fn resolve_function_with_module(&self, module: &str, name: &str) -> Option<(String, FunctionDef)> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            return self
                .symbol_tables
                .get(target_module)
                .and_then(|table| table.functions.get(symbol).cloned().map(|function| (target_module.to_string(), function)));
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(func) = table.functions.get(name) {
                return Some((module.to_string(), func.clone()));
            }

            if let Some(full_path) = table.imported.get(name) {
                if let Some((target_module, symbol)) = full_path.rsplit_once("::") {
                    if let Some(target_table) = self.symbol_tables.get(target_module) {
                        return target_table.functions.get(symbol).cloned().map(|function| (target_module.to_string(), function));
                    }
                }
            }
        }

        None
    }

    pub fn resolve_constant(&self, module: &str, name: &str) -> Option<ConstantDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            return self.symbol_tables.get(target_module).and_then(|table| table.constants.get(symbol).cloned());
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(constant) = table.constants.get(name) {
                return Some(constant.clone());
            }

            if let Some(full_path) = table.imported.get(name) {
                if let Some((target_module, symbol)) = full_path.rsplit_once("::") {
                    if let Some(target_table) = self.symbol_tables.get(target_module) {
                        return target_table.constants.get(symbol).cloned();
                    }
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
            for name in table.types.keys() {
                symbols.push(name.clone());
            }
            for name in table.functions.keys() {
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
            }
        }

        Ok(())
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

    #[test]
    fn test_module_resolver() {
        let mut resolver = ModuleResolver::new();

        let module = Module {
            name: "test".to_string(),
            items: vec![Item::Resource(ResourceDef {
                name: "Token".to_string(),
                type_id: None,
                default_hash_type: None,
                capacity_floor: None,
                capabilities: vec![Capability::Store],
                identity: IdentityPolicy::default(),
                fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
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
                items: vec![
                    Item::Resource(ResourceDef {
                        name: "Token".to_string(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        capabilities: vec![Capability::Store],
                        identity: IdentityPolicy::default(),
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
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
                        span: Span::default(),
                    }),
                ],
                span: Span::default(),
            })
            .unwrap();

        resolver
            .register_module(Module {
                name: "cellscript::launch".to_string(),
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
                items: vec![
                    Item::Resource(ResourceDef {
                        name: "Token".to_string(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        capabilities: vec![Capability::Store],
                        identity: IdentityPolicy::default(),
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
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
                items: vec![Item::Resource(ResourceDef {
                    name: "Token".to_string(),
                    type_id: None,
                    default_hash_type: None,
                    capacity_floor: None,
                    capabilities: vec![Capability::Store],
                    identity: IdentityPolicy::default(),
                    fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();

        let err = resolver
            .register_module(Module {
                name: "app".to_string(),
                items: vec![
                    Item::Use(UseStmt {
                        module_path: vec!["cellscript".to_string(), "token".to_string()],
                        imports: vec![UseImport { name: "Token".to_string(), alias: None }],
                        span: Span::default(),
                    }),
                    Item::Struct(StructDef {
                        name: "Token".to_string(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
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
