use super::positions::{offset_to_position, position_to_offset, word_occurrences};
use super::{
    item_span, position_in_range, span_to_range, stmt_span, DocumentHighlight, DocumentHighlightKind, FoldingRange, FoldingRangeKind,
    LspServer, Position, Range, SelectionRange,
};
use crate::ast::{Item, Stmt};

impl LspServer {
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
                Item::Shared(s) if !s.fields.is_empty() => {
                    let range = span_to_range(content, s.span);
                    ranges.push(FoldingRange {
                        start_line: range.start.line,
                        start_character: Some(range.start.character),
                        end_line: range.end.line,
                        end_character: Some(range.end.character),
                        kind: Some(FoldingRangeKind::Region),
                    });
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

        ranges.sort_by(|a, b| {
            let a_size = (b.start.line - a.start.line) * 10000 + b.start.character.saturating_sub(a.start.character);
            let b_size = (b.start.line - a.start.line) * 10000 + b.start.character.saturating_sub(a.start.character);
            a_size.cmp(&b_size)
        });

        let mut result = SelectionRange { range: ranges[0], parent: None };
        for range in ranges.iter().skip(1) {
            result = SelectionRange { range: *range, parent: Some(Box::new(result)) };
        }

        Some(result)
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
}
