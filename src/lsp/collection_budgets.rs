//! Budget hints use the same checked layouts and scenarios as the CLI. They
//! do not infer execution paths or change allocation policy.
use super::*;
use crate::allocated_values::budget;
use std::collections::HashSet;
#[cfg(feature = "collection-explanation")]
use std::sync::Arc;

impl LspServer {
    pub(super) fn collection_budget_hover(&self, uri: &str, name: &str) -> crate::error::Result<String> {
        let (module, resolver) = self.collection_module(uri)?;
        let mut checker = match &resolver {
            Some(resolver) => crate::types::TypeChecker::with_resolver(resolver, &module.name),
            None => crate::types::TypeChecker::new(),
        };
        checker.check_module(&module)?;
        let declaration = module
            .items
            .iter()
            .find_map(|item| match item {
                Item::Collections(def) if def.name == name => Some(def),
                _ => None,
            })
            .ok_or_else(|| CompileError::without_span("missing collection declaration"))?;
        let report = budget::analyze(&module.name, declaration, |ty| checker.allocated_value_width(ty, &mut HashSet::new()))?;
        Ok(format!(
            "\n\n`items` is a capacity limit, not an initial reservation.\n\n```text\n{}\n```\n\n{}",
            report.lines().join("\n"),
            budget::ASSUMPTIONS.join("\n\n")
        ))
    }

    pub(super) fn collection_module(&self, uri: &str) -> crate::error::Result<(Module, Option<crate::ModuleResolver>)> {
        let source = self.ast_cache.get(uri).ok_or_else(|| CompileError::without_span("missing parsed document"))?;
        let (module, resolver) = if source.items.iter().any(|item| matches!(item, Item::Use(_))) {
            // Resolve and specialize the opened dependency buffers together,
            // preserving package visibility and imported record widths.
            let view = self.view_index(uri).ok_or_else(|| CompileError::without_span("missing project inputs"))?;
            let entry = view
                .modules
                .iter()
                .find(|module| module.ast.name == source.name)
                .ok_or_else(|| CompileError::without_span("missing project entry"))?;
            let project = view.group.prepare_project(&entry.path).map_err(crate::diagnostics_to_compile_error)?;
            (project.entry().ast.clone(), Some(project.resolver))
        } else {
            // In-memory editor/browser documents need no filesystem project.
            (crate::generics::monomorphize_for_edition(source, self.document_edition(uri))?, None)
        };
        Ok((module, resolver))
    }

    #[cfg(feature = "collection-explanation")]
    pub(super) fn collection_storage_hover(&self, uri: &str, position: Position, symbol: &str) -> Option<String> {
        let index = self.view_index(uri)?;
        let source = self.documents.get(uri)?;
        let offset = position_to_offset(source, position)?;
        let ast = self.ast_cache.get(uri)?;
        let schema = ast.items.iter().find_map(|item| {
            let (name, fields) = match item {
                Item::Struct(def) => (&def.name, &def.fields),
                Item::Resource(def) => (&def.name, &def.fields),
                Item::Shared(def) => (&def.name, &def.fields),
                Item::Receipt(def) => (&def.name, &def.fields),
                _ => return None,
            };
            fields.iter().any(|field| field.name == symbol && field.span.start <= offset && offset < field.span.end).then_some(name)
        });
        if let Some(schema) = schema {
            let report = self.collection_storage_report(uri, &index)?;
            let mut models = Vec::new();
            for field in report.schema_fields.iter().filter(|field| {
                field.field == symbol
                    && ((field.source_module.as_deref() == Some(ast.name.as_str()) && field.source_schema.as_ref() == Some(schema))
                        || (field.source_module.is_none() && field.module == ast.name && &field.schema == schema))
            }) {
                if !models.contains(&&field.storage) {
                    models.push(&field.storage);
                }
            }
            return (!models.is_empty()).then(|| models.iter().take(8).map(|model| model.hover()).collect::<Vec<_>>().join("\n"));
        }
        let (root, root_offset, path) = storage_path(&index.tokens, offset, symbol)?;
        let declaration = index.binding_declaration(&root, root_offset)?;
        let entry = ast.items.iter().find_map(|item| {
            let span = item_span(item);
            if offset < span.start || offset > span.end {
                return None;
            }
            match item {
                Item::Action(def) => Some(format!("action:{}", def.name)),
                Item::Lock(def) => Some(format!("lock:{}", def.name)),
                Item::Function(def) => Some(format!("helper:{}", def.name)),
                _ => None,
            }
        })?;
        let report = self.collection_storage_report(uri, &index)?;
        // Matching the source declaration, rather than the name or Vec type,
        // keeps aliases and shadowing attached to their actual lowered value.
        let mut facts = Vec::new();
        for value in
            report.values.iter().filter(|value| value.entry == entry && value.binding == root && value.declaration == declaration)
        {
            let mut value = value.clone();
            let mut steps = path.as_slice();
            if !steps.is_empty() {
                for prefix in &value.access_prefix {
                    match steps.split_first() {
                        Some((StorageAccess::Field(field), tail)) if field == prefix => steps = tail,
                        _ => return None,
                    }
                }
            }
            for step in steps {
                value.storage = match step {
                    StorageAccess::Field(field) => value.storage.member(field)?,
                    StorageAccess::Index => value.storage.indexed()?,
                };
            }
            if !path.is_empty() {
                value.access_prefix.clear();
            }
            if !facts.iter().any(|old: &crate::collection_explanation::Value| old.storage == value.storage) {
                facts.push(value);
            }
        }
        if facts.is_empty() {
            return None;
        }
        let mut detail =
            if facts.len() > 1 { "\n\nMultiple resolved layouts apply to this source binding.".into() } else { String::new() };
        for value in facts.iter().take(8) {
            detail.push_str(&value.hover());
        }
        if facts.len() > 8 {
            detail.push_str("\n\nFull storage layouts are available in `cellc explain collections`.");
        }
        Some(detail)
    }

    #[cfg(feature = "collection-explanation")]
    fn collection_storage_report(
        &self,
        uri: &str,
        index: &super::view_index::ViewIndex,
    ) -> Option<Arc<crate::collection_explanation::Report>> {
        index
            .collection_storage
            .get_or_init(|| {
                if !index.complete_syntax {
                    return Err(CompileError::without_span("collection storage requires a completely parsed document"));
                }
                let (module, resolver) = self.collection_module(uri)?;
                let mut checker = match &resolver {
                    Some(resolver) => crate::types::TypeChecker::with_resolver(resolver, &module.name),
                    None => crate::types::TypeChecker::new(),
                };
                checker.check_module(&module)?;
                let ir = match resolver {
                    Some(resolver) => crate::ir::generate_with_resolver(&module, &resolver, &module.name)?,
                    None => crate::ir::generate(&module)?,
                };
                crate::collection_explanation::analyze(&ir).map(Arc::new)
            })
            .as_ref()
            .ok()
            .cloned()
    }
}

#[cfg(not(feature = "collection-explanation"))]
impl LspServer {
    pub(super) fn collection_storage_hover(&self, _uri: &str, _position: Position, _symbol: &str) -> Option<String> {
        None
    }
}

#[cfg(feature = "collection-explanation")]
#[derive(Debug)]
enum StorageAccess {
    Field(String),
    Index,
}

/// Locate a simple source access path in already admitted tokens. This is
/// navigation only; unfamiliar receivers do not receive a guessed storage model.
#[cfg(feature = "collection-explanation")]
fn storage_path(tokens: &[crate::lexer::token::Token], offset: usize, symbol: &str) -> Option<(String, usize, Vec<StorageAccess>)> {
    let mut cursor = tokens.iter().position(|token| {
        token.span.start <= offset && offset <= token.span.end && matches!(&token.kind,TokenKind::Identifier(name) if name==symbol)
    })?;
    let previous = |mut cursor: usize| loop {
        cursor = cursor.checked_sub(1)?;
        if tokens[cursor].kind != TokenKind::Newline {
            return Some(cursor);
        }
    };
    let mut path = Vec::new();
    for _ in 0..64 {
        match &tokens[cursor].kind {
            TokenKind::Identifier(name) => {
                if let Some(dot) = previous(cursor)
                    && tokens[dot].kind == TokenKind::Dot
                {
                    path.push(StorageAccess::Field(name.clone()));
                    cursor = previous(dot)?;
                } else {
                    path.reverse();
                    return Some((name.clone(), tokens[cursor].span.start, path));
                }
            }
            TokenKind::RBracket => {
                let mut depth = 1usize;
                loop {
                    cursor = previous(cursor)?;
                    match tokens[cursor].kind {
                        TokenKind::RBracket => depth += 1,
                        TokenKind::LBracket => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                path.push(StorageAccess::Index);
                cursor = previous(cursor)?;
            }
            _ => return None,
        }
    }
    None
}
