//! Browser-facing WASM bindings for the CellScript compiler.
//!
//! This crate exposes the pure in-memory compile path
//! (`lex -> parse -> types -> flow -> ir -> metadata`) to JavaScript
//! via `wasm-bindgen`. It does NOT expose the ELF codegen path in v1
//! (that would inflate the bundle beyond the 600KB budget and is
//! tracked as RFC path B / v2).
//!
//! The single exported function `compile_metadata_json` takes source
//! text and an optional target profile, and returns a JSON string.
//! On success the string is the serialized `CompileMetadata`; on
//! failure it is `{"error": "..."}` so the playground can parse it
//! uniformly and render diagnostics.

use cellscript::error::{CompileError, Span};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct CompileDiagnosticRange {
    start: CompileDiagnosticPosition,
    end: CompileDiagnosticPosition,
}

#[derive(Serialize)]
struct CompileDiagnosticPosition {
    line: usize,
    column: usize,
    offset: usize,
}

#[derive(Serialize)]
struct CompileDiagnostic {
    message: String,
    severity: &'static str,
    code: Option<String>,
    file: Option<String>,
    range: Option<CompileDiagnosticRange>,
}

#[derive(Deserialize)]
struct CompileSourceInput {
    path: String,
    source: String,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Serialize)]
struct CompileDiagnosticResult<T: Serialize> {
    metadata: Option<T>,
    diagnostics: Vec<CompileDiagnostic>,
}

#[derive(Serialize)]
struct LanguageServiceResult {
    completions: Vec<cellscript::lsp::CompletionItem>,
    hover: Option<cellscript::lsp::Hover>,
    definition: Option<cellscript::lsp::Location>,
    diagnostics: Vec<cellscript::lsp::Diagnostic>,
}

/// Compile CellScript source to metadata JSON (path A, no ELF).
///
/// Returns a JSON string. On success this is the serialized
/// `CompileMetadata` (module, types, actions with effect_class /
/// consume_set / create_set / estimated_cycles, etc.). On error it
/// is `{"error": "<message>"}`.
///
/// The `target` argument is optional; pass `None` for the default
/// (ckb) target profile.
#[wasm_bindgen]
pub fn compile_metadata_json(source: &str, target: Option<String>) -> String {
    match cellscript::compile_metadata(source, target) {
        Ok(metadata) => serde_json::to_string(&metadata).unwrap_or_else(|e| error_json(&format!("failed to serialize metadata: {e}"))),
        Err(e) => error_json(&e.to_string()),
    }
}

/// Compile CellScript source and return a stable result envelope for tools.
///
/// On success the response is:
/// `{ "metadata": <CompileMetadata>, "diagnostics": [] }`
///
/// On failure the response is:
/// `{ "metadata": null, "diagnostics": [{ message, severity, code, range }, ...] }`
///
/// `range` is omitted when the compiler error is not tied to a source
/// span. Offsets are UTF-8 byte offsets from the original source; line and
/// column are 1-based.
#[wasm_bindgen]
pub fn compile_metadata_json_diagnostics(source: &str, target: Option<String>) -> String {
    let report = cellscript::compile_metadata_with_diagnostics(source, target);
    let result = CompileDiagnosticResult {
        metadata: report.metadata,
        diagnostics: report.diagnostics.iter().map(|error| diagnostic_from_error(error, source)).collect(),
    };
    serde_json::to_string(&result)
        .unwrap_or_else(|e| diagnostic_error_json(&format!("failed to serialize diagnostic report: {e}"), source))
}

/// Compile a virtual multi-file source set and return metadata diagnostics.
///
/// `sources_json` must be a JSON array of `{ path, source, role? }` objects.
/// `entry_path` selects the source that should produce metadata. This is an
/// additive API; the single-source functions remain stable.
#[wasm_bindgen]
pub fn compile_metadata_json_sources(sources_json: &str, entry_path: &str, target: Option<String>) -> String {
    let inputs: Vec<CompileSourceInput> = match serde_json::from_str(sources_json) {
        Ok(inputs) => inputs,
        Err(error) => return diagnostic_error_json(&format!("failed to parse source set JSON: {error}"), ""),
    };
    let sources = inputs
        .into_iter()
        .map(|input| cellscript::InMemorySource { path: input.path, source: input.source, role: input.role })
        .collect::<Vec<_>>();
    let source_by_path = sources.iter().map(|source| (source.path.clone(), source.source.clone())).collect::<HashMap<_, _>>();
    let fallback_source = sources.iter().find(|source| source.path == entry_path).map(|source| source.source.as_str()).unwrap_or("");
    let report = cellscript::compile_sources_metadata_with_diagnostics(&sources, entry_path, target);
    let result = CompileDiagnosticResult {
        metadata: report.metadata,
        diagnostics: report
            .diagnostics
            .iter()
            .map(|error| diagnostic_from_error_for_sources(error, &source_by_path, fallback_source))
            .collect(),
    };
    serde_json::to_string(&result)
        .unwrap_or_else(|e| diagnostic_error_json(&format!("failed to serialize multi-file diagnostic report: {e}"), fallback_source))
}

/// Query the in-process CellScript language service for browser tooling.
///
/// `line` and `character` are zero-based UTF-16 positions, matching LSP.
/// The result contains completion, hover, definition and current document
/// diagnostics in one JSON payload so the playground can avoid multiple
/// WASM calls per cursor move.
#[wasm_bindgen]
pub fn language_service_json(source: &str, line: u32, character: u32) -> String {
    let uri = "file:///playground.cell";
    let position = cellscript::lsp::Position { line, character };
    let mut server = cellscript::lsp::LspServer::new();
    server.open_document(uri.to_string(), source.to_string());
    let result = LanguageServiceResult {
        completions: server.completion(uri, position),
        hover: server.hover(uri, position),
        definition: server.goto_definition(uri, position),
        diagnostics: server.get_diagnostics(uri),
    };
    serde_json::to_string(&result).unwrap_or_else(|error| {
        serde_json::json!({ "error": format!("failed to serialize language service result: {error}") }).to_string()
    })
}

/// Return the compiler version string (e.g. "0.17.0").
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn error_json(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

fn diagnostic_error_json(message: &str, source: &str) -> String {
    let result: CompileDiagnosticResult<serde_json::Value> = CompileDiagnosticResult {
        metadata: None,
        diagnostics: vec![diagnostic_from_error(&CompileError::without_span(message), source)],
    };
    serde_json::to_string(&result).unwrap_or_else(|_| {
        serde_json::json!({ "metadata": null, "diagnostics": [{ "message": message, "severity": "error" }] }).to_string()
    })
}

fn diagnostic_from_error(error: &CompileError, source: &str) -> CompileDiagnostic {
    CompileDiagnostic {
        message: error.message.clone(),
        severity: error.severity.label(),
        code: error.code.clone(),
        file: error.file.as_ref().map(|file| file.to_string()),
        range: span_range(error.span, source),
    }
}

fn diagnostic_from_error_for_sources(
    error: &CompileError,
    source_by_path: &HashMap<String, String>,
    fallback_source: &str,
) -> CompileDiagnostic {
    let source = error.file.as_ref().and_then(|file| source_by_path.get(file.as_str())).map(String::as_str).unwrap_or(fallback_source);
    diagnostic_from_error(error, source)
}

fn span_range(span: Span, source: &str) -> Option<CompileDiagnosticRange> {
    if span.line == 0 || span.column == 0 {
        return None;
    }
    let source_len = source.len();
    let start = span.start.min(source_len);
    let end = span.end.min(source_len).max(start);
    let (end_line, end_column) = line_column_at(source, end);
    Some(CompileDiagnosticRange {
        start: CompileDiagnosticPosition { line: span.line, column: span.column, offset: start },
        end: CompileDiagnosticPosition { line: end_line, column: end_column, offset: end },
    })
}

fn line_column_at(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    let capped_offset = byte_offset.min(source.len());
    for (offset, ch) in source.char_indices() {
        if offset >= capped_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
