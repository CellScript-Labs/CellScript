//! Tower-LSP transport layer for the CellScript language server.
//!
//! This module wraps the in-process `LspServer` behind the `tower_lsp::LanguageServer`
//! trait so that `cellc --lsp` can act as a full JSON-RPC language server.

use super::session::{self, SharedSession};
use crate::lsp;
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability, CodeActionResponse,
    CodeDescription, CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse, Diagnostic,
    DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams, DocumentSymbolParams, DocumentSymbolResponse, Documentation,
    FoldingRange, FoldingRangeKind, FoldingRangeParams, FoldingRangeProviderCapability, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
    InsertTextFormat, Location, MarkupContent, MarkupKind, MessageType, NumberOrString, OneOf, ParameterInformation, ParameterLabel,
    Position, Range, ReferenceParams, RenameParams, SelectionRange, SelectionRangeParams, SelectionRangeProviderCapability,
    ServerCapabilities, ServerInfo, SignatureHelp, SignatureHelpOptions, SignatureHelpParams, SignatureInformation, SymbolInformation,
    SymbolKind, SymbolTag, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, TextEdit, Url,
    WorkDoneProgressOptions, WorkspaceEdit,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

struct CellScriptBackend {
    client: Client,
    state: SharedSession,
    dynamic_watches: AtomicBool,
}

impl CellScriptBackend {
    fn new(client: Client) -> Self {
        Self { client, state: Arc::default(), dynamic_watches: AtomicBool::new(false) }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for CellScriptBackend {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        self.dynamic_watches.store(
            params
                .capabilities
                .workspace
                .as_ref()
                .and_then(|workspace| workspace.did_change_watched_files.as_ref())
                .and_then(|capability| capability.dynamic_registration)
                == Some(true),
            Ordering::Relaxed,
        );
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: None,
                    will_save_wait_until: None,
                    save: None,
                })),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: None,
                    trigger_characters: Some(vec![".".to_string(), ":".to_string()]),
                    all_commit_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
                    completion_item: None,
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                    retrigger_characters: None,
                    work_done_progress_options: WorkDoneProgressOptions { work_done_progress: None },
                }),
                document_highlight_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                ..ServerCapabilities::default()
            },
            server_info: Some(ServerInfo {
                name: "cellscript-language-server".to_string(),
                version: Some(crate::VERSION.to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client.log_message(MessageType::INFO, "CellScript language server initialized").await;
        if self.dynamic_watches.load(Ordering::Relaxed) {
            let _ = self
                .client
                .register_capability(vec![tower_lsp::lsp_types::Registration {
                    id: "cellscript-source-revisions".into(),
                    method: "workspace/didChangeWatchedFiles".into(),
                    register_options: Some(serde_json::json!({ "watchers": [
                    { "globPattern": "**/*.cell" }, { "globPattern": "**/Cell.toml" }, { "globPattern": "**/Cell.lock" }
                ] })),
                }])
                .await;
        }
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    // ---- document sync ----

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let document = params.text_document;
        self.state.lock().unwrap().open(document.uri.to_string(), document.version, document.text);
        session::schedule(&self.state, &self.client);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let changed = self.state.lock().unwrap().change(
            params.text_document.uri.as_str(),
            params.text_document.version,
            params
                .content_changes
                .into_iter()
                .map(|change| lsp::TextDocumentContentChangeEvent {
                    range: change.range.map(convert_range_back),
                    range_length: change.range_length,
                    text: change.text,
                })
                .collect(),
        );
        if changed {
            session::schedule(&self.state, &self.client);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.state.lock().unwrap().close(params.text_document.uri.as_str());
        self.client.publish_diagnostics(params.text_document.uri, Vec::new(), None).await;
        session::schedule(&self.state, &self.client);
    }

    async fn did_change_watched_files(&self, _: tower_lsp::lsp_types::DidChangeWatchedFilesParams) {
        self.state.lock().unwrap().dependencies_changed();
        session::schedule(&self.state, &self.client);
    }

    // ---- language features ----

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri_str = params.text_document_position.text_document.uri.to_string();
        let position = convert_position_back(params.text_document_position.position);
        let items = session::query(&self.state, uri_str.clone(), move |state| state.completion(&uri_str, position)).await?;
        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items.into_iter().map(convert_completion_item).collect())))
        }
    }

    async fn goto_definition(&self, params: GotoDefinitionParams) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri_str = params.text_document_position_params.text_document.uri.to_string();
        let position = convert_position_back(params.text_document_position_params.position);
        let location = session::query(&self.state, uri_str.clone(), move |state| state.goto_definition(&uri_str, position)).await?;
        Ok(location.map(convert_location).transpose()?.map(GotoDefinitionResponse::Scalar))
    }

    async fn references(&self, params: ReferenceParams) -> LspResult<Option<Vec<Location>>> {
        let uri_str = params.text_document_position.text_document.uri.to_string();
        let position = convert_position_back(params.text_document_position.position);
        let refs = session::query(&self.state, uri_str.clone(), move |state| state.find_references(&uri_str, position)).await?;
        if refs.is_empty() {
            Ok(None)
        } else {
            Ok(Some(refs.into_iter().map(convert_location).collect::<LspResult<Vec<_>>>()?))
        }
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri_str = params.text_document_position_params.text_document.uri.to_string();
        let position = convert_position_back(params.text_document_position_params.position);
        let hover = session::query(&self.state, uri_str.clone(), move |state| state.hover(&uri_str, position)).await?;
        Ok(hover.map(|h| Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: h.contents }),
            range: h.range.map(convert_range),
        }))
    }

    async fn document_symbol(&self, params: DocumentSymbolParams) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri_str = params.text_document.uri.to_string();
        let symbols = session::query(&self.state, uri_str.clone(), move |state| state.document_symbols(&uri_str)).await?;
        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Flat(symbols.into_iter().map(convert_symbol_information).collect::<LspResult<Vec<_>>>()?)))
        }
    }

    async fn rename(&self, params: RenameParams) -> LspResult<Option<WorkspaceEdit>> {
        let uri_str = params.text_document_position.text_document.uri.to_string();
        let position = convert_position_back(params.text_document_position.position);
        let changes =
            session::query(&self.state, uri_str.clone(), move |state| state.rename(&uri_str, position, params.new_name)).await?;
        if changes.is_empty() {
            Ok(None)
        } else {
            let mut lsp_changes = HashMap::new();
            for (uri, edits) in changes {
                let url = response_uri(&uri)?;
                lsp_changes.insert(url, edits.into_iter().map(convert_text_edit).collect());
            }
            Ok(Some(WorkspaceEdit { changes: Some(lsp_changes), document_changes: None, change_annotations: None }))
        }
    }

    async fn code_action(&self, params: CodeActionParams) -> LspResult<Option<CodeActionResponse>> {
        let uri_str = params.text_document.uri.to_string();
        let range = convert_range_back(params.range);
        let actions = session::query(&self.state, uri_str.clone(), move |state| state.code_action(&uri_str, range)).await?;
        if actions.is_empty() {
            Ok(None)
        } else {
            let converted = actions
                .into_iter()
                .map(|action| {
                    Ok(CodeActionOrCommand::CodeAction(CodeAction {
                        title: action.title,
                        kind: Some(CodeActionKind::from(action.kind)),
                        diagnostics: None,
                        edit: action.edit.map(convert_workspace_edit).transpose()?,
                        command: None,
                        is_preferred: None,
                        disabled: None,
                        data: None,
                    }))
                })
                .collect::<LspResult<Vec<_>>>()?;
            Ok(Some(converted))
        }
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> LspResult<Option<Vec<TextEdit>>> {
        let uri_str = params.text_document.uri.to_string();
        let edits = session::query(&self.state, uri_str.clone(), move |state| state.format_document(&uri_str)).await?;
        if edits.is_empty() {
            Ok(None)
        } else {
            Ok(Some(edits.into_iter().map(convert_text_edit).collect()))
        }
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> LspResult<Option<SignatureHelp>> {
        let uri_str = params.text_document_position_params.text_document.uri.to_string();
        let position = convert_position_back(params.text_document_position_params.position);
        let help = session::query(&self.state, uri_str.clone(), move |state| state.signature_help(&uri_str, position)).await?;
        Ok(help.map(convert_signature_help))
    }

    async fn document_highlight(&self, params: DocumentHighlightParams) -> LspResult<Option<Vec<DocumentHighlight>>> {
        let uri_str = params.text_document_position_params.text_document.uri.to_string();
        let position = convert_position_back(params.text_document_position_params.position);
        let highlights =
            session::query(&self.state, uri_str.clone(), move |state| state.document_highlight(&uri_str, position)).await?;
        if highlights.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                highlights
                    .into_iter()
                    .map(|h| DocumentHighlight {
                        range: convert_range(h.range),
                        kind: Some(match h.kind {
                            lsp::DocumentHighlightKind::Text => DocumentHighlightKind::TEXT,
                            lsp::DocumentHighlightKind::Read => DocumentHighlightKind::READ,
                            lsp::DocumentHighlightKind::Write => DocumentHighlightKind::WRITE,
                        }),
                    })
                    .collect(),
            ))
        }
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> LspResult<Option<Vec<FoldingRange>>> {
        let uri_str = params.text_document.uri.to_string();
        let ranges = session::query(&self.state, uri_str.clone(), move |state| state.folding_range(&uri_str)).await?;
        if ranges.is_empty() {
            Ok(None)
        } else {
            Ok(Some(
                ranges
                    .into_iter()
                    .map(|r| FoldingRange {
                        start_line: r.start_line,
                        start_character: r.start_character,
                        end_line: r.end_line,
                        end_character: r.end_character,
                        kind: Some(match r.kind {
                            Some(lsp::FoldingRangeKind::Comment) => FoldingRangeKind::Comment,
                            Some(lsp::FoldingRangeKind::Imports) => FoldingRangeKind::Imports,
                            Some(lsp::FoldingRangeKind::Region) => FoldingRangeKind::Region,
                            None => FoldingRangeKind::Region,
                        }),
                        collapsed_text: None,
                    })
                    .collect(),
            ))
        }
    }

    async fn selection_range(&self, params: SelectionRangeParams) -> LspResult<Option<Vec<SelectionRange>>> {
        let uri_str = params.text_document.uri.to_string();
        let results = session::query(&self.state, uri_str.clone(), move |state| {
            params
                .positions
                .into_iter()
                .filter_map(|position| state.selection_range(&uri_str, convert_position_back(position)))
                .map(convert_selection_range)
                .collect::<Vec<_>>()
        })
        .await?;
        if results.is_empty() {
            Ok(None)
        } else {
            Ok(Some(results))
        }
    }
}

// ---------------------------------------------------------------------------
// Type conversion helpers
// ---------------------------------------------------------------------------

fn convert_position(p: lsp::Position) -> Position {
    Position { line: p.line, character: p.character }
}

fn convert_position_back(p: Position) -> lsp::Position {
    lsp::Position { line: p.line, character: p.character }
}

fn convert_range(r: lsp::Range) -> Range {
    Range { start: convert_position(r.start), end: convert_position(r.end) }
}

fn convert_range_back(r: Range) -> lsp::Range {
    lsp::Range { start: convert_position_back(r.start), end: convert_position_back(r.end) }
}

pub(super) fn convert_diagnostic(d: lsp::Diagnostic) -> Diagnostic {
    Diagnostic {
        range: convert_range(d.range),
        severity: Some(match d.severity {
            lsp::DiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
            lsp::DiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
            lsp::DiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
            lsp::DiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
        }),
        code: d.code.map(NumberOrString::String),
        code_description: d.code_description.and_then(|href| Url::parse(&href).ok()).map(|href| CodeDescription { href }),
        source: Some(d.source),
        message: d.message,
        related_information: None,
        tags: None,
        data: None,
    }
}

fn convert_completion_item(item: lsp::CompletionItem) -> CompletionItem {
    CompletionItem {
        label: item.label,
        kind: Some(match item.kind {
            lsp::CompletionItemKind::Text => CompletionItemKind::TEXT,
            lsp::CompletionItemKind::Method => CompletionItemKind::METHOD,
            lsp::CompletionItemKind::Function => CompletionItemKind::FUNCTION,
            lsp::CompletionItemKind::Constructor => CompletionItemKind::CONSTRUCTOR,
            lsp::CompletionItemKind::Field => CompletionItemKind::FIELD,
            lsp::CompletionItemKind::Variable => CompletionItemKind::VARIABLE,
            lsp::CompletionItemKind::Class => CompletionItemKind::CLASS,
            lsp::CompletionItemKind::Interface => CompletionItemKind::INTERFACE,
            lsp::CompletionItemKind::Module => CompletionItemKind::MODULE,
            lsp::CompletionItemKind::Property => CompletionItemKind::PROPERTY,
            lsp::CompletionItemKind::Unit => CompletionItemKind::UNIT,
            lsp::CompletionItemKind::Value => CompletionItemKind::VALUE,
            lsp::CompletionItemKind::Enum => CompletionItemKind::ENUM,
            lsp::CompletionItemKind::Keyword => CompletionItemKind::KEYWORD,
            lsp::CompletionItemKind::Snippet => CompletionItemKind::SNIPPET,
            lsp::CompletionItemKind::Color => CompletionItemKind::COLOR,
            lsp::CompletionItemKind::File => CompletionItemKind::FILE,
            lsp::CompletionItemKind::Reference => CompletionItemKind::REFERENCE,
            lsp::CompletionItemKind::Folder => CompletionItemKind::FOLDER,
            lsp::CompletionItemKind::EnumMember => CompletionItemKind::ENUM_MEMBER,
            lsp::CompletionItemKind::Constant => CompletionItemKind::CONSTANT,
            lsp::CompletionItemKind::Struct => CompletionItemKind::STRUCT,
            lsp::CompletionItemKind::Event => CompletionItemKind::EVENT,
            lsp::CompletionItemKind::Operator => CompletionItemKind::OPERATOR,
            lsp::CompletionItemKind::TypeParameter => CompletionItemKind::TYPE_PARAMETER,
        }),
        detail: item.detail,
        documentation: item.documentation.map(Documentation::String),
        insert_text: item.insert_text,
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        ..CompletionItem::default()
    }
}

fn response_uri(uri: &str) -> LspResult<Url> {
    Url::parse(uri).or_else(|_| {
        Url::from_file_path(uri)
            .map_err(|_| tower_lsp::jsonrpc::Error::invalid_params(format!("invalid document location URI: {uri:?}")))
    })
}

fn convert_workspace_edit(edit: lsp::WorkspaceEdit) -> LspResult<WorkspaceEdit> {
    let changes = edit
        .changes
        .into_iter()
        .map(|(uri, edits)| Ok((response_uri(&uri)?, edits.into_iter().map(convert_text_edit).collect())))
        .collect::<LspResult<HashMap<_, _>>>()?;
    Ok(WorkspaceEdit { changes: Some(changes), document_changes: None, change_annotations: None })
}

fn convert_location(loc: lsp::Location) -> LspResult<Location> {
    Ok(Location { uri: response_uri(&loc.uri)?, range: convert_range(loc.range) })
}

fn convert_symbol_information(sym: lsp::SymbolInformation) -> LspResult<SymbolInformation> {
    let kind = match sym.kind {
        lsp::SymbolKind::File => SymbolKind::FILE,
        lsp::SymbolKind::Module => SymbolKind::MODULE,
        lsp::SymbolKind::Namespace => SymbolKind::NAMESPACE,
        lsp::SymbolKind::Package => SymbolKind::PACKAGE,
        lsp::SymbolKind::Class => SymbolKind::CLASS,
        lsp::SymbolKind::Method => SymbolKind::METHOD,
        lsp::SymbolKind::Property => SymbolKind::PROPERTY,
        lsp::SymbolKind::Field => SymbolKind::FIELD,
        lsp::SymbolKind::Constructor => SymbolKind::CONSTRUCTOR,
        lsp::SymbolKind::Enum => SymbolKind::ENUM,
        lsp::SymbolKind::Interface => SymbolKind::INTERFACE,
        lsp::SymbolKind::Function => SymbolKind::FUNCTION,
        lsp::SymbolKind::Variable => SymbolKind::VARIABLE,
        lsp::SymbolKind::Constant => SymbolKind::CONSTANT,
        lsp::SymbolKind::String => SymbolKind::STRING,
        lsp::SymbolKind::Number => SymbolKind::NUMBER,
        lsp::SymbolKind::Boolean => SymbolKind::BOOLEAN,
        lsp::SymbolKind::Array => SymbolKind::ARRAY,
        lsp::SymbolKind::Object => SymbolKind::OBJECT,
        lsp::SymbolKind::Key => SymbolKind::KEY,
        lsp::SymbolKind::Null => SymbolKind::NULL,
        lsp::SymbolKind::EnumMember => SymbolKind::ENUM_MEMBER,
        lsp::SymbolKind::Struct => SymbolKind::STRUCT,
        lsp::SymbolKind::Event => SymbolKind::EVENT,
        lsp::SymbolKind::Operator => SymbolKind::OPERATOR,
        lsp::SymbolKind::TypeParameter => SymbolKind::TYPE_PARAMETER,
    };
    serde_json::from_value(serde_json::json!({
        "name": sym.name,
        "kind": kind,
        "location": convert_location(sym.location)?,
        "containerName": sym.container_name,
        "tags": Option::<Vec<SymbolTag>>::None,
    }))
    .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())
}

fn convert_text_edit(edit: lsp::TextEdit) -> TextEdit {
    TextEdit { range: convert_range(edit.range), new_text: edit.new_text }
}

fn convert_signature_help(help: lsp::SignatureHelp) -> SignatureHelp {
    SignatureHelp {
        signatures: help
            .signatures
            .into_iter()
            .map(|sig| SignatureInformation {
                label: sig.label,
                documentation: sig.documentation.map(Documentation::String),
                parameters: Some(
                    sig.parameters
                        .into_iter()
                        .map(|p| ParameterInformation {
                            label: ParameterLabel::Simple(match p.label {
                                lsp::ParameterLabel::Simple(s) => s,
                                lsp::ParameterLabel::Labelled { left, right } => {
                                    format!("{}:{}", left, right)
                                }
                            }),
                            documentation: p.documentation.map(Documentation::String),
                        })
                        .collect(),
                ),
                active_parameter: None,
            })
            .collect(),
        active_signature: help.active_signature,
        active_parameter: help.active_parameter,
    }
}

fn convert_selection_range(range: lsp::SelectionRange) -> SelectionRange {
    SelectionRange { range: convert_range(range.range), parent: range.parent.map(|b| Box::new(convert_selection_range(*b))) }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub async fn run_lsp_server() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(CellScriptBackend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Blocking entry point for use from synchronous `main`.
pub fn run_lsp_server_blocking() -> crate::error::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|error| {
        crate::error::CompileError::without_span(format!("failed to build Tokio runtime for LSP: {error}"))
            .with_code("LSP0001")
            .with_category(crate::error::CompileErrorCategory::Io)
            .with_source(error)
    })?;
    runtime.block_on(run_lsp_server());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(uri: &str) -> lsp::Location {
        lsp::Location {
            uri: uri.into(),
            range: lsp::Range { start: lsp::Position { line: 1, character: 2 }, end: lsp::Position { line: 1, character: 5 } },
        }
    }

    #[test]
    fn response_locations_preserve_valid_uris_and_reject_relative_paths() {
        for uri in ["file:///workspace/main.cell", "untitled:buffer", "/workspace/main.cell"] {
            let converted = convert_location(location(uri)).unwrap();
            assert_eq!(converted.range.start, Position::new(1, 2));
            assert_eq!(converted.range.end, Position::new(1, 5));
            assert_eq!(converted.uri, response_uri(uri).unwrap());
        }
        let error = convert_location(location("relative/main.cell")).unwrap_err();
        assert_eq!(error.code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
    }

    #[test]
    fn symbols_propagate_invalid_locations_without_unwinding() {
        for uri in ["file:///workspace/main.cell", "relative/main.cell"] {
            let converted = convert_symbol_information(lsp::SymbolInformation {
                name: "inspect".into(),
                kind: lsp::SymbolKind::Function,
                location: location(uri),
                container_name: Some("module".into()),
            });
            if uri.starts_with("file:") {
                let converted = converted.unwrap();
                assert_eq!(converted.name, "inspect");
                assert_eq!(converted.kind, SymbolKind::FUNCTION);
                assert_eq!(converted.container_name.as_deref(), Some("module"));
            } else {
                assert_eq!(converted.unwrap_err().code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
            }
        }
    }

    #[test]
    fn workspace_edits_reject_invalid_locations_instead_of_returning_partial_edits() {
        let edit = lsp::TextEdit { range: location("file:///workspace/main.cell").range, new_text: "renamed".into() };
        let valid = lsp::WorkspaceEdit { changes: HashMap::from([("file:///workspace/main.cell".into(), vec![edit.clone()])]) };
        let converted = convert_workspace_edit(valid).unwrap();
        assert_eq!(converted.changes.unwrap()[&response_uri("file:///workspace/main.cell").unwrap()][0].new_text, "renamed");
        let invalid = lsp::WorkspaceEdit {
            changes: HashMap::from([
                ("file:///workspace/main.cell".into(), vec![edit.clone()]),
                ("relative/other.cell".into(), vec![edit]),
            ]),
        };
        assert_eq!(convert_workspace_edit(invalid).unwrap_err().code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
    }
}
