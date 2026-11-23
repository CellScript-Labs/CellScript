use super::positions::{offset_to_position, position_to_offset, word_before_offset, word_occurrences};
use super::uris::utf8_path_to_file_uri;
use super::{item_name, item_name_range, position_in_range, span_to_range, Location, LspServer, Position, Range};
use crate::ast::{BindingPattern, Item, Stmt};

impl LspServer {
    pub fn goto_definition(&self, uri: &str, position: Position) -> Option<Location> {
        let symbol = self.symbol_at_position(uri, position)?;
        if let Some(location) = self.named_role_definition(uri, position, &symbol) {
            return Some(location);
        }

        // 1. Try top-level symbol in the current file.
        if let Some(loc) = self.find_top_level_symbol(uri, &symbol) {
            return Some(loc);
        }

        // 2. Try field definition if inside a type reference (e.g. `token.amount`).
        if let Some(loc) = self.find_field_definition(uri, position, &symbol) {
            return Some(loc);
        }

        // 3. Try local variable / parameter definition.
        if let Some(loc) = self.find_local_definition(uri, position, &symbol) {
            return Some(loc);
        }

        // 4. Try workspace modules (cross-file).
        for module in self.workspace_modules(uri).iter() {
            let module_uri = utf8_path_to_file_uri(&module.path);
            if let Some(loc) = module.ast.items.iter().find_map(|item| {
                let name = item_name(item)?;
                if name == symbol {
                    Some(Location { uri: module_uri.clone(), range: item_name_range(&module.source, item, name) })
                } else {
                    None
                }
            }) {
                return Some(loc);
            }
        }

        None
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
                if let Stmt::Let(let_stmt) = stmt
                    && let BindingPattern::Name(name) = &let_stmt.pattern
                    && name == symbol
                {
                    return Some(Location { uri: uri.to_string(), range: span_to_range(content, let_stmt.span) });
                }
            }
        }
        None
    }

    pub fn find_references(&self, uri: &str, position: Position) -> Vec<Location> {
        let Some(symbol) = self.symbol_at_position(uri, position) else {
            return Vec::new();
        };
        if let Some(references) = self.named_role_references(uri, position, &symbol) {
            return references;
        }
        let mut refs = Vec::new();

        let workspace_modules = self.workspace_modules(uri);
        if !workspace_modules.is_empty() {
            for module in workspace_modules.iter() {
                let module_uri = utf8_path_to_file_uri(&module.path);
                for (start, end) in word_occurrences(&module.source, &symbol) {
                    refs.push(Location {
                        uri: module_uri.clone(),
                        range: Range {
                            start: offset_to_position(&module.source, start),
                            end: offset_to_position(&module.source, end),
                        },
                    });
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
            }
        }
        refs
    }
}
