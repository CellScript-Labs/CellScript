use crate::ast::*;
use crate::error::{CompileError, Span};
use crate::lexer::token::TokenKind;
use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

mod completion;
mod diagnostics;
mod document_ranges;
mod edits;
mod hover;
mod hover_display;
mod navigation;
mod positions;
mod signature;
mod symbols;
mod uris;

use diagnostics::{diagnostic_belongs_to_uri, diagnostic_from_error, lowering_diagnostics};
use positions::{offset_to_position, position_le, position_to_offset, word_at_offset, word_occurrences};
use uris::{file_uri_to_utf8_path, same_workspace_path, utf8_path_to_file_uri};

mod cell_views;
mod collection_budgets;
mod view_index;
mod workspace;

#[cfg(all(feature = "lsp", not(feature = "wasm")))]
pub mod server;
#[cfg(all(feature = "lsp", not(feature = "wasm")))]
mod session;

#[derive(Clone)]
pub struct LspServer {
    documents: HashMap<String, Arc<String>>,
    ast_cache: HashMap<String, Arc<Module>>,
    ast_editions: HashMap<String, crate::CellScriptEdition>,
    diagnostics: HashMap<String, Arc<[Diagnostic]>>,
    edition_overrides: HashMap<String, crate::CellScriptEdition>,
    artifact_name: Option<Arc<str>>,
    view_cache: view_index::ViewCache,
    observed_inputs: Option<Arc<std::collections::BTreeMap<String, Arc<workspace::WorkspaceInputs>>>>,
    source_groups: workspace::GroupCache,
    #[cfg(test)]
    read_stats: Arc<workspace::ReadStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub code_description: Option<String>,
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
        Self {
            documents: HashMap::new(),
            ast_cache: HashMap::new(),
            ast_editions: HashMap::new(),
            diagnostics: HashMap::new(),
            edition_overrides: HashMap::new(),
            artifact_name: None,
            view_cache: Default::default(),
            observed_inputs: None,
            source_groups: Default::default(),
            #[cfg(test)]
            read_stats: Arc::default(),
        }
    }

    pub fn open_document(&mut self, uri: String, content: String) {
        self.edition_overrides.remove(&uri);
        self.store_document(uri, content);
    }

    /// Open a virtual document under an explicit source edition.
    ///
    /// Native LSP clients normally resolve the edition from `Cell.toml`.
    /// Browser and other in-memory clients have no manifest path, so they use
    /// this entry point instead of silently falling back to the current edition.
    pub fn open_document_with_edition(&mut self, uri: String, content: String, edition: crate::CellScriptEdition) {
        self.edition_overrides.insert(uri.clone(), edition);
        self.store_document(uri, content);
    }

    pub fn update_document(&mut self, uri: String, content: String) {
        self.store_document(uri, content);
    }

    /// Apply incremental text changes to a document and re-parse.
    ///
    /// If any change has `range == None`, the entire document is replaced with that change's text.
    /// Otherwise, each change's text is spliced into the current document at the given range.
    pub fn update_document_incremental(&mut self, uri: &str, changes: Vec<TextDocumentContentChangeEvent>) {
        self.apply_buffer_changes(uri, changes);
        self.analyze_document(uri);
    }

    /// Transport mutation only: no parsing, manifest lookup or compilation.
    pub(super) fn apply_buffer_changes(&mut self, uri: &str, changes: Vec<TextDocumentContentChangeEvent>) {
        let Some(mut content) = self.documents.get(uri).map(|source| source.as_ref().clone()) else { return };
        for change in changes {
            content = match change.range {
                None => change.text,
                Some(range) => apply_incremental_change(&content, range, &change.text),
            };
            if content.len() > crate::MAX_SOURCE_BYTES {
                self.reject_oversized_document(uri);
                return;
            }
        }
        self.replace_buffer(uri.into(), content);
    }

    pub(super) fn replace_buffer(&mut self, uri: String, content: String) {
        self.observed_inputs = None;
        if content.len() > crate::MAX_SOURCE_BYTES {
            self.reject_oversized_document(&uri);
            return;
        }
        self.ast_cache.remove(&uri);
        self.diagnostics.remove(&uri);
        self.documents.insert(uri, content.into());
    }

    fn analyze_document(&mut self, uri: &str) {
        if let Some(content) = self.documents.get(uri).cloned() {
            self.parse_document(uri, &content);
        }
    }

    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn analyze_documents(&mut self) -> Vec<(String, [u8; 32])> {
        let revision = self.prepare_diagnostic_snapshot();
        self.analyze_captured_documents();
        self.observed_inputs = None;
        revision
    }

    #[cfg(all(feature = "lsp", not(feature = "wasm")))]
    pub(super) fn analyze_captured_documents(&mut self) {
        debug_assert!(self.observed_inputs.is_some());
        let mut uris = self.documents.keys().cloned().collect::<Vec<_>>();
        uris.sort();
        for uri in uris {
            self.analyze_document(&uri);
            // Retain the shared parsed group through the existing view cache;
            // later queries still establish a fresh fingerprint before reuse.
            let _ = self.view_index(&uri);
        }
    }

    /// Navigation parses its target; dependencies use the same workspace overlay
    /// loader as compilation. Other projects' buffers do not delay the request.
    #[cfg(any(test, all(feature = "lsp", not(feature = "wasm"))))]
    pub(super) fn prepare_navigation_for(&mut self, uri: &str) {
        let edition = self.document_edition(uri);
        if self.ast_editions.get(uri) != Some(&edition) {
            self.ast_cache.remove(uri);
        }
        if !self.ast_cache.contains_key(uri)
            && let Some(source) = self.documents.get(uri)
            && let Ok(module) = crate::frontend::parse_diagnostics(source, edition)
        {
            self.ast_cache.insert(uri.into(), module.into());
            self.ast_editions.insert(uri.into(), edition);
        }
    }

    #[cfg(test)]
    pub(super) fn prepare_navigation(&mut self) {
        for uri in self.documents.keys().cloned().collect::<Vec<_>>() {
            self.prepare_navigation_for(&uri);
        }
    }

    pub fn close_document(&mut self, uri: &str) {
        self.observed_inputs = None;
        self.documents.remove(uri);
        self.ast_cache.remove(uri);
        self.ast_editions.remove(uri);
        self.diagnostics.remove(uri);
        self.edition_overrides.remove(uri);
        if let Ok(mut cache) = self.view_cache.lock() {
            cache.remove(uri);
        }
    }

    fn store_document(&mut self, uri: String, content: String) {
        self.replace_buffer(uri.clone(), content);
        self.analyze_document(&uri);
    }

    fn reject_oversized_document(&mut self, uri: &str) {
        self.observed_inputs = None;
        self.documents.remove(uri);
        self.ast_cache.remove(uri);
        self.ast_editions.remove(uri);
        self.diagnostics.insert(
            uri.to_string(),
            vec![Diagnostic {
                range: Range { start: Position { line: 0, character: 0 }, end: Position { line: 0, character: 0 } },
                severity: DiagnosticSeverity::Error,
                code: None,
                code_description: None,
                message: format!("source exceeds the {} byte compiler limit", crate::MAX_SOURCE_BYTES),
                source: "cellscript".to_string(),
            }]
            .into(),
        );
    }

    fn parse_document(&mut self, uri: &str, content: &str) {
        self.ast_cache.remove(uri);
        self.ast_editions.remove(uri);
        let captured = self.observed_inputs.as_ref().and_then(|inputs| inputs.get(uri)).cloned();
        let edition = self.document_edition(uri);
        let parsed = captured
            .as_ref()
            .and_then(|inputs| inputs.entry_ast())
            .unwrap_or_else(|| crate::frontend::parse_diagnostics(content, edition).map(Arc::new));
        let ast = match parsed {
            Ok(ast) => ast,
            Err(errors) => {
                self.diagnostics.insert(
                    uri.to_string(),
                    errors.iter().map(|error| diagnostic_from_error(content, error)).collect::<Vec<_>>().into(),
                );
                return;
            }
        };

        self.ast_cache.insert(uri.to_string(), ast.clone());
        self.ast_editions.insert(uri.into(), edition);
        let inputs = captured.unwrap_or_else(|| workspace::WorkspaceInputs::capture(self, uri));
        let report = inputs.diagnostics();
        let mut diagnostics = report
            .diagnostics
            .iter()
            .filter(|error| diagnostic_belongs_to_uri(error, inputs.entry.as_ref()))
            .map(|error| diagnostic_from_error(content, error))
            .collect::<Vec<_>>();
        if let Some(metadata) = report.metadata.as_ref() {
            diagnostics.extend(lowering_diagnostics(content, &ast, metadata));
        }
        self.diagnostics.insert(uri.to_string(), diagnostics.into());
    }

    fn document_edition(&self, uri: &str) -> crate::CellScriptEdition {
        if let Some(inputs) = self.observed_inputs.as_ref().and_then(|inputs| inputs.get(uri)) {
            return inputs.edition;
        }
        self.edition_overrides.get(uri).copied().unwrap_or_else(|| {
            file_uri_to_utf8_path(uri).as_ref().and_then(|path| crate::source_edition(path).ok()).unwrap_or(crate::CURRENT_EDITION)
        })
    }

    pub fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        self.diagnostics.get(uri).map(|diagnostics| diagnostics.to_vec()).unwrap_or_default()
    }

    fn symbol_at_position(&self, uri: &str, position: Position) -> Option<String> {
        let content = self.documents.get(uri)?;
        let offset = position_to_offset(content, position)?;
        word_at_offset(content, offset)
    }

    fn find_top_level_symbol(&self, uri: &str, symbol: &str) -> Option<Location> {
        if let (Some(ast), Some(source)) = (self.ast_cache.get(uri), self.documents.get(uri))
            && let Some(location) = ast.items.iter().find_map(|item| {
                let name = item_name(item)?;
                if name == symbol {
                    Some(Location { uri: uri.to_string(), range: item_name_range(source, item, name) })
                } else {
                    None
                }
            })
        {
            return Some(location);
        }

        for module in self.workspace_modules(uri).iter() {
            if let Some(location) = module.ast.items.iter().find_map(|item| {
                let name = item_name(item)?;
                if name == symbol {
                    Some(Location { uri: utf8_path_to_file_uri(&module.path), range: item_name_range(&module.source, item, name) })
                } else {
                    None
                }
            }) {
                return Some(location);
            }
        }

        None
    }

    fn workspace_modules(&self, uri: &str) -> Arc<Vec<crate::LoadedModule>> {
        let Some(index) = self.view_index(uri) else { return Arc::default() };
        let mut modules = index.modules.clone();
        if let Some(path) = file_uri_to_utf8_path(uri)
            && !modules.iter().any(|module| same_workspace_path(&module.path, &path))
            && let (Some(content), Some(ast)) = (self.documents.get(uri), self.ast_cache.get(uri))
        {
            Arc::make_mut(&mut modules).push(crate::LoadedModule {
                path,
                source: content.as_ref().clone(),
                ast: ast.as_ref().clone(),
                edition: self.document_edition(uri),
            });
        }
        modules
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
    let Some(start_offset) = position_to_offset(content, range.start) else {
        return content.to_string();
    };
    let Some(end_offset) = position_to_offset(content, range.end) else {
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
    let start = offset_to_position(source, span.start.min(source.len()));
    let end = offset_to_position(source, span.end.min(source.len()));
    Range { start, end }
}

fn item_name(item: &Item) -> Option<&str> {
    match item {
        Item::DataShape(def) => Some(&def.name),
        Item::Collections(def) => Some(&def.name),
        Item::CellRoles(def) => Some(&def.name),
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
        Item::DataShape(def) => def.span,
        Item::Collections(def) => def.span,
        Item::CellRoles(def) => def.span,
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

fn item_name_range(source: &str, item: &Item, name: &str) -> Range {
    let span = item_span(item);
    let start = span.start.min(source.len());
    let end = span.end.min(source.len()).max(start);
    let slice = &source[start..end];
    if let Some((relative_start, relative_end)) = word_occurrences(slice, name).into_iter().next() {
        return Range {
            start: offset_to_position(source, start + relative_start),
            end: offset_to_position(source, start + relative_end),
        };
    }
    span_to_range(source, span)
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Let(s) => s.span,
        Stmt::Return(s) => s.span,
        Stmt::If(s) => s.span,
        Stmt::For(s) => s.span,
        Stmt::While(s) => s.span,
        Stmt::Break(s) | Stmt::Continue(s) => s.span,
        Stmt::Borrow(s) => s.span,
        Stmt::Expr(_) | Stmt::Discard(_) => Span::default(),
    }
}

fn type_to_string(ty: &Type) -> String {
    ty.source_syntax()
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

#[cfg(test)]
mod tests;
