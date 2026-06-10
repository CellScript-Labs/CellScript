use crate::ast::*;
use crate::error::{CompileError, Result, Span};
use crate::lexer::token::TokenKind;
use crate::resolve::ModuleResolver;
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub mod server;

const LSP_MAX_DOCUMENT_BYTES: usize = 10 * 1024 * 1024;
const LSP_MAX_OPEN_DOCUMENTS: usize = 1_000;
const LSP_MAX_WORKSPACE_MODULES: usize = 256;
const LSP_MAX_WORKSPACE_BYTES: usize = 20 * 1024 * 1024;
const LSP_MAX_REFERENCE_LOCATIONS: usize = 2_048;

pub struct LspServer {
    documents: HashMap<String, String>,
    ast_cache: HashMap<String, Module>,
    diagnostics: HashMap<String, Vec<Diagnostic>>,
    primitive_compat: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CompletionItemKind {
    Text = 1,
    Method = 2,
    Function = 3,
    Constructor = 4,
    Field = 5,
    Variable = 6,
    Class = 7,
    Interface = 8,
    Module = 9,
    Property = 10,
    Unit = 11,
    Value = 12,
    Enum = 13,
    Keyword = 14,
    Snippet = 15,
    Color = 16,
    File = 17,
    Reference = 18,
    Folder = 19,
    EnumMember = 20,
    Constant = 21,
    Struct = 22,
    Event = 23,
    Operator = 24,
    TypeParameter = 25,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInformation {
    pub name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub container_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub contents: String,
    pub range: Option<Range>,
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    pub fn new() -> Self {
        Self { documents: HashMap::new(), ast_cache: HashMap::new(), diagnostics: HashMap::new(), primitive_compat: None }
    }

    pub fn set_primitive_compat(&mut self, primitive_compat: Option<String>) {
        self.primitive_compat = primitive_compat;
    }

    pub fn open_document(&mut self, uri: String, content: String) {
        if !self.accept_document_update(&uri, content.len(), !self.documents.contains_key(&uri)) {
            return;
        }
        self.documents.insert(uri.clone(), content.clone());
        self.parse_document(&uri, &content);
    }

    pub fn update_document(&mut self, uri: String, content: String) {
        let new_document = !self.documents.contains_key(&uri);
        if !self.accept_document_update(&uri, content.len(), new_document) {
            return;
        }
        self.documents.insert(uri.clone(), content.clone());
        self.parse_document(&uri, &content);
    }

    /// Apply incremental text changes to a document and re-parse.
    ///
    /// If any change has `range == None`, the entire document is replaced with that change's text.
    /// Otherwise, each change's text is spliced into the current document at the given range.
    pub fn update_document_incremental(&mut self, uri: &str, changes: Vec<TextDocumentContentChangeEvent>) {
        let Some(mut content) = self.documents.get(uri).cloned() else {
            return;
        };

        for change in changes {
            if change.text.len() > LSP_MAX_DOCUMENT_BYTES {
                self.reject_document_update(
                    uri,
                    format!(
                        "document change is too large: {} bytes exceeds the {} byte limit",
                        change.text.len(),
                        LSP_MAX_DOCUMENT_BYTES
                    ),
                );
                return;
            }
            match change.range {
                None => {
                    // Full document replacement.
                    content = change.text;
                }
                Some(range) => {
                    content = apply_incremental_change(&content, range, &change.text);
                }
            }
            if content.len() > LSP_MAX_DOCUMENT_BYTES {
                self.reject_document_update(
                    uri,
                    format!("document is too large: {} bytes exceeds the {} byte limit", content.len(), LSP_MAX_DOCUMENT_BYTES),
                );
                return;
            }
        }

        self.documents.insert(uri.to_string(), content.clone());
        self.parse_document(uri, &content);
    }

    pub fn close_document(&mut self, uri: &str) {
        self.documents.remove(uri);
        self.ast_cache.remove(uri);
        self.diagnostics.remove(uri);
    }

    fn accept_document_update(&mut self, uri: &str, byte_len: usize, new_document: bool) -> bool {
        if new_document && self.documents.len() >= LSP_MAX_OPEN_DOCUMENTS {
            self.reject_document_update(uri, format!("too many open documents: limit is {}", LSP_MAX_OPEN_DOCUMENTS));
            return false;
        }
        if byte_len > LSP_MAX_DOCUMENT_BYTES {
            self.reject_document_update(
                uri,
                format!("document is too large: {} bytes exceeds the {} byte limit", byte_len, LSP_MAX_DOCUMENT_BYTES),
            );
            return false;
        }
        true
    }

    fn reject_document_update(&mut self, uri: &str, message: String) {
        self.documents.remove(uri);
        self.ast_cache.remove(uri);
        self.diagnostics.insert(
            uri.to_string(),
            vec![Diagnostic {
                range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 0, character: 0 } },
                severity: DiagnosticSeverity::Error,
                message,
                source: "cellscript".to_string(),
            }],
        );
    }

    fn parse_document(&mut self, uri: &str, content: &str) {
        self.ast_cache.remove(uri);

        let tokens = match crate::lexer::lex(content) {
            Ok(tokens) => tokens,
            Err(error) => {
                self.diagnostics.insert(uri.to_string(), vec![diagnostic_from_error(content, &error)]);
                return;
            }
        };

        let ast = match crate::parser::parse(&tokens) {
            Ok(ast) => ast,
            Err(error) => {
                self.diagnostics.insert(uri.to_string(), vec![diagnostic_from_error(content, &error)]);
                return;
            }
        };

        let primitive_strict = self.primitive_strict_for_uri(uri);
        if primitive_strict {
            if let Err(error) = crate::check_primitive_strict_015(&ast) {
                self.diagnostics.insert(uri.to_string(), vec![diagnostic_from_error(content, &error)]);
                return;
            }
        }

        self.ast_cache.insert(uri.to_string(), ast.clone());
        let resolver = match self.document_resolver(uri) {
            Ok(resolver) => resolver,
            Err(error) => {
                self.diagnostics.insert(uri.to_string(), vec![diagnostic_from_error(content, &error)]);
                return;
            }
        };

        let type_check = match resolver.as_ref() {
            Some(resolver) => crate::types::check_with_resolver_and_primitive_strict(&ast, resolver, &ast.name, primitive_strict),
            None => crate::types::check_with_primitive_strict(&ast, primitive_strict),
        };
        let diagnostics = match type_check.and_then(|_| crate::flow::check(&ast)) {
            Ok(()) => {
                let mut diagnostics = Vec::new();
                let metadata = match resolver.as_ref() {
                    Some(resolver) => crate::compile_metadata_with_resolver(content, None, resolver, &ast.name),
                    None => crate::compile_metadata(content, None),
                };
                if let Ok(metadata) = metadata {
                    diagnostics.extend(lowering_diagnostics(content, &ast, &metadata));
                }
                diagnostics
            }
            Err(error) => vec![diagnostic_from_error(content, &error)],
        };
        self.diagnostics.insert(uri.to_string(), diagnostics);
    }

    fn document_resolver(&self, uri: &str) -> Result<Option<ModuleResolver>> {
        let modules = self.workspace_modules_result(uri)?;
        if modules.is_empty() {
            return Ok(None);
        }

        let mut resolver = ModuleResolver::new();
        for module in modules {
            resolver.register_module(module.ast)?;
        }
        resolver.check_circular_deps()?;
        Ok(Some(resolver))
    }

    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics.get(uri).cloned().unwrap_or_default()
    }

    pub fn completion(&self, uri: &str, position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        let ctx = self.completion_context(uri, position);

        match ctx {
            CompletionContext::Type => {
                items.extend(self.type_completions());
                // Type-position also allows user-defined types.
                if let Some(ast) = self.ast_cache.get(uri) {
                    items.extend(self.type_symbol_completions(ast));
                }
            }
            CompletionContext::Member { type_name } => {
                items.extend(self.member_completions(uri, &type_name));
            }
            CompletionContext::Namespace { type_name } => {
                if let Some(ast) = self.ast_cache.get(uri) {
                    items.extend(self.namespace_symbol_completions(ast, &type_name));
                }
            }
            CompletionContext::Declaration => {
                items.extend(self.declaration_keyword_completions());
            }
            CompletionContext::Expression => {
                items.extend(self.keyword_completions());
                items.extend(self.type_completions());
                if let (Some(ast), Some(content)) = (self.ast_cache.get(uri), self.documents.get(uri)) {
                    items.extend(self.symbol_completions(ast));
                    items.extend(self.local_completions(content, ast, position));
                }
            }
        }

        items
    }

    /// Determine the completion context at the given position.
    fn completion_context(&self, uri: &str, position: Position) -> CompletionContext {
        let Some(content) = self.documents.get(uri) else {
            return CompletionContext::Expression;
        };

        let line_start = self.line_start_offset(content, position.line);
        let offset = position_to_offset(content, position).unwrap_or(line_start);
        let prefix = &content[line_start..offset];

        // Check for namespace access: `Type::Variant`.
        if let Some(scope_pos) = prefix.rfind("::") {
            let suffix = &prefix[scope_pos + 2..];
            if suffix.chars().all(is_ident_char) {
                let before_scope = &prefix[..scope_pos];
                let type_name = word_before_offset(before_scope, before_scope.len()).unwrap_or_default();
                return CompletionContext::Namespace { type_name };
            }
        }

        // Check for member access: `expr.field`
        if let Some(dot_pos) = prefix.rfind('.') {
            // We want the identifier before the dot.
            let before_dot = &prefix[..dot_pos];
            let type_name = word_before_offset(before_dot, before_dot.len()).unwrap_or_default();
            return CompletionContext::Member { type_name };
        }

        // Check for type context: after `:`, `->`, or `<`
        let trimmed = prefix.trim_end();
        if trimmed.ends_with(':') || trimmed.ends_with("->") || trimmed.ends_with('<') {
            return CompletionContext::Type;
        }

        // Check for top-level / declaration context
        let line_text = prefix.trim();
        if line_text.is_empty() || line_text == "module" {
            return CompletionContext::Declaration;
        }

        CompletionContext::Expression
    }

    /// Get the byte offset where a given line starts.
    fn line_start_offset(&self, content: &str, line: u32) -> usize {
        let mut current_line = 0u32;
        for (idx, ch) in content.char_indices() {
            if current_line == line {
                return idx;
            }
            if ch == '\n' {
                current_line += 1;
            }
        }
        content.len()
    }

    /// Declaration-position keywords only.
    fn declaration_keyword_completions(&self) -> Vec<CompletionItem> {
        vec![
            ("resource", "resource ${1:Name} {\n    $0\n}"),
            ("shared", "shared ${1:Name} {\n    $0\n}"),
            ("receipt", "receipt ${1:Name} {\n    $0\n}"),
            ("struct", "struct ${1:Name} {\n    $0\n}"),
            ("flow", "flow ${1:Name} for ${2:Type}.${3:state} {\n    ${4:Created} -> ${5:Live};\n}"),
            (
                "invariant",
                "invariant ${1:name} {\n    trigger: ${2:type_group}\n    scope: ${3:group}\n    reads: ${4:group_inputs<Token>.amount}, ${5:group_outputs<Token>.amount}\n    assert_conserved(${6:Token.amount}, scope = ${7:group})\n}",
            ),
            ("action", "action ${1:name}(${2:input}: ${3:CellType}) -> ${4:output}: ${3:CellType}\nwhere\n    $0"),
            (
                "lock",
                "lock ${1:name}(protected ${2:cell}: ${3:CellType}, witness ${4:arg}: ${5:Address}, lock_args ${6:owner}: ${7:Address}) -> bool {\n    require ${6} == ${2}.owner\n    require ${4} == ${6}\n    $0\n}",
            ),
            ("const", "const ${1:NAME}: ${2:u64} = $0;"),
            ("enum", "enum ${1:Name} {\n    $0\n}"),
            ("use", "use ${1:path};"),
        ]
        .into_iter()
        .map(|(label, insert)| CompletionItem {
            label: label.to_string(),
            kind: CompletionItemKind::Keyword,
            detail: Some(format!("{} keyword", label)),
            documentation: None,
            insert_text: Some(insert.to_string()),
        })
        .collect()
    }

    /// Completions for user-defined types (at type positions).
    fn type_symbol_completions(&self, module: &Module) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for item in &module.items {
            let (name, kind_label) = match item {
                Item::Resource(r) => (&r.name, "resource"),
                Item::Shared(s) => (&s.name, "shared"),
                Item::Receipt(r) => (&r.name, "receipt"),
                Item::Struct(s) => (&s.name, "struct"),
                Item::Enum(e) => (&e.name, "enum"),
                _ => continue,
            };
            items.push(CompletionItem {
                label: name.clone(),
                kind: CompletionItemKind::Class,
                detail: Some(format!("{} {}", kind_label, name)),
                documentation: None,
                insert_text: Some(name.clone()),
            });
        }
        items
    }

    fn namespace_symbol_completions(&self, module: &Module, type_name: &str) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for item in &module.items {
            match item {
                Item::Enum(enum_def) if enum_def.name == type_name => {
                    items.extend(enum_def.variants.iter().filter(|variant| variant.fields.is_empty()).map(|variant| CompletionItem {
                        label: variant.name.clone(),
                        kind: CompletionItemKind::EnumMember,
                        detail: Some(format!("enum variant {}::{}", enum_def.name, variant.name)),
                        documentation: None,
                        insert_text: Some(variant.name.clone()),
                    }));
                }
                _ => {}
            }
        }
        items.extend(Self::flow_state_completions(module, type_name));
        items
    }

    fn flow_state_completions(module: &Module, type_name: &str) -> Vec<CompletionItem> {
        module
            .items
            .iter()
            .filter_map(|item| {
                let Item::Flow(machine) = item else {
                    return None;
                };
                (machine.target.base == type_name).then_some(machine)
            })
            .flat_map(|machine| {
                let states = Self::flow_enum_states(module, type_name, &machine.target.field)
                    .unwrap_or_else(|| Self::transition_states(machine));
                let field_name = machine.target.field.clone();
                states.into_iter().enumerate().map(move |(index, state)| CompletionItem {
                    label: state.clone(),
                    kind: CompletionItemKind::EnumMember,
                    detail: Some(format!("flow state {}::{}", type_name, state)),
                    documentation: Some(format!("State index {} for flow field `{}.{}`.", index, type_name, field_name)),
                    insert_text: Some(state),
                })
            })
            .collect()
    }

    fn flow_enum_states(module: &Module, type_name: &str, field_name: &str) -> Option<Vec<String>> {
        let enum_name = module.items.iter().find_map(|item| {
            let fields = match item {
                Item::Resource(def) if def.name == type_name => Some(&def.fields),
                Item::Shared(def) if def.name == type_name => Some(&def.fields),
                Item::Receipt(def) if def.name == type_name => Some(&def.fields),
                Item::Struct(def) if def.name == type_name => Some(&def.fields),
                _ => None,
            }?;
            fields.iter().find_map(|field| {
                if field.name == field_name {
                    if let Type::Named(name) = &field.ty {
                        return Some(name.clone());
                    }
                }
                None
            })
        })?;

        module.items.iter().find_map(|item| {
            let Item::Enum(enum_def) = item else {
                return None;
            };
            (enum_def.name == enum_name && enum_def.variants.iter().all(|variant| variant.fields.is_empty()))
                .then(|| enum_def.variants.iter().map(|variant| variant.name.clone()).collect())
        })
    }

    fn transition_states(machine: &FlowDef) -> Vec<String> {
        let mut states = Vec::new();
        for transition in &machine.transitions {
            for raw in [&transition.from, &transition.to] {
                let state = raw.rsplit_once("::").map_or(raw.as_str(), |(_, state)| state);
                if !states.iter().any(|existing| existing == state) {
                    states.push(state.to_string());
                }
            }
        }
        states
    }

    /// Member completions for a given type name (after `.`).
    fn member_completions(&self, uri: &str, type_name: &str) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // Built-in namespace methods.
        match type_name {
            "Vec" => {
                for (name, insert) in [
                    ("new", "Vec::new()"),
                    ("with_capacity", "Vec::with_capacity($0)"),
                    ("capacity", "capacity()"),
                    ("push", "push($0)"),
                    ("extend_from_slice", "extend_from_slice($0)"),
                    ("len", "len()"),
                    ("is_empty", "is_empty()"),
                    ("first", "first()"),
                    ("last", "last()"),
                    ("contains", "contains($0)"),
                    ("set", "set($0)"),
                    ("remove", "remove($0)"),
                    ("pop", "pop()"),
                    ("insert", "insert($0)"),
                    ("reverse", "reverse()"),
                    ("truncate", "truncate($0)"),
                    ("swap", "swap($0)"),
                    ("clear", "clear()"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Method,
                        detail: Some(format!("Vec::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "env" => {
                for (name, insert) in [
                    ("current_timepoint", "env::current_timepoint()"),
                    ("sighash_all", "env::sighash_all(${1:source::group_input(0)})"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Method,
                        detail: Some(format!("env::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "source" => {
                for name in ["input", "output", "cell_dep", "header_dep", "group_input", "group_output"] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("source::{}", name)),
                        documentation: None,
                        insert_text: Some(format!("source::{}(${{1:0}})", name)),
                    });
                }
                return items;
            }
            "witness" => {
                for name in ["raw", "lock", "input_type", "output_type"] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("witness::{}", name)),
                        documentation: None,
                        insert_text: Some(format!("witness::{}(${{1:source::group_input(0)}})", name)),
                    });
                }
                return items;
            }
            "ckb" => {
                for (name, insert) in [
                    ("header_epoch_number", "ckb::header_epoch_number()"),
                    ("header_epoch_start_block_number", "ckb::header_epoch_start_block_number()"),
                    ("header_epoch_length", "ckb::header_epoch_length()"),
                    ("input_since", "ckb::input_since()"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("ckb::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "Address" | "Hash" => {
                // Namespace-style methods.
                return items;
            }
            _ => {}
        }

        // User-defined type fields.
        let mut search_modules: Vec<&Module> = Vec::new();
        if let Some(ast) = self.ast_cache.get(uri) {
            search_modules.push(ast);
        }
        for module in &search_modules {
            for item in &module.items {
                let fields: &[Field] = match item {
                    Item::Resource(r) if r.name == type_name => &r.fields,
                    Item::Shared(s) if s.name == type_name => &s.fields,
                    Item::Receipt(r) if r.name == type_name => &r.fields,
                    Item::Struct(s) if s.name == type_name => &s.fields,
                    _ => continue,
                };
                for field in fields {
                    items.push(CompletionItem {
                        label: field.name.clone(),
                        kind: CompletionItemKind::Field,
                        detail: Some(format!("{}: {}", field.name, type_to_string(&field.ty))),
                        documentation: None,
                        insert_text: Some(field.name.clone()),
                    });
                }
                break;
            }
        }

        items
    }

    /// Completions for local variables visible at `position`.
    fn local_completions(&self, source: &str, module: &Module, position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        for item in &module.items {
            let (params, body) = match item {
                Item::Action(a) => (&a.params, &a.body),
                Item::Function(f) => (&f.params, &f.body),
                Item::Lock(l) => (&l.params, &l.body),
                _ => continue,
            };

            // Check if position is inside this function's span.
            let func_range = span_to_range(source, item_span(item));
            if !position_in_range(position, func_range) {
                continue;
            }

            // Add parameters.
            for param in params {
                items.push(CompletionItem {
                    label: param.name.clone(),
                    kind: CompletionItemKind::Variable,
                    detail: Some(format!("param: {}", type_to_string(&param.ty))),
                    documentation: None,
                    insert_text: Some(param.name.clone()),
                });
            }

            // Add local `let` bindings that are in scope (before position).
            for stmt in body {
                let stmt_range = span_to_range(source, stmt_span(stmt));
                if position_in_range(position, stmt_range) || position_le(stmt_range.start, position) {
                    // We are past the position, stop.
                    if position_le(position, stmt_range.start) && !position_in_range(position, stmt_range) {
                        break;
                    }
                }
                if let Stmt::Let(let_stmt) = stmt {
                    if let BindingPattern::Name(name) = &let_stmt.pattern {
                        items.push(CompletionItem {
                            label: name.clone(),
                            kind: CompletionItemKind::Variable,
                            detail: Some(format!(
                                "let{}: {}",
                                if let_stmt.is_mut { " mut" } else { "" },
                                let_stmt.ty.as_ref().map(type_to_string).unwrap_or_else(|| "_".to_string())
                            )),
                            documentation: None,
                            insert_text: Some(name.clone()),
                        });
                    }
                }
            }
        }

        items
    }

    fn keyword_completions(&self) -> Vec<CompletionItem> {
        let keywords = vec![
            ("module", "module ${1:name}"),
            ("use", "use ${1:path}"),
            ("resource", "resource ${1:Name} {\n    $0\n}"),
            ("shared", "shared ${1:Name} {\n    $0\n}"),
            ("receipt", "receipt ${1:Name} {\n    $0\n}"),
            ("struct", "struct ${1:Name} {\n    $0\n}"),
            ("action", "action ${1:name}(${2:input}: ${3:CellType}) -> ${4:output}: ${3:CellType}\nwhere\n    $0"),
            ("flow", "flow ${1:Name} for ${2:Type}.${3:state} {\n    ${4:Created} -> ${5:Live};\n}"),
            ("input", "input ${1:name}: ${2:CellType}"),
            ("transition", "transition ${1:input}.${2:state}: ${3:Created} -> ${4:output}.${2:state}: ${5:Live}"),
            (
                "lock",
                "lock ${1:name}(protected ${2:cell}: ${3:CellType}, witness ${4:arg}: ${5:Address}, lock_args ${6:owner}: ${7:Address}) -> bool {\n    require ${6} == ${2}.owner\n    require ${4} == ${6}\n    $0\n}",
            ),
            ("let", "let ${1:name} = $0"),
            ("if", "if ${1:condition} {\n    $0\n}"),
            ("for", "for ${1:item} in ${2:iterable} {\n    $0\n}"),
            ("while", "while ${1:condition} {\n    $0\n}"),
            ("return", "return $0"),
            ("create", "create ${1:output} = ${2:Type} { $0 }"),
            ("create_unique", "create_unique<${1:Type}>(identity = ${2:ckb_type_id}) { $0 }"),
            ("replace_unique", "replace_unique<${1:Type}>(identity = ${2:ckb_type_id}) ${3:input} { $0 }"),
            ("destroy", "destroy ${1:expr}"),
            ("destroy_singleton_type", "destroy_singleton_type(${1:expr})"),
            ("destroy_unique", "destroy_unique(${1:expr}, identity = ${2:type_id})"),
            ("destroy_instance", "destroy_instance(${1:expr}, identity_field = ${2:field})"),
            ("burn_amount", "burn_amount(${1:expr}, field = ${2:field})"),
            ("assert", "assert ${1:condition}"),
            ("assert_invariant", "assert_invariant(${1:condition}, \"${2:message}\")"),
            ("require", "require ${1:condition}"),
            ("require_block", "require {\n    ${1:condition}\n}"),
            ("preserve", "preserve ${1:output} from ${2:input} {\n    ${3:field}\n}"),
            ("assert_sum", "assert_sum(${1:expr}, scope = ${2:group})"),
            ("assert_conserved", "assert_conserved(${1:expr}, scope = ${2:group})"),
            ("assert_delta", "assert_delta(${1:expr}, ${2:witness}, scope = ${3:group})"),
            ("assert_distinct", "assert_distinct(${1:expr}, scope = ${2:group})"),
            ("assert_singleton", "assert_singleton(${1:expr}, scope = ${2:group})"),
            ("std::cell::same_lock", "std::cell::same_lock(${1:output}, ${2:input})"),
            ("std::cell::preserve_lock", "std::cell::preserve_lock(${1:output}, ${2:input})"),
            ("std::cell::preserve_type", "std::cell::preserve_type(${1:output}, ${2:input})"),
            ("std::cell::preserve_capacity", "std::cell::preserve_capacity(${1:output}, ${2:input})"),
            ("std::lifecycle::transfer", "std::lifecycle::transfer(${1:input}, ${2:output}, ${3:to}) {\n    ${4:field}\n}"),
            ("std::receipt::claim", "std::receipt::claim(${1:receipt}, ${2:output}, ${3:lock}) {\n    ${4:field}\n}"),
            ("std::lifecycle::settle", "std::lifecycle::settle(${1:input}, ${2:output}, ${3:lock}) {\n    ${4:field}\n}"),
            ("protected", "protected ${1:cell}: ${2:CellType}"),
            ("witness", "witness ${1:arg}: ${2:Address}"),
            ("lock_args", "lock_args ${1:args}: ${2:OwnerArgs}"),
        ];

        keywords
            .into_iter()
            .map(|(label, insert)| CompletionItem {
                label: label.to_string(),
                kind: CompletionItemKind::Keyword,
                detail: Some(format!("{} keyword", label)),
                documentation: None,
                insert_text: Some(insert.to_string()),
            })
            .collect()
    }

    fn type_completions(&self) -> Vec<CompletionItem> {
        let types = vec![
            "u8", "u16", "u32", "u64", "u128", "i8", "i16", "i32", "i64", "i128", "bool", "String", "Address", "Hash", "Bytes", "Vec",
        ];

        types
            .into_iter()
            .map(|ty| CompletionItem {
                label: ty.to_string(),
                kind: CompletionItemKind::TypeParameter,
                detail: Some(format!("{} type", ty)),
                documentation: None,
                insert_text: None,
            })
            .collect()
    }

    fn symbol_completions(&self, module: &Module) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        for item in &module.items {
            match item {
                Item::Resource(r) => {
                    items.push(CompletionItem {
                        label: r.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("resource {}", r.name)),
                        documentation: None,
                        insert_text: Some(r.name.clone()),
                    });
                }
                Item::Shared(s) => {
                    items.push(CompletionItem {
                        label: s.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("shared {}", s.name)),
                        documentation: None,
                        insert_text: Some(s.name.clone()),
                    });
                }
                Item::Receipt(r) => {
                    items.push(CompletionItem {
                        label: r.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("receipt {}", r.name)),
                        documentation: None,
                        insert_text: Some(r.name.clone()),
                    });
                }
                Item::Struct(s) => {
                    items.push(CompletionItem {
                        label: s.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("struct {}", s.name)),
                        documentation: None,
                        insert_text: Some(s.name.clone()),
                    });
                }
                Item::Invariant(i) => {
                    items.push(CompletionItem {
                        label: i.name.clone(),
                        kind: CompletionItemKind::Keyword,
                        detail: Some(format!("invariant {}", i.name)),
                        documentation: None,
                        insert_text: Some(i.name.clone()),
                    });
                }
                Item::Action(a) => {
                    items.push(CompletionItem {
                        label: a.name.clone(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("action {}", a.name)),
                        documentation: a.doc_comment.clone(),
                        insert_text: Some(format!("{}($0)", a.name)),
                    });
                }
                Item::Lock(l) => {
                    items.push(CompletionItem {
                        label: l.name.clone(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("lock {}", l.name)),
                        documentation: None,
                        insert_text: Some(format!("{}($0)", l.name)),
                    });
                }
                _ => {}
            }
        }

        items
    }

    pub fn goto_definition(&self, uri: &str, position: Position) -> Option<Location> {
        let symbol = self.symbol_at_position(uri, position)?;

        // 1. Prefer the most local semantic scopes. A local variable named
        // `Token` or a field named `amount` must not jump to a top-level item
        // with the same text.
        if let Some(loc) = self.find_field_definition(uri, position, &symbol) {
            return Some(loc);
        }

        if let Some(loc) = self.find_local_definition(uri, position, &symbol) {
            return Some(loc);
        }

        // 2. Then try top-level symbols in the current file and workspace.
        self.find_top_level_symbol(uri, &symbol)
    }

    /// Find a field definition for `symbol` when accessed via `expr.field`.
    fn find_field_definition(&self, uri: &str, position: Position, symbol: &str) -> Option<Location> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;

        // Look for a `.` before the symbol.
        let line_start = self.line_start_offset(content, position.line);
        let prefix = &content[line_start..offset];
        let dot_pos = prefix.rfind('.')?;
        let type_name = word_before_offset(prefix, dot_pos)?;

        let ast = self.ast_cache.get(uri)?;
        for item in &ast.items {
            let (name, fields, span) = match item {
                Item::Resource(r) if r.name == type_name => (&r.name, &r.fields, r.span),
                Item::Shared(s) if s.name == type_name => (&s.name, &s.fields, s.span),
                Item::Receipt(r) if r.name == type_name => (&r.name, &r.fields, r.span),
                Item::Struct(s) if s.name == type_name => (&s.name, &s.fields, s.span),
                _ => continue,
            };
            let _ = name; // used in pattern guard
            for field in fields {
                if field.name == symbol {
                    return Some(Location { uri: uri.to_string(), range: span_to_range(content, field.span) });
                }
            }
            let _ = span;
        }
        None
    }

    /// Find a local variable or parameter definition for `symbol`.
    fn find_local_definition(&self, uri: &str, position: Position, symbol: &str) -> Option<Location> {
        let content = self.documents.get(uri)?;
        let ast = self.ast_cache.get(uri)?;

        for item in &ast.items {
            let (params, body, item_span_val) = match item {
                Item::Action(a) => (&a.params, &a.body, a.span),
                Item::Function(f) => (&f.params, &f.body, f.span),
                Item::Lock(l) => (&l.params, &l.body, l.span),
                _ => continue,
            };

            let func_range = span_to_range(content, item_span_val);
            if !position_in_range(position, func_range) {
                continue;
            }

            // Check parameters.
            for param in params {
                if param.name == symbol {
                    return Some(Location { uri: uri.to_string(), range: span_to_range(content, param.span) });
                }
            }

            // Check local let bindings.
            for stmt in body {
                if let Stmt::Let(let_stmt) = stmt {
                    if let BindingPattern::Name(name) = &let_stmt.pattern {
                        if name == symbol {
                            return Some(Location { uri: uri.to_string(), range: span_to_range(content, let_stmt.span) });
                        }
                    }
                }
            }
        }
        None
    }

    fn find_enclosing_callable_range(&self, uri: &str, position: Position) -> Option<Range> {
        let content = self.documents.get(uri)?;
        let ast = self.ast_cache.get(uri)?;
        for item in &ast.items {
            let span = match item {
                Item::Action(action) => action.span,
                Item::Function(function) => function.span,
                Item::Lock(lock) => lock.span,
                _ => continue,
            };
            let range = span_to_range(content, span);
            if position_in_range(position, range) {
                return Some(range);
            }
        }
        None
    }

    pub fn find_references(&self, uri: &str, position: Position) -> Vec<Location> {
        let Some(symbol) = self.symbol_at_position(uri, position) else {
            return Vec::new();
        };
        let mut refs = Vec::new();

        if self.find_local_definition(uri, position, &symbol).is_some() {
            if let (Some(content), Some(scope)) = (self.documents.get(uri), self.find_enclosing_callable_range(uri, position)) {
                for (start, end) in word_occurrences_in_range(content, &symbol, scope) {
                    refs.push(Location {
                        uri: uri.to_string(),
                        range: Range { start: offset_to_position(content, start), end: offset_to_position(content, end) },
                    });
                    if refs.len() >= LSP_MAX_REFERENCE_LOCATIONS {
                        break;
                    }
                }
            }
            return refs;
        }

        let workspace_modules = self.workspace_modules(uri);
        if !workspace_modules.is_empty() {
            for module in workspace_modules {
                let module_uri = utf8_path_to_file_uri(&module.path);
                for (start, end) in word_occurrences(&module.source, &symbol) {
                    refs.push(Location {
                        uri: module_uri.clone(),
                        range: Range {
                            start: offset_to_position(&module.source, start),
                            end: offset_to_position(&module.source, end),
                        },
                    });
                    if refs.len() >= LSP_MAX_REFERENCE_LOCATIONS {
                        return refs;
                    }
                }
            }
            return refs;
        }

        if let Some(content) = self.documents.get(uri) {
            for (start, end) in word_occurrences(content, &symbol) {
                refs.push(Location {
                    uri: uri.to_string(),
                    range: Range { start: offset_to_position(content, start), end: offset_to_position(content, end) },
                });
                if refs.len() >= LSP_MAX_REFERENCE_LOCATIONS {
                    break;
                }
            }
        }
        refs
    }

    pub fn hover(&self, uri: &str, position: Position) -> Option<Hover> {
        let symbol = self.symbol_at_position(uri, position)?;

        // 1. Try top-level item hover (existing logic).
        if let (Some(ast), Some(source)) = (self.ast_cache.get(uri), self.documents.get(uri)) {
            let metadata = crate::compile_metadata(source, None).ok();
            if let Some(hover) = ast.items.iter().find_map(|item| {
                if item_name(item) == Some(symbol.as_str()) {
                    self.item_hover(source, item, metadata.as_ref())
                } else {
                    None
                }
            }) {
                return Some(hover);
            }
        }

        // 2. Try field hover.
        if let Some(hover) = self.field_hover(uri, position, &symbol) {
            return Some(hover);
        }

        // 3. Try local variable / parameter hover.
        if let Some(hover) = self.local_hover(uri, position, &symbol) {
            return Some(hover);
        }

        // 4. Try workspace modules.
        for module in self.workspace_modules(uri) {
            let metadata = crate::compile_metadata(&module.source, None).ok();
            if let Some(hover) = module.ast.items.iter().find_map(|item| {
                if item_name(item) == Some(symbol.as_str()) {
                    self.item_hover(&module.source, item, metadata.as_ref())
                } else {
                    None
                }
            }) {
                return Some(hover);
            }
        }

        None
    }

    /// Hover information for a field access (e.g. `token.amount`).
    fn field_hover(&self, uri: &str, position: Position, symbol: &str) -> Option<Hover> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;
        let line_start = self.line_start_offset(content, position.line);
        let prefix = &content[line_start..offset];
        let dot_pos = prefix.rfind('.')?;
        let type_name = word_before_offset(prefix, dot_pos)?;

        let ast = self.ast_cache.get(uri)?;
        for item in &ast.items {
            let fields: &[Field] = match item {
                Item::Resource(r) if r.name == type_name => &r.fields,
                Item::Shared(s) if s.name == type_name => &s.fields,
                Item::Receipt(r) if r.name == type_name => &r.fields,
                Item::Struct(s) if s.name == type_name => &s.fields,
                _ => continue,
            };
            for field in fields {
                if field.name == symbol {
                    return Some(Hover {
                        contents: format!(
                            "```cellscript\n{}: {}\n```\n\nField of `{}`",
                            field.name,
                            type_to_string(&field.ty),
                            type_name
                        ),
                        range: Some(span_to_range(content, field.span)),
                    });
                }
            }
        }
        None
    }

    /// Hover information for a local variable or parameter.
    fn local_hover(&self, uri: &str, position: Position, symbol: &str) -> Option<Hover> {
        let content = self.documents.get(uri)?;
        let ast = self.ast_cache.get(uri)?;

        for item in &ast.items {
            let (params, body, item_span_val) = match item {
                Item::Action(a) => (&a.params, &a.body, a.span),
                Item::Function(f) => (&f.params, &f.body, f.span),
                Item::Lock(l) => (&l.params, &l.body, l.span),
                _ => continue,
            };

            let func_range = span_to_range(content, item_span_val);
            if !position_in_range(position, func_range) {
                continue;
            }

            // Check parameters.
            for param in params {
                if param.name == symbol {
                    let note = if param.is_mut {
                        "\n\nLeading `mut` only applies to local-style mutable value bindings; Cell state updates should be modeled with `action(before: T) -> after: T` plus `transition` and `require` constraints."
                    } else if param.source == ParamSource::Input {
                        "\n\n`input` marks a consumed transaction input Cell explicitly. Omitting it is equivalent for Cell-backed action parameters."
                    } else if param.source == ParamSource::Output {
                        "\n\n`output` marks a proposed transaction output Cell. Use `transition input.state: Live -> output.state: Filled` for state transitions and `require` for field continuity."
                    } else if param.source == ParamSource::LockArgs {
                        "\n\n`lock_args` is decoded from the executing lock Script.args bytes."
                    } else {
                        ""
                    };
                    return Some(Hover {
                        contents: format!("```cellscript\n{}: {}\n```\n\nParameter{}", param.name, type_to_string(&param.ty), note),
                        range: Some(span_to_range(content, param.span)),
                    });
                }
            }
            if let Item::Action(action) = item {
                for output in &action.outputs {
                    if output.name == symbol {
                        return Some(Hover {
                            contents: format!(
                                "```cellscript\n{}: {}\n```\n\nAction output binding: proposed transaction output Cell.",
                                output.name,
                                type_to_string(&output.ty)
                            ),
                            range: Some(span_to_range(content, output.span)),
                        });
                    }
                }
            }

            // Check local let bindings.
            for stmt in body {
                if let Stmt::Let(let_stmt) = stmt {
                    if let BindingPattern::Name(name) = &let_stmt.pattern {
                        if name == symbol {
                            let ty_str = let_stmt.ty.as_ref().map(type_to_string).unwrap_or_else(|| "_".to_string());
                            return Some(Hover {
                                contents: format!(
                                    "```cellscript\n{}{}: {}\n```\n\nLocal variable",
                                    if let_stmt.is_mut { "mut " } else { "" },
                                    name,
                                    ty_str
                                ),
                                range: Some(span_to_range(content, let_stmt.span)),
                            });
                        }
                    }
                }
            }
        }
        None
    }

    fn item_hover(&self, source: &str, item: &Item, metadata: Option<&crate::CompileMetadata>) -> Option<Hover> {
        let range = span_to_range(source, item_span(item));
        match item {
            Item::Resource(r) => Some(Hover {
                contents: format!("```cellscript\nresource {}\n```\n\nCapabilities: {:?}", r.name, r.capabilities),
                range: Some(range),
            }),
            Item::Shared(s) => Some(Hover { contents: format!("```cellscript\nshared {}\n```", s.name), range: Some(range) }),
            Item::Receipt(r) => Some(Hover {
                contents: format!("```cellscript\nreceipt {}\n```{}", r.name, receipt_flow_hover(r, metadata)),
                range: Some(range),
            }),
            Item::Struct(s) => Some(Hover { contents: format!("```cellscript\nstruct {}\n```", s.name), range: Some(range) }),
            Item::Action(a) => Some(Hover {
                contents: format!(
                    "```cellscript\naction {}\n```\n\n{}{}",
                    a.name,
                    a.doc_comment.as_deref().unwrap_or("No documentation"),
                    action_metadata_hover(&a.name, metadata)
                ),
                range: Some(range),
            }),
            Item::Function(f) => Some(Hover {
                contents: format!("```cellscript\nfn {}\n```\n\n{}", f.name, f.doc_comment.as_deref().unwrap_or("No documentation")),
                range: Some(range),
            }),
            Item::Lock(l) => Some(Hover { contents: format!("```cellscript\nlock {}\n```", l.name), range: Some(range) }),
            Item::Invariant(i) => Some(Hover { contents: format!("```cellscript\ninvariant {}\n```", i.name), range: Some(range) }),
            _ => None,
        }
    }

    pub fn document_symbols(&self, uri: &str) -> Vec<SymbolInformation> {
        let mut symbols = Vec::new();

        if let (Some(ast), Some(source)) = (self.ast_cache.get(uri), self.documents.get(uri)) {
            for item in &ast.items {
                if let Some(symbol) = self.item_symbol(source, item, uri) {
                    symbols.push(symbol);
                }
            }
        }

        symbols
    }

    fn item_symbol(&self, source: &str, item: &Item, uri: &str) -> Option<SymbolInformation> {
        match item {
            Item::Resource(r) => Some(SymbolInformation {
                name: r.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(source, r.span) },
                container_name: None,
            }),
            Item::Shared(s) => Some(SymbolInformation {
                name: s.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(source, s.span) },
                container_name: None,
            }),
            Item::Receipt(r) => Some(SymbolInformation {
                name: r.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(source, r.span) },
                container_name: None,
            }),
            Item::Struct(s) => Some(SymbolInformation {
                name: s.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(source, s.span) },
                container_name: None,
            }),
            Item::Const(c) => Some(SymbolInformation {
                name: c.name.clone(),
                kind: SymbolKind::Constant,
                location: Location { uri: uri.to_string(), range: span_to_range(source, c.span) },
                container_name: None,
            }),
            Item::Enum(e) => Some(SymbolInformation {
                name: e.name.clone(),
                kind: SymbolKind::Enum,
                location: Location { uri: uri.to_string(), range: span_to_range(source, e.span) },
                container_name: None,
            }),
            Item::Action(a) => Some(SymbolInformation {
                name: a.name.clone(),
                kind: SymbolKind::Function,
                location: Location { uri: uri.to_string(), range: span_to_range(source, a.span) },
                container_name: None,
            }),
            Item::Function(f) => Some(SymbolInformation {
                name: f.name.clone(),
                kind: SymbolKind::Function,
                location: Location { uri: uri.to_string(), range: span_to_range(source, f.span) },
                container_name: None,
            }),
            Item::Lock(l) => Some(SymbolInformation {
                name: l.name.clone(),
                kind: SymbolKind::Function,
                location: Location { uri: uri.to_string(), range: span_to_range(source, l.span) },
                container_name: None,
            }),
            Item::Invariant(i) => Some(SymbolInformation {
                name: i.name.clone(),
                kind: SymbolKind::Event,
                location: Location { uri: uri.to_string(), range: span_to_range(source, i.span) },
                container_name: None,
            }),
            _ => None,
        }
    }

    pub fn rename(&self, uri: &str, position: Position, new_name: String) -> HashMap<String, Vec<TextEdit>> {
        let _ = (uri, position, new_name);
        HashMap::new()
    }

    pub fn code_action(&self, uri: &str, range: Range) -> Vec<CodeAction> {
        let mut actions = Vec::new();
        let has_lowering_diagnostic = self
            .diagnostics
            .get(uri)
            .into_iter()
            .flatten()
            .any(|diagnostic| diagnostic.source == "cellscript-lowering" && ranges_overlap(diagnostic.range, range));

        if has_lowering_diagnostic {
            actions.push(CodeAction {
                title: "Inspect lowering/runtime metadata with `cellc metadata`".to_string(),
                kind: "quickfix".to_string(),
                edit: None,
            });
            actions.push(CodeAction {
                title: "Use `--target riscv64-asm` until executable stateful lowering is implemented".to_string(),
                kind: "quickfix".to_string(),
                edit: None,
            });
        }

        actions
    }

    pub fn format_document(&self, uri: &str) -> Vec<TextEdit> {
        let Some(content) = self.documents.get(uri) else {
            return Vec::new();
        };
        let Some(ast) = self.ast_cache.get(uri) else {
            return Vec::new();
        };
        let Ok(formatted) = crate::fmt::format_default(ast) else {
            return Vec::new();
        };
        if &formatted == content {
            return Vec::new();
        }
        vec![TextEdit { range: Range { start: Position { line: 0, character: 0 }, end: end_position(content) }, new_text: formatted }]
    }

    pub fn format_range(&self, uri: &str, _range: Range) -> Vec<TextEdit> {
        self.format_document(uri)
    }

    pub fn signature_help(&self, uri: &str, position: Position) -> Option<SignatureHelp> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;

        let (call_name, active_param) = self.find_call_at_offset(content, offset)?;

        let signature_info = self.find_signature(uri, &call_name)?;

        Some(SignatureHelp { signatures: vec![signature_info], active_signature: Some(0), active_parameter: Some(active_param) })
    }

    pub fn document_highlight(&self, uri: &str, position: Position) -> Vec<DocumentHighlight> {
        let Some(symbol) = self.symbol_at_position(uri, position) else {
            return Vec::new();
        };

        let mut highlights = Vec::new();

        if let Some(content) = self.documents.get(uri) {
            for (start, end) in word_occurrences(content, &symbol) {
                highlights.push(DocumentHighlight {
                    range: Range { start: offset_to_position(content, start), end: offset_to_position(content, end) },
                    kind: DocumentHighlightKind::Read,
                });
            }
        }

        highlights
    }

    pub fn folding_range(&self, uri: &str) -> Vec<FoldingRange> {
        let Some(ast) = self.ast_cache.get(uri) else {
            return Vec::new();
        };
        let Some(content) = self.documents.get(uri) else {
            return Vec::new();
        };

        let mut ranges = Vec::new();

        for item in &ast.items {
            match item {
                Item::Action(action) => {
                    let body_range = self.block_folding_range(content, &action.body, &action.name);
                    if let Some(range) = body_range {
                        ranges.push(range);
                    }
                }
                Item::Function(func) => {
                    let body_range = self.block_folding_range(content, &func.body, &func.name);
                    if let Some(range) = body_range {
                        ranges.push(range);
                    }
                }
                Item::Lock(lock) => {
                    let body_range = self.block_folding_range(content, &lock.body, &lock.name);
                    if let Some(range) = body_range {
                        ranges.push(range);
                    }
                }
                Item::Resource(r) => {
                    if !r.fields.is_empty() {
                        let range = span_to_range(content, r.span);
                        ranges.push(FoldingRange {
                            start_line: range.start.line,
                            start_character: Some(range.start.character),
                            end_line: range.end.line,
                            end_character: Some(range.end.character),
                            kind: Some(FoldingRangeKind::Region),
                        });
                    }
                }
                Item::Shared(s) => {
                    if !s.fields.is_empty() {
                        let range = span_to_range(content, s.span);
                        ranges.push(FoldingRange {
                            start_line: range.start.line,
                            start_character: Some(range.start.character),
                            end_line: range.end.line,
                            end_character: Some(range.end.character),
                            kind: Some(FoldingRangeKind::Region),
                        });
                    }
                }
                _ => {}
            }
        }

        ranges
    }

    pub fn selection_range(&self, uri: &str, position: Position) -> Option<SelectionRange> {
        let content = self.documents.get(uri)?;
        let ast = self.ast_cache.get(uri)?;
        let _offset = position_to_offset(content, position)?;

        let mut ranges: Vec<Range> = Vec::new();

        for item in &ast.items {
            let item_range = span_to_range(content, item_span(item));
            if position_in_range(position, item_range) {
                ranges.push(item_range);

                match item {
                    Item::Action(a) => {
                        for stmt in &a.body {
                            let stmt_range = span_to_range(content, stmt_span(stmt));
                            if position_in_range(position, stmt_range) {
                                ranges.push(stmt_range);
                            }
                        }
                    }
                    Item::Function(f) => {
                        for stmt in &f.body {
                            let stmt_range = span_to_range(content, stmt_span(stmt));
                            if position_in_range(position, stmt_range) {
                                ranges.push(stmt_range);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if ranges.is_empty() {
            let line_range = Range {
                start: Position { line: position.line, character: 0 },
                end: Position { line: position.line, character: u32::MAX },
            };
            ranges.push(line_range);
        }

        ranges.sort_by_key(|range| {
            let line_span = range.end.line.saturating_sub(range.start.line) as u64;
            let character_span = range.end.character.saturating_sub(range.start.character) as u64;
            line_span.saturating_mul(10_000).saturating_add(character_span)
        });

        let mut sorted = ranges.into_iter().rev();
        let mut result = SelectionRange { range: sorted.next()?, parent: None };
        for range in sorted {
            result = SelectionRange { range, parent: Some(Box::new(result)) };
        }

        Some(result)
    }

    fn find_call_at_offset(&self, content: &str, offset: usize) -> Option<(String, u32)> {
        let before = &content[..offset];
        let paren_pos = before.rfind('(')?;

        let _before_paren = &content[..paren_pos];
        let func_name = word_at_offset(content, paren_pos)?.to_string();

        let args_part = &content[paren_pos + 1..offset];
        let active_param = args_part.chars().filter(|c| *c == ',').count() as u32;

        Some((func_name, active_param))
    }

    fn find_signature(&self, uri: &str, name: &str) -> Option<SignatureInformation> {
        if let Some(ast) = self.ast_cache.get(uri) {
            if let Some(info) = self.find_signature_in_items(&ast.items, name) {
                return Some(info);
            }
        }

        for module in self.workspace_modules(uri) {
            if let Some(info) = self.find_signature_in_items(&module.ast.items, name) {
                return Some(info);
            }
        }

        None
    }

    fn find_signature_in_items(&self, items: &[Item], name: &str) -> Option<SignatureInformation> {
        for item in items {
            match item {
                Item::Action(a) if a.name == name => {
                    let params: Vec<ParameterInformation> = a
                        .params
                        .iter()
                        .map(|p| ParameterInformation { label: ParameterLabel::Simple(param_to_string(p)), documentation: None })
                        .collect();
                    let return_type = if !a.outputs.is_empty() {
                        action_outputs_to_string(&a.outputs)
                    } else {
                        a.return_type.as_ref().map(type_to_string).unwrap_or_default()
                    };
                    let label = format!(
                        "action {}({}) -> {}",
                        a.name,
                        params
                            .iter()
                            .map(|p| match &p.label {
                                ParameterLabel::Simple(s) => s.clone(),
                                ParameterLabel::Labelled { left, right } => format!("{}:{}", left, right),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        return_type
                    );
                    return Some(SignatureInformation { label, documentation: a.doc_comment.clone(), parameters: params });
                }
                Item::Function(f) if f.name == name => {
                    let params: Vec<ParameterInformation> = f
                        .params
                        .iter()
                        .map(|p| ParameterInformation { label: ParameterLabel::Simple(param_to_string(p)), documentation: None })
                        .collect();
                    let return_type = f.return_type.as_ref().map(type_to_string).unwrap_or_default();
                    let label = format!(
                        "fn {}({}) -> {}",
                        f.name,
                        params
                            .iter()
                            .map(|p| match &p.label {
                                ParameterLabel::Simple(s) => s.clone(),
                                ParameterLabel::Labelled { left, right } => format!("{}:{}", left, right),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        return_type
                    );
                    return Some(SignatureInformation { label, documentation: f.doc_comment.clone(), parameters: params });
                }
                Item::Lock(l) if l.name == name => {
                    let params: Vec<ParameterInformation> = l
                        .params
                        .iter()
                        .map(|p| ParameterInformation { label: ParameterLabel::Simple(param_to_string(p)), documentation: None })
                        .collect();
                    let label = format!(
                        "lock {}({}) -> {}",
                        l.name,
                        params
                            .iter()
                            .map(|p| match &p.label {
                                ParameterLabel::Simple(s) => s.clone(),
                                ParameterLabel::Labelled { left, right } => format!("{}:{}", left, right),
                            })
                            .collect::<Vec<_>>()
                            .join(", "),
                        type_to_string(&l.return_type)
                    );
                    return Some(SignatureInformation { label, documentation: None, parameters: params });
                }
                _ => {}
            }
        }
        None
    }

    fn block_folding_range(&self, content: &str, stmts: &[Stmt], _name: &str) -> Option<FoldingRange> {
        if stmts.is_empty() {
            return None;
        }
        let first_span = stmt_span(stmts.first()?);
        let last_span = stmt_span(stmts.last()?);
        let start_range = span_to_range(content, first_span);
        let end_range = span_to_range(content, last_span);
        Some(FoldingRange {
            start_line: start_range.start.line,
            start_character: Some(start_range.start.character),
            end_line: end_range.end.line,
            end_character: Some(end_range.end.character),
            kind: Some(FoldingRangeKind::Region),
        })
    }

    fn symbol_at_position(&self, uri: &str, position: Position) -> Option<String> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;
        word_at_offset(content, offset)
    }

    fn find_top_level_symbol(&self, uri: &str, symbol: &str) -> Option<Location> {
        if let (Some(ast), Some(source)) = (self.ast_cache.get(uri), self.documents.get(uri)) {
            if let Some(location) = ast.items.iter().find_map(|item| {
                let name = item_name(item)?;
                if name == symbol {
                    Some(Location { uri: uri.to_string(), range: span_to_range(source, item_span(item)) })
                } else {
                    None
                }
            }) {
                return Some(location);
            }
        }

        for module in self.workspace_modules(uri) {
            if let Some(location) = module.ast.items.iter().find_map(|item| {
                let name = item_name(item)?;
                if name == symbol {
                    Some(Location { uri: utf8_path_to_file_uri(&module.path), range: span_to_range(&module.source, item_span(item)) })
                } else {
                    None
                }
            }) {
                return Some(location);
            }
        }

        None
    }

    fn workspace_modules(&self, uri: &str) -> Vec<crate::LoadedModule> {
        self.workspace_modules_result(uri).unwrap_or_default()
    }

    fn workspace_modules_result(&self, uri: &str) -> Result<Vec<crate::LoadedModule>> {
        let Some(path) = file_uri_to_utf8_path(uri) else {
            return Ok(Vec::new());
        };

        let mut modules = lsp_load_workspace_modules_for_path(&path)?;

        if let (Some(content), Some(ast)) = (self.documents.get(uri), self.ast_cache.get(uri)) {
            if let Some(module) = modules.iter_mut().find(|module| same_workspace_path(&module.path, &path)) {
                module.source = content.clone();
                module.ast = ast.clone();
            } else {
                modules.push(crate::LoadedModule { path, source: content.clone(), ast: ast.clone() });
            }
        }

        Ok(modules)
    }

    fn primitive_strict_for_uri(&self, uri: &str) -> bool {
        if matches!(self.primitive_compat.as_deref(), Some("0.15")) {
            return true;
        }
        let Some(path) = file_uri_to_utf8_path(uri) else {
            return false;
        };
        lsp_manifest_primitive_compat(&path).as_deref() == Some("0.15")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAction {
    pub title: String,
    pub kind: String,
    pub edit: Option<WorkspaceEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEdit {
    pub changes: HashMap<String, Vec<TextEdit>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInformation>,
    pub active_signature: Option<u32>,
    pub active_parameter: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInformation {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<ParameterInformation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterInformation {
    pub label: ParameterLabel,
    pub documentation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParameterLabel {
    Simple(String),
    Labelled { left: String, right: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentHighlight {
    pub range: Range,
    pub kind: DocumentHighlightKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum DocumentHighlightKind {
    Text = 1,
    Read = 2,
    Write = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoldingRange {
    pub start_line: u32,
    pub start_character: Option<u32>,
    pub end_line: u32,
    pub end_character: Option<u32>,
    pub kind: Option<FoldingRangeKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoldingRangeKind {
    Comment,
    Imports,
    Region,
}

/// Context for completion at a given position.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CompletionContext {
    /// At a type position (after `:`, `->`, `<`).
    Type,
    /// At a member access position (after `.`), with the type name before the dot.
    Member { type_name: String },
    /// At a namespace access position (after `::`), with the type name before the scope separator.
    Namespace { type_name: String },
    /// At a top-level declaration position.
    Declaration,
    /// Inside an expression body.
    Expression,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionRange {
    pub range: Range,
    pub parent: Option<Box<SelectionRange>>,
}

/// Incremental text change event sent by the client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentContentChangeEvent {
    /// The range of the document that changed. If `None`, the whole document changed.
    pub range: Option<Range>,
    /// An optional length of the range that got replaced.
    pub range_length: Option<u32>,
    /// The new text of the range/document.
    pub text: String,
}

/// Apply a single incremental text change to a document string.
///
/// Replaces the text in `range` with `new_text`.
fn apply_incremental_change(content: &str, range: Range, new_text: &str) -> String {
    let Some(start_offset) = position_to_offset_strict(content, range.start) else {
        return content.to_string();
    };
    let Some(end_offset) = position_to_offset_strict(content, range.end) else {
        return content.to_string();
    };
    if start_offset > end_offset {
        return content.to_string();
    }
    let mut result = String::with_capacity(content.len() + new_text.len());
    result.push_str(&content[..start_offset]);
    result.push_str(new_text);
    result.push_str(&content[end_offset..]);
    result
}

fn span_to_range(source: &str, span: Span) -> Range {
    if span.start <= span.end && span.end <= source.len() && source.is_char_boundary(span.start) && source.is_char_boundary(span.end) {
        return Range { start: offset_to_position(source, span.start), end: offset_to_position(source, span.end) };
    }
    let fallback = Position {
        line: span.line.saturating_sub(1).min(u32::MAX as usize) as u32,
        character: span.column.saturating_sub(1).min(u32::MAX as usize) as u32,
    };
    Range { start: fallback, end: fallback }
}

fn diagnostic_from_error(source: &str, error: &CompileError) -> Diagnostic {
    Diagnostic {
        range: span_to_range(source, error.span),
        severity: DiagnosticSeverity::Error,
        message: error.message.clone(),
        source: "cellscript".to_string(),
    }
}

fn lowering_diagnostics(source: &str, module: &Module, metadata: &crate::CompileMetadata) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for action in &metadata.actions {
        if action.elf_compatible && action.fail_closed_runtime_features.is_empty() {
            continue;
        }
        let span = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Action(def) if def.name == action.name => Some(def.span),
                _ => None,
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: span_to_range(source, span),
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "action '{}' {}; fail-closed runtime features: {}; CKB runtime features: {}; CKB accesses: {}",
                action.name,
                if action.elf_compatible { "emits fail-closed runtime traps" } else { "is not currently ELF-compatible" },
                diagnostic_list(&action.fail_closed_runtime_features),
                diagnostic_list(&action.ckb_runtime_features),
                diagnostic_access_list(&action.ckb_runtime_accesses)
            ),
            source: "cellscript-lowering".to_string(),
        });
    }

    for lock in &metadata.locks {
        if lock.elf_compatible && lock.fail_closed_runtime_features.is_empty() {
            continue;
        }
        let span = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Lock(def) if def.name == lock.name => Some(def.span),
                _ => None,
            })
            .unwrap_or_default();
        diagnostics.push(Diagnostic {
            range: span_to_range(source, span),
            severity: DiagnosticSeverity::Warning,
            message: format!(
                "lock '{}' {}; fail-closed runtime features: {}; CKB runtime features: {}; CKB accesses: {}",
                lock.name,
                if lock.elf_compatible { "emits fail-closed runtime traps" } else { "is not currently ELF-compatible" },
                diagnostic_list(&lock.fail_closed_runtime_features),
                diagnostic_list(&lock.ckb_runtime_features),
                diagnostic_access_list(&lock.ckb_runtime_accesses)
            ),
            source: "cellscript-lowering".to_string(),
        });
    }

    diagnostics
}

fn diagnostic_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

fn diagnostic_access_list(accesses: &[crate::CkbRuntimeAccessMetadata]) -> String {
    if accesses.is_empty() {
        return "none".to_string();
    }
    accesses
        .iter()
        .map(|access| format!("{}:{}#{} ({})", access.operation, access.source, access.index, access.binding))
        .collect::<Vec<_>>()
        .join(", ")
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::Resource(r) => Some(&r.name),
        Item::Shared(s) => Some(&s.name),
        Item::Receipt(r) => Some(&r.name),
        Item::Struct(s) => Some(&s.name),
        Item::Flow(machine) => machine.name.as_deref(),
        Item::Const(c) => Some(&c.name),
        Item::Enum(e) => Some(&e.name),
        Item::Invariant(i) => Some(&i.name),
        Item::Action(a) => Some(&a.name),
        Item::Function(f) => Some(&f.name),
        Item::Lock(l) => Some(&l.name),
        Item::Use(_) => None,
    }
}

fn item_span(item: &Item) -> Span {
    match item {
        Item::Resource(r) => r.span,
        Item::Shared(s) => s.span,
        Item::Receipt(r) => r.span,
        Item::Struct(s) => s.span,
        Item::Flow(machine) => machine.span,
        Item::Const(c) => c.span,
        Item::Enum(e) => e.span,
        Item::Invariant(i) => i.span,
        Item::Action(a) => a.span,
        Item::Function(f) => f.span,
        Item::Lock(l) => l.span,
        Item::Use(u) => u.span,
    }
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Let(s) => s.span,
        Stmt::Return(s) => s.span,
        Stmt::If(s) => s.span,
        Stmt::For(s) => s.span,
        Stmt::While(s) => s.span,
        Stmt::Expr(expr) => expr.span(),
    }
}

fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::U8 => "u8".to_string(),
        Type::U16 => "u16".to_string(),
        Type::U32 => "u32".to_string(),
        Type::U64 => "u64".to_string(),
        Type::U128 => "u128".to_string(),
        Type::Bool => "bool".to_string(),
        Type::Unit => "()".to_string(),
        Type::Address => "Address".to_string(),
        Type::Hash => "Hash".to_string(),
        Type::Array(inner, size) => format!("[{}; {}]", type_to_string(inner), size),
        Type::Tuple(types) => format!("({})", types.iter().map(type_to_string).collect::<Vec<_>>().join(", ")),
        Type::Named(name) => name.clone(),
        Type::Ref(inner) => format!("&{}", type_to_string(inner)),
        Type::MutRef(inner) => format!("&mut {}", type_to_string(inner)),
    }
}

fn param_to_string(param: &Param) -> String {
    let mut rendered = String::new();
    if param.is_mut {
        rendered.push_str("mut ");
    }
    if param.is_ref {
        rendered.push('&');
    }
    match param.source {
        ParamSource::Input => rendered.push_str("input "),
        ParamSource::Output => rendered.push_str("output "),
        ParamSource::Protected => rendered.push_str("protected "),
        ParamSource::Witness => rendered.push_str("witness "),
        ParamSource::LockArgs => rendered.push_str("lock_args "),
        ParamSource::Default if param.is_read_ref => rendered.push_str("read "),
        ParamSource::Default => {}
    }
    rendered.push_str(&param.name);
    rendered.push_str(": ");
    let ty = match (&param.source, &param.ty) {
        (ParamSource::Protected, Type::Ref(inner)) => inner.as_ref(),
        (ParamSource::Default, Type::Ref(inner)) if param.is_read_ref => inner.as_ref(),
        _ => &param.ty,
    };
    rendered.push_str(&type_to_string(ty));
    rendered
}

fn action_outputs_to_string(outputs: &[ActionOutput]) -> String {
    if outputs.len() == 1 {
        format!("{}: {}", outputs[0].name, type_to_string(&outputs[0].ty))
    } else {
        format!(
            "({})",
            outputs.iter().map(|output| format!("{}: {}", output.name, type_to_string(&output.ty))).collect::<Vec<_>>().join(", ")
        )
    }
}

fn position_in_range(pos: Position, range: Range) -> bool {
    position_le(range.start, pos) && position_le(pos, range.end)
}

fn receipt_flow_hover(receipt: &ReceiptDef, metadata: Option<&crate::CompileMetadata>) -> String {
    if let Some(type_metadata) =
        metadata.and_then(|metadata| metadata.types.iter().find(|type_metadata| type_metadata.name == receipt.name))
    {
        if type_metadata.flow_states.is_empty() {
            return String::new();
        }

        let transitions = if type_metadata.flow_transitions.is_empty() {
            "none".to_string()
        } else {
            type_metadata
                .flow_transitions
                .iter()
                .map(|transition| {
                    format!("{}[{}] -> {}[{}]", transition.from, transition.from_index, transition.to, transition.to_index)
                })
                .collect::<Vec<_>>()
                .join(", ")
        };

        return format!(
            "\n\n**Flow metadata**\n\nStates: `{}`\n\nTransitions: `{}`",
            type_metadata.flow_states.join(" -> "),
            transitions
        );
    }

    String::new()
}

fn action_metadata_hover(name: &str, metadata: Option<&crate::CompileMetadata>) -> String {
    let Some(metadata) = metadata else {
        return String::new();
    };
    let Some(action) = metadata.actions.iter().find(|action| action.name == name) else {
        return String::new();
    };

    let fail_closed_features = if action.fail_closed_runtime_features.is_empty() {
        "none".to_string()
    } else {
        action.fail_closed_runtime_features.join(", ")
    };
    let ckb_features =
        if action.ckb_runtime_features.is_empty() { "none".to_string() } else { action.ckb_runtime_features.join(", ") };
    let accesses = if action.ckb_runtime_accesses.is_empty() {
        "none".to_string()
    } else {
        action
            .ckb_runtime_accesses
            .iter()
            .map(|access| format!("{}:{}#{} ({})", access.operation, access.source, access.index, access.binding))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let obligations = if action.verifier_obligations.is_empty() {
        "none".to_string()
    } else {
        action
            .verifier_obligations
            .iter()
            .map(|obligation| format!("{}:{} ({})", obligation.category, obligation.feature, obligation.status))
            .collect::<Vec<_>>()
            .join(", ")
    };

    format!(
        "\n\n**Lowering metadata**\n\nEffect: `{}`\n\nELF compatible: `{}`\n\nStandalone runner compatible: `{}`\n\nFail-closed runtime features: `{}`\n\nCKB runtime features: `{}`\n\nCKB runtime accesses: `{}`\n\nVerifier obligations: `{}`",
        action.effect_class,
        action.elf_compatible,
        action.standalone_runner_compatible,
        fail_closed_features,
        ckb_features,
        accesses,
        obligations
    )
}

fn position_to_offset(source: &str, position: Position) -> Option<usize> {
    position_to_offset_with_boundary(source, position, Utf16BoundaryMode::SnapForward)
}

fn position_to_offset_strict(source: &str, position: Position) -> Option<usize> {
    position_to_offset_with_boundary(source, position, Utf16BoundaryMode::Strict)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Utf16BoundaryMode {
    Strict,
    SnapForward,
}

fn position_to_offset_with_boundary(source: &str, position: Position, boundary_mode: Utf16BoundaryMode) -> Option<usize> {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut iter = source.char_indices().peekable();

    while let Some((idx, ch)) = iter.next() {
        if line == position.line && col == position.character {
            return Some(idx);
        }
        if ch == '\r' && iter.peek().is_some_and(|(_, next)| *next == '\n') {
            let (_, next) = iter.next()?;
            debug_assert_eq!(next, '\n');
            line += 1;
            col = 0;
        } else if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col = col.checked_add(ch.len_utf16() as u32)?;
            if line == position.line && col == position.character {
                return Some(idx + ch.len_utf8());
            }
            if line == position.line && col > position.character {
                return match boundary_mode {
                    Utf16BoundaryMode::Strict => None,
                    Utf16BoundaryMode::SnapForward => Some(idx + ch.len_utf8()),
                };
            }
        }
    }

    if line == position.line && col == position.character {
        Some(source.len())
    } else {
        None
    }
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;
    let mut iter = source.char_indices().peekable();
    while let Some((idx, ch)) = iter.next() {
        if idx >= offset {
            break;
        }
        if ch == '\r' && iter.peek().is_some_and(|(_, next)| *next == '\n') {
            if let Some((next_idx, next)) = iter.next() {
                debug_assert_eq!(next, '\n');
                line += 1;
                col = 0;
                if next_idx >= offset {
                    break;
                }
            }
        } else if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }
    Position { line, character: col }
}

fn end_position(source: &str) -> Position {
    offset_to_position(source, source.len())
}

fn ranges_overlap(left: Range, right: Range) -> bool {
    position_le(left.start, right.end) && position_le(right.start, left.end)
}

fn position_le(left: Position, right: Position) -> bool {
    left.line < right.line || (left.line == right.line && left.character <= right.character)
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn word_at_offset(source: &str, offset: usize) -> Option<String> {
    if source.is_empty() || offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let mut start = offset;
    while start > 0 {
        let prev_idx = source[..start].char_indices().last()?.0;
        let ch = source[prev_idx..start].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        start = prev_idx;
    }

    let mut end = offset;
    while end < source.len() {
        let ch = source[end..].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        end += ch.len_utf8();
    }

    if start == end {
        None
    } else {
        Some(source[start..end].to_string())
    }
}

/// Get the word immediately before the given offset in `source`.
/// Unlike `word_at_offset`, this scans backwards from `offset` and stops at
/// the first non-identifier character, returning the identifier that ends
/// just before `offset`.
fn word_before_offset(source: &str, offset: usize) -> Option<String> {
    if source.is_empty() || offset == 0 || offset > source.len() {
        return None;
    }
    // Skip trailing whitespace.
    let mut end = offset;
    while end > 0 {
        let prev_idx = source[..end].char_indices().last()?.0;
        let ch = source[prev_idx..end].chars().next()?;
        if !ch.is_whitespace() {
            break;
        }
        end = prev_idx;
    }
    if end == 0 {
        return None;
    }
    // Scan the identifier backwards.
    let mut start = end;
    while start > 0 {
        let prev_idx = source[..start].char_indices().last()?.0;
        let ch = source[prev_idx..start].chars().next()?;
        if !is_ident_char(ch) {
            break;
        }
        start = prev_idx;
    }
    if start == end {
        None
    } else {
        Some(source[start..end].to_string())
    }
}

fn word_occurrences(source: &str, symbol: &str) -> Vec<(usize, usize)> {
    let mut matches = Vec::new();
    if symbol.is_empty() {
        return matches;
    }

    let Ok(tokens) = crate::lexer::lex(source) else {
        return matches;
    };
    for token in tokens {
        if let TokenKind::Identifier(name) = token.kind {
            if name == symbol {
                matches.push((token.span.start, token.span.end));
            }
        }
    }
    matches
}

fn word_occurrences_in_range(source: &str, symbol: &str, range: Range) -> Vec<(usize, usize)> {
    let Some((scope_start, scope_end)) = range_to_offsets(source, range) else {
        return Vec::new();
    };
    word_occurrences(source, symbol).into_iter().filter(|(start, end)| *start >= scope_start && *end <= scope_end).collect()
}

fn range_to_offsets(source: &str, range: Range) -> Option<(usize, usize)> {
    let start = position_to_offset(source, range.start)?;
    let end = position_to_offset(source, range.end)?;
    (start <= end).then_some((start, end))
}

fn lsp_load_workspace_modules_for_path(path: &Utf8Path) -> Result<Vec<crate::LoadedModule>> {
    let files = lsp_workspace_cell_files(path)?;
    let mut modules = Vec::new();
    let mut total_bytes = 0usize;
    for path in files {
        let source = std::fs::read_to_string(&path)
            .map_err(|error| CompileError::new(format!("failed to read module '{}': {}", path, error), Span::default()))?;
        total_bytes = total_bytes
            .checked_add(source.len())
            .ok_or_else(|| CompileError::new("LSP workspace source size overflow while loading modules", Span::default()))?;
        if total_bytes > LSP_MAX_WORKSPACE_BYTES {
            return Err(CompileError::new(
                format!(
                    "LSP workspace source size exceeds {} bytes; narrow Cell.toml package.source_roots or close large workspace files",
                    LSP_MAX_WORKSPACE_BYTES
                ),
                Span::default(),
            ));
        }
        let tokens = crate::lexer::lex(&source).map_err(|error| error.with_file(path.clone()))?;
        let ast = crate::parser::parse(&tokens).map_err(|error| error.with_file(path.clone()))?;
        modules.push(crate::LoadedModule { path, source, ast });
    }
    Ok(modules)
}

fn lsp_workspace_cell_files(path: &Utf8Path) -> Result<Vec<Utf8PathBuf>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let path = lsp_canonical_utf8_path(path)?;
    let Some(package_root) = lsp_find_package_root(&path)? else {
        let mut files = Vec::new();
        let mut seen_files = HashSet::new();
        if let Some(parent) = path.parent() {
            lsp_collect_cell_files_recursive(parent, &mut files, &mut seen_files)?;
        }
        if seen_files.insert(path.clone()) {
            files.push(path);
        }
        files.sort();
        return Ok(files);
    };

    let manifest = lsp_read_manifest_value(&package_root)?;
    let mut roots = Vec::new();
    let mut seen_roots = HashSet::new();
    for source_root in lsp_manifest_source_roots(&manifest) {
        let root = lsp_package_child_path(&package_root, &source_root)?;
        if root.is_dir() && seen_roots.insert(root.clone()) {
            roots.push(root);
        }
    }
    if roots.is_empty() {
        let src_root = package_root.join("src");
        if src_root.is_dir() {
            let src_root = lsp_canonical_utf8_path(&src_root)?;
            if seen_roots.insert(src_root.clone()) {
                roots.push(src_root);
            }
        }
    }
    if let Some(entry) = lsp_manifest_entry(&manifest) {
        let entry_path = lsp_package_child_path(&package_root, &entry)?;
        if let Some(parent) = entry_path.parent() {
            let parent = lsp_canonical_utf8_path(parent)?;
            if parent.is_dir() && seen_roots.insert(parent.clone()) {
                roots.push(parent);
            }
        }
    }

    let mut files = Vec::new();
    let mut seen_files = HashSet::new();
    for root in roots {
        lsp_collect_cell_files_recursive(&root, &mut files, &mut seen_files)?;
    }
    if seen_files.insert(path.clone()) {
        files.push(path);
    }
    files.sort();
    Ok(files)
}

fn lsp_collect_cell_files_recursive(root: &Utf8Path, files: &mut Vec<Utf8PathBuf>, seen: &mut HashSet<Utf8PathBuf>) -> Result<()> {
    if files.len() >= LSP_MAX_WORKSPACE_MODULES {
        return Err(CompileError::new(
            format!("LSP workspace module count exceeds {}; narrow Cell.toml package.source_roots", LSP_MAX_WORKSPACE_MODULES),
            Span::default(),
        ));
    }
    let entries = std::fs::read_dir(root)
        .map_err(|error| CompileError::new(format!("failed to read module directory '{}': {}", root, error), Span::default()))?;
    for entry in entries {
        let entry = entry.map_err(|error| CompileError::new(format!("failed to read directory entry: {}", error), Span::default()))?;
        let Ok(candidate) = Utf8PathBuf::from_path_buf(entry.path()) else {
            continue;
        };
        if candidate.is_dir() {
            if matches!(candidate.file_name(), Some(".git" | ".cell" | "target")) {
                continue;
            }
            lsp_collect_cell_files_recursive(&candidate, files, seen)?;
            continue;
        }
        if candidate.extension() == Some("cell") {
            let candidate = lsp_canonical_utf8_path(&candidate)?;
            if seen.insert(candidate.clone()) {
                files.push(candidate);
                if files.len() > LSP_MAX_WORKSPACE_MODULES {
                    return Err(CompileError::new(
                        format!(
                            "LSP workspace module count exceeds {}; narrow Cell.toml package.source_roots",
                            LSP_MAX_WORKSPACE_MODULES
                        ),
                        Span::default(),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn lsp_find_package_root(path: &Utf8Path) -> Result<Option<Utf8PathBuf>> {
    let mut cursor = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    loop {
        if cursor.join("Cell.toml").is_file() {
            return Ok(Some(lsp_canonical_utf8_path(cursor)?));
        }
        let Some(parent) = cursor.parent() else {
            return Ok(None);
        };
        cursor = parent;
    }
}

fn lsp_manifest_primitive_compat(path: &Utf8Path) -> Option<String> {
    let package_root = lsp_find_package_root(path).ok().flatten()?;
    let manifest = lsp_read_manifest_value(&package_root).ok()?;
    let build = manifest.get("build")?;
    if let Some(mode) = build.get("primitive_compat").and_then(toml::Value::as_str) {
        return Some(mode.to_string());
    }
    match build.get("primitive_strict") {
        Some(value) if value.as_bool() == Some(true) => Some("0.15".to_string()),
        Some(value) => value.as_str().map(str::to_string),
        None => None,
    }
}

fn lsp_read_manifest_value(package_root: &Utf8Path) -> Result<toml::Value> {
    let manifest_path = package_root.join("Cell.toml");
    let source = std::fs::read_to_string(&manifest_path)
        .map_err(|error| CompileError::new(format!("failed to read Cell.toml '{}': {}", manifest_path, error), Span::default()))?;
    toml::from_str(&source)
        .map_err(|error| CompileError::new(format!("failed to parse Cell.toml '{}': {}", manifest_path, error), Span::default()))
}

fn lsp_manifest_source_roots(manifest: &toml::Value) -> Vec<String> {
    manifest
        .get("package")
        .and_then(|package| package.get("source_roots"))
        .and_then(toml::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_string))
        .collect()
}

fn lsp_manifest_entry(manifest: &toml::Value) -> Option<String> {
    manifest.get("package").and_then(|package| package.get("entry")).and_then(toml::Value::as_str).map(str::to_string)
}

fn lsp_package_child_path(package_root: &Utf8Path, raw_path: &str) -> Result<Utf8PathBuf> {
    let candidate = package_root.join(raw_path);
    let canonical = lsp_canonical_utf8_path(&candidate)?;
    if !canonical.starts_with(package_root) {
        return Err(CompileError::new(
            format!("Cell.toml path '{}' resolves outside package root '{}'", raw_path, package_root),
            Span::default(),
        ));
    }
    Ok(canonical)
}

fn lsp_canonical_utf8_path(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| CompileError::new(format!("failed to canonicalize '{}': {}", path, error), Span::default()))?;
    Utf8PathBuf::from_path_buf(canonical)
        .map_err(|path| CompileError::new(format!("path is not valid UTF-8: {}", path.display()), Span::default()))
}

fn file_uri_to_utf8_path(uri: &str) -> Option<Utf8PathBuf> {
    let path = uri.strip_prefix("file://")?;
    let decoded = percent_decode(path)?;
    let candidate = Utf8PathBuf::from(decoded);
    match std::fs::canonicalize(&candidate) {
        Ok(path) => Utf8PathBuf::from_path_buf(path).ok(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(candidate),
        Err(_) => None,
    }
}

fn utf8_path_to_file_uri(path: &camino::Utf8Path) -> String {
    format!("file://{}", path)
}

fn same_workspace_path(left: &camino::Utf8Path, right: &camino::Utf8Path) -> bool {
    left == right
        || std::fs::canonicalize(left).ok().zip(std::fs::canonicalize(right).ok()).map(|(left, right)| left == right).unwrap_or(false)
}

fn percent_decode(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut idx = 0;
    while idx < bytes.len() {
        if bytes[idx] == b'%' {
            if idx + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[idx + 1])?;
            let lo = hex_nibble(bytes[idx + 2])?;
            out.push((hi << 4) | lo);
            idx += 3;
        } else {
            out.push(bytes[idx]);
            idx += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(10 + byte - b'a'),
        b'A'..=b'F' => Some(10 + byte - b'A'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_lsp_position_conversion_uses_utf16_columns() {
        let source = "a😀b\nβc";
        let b_offset = source.find('b').expect("b offset");
        let beta_offset = source.find('β').expect("beta offset");

        assert_eq!(offset_to_position(source, b_offset), Position { line: 0, character: 3 });
        assert_eq!(position_to_offset(source, Position { line: 0, character: 3 }), Some(b_offset));
        assert_eq!(position_to_offset(source, Position { line: 0, character: 2 }), Some(b_offset));
        assert_eq!(position_to_offset_strict(source, Position { line: 0, character: 2 }), None);
        assert_eq!(offset_to_position(source, beta_offset), Position { line: 1, character: 0 });
        assert_eq!(position_to_offset(source, Position { line: 1, character: 1 }), Some(beta_offset + 'β'.len_utf8()));
    }

    #[test]
    fn span_to_range_rejects_invalid_utf8_boundaries() {
        let source = "aβ\nz";
        let range = span_to_range(source, Span::new(1, 2, 3, 4));

        assert_eq!(range.start, Position { line: 2, character: 3 });
        assert_eq!(range.end, range.start);
    }

    #[test]
    fn word_at_offset_rejects_non_boundary_offsets() {
        let source = "aβ";

        assert_eq!(word_at_offset(source, 2), None);
        assert_eq!(word_at_offset(source, source.len()), Some("aβ".to_string()));
    }

    #[test]
    fn stmt_span_uses_return_statement_span() {
        let return_span = Span::new(10, 16, 2, 5);
        let expr_span = Span::new(17, 18, 2, 12);

        let return_stmt = Stmt::Return(ReturnStmt { value: None, span: return_span });
        let expr_stmt = Stmt::Expr(Expr::Integer(1, expr_span));

        assert_eq!(stmt_span(&return_stmt), return_span);
        assert_eq!(stmt_span(&expr_stmt), expr_span);
    }

    #[test]
    fn file_uri_to_utf8_path_canonicalizes_existing_paths_and_preserves_missing_paths() {
        let dir = tempdir().unwrap();
        let file = Utf8PathBuf::from_path_buf(dir.path().join("source.cell")).unwrap();
        std::fs::write(&file, "module source\n").unwrap();

        let resolved = file_uri_to_utf8_path(&utf8_path_to_file_uri(&file)).unwrap();
        assert_eq!(resolved, lsp_canonical_utf8_path(&file).unwrap());

        let missing = Utf8PathBuf::from_path_buf(dir.path().join("missing.cell")).unwrap();
        assert_eq!(file_uri_to_utf8_path(&utf8_path_to_file_uri(&missing)), Some(missing));
    }

    #[test]
    fn lsp_position_conversion_treats_crlf_as_single_line_ending() {
        let source = "alpha\r\nbeta";
        let beta_offset = source.find("beta").expect("beta offset");

        assert_eq!(offset_to_position(source, beta_offset), Position { line: 1, character: 0 });
        assert_eq!(offset_to_position(source, source.find('\n').expect("lf offset")), Position { line: 1, character: 0 });
        assert_eq!(position_to_offset(source, Position { line: 0, character: 5 }), Some(5));
        assert_eq!(position_to_offset(source, Position { line: 1, character: 0 }), Some(beta_offset));
        assert_eq!(position_to_offset(source, Position { line: 1, character: 4 }), Some(source.len()));
    }

    #[test]
    fn lsp_position_incremental_change_applies_crlf_ranges() {
        let source = "module demo\r\n// marker\r\n";
        let marker_start = source.find("marker").expect("marker start");
        let marker_end = marker_start + "marker".len();
        let updated = apply_incremental_change(
            source,
            Range { start: offset_to_position(source, marker_start), end: offset_to_position(source, marker_end) },
            "done",
        );

        assert_eq!(updated, "module demo\r\n// done\r\n");
    }

    #[test]
    fn lsp_rejects_oversized_documents() {
        let mut server = LspServer::new();
        let uri = "file:///oversized.cell".to_string();

        server.open_document(uri.clone(), "x".repeat(LSP_MAX_DOCUMENT_BYTES + 1));

        assert!(!server.documents.contains_key(&uri));
        assert!(server.get_diagnostics(&uri).iter().any(|diagnostic| diagnostic.message.contains("too large")));
    }

    #[test]
    fn lsp_rejects_document_count_over_limit() {
        let mut server = LspServer::new();
        for index in 0..LSP_MAX_OPEN_DOCUMENTS {
            server.documents.insert(format!("file:///{index}.cell"), String::new());
        }
        let uri = "file:///extra.cell".to_string();

        server.open_document(uri.clone(), "module extra".to_string());

        assert!(!server.documents.contains_key(&uri));
        assert!(server.get_diagnostics(&uri).iter().any(|diagnostic| diagnostic.message.contains("too many open documents")));
    }

    #[test]
    fn test_incremental_change_applies_utf16_ranges_after_non_bmp_text() {
        let source = "module demo\n// 😀 marker\n";
        let start = source.find("marker").expect("marker start");
        let end = start + "marker".len();
        let updated = apply_incremental_change(
            source,
            Range { start: offset_to_position(source, start), end: offset_to_position(source, end) },
            "done",
        );

        assert_eq!(updated, "module demo\n// 😀 done\n");
    }

    #[test]
    fn test_incremental_change_ignores_invalid_utf16_ranges() {
        let source = "module demo\n// 😀 marker\n";

        let invalid_surrogate_middle = apply_incremental_change(
            source,
            Range { start: Position { line: 1, character: 4 }, end: Position { line: 1, character: 4 } },
            "bad",
        );
        assert_eq!(invalid_surrogate_middle, source);

        let reversed = apply_incremental_change(
            source,
            Range { start: Position { line: 1, character: 12 }, end: Position { line: 1, character: 8 } },
            "bad",
        );
        assert_eq!(reversed, source);
    }

    #[test]
    fn test_lsp_server() {
        let mut server = LspServer::new();

        let uri = "file:///test.cell".to_string();
        let content = "module test;\n\naction answer() -> u64\nwhere\n    42\n".to_string();

        server.open_document(uri.clone(), content);
        assert!(server.get_diagnostics(&uri).is_empty());

        let completions = server.completion(&uri, Position { line: 0, character: 0 });
        assert!(!completions.is_empty());

        let keywords: Vec<_> = completions.iter().filter(|c| c.kind == CompletionItemKind::Keyword).collect();
        assert!(!keywords.is_empty());
    }

    #[test]
    fn test_selection_range_orders_child_before_parent() {
        let mut server = LspServer::new();
        let uri = "file:///selection.cell".to_string();
        let content = "module test\n\naction answer(x: u64) -> u64\nwhere\n    let y = x + 1\n    return y\n".to_string();

        server.open_document(uri.clone(), content);
        let selection = server.selection_range(&uri, Position { line: 4, character: 8 }).expect("selection range");
        let parent = selection.parent.as_ref().expect("parent range");

        assert!(position_le(parent.range.start, selection.range.start));
        assert!(position_le(selection.range.end, parent.range.end));
    }

    #[test]
    fn test_keyword_completions() {
        let server = LspServer::new();
        let keywords = server.keyword_completions();

        assert!(keywords.iter().any(|k| k.label == "module"));
        assert!(keywords.iter().any(|k| k.label == "resource"));
        assert!(keywords.iter().any(|k| k.label == "action"));
        assert!(keywords.iter().any(|k| k.label == "flow"));
        assert!(keywords.iter().any(|k| k.label == "input"));
        assert!(!keywords.iter().any(|k| k.label == "output"));
        assert!(keywords.iter().any(|k| k.label == "transition"));
        assert!(!keywords.iter().any(|k| k.label == "move"));
        assert!(keywords.iter().any(|k| k.label == "require"));
        assert!(!keywords.iter().any(|k| k.label == "transfer"));
        assert!(keywords.iter().any(|k| k.label == "std::cell::same_lock"));
        assert!(keywords.iter().any(|k| k.label == "std::cell::preserve_capacity"));
        assert!(keywords.iter().any(|k| k.label == "std::lifecycle::transfer"));
        assert!(keywords.iter().any(|k| k.label == "std::receipt::claim"));
        assert!(keywords.iter().any(|k| k.label == "std::lifecycle::settle"));
        assert!(keywords.iter().any(|k| k.label == "protected"));
        assert!(keywords.iter().any(|k| k.label == "witness"));
        assert!(keywords.iter().any(|k| k.label == "lock_args"));
    }

    #[test]
    fn test_ckb_namespace_completions() {
        let server = LspServer::new();

        let env = server.member_completions("file:///test.cell", "env");
        assert!(env.iter().any(|item| item.label == "sighash_all"));

        let source = server.member_completions("file:///test.cell", "source");
        assert!(source.iter().any(|item| item.label == "group_input"));

        let witness = server.member_completions("file:///test.cell", "witness");
        assert!(witness.iter().any(|item| item.label == "lock"));

        let ckb = server.member_completions("file:///test.cell", "ckb");
        assert!(ckb.iter().any(|item| item.label == "input_since"));
    }

    #[test]
    fn test_vec_member_completions_match_supported_helpers() {
        let server = LspServer::new();
        let completions = server.member_completions("file:///test.cell", "Vec");
        let labels = completions.iter().map(|item| item.label.as_str()).collect::<std::collections::BTreeSet<_>>();

        for helper in [
            "new",
            "with_capacity",
            "capacity",
            "push",
            "extend_from_slice",
            "len",
            "is_empty",
            "first",
            "last",
            "contains",
            "set",
            "remove",
            "pop",
            "insert",
            "reverse",
            "truncate",
            "swap",
            "clear",
        ] {
            assert!(labels.contains(helper), "missing Vec completion for {helper}");
        }
        assert!(!labels.contains("get"), "Vec completion should not advertise unsupported get()");
    }

    #[test]
    fn test_flow_u8_namespace_completions() {
        let mut server = LspServer::new();
        let uri = "file:///flow_completion.cell".to_string();
        let source = r#"
module flow_completion

receipt Ticket has store {
    state: u8,
    id: u64,
}

receipt OtherTicket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
    Active -> Closed;
}

flow OtherTicket.state {
    Draft -> Live;
}

action activate(ticket: Ticket) -> active_ticket: Ticket
    transition ticket.state: Created -> active_ticket.state: Active
where
    assert_invariant(ticket.state < Ticket::Closed, "closed")
    require active_ticket.state == Ticket::Active
    require active_ticket.id == ticket.id
"#
        .to_string();

        server.open_document(uri.clone(), source.clone());
        assert!(server.get_diagnostics(&uri).is_empty());

        let offset = source.find("Ticket::Active").expect("qualified state") + "Ticket::".len();
        let completions = server.completion(&uri, offset_to_position(&source, offset));
        let labels = completions.iter().map(|item| item.label.as_str()).collect::<std::collections::BTreeSet<_>>();

        assert!(labels.contains("Created"));
        assert!(labels.contains("Active"));
        assert!(labels.contains("Closed"));
        assert!(!labels.contains("Live"), "Ticket:: completion must not leak OtherTicket flow states");
        assert!(completions.iter().any(|item| {
            item.label == "Active"
                && item.kind == CompletionItemKind::EnumMember
                && item.detail.as_deref() == Some("flow state Ticket::Active")
        }));
    }

    #[test]
    fn test_flow_namespace_completions() {
        let mut server = LspServer::new();
        let uri = "file:///flow_completion.cell".to_string();
        let source = r#"
module flow_completion

enum OfferState {
    Created,
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState,
    amount: u64,
}

flow OfferFlow for Offer.state {
    Created -> Live;
    Live -> Filled by accept;
}

action accept(input: Offer) -> output: Offer
    transition input.state: Live -> output.state: Filled
where
    require output.state == Offer::Filled
    require output.amount == input.amount
"#
        .to_string();

        server.open_document(uri.clone(), source.clone());
        assert!(server.get_diagnostics(&uri).is_empty());

        let offset = source.find("Offer::Filled").expect("qualified state") + "Offer::".len();
        let completions = server.completion(&uri, offset_to_position(&source, offset));
        let labels = completions.iter().map(|item| item.label.as_str()).collect::<std::collections::BTreeSet<_>>();

        assert!(labels.contains("Created"));
        assert!(labels.contains("Live"));
        assert!(labels.contains("Filled"));
        assert!(completions.iter().any(|item| {
            item.label == "Filled"
                && item.kind == CompletionItemKind::EnumMember
                && item.detail.as_deref() == Some("flow state Offer::Filled")
        }));
    }

    #[test]
    fn test_parse_errors_become_diagnostics() {
        let mut server = LspServer::new();
        let uri = "file:///bad.cell".to_string();
        server.open_document(uri.clone(), "module bad;\naction broken( {\n".to_string());
        let diagnostics = server.get_diagnostics(&uri);
        assert!(!diagnostics.is_empty());
        assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn lsp_primitive_strict_rejects_legacy_capabilities() {
        let mut server = LspServer::new();
        server.set_primitive_compat(Some("0.15".to_string()));
        let uri = "file:///strict.cell".to_string();
        let source = r#"
module strict

resource Coin has store, transfer {
    amount: u64,
}
"#;
        server.open_document(uri.clone(), source.to_string());
        let diagnostics = server.get_diagnostics(&uri);
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.message.contains("CS0150")),
            "strict LSP diagnostics should reject legacy transfer capability: {:?}",
            diagnostics
        );
    }

    #[test]
    fn lsp_reads_primitive_strict_from_manifest() {
        let temp = tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cell.toml"), "[package]\nentry = \"src/main.cell\"\n\n[build]\nprimitive_compat = \"0.15\"\n")
            .unwrap();
        let source = r#"
module strict_manifest

resource Coin has store, transfer {
    amount: u64,
}
"#;
        let main_path = root.join("src/main.cell");
        std::fs::write(&main_path, source).unwrap();

        let mut server = LspServer::new();
        let uri = utf8_path_to_file_uri(&main_path);
        server.open_document(uri.clone(), source.to_string());
        let diagnostics = server.get_diagnostics(&uri);
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.message.contains("CS0150")),
            "manifest strict LSP diagnostics should reject legacy transfer capability: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_goto_definition_and_references() {
        let mut server = LspServer::new();
        let uri = "file:///defs.cell".to_string();
        let source =
            "module defs;\n\nresource Token {\n    amount: u64,\n}\n\naction make() -> u64\nwhere\n    let token = Token { amount: 1 };\n    token.amount\n";
        server.open_document(uri.clone(), source.to_string());

        let definition = server.goto_definition(&uri, Position { line: 8, character: 16 }).expect("definition");
        assert_eq!(definition.range.start.line, 2);

        let refs = server.find_references(&uri, Position { line: 8, character: 16 });
        assert!(refs.len() >= 2);
    }

    #[test]
    fn goto_definition_prefers_local_scope_over_top_level_symbol() {
        let mut server = LspServer::new();
        let uri = "file:///shadow.cell".to_string();
        let source = r#"
module shadow

resource Token {
    amount: u64,
}

action inspect() -> u64
where
    let Token = 1
    return Token
"#;
        server.open_document(uri.clone(), source.to_string());
        let token_use = source.find("return Token").expect("return Token") + "return ".len();
        let definition =
            server.goto_definition(&uri, offset_to_position(source, token_use)).expect("local shadow definition should resolve");
        assert_eq!(definition.range.start.line, 9, "local binding should win over top-level resource: {:?}", definition);
    }

    #[test]
    fn find_references_for_locals_stays_in_enclosing_callable_scope() {
        let mut server = LspServer::new();
        let uri = "file:///local_refs.cell".to_string();
        let source = r#"
module local_refs

action first() -> u64
where
    let value = 1
    return value

action second() -> u64
where
    let value = 2
    return value
"#;
        server.open_document(uri.clone(), source.to_string());
        let first_value_use = source.find("return value").expect("first return value") + "return ".len();
        let refs = server.find_references(&uri, offset_to_position(source, first_value_use));
        assert_eq!(refs.len(), 2, "local reference search should include only first action let/use: {:?}", refs);
        assert!(
            refs.iter().all(|location| location.range.start.line < 8),
            "second action references leaked into first action: {:?}",
            refs
        );
    }

    #[test]
    fn test_hover() {
        let mut server = LspServer::new();
        let uri = "file:///hover.cell".to_string();
        let source = "module hover;\n\naction demo(x: u64)->u64\nwhere\n    x\n";
        server.open_document(uri.clone(), source.to_string());

        let hover = server.hover(&uri, Position { line: 2, character: 7 }).expect("hover");
        assert!(hover.contents.contains("action demo"));
    }

    #[test]
    fn test_action_hover_includes_lowering_metadata() {
        let mut server = LspServer::new();
        let uri = "file:///metadata_hover.cell".to_string();
        let source = r#"
module metadata_hover

shared Config has store, read_ref {
    threshold: u64,
}

resource Token has store, create, consume, replace, burn, relock {
    amount: u64,
}

action update(amount: u64) -> u64
where
    let cfg = read_ref<Config>()
    let token = create Token { amount: amount }
    consume token
    return cfg.threshold
"#;
        server.open_document(uri.clone(), source.to_string());

        let hover = server.hover(&uri, Position { line: 11, character: 8 }).expect("hover");
        assert!(hover.contents.contains("Lowering metadata"));
        assert!(hover.contents.contains("ELF compatible: `true`"));
        // This action uses read_ref + consume, which require CKB runtime,
        // so standalone runner is not compatible.
        assert!(hover.contents.contains("Standalone runner compatible: `false`"));
        assert!(hover.contents.contains("Fail-closed runtime features: `none"));
        assert!(hover.contents.contains("CKB runtime features: `consume-input-cell, read-cell-dep, verify-output-cell`"));
        assert!(hover.contents.contains("consume:Input#0"));
        assert!(hover.contents.contains("read_ref:CellDep#0"));
        assert!(hover.contents.contains("create:Output#0"));
        assert!(hover.contents.contains("Verifier obligations"));
        assert!(hover.contents.contains("cell-access:consume:Input#0 (ckb-runtime)"));
    }

    #[test]
    fn test_receipt_hover_includes_flow_metadata() {
        let mut server = LspServer::new();
        let uri = "file:///flow_hover.cell".to_string();
        let source = r#"
module flow_hover

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action activate(ticket: Ticket) -> active_ticket: Ticket
    transition ticket.state: Created -> active_ticket.state: Active
where
    let active = Ticket::Active
    require active_ticket.state == active
    require active_ticket.id == ticket.id
"#;
        server.open_document(uri.clone(), source.to_string());

        let offset = source.find("Ticket has").expect("receipt name");
        let hover = server.hover(&uri, offset_to_position(source, offset)).expect("hover");
        assert!(hover.contents.contains("receipt Ticket"));
        assert!(hover.contents.contains("Flow metadata"));
        assert!(hover.contents.contains("States: `Created -> Active`"));
        assert!(hover.contents.contains("Created[0] -> Active[1]"));
    }

    #[test]
    fn test_lowering_diagnostics_warn_for_fail_closed_runtime_actions() {
        let mut server = LspServer::new();
        let uri = "file:///metadata_diagnostic.cell".to_string();
        let source = r#"
module metadata_diagnostic

shared Config has store, read_ref {
    threshold: u64,
}

resource Token has store, create, consume, replace, burn, relock {
    amount: u64,
}

action update(amount: u64) -> u64
where
    let cfg = read_ref<Config>()
    let token = create Token { amount: amount }
    consume token
    return cfg.threshold
"#;
        server.open_document(uri.clone(), source.to_string());

        let diagnostics = server.get_diagnostics(&uri);
        // consume/create/read_ref now have real verifier lowering, so this program
        // is ELF-compatible and no longer triggers a lowering diagnostic.
        let lowering_warning = diagnostics.iter().find(|diagnostic| diagnostic.source == "cellscript-lowering");
        assert!(lowering_warning.is_none(), "consume/create/read_ref should not produce lowering warning: {:?}", lowering_warning);
    }

    #[test]
    fn test_code_actions_for_lowering_diagnostics() {
        let mut server = LspServer::new();
        let uri = "file:///metadata_action.cell".to_string();
        let source = r#"
module metadata_action

resource NFT has store, create {
    token_id: u64,
}

action use_collection() -> Vec<NFT>
where
    let mut items = Vec::new()
    let nft = create NFT {
        token_id: 1,
    }
    items.push(nft)
    return items
"#;
        server.open_document(uri.clone(), source.to_string());

        let actions =
            server.code_action(&uri, Range { start: Position { line: 0, character: 0 }, end: Position { line: 20, character: 0 } });
        assert!(actions.iter().any(|action| action.title.contains("cellc metadata")));
        assert!(actions.iter().any(|action| action.title.contains("riscv64-asm")));
        assert!(actions.iter().all(|action| action.edit.is_none()));
    }

    #[test]
    fn test_format_document() {
        let mut server = LspServer::new();
        let uri = "file:///fmt.cell".to_string();
        let source = "module fmt\naction demo(x:u64)->u64\nwhere\nx\n";
        server.open_document(uri.clone(), source.to_string());

        let edits = server.format_document(&uri);
        assert_eq!(edits.len(), 1);
        assert!(edits[0].new_text.contains("action demo(x: u64) -> u64\nwhere"));
    }

    #[test]
    fn test_workspace_goto_definition_across_modules() {
        let temp = tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cell.toml"), "[package]\nentry = \"src/main.cell\"\n").unwrap();
        std::fs::write(root.join("src/types.cell"), "module demo::types\n\nresource Token {\n    amount: u64,\n}\n").unwrap();
        let main_source =
            "module demo::main\n\nuse demo::types::Token\n\naction inspect(token: Token) -> u64\nwhere\n    token.amount\n";
        let main_path = root.join("src/main.cell");
        std::fs::write(&main_path, main_source).unwrap();

        let mut server = LspServer::new();
        let main_uri = utf8_path_to_file_uri(&main_path);
        server.open_document(main_uri.clone(), main_source.to_string());

        let definition = server.goto_definition(&main_uri, Position { line: 4, character: 22 }).expect("cross-module definition");
        assert!(definition.uri.ends_with("/src/types.cell"));
        assert_eq!(definition.range.start.line, 2);
    }

    #[test]
    fn test_workspace_diagnostics_check_imported_type_id_collisions() {
        let temp = tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cell.toml"), "[package]\nentry = \"src/main.cell\"\n").unwrap();
        std::fs::write(
            root.join("src/left.cell"),
            r#"module demo::left

#[type_id("demo::asset::Token:v1")]
resource TokenA has store {
    amount: u64
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("src/right.cell"),
            r#"module demo::right

#[type_id("demo::asset::Token:v1")]
resource TokenB has store {
    amount: u64
}
"#,
        )
        .unwrap();
        let main_source = r#"module demo::main

use demo::left::TokenA
use demo::right::TokenB

action inspect(token: TokenA) -> u64
where
    token.amount
"#;
        let main_path = root.join("src/main.cell");
        std::fs::write(&main_path, main_source).unwrap();

        let mut server = LspServer::new();
        let main_uri = utf8_path_to_file_uri(&main_path);
        server.open_document(main_uri.clone(), main_source.to_string());

        let diagnostics = server.get_diagnostics(&main_uri);
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.message.contains("duplicate type_id 'demo::asset::Token:v1'")),
            "expected imported type_id collision diagnostic, got {:?}",
            diagnostics
        );
    }

    #[test]
    fn lsp_loads_sibling_modules_for_standalone_example_imports() {
        let root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let launch_path = root.join("examples/launch.cell");
        let launch_source = std::fs::read_to_string(&launch_path).expect("launch example source");
        let launch_uri = utf8_path_to_file_uri(&launch_path);

        let mut server = LspServer::new();
        server.open_document(launch_uri.clone(), launch_source);

        let diagnostics = server.get_diagnostics(&launch_uri);
        assert!(diagnostics.is_empty(), "unexpected launch example diagnostics: {:?}", diagnostics);
    }

    #[test]
    fn test_workspace_references_across_modules() {
        let temp = tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("Cell.toml"), "[package]\nentry = \"src/main.cell\"\n").unwrap();
        let types_source = "module demo::types\n\nresource Token {\n    amount: u64,\n}\n";
        let types_path = root.join("src/types.cell");
        std::fs::write(&types_path, types_source).unwrap();
        let main_source =
            "module demo::main\n\nuse demo::types::Token\n\naction inspect(token: Token) -> u64\nwhere\n    token.amount\n";
        std::fs::write(root.join("src/main.cell"), main_source).unwrap();

        let mut server = LspServer::new();
        let types_uri = utf8_path_to_file_uri(&types_path);
        server.open_document(types_uri.clone(), types_source.to_string());

        let refs = server.find_references(&types_uri, Position { line: 2, character: 10 });
        assert!(refs.iter().any(|location| location.uri.ends_with("/src/types.cell")));
        assert!(refs.iter().any(|location| location.uri.ends_with("/src/main.cell")));
        assert!(refs.len() >= 3);
    }

    #[test]
    fn test_workspace_rename_is_disabled_until_symbol_scoped() {
        let mut server = LspServer::new();
        let uri = "file:///rename.cell".to_string();
        let source = "module demo\n\nresource Token {\n    amount: u64,\n}\n";
        server.open_document(uri.clone(), source.to_string());

        let changes = server.rename(&uri, Position { line: 2, character: 10 }, "Asset".to_string());

        assert!(changes.is_empty(), "rename must fail closed until symbol-scoped edits are implemented");
    }
}
