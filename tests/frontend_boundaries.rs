use cellscript::{error::SourceDiagnosticCode as Code, frontend, lexer, CellScriptEdition};

#[test]
fn reserved_mutable_reference_diagnostics_keep_mutable_values_and_readonly_views_distinct() {
    let source = include_str!("syntax_combo/seeds/reserved-mutable-reference-reject.cell");
    // Reserved reference syntax is losslessly recognized; executable admission
    // is a separate typed boundary, shared with editor/browser diagnostics.
    frontend::parse_document(source, CellScriptEdition::Edition2027).unwrap();
    let error = cellscript::compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap_err();
    assert!(error.message.contains("references are reserved"), "{error}");
    let diagnostics = cellscript::compile_metadata_with_diagnostics(source, CellScriptEdition::Edition2027, None);
    assert!(diagnostics.metadata.is_none());
    assert!(diagnostics.diagnostics.iter().any(|diagnostic| diagnostic.message == error.message));
    #[cfg(feature = "lsp")]
    {
        let mut server = cellscript::lsp::LspServer::new();
        let uri = "file:///reserved-reference.cell";
        server.open_document(uri.into(), source.into());
        assert!(server.get_diagnostics(uri).iter().any(|diagnostic| diagnostic.message == error.message));
    }
    let valid = include_str!("fixtures/authoring/mutable-values-and-readonly-views.cell");
    let result =
        cellscript::compile(valid, cellscript::CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    result.validate().unwrap();
    assert!(result.metadata.runtime.fail_closed_runtime_features.is_empty());
}

#[test]
fn named_arrow_outputs_have_shared_migration_diagnostics() {
    for declaration in [
        "action entry() -> next: Token {}",
        "action entry() -> /* output */ next\n/* type separator */ : Token {}",
        "action entry() -> (next: Token) {}",
        "action entry() -> (\n next: Token, change: Token,\n) {}",
        "action entry(output first: Token) -> next: Token {}",
    ] {
        let source = format!("module output_migration; {declaration}");
        let expected = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!(expected.code.as_deref(), Some(Code::RemovedAuthoringSpelling.code()));
        assert!(expected.message.contains("output name: Type"), "{expected}");
        for error in [
            frontend::parse_document(&source, CellScriptEdition::Edition2027).err().unwrap(),
            cellscript::fmt::format_source(&source, CellScriptEdition::Edition2027, Default::default()).unwrap_err(),
            cellscript::compile_metadata(&source, CellScriptEdition::Edition2027, None).unwrap_err(),
            frontend::parse_diagnostics(&source, CellScriptEdition::Edition2027).unwrap_err().remove(0),
        ] {
            assert_eq!((&error.code, &error.message, error.span), (&expected.code, &expected.message, expected.span));
        }
        #[cfg(feature = "lsp")]
        {
            let mut server = cellscript::lsp::LspServer::new();
            server.open_document("file:///named-arrow.cell".into(), source);
            assert!(server.get_diagnostics("file:///named-arrow.cell").iter().any(|error| error.message == expected.message));
        }
    }
}

#[test]
fn ordinary_value_tuple_and_anonymous_cell_returns_keep_their_domain() {
    for declaration in [
        "fn identity(value: u64) -> u64 { value }",
        "fn pair(value: u64) -> (u64, bool) { (value, true) }",
        "action amount(witness value: u64) -> u64 { value }",
        "action pair(witness value: u64) -> (u64, bool) { (value, true) }",
        "action mint(witness value: u64, witness recipient: Address) -> Token { create Token { amount: value, } with_lock(recipient) }",
    ] {
        let source = format!("module returned_values; resource Token has store, create {{ amount: u64, }} {declaration}");
        let document = frontend::parse_document(&source, CellScriptEdition::Edition2027).unwrap();
        let formatted = cellscript::fmt::format_source(&source, CellScriptEdition::Edition2027, Default::default()).unwrap();
        match document.ast.items.last().unwrap() {
            cellscript::ast::Item::Action(action) => {
                assert!(action.outputs.is_empty(), "a returned value is not a named Cell role");
                assert!(action.return_type.is_some());
            }
            cellscript::ast::Item::Function(function) => assert!(function.return_type.is_some()),
            item => panic!("expected a callable, got {item:?}"),
        }
        assert_eq!(formatted, cellscript::fmt::format_source(&formatted, CellScriptEdition::Edition2027, Default::default()).unwrap());
        let before = cellscript::compile_metadata(&source, CellScriptEdition::Edition2027, None).unwrap();
        let after = cellscript::compile_metadata(&formatted, CellScriptEdition::Edition2027, None).unwrap();
        assert_eq!(before.typed_semantics, after.typed_semantics);
    }
}

#[test]
fn inferred_and_explicit_input_sources_share_accepting_path_checks() {
    for body in [
        "if flag { consume token; }",
        "while flag { consume token; }",
        "for index in 0..1 { consume token; }",
        "if flag { return; } else { consume token; return; }",
    ] {
        let source = format!("module input_paths; resource Token has store, consume {{ amount:u64, }} action bad(token:Token,witness flag:bool) {{ {body} }}");
        let explicit = source.replace("bad(token:", "bad(input token:");
        assert_ne!(source, explicit);
        let before = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap_err();
        let after = frontend::parse(&explicit, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!((&before.code, &before.message), (&after.code, &after.message));
        assert!(before.message.contains("every accepting path") || before.message.contains("inside a loop"), "{before}");
    }
}

#[test]
fn ownership_diagnostics_are_stable_across_fresh_compiler_instances() {
    let source = "module stable_ownership; resource Token has store, consume { amount:u64, } action bad(input left:Token,input right:Token,witness flag:bool) { let moved = if flag { left } else { right }; consume moved; }";
    let mut expected = None;
    for _ in 0..32 {
        let report = cellscript::compile_metadata_with_diagnostics(source, CellScriptEdition::Edition2027, None);
        assert!(report.metadata.is_none());
        let diagnostics = report.diagnostics.iter().map(|error| (error.code, error.message.clone())).collect::<Vec<_>>();
        assert!(diagnostics[0].1.contains("linear resource 'left' has inconsistent ownership state"), "{diagnostics:?}");
        if let Some(expected) = &expected {
            assert_eq!(&diagnostics, expected);
        } else {
            expected = Some(diagnostics);
        }
    }
}

#[test]
fn implicit_entry_sources_fail_after_resolving_their_actual_role() {
    for (declaration, spelling) in [
        ("action verify(value: u64) -> u64 { value }", "witness value: u64"),
        ("lock verify(value: u64) -> bool { value == 7 }", "witness value: u64"),
        ("action verify(mut value: u64) -> u64 { value = value + 1; value }", "mut witness value: u64"),
        ("action verify(value: [u8; 4]) { require value[0] == 7; }", "witness value: [u8; 4]"),
        ("action verify(value: Option<u64>) -> u64 { 0 }", "witness value: Option<u64>"),
        ("action verify(value: Vault) { consume value; }", "input value: Vault"),
    ] {
        let source = format!("module explicit_provenance; resource Vault has store, consume {{ value:u64, }} {declaration}");
        let error = cellscript::compile_metadata(&source, CellScriptEdition::Edition2027, None).unwrap_err();
        assert_eq!(error.code.as_deref(), Some(Code::RemovedAuthoringSpelling.code()), "{error}");
        assert!(error.message.contains(spelling), "{error}");
        let report = cellscript::compile_metadata_with_diagnostics(&source, CellScriptEdition::Edition2027, None);
        assert!(report.metadata.is_none());
        assert_eq!(report.diagnostics.iter().filter(|diagnostic| diagnostic.message == error.message).count(), 1);
        let canonical = source.replace(
            if declaration.contains("mut value") {
                "mut value: u64"
            } else if declaration.contains("value: [") {
                "value: [u8; 4]"
            } else if declaration.contains("value: Vault") {
                "value: Vault"
            } else if declaration.contains("value: Option") {
                "value: Option<u64>"
            } else {
                "value: u64"
            },
            spelling,
        );
        assert_ne!(source, canonical);
        cellscript::compile_metadata(&canonical, CellScriptEdition::Edition2027, None).unwrap();
        #[cfg(feature = "lsp")]
        {
            let mut server = cellscript::lsp::LspServer::new();
            server.open_document("file:///implicit-source.cell".into(), source);
            assert!(server
                .get_diagnostics("file:///implicit-source.cell")
                .iter()
                .any(|diagnostic| diagnostic.message == error.message));
        }
    }
}

#[test]
fn entry_source_migration_uses_imported_schema_identity_and_retains_read_helpers() {
    let schema =
        frontend::parse("module schema; resource Token has store, consume { value:u64, }", CellScriptEdition::Edition2027).unwrap();
    let module = frontend::parse(
        "module caller; use schema::Token as Asset; action verify(value: Asset) { consume value; }",
        CellScriptEdition::Edition2027,
    )
    .unwrap();
    let mut resolver = cellscript::resolve::ModuleResolver::new();
    resolver.register_module(schema).unwrap();
    resolver.register_module(module.clone()).unwrap();
    let error = cellscript::types::check_with_resolver(&module, &resolver, &module.name).unwrap_err();
    assert_eq!(error.code.as_deref(), Some(Code::RemovedAuthoringSpelling.code()));
    assert!(error.message.contains("input value: Asset"), "{error}");
    let source = "module explicit_reads; resource Config has store { value:u64, } fn amount(value: &Config) -> u64 { value.value } action verify(read config: Config) -> u64 { amount(config) }";
    cellscript::compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
}

#[test]
fn mutable_witness_value_bindings_keep_cell_and_reference_restrictions() {
    let source = "module local_witness; action increment(mut witness value: u64) -> u64 { value = value + 1; value }";
    let result =
        cellscript::compile(source, cellscript::CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    result.validate().unwrap();
    let parameter = &result.metadata.typed_semantics.entries[0].params[0];
    assert_eq!((parameter.source.as_str(), parameter.mutable, parameter.reference), ("witness", true, false));
    let formatted = cellscript::fmt::format_source(source, CellScriptEdition::Edition2027, Default::default()).unwrap();
    assert!(formatted.contains("mut witness value: u64"));
    assert_eq!(formatted, cellscript::fmt::format_source(&formatted, CellScriptEdition::Edition2027, Default::default()).unwrap());
    for (declaration, expected) in [
        ("action bad(witness value: u64) { value = 8; }", "not mutable"),
        ("action bad(mut witness value: Vault) { consume value; }", "cell-backed parameter"),
        ("action bad(mut witness value: &Vault) {}", "not a Cell reference"),
        ("action bad(mut witness value: BoundedList<Payout,16>) {}", "immutable bounded-plan"),
        ("action bad(mut witness value: Opening<Payout>) {}", "immutable bounded-plan"),
        ("lock bad(mut protected value: Vault) -> bool { true }", "without mut/ref/read_ref"),
        ("lock bad(mut lock_args value: u64) -> bool { true }", "plain typed script args"),
    ] {
        let source = format!(
            "module invalid_witness; struct Payout {{ value:u64, }} resource Vault has store, consume {{ value:u64, }} {declaration}"
        );
        let error = cellscript::compile_metadata(&source, CellScriptEdition::Edition2027, None).unwrap_err();
        assert!(error.message.contains(expected), "{error}");
    }
}

#[test]
fn postfix_entry_sources_have_precise_shared_migration_diagnostics() {
    for (declaration, role, ty) in [
        ("action", "witness", "u64"),
        ("fn", "witness", "[u8; 4]"),
        ("lock", "protected", "Vault"),
        ("lock", "lock_args", "Address"),
        ("action", "witness", "(u64, bool)"),
        ("fn", "witness", "&Vault"),
    ] {
        let source = format!("module roles; {declaration} entry(value: /* source */ {role}\n/* type */ {ty}) {{ true }}");
        let expected = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!(expected.code.as_deref(), Some(Code::RemovedAuthoringSpelling.code()));
        assert_eq!(&source[expected.span.start..expected.span.end], role);
        assert!(expected.message.contains(&format!("{role} value: Type")), "{expected}");
        let errors = [
            frontend::parse_document(&source, CellScriptEdition::Edition2027).err().unwrap(),
            cellscript::fmt::format_source(&source, CellScriptEdition::Edition2027, Default::default()).unwrap_err(),
            cellscript::compile_metadata(&source, CellScriptEdition::Edition2027, None).unwrap_err(),
            frontend::parse_diagnostics(&source, CellScriptEdition::Edition2027).unwrap_err().remove(0),
        ];
        for error in errors {
            assert_eq!((&error.code, &error.message, error.span), (&expected.code, &expected.message, expected.span));
        }
        #[cfg(feature = "lsp")]
        {
            let mut server = cellscript::lsp::LspServer::new();
            server.open_document("file:///postfix-source.cell".into(), source);
            assert!(server.get_diagnostics("file:///postfix-source.cell").iter().any(|error| error.message == expected.message));
        }
    }
}

#[test]
fn source_words_remain_nominal_types_and_qualified_type_paths() {
    for ty in ["witness", "protected", "lock_args", "witness<u64>", "types::protected", "types::lock_args<u64>"] {
        let source = format!("module contextual_sources; fn identity(value: {ty}) -> {ty} {{ value }}");
        let module = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap();
        let cellscript::ast::Item::Function(function) = &module.items[0] else { panic!("function") };
        assert_eq!(function.params[0].source, cellscript::ast::ParamSource::Default);
        assert_eq!(function.params[0].ty.source_syntax(), ty);
        let formatted = cellscript::fmt::format_source(&source, CellScriptEdition::Edition2027, Default::default()).unwrap();
        assert_eq!(formatted, cellscript::fmt::format_source(&formatted, CellScriptEdition::Edition2027, Default::default()).unwrap());
    }
}

#[test]
fn canonical_prefix_sources_preserve_comments_and_bindings() {
    let source = "module prefix_sources; resource Vault has store { value: u64, }\nlock unlock(\n protected /* bound Cell */ vault: Vault,\n lock_args limit: /* args */ u64,\n witness claimed: /* witness */ u64,\n) -> bool { vault.value == limit && claimed == limit }";
    let formatted = cellscript::fmt::format_source(source, CellScriptEdition::Edition2027, Default::default()).unwrap();
    for comment in ["/* bound Cell */", "/* args */", "/* witness */"] {
        assert_eq!(formatted.matches(comment).count(), 1);
    }
    let before = cellscript::compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
    let after = cellscript::compile_metadata(&formatted, CellScriptEdition::Edition2027, None).unwrap();
    assert_eq!(before.typed_semantics, after.typed_semantics);
    let entry = &before.typed_semantics.entries[0];
    assert_eq!(entry.params.iter().map(|param| param.source.as_str()).collect::<Vec<_>>(), ["protected", "lockargs", "witness"]);
}

#[test]
fn branch_lifecycle_source_formatting_preserves_comments_and_editor_admission() {
    for fixture in [
        include_str!("fixtures/authoring/branch-fresh.cell"),
        include_str!("fixtures/authoring/branch-retire.cell"),
        include_str!("fixtures/authoring/branch-pool-many.cell"),
        include_str!("fixtures/authoring/branch-continue-retire.cell"),
    ] {
        let source = fixture
            .replace("if choose {", "if choose { /* selected branch */")
            .replace("} else {", "} else { // alternate branch\n")
            .replace("absence =", "/* identity policy */ absence =")
            .replace("data {", "data { /* source field order */");
        let formatted = cellscript::fmt::format_source(&source, CellScriptEdition::Edition2027, Default::default()).unwrap();
        for marker in ["/* selected branch */", "// alternate branch", "/* identity policy */", "/* source field order */"] {
            assert_eq!(formatted.matches(marker).count(), source.matches(marker).count());
        }
        assert_eq!(cellscript::fmt::format_source(&formatted, CellScriptEdition::Edition2027, Default::default()).unwrap(), formatted);
        #[cfg(feature = "lsp")]
        {
            let mut server = cellscript::lsp::LspServer::new();
            server.open_document("file:///branch-lifecycle.cell".into(), formatted);
            let diagnostics = server.get_diagnostics("file:///branch-lifecycle.cell");
            assert!(
                diagnostics.iter().all(|diagnostic| diagnostic.severity != cellscript::lsp::DiagnosticSeverity::Error),
                "{diagnostics:?}"
            );
        }
    }
}

#[test]
fn lifecycle_context_and_audit_placement_report_the_actual_unsupported_boundary() {
    for policy in ["pool", "retire", "fresh", "audit"] {
        for body in [
            format!("if true {{ {policy} subject {{ }} }}"),
            format!("while true {{ {policy} subject {{ }} }}"),
            format!("{policy} subject {{ }}"),
        ] {
            let source = format!("module policy_placement; fn helper() {{ {body} }}");
            let error = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap_err();
            let expected = if policy == "audit" {
                "`audit` is an entry-level policy"
            } else {
                "lifecycle policies require an action entry with declared roles"
            };
            assert!(error.message.contains(expected), "{error}");
            assert_eq!(&source[error.span.start..error.span.end], policy);
            let formatted = cellscript::fmt::format_source(&source, CellScriptEdition::Edition2027, Default::default()).unwrap_err();
            assert_eq!(formatted.message, error.message);
            assert_eq!(formatted.span, error.span);
            let report = cellscript::compile_metadata_with_diagnostics(&source, CellScriptEdition::Edition2027, None);
            assert!(report.metadata.is_none());
            assert!(report.diagnostics.iter().any(|diagnostic| diagnostic.message == error.message));
            #[cfg(feature = "lsp")]
            {
                let mut server = cellscript::lsp::LspServer::new();
                server.open_document("file:///policy-placement.cell".into(), source.clone());
                assert!(server.get_diagnostics("file:///policy-placement.cell").iter().any(|diagnostic| {
                    diagnostic.message == error.message && diagnostic.severity == cellscript::lsp::DiagnosticSeverity::Error
                }));
            }
        }
    }
    let source = "module policy_names; fn pool(n: u64) -> u64 { n } fn retire(n: u64) -> u64 { n } fn fresh(n: u64) -> u64 { n } fn audit(n: u64) -> u64 { n } action inspect() -> u64 { pool(1) + retire(2) + fresh(3) + audit(4) }";
    cellscript::compile_metadata(source, CellScriptEdition::Edition2027, None).unwrap();
}

#[test]
fn unrepresentable_fixed_extents_return_diagnostics_before_metadata_or_codegen() {
    let maximum = usize::MAX;
    let mut types = vec![format!("[u64; {maximum}]")];
    if usize::BITS == 64 {
        types.push(format!("[u8; {maximum}]"));
        types.push(format!("([u8; {}], u8)", i64::MAX));
        types.push(format!("[u8; {}]", u64::from(u32::MAX) + 1));
        types.push(format!("([u8; {}], u8)", u32::MAX));
    }
    for ty in types {
        {
            let edition = CellScriptEdition::Edition2027;
            let source = format!("module fixed_layout;\naction inspect(witness payload: {ty}) {{  }}");
            let metadata = cellscript::compile_metadata(&source, edition, None).unwrap_err();
            let artifact = cellscript::compile(&source, cellscript::CompileOptions { edition, ..Default::default() }).unwrap_err();
            for error in [&metadata, &artifact] {
                assert_eq!(error.code.as_deref(), Some(Code::FixedLayoutBounds.code()), "{error}");
                assert!(error.span.start > 0 && error.span.end > error.span.start, "{error}");
            }
        }
    }
    let valid = "module fixed_layout;\naction inspect(witness payload: [u8; 32]) {  require payload[0] == 7; }";
    assert!(cellscript::compile(valid, Default::default()).is_ok());
}

#[test]
#[cfg(target_pointer_width = "64")]
fn oversized_fixed_aliases_do_not_reach_typed_parameter_alignment() {
    let source = format!(
        "module fixed_alias;\naction inspect(witness payload: [u8; {}]) {{ let alias = payload; require alias[0] == 7; }}",
        i64::MAX
    );
    {
        let edition = CellScriptEdition::Edition2027;
        let report = cellscript::compile_metadata_with_diagnostics(&source, edition, None);
        assert!(report.metadata.is_none());
        assert_eq!(report.diagnostics[0].code.as_deref(), Some(Code::FixedLayoutBounds.code()));
        let error = cellscript::compile(&source, cellscript::CompileOptions { edition, ..Default::default() }).unwrap_err();
        assert_eq!(error.code.as_deref(), Some(Code::FixedLayoutBounds.code()));
        assert!(error.span.start > 0 && error.span.end > error.span.start);
    }
}

#[test]
fn oversized_aggregate_rebuilds_reject_before_any_expansion_budget() {
    // [u8; 100000000] fits the u32 fixed-layout representation bound, so the
    // only guard between the declared type and component-by-component rebuild
    // work is the aggregate rebuild budget. The rejection must precede any
    // materialization on every consumer path, including unexecuted actions.
    let source =
        "module aggregate_rebuild;\naction inspect(witness payload: [u8; 100000000]) {\n    let mut xs = payload;\n    xs[0] = 7;\n}";
    {
        let edition = CellScriptEdition::Edition2027;
        let report = cellscript::compile_metadata_with_diagnostics(source, edition, None);
        assert!(report.metadata.is_none());
        assert!(
            report.diagnostics.iter().any(|error| error.message.contains("rebuilds 100000000 components")),
            "{:?}",
            report.diagnostics
        );
        let error = cellscript::compile(source, cellscript::CompileOptions { edition, ..Default::default() }).unwrap_err();
        assert!(error.message.contains("rebuilds 100000000 components"), "{error}");
    }
    #[cfg(feature = "lsp")]
    {
        let mut server = cellscript::lsp::LspServer::new();
        server.open_document_with_edition("file:///aggregate_rebuild.cell".into(), source.into(), CellScriptEdition::Edition2027);
        assert!(server
            .get_diagnostics("file:///aggregate_rebuild.cell")
            .iter()
            .any(|diagnostic| diagnostic.message.contains("rebuilds 100000000 components")));
    }
    let admitted = "module aggregate_rebuild;\naction inspect(witness payload: [u8; 32]) {\n    let mut xs = payload;\n    xs[0] = 7;\n    require xs[0] == 7;\n}";
    assert!(cellscript::compile_metadata(admitted, CellScriptEdition::Edition2027, None).is_ok());
}

#[test]
fn aggregate_rebuild_module_budget_bounds_cumulative_lowering_work() {
    // Sixteen rebuilds of a maximum-size aggregate exactly exhaust the module
    // budget; the seventeenth must reject before materializing its parts.
    let assignments = (0..17).map(|index| format!("    xs[{index}] = {index};")).collect::<Vec<_>>().join("\n");
    let source = format!(
        "module aggregate_module_budget;\naction inspect(witness payload: [u8; 65536]) {{\n    let mut xs = payload;\n{assignments}\n}}"
    );
    let edition = CellScriptEdition::Edition2027;
    let report = cellscript::compile_metadata_with_diagnostics(&source, edition, None);
    assert!(report.metadata.is_none());
    assert!(
        report.diagnostics.iter().any(|error| error.message.contains("1048576-component compiler budget")),
        "{:?}",
        report.diagnostics
    );
}

#[test]
fn reserved_specialization_names_are_diagnosed_before_internal_decoding() {
    for name in ["value__mono__汉字", "value__mono__éé", "value__mono__u64", "value__mono__0g"] {
        let source =
            format!("module unicode_names;\nfn {name}() -> u64 {{ return 7; }}\naction main() -> u64 {{  return {name}(); }}");
        {
            let edition = CellScriptEdition::Edition2027;
            let error = cellscript::compile_metadata(&source, edition, None).unwrap_err();
            assert_eq!(error.code.as_deref(), Some("E2110"));
            let error = cellscript::compile(&source, cellscript::CompileOptions { edition, ..Default::default() }).unwrap_err();
            assert_eq!(error.code.as_deref(), Some("E2110"));
        }
    }
}

fn pool(outputs: usize, assignments: &str) -> String {
    let roles = ["first", "second"][..outputs].join(", ");
    let declarations =
        (0..outputs).map(|index| format!("output {}: Token,", ["first", "second"][index])).collect::<Vec<_>>().join("\n");
    let locks = ["first", "second"][..outputs].iter().map(|name| format!("{name} = exact(recipient),")).collect::<Vec<_>>().join("\n");
    format!(
        r#"// π
module pool_boundary;
resource Token has store, create, consume {{ owner: Address, amount: u64 }}
action merge(
        input left: Token,
        input right: Token,
        witness recipient: Address,
        {declarations}
    ) {{
        require left.amount > 0;
        pool value_flow {{
            inputs {{ left, right }};
            outputs {{ {roles} }};
            data {{
                owner {{ {assignments} }},
                amount = conserve,
            }};
            lock {{ {locks} }};
        }}
    }}
"#
    )
}

fn check_frontend_error(source: &str, edition: CellScriptEdition, code: Code) {
    let error = frontend::parse(source, edition).unwrap_err();
    assert_eq!(error.code.as_deref(), Some(code.code()), "{error}");
    let diagnostics = frontend::parse_diagnostics(source, edition).unwrap_err();
    assert_eq!(diagnostics[0].code, error.code);
    let report = cellscript::compile_metadata_with_diagnostics(source, edition, None);
    assert!(report.metadata.is_none());
    assert_eq!(report.diagnostics[0].code, error.code);
    #[cfg(feature = "lsp")]
    {
        let mut server = cellscript::lsp::LspServer::new();
        server.open_document_with_edition("file:///boundary.cell".into(), source.into(), edition);
        assert!(server
            .get_diagnostics("file:///boundary.cell")
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == error.code.as_deref()));
    }
}

#[test]
fn native_dispositions_are_direct_and_the_removed_wrapper_rejects() {
    for source in [
        include_str!("syntax_combo/seeds/edition-2027-type-script.cell"),
        include_str!("syntax_combo/seeds/edition-2027-pool.cell"),
        include_str!("syntax_combo/seeds/edition-2027-retire-fresh.cell"),
        include_str!("syntax_combo/seeds/edition-2027-script-hash.cell"),
    ] {
        let ast = frontend::parse(source, CellScriptEdition::Edition2027).unwrap();
        let formatted = cellscript::fmt::format_default(&ast).unwrap();
        assert!(!formatted.contains("effects"));
        let reparsed = frontend::parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(cellscript::fmt::format_default(&reparsed).unwrap(), formatted);
        let start = formatted
            .lines()
            .find(|line| ["replace ", "pool ", "retire ", "fresh "].iter().any(|verb| line.trim_start().starts_with(verb)))
            .unwrap();
        let wrapped = formatted
            .replacen(start, &format!("        effects {{\n{start}"), 1)
            .rsplit_once("\n    }")
            .map(|(before, after)| format!("{before}\n        }}\n    }}{after}"))
            .unwrap();
        check_frontend_error(&wrapped, CellScriptEdition::Edition2027, Code::RemovedNativeEffectsBlock);
        let error = frontend::parse(&wrapped, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!(&wrapped[error.span.start..error.span.end], "effects");
    }
}

#[test]
fn pool_assignments_reject_before_lowering_for_one_and_multiple_outputs() {
    for (count, assignments) in [
        (1, ""),
        (2, "first = recipient,"),
        (2, "second = recipient,"),
        (2, "first = recipient,\nfirst = recipient,"),
        (2, "first = recipient,\nunknown = recipient,"),
        (2, "second = recipient,\nfirst = recipient,"),
    ] {
        let source = pool(count, assignments);
        check_frontend_error(&source, CellScriptEdition::Edition2027, Code::PoolAssignments);
        let error = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!(&source[error.span.start..error.span.end], "owner");
    }
    let empty_inputs = pool(1, "first = recipient,").replace("inputs { left, right }", "inputs { }");
    check_frontend_error(&empty_inputs, CellScriptEdition::Edition2027, Code::PoolRoles);
    for (count, assignments) in [(1, "first = recipient,"), (2, "first = recipient,\nsecond = recipient,")] {
        let source = pool(count, assignments);
        let ast = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap();
        let formatted = cellscript::fmt::format_default(&ast).unwrap();
        frontend::parse(&formatted, CellScriptEdition::Edition2027).unwrap();
    }
}

#[test]
fn unterminated_tokens_keep_precise_unicode_byte_and_character_positions() {
    for (tail, code, delimiter) in [
        ("\"hello", Code::UnterminatedString, "\""),
        ("'hello", Code::InvalidEscape, "'"),
        ("\"hello\\", Code::UnterminatedEscape, "\\"),
        ("'hello\\", Code::InvalidEscape, "'"),
        ("/* outer /* inner */", Code::UnterminatedComment, "/*"),
        ("/* outer /* inner", Code::UnterminatedComment, "/*"),
    ] {
        let source = format!("// π🙂\nmodule boundary;\n{tail}");
        let error = lexer::lex(&source).unwrap_err();
        assert_eq!(error.code.as_deref(), Some(code.code()));
        assert_eq!(&source[error.span.start..error.span.end], delimiter);
        assert_eq!(error.span.line, 3);
        assert_eq!(error.span.column, if delimiter == "\\" { 7 } else { 1 });
        {
            let edition = CellScriptEdition::Edition2027;
            check_frontend_error(&source, edition, code);
        }
    }
    let source = "/* π🙂 */ \"unterminated";
    let error = lexer::lex(source).unwrap_err();
    assert_eq!(error.span.start, source.find('"').unwrap());
    assert_eq!(error.span.column, 10);
}

#[test]
fn closed_strings_nested_comments_and_line_comments_at_eof_stay_valid() {
    for source in [
        r#""hello\n\"world""#,
        "/* outer /* inner */ closed */",
        "// at eof",
        "module boundary;\n/* outer /* inner */ closed */\n// end",
    ] {
        lexer::lex(source).unwrap_or_else(|error| panic!("{source:?}: {error}"));
    }
}

#[test]
fn double_quoted_text_and_byte_hex_errors_have_stable_codes() {
    let tokens = lexer::lex(r#""a\'b\"c""#).unwrap();
    assert_eq!(tokens[0].kind, cellscript::lexer::token::TokenKind::String("a'b\"c".into()));
    for (literal, code) in [
        (r#""\q""#, Code::InvalidEscape),
        (r#"'\q'"#, Code::InvalidEscape),
        (r#"b"\q""#, Code::InvalidEscape),
        ("b\"\\x", Code::InvalidByteHexEscape),
        ("b\"\\xA", Code::InvalidByteHexEscape),
        (r#"b"\xG0""#, Code::InvalidByteHexEscape),
        (r#"b"\x0π""#, Code::InvalidByteHexEscape),
        (r#"b"π""#, Code::NonAsciiByteString),
        ("0x", Code::InvalidIntegerLiteral),
        ("340282366920938463463374607431768211456", Code::InvalidIntegerLiteral),
    ] {
        let source = format!("// 🙂\nmodule escapes;\n{literal}");
        let error = lexer::lex(&source).unwrap_err();
        assert_eq!(error.code.as_deref(), Some(code.code()), "{literal:?}");
        assert!(source.is_char_boundary(error.span.start) && source.is_char_boundary(error.span.end));
        {
            let edition = CellScriptEdition::Edition2027;
            check_frontend_error(&source, edition, code);
        }
    }
}

#[test]
fn unterminated_byte_strings_reject_metadata_and_every_artifact_with_source_spans() {
    for prefix in [
        "// π🙂\nmodule byte_boundary;\naction inspect() {  let bytes = ",
        "module byte_boundary;\naction inspect() {  /* π🙂 */ let bytes = ",
    ] {
        for (tail, code, delimiter) in [
            ("b\"", Code::UnterminatedByteString, "b\""),
            ("b\"abc", Code::UnterminatedByteString, "b\""),
            ("b\"abc\\\"", Code::UnterminatedByteString, "b\""),
            ("b\"abc\\\\", Code::UnterminatedByteString, "b\""),
            ("b\"\\", Code::UnterminatedByteStringEscape, "\\"),
            ("b\"abc\\", Code::UnterminatedByteStringEscape, "\\"),
        ] {
            let source = format!("{prefix}{tail}");
            let error = lexer::lex(&source).unwrap_err();
            assert_eq!(error.code.as_deref(), Some(code.code()), "{source:?}: {error}");
            assert_eq!(&source[error.span.start..error.span.end], delimiter);
            let expected_start = if delimiter == "\\" { source.rfind('\\').unwrap() } else { prefix.len() };
            assert_eq!(error.span.start, expected_start);
            assert_eq!(error.span.end, expected_start + delimiter.len());
            let before = &source[..expected_start];
            assert_eq!(error.span.line, before.chars().filter(|ch| *ch == '\n').count() + 1);
            assert_eq!(error.span.column, before.rsplit('\n').next().unwrap().chars().count() + 1);
            {
                let edition = CellScriptEdition::Edition2027;
                check_frontend_error(&source, edition, code);
                let report = cellscript::compile_metadata_with_diagnostics(&source, edition, None);
                assert_eq!(report.diagnostics[0].span, error.span);
                for target in ["riscv64-asm", "riscv64-elf"] {
                    for policy in
                        [cellscript::ExecutableSurfacePolicy::AllowFailClosed, cellscript::ExecutableSurfacePolicy::DenyFailClosed]
                    {
                        let artifact_error = cellscript::compile_with_executable_surface_policy(
                            &source,
                            cellscript::CompileOptions { edition, target: Some(target.into()), ..Default::default() },
                            policy,
                        )
                        .unwrap_err();
                        assert_eq!(artifact_error.code, error.code);
                        assert_eq!(artifact_error.span, error.span);
                    }
                }
            }
        }
    }
    let valid = "module byte_boundary;\naction inspect() -> u64 {  let payload = b\"\\x2A\"; require payload[0] == 42; return 0; }";
    {
        let edition = CellScriptEdition::Edition2027;
        assert!(cellscript::compile_metadata(valid, edition, None).is_ok());
        for target in ["riscv64-asm", "riscv64-elf"] {
            cellscript::compile(valid, cellscript::CompileOptions { edition, target: Some(target.into()), ..Default::default() })
                .unwrap();
        }
    }
}

#[test]
fn scheduler_hint_unknown_names_reject_before_metadata_or_artifacts() {
    for (hints, unknown) in [
        ("parallelism", "parallelism"),
        ("parallel, estimated_cycle = 8", "estimated_cycle"),
        ("priority = estimate(1)", "priority"),
        ("priority(nested(1))", "priority"),
        ("priority = { nested: 1 }", "priority"),
        ("sequential, 未知 = 1", "未知"),
    ] {
        let source =
            format!("// π🙂\nmodule hint_boundary;\n/* π🙂 */ #[scheduler_hint({hints})]\naction inspect() {{ require true; }}");
        let expected_start = source.find(unknown).unwrap();
        {
            let edition = CellScriptEdition::Edition2027;
            check_frontend_error(&source, edition, Code::UnknownSchedulerHint);
            let error = frontend::parse(&source, edition).unwrap_err();
            assert_eq!(&source[error.span.start..error.span.end], unknown);
            assert_eq!(error.span.start, expected_start);
            assert_eq!(error.span.end, expected_start + unknown.len());
            assert_eq!(error.span.line, 3);
            assert_eq!(error.span.column, source[..expected_start].rsplit('\n').next().unwrap().chars().count() + 1);
            assert!(error.message.contains("parallel, sequential, or estimated_cycles"), "{error}");
            let report = cellscript::compile_metadata_with_diagnostics(&source, edition, None);
            assert!(report.metadata.is_none());
            assert_eq!(report.diagnostics[0].span, error.span);
            for target in ["riscv64-asm", "riscv64-elf"] {
                let artifact_error = cellscript::compile(
                    &source,
                    cellscript::CompileOptions { edition, target: Some(target.into()), ..Default::default() },
                )
                .unwrap_err();
                assert_eq!(artifact_error.code.as_deref(), Some(Code::UnknownSchedulerHint.code()));
                assert_eq!(artifact_error.span, error.span);
            }
        }
    }
}

#[test]
fn scheduler_hint_known_names_keep_literal_values_and_defaults() {
    for (hints, parallelizable, estimated_cycles) in [
        ("", true, 1000),
        ("parallel", true, 1000),
        ("sequential", false, 1000),
        ("estimated_cycles = 0", true, 0),
        ("estimated_cycles = 18446744073709551615", true, u64::MAX),
        ("sequential, estimated_cycles = 7", false, 7),
        ("parallel, estimated_cycles = 8,", true, 8),
    ] {
        let source = format!("module hint_boundary;\n#[scheduler_hint({hints})]\naction inspect() {{ require true; }}");
        {
            let edition = CellScriptEdition::Edition2027;
            let module = frontend::parse(&source, edition).unwrap();
            let cellscript::ast::Item::Action(action) = &module.items[0] else { panic!("action") };
            let hint = action.scheduler_hint.as_ref().unwrap();
            assert_eq!(hint.parallelizable, parallelizable);
            assert_eq!(hint.estimated_cycles, estimated_cycles);
            let formatted = cellscript::fmt::format_default(&module).unwrap();
            let reparsed = frontend::parse(&formatted, edition).unwrap();
            let cellscript::ast::Item::Action(action) = &reparsed.items[0] else { panic!("action") };
            let hint = action.scheduler_hint.as_ref().unwrap();
            assert_eq!(hint.parallelizable, parallelizable);
            assert_eq!(hint.estimated_cycles, estimated_cycles);
            cellscript::compile_metadata(&source, edition, None).unwrap();
        }
    }
}

#[test]
fn scheduler_hint_known_values_remain_literal_only() {
    for hints in [
        "estimated_cycles = budget",
        "estimated_cycles = estimate(1)",
        "estimated_cycles = (1)",
        "estimated_cycles = 1 + 2",
        "estimated_cycles = 18446744073709551616",
        "estimated_cycles = -1",
        "parallel(value)",
    ] {
        let source = format!("module hint_boundary;\n#[scheduler_hint({hints})]\naction inspect() {{ require true; }}");
        {
            let edition = CellScriptEdition::Edition2027;
            let error = frontend::parse(&source, edition).unwrap_err();
            assert_ne!(error.code.as_deref(), Some(Code::UnknownSchedulerHint.code()), "{hints}: {error}");
        }
    }
}

#[test]
fn explicit_group_bodies_reject_retired_condition_and_relation_spellings() {
    let source = include_str!("syntax_combo/seeds/edition-2027-script-hash.cell");
    for obsolete in [
        source.replace("input token: Token,", "entry token: Token,"),
        source.replace("require token.amount > 0", "verify { enforce token.amount > 0 }"),
        source.replace("type_script = same", "identity = same"),
        source.replace("lock = exact_hash", "lock_script = exact_hash"),
        source.replace("capacity = same;", "capacity = same;\ncardinality = one_to_one;"),
    ] {
        let error = frontend::parse(&obsolete, CellScriptEdition::Edition2027).unwrap_err();
        assert!(error.span.end > error.span.start, "{error}");
        let diagnostics = frontend::parse_diagnostics(&obsolete, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!(diagnostics[0].message, error.message);
        let formatted = cellscript::fmt::format_source(&obsolete, CellScriptEdition::Edition2027, Default::default()).unwrap_err();
        assert_eq!(formatted.message, error.message);
        let report = cellscript::compile_metadata_with_diagnostics(&obsolete, CellScriptEdition::Edition2027, None);
        assert!(report.metadata.is_none());
        assert_eq!(report.diagnostics[0].message, error.message);
    }
}

#[test]
fn removed_group_spellings_do_not_reserve_ordinary_helper_names() {
    let source = "module contextual_names;\nfn enforce(value: u64) -> bool { return value > 0; }\nfn verify(value: u64) -> bool { return enforce(value); }\naction entry(witness value: u64) -> bool { return verify(value); }";
    let ast = frontend::parse(source, CellScriptEdition::Edition2027).unwrap();
    let formatted = cellscript::fmt::format_default(&ast).unwrap();
    assert!(formatted.contains("fn enforce("));
    assert!(formatted.contains("action entry("));
    cellscript::compile_metadata(&formatted, CellScriptEdition::Edition2027, None).unwrap();
}
