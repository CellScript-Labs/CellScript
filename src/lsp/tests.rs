use super::hover_display::generic_params_hover;
use super::*;
use tempfile::tempdir;

#[test]
fn named_unsigned_bounds_editor_keeps_policy_hover_symbols_and_budget_revision() {
    let source = "module bounded_editor; private const BYTES: u32 = 256; private const EVENTS: u32 = 3; private const ITEMS: u32 = 3;
        data_shape Messages { element: Vec<u8>, limits { items: ITEMS, payload_bytes: 8, encoded_bytes: 128 } }
        collections Work { limits { bytes: BYTES, allocations: EVENTS }, rows: Vec<u64> { items: ITEMS } }
        public action inspect(witness bytes: [u8; ITEMS], witness messages: Messages) -> u64 {
            require bytes[0] == 0; return messages.len();
        }";
    let uri = "untitled:named-unsigned-bounds";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).is_empty(), "{:?}", server.get_diagnostics(uri));
    let symbols = server.document_symbols(uri);
    for name in ["Messages", "Work", "ITEMS"] {
        assert!(symbols.iter().any(|symbol| symbol.name == name));
    }
    let work_position = offset_to_position(source, source.find("collections Work").unwrap() + 13);
    let hover = server.hover(uri, work_position).unwrap().contents;
    assert!(hover.contains("rows: 3 items x 8 bytes"), "{hover}");
    let messages_position = offset_to_position(source, source.find("data_shape Messages").unwrap() + 12);
    let hover = server.hover(uri, messages_position).unwrap().contents;
    assert!(hover.contains("3 items, 8 payload bytes, 128 encoded bytes"), "{hover}");
    let changed = source.replace("const ITEMS: u32 = 3", "const ITEMS: u32 = 4");
    server.open_document_with_edition(uri.into(), changed, crate::CURRENT_EDITION);
    let hover = server.hover(uri, work_position).unwrap().contents;
    assert!(hover.contains("rows: 4 items x 8 bytes"), "{hover}");
    let invalid = source.replace("const ITEMS: u32 = 3", "const ITEMS: u32 = 2048");
    server.open_document_with_edition(uri.into(), invalid, crate::CURRENT_EDITION);
    let hover = server.hover(uri, work_position).unwrap().contents;
    assert!(hover.contains("Budget estimate unavailable"), "{hover}");
    assert!(!hover.contains("Reserve all members once"), "{hover}");
}

#[test]
#[cfg(feature = "collection-explanation")]
fn collection_storage_hover_follows_aliases_disjoint_scopes_and_rejected_shadowing() {
    let source = r#"module storage_hover;
resource Bag has store { values: Vec<u64>, }
action verify(read bag: Bag, witness choose: bool) -> u64 {
    let mut root_values: Vec<u64> = Vec::with_capacity(4);
    let alias = root_values;
    if choose {
        let values = bag.values;
        require values.len() > 0;
    } else {
        let mut values: Vec<u16> = Vec::new();
        require values.capacity() == 128;
    }
    let count = {
        let mut inner: Vec<u8> = Vec::new();
        inner.capacity()
    };
    let via_match = match alias { captured => captured.capacity(), };
    let scalar = match 1 { scalar_value => scalar_value + 1, };
    return root_values.capacity() + count + via_match + scalar;
}
"#;
    let uri = "file:///storage_hover.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(
        server.get_diagnostics(uri).iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error),
        "{:?}",
        server.get_diagnostics(uri)
    );
    for (text, capacity) in [("root_values: Vec<u64>", 32), ("alias =", 32), ("values: Vec<u16>", 128), ("inner: Vec<u8>", 256)] {
        let hover = server
            .hover(uri, offset_to_position(source, source.find(text).unwrap() + 1))
            .unwrap_or_else(|| panic!("missing storage hover for {text}"))
            .contents;
        assert!(hover.contains(&format!("capacity(): {capacity} elements")), "{text}: {hover}");
    }
    let view = server.hover(uri, offset_to_position(source, source.find("values.len()").unwrap() + 1)).unwrap().contents;
    assert!(view.contains("encoded vector view"), "{view}");
    assert!(!view.contains("capacity(): 32 elements"), "{view}");
    let field = server.hover(uri, offset_to_position(source, source.find("values: Vec<u64>").unwrap() + 1)).unwrap().contents;
    assert!(field.contains("encoded vector view"), "{field}");
    for (text, capacity) in [("values.capacity()", 128), ("inner.capacity()", 256), ("root_values.capacity()", 32)] {
        let hover = server.hover(uri, offset_to_position(source, source.find(text).unwrap() + 1)).unwrap().contents;
        assert!(hover.contains(&format!("capacity(): {capacity} elements")), "{text}: {hover}");
    }
    let captured = server.hover(uri, offset_to_position(source, source.find("captured.capacity()").unwrap() + 1)).unwrap().contents;
    assert!(captured.contains("capacity(): 32 elements"), "{captured}");
    let scalar = server.hover(uri, offset_to_position(source, source.find("scalar_value + 1").unwrap() + 1));
    assert!(scalar.is_none_or(|hover| !hover.contents.contains("Resolved collection storage")));
    let invalid = source.replace("scalar_value => scalar_value + 1", "root_values => root_values + 1");
    server.open_document_with_edition(uri.into(), invalid.clone(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().any(|diagnostic| diagnostic.message.contains("already exists")));
    let shadow = server.hover(uri, offset_to_position(&invalid, invalid.find("root_values + 1").unwrap() + 1));
    assert!(shadow.is_none_or(|hover| !hover.contents.contains("Resolved collection storage")));
}

#[test]
#[cfg(feature = "collection-explanation")]
fn collection_storage_hover_follows_context_fields_indexing_and_payload_limits() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-nested-data-views.cell");
    let uri = "file:///storage_codecs.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(
        server.get_diagnostics(uri).iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error),
        "{:?}",
        server.get_diagnostics(uri)
    );
    let alias = server.hover(uri, offset_to_position(source, source.find("alias[0]").unwrap() + 1)).unwrap().contents;
    assert!(alias.contains("Maximum list items: 32"), "{alias}");
    assert!(alias.contains("Maximum whole-list encoding: 4096 bytes"), "{alias}");
    let offset = source.find("entries[0].payload").unwrap() + "entries[0].".len() + 1;
    let payload = server.hover(uri, offset_to_position(source, offset)).unwrap().contents;
    assert!(payload.contains("payload length bound: 256 bytes"), "{payload}");
    assert!(!payload.contains("capacity():"), "{payload}");
    let offset = source.find("after.data.messages").unwrap() + "after.data.".len() + 1;
    let messages = server.hover(uri, offset_to_position(source, offset)).unwrap().contents;
    assert!(messages.contains("immutable nested-values codec view"), "{messages}");
}

#[test]
#[cfg(feature = "collection-explanation")]
fn collection_storage_hover_refreshes_imported_generic_widths_from_unsaved_dependencies() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        "[package]\nname=\"storage\"\nversion=\"0.1.0\"\nedition=\"2027\"\ncellscript_version=\">=0.33.0-alpha.1\"\n",
    )
    .unwrap();
    let schema = "module schema; public struct Pair<T: fixed_value> {left:T,right:T,}";
    let source="module storage; use schema::Pair as Row; collections Work {limits {bytes:1024,allocations:8}, entries:Vec<Row<u64>> {items:16}} action verify()->u64 {let work=Work::new(); return work.entries.len();}";
    std::fs::write(root.join("src/schema.cell"), schema).unwrap();
    std::fs::write(root.join("src/main.cell"), source).unwrap();
    let uri = utf8_path_to_file_uri(&root.join("src/main.cell"));
    let schema_uri = utf8_path_to_file_uri(&root.join("src/schema.cell"));
    let mut server = LspServer::new();
    server.open_document(uri.clone(), source.into());
    let position = offset_to_position(source, source.find("work.entries").unwrap() + 1);
    let first = server.hover(&uri, position).unwrap().contents;
    assert!(first.contains("element width 16 bytes"), "{first}");
    server.open_document(schema_uri, schema.replace("right:T,", "right:T,extra:T,"));
    let second = server.hover(&uri, position).unwrap().contents;
    assert!(second.contains("element width 24 bytes"), "{second}");
    assert!(!second.contains("element width 16 bytes"), "{second}");
}

#[test]
fn test_lsp_position_conversion_uses_utf16_columns() {
    let source = "a😀b\nβc";
    let b_offset = source.find('b').expect("b offset");
    let beta_offset = source.find('β').expect("beta offset");

    assert_eq!(offset_to_position(source, b_offset), Position { line: 0, character: 3 });
    assert_eq!(position_to_offset(source, Position { line: 0, character: 3 }), Some(b_offset));
    assert_eq!(position_to_offset(source, Position { line: 0, character: 2 }), None);
    assert_eq!(offset_to_position(source, beta_offset), Position { line: 1, character: 0 });
    assert_eq!(position_to_offset(source, Position { line: 1, character: 1 }), Some(beta_offset + 'β'.len_utf8()));
}

#[test]
fn named_cell_roles_catalogs_have_completion_definition_and_hover() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-role-verification.cell");
    let uri = "file:///named_roles.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    let diagnostics = server.get_diagnostics(uri);
    assert!(diagnostics.iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
    let call = source.rfind("Settlement").unwrap();
    let position = offset_to_position(source, call + 1);
    let completion = server.completion(uri, position);
    assert!(completion.iter().any(|item| item.label == "Settlement"
        && item.documentation.as_deref().is_some_and(|doc| doc.contains("source, authority and lifetime"))));
    let definition = server.goto_definition(uri, position).unwrap();
    assert_eq!(definition.range.start, offset_to_position(source, source.find("Settlement").unwrap()));
    assert!(server.hover(uri, position).unwrap().contents.contains("named roles"));
    assert!(server.document_symbols(uri).iter().any(|symbol| symbol.name == "Settlement"));
    let renamed = server.rename(uri, position, "SettlementRules".into());
    assert_eq!(renamed.get(uri).unwrap().len(), 2, "rename the declaration and catalog reference together");
}

#[test]
fn mixed_settlement_has_named_role_navigation_and_helper_diagnostics() {
    let source = include_str!("../../examples/language/batches/mixed_settlement.cell");
    let uri = "file:///mixed_settlement.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    let diagnostics = server.get_diagnostics(uri);
    assert!(diagnostics.iter().all(|item| item.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
    let offset = source.find("selected.quote.input_count").unwrap() + "selected.".len();
    let definition = server.goto_definition(uri, offset_to_position(source, offset + 1)).unwrap();
    assert_eq!(definition.range.start, offset_to_position(source, source.find("quote: Balance").unwrap()));
    let declaration = &source[source.find("cell_roles Settlement").unwrap()..source.find("fn full_fill").unwrap()];
    let augmented =
        format!("{source}\n{}\nfn unrelated(quote: u64) -> u64 {{ return quote; }}", declaration.replace("Settlement", "Other"));
    server.open_document_with_edition(uri.into(), augmented, crate::CURRENT_EDITION);
    assert!(server.rename(uri, offset_to_position(source, offset + 1), "base".into()).is_empty());
    let renamed = server.rename(uri, offset_to_position(source, offset + 1), "payments".into());
    let edits = renamed.get(uri).unwrap();
    assert_eq!(edits.len(), 1 + source.matches("selected.quote.").count());
    assert!(edits.iter().all(|edit| edit.range.start.line < source.lines().count() as u32));
    let invalid = source.replace("order.data.give", "order.data.missing");
    server.open_document_with_edition(uri.into(), invalid, crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().any(|item| item.severity == DiagnosticSeverity::Error));
}

#[test]
fn named_role_rename_preserves_catalog_identity_across_receiver_forms() {
    let original = include_str!("../../examples/language/batches/mixed_settlement.cell");
    let declaration = &original[original.find("cell_roles Settlement").unwrap()..original.find("fn full_fill").unwrap()];
    for (receiver, separator) in [
        ("selected", " /* quote is a role */ "),
        ("selected", " /* quote remains\n in this catalog */ "),
        ("(selected)", " /* quote is a role */ "),
        ("((selected))", " /* quote remains\n in this catalog */ "),
    ] {
        let access = format!("{receiver}{separator}.quote");
        let source = format!(
            "{}\n{}\nfn unrelated(quote: u64) -> u64 {{ return quote; }}",
            original.replace("selected.quote", &access),
            declaration.replace("Settlement", "Other")
        );
        let uri = "file:///role_comments.cell";
        let mut server = LspServer::new();
        server.open_document_with_edition(uri.into(), source.clone(), crate::CURRENT_EDITION);
        assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
        let use_offset = source.find(&access).unwrap() + receiver.len() + separator.len() + 1;
        let use_position = offset_to_position(&source, use_offset + 1);
        let comment = source.find("/* quote").unwrap() + "/* ".len();
        assert!(server.rename(uri, offset_to_position(&source, comment + 1), "payments".into()).is_empty());
        let definition = server.goto_definition(uri, use_position).expect("commented role receiver must resolve");
        assert_eq!(definition.range.start, offset_to_position(&source, source.find("quote: Balance").unwrap()));
        assert!(server.rename(uri, use_position, "base".into()).is_empty());
        let changes = server.rename(uri, use_position, "payments".into());
        assert_eq!(changes.len(), 1);
        let edits = changes.get(uri).unwrap();
        assert_eq!(edits.len(), 1 + original.matches("selected.quote.").count());
        let mut replacements = edits
            .iter()
            .map(|edit| {
                (
                    position_to_offset(&source, edit.range.start).unwrap(),
                    position_to_offset(&source, edit.range.end).unwrap(),
                    &edit.new_text,
                )
            })
            .collect::<Vec<_>>();
        replacements.sort_by_key(|(start, _, _)| std::cmp::Reverse(*start));
        let mut renamed = source.clone();
        for (start, end, replacement) in replacements {
            renamed.replace_range(start..end, replacement);
        }
        assert!(renamed.contains("fn unrelated(quote: u64) -> u64 { return quote; }"));
        assert!(renamed.contains(&declaration.replace("Settlement", "Other")));
        assert!(renamed.contains(separator));
        assert_eq!(renamed.matches(".payments.").count(), original.matches("selected.quote.").count());
        crate::compile_metadata(&renamed, crate::CURRENT_EDITION, None).unwrap();
    }
}

#[test]
fn declaration_snippets_expand_to_current_lossless_source_and_do_not_offer_reserved_entries() {
    use crate::ast::semantic_shape::SemanticEq;
    fn expand_defaults(snippet: &str) -> String {
        let mut expanded = snippet.replace("$0", "");
        while let Some(start) = expanded.find("${") {
            let end = start + expanded[start..].find('}').unwrap();
            let (_, value) = expanded[start + 2..end].split_once(':').unwrap();
            let value = value.to_string();
            expanded.replace_range(start..=end, &value);
        }
        expanded
    }
    let server = LspServer::new();
    let uri = "untitled:declaration-snippets";
    for completions in [server.declaration_keyword_completions(uri), server.keyword_completions(uri)] {
        assert!(completions.iter().all(|item| !matches!(item.label.as_str(), "launch" | "lock_script" | "&mut")));
        for label in ["action", "flow", "lock", "invariant"] {
            let item = completions.iter().find(|item| item.label == label);
            let Some(item) = item else { continue };
            let source = format!("module snippets;\n{}", expand_defaults(item.insert_text.as_deref().unwrap()));
            let document = crate::frontend::parse_document(&source, crate::CURRENT_EDITION)
                .unwrap_or_else(|error| panic!("{label} snippet: {error}\n{source}"));
            let formatted = crate::fmt::format_source(&source, crate::CURRENT_EDITION, Default::default()).unwrap();
            let reparsed = crate::frontend::parse_document(&formatted, crate::CURRENT_EDITION).unwrap();
            assert!(document.ast.same_semantics(&reparsed.ast), "{label} snippet formatting changed semantics");
            assert_eq!(formatted, crate::fmt::format_source(&formatted, crate::CURRENT_EDITION, Default::default()).unwrap());
            if label == "action" {
                let crate::ast::Item::Action(action) = &document.ast.items[0] else { panic!("expected action") };
                assert_eq!(action.outputs.len(), 1);
                assert!(action.return_type.is_none());
                assert_eq!(action.params[0].source, crate::ast::ParamSource::Input);
            }
        }
    }
}

#[test]
fn distributed_editor_entry_and_flow_snippets_parse_under_the_native_source_contract() {
    use crate::ast::semantic_shape::SemanticEq;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/vscode-cellscript");
    let path = root.join("snippets/cellscript.json");
    if !path.is_file() {
        // The compiler supports checkouts with uninitialized editor submodules.
        // Consumer delivery separately requires an initialized, verified pin.
        return;
    }
    let snippets: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let defaults = regex::Regex::new(r"\$\{([0-9]+):([^{}]*)\}").unwrap();
    let references = regex::Regex::new(r"\$\{([0-9]+)\}").unwrap();
    for label in ["Action", "Lock", "Flow", "Flow Edge By Action", "Lock Script Policy", "Action with Cell Roles"] {
        let mut source =
            snippets[label]["body"].as_array().unwrap().iter().map(|line| line.as_str().unwrap()).collect::<Vec<_>>().join("\n");
        let mut values = std::collections::HashMap::<String, String>::new();
        for _ in 0..16 {
            let previous = source.clone();
            source = defaults
                .replace_all(&source, |captures: &regex::Captures<'_>| {
                    values.entry(captures[1].to_string()).or_insert_with(|| captures[2].to_string()).clone()
                })
                .into_owned();
            source = references
                .replace_all(&source, |captures: &regex::Captures<'_>| {
                    values.get(&captures[1]).cloned().unwrap_or_else(|| captures[0].to_string())
                })
                .replace("$0", "");
            if source == previous {
                break;
            }
        }
        assert!(!source.contains('$'), "unexpanded snippet {label}: {source}");
        let source = format!("module editor_snippets;\n{source}");
        let document = crate::frontend::parse_document(&source, crate::CURRENT_EDITION)
            .unwrap_or_else(|error| panic!("{label}: {error}\n{source}"));
        let formatted = crate::fmt::format_source(&source, crate::CURRENT_EDITION, Default::default()).unwrap();
        let reparsed = crate::frontend::parse_document(&formatted, crate::CURRENT_EDITION).unwrap();
        assert!(document.ast.same_semantics(&reparsed.ast), "{label}: formatter changed AST");
        assert_eq!(formatted, crate::fmt::format_source(&formatted, crate::CURRENT_EDITION, Default::default()).unwrap());
    }
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    let mut examples = 0;
    for block in readme.split("```cellscript\n").skip(1) {
        let source = format!("module editor_readme;\n{}", block.split("```").next().unwrap());
        crate::frontend::parse_document(&source, crate::CURRENT_EDITION).unwrap_or_else(|error| panic!("README: {error}\n{source}"));
        examples += 1;
    }
    assert!(examples >= 2, "the README entry examples must actually be exercised");
}

#[test]
fn named_runtime_snippets_use_declarations_and_respect_source_modes() {
    fn expand_defaults(snippet: &str) -> String {
        let mut expanded = snippet.to_string();
        while let Some(start) = expanded.find("${") {
            let end = start + expanded[start..].find('}').unwrap();
            let (_, value) = expanded[start + 2..end].split_once(':').unwrap();
            let value = value.to_string();
            expanded.replace_range(start..=end, &value);
        }
        expanded
    }
    let uri = "file:///named_snippets.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), "module snippets;\n".into(), crate::CURRENT_EDITION);
    let declarations = server.declaration_keyword_completions(uri);
    let mut source = "module snippets;\nresource Order has store { id: u64, }\n".to_string();
    for name in ["cell_roles", "data_shape", "collections"] {
        let item = declarations.iter().find(|item| item.label == name).unwrap();
        source.push_str(&expand_defaults(item.insert_text.as_ref().unwrap()));
        source.push('\n');
        assert!(server.keyword_completions(uri).iter().any(|item| item.label == name));
    }
    server.open_document_with_edition(uri.into(), source.clone(), crate::CURRENT_EDITION);
    let calls = server.member_completions(uri, "ckb");
    let call = |name: &str| {
        let snippet = calls.iter().find(|item| item.label == name).unwrap().insert_text.as_ref().unwrap();
        assert_eq!(snippet, &format!("ckb::{name}::<${{1:Settlement}}>()"));
        expand_defaults(snippet)
    };
    source.push_str(&format!("action inspect(witness messages: Messages) -> u64 {{\n{};\nlet selected = {};\nrequire selected.orders.input_count() <= 32;\nrequire messages.len() <= 32;\nreturn 0;\n}}",
            call("verify_cell_roles"), call("select_cell_roles")));
    crate::compile_metadata(&source, crate::CURRENT_EDITION, None).unwrap();

    {
        let edition = crate::CURRENT_EDITION;
        let source = "module unified;\nresource Order { id: u64, }\nlock inspect(protected order: Order) { true }";
        server.open_document_with_edition(uri.into(), source.into(), edition);
        for items in [server.declaration_keyword_completions(uri), server.keyword_completions(uri)] {
            for name in ["cell_roles", "data_shape", "collections"] {
                assert!(items.iter().any(|item| item.label == name));
            }
        }
        assert!(server
            .member_completions(uri, "ckb")
            .iter()
            .find(|item| item.label == "select_cell_roles")
            .unwrap()
            .insert_text
            .as_ref()
            .unwrap()
            .contains("${1:Catalog}"));
    }
}

#[test]
fn collection_budget_hover_explains_shared_fit_and_growth_without_reserving() {
    let source = include_str!("../../examples/language/batches/multi_asset_transfer.cell");
    let uri = "untitled:budget-hover";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    let position = offset_to_position(source, source.find("collections Totals").unwrap() + 13);
    let hover = server.hover(uri, position).unwrap().contents;
    for expected in [
        "items` is a capacity limit, not an initial reservation",
        "Reserve all members once: 3712 bytes / 4 allocations — fits",
        "Grow all members incrementally: 7840 bytes / 24 allocations — does not fit",
        "shortfall: 3744 bytes / 20 allocation events",
        "not a program-path or worst-case proof",
    ] {
        assert!(hover.contains(expected), "missing {expected}: {hover}");
    }
    let generic = "module budgets;\nstruct Pair<T: fixed_value> { left: T, right: T, }\ncollections Work { limits { bytes: 256, allocations: 3 },\nrows: Vec<Pair<u64>> { items: 3 }\n}\naction inspect() -> u64 { return 0; }";
    server.open_document_with_edition(uri.into(), generic.into(), crate::CURRENT_EDITION);
    let position = offset_to_position(generic, generic.find("collections Work").unwrap() + 13);
    let hover = server.hover(uri, position).unwrap().contents;
    assert!(hover.contains("rows: 3 items x 16 bytes"), "{hover}");
    assert!(hover.contains("Reserve all members once: 80 bytes / 1 allocations — fits"), "{hover}");
    server.open_document_with_edition(uri.into(), generic.replace("Pair<u64>", "Unknown"), crate::CURRENT_EDITION);
    let hover = server.hover(uri, position).unwrap().contents;
    assert!(hover.contains("Budget estimate unavailable"), "{hover}");
    assert!(!hover.contains("Reserve all members once"), "{hover}");
}

#[test]
fn named_collections_have_navigation_and_bounded_contract_status() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-collections.cell");
    let uri = "file:///named_collections.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    let offset = source.find("Scratch").unwrap();
    let position = offset_to_position(source, offset + 1);
    assert!(server.hover(uri, position).unwrap().contents.contains("local operations and read-only helpers"));
    assert!(server.document_symbols(uri).iter().any(|symbol| symbol.name == "Scratch"));
    assert_eq!(server.goto_definition(uri, position).unwrap().range.start, offset_to_position(source, offset));
    assert_eq!(server.rename(uri, position, "ScratchValues".into()).get(uri).unwrap().len(), 1);
    assert!(server.keyword_completions(uri).iter().any(|item| item.label == "collections"));
    assert!(server
        .symbol_completions(server.ast_cache.get(uri).unwrap())
        .iter()
        .any(|item| item.label == "Scratch" && item.detail.as_deref().is_some_and(|detail| detail == "collections Scratch")));
    let constructed = source.replace("return 0;", "let scratch = Scratch::new();\nreturn 0;");
    server.open_document_with_edition(uri.into(), constructed, crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    let invalid = format!("{source}\nfn count(view: &Scratch) -> u64 {{ view.pending.push(1)\nreturn 0 }}");
    server.open_document_with_edition(uri.into(), invalid, crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().any(|item| item.severity == DiagnosticSeverity::Error));
    let sorted = include_str!("../../tests/syntax_combo/seeds/collections-sorted.cell");
    server.open_document_with_edition(uri.into(), sorted.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    let records = include_str!("../../tests/syntax_combo/seeds/collections-records.cell");
    server.open_document_with_edition(uri.into(), records.into(), crate::CURRENT_EDITION);
    let diagnostics = server.get_diagnostics(uri);
    assert!(diagnostics.iter().all(|item| item.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
    let reference = records.find("Vec<Pair<u64>>").unwrap() + "Vec<".len();
    assert_eq!(
        server.goto_definition(uri, offset_to_position(records, reference + 1)).unwrap().range.start,
        offset_to_position(records, records.find("Pair<T").unwrap())
    );
}

#[test]
fn multi_asset_transfer_navigates_collections_and_checks_borrowed_helpers() {
    let source = include_str!("../../examples/language/batches/multi_asset_transfer.cell");
    let uri = "file:///multi_asset_transfer.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    let diagnostics = server.get_diagnostics(uri);
    assert!(diagnostics.iter().all(|item| item.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
    let position = offset_to_position(source, source.find("Totals::new").unwrap() + 1);
    let definition = server.goto_definition(uri, position).unwrap();
    assert_eq!(definition.range.start, offset_to_position(source, source.find("Totals {").unwrap()));
    let renamed = server.rename(uri, position, "TransferTotals".into());
    assert_eq!(renamed.get(uri).unwrap().len(), 3);
    let invalid = source.replace("return true", "totals.incoming.reserve(32)\nreturn true");
    server.open_document_with_edition(uri.into(), invalid, crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().any(|item| item.severity == DiagnosticSeverity::Error));
}

#[test]
fn named_data_shapes_have_navigation_rename_and_inferred_view_completion() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-data-shapes.cell");
    let uri = "file:///named_shapes.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    let diagnostics = server.get_diagnostics(uri);
    assert!(diagnostics.iter().all(|item| item.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
    let reference = source.rfind("Messages").unwrap();
    let position = offset_to_position(source, reference + 1);
    assert!(server.completion(uri, position).iter().any(|item| item.label == "Messages"));
    let type_position = source.find("witness messages:").unwrap() + "witness messages:".len();
    assert!(server.completion(uri, offset_to_position(source, type_position)).iter().any(|item| item.label == "Messages"));
    assert!(server.keyword_completions(uri).iter().any(|item| item.label == "data_shape"));
    assert!(server.hover(uri, position).unwrap().contents.contains("encoded bytes"));
    assert_eq!(
        server.goto_definition(uri, position).unwrap().range.start,
        offset_to_position(source, source.find("Messages").unwrap())
    );
    assert_eq!(server.rename(uri, position, "MessageBatch".into()).get(uri).unwrap().len(), 2);
    for (needle, expected) in [("entry.id", "id"), ("entry.payload.len", "len")] {
        let offset = source.find(needle).unwrap() + needle.rfind('.').unwrap() + 1;
        let items = server.completion(uri, offset_to_position(source, offset));
        assert!(items.iter().any(|item| item.label == expected), "{needle}: {items:?}");
    }
}

#[test]
fn named_nested_helpers_complete_views_inside_predicate_bodies() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-nested-helpers.cell");
    let uri = "file:///nested_helpers.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    let position = source.find("entry.payload").unwrap() + "entry.".len();
    assert!(server.completion(uri, offset_to_position(source, position)).iter().any(|item| item.label == "payload"));
    let parameter = source.find("values: Messages").unwrap() + "values: ".len() + 1;
    assert_eq!(
        server.goto_definition(uri, offset_to_position(source, parameter)).unwrap().range.start,
        offset_to_position(source, source.find("Messages").unwrap())
    );
}

#[test]
fn nested_specializations_resolve_qualified_phantom_arguments_in_revised_documents() {
    let source = include_str!("../../tests/fixtures/nested_qualified_arguments.cell");
    let uri = "file:///nested_qualified_arguments.cell";
    let mut server = LspServer::new();
    for (source, accepted) in [
        (source.to_string(), true),
        (source.replace("Envelope<nested_qualified_arguments::Value>", "Envelope<Other>"), false),
        (source.to_string(), true),
    ] {
        server.open_document_with_edition(uri.into(), source, crate::CURRENT_EDITION);
        let errors =
            server.get_diagnostics(uri).into_iter().filter(|item| item.severity == DiagnosticSeverity::Error).collect::<Vec<_>>();
        assert_eq!(errors.is_empty(), accepted, "{errors:?}");
    }
}

#[test]
fn named_derived_helpers_complete_types_fields_and_shape_rename() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-derived-helpers.cell");
    let uri = "file:///derived_helpers.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    let at = source.find("Entries::Payload").unwrap() + "Entries::".len();
    let items = server.completion(uri, offset_to_position(source, at));
    for expected in ["Item", "Payload"] {
        assert!(items.iter().any(|item| item.label == expected), "{items:?}");
    }
    let at = source.find("entry.payload").unwrap() + "entry.".len();
    assert!(server.completion(uri, offset_to_position(source, at)).iter().any(|item| item.label == "payload"));
    let at = source.find("Entries::Payload").unwrap() + 1;
    assert_eq!(
        server.goto_definition(uri, offset_to_position(source, at)).unwrap().range.start,
        offset_to_position(source, source.find("Entries {").unwrap())
    );
    assert_eq!(server.rename(uri, offset_to_position(source, at), "Records".into()).get(uri).unwrap().len(), 5);
}

#[test]
fn named_cell_roles_complete_role_aliases_methods_and_data() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-role-selection.cell");
    let uri = "file:///named_selection.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    for (needle, expected) in [("selected.orders", "orders"), ("alias.input(count", "input"), ("before.data.amount", "amount")] {
        let last_dot = needle.rfind('.').unwrap();
        let offset = source.find(needle).unwrap() + last_dot + 1;
        let items = server.completion(uri, offset_to_position(source, offset));
        assert!(items.iter().any(|item| item.label == expected), "{needle}: {items:?}");
    }
}

#[test]
fn record_migration_completes_fields_and_renames_shape_across_helpers() {
    let source = include_str!("../../examples/language/batches/record_migration.cell");
    let uri = "file:///record_migration.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    let diagnostics = server.get_diagnostics(uri);
    assert!(diagnostics.iter().all(|item| item.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
    let at = source.find("before.payload").unwrap() + "before.".len();
    let items = server.completion(uri, offset_to_position(source, at));
    for field in ["revision", "payload"] {
        assert!(items.iter().any(|item| item.label == field), "{items:?}");
    }
    let at = source.find("Messages::Payload").unwrap() + 1;
    assert_eq!(
        server.goto_definition(uri, offset_to_position(source, at)).unwrap().range.start,
        offset_to_position(source, source.find("Messages {").unwrap())
    );
    let changes = server.rename(uri, offset_to_position(source, at), "Entries".into());
    let mut edits = changes[uri]
        .iter()
        .map(|edit| {
            (
                position_to_offset(source, edit.range.start).unwrap(),
                position_to_offset(source, edit.range.end).unwrap(),
                edit.new_text.as_str(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(edits.len(), source.matches("Messages").count());
    edits.sort_by_key(|edit| std::cmp::Reverse(edit.0));
    let mut renamed = source.to_string();
    for (start, end, replacement) in edits {
        renamed.replace_range(start..end, replacement);
    }
    crate::compile_metadata(&renamed, crate::CURRENT_EDITION, None).unwrap();
    server.open_document_with_edition(
        uri.into(),
        source.replace("before.revision != after.revision", "before.missing != after.revision"),
        crate::CURRENT_EDITION,
    );
    assert!(server.get_diagnostics(uri).iter().any(|item| item.severity == DiagnosticSeverity::Error));
}

#[test]
fn named_nested_data_views_complete_fields_and_keep_read_only_diagnostics() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-nested-data-views.cell");
    let uri = "file:///nested_data_views.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    let offset = source.find("input.messages").unwrap() + "input.".len();
    let items = server.completion(uri, offset_to_position(source, offset));
    for expected in ["messages", "id", "owner", "entries"] {
        assert!(items.iter().any(|item| item.label == expected), "{items:?}");
    }
    server.open_document_with_edition(
        uri.into(),
        source.replace("let messages = input.messages;", "let mut messages = input.messages;"),
        crate::CURRENT_EDITION,
    );
    assert!(server.get_diagnostics(uri).iter().any(|item| item.severity == DiagnosticSeverity::Error));
}

#[test]
fn named_nested_selection_has_diagnostics_and_coordinate_completions() {
    let source = include_str!("../../tests/syntax_combo/seeds/named-nested-selection.cell");
    let uri = "file:///named_nested_selection.cell";
    let mut server = LspServer::new();
    server.open_document_with_edition(uri.into(), source.into(), crate::CURRENT_EDITION);
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    for (needle, expected) in [("selected.orders", "orders"), ("orders.input(count", "input"), ("after.output_index", "output_index")]
    {
        let offset = source.find(needle).unwrap() + needle.rfind('.').unwrap() + 1;
        let items = server.completion(uri, offset_to_position(source, offset));
        assert!(items.iter().any(|item| item.label == expected), "{needle}: {items:?}");
    }
    server.open_document_with_edition(
        uri.into(),
        source.replace("return after.output_index", "return after.data.absent"),
        crate::CURRENT_EDITION,
    );
    assert!(server.get_diagnostics(uri).iter().any(|item| item.severity == DiagnosticSeverity::Error));
}

#[test]
fn test_incremental_change_applies_utf16_ranges_after_non_bmp_text() {
    let source = "module demo;\n// 😀 marker\n";
    let start = source.find("marker").expect("marker start");
    let end = start + "marker".len();
    let updated = apply_incremental_change(
        source,
        Range { start: offset_to_position(source, start), end: offset_to_position(source, end) },
        "done",
    );

    assert_eq!(updated, "module demo;\n// 😀 done\n");
}

#[test]
fn test_incremental_change_ignores_invalid_utf16_ranges() {
    let source = "module demo;\n// 😀 marker\n";

    let invalid_surrogate_middle = apply_incremental_change(
        source,
        Range { start: Position { line: 1, character: 4 }, end: Position { line: 1, character: 4 } },
        "bad",
    );
    assert_eq!(invalid_surrogate_middle, source);

    let reversed = apply_incremental_change(
        source,
        Range { start: Position { line: 1, character: 12 }, end: Position { line: 1, character: 8 } },
        "bad",
    );
    assert_eq!(reversed, source);
}

#[test]
fn test_lsp_server() {
    let mut server = LspServer::new();

    let uri = "file:///test.cell".to_string();
    let content = "module test;\n\naction answer() -> u64 {\n    \n        42\n}\n".to_string();

    server.open_document(uri.clone(), content);
    assert!(server.get_diagnostics(&uri).is_empty());

    let completions = server.completion(&uri, Position { line: 0, character: 0 });
    assert!(!completions.is_empty());

    let keywords: Vec<_> = completions.iter().filter(|c| c.kind == CompletionItemKind::Keyword).collect();
    assert!(!keywords.is_empty());
}

#[test]
fn lsp_rejects_oversized_documents_without_retaining_them() {
    let mut server = LspServer::new();
    let uri = "file:///oversized.cell".to_string();
    let source = " ".repeat(crate::MAX_SOURCE_BYTES + 1);

    server.open_document(uri.clone(), source.clone());
    assert!(!server.documents.contains_key(&uri));
    assert!(server.get_diagnostics(&uri)[0].message.contains("source exceeds"));

    server.open_document(uri.clone(), "module ok;".to_string());
    assert!(server.documents.contains_key(&uri));
    server.update_document(uri.clone(), source);
    assert!(!server.documents.contains_key(&uri));
    assert!(server.get_diagnostics(&uri)[0].message.contains("source exceeds"));
}

#[test]
fn test_keyword_completions() {
    let server = LspServer::new();
    let keywords = server.keyword_completions("file:///stable.cell");

    assert!(keywords.iter().any(|k| k.label == "module"));
    assert!(keywords.iter().any(|k| k.label == "resource"));
    assert!(keywords.iter().any(|k| k.label == "action"));
    assert!(keywords.iter().any(|k| k.label == "flow"));
    assert!(keywords.iter().any(|k| k.label == "input"));
    assert!(!keywords.iter().any(|k| k.label == "output"));
    assert!(keywords.iter().any(|k| k.label == "transition"));
    assert!(!keywords.iter().any(|k| k.label == "move"));
    assert!(keywords.iter().any(|k| k.label == "require"));
    assert!(keywords.iter().any(|k| k.label == "validity"));
    assert!(keywords.iter().any(|k| k.label == "fixed_value"));
    assert!(keywords.iter().any(|k| k.label == "forall"));
    assert!(keywords.iter().any(|k| k.label == "count"));
    assert!(keywords.iter().any(|k| k.label == "consume_each"));
    assert!(keywords.iter().any(|k| k.label == "create_each"));
    assert!(keywords.iter().any(|k| k.label == "break"));
    assert!(keywords.iter().any(|k| k.label == "continue"));
    assert!(keywords.iter().any(|k| k.label == "label"));
    assert!(!keywords.iter().any(|k| k.label == "transfer"));
    assert!(keywords.iter().any(|k| k.label == "std::cell::same_lock"));
    assert!(keywords.iter().any(|k| k.label == "std::cell::preserve_capacity"));
    assert!(keywords.iter().any(|k| k.label == "std::lifecycle::transfer"));
    assert!(keywords.iter().any(|k| k.label == "std::receipt::claim"));
    assert!(keywords.iter().any(|k| k.label == "std::lifecycle::settle"));
    assert!(keywords.iter().any(|k| k.label == "protected"));
    assert!(keywords.iter().any(|k| k.label == "witness"));
    assert!(keywords.iter().any(|k| k.label == "lock_args"));
    let types = server.type_completions();
    assert!(types.iter().any(|item| item.label == "BoundedCellSet"));
    assert!(types.iter().any(|item| item.label == "BoundedList"));
    assert!(types.iter().any(|item| item.label == "NestedValues"
        && item.documentation.as_deref().is_some_and(|doc| doc.contains("codec, source, transport and lifetime"))));
    assert!(types.iter().any(|item| item.label == "NestedBytesView"));
    assert!(types.iter().any(|item| item.label == "NestedRecordView"));
    assert!(types.iter().any(|item| item.label == "DecodedSince"));
    assert!(types.iter().any(|item| item.label == "SighashAllDigest"));
    assert!(types.iter().any(|item| item.label == "AbsoluteBlockSince"));
    assert!(types.iter().any(|item| item.label == "RelativeTimestampSince"));
}

#[test]
fn generic_hover_uses_the_canonical_fixed_value_profile() {
    let params = [TypeParam {
        uses_fixed_value_profile: true,
        name: "T".to_string(),
        constraints: ValueAbility::FIXED_VALUE_PROFILE.to_vec(),
        phantom: false,
        span: Span::default(),
    }];
    assert_eq!(generic_params_hover(&params), "<T: fixed_value>");
}

#[test]
fn test_ckb_namespace_completions() {
    let server = LspServer::new();

    let zk = server.member_completions("file:///test.cell", "zk");
    assert!(zk.iter().any(|item| item.insert_text.as_deref() == Some("zk::require_valid(${1:proof})")));
    let vk = server.member_completions("file:///test.cell", "VerificationKeyCommitment");
    assert!(vk.iter().any(|item| item.label == "from_bytes"));
    let types = server.type_completions();
    assert!(types.iter().any(|item| item.label == "ZkProof"));
    assert!(types.iter().any(|item| item.label == "ZkTransitionProof"));
    assert!(types.iter().any(|item| item.label == "VerificationKeyCommitment"));

    let env = server.member_completions("file:///test.cell", "env");
    assert!(env.iter().any(|item| item.label == "sighash_all"));
    assert!(env.iter().any(|item| item.label == "sighash_all_zero_lock"));

    let hash = server.member_completions("file:///test.cell", "Hash");
    assert!(hash.iter().any(|item| item.label == "from_sighash_all"));

    let source = server.member_completions("file:///test.cell", "source");
    assert!(source.iter().any(|item| item.label == "group_input"));

    let witness = server.member_completions("file:///test.cell", "witness");
    assert!(witness.iter().any(|item| item.label == "lock_exact32"));
    assert!(!witness.iter().any(|item| item.label == "lock"));
    assert!(witness.iter().any(|item| item.label == "bounded_entry"));
    assert!(witness.iter().any(|item| item.label == "blake2b"));
    assert!(witness.iter().any(|item| item.label == "blake2b_span"));
    assert!(witness.iter().any(|item| item.label == "bytes32"));

    let script = server.member_completions("file:///test.cell", "script");
    assert!(script.iter().any(|item| item.label == "new"));
    assert!(script.iter().any(|item| item.label == "args"));
    assert!(script.iter().any(|item| item.label == "hash"));
    assert!(script.iter().any(|item| item.label == "require_cell_lock_matches"));

    let hash = server.member_completions("file:///test.cell", "Hash");
    assert!(hash.iter().any(|item| item.label == "zero"));
    assert!(hash.iter().any(|item| item.label == "from_bytes"));

    let ckb = server.member_completions("file:///test.cell", "ckb");
    assert!(ckb.iter().any(|item| item.label == "cell_data_hash_field"));
    assert!(!ckb.iter().any(|item| item.label == "cell_data_hash"));
    assert!(ckb.iter().any(|item| item.label == "cell_data_bytes32"));
    assert!(ckb.iter().any(|item| item.label == "input_since"));
    assert!(ckb.iter().any(|item| item.label == "since_epoch_relative"));
    assert!(ckb.iter().any(|item| item.label == "since_absolute_epoch"));
    assert!(ckb.iter().any(|item| item.label == "since_relative_epoch"));
    assert!(ckb.iter().any(|item| item.label == "since_absolute_block"));
    assert!(ckb.iter().any(|item| item.label == "since_relative_block"));
    assert!(ckb.iter().any(|item| item.label == "since_absolute_timestamp"));
    assert!(ckb.iter().any(|item| item.label == "since_relative_timestamp"));
    assert!(ckb.iter().any(|item| item.label == "since_decode"));
    assert!(ckb.iter().any(|item| item.label == "since_from_raw_checked"));
    assert!(ckb.iter().any(|item| item.label == "since_as_absolute_epoch"));
    assert!(ckb.iter().any(|item| item.label == "since_metric"));
    assert!(ckb.iter().any(|item| item.label == "since_to_raw"));
    assert!(ckb.iter().any(|item| item.label == "epoch_duration"));
    assert!(ckb.iter().any(|item| item.label == "epoch_add"));
    assert!(ckb.iter().any(|item| item.label == "epoch_sub"));
    assert!(ckb.iter().any(|item| item.label == "epoch_number_to_u64"));
    assert!(ckb.iter().any(|item| item.label == "epoch_duration_to_u64"));
    assert!(ckb.iter().any(|item| item.label == "timestamp_millis_to_u64"));
    assert!(ckb.iter().any(|item| item.label == "cell_lock_code_hash"));
    assert!(ckb.iter().any(|item| item.label == "cell_type_args32"));
    assert!(ckb.iter().any(|item| item.label == "require_cell_lock_args_prefix32"));
    assert!(ckb.iter().any(|item| item.label == "require_cell_type_args_suffix32"));
    assert!(ckb.iter().any(|item| item.label == "require_cell_lock_args_empty"));
    assert!(ckb.iter().any(|item| item.label == "require_bounded_cell_dep_data_hash"));
    assert!(ckb.iter().any(|item| item.label == "hash_sha256d"));
    assert!(ckb.iter().any(|item| item.label == "require_sha256d_merkle_root"));
    assert!(ckb.iter().any(|item| item.label == "raw_transaction_hash_without_cell_deps"));
    assert!(ckb.iter().any(|item| item.label == "transaction_hash"));
    assert!(ckb.iter().any(|item| item.label == "trusted_exec_cell_dep_u8_args"));
    assert!(ckb.iter().any(|item| item.label == "trusted_spawn_wait_cell_dep_hex4"));

    let header = server.member_completions("file:///test.cell", "HeaderDepView");
    assert!(header.iter().any(|item| item.label == "block_number"));
    assert!(header.iter().any(|item| item.label == "timestamp"));

    let bip340 = server.member_completions("file:///test.cell", "verifier::btc::bip340");
    assert!(bip340.iter().any(|item| item.label == "require_signature_from_cell_dep"));

    let dao = server.member_completions("file:///test.cell", "dao");
    assert!(dao.iter().any(|item| item.label == "require_input_relative_epoch_since_at_least"));
}

#[test]
fn test_fixed_cell_data_view_editor_contract() {
    let uri = "file:///cell-data-view.cell";
    let mut server = LspServer::new();
    server.open_document(uri.into(), include_str!("../../examples/language/ckb/typed_cell_data.cell").into());
    let diagnostics = server.get_diagnostics(uri);
    assert!(diagnostics.iter().all(|item| item.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
    assert!(!diagnostics.iter().any(|item| item.message.contains("cell-data-view-artifact-proof")));
    assert!(!diagnostics.iter().any(|item| item.message.contains("requires experimental artifact admission")));
    assert!(diagnostics.iter().all(|item| !item.message.contains("emits fail-closed runtime traps")));
    let input = server.member_completions(uri, "InputView<Token>");
    assert!(input.iter().any(|field| field.label == "data"));
    assert!(input.iter().any(|field| field.label == "out_point"));
    let output = server.member_completions(uri, "OutputView<Token>");
    assert!(output.iter().any(|field| field.label == "output_index"));
    assert!(!output.iter().any(|field| field.label == "out_point"));
    let data = server.member_completions(uri, "CellDataView<Token>");
    assert_eq!(data.iter().map(|field| field.label.as_str()).collect::<Vec<_>>(), ["id", "amount", "owner", "large", "bytes"]);
    let source = include_str!("../../examples/language/ckb/typed_cell_data.cell");
    for (prefix, expected) in [("input.", "data"), ("alias.", "amount"), ("output.data.", "owner")] {
        let offset = source.find(prefix).unwrap() + prefix.len();
        let completion = server.completion(uri, offset_to_position(source, offset));
        assert!(completion.iter().any(|item| item.label == expected), "{prefix}: {completion:?}");
    }
}

#[test]
fn test_fixed_cell_record_array_completion() {
    let uri = "file:///fixed-cell-record-array.cell";
    let source = include_str!("../../tests/syntax_combo/seeds/fixed-record-cell-data.cell");
    let mut server = LspServer::new();
    server.open_document(uri.into(), source.into());
    assert!(server.get_diagnostics(uri).iter().all(|item| item.severity != DiagnosticSeverity::Error));
    for (prefix, label) in [("before.", "rows"), ("row.", "amount")] {
        let offset = source.find(prefix).unwrap() + prefix.len();
        let completion = server.completion(uri, offset_to_position(source, offset));
        assert!(completion.iter().any(|item| item.label == label), "{completion:?}");
    }
    let draft = source.replace("row.amount", "before.rows[index].");
    let offset = draft.find("before.rows[index].").unwrap() + "before.rows[index].".len();
    server.open_document(uri.into(), draft.clone());
    let completion = server.completion(uri, offset_to_position(&draft, offset));
    assert!(completion.iter().any(|item| item.label == "amount"), "{completion:?}");
}

#[test]
fn test_cell_view_helper_parameter_completions_keep_nominal_fields() {
    let uri = "file:///cell-helper-completion.cell";
    let mut server = LspServer::new();
    for (parameter, expression, expected) in [
        ("InputView<Token>", "view.data.", "owner"),
        ("OutputView<Token>", "view.data.", "owner"),
        ("InputView<Token>", "view.", "out_point"),
        ("OutputView<Token>", "view.", "capacity"),
    ] {
        let marked = format!("module helper_completion;\nresource Token has store {{ id: u64, owner: Hash }}\nfn inspect(view: {parameter}) -> u64 {{ return {expression}| }}");
        let offset = marked.find('|').unwrap();
        let source = marked.replace('|', "");
        let position = offset_to_position(&source, offset);
        server.open_document(uri.into(), source);
        let completion = server.completion(uri, position);
        assert!(completion.iter().any(|item| item.label == expected), "{parameter}: {completion:?}");
    }
}

#[test]
fn test_cell_view_completions_recover_trailing_dot_and_preserve_scope() {
    let uri = "file:///cell-data-completion.cell";
    let mut server = LspServer::new();
    for (body, expected) in [
        ("let view = ckb::input::<Token>(0);\nreturn view.data.|", Some("owner")),
        ("let view = ckb::selected_successor::<Token>(selection, 0, 0);\nlet alias = view.data;\nreturn alias.|", Some("owner")),
        ("if true { let view = ckb::output::<Token>(0);\nreturn view.| }\nreturn 0;", Some("output_index")),
        ("if true { let view = ckb::output::<Token>(0); }\nreturn view.|", None),
        ("let view = ckb::input::<Token>(0);\nif true { let view = 7;\nreturn view.| }\nreturn 0;", None),
        ("let view = ckb::input::<Token>(0);\nreturn later.|;\nlet later = view.data;", None),
    ] {
        let marked = format!(
            "module data_completion;\nresource Token has store {{ id: u64, owner: Hash, }}\naction inspect() -> u64 {{ \n{body}\n}}"
        );
        let offset = marked.find('|').unwrap();
        let source = marked.replace('|', "");
        let position = offset_to_position(&source, offset);
        server.open_document(uri.into(), source);
        let before = server.get_diagnostics(uri);
        let completion = server.completion(uri, position);
        if let Some(expected) = expected {
            assert!(completion.iter().any(|item| item.label == expected), "{body}: {completion:?}");
        } else {
            assert!(completion.is_empty(), "out-of-scope view: {body}: {completion:?}");
        }
        assert_eq!(serde_json::to_value(server.get_diagnostics(uri)).unwrap(), serde_json::to_value(before).unwrap());
    }
}

#[test]
fn test_payload_enum_namespace_completions_expose_constructor_shape() {
    let source = "module payload_completion;\nenum Limit { None, Some(u64), }\n";
    let tokens = crate::lexer::lex(source).unwrap();
    let module = crate::parser::parse(&tokens).unwrap();
    let server = LspServer::new();
    let completions = server.namespace_symbol_completions(&module, "Limit");
    let some = completions.iter().find(|item| item.label == "Some").expect("Some completion");
    assert_eq!(some.insert_text.as_deref(), Some("Some(value1)"));
    assert!(some.detail.as_deref().is_some_and(|detail| detail.contains("Some(u64)")));
    let none = completions.iter().find(|item| item.label == "None").expect("None completion");
    assert_eq!(none.insert_text.as_deref(), Some("None"));
}

#[test]
fn test_vec_member_completions_match_supported_helpers() {
    let server = LspServer::new();
    let completions = server.member_completions("file:///test.cell", "Vec");
    let labels = completions.iter().map(|item| item.label.as_str()).collect::<std::collections::BTreeSet<_>>();

    for helper in [
        "new",
        "with_capacity",
        "capacity",
        "push",
        "extend_from_slice",
        "len",
        "is_empty",
        "first",
        "last",
        "contains",
        "set",
        "remove",
        "pop",
        "insert",
        "reverse",
        "truncate",
        "swap",
        "clear",
    ] {
        assert!(labels.contains(helper), "missing Vec completion for {helper}");
    }
    assert!(!labels.contains("get"), "Vec completion should not advertise unsupported get()");
}

#[test]
fn test_flow_u8_namespace_completions() {
    let mut server = LspServer::new();
    let uri = "file:///flow_completion.cell".to_string();
    let source = r#"
module flow_completion;

receipt Ticket has store {
    state: u8,
    id: u64,
}

receipt OtherTicket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
    Active -> Closed;
}

flow OtherTicket.state {
    Draft -> Live;
}

action activate(input ticket: Ticket,  output active_ticket: Ticket) {
    transition ticket.state: Created -> active_ticket.state: Active;

        require ticket.state < Ticket::Closed, "closed";
        require active_ticket.state == Ticket::Active;
        require active_ticket.id == ticket.id;
}
"#
    .to_string();

    server.open_document(uri.clone(), source.clone());
    assert!(server.get_diagnostics(&uri).is_empty());

    let offset = source.find("Ticket::Active").expect("qualified state") + "Ticket::".len();
    let completions = server.completion(&uri, offset_to_position(&source, offset));
    let labels = completions.iter().map(|item| item.label.as_str()).collect::<std::collections::BTreeSet<_>>();

    assert!(labels.contains("Created"));
    assert!(labels.contains("Active"));
    assert!(labels.contains("Closed"));
    assert!(!labels.contains("Live"), "Ticket:: completion must not leak OtherTicket flow states");
    assert!(completions.iter().any(|item| {
        item.label == "Active"
            && item.kind == CompletionItemKind::EnumMember
            && item.detail.as_deref() == Some("flow state Ticket::Active")
    }));
}

#[test]
fn test_flow_namespace_completions() {
    let mut server = LspServer::new();
    let uri = "file:///flow_completion.cell".to_string();
    let source = r#"
module flow_completion;

enum OfferState {
    Created,
    Live,
    Filled,
}

resource Offer has store {
    state: OfferState,
    amount: u64,
}

flow OfferFlow for Offer.state {
    Created -> Live;
    Live -> Filled by accept;
}

action accept(input input: Offer,  output output: Offer) {
    transition input.state: Live -> output.state: Filled;

        require output.state == Offer::Filled;
        require output.amount == input.amount;
}
"#
    .to_string();

    server.open_document(uri.clone(), source.clone());
    assert!(server.get_diagnostics(&uri).is_empty());

    let offset = source.find("Offer::Filled").expect("qualified state") + "Offer::".len();
    let completions = server.completion(&uri, offset_to_position(&source, offset));
    let labels = completions.iter().map(|item| item.label.as_str()).collect::<std::collections::BTreeSet<_>>();

    assert!(labels.contains("Created"));
    assert!(labels.contains("Live"));
    assert!(labels.contains("Filled"));
    assert!(completions.iter().any(|item| {
        item.label == "Filled"
            && item.kind == CompletionItemKind::EnumMember
            && item.detail.as_deref() == Some("flow state Offer::Filled")
    }));
}

#[test]
fn test_parse_errors_become_diagnostics() {
    let mut server = LspServer::new();
    let uri = "file:///bad.cell".to_string();
    server.open_document(uri.clone(), "module bad;\naction broken( {\n".to_string());
    let diagnostics = server.get_diagnostics(&uri);
    assert!(!diagnostics.is_empty());
    assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
}

#[test]
fn parse_failure_drops_the_previous_successful_ast() {
    let mut server = LspServer::new();
    let uri = "file:///changing.cell".to_string();
    server.open_document(uri.clone(), "module changing;\naction ok() -> bool {  true }\n".to_string());
    assert!(server.ast_cache.contains_key(&uri));

    server.update_document(uri.clone(), "module changing;\naction broken( {\n".to_string());

    assert!(!server.ast_cache.contains_key(&uri));
    assert!(!server.get_diagnostics(&uri).is_empty());
}

#[test]
fn compiler_diagnostic_codes_keep_their_lsp_documentation_link() {
    let error = CompileError::without_span("instruction encoding failed").with_code(crate::error::DiagnosticCode::E2202);
    let diagnostic = diagnostic_from_error("", &error);

    assert_eq!(diagnostic.code.as_deref(), Some("E2202"));
    assert!(diagnostic.code_description.as_deref().is_some_and(|url| url.ends_with("#e2202")));
}

#[test]
fn test_parse_recovery_collects_multiple_diagnostics() {
    let mut server = LspServer::new();
    let uri = "file:///multi_parse_errors.cell".to_string();
    let source = r#"
module multi_parse_errors;

action bad() -> bool {

        let first: u64 true
        let second: bool 1
        return true;
}
"#;
    server.open_document(uri.clone(), source.to_string());
    let diagnostics = server.get_diagnostics(&uri);
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("expected '=', found 'true'")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("expected '=', found integer 1")));
}

#[test]
fn test_type_errors_collect_multiple_diagnostics() {
    let mut server = LspServer::new();
    let uri = "file:///multi_errors.cell".to_string();
    let source = r#"
module multi_errors;

action bad_one() -> u64 {

        return true;
}

action bad_two() -> bool {

        return 1;
}
"#;
    server.open_document(uri.clone(), source.to_string());
    let diagnostics = server.get_diagnostics(&uri);
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("expected U64, found Bool")));
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains("expected Bool, found U64")));
}

#[test]
fn compiler_warning_diagnostics_keep_warning_severity() {
    let error = CompileError::warning("compatibility note", Span::new(0, 4, 1, 1));
    let diagnostic = diagnostic_from_error("note", &error);
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
}

#[test]
fn test_goto_definition_and_references() {
    let mut server = LspServer::new();
    let uri = "file:///defs.cell".to_string();
    let source =
            "module defs;\n\nresource Token {\n    amount: u64,\n}\n\naction make() -> u64 {\n    \n        let token = new Token { amount: 1, };\n        token.amount\n}\n";
    server.open_document(uri.clone(), source.to_string());

    let definition =
        server.goto_definition(&uri, offset_to_position(source, source.find("new Token").unwrap() + 5)).expect("definition");
    assert_eq!(definition.range.start.line, 2);
    assert_eq!(definition.range.start.character, 9);
    assert_eq!(definition.range.end.character, 14);

    let refs = server.find_references(&uri, offset_to_position(source, source.find("new Token").unwrap() + 5));
    assert!(refs.len() >= 2);
}

#[test]
fn test_hover() {
    let mut server = LspServer::new();
    let uri = "file:///hover.cell".to_string();
    let source = "module hover;\n\naction demo(witness x: u64)->u64 {\n    \n        x\n}\n";
    server.open_document(uri.clone(), source.to_string());

    let hover = server.hover(&uri, Position { line: 2, character: 7 }).expect("hover");
    assert!(hover.contents.contains("action demo"));
}

#[test]
fn test_action_hover_includes_lowering_metadata() {
    let mut server = LspServer::new();
    let uri = "file:///metadata_hover.cell".to_string();
    let source = r#"
module metadata_hover;

shared Config {
    threshold: u64,
}

resource Token has store, create, consume, replace, burn, relock {
    amount: u64,
}

action update(witness amount: u64) -> u64 {

        let cfg = read_ref::<Config>();
        let token = create Token { amount: amount, };
        consume token;
        return cfg.threshold;
}
"#;
    server.open_document(uri.clone(), source.to_string());

    let hover = server.hover(&uri, Position { line: 11, character: 8 }).expect("hover");
    assert!(hover.contents.contains("Lowering metadata"));
    assert!(hover.contents.contains("ELF compatible: `true`"));
    // This action uses read_ref + consume, which require CKB runtime,
    // so standalone runner is not compatible.
    assert!(hover.contents.contains("Standalone runner compatible: `false`"));
    assert!(hover.contents.contains("Fail-closed runtime features: `none"));
    assert!(hover.contents.contains("CKB runtime features: `consume-input-cell, read-cell-dep, verify-output-cell`"));
    assert!(hover.contents.contains("consume:Input#0"));
    assert!(hover.contents.contains("read_ref:CellDep#0"));
    assert!(hover.contents.contains("create:Output#0"));
    assert!(hover.contents.contains("Verifier obligations"));
    assert!(hover.contents.contains("cell-access:consume:Input#0 (ckb-runtime)"));
}

#[test]
fn lifecycle_role_hover_uses_the_selected_artifact_policy_projection() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cell.toml"),
        r#"[package]
name = "policies"
version = "0.1.0"
edition = "2027"
cellscript_version = ">=0.33.0-alpha.1"
entry = "src/main.cell"
[[artifacts]]
name = "fixed"
context = { kind = "type-group" }
bindings = "group-relative"
coverage = "fixed-group"
layout = "Token"
dispatch = "single-entry"
entry = "verify"
[[artifacts]]
name = "manual"
context = { kind = "type-group" }
bindings = "transaction-absolute"
coverage = "action-defined"
dispatch = "single-entry"
entry = "verify"
"#,
    )
    .unwrap();
    let source = include_str!("../../tests/syntax_combo/authoring/absolute-fresh.cell");
    let path = root.join("src/main.cell");
    std::fs::write(&path, source).unwrap();
    let uri = utf8_path_to_file_uri(&path);
    let mut server = LspServer::new();
    for (artifact, selector, category) in
        [("fixed", "group-output[0]", "derived-and-enforced"), ("manual", "output[0]", "unconstrained-by-this-relation")]
    {
        server.artifact_name = Some(Arc::from(artifact));
        server.open_document(uri.clone(), source.into());
        assert!(server.get_diagnostics(&uri).is_empty(), "{:?}", server.get_diagnostics(&uri));
        let report = workspace::WorkspaceInputs::capture(&server, &uri).diagnostics();
        assert!(report.metadata.is_some(), "{artifact}: {:?}", report.diagnostics);
        for offset in
            [source.find("verify(").unwrap() + 1, source.find("output after").unwrap() + 8, source.find("fresh after").unwrap() + 7]
        {
            let hover = server.hover(&uri, offset_to_position(source, offset)).unwrap();
            assert!(hover.contents.contains("Resolved Cell policy"), "{}", hover.contents);
            assert!(hover.contents.contains(selector), "{}", hover.contents);
            assert!(hover.contents.contains(&format!("type-script: ` {category} `")), "{}", hover.contents);
            assert!(hover.contents.contains("capacity: ` unconstrained-by-this-relation `"), "{}", hover.contents);
        }
    }
}

#[test]
fn test_receipt_hover_includes_flow_metadata() {
    let mut server = LspServer::new();
    let uri = "file:///flow_hover.cell".to_string();
    let source = r#"
module flow_hover;

receipt Ticket has store {
    state: u8,
    id: u64,
}

flow Ticket.state {
    Created -> Active;
}

action activate(input ticket: Ticket,  output active_ticket: Ticket) {
    transition ticket.state: Created -> active_ticket.state: Active;

        let active = Ticket::Active;
        require active_ticket.state == active;
        require active_ticket.id == ticket.id;
}
"#;
    server.open_document(uri.clone(), source.to_string());

    let offset = source.find("Ticket has").expect("receipt name");
    let hover = server.hover(&uri, offset_to_position(source, offset)).expect("hover");
    assert!(hover.contents.contains("receipt Ticket"));
    assert!(hover.contents.contains("Flow metadata"));
    assert!(hover.contents.contains("States: `Created -> Active`"));
    assert!(hover.contents.contains("Created[0] -> Active[1]"));
}

#[test]
fn test_type_hover_includes_validity_evidence() {
    let mut server = LspServer::new();
    let uri = "file:///validity_hover.cell".to_string();
    let source = r#"
module validity_hover;

resource Token has store, create {
    amount: u64,
    height: u64,

    validity {
        require amount > 0;
        require height > env::block_number(); }
}

action mint(witness amount: u64, witness height: u64) -> Token {

        return create Token { amount: amount, height: height, };
}
"#;
    server.open_document(uri.clone(), source.to_string());

    let offset = source.find("Token has").expect("resource name");
    let hover = server.hover(&uri, offset_to_position(source, offset)).expect("hover");
    assert!(hover.contents.contains("Validity metadata"));
    assert!(hover.contents.contains("`amount > 0` — `checked-runtime`"));
    assert!(hover.contents.contains("create: `checked-runtime`"));
    assert!(hover.contents.contains("`height > env::block_number()` — `builder-evidence-required`"));
    assert!(hover.contents.contains("create: `builder-header-evidence-required`"));
}

#[test]
fn test_lowering_diagnostics_warn_for_fail_closed_runtime_actions() {
    let mut server = LspServer::new();
    let uri = "file:///metadata_diagnostic.cell".to_string();
    let source = r#"
module metadata_diagnostic;

shared Config {
    threshold: u64,
}

resource Token has store, create, consume, replace, burn, relock {
    amount: u64,
}

action update(witness amount: u64) -> u64 {

        let cfg = read_ref::<Config>();
        let token = create Token { amount: amount, };
        consume token;
        return cfg.threshold;
}
"#;
    server.open_document(uri.clone(), source.to_string());

    let diagnostics = server.get_diagnostics(&uri);
    // consume/create/read_ref now have real verifier lowering, so this program
    // is ELF-compatible and no longer triggers a lowering diagnostic.
    let lowering_warning = diagnostics.iter().find(|diagnostic| diagnostic.source == "cellscript-lowering");
    assert!(lowering_warning.is_none(), "consume/create/read_ref should not produce lowering warning: {:?}", lowering_warning);
}

#[test]
fn test_lowering_diagnostics_explain_deferred_runtime_in_locks_and_helpers() {
    let fixtures = [
            (
                "lock 'unlock'",
                "lock:unlock",
                "lock unlock",
                "module deferred_digest;\nlock unlock() -> bool {  let value = env::sighash_all(source::group_input(0)); return value == value; }\n",
            ),
            (
                "fn 'digest'",
                "fn:digest",
                "fn digest",
                "module deferred_digest;\nfn digest() -> Hash { return env::sighash_all(source::group_input(0)); }\n",
            ),
            (
                "fn 'digest'",
                "fn:digest",
                "fn digest",
                "module deferred_digest;\nfn digest() -> Hash { return env::sighash_all(source::group_input(0)); }\nlock unlock() -> bool {  let value = digest(); return value == value; }\n",
            ),
        ];
    {
        let edition = crate::CURRENT_EDITION;
        for (callable, scope, declaration, source) in fixtures {
            let source = source.replace("verification", "");
            let metadata = crate::compile_metadata(&source, edition, None).expect("audit metadata remains available");
            let obligation = metadata
                .runtime
                .verifier_obligations
                .iter()
                .find(|obligation| {
                    obligation.scope == scope && obligation.feature == "ckb-sighash-all-deferred" && obligation.status == "fail-closed"
                })
                .expect("shared metadata must explain the deferred operation");
            let mut server = LspServer::new();
            let uri = "file:///deferred_digest.cell".to_string();
            server.open_document_with_edition(uri.clone(), source.clone(), edition);
            let diagnostics = server.get_diagnostics(&uri);
            assert!(diagnostics.iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error), "{diagnostics:?}");
            let warning = diagnostics
                .iter()
                .find(|diagnostic| diagnostic.source == "cellscript-lowering" && diagnostic.message.contains(callable))
                .expect("a lock or helper must expose its unsupported runtime operation");
            assert_eq!(warning.severity, DiagnosticSeverity::Warning);
            assert!(warning.message.contains(&obligation.feature), "{warning:?}");
            assert!(warning.message.contains(&obligation.detail), "{warning:?}");
            assert_eq!(warning.range.start, offset_to_position(&source, source.find(declaration).unwrap()));
            assert!(warning.range.end.line >= warning.range.start.line);
        }
    }
}

#[test]
fn test_code_actions_for_lowering_diagnostics() {
    let mut server = LspServer::new();
    let uri = "file:///metadata_action.cell".to_string();
    let diagnostic_range = Range { start: Position { line: 2, character: 0 }, end: Position { line: 5, character: 1 } };
    server.diagnostics.insert(
        uri.clone(),
        vec![Diagnostic {
            range: diagnostic_range,
            severity: DiagnosticSeverity::Warning,
            code: None,
            code_description: None,
            message: "action emits fail-closed runtime traps".to_string(),
            source: "cellscript-lowering".to_string(),
        }]
        .into(),
    );

    let actions = server.code_action(&uri, diagnostic_range);
    assert!(actions.iter().any(|action| action.title.contains("cellc metadata")));
    assert!(actions.iter().any(|action| action.title.contains("cellc resolve-graph")));
    assert!(actions.iter().any(|action| action.title.contains("cellc build-plan")));
    assert!(actions.iter().any(|action| action.title.contains("cellc update-plan")));
    assert!(actions.iter().any(|action| action.title.contains("riscv64-asm")));
    assert!(actions.iter().all(|action| action.edit.is_none()));
}

#[test]
fn temporal_warning_offers_a_complete_mechanical_workspace_edit() {
    let mut server = LspServer::new();
    let uri = "file:///temporal_migration.cell".to_string();
    let source = "module temporal_migration;\nfn epoch() -> u64 { return env::current_timepoint(); }\n";
    server.open_document(uri.clone(), source.to_string());
    let warning = server
        .get_diagnostics(&uri)
        .into_iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("W3012"))
        .expect("legacy temporal API must have a targeted warning");
    let actions = server.code_action(&uri, warning.range);
    let action = actions
        .iter()
        .find(|action| action.title.contains("legacy CKB temporal call"))
        .expect("warning must offer a mechanical migration action");
    let edit = action.edit.as_ref().expect("migration action must contain a workspace edit");
    let edits = edit.changes.get(&uri).expect("migration edit must target the source document");
    assert_eq!(edits.len(), 1);
    assert!(edits[0].new_text.contains("ckb::epoch_number_to_u64(ckb::header_dep(0).epoch_number)"));
    assert!(crate::frontend::legacy_temporal_migration_diagnostics(&edits[0].new_text).is_empty());
}

#[test]
fn test_format_document() {
    let mut server = LspServer::new();
    let uri = "file:///fmt.cell".to_string();
    let source = "module fmt;\naction demo(witness x:u64)->u64 {\n\nx\n}\n";
    server.open_document(uri.clone(), source.to_string());

    let edits = server.format_document(&uri);
    assert_eq!(edits.len(), 1);
    assert!(edits[0].new_text.contains("action demo(witness x: u64) -> u64 {\n    x\n}"));
    assert!(!edits[0].new_text.contains("verification"));
}

#[test]
fn formatting_preserves_explicit_ability_bounds() {
    let mut server = LspServer::new();
    let uri = "file:///legacy-format.cell".to_string();
    let source = "module legacy;\nstruct Box<T: copy + drop + store + fixed + serializable + non_linear> has copy, drop, store, fixed, serializable, non_linear { value: T, }\n";
    server.open_document(uri.clone(), source.to_string());
    let edits = server.format_document(&uri);
    assert_eq!(edits.len(), 1);
    assert!(!edits[0].new_text.contains("fixed_value"));
    assert!(edits[0]
        .new_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains("> has copy, drop, store, fixed, serializable, non_linear"));
}

#[test]
fn fixed_group_action_resolves_the_artifact_in_its_own_module() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(root.join("Cell.toml"), include_str!("../../examples/semantic-foundation-2027/Cell.toml")).unwrap();
    let entry_source = include_str!("../../examples/semantic-foundation-2027/src/main.cell");
    let helper_source = "module helper;\nfn identity(value: u64) -> u64 { return value; }\n";
    let mut server = LspServer::new();
    for (name, source, bound) in [("main.cell", entry_source, true), ("helper.cell", helper_source, false)] {
        let path = root.join("src").join(name);
        std::fs::write(&path, source).unwrap();
        let uri = utf8_path_to_file_uri(&path);
        server.open_document(uri.clone(), source.into());
        let report = workspace::WorkspaceInputs::capture(&server, &uri).diagnostics();
        let metadata = report.metadata.unwrap_or_else(|| panic!("{name}: {:?}", report.diagnostics));
        let expected_entry = if bound { "action:transfer" } else { "none" };
        assert_eq!(metadata.typed_semantics.foundation.entry_contract.exact_entry, expected_entry);
    }
}

#[test]
fn counter_proof_policy_uses_the_captured_package_in_workspace_diagnostics() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir(root.join("src")).unwrap();
    let manifest = include_str!("../../examples/zk/plonkish-lock/counter/Cell.toml.in")
        .replace("@SCHEME@", "groth16")
        .replace("@PROFILE@", "groth16-bn254-counter-hex4-v1")
        .replace("@CHILD_HASH@", &"11".repeat(32))
        .replace("@KEY@", &"22".repeat(32))
        .replace("@DOMAIN@", &"33".repeat(32))
        .replace("@ACTION@", &"44".repeat(32));
    std::fs::write(root.join("Cell.toml"), &manifest).unwrap();
    let source = include_str!("../../examples/zk/plonkish-lock/counter/main.cell");
    let path = root.join("src/main.cell");
    std::fs::write(&path, source).unwrap();
    let uri = utf8_path_to_file_uri(&path);
    let mut server = LspServer::new();
    server.open_document(uri.clone(), source.into());
    let captured = workspace::WorkspaceInputs::capture(&server, &uri);
    std::fs::write(root.join("Cell.toml"), manifest.replace("CounterIncrement", "OtherPolicy")).unwrap();
    let report = captured.diagnostics();
    assert_eq!(report.metadata.as_ref().unwrap().actions[0].params[0].fixed_byte_len, Some(128));
    server.update_document(uri.clone(), source.into());
    assert!(server.get_diagnostics(&uri).iter().any(|d| d.message.contains("CounterIncrement")));
}

#[test]
fn unknown_manifest_edition_blocks_workspace_metadata_and_semantic_hover() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    let manifest_path = root.join("Cell.toml");
    let manifest = format!(
            "[package]\ncellscript_version = \">={}\"\nedition = \"2027\"\nname = \"unknown_edition_package\"\nversion = \"0.1.0\"\nentry = \"src/main.cell\"\n",
            crate::VERSION
        );
    std::fs::write(&manifest_path, &manifest).unwrap();
    let source = "module unknown_edition_package;\naction verify(witness value: u64) -> u64 { return value; }\n";
    let source_path = root.join("src/main.cell");
    std::fs::write(&source_path, source).unwrap();
    let uri = utf8_path_to_file_uri(&source_path);
    let position = Position { line: 1, character: 8 };
    let mut server = LspServer::new();
    server.open_document(uri.clone(), source.into());
    assert!(workspace::WorkspaceInputs::capture(&server, &uri).diagnostics().metadata.is_some());
    assert!(server.hover(&uri, position).unwrap().contents.contains("Lowering metadata"));

    std::fs::write(&manifest_path, manifest.replace("edition = \"2027\"", "edition = \"2099\"")).unwrap();
    server.update_document(uri.clone(), source.into());
    let diagnostics = server.get_diagnostics(&uri);
    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == DiagnosticSeverity::Error && diagnostic.message.contains("unsupported CellScript edition '2099'")
        }),
        "{diagnostics:?}"
    );
    let inputs = workspace::WorkspaceInputs::capture(&server, &uri);
    let report = inputs.diagnostics();
    assert!(report.metadata.is_none());
    assert!(report.diagnostics.iter().any(|diagnostic| diagnostic.message.contains("unsupported CellScript edition '2099'")));
    assert!(inputs.entry_ast().is_none());
    assert!(inputs.group.modules().is_empty());
    assert!(inputs.group.resolver().is_none());
    let hover = server.hover(&uri, position).expect("syntax hover remains available");
    assert!(!hover.contents.contains("Lowering metadata"), "{hover:?}");

    std::fs::write(&manifest_path, &manifest).unwrap();
    server.update_document(uri.clone(), source.into());
    assert!(server.get_diagnostics(&uri).iter().all(|diagnostic| diagnostic.severity != DiagnosticSeverity::Error));
    assert!(workspace::WorkspaceInputs::capture(&server, &uri).diagnostics().metadata.is_some());
}

#[test]
fn unsaved_package_overlay_preserves_imports_and_manifest_admission() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    let manifest_path = root.join("Cell.toml");
    let manifest = format!(
        "[package]\ncellscript_version = \">={}\"\nedition = \"2099\"\nname = \"unsaved_package\"\nversion = \"0.1.0\"\n",
        crate::VERSION
    );
    std::fs::write(&manifest_path, &manifest).unwrap();
    std::fs::write(root.join("src/helper.cell"), "module unsaved_package::helper;\npublic fn expected() -> u64 { return 7; }\n")
        .unwrap();
    let source =
        "module unsaved_package::new;\nuse unsaved_package::helper::expected;\naction verify() -> u64 { return expected(); }\n";
    let path = root.join("src/new.cell");
    assert!(!path.exists());
    let uri = utf8_path_to_file_uri(&path);
    let position = Position { line: 2, character: 8 };
    let mut server = LspServer::new();
    server.open_document(uri.clone(), source.into());
    let rejected = workspace::WorkspaceInputs::capture(&server, &uri);
    let report = rejected.diagnostics();
    assert!(report.metadata.is_none());
    assert!(report.diagnostics.iter().any(|error| error.message.contains("unsupported CellScript edition '2099'")));
    assert!(!server.hover(&uri, position).unwrap().contents.contains("Lowering metadata"));

    std::fs::write(&manifest_path, manifest.replace("2099", "2027")).unwrap();
    let accepted = workspace::WorkspaceInputs::capture(&server, &uri);
    assert_ne!(accepted.fingerprint, rejected.fingerprint);
    let report = accepted.diagnostics();
    assert!(report.metadata.is_some(), "{:?}", report.diagnostics);
    assert_eq!(accepted.group.modules().len(), 2);
    assert!(accepted.group.resolver().is_some());
    server.analyze_document(&uri);
    assert!(server.get_diagnostics(&uri).iter().all(|error| error.severity != DiagnosticSeverity::Error));
    assert!(server.hover(&uri, position).unwrap().contents.contains("Lowering metadata"));

    std::fs::write(&manifest_path, &manifest).unwrap();
    let rejected_again = workspace::WorkspaceInputs::capture(&server, &uri);
    assert_ne!(rejected_again.fingerprint, accepted.fingerprint);
    assert!(rejected_again.diagnostics().metadata.is_none());
    server.analyze_document(&uri);
    assert!(server.get_diagnostics(&uri).iter().any(|error| error.message.contains("unsupported CellScript edition '2099'")));
    assert!(!server.hover(&uri, position).unwrap().contents.contains("Lowering metadata"));
    assert!(!path.exists());
}

#[test]
fn test_manifest_edition_selects_the_preview_frontend() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
            root.join("Cell.toml"),
            "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"preview\"\nversion = \"0.1.0\"\nentry = \"src/main.cell\"\n",
        )
        .unwrap();
    let source = "module preview;\naction main(witness value: u64) -> u64 { return value; }\n";
    let source_path = root.join("src/main.cell");
    std::fs::write(&source_path, source).unwrap();

    let mut server = LspServer::new();
    let uri = utf8_path_to_file_uri(&source_path);
    server.open_document(uri.clone(), source.to_string());

    let diagnostics = server.get_diagnostics(&uri);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
    let completions = server.completion(&uri, Position { line: 1, character: 64 });
    assert!(server.keyword_completions(&uri).iter().any(|item| item.label == "consume"));
    assert!(server.keyword_completions(&uri).iter().any(|item| item.label == "consume_each"));
    assert!(completions.iter().any(|item| item.label == "action (Cell roles)"));
    assert!(completions.iter().any(|item| item.label == "lock (authorization)"));
    assert!(!completions.iter().any(|item| item.label == "lock_script"));
    for label in ["effects", "pool", "retire", "fresh", "audit"] {
        assert!(!server.keyword_completions(&uri).iter().any(|item| item.label == label));
    }

    let native = r#"module preview;
resource Token has store, replace, relock { owner: Address, amount: u64, }
action transfer(
        input token: Token,
        witness recipient: Address,
        output next: Token,
    ) {
        require token.amount > 0;
        replace token -> next {
            data { owner = same, amount = same, };
            type_script = same;
            lock = exact(recipient);
            capacity = same;
        }
    }
"#;
    let manifest_path = root.join("Cell.toml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(&source_path, native).unwrap();
    server.update_document(uri.clone(), native.to_string());
    let plain = workspace::WorkspaceInputs::capture(&server, &uri).diagnostics().metadata.unwrap();
    assert!(plain.typed_semantics.foundation.entry_contract.cells.is_none());
    std::fs::write(
        &manifest_path,
        format!(
            r#"{manifest}
[[artifacts]]
name = "transfer"
context = {{ kind = "type-group" }}
bindings = "group-relative"
coverage = "fixed-group"
layout = "Token"
dispatch = "single-entry"
entry = "transfer"
"#
        ),
    )
    .unwrap();
    server.update_document(uri.clone(), native.to_string());
    assert!(server.get_diagnostics(&uri).is_empty());
    let bound = workspace::WorkspaceInputs::capture(&server, &uri).diagnostics().metadata.unwrap();
    assert_eq!(
        bound.typed_semantics.foundation.entry_contract.cells.as_ref().unwrap().coverage,
        cellscript_artifact_checker::EntryCoverage::FixedGroup
    );
    let hover = server.hover(&uri, Position { line: 2, character: 9 }).unwrap();
    assert!(hover.contents.contains("**Coverage** `fixed-group`"));
    assert!(hover.contents.contains("`token` at `group-input[0]`"));

    let fixed_manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(
        &manifest_path,
        format!(
            r#"{fixed_manifest}
[[artifacts]]
name = "manual"
context = {{ kind = "type-group" }}
bindings = "transaction-absolute"
coverage = "action-defined"
dispatch = "single-entry"
entry = "transfer"
"#
        ),
    )
    .unwrap();
    server.artifact_name = Some(Arc::from("manual"));
    server.update_document(uri.clone(), native.to_string());
    assert!(server.get_diagnostics(&uri).is_empty());
    let hover = server.hover(&uri, Position { line: 2, character: 9 }).unwrap();
    assert!(hover.contents.contains("**Coverage** `action-defined`"));
    assert!(hover.contents.contains("`token` at `input[0]`"));
    server.artifact_name = Some(Arc::from("missing"));
    server.update_document(uri.clone(), native.to_string());
    assert!(server.get_diagnostics(&uri).iter().any(|diagnostic| diagnostic.message.contains("artifact 'missing' is not declared")));
    server.artifact_name = None;
    std::fs::write(&manifest_path, fixed_manifest).unwrap();
    server.update_document(uri.clone(), native.to_string());

    for label in ["pool", "retire", "fresh", "audit"] {
        assert!(server.keyword_completions(&uri).iter().any(|item| item.label == label));
    }
    for label in ["action", "lock", "require", "replace"] {
        assert!(server.keyword_completions(&uri).iter().any(|item| item.label == label));
    }
    for label in ["entry", "verify", "enforce"] {
        assert!(!server.keyword_completions(&uri).iter().any(|item| item.label == label));
    }
    assert!(server.keyword_completions(&uri).iter().any(|item| item.label == "consume"));
    let edits = server.format_document(&uri);
    assert_eq!(edits.len(), 1);
    assert!(edits[0].new_text.contains("action transfer("));

    let native_lock = r#"module preview;
resource Vault has store { owner: Address, }
lock unlock(
        protected vault: Vault,
        lock_args owner: Address,
        witness claimed_owner: Address,
    ) {
        require vault.owner == owner;
        require claimed_owner == owner;
        true
    }
"#;
    std::fs::write(&manifest_path, manifest).unwrap();
    std::fs::write(&source_path, native_lock).unwrap();
    server.update_document(uri.clone(), native_lock.to_string());
    assert!(server.get_diagnostics(&uri).is_empty());
    let edits = server.format_document(&uri);
    assert_eq!(edits.len(), 1);
    assert!(edits[0].new_text.contains("lock unlock("));
}

#[test]
fn test_workspace_goto_definition_across_modules() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
            root.join("Cell.toml"),
            "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"demo\"\nversion = \"0.1.0\"\nentry = \"src/main.cell\"\n",
        )
        .unwrap();
    std::fs::write(root.join("src/types.cell"), "module demo::types;\n\nresource Token {\n    amount: u64,\n}\n").unwrap();
    let main_source =
        "module demo::main;\n\nuse demo::types::Token;\n\naction inspect(input token: Token) -> u64 {\n    \n        token.amount\n}\n";
    let main_path = root.join("src/main.cell");
    std::fs::write(&main_path, main_source).unwrap();

    let mut server = LspServer::new();
    let main_uri = utf8_path_to_file_uri(&main_path);
    server.open_document(main_uri.clone(), main_source.to_string());

    let type_column = main_source.lines().nth(4).unwrap().find("Token").unwrap() as u32;
    let definition = server.goto_definition(&main_uri, Position { line: 4, character: type_column }).expect("cross-module definition");
    assert!(definition.uri.ends_with("/src/types.cell"));
    assert_eq!(definition.range.start.line, 2);
}

#[test]
fn test_workspace_references_across_modules() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
            root.join("Cell.toml"),
            "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"demo\"\nversion = \"0.1.0\"\nentry = \"src/main.cell\"\n",
        )
        .unwrap();
    let types_source = "module demo::types;\n\nresource Token {\n    amount: u64,\n}\n";
    let types_path = root.join("src/types.cell");
    std::fs::write(&types_path, types_source).unwrap();
    let main_source =
        "module demo::main;\n\nuse demo::types::Token;\n\naction inspect(input token: Token) -> u64 {\n    \n        token.amount\n}\n";
    std::fs::write(root.join("src/main.cell"), main_source).unwrap();

    let mut server = LspServer::new();
    let types_uri = utf8_path_to_file_uri(&types_path);
    server.open_document(types_uri.clone(), types_source.to_string());

    let refs = server.find_references(&types_uri, Position { line: 2, character: 10 });
    assert!(refs.iter().any(|location| location.uri.ends_with("/src/types.cell")));
    assert!(refs.iter().any(|location| location.uri.ends_with("/src/main.cell")));
    assert!(refs.len() >= 3);
}

#[test]
fn test_workspace_rename_groups_edits_by_file() {
    let temp = tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_path_buf()).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
            root.join("Cell.toml"),
            "[package]\ncellscript_version = \">=0.33.0-alpha.1\"\nedition = \"2027\"\nname = \"demo\"\nversion = \"0.1.0\"\nentry = \"src/main.cell\"\n",
        )
        .unwrap();
    let types_source = "module demo::types;\n\nresource Token {\n    amount: u64,\n}\n";
    let types_path = root.join("src/types.cell");
    std::fs::write(&types_path, types_source).unwrap();
    let main_source =
        "module demo::main;\n\nuse demo::types::Token;\n\naction inspect(input token: Token) -> u64 {\n    \n        token.amount\n}\n";
    let main_path = root.join("src/main.cell");
    std::fs::write(&main_path, main_source).unwrap();

    let mut server = LspServer::new();
    let types_uri = utf8_path_to_file_uri(&types_path);
    server.open_document(types_uri.clone(), types_source.to_string());

    let changes = server.rename(&types_uri, Position { line: 2, character: 10 }, "Asset".to_string());

    let type_uri = changes.keys().find(|uri| uri.ends_with("/src/types.cell")).expect("rename should edit the defining file").clone();
    let main_uri =
        changes.keys().find(|uri| uri.ends_with("/src/main.cell")).expect("rename should edit referencing files separately").clone();
    let type_edits = changes.get(&type_uri).expect("defining file edits should be present");
    let main_edits = changes.get(&main_uri).expect("referencing file edits should be present");
    assert_eq!(changes.len(), 2);
    assert_eq!(type_edits.len(), 1);
    assert!(main_edits.len() >= 2, "main file should include the import and parameter references: {:?}", main_edits);
    assert!(changes.values().flatten().all(|edit| edit.new_text == "Asset"));
}

#[test]
fn test_workspace_rename_rejects_invalid_new_names() {
    let mut server = LspServer::new();
    let uri = "file:///rename.cell".to_string();
    let source = "module demo;\n\nresource Token {\n    amount: u64,\n}\n";
    server.open_document(uri.clone(), source.to_string());

    for new_name in ["", "123Token", "Token-V2", "resource", "Address"] {
        let changes = server.rename(&uri, Position { line: 2, character: 10 }, new_name.to_string());
        assert!(changes.is_empty(), "rename should fail closed for invalid new name `{new_name}`");
    }
}

#[test]
fn test_workspace_rename_respects_unicode_identifier_boundaries() {
    let mut server = LspServer::new();
    let uri = "file:///unicode_rename.cell".to_string();
    let source = r#"module unicode_rename;

resource βToken {
    amount: u64,
}

resource Token {
    amount: u64,
}

action inspect(input token: Token) -> u64 {

        token.amount
}
"#;
    server.open_document(uri.clone(), source.to_string());

    let changes = server.rename(&uri, Position { line: 6, character: 10 }, "Asset".to_string());
    let edits = changes.get(&uri).expect("rename should edit the current document");

    assert!(edits.iter().all(|edit| edit.range.start.line != 2), "rename must not edit the suffix of βToken: {edits:?}");
    assert!(edits.iter().any(|edit| edit.range.start.line == 6), "definition should be renamed: {edits:?}");
    assert!(edits.iter().any(|edit| edit.range.start.line == 10), "type reference should be renamed: {edits:?}");
}

#[test]
fn test_workspace_rename_skips_comments_and_strings() {
    let mut server = LspServer::new();
    let uri = "file:///rename_text.cell".to_string();
    let source = r#"module rename_text;

// Token in a comment must not be edited.
resource Token {
    amount: u64,
}

action inspect(token: Token) -> u64 {

        let label = "Token"
        token.amount
}
"#;
    server.open_document(uri.clone(), source.to_string());

    let changes = server.rename(&uri, Position { line: 3, character: 10 }, "Asset".to_string());
    let edits = changes.get(&uri).expect("rename should edit identifiers in the current document");

    assert!(edits.iter().all(|edit| edit.range.start.line != 2), "rename must not edit comments: {edits:?}");
    assert!(edits.iter().all(|edit| edit.range.start.line != 8), "rename must not edit string literals: {edits:?}");
    assert!(edits.iter().any(|edit| edit.range.start.line == 3), "definition should be renamed: {edits:?}");
    assert!(edits.iter().any(|edit| edit.range.start.line == 7), "type reference should be renamed: {edits:?}");
    for offset in [source.find("Token in a comment").unwrap(), source.find("\"Token\"").unwrap() + 1] {
        assert!(server.rename(&uri, offset_to_position(source, offset + 1), "Asset".into()).is_empty());
    }
}
