use super::positions::{end_position, offset_to_position, position_to_offset, ranges_overlap, word_occurrences};
use super::uris::is_valid_rename_identifier;
use super::{CodeAction, LspServer, Position, Range, TextEdit, WorkspaceEdit};
use std::collections::HashMap;

impl LspServer {
    pub fn rename(&self, uri: &str, position: Position, new_name: String) -> HashMap<String, Vec<TextEdit>> {
        let mut changes = HashMap::new();
        if !is_valid_rename_identifier(&new_name) {
            return changes;
        }
        let Some(symbol) = self.symbol_at_position(uri, position) else { return changes };
        let Some(source) = self.documents.get(uri) else { return changes };
        let Some(offset) = position_to_offset(source, position) else { return changes };
        if !word_occurrences(source, &symbol).iter().any(|&(start, end)| start <= offset && offset <= end) {
            return changes;
        }
        if let Some(definition) = self.named_role_definition(uri, position, &symbol)
            && let (Some(module), Some(source)) = (self.ast_cache.get(uri), self.documents.get(uri))
            && module.items.iter().any(|item| {
                super::cell_views::source_role_sites(item).is_some_and(|(_, roles)| {
                    roles.iter().any(|(_, span)| offset_to_position(source, span.start) == definition.range.start)
                        && roles.iter().any(|(name, _)| *name == new_name && *name != symbol)
                })
            })
        {
            return changes;
        }
        let refs = self.find_references(uri, position);
        if refs.is_empty() {
            return changes;
        }
        for location in refs {
            changes.entry(location.uri).or_insert_with(Vec::new).push(TextEdit { range: location.range, new_text: new_name.clone() });
        }
        changes
    }

    pub fn code_action(&self, uri: &str, range: Range) -> Vec<CodeAction> {
        let mut actions = Vec::new();
        let has_temporal_migration_diagnostic = self
            .diagnostics
            .get(uri)
            .into_iter()
            .flat_map(|diagnostics| diagnostics.iter())
            .any(|diagnostic| diagnostic.code.as_deref() == Some("W3012") && ranges_overlap(diagnostic.range, range));
        if has_temporal_migration_diagnostic
            && let Some(content) = self.documents.get(uri)
            && let Ok(candidate) = crate::frontend::migrate_legacy_temporal_source(content, self.document_edition(uri))
            && candidate.replacements > 0
        {
            actions.push(CodeAction {
                title: format!(
                    "Migrate {} legacy CKB temporal call{} to explicit typed domains",
                    candidate.replacements,
                    if candidate.replacements == 1 { "" } else { "s" }
                ),
                kind: "quickfix".to_string(),
                edit: Some(WorkspaceEdit {
                    changes: HashMap::from([(
                        uri.to_string(),
                        vec![TextEdit {
                            range: Range { start: Position { line: 0, character: 0 }, end: end_position(content) },
                            new_text: candidate.source,
                        }],
                    )]),
                }),
            });
        }
        let has_lowering_diagnostic = self
            .diagnostics
            .get(uri)
            .into_iter()
            .flat_map(|diagnostics| diagnostics.iter())
            .any(|diagnostic| diagnostic.source == "cellscript-lowering" && ranges_overlap(diagnostic.range, range));

        if has_lowering_diagnostic {
            actions.push(CodeAction {
                title: "Inspect lowering/runtime metadata with `cellc metadata`".to_string(),
                kind: "quickfix".to_string(),
                edit: None,
            });
            actions.push(CodeAction {
                title: "Inspect locked package selection with `cellc resolve-graph --offline`".to_string(),
                kind: "quickfix".to_string(),
                edit: None,
            });
            actions.push(CodeAction {
                title: "Inspect build units with `cellc build-plan --offline`".to_string(),
                kind: "quickfix".to_string(),
                edit: None,
            });
            actions.push(CodeAction {
                title: "Plan dependency changes with `cellc update-plan --offline`".to_string(),
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
        let Some(_) = self.ast_cache.get(uri) else {
            return Vec::new();
        };
        let Ok(formatted) = crate::fmt::format_source(content, crate::CURRENT_EDITION, Default::default()) else {
            return Vec::new();
        };
        if formatted.as_str() == content.as_str() {
            return Vec::new();
        }
        vec![TextEdit { range: Range { start: Position { line: 0, character: 0 }, end: end_position(content) }, new_text: formatted }]
    }

    pub fn format_range(&self, uri: &str, _range: Range) -> Vec<TextEdit> {
        self.format_document(uri)
    }
}
