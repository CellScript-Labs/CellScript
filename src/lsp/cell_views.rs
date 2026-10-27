//! Lexical completion hints for invocation-local transaction/data views.
//! These hints do not replace type checking or establish runtime provenance.
use super::*;

pub(super) const NAMED_RUNTIME_DECLARATIONS: &[(&str, &str)] = &[
    ("collections", "collections ${1:Scratch} {\n    limits { bytes: ${2:8192}, allocations: ${3:32} }\n    ${4:totals}: OrderedMap<Hash, u64> { items: ${5:64} }\n    ${6:seen}: OrderedSet<Hash> { items: ${7:64} }\n}"),
    ("data_shape", "data_shape ${1:Messages} {\n    element: ${2:Vec<u8>},\n    limits { items: ${3:32}, payload_bytes: ${4:256}, encoded_bytes: ${5:4096} }\n}"),
    ("cell_roles", "cell_roles ${1:Settlement} {\n    limits { input_scan: ${2:64}, output_scan: ${3:64}, selected_per_side: ${4:32}, aggregate_bytes: ${5:32768} }\n    ${6:orders}: ${7:Order} {\n        type_hash: \"${8:0x0000000000000000000000000000000000000000000000000000000000000000}\",\n        lock_hash: \"${9:0x0000000000000000000000000000000000000000000000000000000000000000}\",\n        identity: ${10:id},\n        authority: ${11:observe},\n        correspondence: ${12:preserve},\n        inputs: ${13:0}..=${14:32},\n        outputs: ${15:0}..=${16:32},\n    }\n}"),
];

impl LspServer {
    /// Follow the same nominal imports and package visibility as compilation.
    /// Searching every workspace declaration by spelling can select a decoy.
    pub(super) fn completion_type_definition(&self, uri: &str, name: &str) -> Option<crate::resolve::TypeDef> {
        use crate::resolve::TypeDef;
        let module = self.ast_cache.get(uri)?;
        if let Some(definition) = module.items.iter().find_map(|item| match item {
            Item::Resource(def) if def.name == name => Some(TypeDef::Resource(def.clone())),
            Item::Shared(def) if def.name == name => Some(TypeDef::Shared(def.clone())),
            Item::Receipt(def) if def.name == name => Some(TypeDef::Receipt(def.clone())),
            Item::Struct(def) if def.name == name => Some(TypeDef::Struct(def.clone())),
            _ => None,
        }) {
            return Some(definition);
        }
        let modules = self.workspace_modules(uri);
        let resolver = crate::build_module_resolver_from_loaded_modules(&modules).ok()?;
        resolver.resolve_type(&module.name, name)
    }

    pub(super) fn completion_record_fields(&self, uri: &str, name: &str) -> Option<Vec<Field>> {
        let crate::resolve::TypeDef::Struct(record) = self.completion_type_definition(uri, name)? else { return None };
        Some(record.fields)
    }

    pub(super) fn named_runtime_authoring(&self, uri: &str) -> bool {
        self.document_edition(uri) == crate::NEXT_EDITION
            && !self.documents.get(uri).is_some_and(|source| crate::frontend::uses_native_preview(source))
    }

    pub(super) fn named_role_call_completions(&self, uri: &str, items: &mut [CompletionItem]) {
        if !self.named_runtime_authoring(uri) {
            return;
        }
        let catalog = self
            .ast_cache
            .get(uri)
            .and_then(|module| {
                module.items.iter().find_map(|item| match item {
                    Item::CellRoles(def) => Some(def.name.as_str()),
                    _ => None,
                })
            })
            .unwrap_or("Catalog");
        for item in items {
            if matches!(item.label.as_str(), "verify_cell_roles" | "select_cell_roles") {
                item.insert_text = Some(format!("ckb::{}<${{1:{catalog}}}>()", item.label));
                item.documentation = Some("Uses the named catalog's schema, identities and resource limits. Experimental; a catalog name does not authenticate its Script hashes.".into());
            }
        }
    }

    /// Role navigation is nominal and lexical: an alias keeps its catalog,
    /// while the same spelling in another catalog or local binding is distinct.
    pub(super) fn named_role_definition(&self, uri: &str, position: Position, symbol: &str) -> Option<Location> {
        let tokens = crate::lexer::lex(self.documents.get(uri)?).ok()?;
        self.named_role_definition_from_tokens(uri, position, symbol, &tokens)
    }

    fn named_role_definition_from_tokens(
        &self,
        uri: &str,
        position: Position,
        symbol: &str,
        tokens: &[crate::lexer::token::Token],
    ) -> Option<Location> {
        let source = self.documents.get(uri)?;
        let module = self.ast_cache.get(uri)?;
        for item in &module.items {
            if let Item::CellRoles(catalog) = item {
                for role in &catalog.roles {
                    let range = Range {
                        start: offset_to_position(source, role.span.start),
                        end: offset_to_position(source, role.span.start + role.name.len()),
                    };
                    if role.name == symbol && position_in_range(position, range) {
                        return Some(Location { uri: uri.into(), range });
                    }
                }
            }
        }
        let offset = position_to_offset(source, position)?;
        // Comments may span lines between a receiver and its role. Use real
        // identifier/dot tokens so their contents cannot change the receiver.
        let first = tokens.partition_point(|token| token.span.end < offset);
        let role_token = tokens[first..]
            .iter()
            .take_while(|token| token.span.start <= offset)
            .position(|token| matches!(&token.kind, TokenKind::Identifier(name) if name == symbol))?
            + first;
        let dot = role_token.checked_sub(1)?;
        if tokens[dot].kind != TokenKind::Dot {
            return None;
        }
        let start = receiver_start(tokens, dot.checked_sub(1)?)?;
        let receiver = crate::parser::parse_expression_fragment(&tokens[start..dot]).ok()?;
        let ty = self.cell_view_expression_type(uri, position, &receiver)?;
        let selection = crate::cell_selection::parse_selection_type(&ty)?;
        if selection.selected.is_some() {
            return None;
        }
        let name = selection.catalog?;
        let role = module.items.iter().find_map(|item| match item {
            Item::CellRoles(catalog) if catalog.name == name => catalog.roles.iter().find(|role| role.name == symbol),
            _ => None,
        })?;
        Some(Location {
            uri: uri.into(),
            range: Range {
                start: offset_to_position(source, role.span.start),
                end: offset_to_position(source, role.span.start + role.name.len()),
            },
        })
    }

    pub(super) fn named_role_references(&self, uri: &str, position: Position, symbol: &str) -> Option<Vec<Location>> {
        let source = self.documents.get(uri)?;
        let tokens = crate::lexer::lex(source).ok()?;
        let definition = self.named_role_definition_from_tokens(uri, position, symbol, &tokens)?;
        Some(
            tokens
                .iter()
                .filter(|token| matches!(&token.kind, TokenKind::Identifier(name) if name == symbol))
                .filter_map(|token| {
                    let at = offset_to_position(source, token.span.start);
                    (self.named_role_definition_from_tokens(uri, at, symbol, &tokens).as_ref().map(|location| location.range)
                        == Some(definition.range))
                    .then(|| Location { uri: uri.into(), range: Range { start: at, end: offset_to_position(source, token.span.end) } })
                })
                .collect(),
        )
    }

    pub(super) fn cell_view_completion_type(&self, uri: &str, position: Position, before_dot: &str) -> Option<String> {
        let path = before_dot.trim_end().rsplit(|ch: char| !is_ident_char(ch) && ch != '.').next()?.rsplit("..").next()?;
        let tokens = crate::lexer::lex(path).ok()?;
        let receiver = crate::parser::parse_expression_fragment(&tokens[..tokens.len().checked_sub(1)?]).ok()?;
        self.cell_view_expression_type(uri, position, &receiver)
    }

    fn cell_view_expression_type(&self, uri: &str, position: Position, receiver: &Expr) -> Option<String> {
        let source = self.documents.get(uri)?;
        let module = self.ast_cache.get(uri)?;
        let catalogs = module
            .items
            .iter()
            .filter_map(|item| match item {
                Item::CellRoles(def) => Some((def.name.clone(), def.clone())),
                _ => None,
            })
            .collect();
        let record_fields = |name: &str| self.completion_record_fields(uri, name);
        for item in &module.items {
            if !position_in_range(position, span_to_range(source, item_span(item))) {
                continue;
            }
            let (params, body) = match item {
                Item::Action(item) => (&item.params, &item.body),
                Item::Lock(item) => (&item.params, &item.body),
                Item::Function(item) => (&item.params, &item.body),
                _ => continue,
            };
            let mut bindings =
                params.iter().map(|param| (param.name.clone(), shape_type(&type_to_string(&param.ty), module))).collect();
            visible_bindings(source, body, position, &mut bindings, &catalogs, module, &record_fields);
            let ty = expression_type(receiver, &bindings, &catalogs, &record_fields)?;
            return (crate::cell_data_view::transaction_schema(&ty).is_some()
                || crate::cell_data_view::schema(&ty).is_some()
                || crate::cell_selection::parse_selection_type(&ty).is_some()
                || crate::nested_values::parsed(&ty).is_some())
            .then_some(ty);
        }
        None
    }
}

/// Locate a lexical receiver path, preserving balanced grouping for the real
/// expression parser. Range endpoints stop at `..`, which lexes as two dots.
fn receiver_start(tokens: &[crate::lexer::token::Token], mut cursor: usize) -> Option<usize> {
    loop {
        match tokens.get(cursor)?.kind {
            TokenKind::Identifier(_) => {}
            TokenKind::RParen => {
                let mut depth = 1usize;
                while depth != 0 {
                    cursor = cursor.checked_sub(1)?;
                    match tokens[cursor].kind {
                        TokenKind::RParen => depth += 1,
                        TokenKind::LParen => depth -= 1,
                        _ => {}
                    }
                }
            }
            _ => return None,
        }
        if cursor < 2 || tokens[cursor - 1].kind != TokenKind::Dot || tokens[cursor - 2].kind == TokenKind::Dot {
            return Some(cursor);
        }
        cursor -= 2;
    }
}

fn visible_bindings(
    source: &str,
    body: &[Stmt],
    position: Position,
    bindings: &mut HashMap<String, String>,
    catalogs: &HashMap<String, crate::ast::CellRolesDef>,
    module: &Module,
    record_fields: &dyn Fn(&str) -> Option<Vec<Field>>,
) {
    for stmt in body {
        let range = span_to_range(source, stmt_span(stmt));
        if position_le(position, range.start) {
            break;
        }
        match stmt {
            Stmt::Let(binding) if position_le(range.end, position) => {
                if let BindingPattern::Name(name) = &binding.pattern {
                    let ty = binding
                        .ty
                        .as_ref()
                        .map(|ty| shape_type(&type_to_string(ty), module))
                        .or_else(|| expression_type(&binding.value, bindings, catalogs, record_fields));
                    bindings.remove(name);
                    if let Some(ty) = ty {
                        bindings.insert(name.clone(), ty);
                    }
                } else {
                    remove_pattern(&binding.pattern, bindings);
                }
            }
            Stmt::If(branch) if position_in_range(position, range) => {
                for body in std::iter::once(&branch.then_branch).chain(branch.else_branch.iter()) {
                    if body.iter().any(|stmt| position_in_range(position, span_to_range(source, stmt_span(stmt)))) {
                        visible_bindings(source, body, position, bindings, catalogs, module, record_fields);
                        break;
                    }
                }
            }
            Stmt::While(loop_) if position_in_range(position, range) => {
                visible_bindings(source, &loop_.body, position, bindings, catalogs, module, record_fields)
            }
            Stmt::For(loop_) if position_in_range(position, range) => {
                let element = expression_type(&loop_.iterable, bindings, catalogs, record_fields).and_then(|ty| nested_element(&ty));
                remove_pattern(&loop_.pattern, bindings);
                if let (BindingPattern::Name(name), Some(element)) = (&loop_.pattern, element) {
                    bindings.insert(name.clone(), element);
                }
                visible_bindings(source, &loop_.body, position, bindings, catalogs, module, record_fields);
            }
            Stmt::Borrow(borrow) if position_in_range(position, range) => {
                bindings.remove(&borrow.binding);
                visible_bindings(source, &borrow.body, position, bindings, catalogs, module, record_fields);
            }
            _ => {}
        }
    }
}

fn remove_pattern(pattern: &BindingPattern, bindings: &mut HashMap<String, String>) {
    match pattern {
        BindingPattern::Name(name) => {
            bindings.remove(name);
        }
        BindingPattern::Tuple(parts) => parts.iter().for_each(|part| remove_pattern(part, bindings)),
        BindingPattern::Wildcard => {}
    }
}

fn shape_type(ty: &str, module: &Module) -> String {
    let (shape, member) = ty.rsplit_once("::").map_or((ty, None), |(shape, member)| (shape, Some(member)));
    module
        .items
        .iter()
        .find_map(|item| match item {
            Item::DataShape(def) if def.name == shape => {
                crate::nested_values::declared_values(def).ok().and_then(|values| match member {
                    None => Some(type_to_string(&values.source_type())),
                    Some("Item") => Some(values.item_type()),
                    Some("Payload") => Some(crate::nested_values::bytes_view_type(values.max_payload_bytes)),
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap_or_else(|| ty.into())
}

fn nested_element(ty: &str) -> Option<String> {
    match crate::nested_values::parsed(ty)? {
        crate::nested_values::NestedType::Values(values) => Some(values.item_type()),
        crate::nested_values::NestedType::BytesView { .. } => Some("u8".into()),
        _ => None,
    }
}

fn project(ty: &str, field: &str, record_fields: &dyn Fn(&str) -> Option<Vec<Field>>) -> Option<String> {
    if let Some(crate::nested_values::NestedType::RecordView { element, max_payload_bytes }) = crate::nested_values::parsed(ty) {
        let fields = record_fields(&element)?;
        let (index, member) = fields.iter().enumerate().find(|(_, member)| member.name == field)?;
        return Some(if index + 1 == fields.len() {
            crate::nested_values::bytes_view_type(max_payload_bytes)
        } else {
            type_to_string(&member.ty)
        });
    }
    if let Some(projected) = crate::cell_selection::project_role_type(ty, field) {
        return Some(projected);
    }
    (field == "data").then(|| crate::cell_data_view::transaction_schema(ty)).flatten().map(|schema| format!("CellDataView<{schema}>"))
}

fn expression_type(
    expr: &Expr,
    bindings: &HashMap<String, String>,
    catalogs: &HashMap<String, crate::ast::CellRolesDef>,
    record_fields: &dyn Fn(&str) -> Option<Vec<Field>>,
) -> Option<String> {
    match expr {
        Expr::Identifier(name) => bindings.get(name).cloned(),
        Expr::Index(index) => nested_element(&expression_type(&index.expr, bindings, catalogs, record_fields)?),
        Expr::FieldAccess(field) => {
            project(&expression_type(&field.expr, bindings, catalogs, record_fields)?, &field.field, record_fields)
        }
        Expr::Call(call) if matches!(call.func.as_ref(), Expr::Identifier(name) if name == crate::cell_selection::SELECT) => {
            Some(crate::cell_selection::selection_type(&crate::cell_selection::resolve_call(call, catalogs).ok()?))
        }
        Expr::Call(call) if matches!(call.func.as_ref(), Expr::FieldAccess(_)) => {
            let Expr::FieldAccess(method) = call.func.as_ref() else { return None };
            let owner = expression_type(&method.expr, bindings, catalogs, record_fields)?;
            let selected = crate::cell_selection::parse_selection_type(&owner)?;
            let read = crate::cell_selection::method(&method.field)?;
            Some(read.result_type(&selected.schemas[selected.selected? as usize]))
        }
        Expr::Call(call) if call.type_args.len() == 1 => {
            let Expr::Identifier(name) = call.func.as_ref() else { return None };
            let view = match name.as_str() {
                "ckb::input" | "ckb::group_input" | "ckb::selected_input" => "InputView",
                "ckb::output" | "ckb::group_output" | "ckb::selected_output" | "ckb::selected_successor" => "OutputView",
                _ => return None,
            };
            Some(format!("{view}<{}>", type_to_string(&call.type_args[0])))
        }
        _ => None,
    }
}
