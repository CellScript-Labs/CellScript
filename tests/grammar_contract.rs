//! Issue #106: source rewriting must preserve checked semantics and failures.
use cellscript::{compile, compile_metadata, fmt, CompileOptions, CURRENT_EDITION};

#[allow(dead_code)]
#[path = "support/ckb_script_runner.rs"]
mod ckb_script_runner;
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};

fn formatted(source: &str) -> String {
    let result = fmt::format_source(source, CURRENT_EDITION, Default::default()).unwrap();
    assert_eq!(result, fmt::format_source(&result, CURRENT_EDITION, Default::default()).unwrap());
    result
}

fn execute(source: &str) -> i64 {
    let result = compile(source, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
    execute_result(&result)
}

fn execute_result(result: &cellscript::CompileResult) -> i64 {
    result.validate().unwrap();
    let payload = result.metadata.actions[0].entry_witness_args(&[]).unwrap();
    let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
    fixture.current_type_script_input_indices = vec![0];
    fixture.witnesses = vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
    ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&result.artifact_bytes), &fixture).exit_code
}

#[test]
fn formatting_preserves_checked_negation_and_its_success_control_in_vm() {
    for (operand, expected) in [
        ("1 as u128", 49),
        ("0 as u128", 0),
        ("(18446744073709551617 - 18446744073709551616)", 49),
        ("-(1 as u128)", 49),
        ("-(0 as u128)", 0),
        ("input", 49),
        ("input - input", 0),
    ] {
        let source = format!(
            "module checked_negation;\naction main() -> u64 {{ let input: u128 = 1; let _: u128 = -({operand});\nreturn 0; }}"
        );
        let output = formatted(&source);
        assert_eq!(execute(&source), expected, "original {source}");
        assert_eq!(execute(&output), expected, "formatted {output}");
        let ast = cellscript::frontend::parse(&source, CURRENT_EDITION).unwrap();
        let display = fmt::format_default(&ast).unwrap();
        assert_eq!(execute(&display), expected, "AST rendering {display}");
    }
}

#[test]
fn formatting_preserves_reference_receivers_and_types() {
    let source =
        "module receivers;\naction main() -> u64 { let pair = (7, 8);\nlet first = (&pair).0;\nrequire first == 7;\nreturn 0; }";
    let output = formatted(source);
    assert!(output.contains("(&pair).0"), "{output}");
    compile_metadata(source, CURRENT_EDITION, None).unwrap();
    compile_metadata(&output, CURRENT_EDITION, None).unwrap();
    assert_eq!(execute(source), 0);
    assert_eq!(execute(&output), 0);
    let display = fmt::format_default(&cellscript::frontend::parse(source, CURRENT_EDITION).unwrap()).unwrap();
    assert_eq!(execute(&display), 0);
}

#[test]
fn formatting_preserves_singleton_tuples_and_cast_receivers() {
    for source in [
        "module tuple;\naction main() -> u64 { let pair = (7,);\nreturn pair.0; }",
        "module grouped_cast;\naction main() -> u64 { let value = -(1 as u128);\nreturn 0; }",
    ] {
        let output = formatted(source);
        compile_metadata(&output, CURRENT_EDITION, None).unwrap();
    }
}

#[test]
fn text_literal_printer_uses_the_cellscript_escape_contract() {
    let source = r#"module strings; action main() -> u64 { require true, "bell:\u{7} nul:\0 quote:\""; return 0; }"#;
    let output = formatted(source);
    assert!(output.contains("\\u{7}"), "{output}");
    compile_metadata(&output, CURRENT_EDITION, None).unwrap();
    for escape in ["\\u{}", "\\u{d800}", "\\u{110000}", "\\u{0000000}", "\\u{7", "\\u7"] {
        let invalid = format!("module bad;\naction main() {{ require true, \"{escape}\"; }}");
        assert!(cellscript::frontend::parse(&invalid, CURRENT_EDITION).is_err(), "{escape}");
    }
}

#[test]
fn source_formatting_retains_comments_and_the_complete_token_partition() {
    let source = "// audit: phase=accept\nmodule comments;\n/* outer /* nested */ comment */\naction main() -> u64 {\n  let pair /* receiver */ = (7, 8); // end of line\n  require pair.0 == 7;\n  return 0;\n}\n// footer\n";
    let document = cellscript::frontend::parse_document(source, CURRENT_EDITION).unwrap();
    let restored = document.syntax.pieces().iter().map(|piece| &source[piece.span.start..piece.span.end]).collect::<String>();
    assert_eq!(restored, source);
    let result = formatted(source);
    for text in ["// audit: phase=accept", "/* outer /* nested */ comment */", "/* receiver */", "// end of line", "// footer"] {
        assert!(result.contains(text), "lost {text}: {result}");
    }
    assert_eq!(execute(source), execute(&result));
}

#[test]
fn scalar_cell_field_borrows_and_reborrows_reach_checked_elf_and_vm() {
    for body in [
        "borrow token.amount as view { require *view > 0; }",
        "borrow token.amount as view { borrow view as again { require *again > 0; } }",
    ] {
        let source = format!("module field_borrow;\nresource Token has consume, burn {{ amount: u64 }}\naction inspect(token: Token) -> u64 {{ {body}\ndestroy token;\nreturn 0; }}");
        for program in [source.clone(), formatted(&source)] {
            let compiled = compile(&program, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
            compiled.validate().unwrap();
            let payload = compiled.metadata.actions[0].entry_witness_args(&[]).unwrap();
            for (amount, expected) in [(1u64, 0), (0, 5)] {
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 0);
                fixture.current_type_script_input_indices = vec![0];
                fixture.inputs[0].data = Bytes::from(amount.to_le_bytes().to_vec());
                fixture.witnesses =
                    vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload.clone())).pack()).build().as_bytes()];
                let result =
                    ckb_script_runner::execute_cellscript_script(cellscript::strip_vm_abi_trailer(&compiled.artifact_bytes), &fixture);
                assert_eq!(result.exit_code, expected, "{program}\namount {amount}: {result:?}");
            }
        }
    }
}

#[test]
fn syntax_ranges_come_from_the_shared_parser_including_native_bodies() {
    use cellscript::syntax::SyntaxKind;
    let source = "module ranges;\nstruct Pair { value: u64, }\nfn id<T: fixed_value>(x: T) -> T { x }\naction main() -> u64 { let p = new Pair { value: id::<u64>(7), };\nfor i in 0..1 { require i == 0; }\nreturn p.value; }";
    let document = cellscript::frontend::parse_document(source, CURRENT_EDITION).unwrap();
    for (kind, expected) in [
        (SyntaxKind::Constructor, "new Pair { value: id::<u64>(7), }"),
        (SyntaxKind::GenericArguments, "<u64>"),
        (SyntaxKind::LoopBody, "{ require i == 0; }"),
    ] {
        let node = document.syntax.nodes.iter().find(|node| node.kind == kind).unwrap();
        assert_eq!(&source[node.span.start..node.span.end], expected);
    }
    let source = include_str!("syntax_combo/seeds/edition-2027-type-script.cell");
    let native = cellscript::frontend::parse_document(source, CURRENT_EDITION).unwrap();
    assert!(native.syntax.nodes.iter().any(|node| matches!(node.kind, SyntaxKind::Statement { .. })));
    assert!(native.syntax.nodes.iter().any(|node| node.kind == SyntaxKind::ModuleHeader));
    for node in &native.syntax.nodes {
        assert!(source.get(node.span.start..node.span.end).is_some(), "{node:?}");
    }
}

#[test]
fn explicit_discard_is_not_an_implicit_return() {
    use cellscript::ast::{Item, Stmt};
    for (body, tail) in [("7", true), ("7;", false)] {
        let source = format!("module tail; fn value() -> u64 {{ {body} }} action main() -> u64 {{ value() }}");
        let parsed = cellscript::frontend::parse(&source, CURRENT_EDITION).unwrap();
        let Item::Function(function) = &parsed.items[0] else { panic!("function") };
        assert_eq!(matches!(function.body[0], Stmt::Expr(_)), tail);
        assert_eq!(matches!(function.body[0], Stmt::Discard(_)), !tail);
        assert_eq!(compile_metadata(&source, CURRENT_EDITION, None).is_ok(), tail, "{source}");
        let output = formatted(&source);
        assert_eq!(compile_metadata(&output, CURRENT_EDITION, None).is_ok(), tail);
        if tail {
            assert_eq!(execute(&source), 7);
            assert_eq!(execute(&output), 7);
        }
    }
    let source = "module return_line; fn value() -> u64 { return\n7; } action main() -> u64 { value() }";
    assert_eq!(execute(source), 7);
    assert_eq!(execute(&formatted(source)), 7);
}

#[test]
fn explicit_construction_and_binding_powers_execute() {
    let source = r#"module syntax;
private struct pair_value<T: fixed_value> { left: T, right: T, }
private fn id<T: fixed_value>(x: T) -> T { x }
action main() -> u64 {
    let Flag = true;
    let pair = new pair_value<u64> { left: id::<u64>(7), right: 8, };
    require { pair.left == 7, pair.right == 8, };
    require 3 & 1 == 1;
    if Flag { return 0; }
    else { return 1; }
}"#;
    assert_eq!(execute(source), 0);
    assert_eq!(execute(&formatted(source)), 0);
    let display = fmt::format_default(&cellscript::frontend::parse(source, CURRENT_EDITION).unwrap()).unwrap();
    assert_eq!(execute(&display), 0, "{display}");
}

#[test]
fn retired_and_ambiguous_syntax_is_rejected_at_the_source_boundary() {
    for source in [
        "module old\naction main() { return; }",
        "module old; action main() { verification return; }",
        "module old; action main() { let x = 1 }",
        "module old; struct Pair { x: u64 } action main() { let p = Pair { x: 1 }; }",
        "module old; fn id<T>(x: T) -> T { x } action main() { id<u64>(1); }",
        "module old; action main() { require { true\nfalse }; }",
        "module old; struct Pair { x: u64\ny: u64 }",
        "module old; action main() { require 1 < 2 < 3; }",
        "module old; action main() { require 1 < 2 == true; }",
        "module old; action main() { require true == 1 < 2; }",
        "module old; action main() { let x = 1 < < 2; }",
        "module old; #[capability(store)] resource Token { value: u64, }",
        "module old; #[scheduler_hint(parallel)] struct Pair { value: u64, }",
        "module old; private #[type_id(\"pair\")] struct Pair { value: u64, }",
        "module old; #[scheduler_hint(parallel, sequential)] action main() { return; }",
    ] {
        assert!(cellscript::frontend::parse(source, CURRENT_EDITION).is_err(), "accepted {source}");
    }
    compile_metadata("module grouped; action main() -> u64 { require (1 < 2) == true; return 0; }", CURRENT_EDITION, None).unwrap();
}

#[test]
fn type_grouping_and_singleton_source_rendering_keep_separate_domains() {
    use cellscript::ast::{Item, Type};
    let source = "module tuples; private fn f(a: (u64), b: (u64,), c: ()) {} action main() { return; }";
    let parsed = cellscript::frontend::parse(source, CURRENT_EDITION).unwrap();
    let Item::Function(function) = &parsed.items[0] else { panic!("function") };
    assert!(matches!(function.params[0].ty, Type::U64));
    assert!(matches!(&function.params[1].ty, Type::Tuple(items) if items.len() == 1));
    assert!(matches!(function.params[2].ty, Type::Unit));
    let singleton = Type::Tuple(vec![Type::U64]);
    assert_eq!(singleton.source_syntax(), "(u64,)");
    assert_eq!(singleton.source_text(), "(u64)", "established packed-domain spelling must remain stable");
    let rendered = fmt::format_default(&parsed).unwrap();
    assert!(rendered.contains("b: (u64,)"), "{rendered}");
    cellscript::frontend::parse(&rendered, CURRENT_EDITION).unwrap();
}

#[test]
fn qualified_and_aliased_generic_constructors_reach_checked_package_elf() {
    let package = tempfile::tempdir().unwrap();
    let directory = package.path();
    std::fs::create_dir(directory.join("src")).unwrap();
    std::fs::write(directory.join("Cell.toml"), "[package]\nname = \"constructor_aliases\"\nversion = \"0.1.0\"\nedition = \"2027\"\ncellscript_version = \">=0.33.0-alpha.2\"\nentry = \"src/main.cell\"\n").unwrap();
    let library = "module shapes; public struct lower_pair<T: fixed_value> { left: T, right: T, } public fn identity<T: fixed_value>(value: T) -> T { value }";
    let source = "module constructors; use shapes::{lower_pair as Alias, identity as ident}; action main() -> u64 { let a = new Alias<u64> { left: 7, right: 8, }; let b = new shapes::lower_pair<u64> { left: 7, right: 9, }; require shapes::identity::<u64>(a.left) == ident::<u64>(b.left); return 0; }";
    for (main, shapes) in [(source.to_owned(), library.to_owned()), (formatted(source), formatted(library))] {
        std::fs::write(directory.join("src/main.cell"), main).unwrap();
        std::fs::write(directory.join("src/shapes.cell"), shapes).unwrap();
        let lock =
            std::process::Command::new(env!("CARGO_BIN_EXE_cellc")).current_dir(directory).args(["lock", "--json"]).output().unwrap();
        assert!(lock.status.success(), "{}", String::from_utf8_lossy(&lock.stdout));
        let path = camino::Utf8Path::from_path(directory).unwrap();
        let result =
            cellscript::compile_path(path, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
        assert_eq!(execute_result(&result), 0);
    }
}

#[test]
fn representative_sources_reach_metadata_assembly_and_checked_elf_before_and_after_formatting() {
    for (name, source) in [
        ("native_group", include_str!("syntax_combo/seeds/edition-2027-type-script.cell")),
        ("patterns", include_str!("syntax_combo/seeds/complete-patterns.cell")),
        ("borrow", include_str!("../examples/language/ownership/borrow.cell")),
        ("lifecycle", include_str!("../examples/language/core/canonical_style.cell")),
    ] {
        for program in [source.to_owned(), formatted(source)] {
            compile_metadata(&program, CURRENT_EDITION, None).unwrap_or_else(|error| panic!("{name} metadata: {error}"));
            for target in ["riscv64", "riscv64-elf"] {
                let result = compile(&program, CompileOptions { target: Some(target.into()), ..Default::default() })
                    .unwrap_or_else(|error| panic!("{name} {target}: {error}"));
                if target == "riscv64-elf" {
                    result.validate().unwrap_or_else(|error| panic!("{name} checked ELF: {error}"));
                } else {
                    assert!(!result.artifact_bytes.is_empty(), "{name} assembly");
                }
            }
        }
    }
    let source = include_str!("../examples/language/collections/bounded_lifecycle.cell");
    for program in [source.to_owned(), formatted(source)] {
        compile_metadata(&program, CURRENT_EDITION, None).unwrap();
        assert!(
            cellscript::compile_with_executable_surface_policy(
                &program,
                CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() },
                cellscript::ExecutableSurfacePolicy::DenyFailClosed,
            )
            .is_err(),
            "frontend-only lifecycle iteration must retain its executable rejection"
        );
    }
}

#[test]
fn source_tooling_keeps_the_existing_input_and_layout_limits() {
    let source = format!("// {}\nmodule bounded; action main() -> u64 {{ return 0; }}", "x".repeat(cellscript::MAX_SOURCE_BYTES - 64));
    assert!(source.len() <= cellscript::MAX_SOURCE_BYTES);
    let output = formatted(&source);
    assert!(output.starts_with(&source[..source.find('\n').unwrap()]));
    let oversized = " ".repeat(cellscript::MAX_SOURCE_BYTES + 1);
    assert!(cellscript::frontend::parse_document(&oversized, CURRENT_EDITION).is_err());
    assert!(fmt::format_source(&oversized, CURRENT_EDITION, Default::default()).is_err());
    let source = "module bounded; action main() { return; }";
    for config in [
        fmt::FormatConfig { indent_width: 17, ..Default::default() },
        fmt::FormatConfig { max_line_width: 4097, ..Default::default() },
    ] {
        assert!(fmt::format_source(source, CURRENT_EDITION, config).is_err());
    }
}
