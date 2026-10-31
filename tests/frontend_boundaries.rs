use cellscript::{error::SourceDiagnosticCode as Code, frontend, lexer, CellScriptEdition};

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
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            let source = format!("module fixed_layout\naction inspect(witness payload: {ty}) {{ verification }}");
            let metadata = cellscript::compile_metadata(&source, edition, None).unwrap_err();
            let artifact = cellscript::compile(&source, cellscript::CompileOptions { edition, ..Default::default() }).unwrap_err();
            for error in [&metadata, &artifact] {
                assert_eq!(error.code.as_deref(), Some(Code::FixedLayoutBounds.code()), "{error}");
                assert!(error.span.start > 0 && error.span.end > error.span.start, "{error}");
            }
        }
    }
    let valid = "module fixed_layout\naction inspect(witness payload: [u8; 32]) { verification require payload[0] == 7 }";
    assert!(cellscript::compile(valid, Default::default()).is_ok());
}

#[test]
#[cfg(target_pointer_width = "64")]
fn oversized_fixed_aliases_do_not_reach_typed_parameter_alignment() {
    let source = format!(
        "module fixed_alias\naction inspect(witness payload: [u8; {}]) {{ verification let alias = payload require alias[0] == 7 }}",
        i64::MAX
    );
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
        let report = cellscript::compile_metadata_with_diagnostics(&source, edition, None);
        assert!(report.metadata.is_none());
        assert_eq!(report.diagnostics[0].code.as_deref(), Some(Code::FixedLayoutBounds.code()));
        let error = cellscript::compile(&source, cellscript::CompileOptions { edition, ..Default::default() }).unwrap_err();
        assert_eq!(error.code.as_deref(), Some(Code::FixedLayoutBounds.code()));
        assert!(error.span.start > 0 && error.span.end > error.span.start);
    }
}

#[test]
fn reserved_specialization_names_are_diagnosed_before_internal_decoding() {
    for name in ["value__mono__汉字", "value__mono__éé", "value__mono__u64", "value__mono__0g"] {
        let source = format!(
            "module unicode_names\nfn {name}() -> u64 {{ return 7 }}\naction main() -> u64 {{ verification return {name}() }}"
        );
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            let error = cellscript::compile_metadata(&source, edition, None).unwrap_err();
            assert_eq!(error.code.as_deref(), Some("E2110"));
            let error = cellscript::compile(&source, cellscript::CompileOptions { edition, ..Default::default() }).unwrap_err();
            assert_eq!(error.code.as_deref(), Some("E2110"));
        }
    }
}

fn pool(outputs: usize, assignments: &str) -> String {
    let roles = ["first", "second"][..outputs].join(", ");
    let declarations = (0..outputs)
        .map(|index| format!("output {}: Token from group_output[{index}],", ["first", "second"][index]))
        .collect::<Vec<_>>()
        .join("\n");
    let locks =
        ["first", "second"][..outputs].iter().map(|name| format!("{name} = exact_hash(recipient)")).collect::<Vec<_>>().join("\n");
    format!(
        r#"// π
module pool_boundary
resource Token has store, create, consume {{ owner: Address, amount: u64 }}
type_script TokenPool on type_group<Token> {{
    entry merge(
        input left: Token from group_input[0],
        input right: Token from group_input[1],
        witness recipient: Address from group_witness.input_type,
        {declarations}
    ) {{
        verify {{ enforce left.amount > 0 }}
        pool value_flow {{
            inputs {{ left, right }}
            outputs {{ {roles} }}
            data {{
                owner {{ {assignments} }}
                amount = conserve
            }}
            identity = pooled
            type_script = same
            lock_script {{ {locks} }}
            capacity = builder_computed
            cardinality = declared
        }}
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
        assert!(server.get_diagnostics("file:///boundary.cell").iter().any(|diagnostic| diagnostic.code == error.code));
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
        (2, "first = recipient"),
        (2, "second = recipient"),
        (2, "first = recipient\nfirst = recipient"),
        (2, "first = recipient\nunknown = recipient"),
        (2, "second = recipient\nfirst = recipient"),
    ] {
        let source = pool(count, assignments);
        check_frontend_error(&source, CellScriptEdition::Edition2027, Code::PoolAssignments);
        let error = frontend::parse(&source, CellScriptEdition::Edition2027).unwrap_err();
        assert_eq!(&source[error.span.start..error.span.end], "owner");
    }
    let empty_inputs = pool(1, "first = recipient").replace("inputs { left, right }", "inputs { }");
    check_frontend_error(&empty_inputs, CellScriptEdition::Edition2027, Code::PoolRoles);
    for (count, assignments) in [(1, "first = recipient"), (2, "first = recipient\nsecond = recipient")] {
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
        ("'hello", Code::UnterminatedString, "'"),
        ("\"hello\\", Code::UnterminatedEscape, "\\"),
        ("'hello\\", Code::UnterminatedEscape, "\\"),
        ("/* outer /* inner */", Code::UnterminatedComment, "/*"),
        ("/* outer /* inner", Code::UnterminatedComment, "/*"),
    ] {
        let source = format!("// π🙂\nmodule boundary\n{tail}");
        let error = lexer::lex(&source).unwrap_err();
        assert_eq!(error.code.as_deref(), Some(code.code()));
        assert_eq!(&source[error.span.start..error.span.end], delimiter);
        assert_eq!(error.span.line, 3);
        assert_eq!(error.span.column, if delimiter == "\\" { 7 } else { 1 });
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
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
        "'hello\\n\\'world'",
        "/* outer /* inner */ closed */",
        "// at eof",
        "module boundary\n/* outer /* inner */ closed */\n// end",
    ] {
        lexer::lex(source).unwrap_or_else(|error| panic!("{source:?}: {error}"));
    }
}

#[test]
fn unterminated_byte_strings_reject_metadata_and_every_artifact_with_source_spans() {
    for prefix in [
        "// π🙂\nmodule byte_boundary\naction inspect() { verification let bytes = ",
        "module byte_boundary\naction inspect() { verification /* π🙂 */ let bytes = ",
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
            for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
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
    let valid =
        "module byte_boundary\naction inspect() -> u64 { verification let payload = b\"\\x2A\" require payload[0] == 42 return 0 }";
    for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
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
        let source = format!(
            "// π🙂\nmodule hint_boundary\n/* π🙂 */ #[scheduler_hint({hints})]\naction inspect() {{ verification require true }}"
        );
        let expected_start = source.find(unknown).unwrap();
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
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
fn scheduler_hint_known_names_keep_literal_values_defaults_and_last_mode() {
    for (hints, parallelizable, estimated_cycles) in [
        ("", true, 1000),
        ("parallel", true, 1000),
        ("sequential", false, 1000),
        ("estimated_cycles = 0", true, 0),
        ("estimated_cycles = 18446744073709551615", true, u64::MAX),
        ("parallel, sequential, estimated_cycles = 7", false, 7),
        ("sequential, parallel, estimated_cycles = 8,", true, 8),
    ] {
        let source = format!("module hint_boundary\n#[scheduler_hint({hints})]\naction inspect() {{ verification require true }}");
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
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
        let source = format!("module hint_boundary\n#[scheduler_hint({hints})]\naction inspect() {{ verification require true }}");
        for edition in [CellScriptEdition::Edition2026, CellScriptEdition::Edition2027] {
            let error = frontend::parse(&source, edition).unwrap_err();
            assert_ne!(error.code.as_deref(), Some(Code::UnknownSchedulerHint.code()), "{hints}: {error}");
        }
    }
}
