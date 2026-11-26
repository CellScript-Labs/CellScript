//! Lexical completion hints for invocation-local transaction/data views.
//! These hints do not replace type checking or establish runtime provenance.
use super::*;

pub(super) const NAMED_RUNTIME_DECLARATIONS: &[(&str, &str)] = &[
    ("collections", "collections ${1:Scratch} {\n    limits { bytes: ${2:8192}, allocations: ${3:32} },\n    ${4:totals}: OrderedMap<Hash, u64> { items: ${5:64} },\n    ${6:seen}: OrderedSet<Hash> { items: ${7:64} }\n}"),
    ("data_shape", "data_shape ${1:Messages} {\n    element: ${2:Vec<u8>},\n    limits { items: ${3:32}, payload_bytes: ${4:256}, encoded_bytes: ${5:4096} },\n}"),
    ("cell_roles", "cell_roles ${1:Settlement} {\n    limits { input_scan: ${2:64}, output_scan: ${3:64}, selected_per_side: ${4:32}, aggregate_bytes: ${5:32768} },\n    ${6:orders}: ${7:Order} {\n        type_hash: \"${8:0x0000000000000000000000000000000000000000000000000000000000000000}\",\n        lock_hash: \"${9:0x0000000000000000000000000000000000000000000000000000000000000000}\",\n        identity: ${10:id},\n        authority: ${11:observe},\n        correspondence: ${12:preserve},\n        inputs: ${13:0}..=${14:32},\n        outputs: ${15:0}..=${16:32},\n    }\n}"),
];

pub(super) fn source_role_sites(item: &Item) -> Option<(&str, Vec<(&str, Span)>)> {
    match item {
        Item::CellRoles(def) => Some((&def.name, def.roles.iter().map(|role| (role.name.as_str(), role.span)).collect())),
        Item::BoundPolicy(crate::ast::BoundPolicy::CellRoles(def)) => {
            Some((&def.name, def.roles.iter().map(|role| (role.name.as_str(), role.span)).collect()))
        }
        _ => None,
    }
}

impl LspServer {
    /// Follow the same nominal imports and package visibility as compilation.
    /// Searching every workspace declaration by spelling can select a decoy.
    pub(super) fn completion_type_definition(&self, uri: &str, name: &str) -> Option<crate::resolve::TypeDef> {
        self.view_index(uri)?.resolve_type(name)
    }

    pub(super) fn completion_record_fields(&self, uri: &str, name: &str) -> Option<Vec<Field>> {
        let crate::resolve::TypeDef::Struct(record) = self.completion_type_definition(uri, name)? else { return None };
        Some(record.fields)
    }

    pub(super) fn named_role_call_completions(&self, uri: &str, items: &mut [CompletionItem]) {
        let catalog = self
            .ast_cache
            .get(uri)
            .and_then(|module| {
                module.items.iter().find_map(|item| match item {
                    Item::CellRoles(def) => Some(def.name.as_str()),
                    Item::BoundPolicy(crate::ast::BoundPolicy::CellRoles(def)) => Some(def.name.as_str()),
                    _ => None,
                })
            })
            .unwrap_or("Catalog");
        for item in items {
            if matches!(item.label.as_str(), "verify_cell_roles" | "select_cell_roles") {
                item.insert_text = Some(format!("ckb::{}::<${{1:{catalog}}}>()", item.label));
                item.documentation = Some("Uses the named catalog's schema, identities and resource limits. Experimental; a catalog name does not authenticate its Script hashes.".into());
            }
        }
    }

    /// Role navigation is nominal and lexical: an alias keeps its catalog,
    /// while the same spelling in another catalog or local binding is distinct.
    pub(super) fn named_role_definition(&self, uri: &str, position: Position, symbol: &str) -> Option<Location> {
        let index = self.view_index(uri)?;
        self.named_role_definition_from_tokens(uri, position, symbol, &index)
    }

    fn named_role_definition_from_tokens(
        &self,
        uri: &str,
        position: Position,
        symbol: &str,
        index: &super::view_index::ViewIndex,
    ) -> Option<Location> {
        let tokens = &index.tokens;
        let source = self.documents.get(uri)?;
        let module = self.ast_cache.get(uri)?;
        for item in &module.items {
            if let Some((_, roles)) = source_role_sites(item) {
                for (name, span) in roles {
                    let range = Range {
                        start: offset_to_position(source, span.start),
                        end: offset_to_position(source, span.start + name.len()),
                    };
                    if name == symbol && position_in_range(position, range) {
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
        let ty = index.expression_at(offset, &receiver)?;
        let selection = crate::cell_selection::parse_selection_type(&ty)?;
        if selection.selected.is_some() {
            return None;
        }
        let name = selection.catalog?;
        let (role_name, role_span) = module.items.iter().find_map(|item| {
            let (catalog, roles) = source_role_sites(item)?;
            (catalog == name).then(|| roles.into_iter().find(|(name, _)| *name == symbol)).flatten()
        })?;
        Some(Location {
            uri: uri.into(),
            range: Range {
                start: offset_to_position(source, role_span.start),
                end: offset_to_position(source, role_span.start + role_name.len()),
            },
        })
    }

    pub(super) fn named_role_references(&self, uri: &str, position: Position, symbol: &str) -> Option<Vec<Location>> {
        let source = self.documents.get(uri)?;
        let index = self.view_index(uri)?;
        let tokens = &index.tokens;
        let definition = self.named_role_definition_from_tokens(uri, position, symbol, &index)?;
        Some(
            tokens
                .iter()
                .filter(|token| matches!(&token.kind, TokenKind::Identifier(name) if name == symbol))
                .filter_map(|token| {
                    let at = offset_to_position(source, token.span.start);
                    (self.named_role_definition_from_tokens(uri, at, symbol, &index).as_ref().map(|location| location.range)
                        == Some(definition.range))
                    .then(|| Location { uri: uri.into(), range: Range { start: at, end: offset_to_position(source, token.span.end) } })
                })
                .collect(),
        )
    }

    pub(super) fn cell_view_completion_type(&self, uri: &str, position: Position, before_dot: &str) -> Option<String> {
        let source = self.documents.get(uri)?;
        let offset = position_to_offset(source, position)?;
        let dot = self.line_start_offset(source, position.line) + before_dot.len();
        let index = self.view_index(uri)?;
        let end = index.tokens.partition_point(|token| token.span.start < dot);
        let start = receiver_start(&index.tokens, end.checked_sub(1)?)?;
        let receiver = crate::parser::parse_expression_fragment(&index.tokens[start..end]).ok()?;
        let ty = index.expression_at(offset, &receiver)?;
        (crate::cell_data_view::transaction_schema(&ty).is_some()
            || crate::cell_data_view::schema(&ty).is_some()
            || crate::cell_selection::parse_selection_type(&ty).is_some()
            || crate::nested_values::parsed(&ty).is_some()
            || matches!(index.resolve_type(&ty), Some(crate::resolve::TypeDef::Struct(_))))
        .then_some(ty)
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
            TokenKind::RBracket => {
                let mut depth = 1usize;
                while depth != 0 {
                    cursor = cursor.checked_sub(1)?;
                    match tokens[cursor].kind {
                        TokenKind::RBracket => depth += 1,
                        TokenKind::LBracket => depth -= 1,
                        _ => {}
                    }
                }
                cursor = cursor.checked_sub(1)?;
                continue;
            }
            _ => return None,
        }
        if cursor < 2 || tokens[cursor - 1].kind != TokenKind::Dot || tokens[cursor - 2].kind == TokenKind::Dot {
            return Some(cursor);
        }
        cursor -= 2;
    }
}

pub(super) fn shape_type(ty: &str, module: &Module) -> String {
    let (shape, member) = ty.rsplit_once("::").map_or((ty, None), |(shape, member)| (shape, Some(member)));
    module
        .items
        .iter()
        .find_map(|item| match item {
            Item::DataShape(def) if def.name == shape => {
                crate::nested_values::declared_values(def).ok().and_then(|values| match member {
                    None => Some(type_to_string(&values.source_type())),
                    Some("Item") => Some(values.item_type().source_text()),
                    Some("Payload") => Some(crate::nested_values::bytes_view_type(values.max_payload_bytes)),
                    _ => None,
                })
            }
            _ => None,
        })
        .unwrap_or_else(|| ty.into())
}

pub(super) fn nested_element(ty: &str) -> Option<String> {
    match crate::nested_values::parsed(ty)? {
        crate::nested_values::NestedType::Values(values) => Some(values.item_type().source_text()),
        crate::nested_values::NestedType::BytesView { .. } => Some("u8".into()),
        _ => None,
    }
}

fn project(ty: &str, field: &str, resolve_type: &dyn Fn(&str) -> Option<crate::resolve::TypeDef>) -> Option<String> {
    use crate::resolve::TypeDef;
    if let Some(crate::nested_values::NestedType::RecordView { element, max_payload_bytes }) = crate::nested_values::parsed(ty) {
        let TypeDef::Struct(record) = resolve_type(element.record_name()?)? else { return None };
        let fields = record.fields;
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
    if let Some(schema) = crate::cell_data_view::schema(ty) {
        let fields = match resolve_type(schema)? {
            TypeDef::Resource(def) => def.fields,
            TypeDef::Shared(def) => def.fields,
            TypeDef::Receipt(def) => def.fields,
            _ => return None,
        };
        return fields.iter().find(|member| member.name == field).map(|member| type_to_string(&member.ty));
    }
    (field == "data").then(|| crate::cell_data_view::transaction_schema(ty)).flatten().map(|schema| format!("CellDataView<{schema}>"))
}

pub(super) fn expression_type(
    expr: &Expr,
    bindings: &dyn Fn(&str) -> Option<String>,
    catalogs: &HashMap<String, crate::ast::CellRolesDef>,
    resolve_type: &dyn Fn(&str) -> Option<crate::resolve::TypeDef>,
) -> Option<String> {
    match expr {
        Expr::Identifier(name) => bindings(name),
        Expr::Index(index) => {
            let ty = expression_type(&index.expr, bindings, catalogs, resolve_type)?;
            if let Some((element, count)) = ty.strip_prefix('[').and_then(|ty| ty.strip_suffix(']')).and_then(|ty| ty.rsplit_once(';'))
                && count.trim().parse::<usize>().is_ok()
            {
                return Some(element.trim().into());
            }
            nested_element(&ty)
        }
        Expr::FieldAccess(field) => {
            project(&expression_type(&field.expr, bindings, catalogs, resolve_type)?, &field.field, resolve_type)
        }
        Expr::Call(call) if matches!(call.func.as_ref(), Expr::Identifier(name) if name == crate::cell_selection::SELECT) => {
            Some(crate::cell_selection::selection_type(&crate::cell_selection::resolve_call(call, catalogs).ok()?))
        }
        Expr::Call(call) if matches!(call.func.as_ref(), Expr::FieldAccess(_)) => {
            let Expr::FieldAccess(method) = call.func.as_ref() else { return None };
            let owner = expression_type(&method.expr, bindings, catalogs, resolve_type)?;
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
