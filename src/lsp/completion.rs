use super::positions::{is_ident_char, position_le, position_to_offset, word_before_offset};
use super::{
    cell_views, item_span, position_in_range, span_to_range, stmt_span, type_to_string, CompletionContext, CompletionItem,
    CompletionItemKind, LspServer, Position,
};
use crate::ast::{BindingPattern, FlowDef, Item, Module, Stmt, Type};
use crate::lexer::token::TokenKind;
use std::collections::HashMap;

impl LspServer {
    pub fn completion(&self, uri: &str, position: Position) -> Vec<CompletionItem> {
        // Member completion commonly runs on a temporarily unfinished `view.`.
        // Recover a missing field and its active statement terminator; keep the document and its actual
        // diagnostics intact, and use the repaired AST for this request alone.
        if !self.ast_cache.contains_key(uri)
            && let Some(source) = self.documents.get(uri).filter(|source| source.len() <= crate::MAX_SOURCE_BYTES)
            && let Some(offset) = position_to_offset(source, position)
            && source[..offset].ends_with('.')
        {
            let mut repaired = format!("{}__completion{}", &source[..offset], &source[offset..]);
            let mut parsed = crate::frontend::parse_diagnostics(&repaired, self.document_edition(uri));
            if let Err(errors) = &parsed
                && errors.len() == 1
                && errors[0].message.starts_with("expected `;` to end")
            {
                let end = errors[0].span.start;
                let placeholder_end = offset + "__completion".len();
                if end >= placeholder_end
                    && end <= repaired.len()
                    && crate::lexer::lex(&repaired[placeholder_end..end]).is_ok_and(|tokens| {
                        tokens.len() <= 33
                            && tokens.iter().all(|token| {
                                matches!(token.kind, TokenKind::RParen | TokenKind::RBracket | TokenKind::Newline | TokenKind::Eof)
                            })
                    })
                {
                    // Complete only the active draft statement for a lexical
                    // hint. Original bytes and diagnostics remain untouched.
                    repaired.insert(end, ';');
                    parsed = crate::frontend::parse_diagnostics(&repaired, self.document_edition(uri));
                }
            }
            if let Ok(module) = parsed {
                // Keep imported schemas from other open buffers. A fresh
                // server would silently fall back to their older disk copies.
                let mut temporary = Self {
                    documents: self.documents.clone(),
                    ast_cache: self.ast_cache.clone(),
                    ast_editions: self.ast_editions.clone(),
                    diagnostics: HashMap::new(),
                    edition_overrides: self.edition_overrides.clone(),
                    view_cache: Default::default(),
                    observed_inputs: None,
                    source_groups: self.source_groups.clone(),
                    #[cfg(test)]
                    read_stats: self.read_stats.clone(),
                };
                temporary.documents.insert(uri.into(), repaired.into());
                temporary.ast_cache.insert(uri.into(), module.into());
                temporary.ast_editions.insert(uri.into(), self.document_edition(uri));
                temporary.edition_overrides.insert(uri.into(), self.document_edition(uri));
                return temporary.completion(uri, position);
            }
        }
        let mut items = Vec::new();

        let ctx = self.completion_context(uri, position);

        match ctx {
            CompletionContext::Type => {
                items.extend(self.type_completions());
                // Type-position also allows user-defined types.
                if let Some(ast) = self.ast_cache.get(uri) {
                    items.extend(self.type_symbol_completions(ast));
                }
            }
            CompletionContext::Member { type_name } => {
                items.extend(self.member_completions(uri, &type_name));
            }
            CompletionContext::Namespace { type_name } => {
                // These built-in namespaces do not require a complete AST:
                // completion commonly runs while `witness::` is unfinished.
                if matches!(type_name.as_str(), "witness" | "ckb") {
                    items.extend(self.member_completions(uri, &type_name));
                }
                if let Some(ast) = self.ast_cache.get(uri) {
                    items.extend(self.namespace_symbol_completions(ast, &type_name));
                }
            }
            CompletionContext::Declaration => {
                items.extend(self.declaration_keyword_completions(uri));
            }
            CompletionContext::Expression => {
                items.extend(self.keyword_completions(uri));
                items.extend(self.type_completions());
                if let (Some(ast), Some(content)) = (self.ast_cache.get(uri), self.documents.get(uri)) {
                    items.extend(self.symbol_completions(ast));
                    items.extend(self.local_completions(content, ast, position));
                }
            }
        }

        items
    }

    /// Determine the completion context at the given position.
    fn completion_context(&self, uri: &str, position: Position) -> CompletionContext {
        let Some(content) = self.documents.get(uri) else {
            return CompletionContext::Expression;
        };

        let line_start = self.line_start_offset(content, position.line);
        let offset = position_to_offset(content, position).unwrap_or(line_start);
        let prefix = &content[line_start..offset];

        // Check for namespace access: `Type::Variant`.
        if let Some(scope_pos) = prefix.rfind("::") {
            let suffix = &prefix[scope_pos + 2..];
            if suffix.chars().all(is_ident_char) {
                let before_scope = &prefix[..scope_pos];
                let type_name = word_before_offset(before_scope, before_scope.len()).unwrap_or_default();
                return CompletionContext::Namespace { type_name };
            }
        }

        // Check for member access: `expr.field`
        if let Some(dot_pos) = prefix.rfind('.') {
            // We want the identifier before the dot.
            let before_dot = &prefix[..dot_pos];
            let type_name = self
                .cell_view_completion_type(uri, position, before_dot)
                .unwrap_or_else(|| word_before_offset(before_dot, before_dot.len()).unwrap_or_default());
            return CompletionContext::Member { type_name };
        }

        // Check for type context: after `:`, `->`, or `<`
        let trimmed = prefix.trim_end();
        if trimmed.ends_with(':') || trimmed.ends_with("->") || trimmed.ends_with('<') {
            return CompletionContext::Type;
        }

        // Check for top-level / declaration context
        let line_text = prefix.trim();
        if line_text.is_empty() || line_text == "module" {
            return CompletionContext::Declaration;
        }

        CompletionContext::Expression
    }

    /// Get the byte offset where a given line starts.
    pub(super) fn line_start_offset(&self, content: &str, line: u32) -> usize {
        let mut current_line = 0u32;
        for (idx, ch) in content.char_indices() {
            if current_line == line {
                return idx;
            }
            if ch == '\n' {
                current_line += 1;
            }
        }
        content.len()
    }

    /// Declaration-position keywords only.
    pub(super) fn declaration_keyword_completions(&self, uri: &str) -> Vec<CompletionItem> {
        let mut declarations = vec![
            ("resource", "resource ${1:Name} {\n    $0\n}"),
            ("shared", "shared ${1:Name} {\n    $0\n}"),
            ("receipt", "receipt ${1:Name} {\n    $0\n}"),
            ("struct", "struct ${1:Name} {\n    $0\n}"),
            ("flow", "flow ${1:Name} for ${2:Type}.${3:state} {\n    ${4:Created} -> ${5:Live};\n}"),
            (
                "invariant",
                "invariant ${1:name} {\n    trigger: ${2:type_group};\n    scope: ${3:group};\n    reads: ${4:group_inputs<Token>.amount}, ${5:group_outputs<Token>.amount};\n    assert_conserved(${6:Token.amount}, scope = ${7:group});\n}",
            ),
            ("action", "action ${1:name}(input ${2:cell}: ${3:CellType}) -> ${4:output}: ${3:CellType} {\n    $0\n}"),
            (
                "lock",
                "lock ${1:name}(protected ${2:cell}: ${3:CellType}) -> bool {\n    require ${4:authorization_condition};\n    true\n    $0\n}",
            ),
            ("const", "const ${1:NAME}: ${2:u64} = $0;"),
            ("enum", "enum ${1:Name} {\n    $0\n}"),
            ("use", "use ${1:path};"),
            ("public", "public ${1:fn}"),
            ("private", "private ${1:fn}"),
            ("public(package)", "public(package) ${1:fn}"),
        ];
        {
            declarations.push((
                "type_script",
                "type_script ${1:Name} on type_group<${2:CellType}> {\n    action ${3:transfer}(\n        input ${4:before}: ${2:CellType} from group_input[0],\n        witness ${5:to}: Address from group_witness.input_type,\n        output ${6:after}: ${2:CellType} from group_output[0],\n    ) {\n        require ${4:before}.${7:amount} > 0;\n\n        replace ${4:before} -> ${6:after} {\n            data {\n                ${7:amount} = same,\n            };\n            type_script = same;\n            lock = exact(${5:to});\n            capacity = same;\n        }\n    }\n}",
            ));
            declarations.push((
                "lock_script",
                "lock_script ${1:Name} on lock_group {\n    lock ${2:unlock}(\n        protected ${3:cell}: ${4:CellType} from group_input[0],\n        lock_args ${5:owner}: Address from current_script.args,\n        witness ${6:claimed_owner}: Address from group_witness.input_type,\n    ) {\n        require ${3:cell}.${7:owner} == ${5:owner};\n        require ${6:claimed_owner} == ${5:owner};\n        true\n    }\n}",
            ));
        }
        if self.named_runtime_authoring(uri) {
            declarations.extend_from_slice(cell_views::NAMED_RUNTIME_DECLARATIONS);
        }
        declarations
            .into_iter()
            .map(|(label, insert)| CompletionItem {
                label: label.to_string(),
                kind: CompletionItemKind::Keyword,
                detail: Some(format!("{} keyword", label)),
                documentation: None,
                insert_text: Some(insert.to_string()),
            })
            .collect()
    }

    /// Completions for user-defined types (at type positions).
    fn type_symbol_completions(&self, module: &Module) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for item in &module.items {
            let (name, kind_label) = match item {
                Item::DataShape(def) => (&def.name, "data_shape"),
                Item::Resource(r) => (&r.name, "resource"),
                Item::Shared(s) => (&s.name, "shared"),
                Item::Receipt(r) => (&r.name, "receipt"),
                Item::Struct(s) => (&s.name, "struct"),
                Item::Enum(e) => (&e.name, "enum"),
                _ => continue,
            };
            items.push(CompletionItem {
                label: name.clone(),
                kind: CompletionItemKind::Class,
                detail: Some(format!("{} {}", kind_label, name)),
                documentation: None,
                insert_text: Some(name.clone()),
            });
        }
        items
    }

    pub(super) fn namespace_symbol_completions(&self, module: &Module, type_name: &str) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for item in &module.items {
            match item {
                Item::DataShape(def) if def.name == type_name => {
                    items.extend(["Item", "Payload"].map(|member| CompletionItem {
                        label: member.into(),
                        kind: CompletionItemKind::Class,
                        detail: Some(format!("read-only {}::{member}", def.name)),
                        documentation: Some("Derived view with this shape's payload bound; immutable helper parameters only, no ownership or escape.".into()),
                        insert_text: Some(member.into()),
                    }));
                }
                Item::Enum(enum_def) if enum_def.name == type_name => {
                    items.extend(enum_def.variants.iter().map(|variant| {
                        let payload = variant.fields.iter().map(type_to_string).collect::<Vec<_>>();
                        let insert_text = if payload.is_empty() {
                            variant.name.clone()
                        } else {
                            format!(
                                "{}({})",
                                variant.name,
                                (0..payload.len()).map(|index| format!("value{}", index + 1)).collect::<Vec<_>>().join(", ")
                            )
                        };
                        CompletionItem {
                            label: variant.name.clone(),
                            kind: CompletionItemKind::EnumMember,
                            detail: Some(if payload.is_empty() {
                                format!("enum variant {}::{}", enum_def.name, variant.name)
                            } else {
                                format!("enum variant {}::{}({})", enum_def.name, variant.name, payload.join(", "))
                            }),
                            documentation: (!payload.is_empty()).then(|| {
                                "Fixed-width payload constructor; generic enum templates specialize to the same checked layout before IR."
                                    .to_string()
                            }),
                            insert_text: Some(insert_text),
                        }
                    }));
                }
                _ => {}
            }
        }
        items.extend(Self::flow_state_completions(module, type_name));
        items
    }

    fn flow_state_completions(module: &Module, type_name: &str) -> Vec<CompletionItem> {
        module
            .items
            .iter()
            .filter_map(|item| {
                let Item::Flow(machine) = item else {
                    return None;
                };
                (machine.target.base == type_name).then_some(machine)
            })
            .flat_map(|machine| {
                let states = Self::flow_enum_states(module, type_name, &machine.target.field)
                    .unwrap_or_else(|| Self::transition_states(machine));
                let field_name = machine.target.field.clone();
                states.into_iter().enumerate().map(move |(index, state)| CompletionItem {
                    label: state.clone(),
                    kind: CompletionItemKind::EnumMember,
                    detail: Some(format!("flow state {}::{}", type_name, state)),
                    documentation: Some(format!("State index {} for flow field `{}.{}`.", index, type_name, field_name)),
                    insert_text: Some(state),
                })
            })
            .collect()
    }

    fn flow_enum_states(module: &Module, type_name: &str, field_name: &str) -> Option<Vec<String>> {
        let enum_name = module.items.iter().find_map(|item| {
            let fields = match item {
                Item::Resource(def) if def.name == type_name => Some(&def.fields),
                Item::Shared(def) if def.name == type_name => Some(&def.fields),
                Item::Receipt(def) if def.name == type_name => Some(&def.fields),
                Item::Struct(def) if def.name == type_name => Some(&def.fields),
                _ => None,
            }?;
            fields.iter().find_map(|field| {
                if field.name == field_name
                    && let Type::Named(name) = &field.ty
                {
                    return Some(name.clone());
                }
                None
            })
        })?;

        module.items.iter().find_map(|item| {
            let Item::Enum(enum_def) = item else {
                return None;
            };
            (enum_def.name == enum_name && enum_def.variants.iter().all(|variant| variant.fields.is_empty()))
                .then(|| enum_def.variants.iter().map(|variant| variant.name.clone()).collect())
        })
    }

    fn transition_states(machine: &FlowDef) -> Vec<String> {
        let mut states = Vec::new();
        for transition in &machine.transitions {
            for raw in [&transition.from, &transition.to] {
                let state = raw.rsplit_once("::").map_or(raw.as_str(), |(_, state)| state);
                if !states.iter().any(|existing| existing == state) {
                    states.push(state.to_string());
                }
            }
        }
        states
    }

    /// Member completions for a given type name (after `.`).
    pub(super) fn member_completions(&self, uri: &str, type_name: &str) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        if let Some(nested) = crate::nested_values::parsed(type_name) {
            match nested {
                crate::nested_values::NestedType::Values(_) | crate::nested_values::NestedType::BytesView { .. } => {
                    for method in ["len", "is_empty"] {
                        items.push(CompletionItem {
                            label: method.into(),
                            kind: CompletionItemKind::Method,
                            detail: Some(format!("{type_name}.{method}()")),
                            documentation: Some("Read-only bounded view; mutation and escape remain forbidden.".into()),
                            insert_text: Some(format!("{method}()")),
                        });
                    }
                }
                crate::nested_values::NestedType::RecordView { element, max_payload_bytes } => {
                    if let Some(element) = element.record_name()
                        && let Some(fields) = self.completion_record_fields(uri, element)
                    {
                        for (index, field) in fields.iter().enumerate() {
                            let ty = if index + 1 == fields.len() {
                                crate::nested_values::bytes_view_type(max_payload_bytes)
                            } else {
                                type_to_string(&field.ty)
                            };
                            items.push(CompletionItem {
                                label: field.name.clone(),
                                kind: CompletionItemKind::Field,
                                detail: Some(ty),
                                documentation: None,
                                insert_text: Some(field.name.clone()),
                            });
                        }
                    }
                }
            }
            return items;
        }

        if let Some(selection) = crate::cell_selection::parse_selection_type(type_name) {
            if let Some(role) = selection.selected {
                for method in ["input_count", "output_count", "input", "output", "successor"] {
                    let Some(read) = crate::cell_selection::method(method) else {
                        continue;
                    };
                    items.push(CompletionItem {
                        label: method.into(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!(
                            "({}) -> {}",
                            if read.indexed() { "ordinal: u64" } else { "" },
                            read.result_type(&selection.schemas[role as usize])
                        )),
                        documentation: Some(
                            "Experimental checked role read; ordinals are local to this role, not global transaction indices.".into(),
                        ),
                        insert_text: Some(method.into()),
                    });
                }
            } else {
                for (name, schema) in selection.names.iter().zip(&selection.schemas) {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: CompletionItemKind::Field,
                        detail: Some(format!("Immutable selected role of {schema}")),
                        documentation: None,
                        insert_text: Some(name.clone()),
                    });
                }
            }
            return items;
        }

        if crate::cell_data_view::transaction_schema(type_name).is_some() {
            for field in [
                "capacity",
                "occupied_capacity",
                "unoccupied_capacity",
                "data_size",
                "data_hash",
                "lock_hash",
                "type_hash",
                "lock",
                "type_script",
                "data",
            ] {
                items.push(CompletionItem {
                    label: field.into(),
                    kind: CompletionItemKind::Field,
                    detail: Some(if field == "data" {
                        "Read-only fixed Cell data; exact schema length checked at each field read".into()
                    } else {
                        format!("{type_name}.{field}")
                    }),
                    documentation: None,
                    insert_text: Some(field.into()),
                });
            }
            let fields: &[&str] = if type_name.starts_with("InputView<") { &["out_point", "since"] } else { &["output_index"] };
            for field in fields {
                items.push(CompletionItem {
                    label: (*field).into(),
                    kind: CompletionItemKind::Field,
                    detail: Some(format!("{type_name}.{field}")),
                    documentation: None,
                    insert_text: Some((*field).into()),
                });
            }
            return items;
        }
        let type_name = crate::cell_data_view::schema(type_name).unwrap_or(type_name);

        // Built-in namespace methods.
        match type_name {
            "HeaderDepView" => {
                for field in ["epoch_number", "epoch_start_block_number", "epoch_length", "block_number", "timestamp"] {
                    items.push(CompletionItem {
                        label: field.to_string(),
                        kind: CompletionItemKind::Field,
                        detail: Some(format!("HeaderDepView.{field}")),
                        documentation: None,
                        insert_text: Some(field.to_string()),
                    });
                }
                return items;
            }
            "Vec" => {
                for (name, insert) in [
                    ("new", "Vec::new()"),
                    ("with_capacity", "Vec::with_capacity($0)"),
                    ("capacity", "capacity()"),
                    ("push", "push($0)"),
                    ("extend_from_slice", "extend_from_slice($0)"),
                    ("len", "len()"),
                    ("is_empty", "is_empty()"),
                    ("first", "first()"),
                    ("last", "last()"),
                    ("contains", "contains($0)"),
                    ("set", "set($0)"),
                    ("remove", "remove($0)"),
                    ("pop", "pop()"),
                    ("insert", "insert($0)"),
                    ("reverse", "reverse()"),
                    ("truncate", "truncate($0)"),
                    ("swap", "swap($0)"),
                    ("clear", "clear()"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Method,
                        detail: Some(format!("Vec::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "env" => {
                for (name, insert) in [
                    ("current_timepoint", "env::current_timepoint()"),
                    ("sighash_all", "env::sighash_all(${1:source::group_input(0)})"),
                    ("sighash_all_zero_lock", "env::sighash_all_zero_lock(${1:8}, ${2:64}, ${3:16}, ${4:65536})"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Method,
                        detail: Some(format!("env::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "source" => {
                for name in ["input", "output", "cell_dep", "header_dep", "group_input", "group_output"] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("source::{}", name)),
                        documentation: None,
                        insert_text: Some(format!("source::{}(${{1:0}})", name)),
                    });
                }
                return items;
            }
            "witness" => {
                items.push(CompletionItem {
                    label: "args".to_string(),
                    kind: CompletionItemKind::Function,
                    detail: Some("witness::args".to_string()),
                    documentation: Some("Named read-only WitnessArgs transaction view".to_string()),
                    insert_text: Some("witness::args(${1:0})".to_string()),
                });
                for (name, owner) in [
                    ("bounded_raw", "raw witness bytes"),
                    ("bounded_lock", "WitnessArgs.lock bytes"),
                    ("bounded_entry", "WitnessArgs.input_type entry bytes"),
                    ("bounded_output_type", "WitnessArgs.output_type bytes"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("witness::{}", name)),
                        documentation: Some(format!("Read-only bounded view over {owner}; maximum must be a literal in 0..=65536")),
                        insert_text: Some(format!("witness::{}(${{1:witness::args(0)}}, ${{2:4096}})", name)),
                    });
                }
                for name in ["lock_exact32", "input_type_exact32", "output_type_exact32", "size"] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("witness::{}", name)),
                        documentation: None,
                        insert_text: Some(format!("witness::{}(${{1:source::group_input(0)}})", name)),
                    });
                }
                for (name, insert) in [
                    ("count", "witness::count()"),
                    ("byte", "witness::byte(${1:source::input(0)}, ${2:0})"),
                    ("u32_le", "witness::u32_le(${1:source::input(0)}, ${2:0})"),
                    ("u64_le", "witness::u64_le(${1:source::input(0)}, ${2:0})"),
                    ("blake2b", "witness::blake2b(${1:bounded_view})"),
                    ("blake2b_span", "witness::blake2b_span(${1:source::input(0)}, ${2:0}, ${3:32})"),
                    ("bytes32", "witness::bytes32(${1:source::input(0)}, ${2:0})"),
                    ("blake2b_select_chunks", "witness::blake2b_select_chunks(${1:source::input(0)}, ${2:0}, ${3:1}, ${4:selection}, ${5:prefix}, ${6:suffix})"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("witness::{}", name)),
                        documentation: Some(
                            "Exact raw witness access; short reads fail closed; count includes extra witnesses".to_string(),
                        ),
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "script" => {
                for (name, insert) in [
                    ("hash_type_data", "script::hash_type_data()"),
                    ("hash_type_type", "script::hash_type_type()"),
                    ("hash_type_data1", "script::hash_type_data1()"),
                    ("hash_type_data2", "script::hash_type_data2()"),
                    ("args_empty", "script::args_empty()"),
                    ("args", "script::args(${1:b\"owner\"})"),
                    ("new", "script::new(${1:code_hash}, ${2:hash_type}, ${3:args})"),
                    ("hash", "script::hash(${1:script})"),
                    ("require_cell_lock_matches", "script::require_cell_lock_matches(${1:source::input(0)}, ${2:expected_script})"),
                    ("require_cell_type_matches", "script::require_cell_type_matches(${1:source::output(0)}, ${2:expected_script})"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("script::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "ckb" => {
                for (name, insert) in [
                    ("input", "ckb::input::<${1:CellType}>(${2:0})"),
                    ("output", "ckb::output::<${1:CellType}>(${2:0})"),
                    ("group_input", "ckb::group_input::<${1:CellType}>(${2:0})"),
                    ("group_output", "ckb::group_output::<${1:CellType}>(${2:0})"),
                    ("cell_dep", "ckb::cell_dep(${1:0})"),
                    ("header_dep", "ckb::header_dep(${1:0})"),
                    ("input_out_point", "ckb::input_out_point(${1:input})"),
                    ("lock_script", "ckb::lock_script(${1:cell})"),
                    ("type_script", "ckb::type_script(${1:cell})"),
                    ("header_epoch_number", "ckb::header_epoch_number()"),
                    ("header_epoch_start_block_number", "ckb::header_epoch_start_block_number()"),
                    ("header_epoch_length", "ckb::header_epoch_length()"),
                    ("input_since", "ckb::input_since()"),
                    ("input_since_raw", "ckb::input_since_raw()"),
                    ("since_epoch_absolute", "ckb::since_epoch_absolute(${1:number}, ${2:index}, ${3:length})"),
                    ("since_epoch_relative", "ckb::since_epoch_relative(${1:number}, ${2:index}, ${3:length})"),
                    ("since_absolute_epoch", "ckb::since_absolute_epoch(${1:number}, ${2:index}, ${3:length})"),
                    ("since_relative_epoch", "ckb::since_relative_epoch(${1:number}, ${2:index}, ${3:length})"),
                    ("since_absolute_block", "ckb::since_absolute_block(${1:block_number})"),
                    ("since_relative_block", "ckb::since_relative_block(${1:block_count})"),
                    ("since_absolute_timestamp", "ckb::since_absolute_timestamp(${1:seconds})"),
                    ("since_relative_timestamp", "ckb::since_relative_timestamp(${1:seconds})"),
                    ("since_decode", "ckb::since_decode(${1:encoded})"),
                    ("since_from_raw_checked", "ckb::since_from_raw_checked(${1:raw})"),
                    ("since_as_absolute_block", "ckb::since_as_absolute_block(${1:decoded})"),
                    ("since_as_relative_block", "ckb::since_as_relative_block(${1:decoded})"),
                    ("since_as_absolute_epoch", "ckb::since_as_absolute_epoch(${1:decoded})"),
                    ("since_as_relative_epoch", "ckb::since_as_relative_epoch(${1:decoded})"),
                    ("since_as_absolute_timestamp", "ckb::since_as_absolute_timestamp(${1:decoded})"),
                    ("since_as_relative_timestamp", "ckb::since_as_relative_timestamp(${1:decoded})"),
                    ("since_is_relative", "ckb::since_is_relative(${1:decoded})"),
                    ("since_is_disabled", "ckb::since_is_disabled(${1:decoded})"),
                    ("since_metric", "ckb::since_metric(${1:decoded})"),
                    ("since_value", "ckb::since_value(${1:decoded})"),
                    ("since_to_raw", "ckb::since_to_raw(${1:since})"),
                    ("epoch_duration", "ckb::epoch_duration(${1:epochs})"),
                    ("epoch_add", "ckb::epoch_add(${1:epoch}, ${2:duration})"),
                    ("epoch_sub", "ckb::epoch_sub(${1:epoch}, ${2:duration})"),
                    ("epoch_number_to_u64", "ckb::epoch_number_to_u64(${1:epoch})"),
                    ("epoch_duration_to_u64", "ckb::epoch_duration_to_u64(${1:duration})"),
                    ("block_number_to_u64", "ckb::block_number_to_u64(${1:block})"),
                    ("epoch_length_to_u64", "ckb::epoch_length_to_u64(${1:length})"),
                    ("timestamp_millis_to_u64", "ckb::timestamp_millis_to_u64(${1:timestamp})"),
                    ("current_role", "ckb::current_role()"),
                    ("current_script_hash", "ckb::current_script_hash()"),
                    ("transaction_hash", "ckb::transaction_hash()"),
                    ("script_hash", "ckb::script_hash(${1:hash})"),
                    ("cell_capacity", "ckb::cell_capacity(${1:source::group_input(0)})"),
                    ("cell_occupied_capacity", "ckb::cell_occupied_capacity(${1:source::group_input(0)})"),
                    ("cell_unoccupied_capacity", "ckb::cell_unoccupied_capacity(${1:source::group_input(0)})"),
                    ("cell_output_index", "ckb::cell_output_index(${1:source::group_output(0)})"),
                    ("input_out_point_index", "ckb::input_out_point_index(${1:source::group_input(0)})"),
                    ("input_out_point_tx_hash_low", "ckb::input_out_point_tx_hash_low(${1:source::group_input(0)})"),
                    ("cell_lock_hash_low", "ckb::cell_lock_hash_low(${1:source::group_input(0)})"),
                    ("cell_type_hash_low", "ckb::cell_type_hash_low(${1:source::group_input(0)})"),
                    ("cell_lock_hash", "ckb::cell_lock_hash(${1:source::group_input(0)})"),
                    ("cell_data_blake2b_span", "ckb::cell_data_blake2b_span(${1:source::input(0)}, ${2:0}, ${3:32})"),
                    ("raw_transaction_hash_without_cell_deps", "ckb::raw_transaction_hash_without_cell_deps()"),
                    ("transaction_u32_le", "ckb::transaction_u32_le(${1:0})"),
                    ("transaction_blake2b_gather", "ckb::transaction_blake2b_gather(${1:offsets}, ${2:lengths}, ${3:prefix}, ${4:suffix})"),
                    ("cell_data_bytes32", "ckb::cell_data_bytes32(${1:source::input(0)}, ${2:0})"),
                    ("cell_data_hash_field", "ckb::cell_data_hash_field(${1:source::cell_dep(0)})"),
                    ("cell_type_hash", "ckb::cell_type_hash(${1:source::group_input(0)})"),
                    ("cell_lock_code_hash", "ckb::cell_lock_code_hash(${1:source::group_input(0)})"),
                    ("cell_type_code_hash", "ckb::cell_type_code_hash(${1:source::group_input(0)})"),
                    ("cell_lock_hash_type", "ckb::cell_lock_hash_type(${1:source::group_input(0)})"),
                    ("cell_type_hash_type", "ckb::cell_type_hash_type(${1:source::group_input(0)})"),
                    ("cell_lock_args_empty", "ckb::cell_lock_args_empty(${1:source::group_input(0)})"),
                    ("cell_type_args_empty", "ckb::cell_type_args_empty(${1:source::group_input(0)})"),
                    ("cell_lock_args32", "ckb::cell_lock_args32(${1:source::group_input(0)})"),
                    ("cell_type_args32", "ckb::cell_type_args32(${1:source::group_input(0)})"),
                    ("require_cell_lock_hash", "ckb::require_cell_lock_hash(${1:source::group_input(0)}, ${2:expected_lock_hash})"),
                    ("require_cell_type_hash", "ckb::require_cell_type_hash(${1:source::group_input(0)}, ${2:expected_type_hash})"),
                    ("require_cell_data_hash", "ckb::require_cell_data_hash(${1:source::cell_dep(0)}, ${2:expected_data_hash})"),
                    (
                        "require_cell_lock_exact_handle",
                        "ckb::require_cell_lock_exact_handle(${1:source::group_input(0)}, ${2:handle}, ${3:expected_handle_hash})",
                    ),
                    (
                        "require_cell_type_exact_handle",
                        "ckb::require_cell_type_exact_handle(${1:source::group_input(0)}, ${2:handle}, ${3:expected_handle_hash})",
                    ),
                    (
                        "require_cell_dep_exact_verifier_handle",
                        "ckb::require_cell_dep_exact_verifier_handle(${1:source::cell_dep(0)}, ${2:handle}, ${3:expected_handle_hash})",
                    ),
                    (
                        "require_cell_dep_open_script_handle",
                        "ckb::require_cell_dep_open_script_handle(${1:source::cell_dep(0)}, ${2:handle}, ${3:expected_root})",
                    ),
                    (
                        "require_cell_dep_open_verifier_handle",
                        "ckb::require_cell_dep_open_verifier_handle(${1:source::cell_dep(0)}, ${2:handle}, ${3:expected_root})",
                    ),
                    (
                        "require_cell_dep_open_script_handle_from_args",
                        "ckb::require_cell_dep_open_script_handle_from_args(${1:source::cell_dep(0)}, ${2:handle})",
                    ),
                    (
                        "require_open_script_participant_from_args",
                        "ckb::require_open_script_participant_from_args(${1:ckb::cell_dep(0)}, ${2:handle}, ${3:ckb::input::<Cell>(0)})",
                    ),
                    (
                        "require_instance_script_participant_from_args",
                        "ckb::require_instance_script_participant_from_args(${1:ckb::cell_dep(0)}, ${2:handle}, ${3:ckb::input::<Cell>(0)})",
                    ),
                    (
                        "require_cell_dep_open_verifier_handle_from_args",
                        "ckb::require_cell_dep_open_verifier_handle_from_args(${1:source::cell_dep(0)}, ${2:handle})",
                    ),
                    (
                        "require_bounded_cell_dep_data_hash",
                        "ckb::require_bounded_cell_dep_data_hash(${1:8}, ${2:expected_data_hash})",
                    ),
                    ("hash_sha256", "ckb::hash_sha256(${1:input})"),
                    ("hash_sha256d", "ckb::hash_sha256d(${1:input})"),
                    ("hash_sha256_pair", "ckb::hash_sha256_pair(${1:left}, ${2:right})"),
                    ("hash_sha256d_pair", "ckb::hash_sha256d_pair(${1:left}, ${2:right})"),
                    (
                        "require_sha256d_merkle_root",
                        "ckb::require_sha256d_merkle_root(${1:leaf}, ${2:siblings}, ${3:depth}, ${4:leaf_index}, ${5:expected_root})",
                    ),
                    ("require_current_script_args_empty", "ckb::require_current_script_args_empty()"),
                    ("require_cell_lock_args_empty", "ckb::require_cell_lock_args_empty(${1:source::group_input(0)})"),
                    ("require_cell_type_args_empty", "ckb::require_cell_type_args_empty(${1:source::group_input(0)})"),
                    (
                        "require_cell_lock_args32",
                        "ckb::require_cell_lock_args32(${1:source::group_input(0)}, ${2:expected_args_bytes})",
                    ),
                    (
                        "require_cell_type_args32",
                        "ckb::require_cell_type_args32(${1:source::group_input(0)}, ${2:expected_args_bytes})",
                    ),
                    (
                        "require_cell_lock_args_prefix32",
                        "ckb::require_cell_lock_args_prefix32(${1:source::group_input(0)}, ${2:expected_args_bytes})",
                    ),
                    (
                        "require_cell_type_args_prefix32",
                        "ckb::require_cell_type_args_prefix32(${1:source::group_input(0)}, ${2:expected_args_bytes})",
                    ),
                    (
                        "require_cell_lock_args_suffix32",
                        "ckb::require_cell_lock_args_suffix32(${1:source::group_input(0)}, ${2:expected_args_bytes})",
                    ),
                    (
                        "require_cell_type_args_suffix32",
                        "ckb::require_cell_type_args_suffix32(${1:source::group_input(0)}, ${2:expected_args_bytes})",
                    ),
                    (
                        "require_cell_lock_script_hash_type",
                        "ckb::require_cell_lock_script_hash_type(${1:source::group_input(0)}, ${2:expected_code_hash}, ${3:expected_hash_type})",
                    ),
                    (
                        "require_cell_type_script_hash_type",
                        "ckb::require_cell_type_script_hash_type(${1:source::group_input(0)}, ${2:expected_code_hash}, ${3:expected_hash_type})",
                    ),
                    (
                        "require_input_out_point_tx_hash",
                        "ckb::require_input_out_point_tx_hash(${1:source::group_input(0)}, ${2:expected_tx_hash})",
                    ),
                    (
                        "require_input_out_point",
                        "ckb::require_input_out_point(${1:source::group_input(0)}, ${2:expected_tx_hash}, ${3:expected_index})",
                    ),
                    (
                        "require_metapoint_relative",
                        "ckb::require_metapoint_relative(${1:source::group_input(0)}, ${2:source::group_input(1)}, ${3:relative_distance})",
                    ),
                    (
                        "require_lock_type_metapoint_pairs",
                        "ckb::require_lock_type_metapoint_pairs(${1:source::input(0)}, ${2:relative_distance})",
                    ),
                    (
                        "require_type_lock_metapoint_pairs",
                        "ckb::require_type_lock_metapoint_pairs(${1:source::input(0)}, ${2:relative_distance})",
                    ),
                    (
                        "require_lock_type_metapoint_pairs_from_i32_data",
                        "ckb::require_lock_type_metapoint_pairs_from_i32_data(${1:source::input(0)}, ${2:distance_offset})",
                    ),
                    (
                        "require_type_lock_metapoint_pairs_from_i32_data",
                        "ckb::require_type_lock_metapoint_pairs_from_i32_data(${1:source::input(0)}, ${2:distance_offset})",
                    ),
                    (
                        "require_lock_type_metapoint_pairs_from_i32_data_filtered",
                        "ckb::require_lock_type_metapoint_pairs_from_i32_data_filtered(${1:source::input(0)}, ${2:distance_offset}, ${3:expected_related_type_hash}, ${4:related_data_rule})",
                    ),
                    (
                        "require_type_lock_metapoint_pairs_from_i32_data_filtered",
                        "ckb::require_type_lock_metapoint_pairs_from_i32_data_filtered(${1:source::input(0)}, ${2:distance_offset}, ${3:expected_related_type_hash}, ${4:related_data_rule})",
                    ),
                    (
                        "require_lock_match_master_out_point_pairs_from_data",
                        "ckb::require_lock_match_master_out_point_pairs_from_data(${1:source::input(0)}, ${2:source::output(0)}, ${3:action_offset}, ${4:tx_hash_offset}, ${5:index_offset})",
                    ),
                    ("cell_data_size", "ckb::cell_data_size(${1:source::group_input(0)})"),
                    ("cell_count", "ckb::cell_count(${1:source::input(0)})"),
                    ("verify_cell_roles", "ckb::verify_cell_roles(${1:64}, ${2:64}, ${3:32}, ${4:32768}, ${5:roles})"),
                    ("select_cell_roles", "ckb::select_cell_roles(${1:64}, ${2:64}, ${3:32}, ${4:32768}, ${5:roles})"),
                    ("selected_input_count", "ckb::selected_input_count::<${1:CellType}>(${2:selection}, ${3:0})"),
                    ("selected_output_count", "ckb::selected_output_count::<${1:CellType}>(${2:selection}, ${3:0})"),
                    ("selected_input", "ckb::selected_input::<${1:CellType}>(${2:selection}, ${3:0}, ${4:0})"),
                    ("selected_output", "ckb::selected_output::<${1:CellType}>(${2:selection}, ${3:0}, ${4:0})"),
                    ("selected_successor", "ckb::selected_successor::<${1:CellType}>(${2:selection}, ${3:0}, ${4:0})"),
                    ("exact_cell_role", "ckb::exact_cell_role::<${1:CellType}>(\"${2:complete_type_hash}\", \"${3:complete_lock_hash}\", \"${4:id}\", \"${5:observe}\", \"${6:preserve}\", ${7:0}, ${8:32}, ${9:0}, ${10:32})"),
                    ("cell_has_type", "ckb::cell_has_type(${1:source::input(0)})"),
                    ("cell_data_u8", "ckb::cell_data_u8(${1:source::group_input(0)}, ${2:0})"),
                    ("cell_lock_size", "ckb::cell_lock_size(${1:source::group_input(0)})"),
                    ("cell_type_size", "ckb::cell_type_size(${1:source::group_input(0)})"),
                    ("cell_lock_u8", "ckb::cell_lock_u8(${1:source::group_input(0)}, ${2:0})"),
                    ("cell_type_u8", "ckb::cell_type_u8(${1:source::group_input(0)}, ${2:0})"),
                    ("input_since_at", "ckb::input_since_at(${1:source::input(0)})"),
                    ("exec_cell_dep_u8_args", "ckb::exec_cell_dep_u8_args(${1:0}, ${2:0}, ${3:0}, ${4:0}, ${5:0}, ${6:0})"),
                    ("trusted_exec_cell_dep_u8_args", "ckb::trusted_exec_cell_dep_u8_args(${1:0}, ${2:code_hash}, ${3:0}, ${4:0}, ${5:0}, ${6:0}, ${7:0})"),
                    ("exec_cell_dep_hex4", "ckb::exec_cell_dep_hex4(${1:0}, ${2:bytes}, ${3:0}, ${4:0}, ${5:0}, ${6:0})"),
                    ("trusted_exec_cell_dep_hex4", "ckb::trusted_exec_cell_dep_hex4(${1:0}, ${2:code_hash}, ${3:bytes}, ${4:0}, ${5:0}, ${6:0}, ${7:0})"),
                    ("spawn_wait_cell_dep_hex4", "ckb::spawn_wait_cell_dep_hex4(${1:0}, ${2:bytes}, ${3:0}, ${4:0}, ${5:0}, ${6:0})"),
                    ("trusted_spawn_wait_cell_dep_hex4", "ckb::trusted_spawn_wait_cell_dep_hex4(${1:0}, ${2:code_hash}, ${3:bytes}, ${4:0}, ${5:0}, ${6:0}, ${7:0})"),
                    ("cell_data_u32_le", "ckb::cell_data_u32_le(${1:source::group_input(0)}, ${2:0})"),
                    ("cell_data_u64_le", "ckb::cell_data_u64_le(${1:source::group_input(0)}, ${2:0})"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("ckb::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                self.named_role_call_completions(uri, &mut items);
                return items;
            }
            "zk" => {
                items.push(CompletionItem {
                    label: "require_valid".into(),
                    kind: CompletionItemKind::Function,
                    detail: Some("Verify the sole ZkProof<Policy> witness using its package-bound verifier".into()),
                    documentation: None,
                    insert_text: Some("zk::require_valid(${1:proof})".into()),
                });
                items.push(CompletionItem {
                    label: "require_valid".into(), kind: CompletionItemKind::Function,
                    detail: Some("Exact Groth16 single Type-group transition; returns Unit and rejects child failures".into()),
                    documentation: None,
                    insert_text: Some("zk::require_valid(\"${1:policy}\", ${2:proof}, ${3:dependency}, ${4:verifier}, ${5:handle_hash}, ${6:vk_hash}, ${7:domain}, ${8:action})".into()),
                });
                return items;
            }
            "VerificationKeyCommitment" => {
                items.push(CompletionItem {
                    label: "from_bytes".into(),
                    kind: CompletionItemKind::Function,
                    detail: Some("Literal 32-byte verification-key CellDep data hash".into()),
                    documentation: None,
                    insert_text: Some("VerificationKeyCommitment::from_bytes(${1:bytes})".into()),
                });
                return items;
            }
            "verifier::btc::bip340" => {
                for (name, insert) in [
                    (
                        "require_signature",
                        "verifier::btc::bip340::require_signature(${1:message_hash}, ${2:pubkey}, ${3:signature})",
                    ),
                    (
                        "require_signature_from_cell_dep",
                        "verifier::btc::bip340::require_signature_from_cell_dep(${1:dep_index}, ${2:message_hash}, ${3:pubkey}, ${4:signature})",
                    ),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("verifier::btc::bip340::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "dao" => {
                for (name, insert) in [
                    ("accumulated_rate", "dao::accumulated_rate(${1:source::header_dep(0)})"),
                    ("input_accumulated_rate", "dao::input_accumulated_rate(${1:source::group_input(0)})"),
                    ("has_dao_type", "dao::has_dao_type(${1:source::group_input(0)})"),
                    ("is_deposit_data", "dao::is_deposit_data(${1:source::group_input(0)})"),
                    ("is_withdrawal_request_data", "dao::is_withdrawal_request_data(${1:source::group_input(0)})"),
                    (
                        "require_header_dep_for_input",
                        "dao::require_header_dep_for_input(${1:source::group_input(0)}, ${2:source::header_dep(0)})",
                    ),
                    (
                        "require_input_since_at_least",
                        "dao::require_input_since_at_least(${1:source::group_input(0)}, ${2:required_since})",
                    ),
                    (
                        "require_input_relative_epoch_since_at_least",
                        "dao::require_input_relative_epoch_since_at_least(${1:source::group_input(0)}, ${2:number}, ${3:index}, ${4:length})",
                    ),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("dao::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "c256" => {
                for (name, insert) in [
                    (
                        "require_product_lte",
                        "c256::require_product_lte(${1:left_amount}, ${2:left_multiplier}, ${3:right_amount}, ${4:right_multiplier})",
                    ),
                    (
                        "require_product_eq",
                        "c256::require_product_eq(${1:left_amount}, ${2:left_multiplier}, ${3:right_amount}, ${4:right_multiplier})",
                    ),
                    (
                        "require_sum2_products_lte",
                        "c256::require_sum2_products_lte(${1:left_amount_a}, ${2:left_multiplier_a}, ${3:left_amount_b}, ${4:left_multiplier_b}, ${5:right_amount_a}, ${6:right_multiplier_a}, ${7:right_amount_b}, ${8:right_multiplier_b})",
                    ),
                    (
                        "require_sum2_products_eq",
                        "c256::require_sum2_products_eq(${1:left_amount_a}, ${2:left_multiplier_a}, ${3:left_amount_b}, ${4:left_multiplier_b}, ${5:right_amount_a}, ${6:right_multiplier_a}, ${7:right_amount_b}, ${8:right_multiplier_b})",
                    ),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("c256::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "xudt" => {
                for (name, insert) in [
                    ("amount_low", "xudt::amount_low(${1:source::group_input(0)})"),
                    ("amount_high", "xudt::amount_high(${1:source::group_input(0)})"),
                    ("owner_mode_input_type_hash", "xudt::owner_mode_input_type_hash(${1:source::group_input(0)})"),
                    (
                        "require_owner_mode_input_type",
                        "xudt::require_owner_mode_input_type(${1:source::group_input(0)}, ${2:expected_type_hash})",
                    ),
                    (
                        "require_owner_mode_type_args",
                        "xudt::require_owner_mode_type_args(${1:source::group_input(0)}, ${2:owner_hash}, ${3:2147483648})",
                    ),
                    (
                        "require_owner_mode_type_args_current_script",
                        "xudt::require_owner_mode_type_args_current_script(${1:source::group_input(0)}, ${2:2147483648})",
                    ),
                    ("require_group_amount_conserved", "xudt::require_group_amount_conserved()"),
                    ("require_group_amount_minted", "xudt::require_group_amount_minted(${1:delta})"),
                    ("require_group_amount_burned", "xudt::require_group_amount_burned(${1:delta})"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("xudt::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            "Address" => {
                items.push(CompletionItem {
                    label: "zero".to_string(),
                    kind: CompletionItemKind::Function,
                    detail: Some("Address::zero".to_string()),
                    documentation: None,
                    insert_text: Some("Address::zero()".to_string()),
                });
                return items;
            }
            "Hash" => {
                for (name, insert) in [
                    ("zero", "Hash::zero()"),
                    ("from_bytes", "Hash::from_bytes(${1:b\"\\\\x00\\\\x00...\"})"),
                    ("from_sighash_all", "Hash::from_sighash_all(${1:digest})"),
                ] {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("Hash::{}", name)),
                        documentation: None,
                        insert_text: Some(insert.to_string()),
                    });
                }
                return items;
            }
            _ => {}
        }

        if let Some(definition) = self.completion_type_definition(uri, type_name) {
            use crate::resolve::TypeDef;
            let fields = match definition {
                TypeDef::Resource(def) => def.fields,
                TypeDef::Shared(def) => def.fields,
                TypeDef::Receipt(def) => def.fields,
                TypeDef::Struct(def) => def.fields,
                TypeDef::Enum(_) => Vec::new(),
            };
            for field in fields {
                items.push(CompletionItem {
                    label: field.name.clone(),
                    kind: CompletionItemKind::Field,
                    detail: Some(format!("{}: {}", field.name, type_to_string(&field.ty))),
                    documentation: None,
                    insert_text: Some(field.name),
                });
            }
        }

        items
    }

    /// Completions for local variables visible at `position`.
    fn local_completions(&self, source: &str, module: &Module, position: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        for item in &module.items {
            let (params, body) = match item {
                Item::Action(a) => (&a.params, &a.body),
                Item::Function(f) => (&f.params, &f.body),
                Item::Lock(l) => (&l.params, &l.body),
                _ => continue,
            };

            // Check if position is inside this function's span.
            let func_range = span_to_range(source, item_span(item));
            if !position_in_range(position, func_range) {
                continue;
            }

            // Add parameters.
            for param in params {
                items.push(CompletionItem {
                    label: param.name.clone(),
                    kind: CompletionItemKind::Variable,
                    detail: Some(format!("param: {}", type_to_string(&param.ty))),
                    documentation: None,
                    insert_text: Some(param.name.clone()),
                });
            }

            // Add local `let` bindings that are in scope (before position).
            for stmt in body {
                let stmt_range = span_to_range(source, stmt_span(stmt));
                if position_in_range(position, stmt_range) || position_le(stmt_range.start, position) {
                    // We are past the position, stop.
                    if position_le(position, stmt_range.start) && !position_in_range(position, stmt_range) {
                        break;
                    }
                }
                if let Stmt::Let(let_stmt) = stmt
                    && let BindingPattern::Name(name) = &let_stmt.pattern
                {
                    items.push(CompletionItem {
                        label: name.clone(),
                        kind: CompletionItemKind::Variable,
                        detail: Some(format!(
                            "let{}: {}",
                            if let_stmt.is_mut { " mut" } else { "" },
                            let_stmt.ty.as_ref().map(type_to_string).unwrap_or_else(|| "_".to_string())
                        )),
                        documentation: None,
                        insert_text: Some(name.clone()),
                    });
                }
            }
        }

        items
    }

    pub(super) fn keyword_completions(&self, uri: &str) -> Vec<CompletionItem> {
        let mut keywords = vec![
            ("new", "new ${1:pair} { ${2:field}: ${3:value}, }"),
            ("generic_call", "${1:identity}::<${2:u64}>(${3:value})"),
            ("module", "module ${1:name};"),
            ("use", "use ${1:path};"),
            ("resource", "resource ${1:Name} {\n    $0\n}"),
            ("shared", "shared ${1:Name} {\n    $0\n}"),
            ("receipt", "receipt ${1:Name} {\n    $0\n}"),
            ("struct", "struct ${1:Name} {\n    $0\n}"),
            ("action", "action ${1:name}(input ${2:cell}: ${3:CellType}) -> ${4:output}: ${3:CellType} {\n    $0\n}"),
            ("flow", "flow ${1:Name} for ${2:Type}.${3:state} {\n    ${4:Created} -> ${5:Live};\n}"),
            ("input", "input ${1:name}: ${2:CellType}"),
            ("transition", "transition ${1:input} -> ${2:output};"),
            ("lock", "lock ${1:name}(protected ${2:cell}: ${3:CellType}) -> bool {\n    require ${4:authorization_condition};\n    true\n}"),
            ("let", "let ${1:name} = $0;"),
            ("if", "if ${1:condition} {\n    $0\n}"),
            ("for", "for ${1:item} in ${2:iterable} {\n    $0\n}"),
            ("while", "while ${1:condition} {\n    $0\n}"),
            ("label", "label ${1:name}: ${2:while} ${3:condition} {\n    $0\n}"),
            ("break", "break${1: label};"),
            ("continue", "continue${1: label};"),
            ("borrow", "borrow ${1:root} as ${2:view} {\n    $0\n}"),
            ("return", "return $0;"),
            ("create", "create ${1:output} = ${2:Type} { $0 }"),
            ("consume", "consume ${1:input};"),
            ("destroy", "destroy ${1:expr};"),
            ("require", "require ${1:condition};"),
            ("forall", "forall ${1:output} ${2:item} in ${3:group_outputs<CellType>} {\n    require ${2:item}.${4:field} ${5:>} ${6:0};\n}"),
            ("count", "count(${1:outputs<CellType>} where ${2:field} == ${3:value}) == ${4:1};"),
            ("consume_each", "consume_each ${1:item} in ${2:inputs} {\n    require ${1:item}.${3:field} ${4:>} ${5:0};\n}"),
            ("create_each", "create_each ${1:plan} in ${2:plans} {\n    require ${1:plan}.${3:field} ${4:>} ${5:0};\n    create ${6:CellType} { $0 }\n}"),

            ("validity", "validity\n    require ${1:field} ${2:>} ${3:0};"),
            ("fixed_value", "fixed_value"),
            ("require_block", "require {\n    ${1:condition},\n};"),
            ("preserve", "preserve ${1:output} from ${2:input} {\n    ${3:field},\n}"),
            ("std::cell::same_lock", "std::cell::same_lock(${1:output}, ${2:input});"),
            ("replace", "replace ${1:input} -> ${2:output} {\n    data = same except {\n        $0\n    };\n    lock = same;\n    type_script = same;\n    capacity = same;\n}"),
            ("std::cell::preserve_lock", "std::cell::preserve_lock(${1:output}, ${2:input});"),
            ("std::cell::preserve_type", "std::cell::preserve_type(${1:output}, ${2:input});"),
            ("std::cell::preserve_capacity", "std::cell::preserve_capacity(${1:output}, ${2:input});"),
            ("std::lifecycle::transfer", "std::lifecycle::transfer(${1:input}, ${2:output}, ${3:to}) {\n    ${4:field},\n};"),
            ("std::receipt::claim", "std::receipt::claim(${1:receipt}, ${2:output}, ${3:lock}) {\n    ${4:field},\n};"),
            ("std::lifecycle::settle", "std::lifecycle::settle(${1:input}, ${2:output}, ${3:lock}) {\n    ${4:field},\n};"),
            ("protected", "protected ${1:cell}: ${2:CellType}"),
            ("witness", "witness ${1:arg}: ${2:Address}"),
            ("lock_args", "lock_args ${1:args}: ${2:OwnerArgs}"),
        ];
        {
            keywords.extend([
                ("type_script", "type_script ${1:Name} on type_group<${2:CellType}> {\n    $0\n}"),
                ("lock_script", "lock_script ${1:Name} on lock_group {\n    $0\n}"),
            ]);
        }
        if self.named_runtime_authoring(uri) {
            keywords.extend_from_slice(cell_views::NAMED_RUNTIME_DECLARATIONS);
        }
        if self.documents.get(uri).is_some_and(|source| crate::frontend::uses_native_preview(source)) {
            keywords.retain(|(label, _)| !matches!(*label, "consume" | "consume_each"));
            keywords.extend([
                ("lock_group", "lock_group"),
                ("current_script", "current_script.args"),
                ("pool", "pool ${1:name} {\n    inputs { ${2:input}, }\n    outputs { ${3:output}, }\n    data {\n        ${4:amount} = conserve;\n    }\n    identity = pooled;\n    type_script = same;\n    lock { ${3:output} = exact(${5:recipient}); }\n    capacity = builder_computed;\n    cardinality = declared;\n}"),
                ("retire", "retire ${1:input} {\n    absence = ${2:field(identity)};\n    data = discarded;\n    lock = none;\n    type_script = absent;\n    capacity = released;\n    cardinality = one;\n}"),
                ("fresh", "fresh ${1:output} {\n    data {\n        ${2:field} = ${3:value};\n    }\n    identity = ${4:none};\n    type_script = declared;\n    lock = exact(${5:recipient});\n    capacity = builder_computed;\n    cardinality = one;\n}"),
                ("audit", "audit ${1:name} {\n    expected_evidence = external_policy(${2:subject});\n}"),
                ("exact_hash", "exact_hash(${1:script_hash})"),
            ]);
        }

        keywords
            .into_iter()
            .map(|(label, insert)| CompletionItem {
                label: label.to_string(),
                kind: CompletionItemKind::Keyword,
                detail: Some(format!("{} keyword", label)),
                documentation: None,
                insert_text: Some(insert.to_string()),
            })
            .collect()
    }

    pub(super) fn type_completions(&self) -> Vec<CompletionItem> {
        let types = vec![
            "u8",
            "u16",
            "u32",
            "u64",
            "u128",
            "i8",
            "i16",
            "i32",
            "i64",
            "i128",
            "bool",
            "String",
            "Address",
            "Hash",
            "ScriptHash",
            "ExactScriptHandle",
            "ScriptHandle",
            "InstanceScriptHandle",
            "VerifierHandle",
            "ZkProof",
            "ZkTransitionProof",
            "VerificationKeyCommitment",
            "DeploymentLineHandle",
            "SighashAllDigest",
            "EpochNumber",
            "EpochDuration",
            "BlockNumber",
            "EpochLength",
            "TimestampMillis",
            "EncodedSince",
            "DecodedSince",
            "AbsoluteBlockSince",
            "AbsoluteEpochSince",
            "AbsoluteTimestampSince",
            "RelativeBlockSince",
            "RelativeEpochSince",
            "RelativeTimestampSince",
            "Bytes",
            "Option",
            "Vec",
            "BoundedCellSet",
            "BoundedList",
            "NestedValues",
            "NestedBytesView",
            "NestedRecordView",
        ];

        types
            .into_iter()
            .map(|ty| CompletionItem {
                label: ty.to_string(),
                kind: CompletionItemKind::TypeParameter,
                detail: Some(format!("{} type", ty)),
                documentation: match ty {
                    "NestedValues" => Some("Bounded NestedValues<T, N, P, B>: immutable bounded lists for explicit witnesses or direct Cell fields, containing Vec<u8> or records with a trailing byte payload. Bounds: N <= 32, P <= 256, 4 <= B <= 4096. The complete entry witness also has a 4096-byte budget. Native artifact checking requires codec, source, transport and lifetime evidence; browser metadata alone is not machine evidence.".into()),
                    "NestedBytesView" => Some("NestedBytesView<P>: invocation-local read-only payload derived by indexing NestedValues. Supports len, is_empty, iteration and checked byte indexing. Supports checked read-only helper parameters; cannot be constructed, mutated, stored or returned.".into()),
                    "NestedRecordView" => Some("NestedRecordView<T, P>: invocation-local read-only record derived by indexing NestedValues. Fixed fields retain their declared value types; the trailing payload is NestedBytesView<P>. Supports checked read-only helper parameters; cannot escape through storage or return values.".into()),
                    "ScriptHandle" => Some("ScriptHandle<I>: a 656-byte nominal imported-interface selection for a checked Lock/Type participant; accepted as an action or Lock witness. Membership does not prove peer acceptance.".into()),
                    "InstanceScriptHandle" => Some("InstanceScriptHandle<I>: a versioned 1768-byte selection combining implementation membership with bounded Script-args constraints. Its consumer commits a policy, concrete subject and configuration in 208-byte CSINCv1 args. Observation grants no ownership or peer-execution authority.".into()),
                    "VerifierHandle" => Some("VerifierHandle<I>: a 656-byte nominal imported-interface selection. Native admission requires the parameterless child profile and unconditional checked SPAWN/WAIT after committed-args membership.".into()),
                    _ => None,
                },
                insert_text: None,
            })
            .collect()
    }

    pub(super) fn symbol_completions(&self, module: &Module) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        for item in &module.items {
            match item {
                Item::Collections(def) => items.push(CompletionItem {
                    label: def.name.clone(), kind: CompletionItemKind::Struct,
                    detail: Some(format!("collections {}", def.name)),
                    documentation: Some("Shared named allocation policy with local owner and read-only helper typing. Local operations and read-only helpers use checked allocation, bounds and lifetime contracts.".into()),
                    insert_text: Some(def.name.clone()),
                }),
                Item::DataShape(def) => items.push(CompletionItem {
                    label: def.name.clone(), kind: CompletionItemKind::Struct,
                    detail: Some(format!("data_shape {}", def.name)),
                    documentation: Some(format!("Bounded codec shape: {} items, {} bytes per payload, {} encoded bytes. Entry-witness budget is separate.", def.limits.items, def.limits.payload_bytes, def.limits.encoded_bytes)),
                    insert_text: Some(def.name.clone()),
                }),
                Item::CellRoles(def) => items.push(CompletionItem {
                    label: def.name.clone(), kind: CompletionItemKind::Constant,
                    detail: Some(format!("cell_roles {}", def.name)),
                    documentation: Some("Bounded exact-role catalog. Verify with ckb::verify_cell_roles::<Catalog>() or select with ckb::select_cell_roles::<Catalog>() and read named roles. Native artifact checking requires source, authority and lifetime evidence.".into()),
                    insert_text: Some(def.name.clone()),
                }),
                Item::Resource(r) => {
                    items.push(CompletionItem {
                        label: r.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("resource {}", r.name)),
                        documentation: None,
                        insert_text: Some(r.name.clone()),
                    });
                }
                Item::Shared(s) => {
                    items.push(CompletionItem {
                        label: s.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("shared {}", s.name)),
                        documentation: None,
                        insert_text: Some(s.name.clone()),
                    });
                }
                Item::Receipt(r) => {
                    items.push(CompletionItem {
                        label: r.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("receipt {}", r.name)),
                        documentation: None,
                        insert_text: Some(r.name.clone()),
                    });
                }
                Item::Struct(s) => {
                    items.push(CompletionItem {
                        label: s.name.clone(),
                        kind: CompletionItemKind::Struct,
                        detail: Some(format!("struct {}", s.name)),
                        documentation: None,
                        insert_text: Some(s.name.clone()),
                    });
                }
                Item::Invariant(i) => {
                    items.push(CompletionItem {
                        label: i.name.clone(),
                        kind: CompletionItemKind::Keyword,
                        detail: Some(format!("invariant {}", i.name)),
                        documentation: None,
                        insert_text: Some(i.name.clone()),
                    });
                }
                Item::Action(a) => {
                    items.push(CompletionItem {
                        label: a.name.clone(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("action {}", a.name)),
                        documentation: a.doc_comment.clone(),
                        insert_text: Some(format!("{}($0)", a.name)),
                    });
                }
                Item::Lock(l) => {
                    items.push(CompletionItem {
                        label: l.name.clone(),
                        kind: CompletionItemKind::Function,
                        detail: Some(format!("lock {}", l.name)),
                        documentation: None,
                        insert_text: Some(format!("{}($0)", l.name)),
                    });
                }
                _ => {}
            }
        }

        items
    }
}
