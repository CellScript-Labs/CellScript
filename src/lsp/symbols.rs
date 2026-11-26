use super::{span_to_range, Location, LspServer, SymbolInformation, SymbolKind};
use crate::ast::Item;

impl LspServer {
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
            Item::BoundPolicy(def) => Some(SymbolInformation {
                name: def.name().into(),
                kind: if matches!(def, crate::ast::BoundPolicy::CellRoles(_)) { SymbolKind::Constant } else { SymbolKind::Struct },
                location: Location { uri: uri.to_string(), range: span_to_range(source, def.span()) },
                container_name: None,
            }),
            Item::Collections(def) => Some(SymbolInformation {
                name: def.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(source, def.span) },
                container_name: None,
            }),
            Item::DataShape(def) => Some(SymbolInformation {
                name: def.name.clone(),
                kind: SymbolKind::Struct,
                location: Location { uri: uri.to_string(), range: span_to_range(source, def.span) },
                container_name: None,
            }),
            Item::CellRoles(def) => Some(SymbolInformation {
                name: def.name.clone(),
                kind: SymbolKind::Constant,
                location: Location { uri: uri.to_string(), range: span_to_range(source, def.span) },
                container_name: None,
            }),
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
}
