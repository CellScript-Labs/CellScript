use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::type_graph::{visit_type_dependency_graph, TypeDependencyEdge};
use std::collections::HashMap;

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

impl SymbolTable {
    fn contains_symbol(&self, name: &str) -> bool {
        self.types.contains_key(name)
            || self.functions.contains_key(name)
            || self.constants.contains_key(name)
            || self.imported.contains_key(name)
    }
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
        if name == "verifier" || name.starts_with("verifier::") {
            return Err(CompileError::new(
                "module namespace 'verifier::*' is reserved for compiler-owned verifier capabilities",
                module.span,
            ));
        }
        if self.modules.contains_key(&name) {
            return Err(CompileError::new(format!("duplicate module '{}'", name), module.span));
        }

        let mut symbol_table = SymbolTable::default();
        let mut pending_imports = Vec::new();

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
                        pending_imports.push(import_item);
                    }
                }
            }
        }

        self.validate_pending_imports_against_module(&name, &symbol_table, &pending_imports)?;
        self.validate_existing_imports_targeting(&name, &symbol_table)?;

        self.symbol_tables.insert(name.clone(), symbol_table);
        if !pending_imports.is_empty() {
            self.imports.entry(name.clone()).or_default().extend(pending_imports);
        }
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
        if symbol_table.contains_symbol(name) {
            Err(CompileError::new(format!("duplicate symbol '{}'", name), span))
        } else {
            Ok(())
        }
    }

    fn sorted_symbol_tables(&self) -> Vec<(&str, &SymbolTable)> {
        let mut tables = self.symbol_tables.iter().map(|(module, table)| (module.as_str(), table)).collect::<Vec<_>>();
        tables.sort_by(|(left, _), (right, _)| left.cmp(right));
        tables
    }

    fn process_import(&self, symbol_table: &mut SymbolTable, import: &ImportItem) -> Result<()> {
        if import.module_path.is_empty() || import.name.is_empty() {
            return Err(CompileError::new("empty import path", import.span));
        }

        let full_path = import.module_path.iter().chain(std::iter::once(&import.name)).cloned().collect::<Vec<_>>().join("::");
        let local_name = import.alias.clone().unwrap_or_else(|| import.name.clone());

        Self::ensure_symbol_available(symbol_table, &local_name, import.span)?;
        self.validate_loaded_import_target(import)?;
        symbol_table.imported.insert(local_name, full_path);

        Ok(())
    }

    fn validate_loaded_import_target(&self, import: &ImportItem) -> Result<()> {
        let target_module = import.module_path.join("::");
        let Some(target_table) = self.symbol_tables.get(&target_module) else {
            return Ok(());
        };
        Self::validate_import_symbol(target_table, &target_module, import)
    }

    fn validate_pending_imports_against_module(
        &self,
        module_name: &str,
        symbol_table: &SymbolTable,
        pending_imports: &[ImportItem],
    ) -> Result<()> {
        for import in pending_imports {
            if import.module_path.join("::") == module_name {
                Self::validate_import_symbol(symbol_table, module_name, import)?;
            }
        }
        Ok(())
    }

    fn validate_existing_imports_targeting(&self, module_name: &str, symbol_table: &SymbolTable) -> Result<()> {
        for imports in self.imports.values() {
            for import in imports {
                if import.module_path.join("::") == module_name {
                    Self::validate_import_symbol(symbol_table, module_name, import)?;
                }
            }
        }
        Ok(())
    }

    fn validate_import_symbol(symbol_table: &SymbolTable, target_module: &str, import: &ImportItem) -> Result<()> {
        if symbol_table.contains_symbol(&import.name) {
            Ok(())
        } else {
            Err(CompileError::new(format!("symbol '{}' not found in module '{}'", import.name, target_module), import.span))
        }
    }

    pub fn resolve_type(&self, module: &str, name: &str) -> Option<TypeDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if let Some(table) = self.symbol_tables.get(target_module) {
                return table.types.get(symbol).cloned();
            }
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(ty) = table.types.get(name) {
                return Some(ty.clone());
            }

            if let Some(full_path) = table.imported.get(name) {
                if let Some((target_module, type_name)) = full_path.rsplit_once("::") {
                    if let Some(target_table) = self.symbol_tables.get(target_module) {
                        return target_table.types.get(type_name).cloned();
                    }
                }
            }
        }

        self.resolve_type_global(name)
    }

    pub fn resolve_type_with_module(&self, module: &str, name: &str) -> Option<(String, TypeDef)> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if let Some(table) = self.symbol_tables.get(target_module) {
                return table.types.get(symbol).cloned().map(|ty| (target_module.to_string(), ty));
            }
        }

        if let Some(table) = self.symbol_tables.get(module) {
            if let Some(ty) = table.types.get(name) {
                return Some((module.to_string(), ty.clone()));
            }

            if let Some(full_path) = table.imported.get(name) {
                if let Some((target_module, type_name)) = full_path.rsplit_once("::") {
                    if let Some(target_table) = self.symbol_tables.get(target_module) {
                        return target_table.types.get(type_name).cloned().map(|ty| (target_module.to_string(), ty));
                    }
                }
            }
        }

        self.resolve_type_global_with_module(name)
    }

    fn resolve_type_global_with_module(&self, name: &str) -> Option<(String, TypeDef)> {
        let symbol = terminal_path_symbol(name);
        let mut matches = self
            .sorted_symbol_tables()
            .into_iter()
            .filter_map(|(module, table)| table.types.get(symbol).cloned().map(|ty| (module.to_string(), ty)));
        let resolved = matches.next()?;
        matches.next().is_none().then_some(resolved)
    }

    pub fn resolve_function(&self, module: &str, name: &str) -> Option<FunctionDef> {
        self.resolve_function_with_module(module, name).map(|(_, function)| function)
    }

    pub fn resolve_function_with_module(&self, module: &str, name: &str) -> Option<(String, FunctionDef)> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if let Some(table) = self.symbol_tables.get(target_module) {
                return table.functions.get(symbol).cloned().map(|function| (target_module.to_string(), function));
            }
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

        self.resolve_function_global_with_module(name)
    }

    pub fn resolve_constant(&self, module: &str, name: &str) -> Option<ConstantDef> {
        if let Some((target_module, symbol)) = name.rsplit_once("::") {
            if let Some(table) = self.symbol_tables.get(target_module) {
                return table.constants.get(symbol).cloned();
            }
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

        self.resolve_constant_global(name)
    }

    pub fn resolve_type_global(&self, name: &str) -> Option<TypeDef> {
        self.resolve_type_global_with_module(name).map(|(_, ty)| ty)
    }

    pub fn resolve_function_global(&self, name: &str) -> Option<FunctionDef> {
        self.resolve_function_global_with_module(name).map(|(_, function)| function)
    }

    pub fn resolve_function_global_with_module(&self, name: &str) -> Option<(String, FunctionDef)> {
        let symbol = terminal_path_symbol(name);
        let mut matches = self
            .sorted_symbol_tables()
            .into_iter()
            .filter_map(|(module, table)| table.functions.get(symbol).cloned().map(|function| (module.to_string(), function)));
        let resolved = matches.next()?;
        matches.next().is_none().then_some(resolved)
    }

    pub fn resolve_constant_global(&self, name: &str) -> Option<ConstantDef> {
        let symbol = terminal_path_symbol(name);
        let mut matches = self.sorted_symbol_tables().into_iter().filter_map(|(_, table)| table.constants.get(symbol).cloned());
        let resolved = matches.next()?;
        matches.next().is_none().then_some(resolved)
    }

    pub fn imports_for_module(&self, module: &str) -> Vec<ImportItem> {
        self.imports.get(module).cloned().unwrap_or_default()
    }

    pub fn module(&self, module: &str) -> Option<&Module> {
        self.modules.get(module)
    }

    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
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
        self.validate_imports()?;
        self.check_type_dependency_cycles()?;
        Ok(())
    }

    fn validate_imports(&self) -> Result<()> {
        for imports in self.imports.values() {
            for import in imports {
                let target_module = import.module_path.join("::");
                let Some(target_table) = self.symbol_tables.get(&target_module) else {
                    return Err(CompileError::new(format!("module '{}' not found", target_module), import.span));
                };
                if !target_table.contains_symbol(&import.name) {
                    return Err(CompileError::new(
                        format!("symbol '{}' not found in module '{}'", import.name, target_module),
                        import.span,
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_type_dependency_cycles(&self) -> Result<()> {
        let mut graph = HashMap::<String, Vec<TypeDependencyEdge>>::new();

        for (module_name, module) in &self.modules {
            for item in &module.items {
                let Some((type_name, _, _, _)) = type_item_parts(item) else {
                    continue;
                };
                graph.entry(format!("{}::{}", module_name, type_name)).or_default();
            }
        }

        for (module_name, module) in &self.modules {
            for item in &module.items {
                let Some((type_name, fields, claim_output, enum_fields)) = type_item_parts(item) else {
                    continue;
                };
                let owner = format!("{}::{}", module_name, type_name);
                for field in fields {
                    self.collect_type_dependencies(module_name, &owner, &field.ty, field.span, &mut graph);
                }
                if let Some((ty, span)) = claim_output {
                    self.collect_type_dependencies(module_name, &owner, ty, span, &mut graph);
                }
                for (ty, span) in enum_fields {
                    self.collect_type_dependencies(module_name, &owner, ty, span, &mut graph);
                }
            }
        }

        let mut states = HashMap::new();
        let mut stack = Vec::new();
        for name in graph.keys().cloned().collect::<Vec<_>>() {
            if !states.contains_key(&name) {
                visit_type_dependency_graph(&name, &graph, &mut states, &mut stack)?;
            }
        }
        Ok(())
    }

    fn collect_type_dependencies(
        &self,
        module_name: &str,
        owner: &str,
        ty: &Type,
        span: Span,
        graph: &mut HashMap<String, Vec<TypeDependencyEdge>>,
    ) {
        match ty {
            Type::Array(inner, _) | Type::Ref(inner) | Type::MutRef(inner) => {
                self.collect_type_dependencies(module_name, owner, inner, span, graph);
            }
            Type::Tuple(items) => {
                for item in items {
                    self.collect_type_dependencies(module_name, owner, item, span, graph);
                }
            }
            Type::Named(name) => self.collect_named_type_dependencies(module_name, owner, name, span, graph),
            Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::U128 | Type::Bool | Type::Unit | Type::Address | Type::Hash => {}
        }
    }

    fn collect_named_type_dependencies(
        &self,
        module_name: &str,
        owner: &str,
        name: &str,
        span: Span,
        graph: &mut HashMap<String, Vec<TypeDependencyEdge>>,
    ) {
        let mut token = String::new();
        for ch in name.chars().chain(std::iter::once(' ')) {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == ':' {
                token.push(ch);
                continue;
            }
            self.push_named_type_dependency(module_name, owner, &token, span, graph);
            token.clear();
        }
    }

    fn push_named_type_dependency(
        &self,
        module_name: &str,
        owner: &str,
        token: &str,
        span: Span,
        graph: &mut HashMap<String, Vec<TypeDependencyEdge>>,
    ) {
        if token.is_empty() || matches!(token, "String" | "Range" | "Vec" | "usize" | "isize") {
            return;
        }
        let Some((target_module, target_def)) = self.resolve_type_with_module(module_name, token) else {
            return;
        };
        let target = format!("{}::{}", target_module, type_def_name(&target_def));
        if graph.contains_key(&target) {
            graph.entry(owner.to_string()).or_default().push(TypeDependencyEdge { target, span });
        }
    }

    pub fn resolve_qualified_name(&self, path: &[String]) -> Option<ResolvedName> {
        if path.is_empty() {
            return None;
        }

        let module_name = &path[0];

        if let Some(table) = self.symbol_tables.get(module_name) {
            if path.len() == 1 {
                return Some(ResolvedName::Module(module_name.clone()));
            }

            let symbol_name = &path[1];

            if let Some(ty) = table.types.get(symbol_name) {
                return Some(ResolvedName::Type(module_name.clone(), symbol_name.clone(), ty.clone()));
            }

            if let Some(func) = table.functions.get(symbol_name) {
                return Some(ResolvedName::Function(module_name.clone(), symbol_name.clone(), func.clone()));
            }
        }

        None
    }
}

fn terminal_path_symbol(name: &str) -> &str {
    name.rsplit("::").next().expect("str::rsplit always yields at least one element")
}

fn type_def_name(type_def: &TypeDef) -> &str {
    match type_def {
        TypeDef::Resource(resource) => &resource.name,
        TypeDef::Shared(shared) => &shared.name,
        TypeDef::Receipt(receipt) => &receipt.name,
        TypeDef::Struct(struct_def) => &struct_def.name,
        TypeDef::Enum(enum_def) => &enum_def.name,
    }
}

type TypeItemParts<'a> = (&'a str, &'a [Field], Option<(&'a Type, Span)>, Vec<(&'a Type, Span)>);

fn type_item_parts(item: &Item) -> Option<TypeItemParts<'_>> {
    match item {
        Item::Resource(resource) => Some((&resource.name, &resource.fields, None, Vec::new())),
        Item::Shared(shared) => Some((&shared.name, &shared.fields, None, Vec::new())),
        Item::Receipt(receipt) => {
            Some((&receipt.name, &receipt.fields, receipt.claim_output.as_ref().map(|ty| (ty, receipt.span)), Vec::new()))
        }
        Item::Struct(struct_def) => Some((&struct_def.name, &struct_def.fields, None, Vec::new())),
        Item::Enum(enum_def) => {
            let fields = enum_def
                .variants
                .iter()
                .flat_map(|variant| variant.fields.iter().map(move |ty| (ty, variant.span)))
                .collect::<Vec<_>>();
            Some((&enum_def.name, &[], None, fields))
        }
        _ => None,
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
        let tokens = lexer::lex(source).unwrap();
        parser::parse(&tokens).unwrap()
    }

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
    fn rejects_cross_module_type_dependency_cycles() {
        let left = source_module(
            r#"
module left

use right::B

struct A {
    b: B
}
"#,
        );
        let right = source_module(
            r#"
module right

use left::A

struct B {
    a: A
}
"#,
        );
        let mut resolver = ModuleResolver::new();
        resolver.register_module(left).unwrap();
        resolver.register_module(right).unwrap();

        let err = resolver.check_circular_deps().unwrap_err();

        assert!(err.message.contains("cyclic type dependency detected"), "unexpected error: {}", err.message);
        assert!(err.message.contains("left::A") && err.message.contains("right::B"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_imported_type_resolution_uses_exact_module_path() {
        let mut resolver = ModuleResolver::new();

        resolver
            .register_module(Module {
                name: "foo".to_string(),
                items: vec![Item::Struct(StructDef {
                    name: "Type".to_string(),
                    type_id: None,
                    default_hash_type: None,
                    capacity_floor: None,
                    fields: vec![Field { name: "small".to_string(), ty: Type::U8, span: Span::default() }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();
        resolver
            .register_module(Module {
                name: "foobar".to_string(),
                items: vec![Item::Struct(StructDef {
                    name: "Type".to_string(),
                    type_id: None,
                    default_hash_type: None,
                    capacity_floor: None,
                    fields: vec![Field { name: "wide".to_string(), ty: Type::U64, span: Span::default() }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();
        resolver
            .register_module(Module {
                name: "app".to_string(),
                items: vec![Item::Use(UseStmt {
                    module_path: vec!["foobar".to_string()],
                    imports: vec![UseImport { name: "Type".to_string(), alias: None }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();

        let Some(TypeDef::Struct(resolved)) = resolver.resolve_type("app", "Type") else {
            panic!("expected imported struct to resolve");
        };
        assert_eq!(resolved.fields[0].name, "wide");
    }

    #[test]
    fn test_global_type_resolution_rejects_ambiguous_symbol() {
        let mut resolver = ModuleResolver::new();

        for module_name in ["left", "right"] {
            resolver
                .register_module(Module {
                    name: module_name.to_string(),
                    items: vec![Item::Struct(StructDef {
                        name: "Token".to_string(),
                        type_id: None,
                        default_hash_type: None,
                        capacity_floor: None,
                        fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                        span: Span::default(),
                    })],
                    span: Span::default(),
                })
                .unwrap();
        }

        assert!(resolver.resolve_type_global("Token").is_none());
        assert!(resolver.resolve_type("left", "Token").is_some());
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
                        body: vec![Stmt::Return(Some(Expr::Integer(0, Span::default())))],
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
    fn test_register_module_rejects_missing_imported_symbol_when_target_is_loaded() {
        let mut resolver = ModuleResolver::new();
        resolver
            .register_module(Module {
                name: "cellscript::token".to_string(),
                items: vec![Item::Struct(StructDef {
                    name: "Token".to_string(),
                    type_id: None,
                    default_hash_type: None,
                    capacity_floor: None,
                    fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();
        let err = resolver
            .register_module(Module {
                name: "app".to_string(),
                items: vec![Item::Use(UseStmt {
                    module_path: vec!["cellscript".to_string(), "token".to_string()],
                    imports: vec![UseImport { name: "Missing".to_string(), alias: None }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap_err();

        assert!(err.message.contains("symbol 'Missing' not found in module 'cellscript::token'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_register_module_rejects_deferred_missing_import_when_target_arrives() {
        let mut resolver = ModuleResolver::new();
        resolver
            .register_module(Module {
                name: "app".to_string(),
                items: vec![Item::Use(UseStmt {
                    module_path: vec!["cellscript".to_string(), "token".to_string()],
                    imports: vec![UseImport { name: "Missing".to_string(), alias: None }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap();

        let err = resolver
            .register_module(Module {
                name: "cellscript::token".to_string(),
                items: vec![Item::Struct(StructDef {
                    name: "Token".to_string(),
                    type_id: None,
                    default_hash_type: None,
                    capacity_floor: None,
                    fields: vec![Field { name: "amount".to_string(), ty: Type::U64, span: Span::default() }],
                    span: Span::default(),
                })],
                span: Span::default(),
            })
            .unwrap_err();

        assert!(err.message.contains("symbol 'Missing' not found in module 'cellscript::token'"), "unexpected error: {}", err.message);
    }

    #[test]
    fn test_path_resolver() {
        let path = PathResolver::parse_path("cellscript::fungible_token::Token");
        assert_eq!(path, vec!["cellscript", "fungible_token", "Token"]);

        let qualified = PathResolver::build_qualified_name("cellscript", "Token");
        assert_eq!(qualified, "cellscript::Token");
    }
}
