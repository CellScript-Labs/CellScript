//! Regression boundaries found by comparing the 0.25 and 0.30 source surfaces.
use cellscript::{compile, compile_metadata, CellScriptEdition, CompileOptions, NEXT_EDITION};

#[test]
fn legacy_public_generics_keep_specialization_checks_without_new_declaration_bounds() {
    for declaration in
        ["struct Box<T: copy> has copy { value: T }", "struct Box<T> { value: T }", "public struct Box<T: copy> has copy { value: T }"]
    {
        let source = format!("module legacy\n{declaration}\naction main() -> u64 {{ verification let value: Box<u64> = Box<u64> {{ value: 7 }} return value.value }}");
        let result = compile(&source, CompileOptions { target: Some("riscv64-elf".to_string()), ..Default::default() }).unwrap();
        result.validate().unwrap();
        compile_metadata(&source, CellScriptEdition::Edition2026, None).unwrap();
        let error = compile(&source, CompileOptions { edition: NEXT_EDITION, ..Default::default() }).unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2110"));
        assert!(error.message.contains("fixed value layout boundary"), "{error}");
    }
    let source = "module legacy\nenum Choice<T: copy> has copy { Some(T), None }\naction main() -> u64 { verification return 0 }";
    compile_metadata(source, CellScriptEdition::Edition2026, None).unwrap();
    assert!(compile_metadata(source, NEXT_EDITION, None).unwrap_err().message.contains("fixed value layout boundary"));
}

#[test]
fn formatter_preserves_constraint_spelling_and_explicit_abilities() {
    for constraints in ["copy + drop + store + fixed + serializable + non_linear", "fixed_value"] {
        let source = format!("module format_compat\nstruct Box<T: {constraints}> has copy, drop, store, fixed, serializable, non_linear {{ value: T }}\nenum Choice<T: {constraints}> has copy, drop, store, fixed, serializable, non_linear {{ Some(T), None }}");
        let ast = cellscript::frontend::parse(&source, CellScriptEdition::Edition2026).unwrap();
        let formatted = cellscript::fmt::format_default(&ast).unwrap();
        assert!(formatted.contains(&format!("Box<T: {constraints}> has")), "{formatted}");
        assert!(formatted.contains(&format!("Choice<T: {constraints}> has")), "{formatted}");
        let reparsed = cellscript::frontend::parse(&formatted, CellScriptEdition::Edition2026).unwrap();
        assert_eq!(formatted, cellscript::fmt::format_default(&reparsed).unwrap());
        let before = compile_metadata(&source, CellScriptEdition::Edition2026, None).unwrap();
        let after = compile_metadata(&formatted, CellScriptEdition::Edition2026, None).unwrap();
        assert_eq!(before.public_interface, after.public_interface);
    }
}

#[test]
fn native_exact_hash_accepts_script_hash_and_keeps_legacy_preview_inputs() {
    for seed in [
        include_str!("syntax_combo/seeds/edition-2027-type-script.cell"),
        include_str!("syntax_combo/seeds/edition-2027-pool.cell"),
        include_str!("syntax_combo/seeds/edition-2027-retire-fresh.cell"),
    ] {
        for domain in ["Address", "Hash", "ScriptHash"] {
            let source = seed
                .replace("recipient: Address", &format!("recipient: {domain}"))
                .replace("owner: Address", &format!("owner: {domain}"));
            let result = compile(
                &source,
                CompileOptions { edition: NEXT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("{domain}: {error}"));
            result.validate().unwrap();
            let formatted = cellscript::fmt::format_default(&cellscript::frontend::parse(&source, NEXT_EDITION).unwrap()).unwrap();
            compile_metadata(&formatted, NEXT_EDITION, None).unwrap();
        }
        let invalid = seed.replace("recipient: Address", "recipient: u64");
        assert!(compile_metadata(&invalid, NEXT_EDITION, None).is_err());
    }
}

#[test]
fn parser_rejects_pathological_flat_binary_chains() {
    // A flat left-associative chain builds one AST nesting level per
    // operator without consuming the shared expression recursion budget, so
    // it needs its own admission bound.
    for term in ["1", "(1)", "((1))", "identity(1)", "[1][0]"] {
        let chain = format!("module bounds\naction verify() -> u64 {{ verification return {} }}", vec![term; 600].join(" + "));
        let error = cellscript::frontend::parse(&chain, CellScriptEdition::Edition2026).unwrap_err();
        assert!(error.message.contains("binary operator chain exceeds the parser limit"), "{term}: {error}");
    }

    let sane = format!("module bounds\naction verify() -> u64 {{ verification return {} }}", vec!["1"; 32].join(" + "));
    cellscript::frontend::parse(&sane, CellScriptEdition::Edition2026).unwrap();
    // Separate statements receive fresh budgets, even after nested expressions.
    let statements = (0..20).map(|index| format!("let value{index} = {}\n", vec!["(1)"; 32].join(" + "))).collect::<String>();
    let independent = format!("module bounds\naction verify() -> u64 {{ verification {statements} return 0 }}");
    cellscript::frontend::parse(&independent, CellScriptEdition::Edition2026).unwrap();
}

#[test]
fn parser_rejects_deeply_nested_binding_and_match_patterns() {
    let depth = 96;
    let opens = "(".repeat(depth);
    let closes = ")".repeat(depth);
    let nested_let = format!("module bounds\naction verify() -> u64 {{ verification let {opens}_{closes} = ignored return 0 }}");
    let error = cellscript::frontend::parse(&nested_let, CellScriptEdition::Edition2026).unwrap_err();
    assert!(error.message.contains("pattern nesting exceeds the parser limit"), "{error}");

    let nested_match = format!(
        "module bounds\naction verify(value: u64) -> u64 {{ verification return match value {{ {opens}_{closes} => {{ 0 }} }} }}"
    );
    let error = cellscript::frontend::parse(&nested_match, CellScriptEdition::Edition2026).unwrap_err();
    assert!(error.message.contains("pattern nesting exceeds the parser limit"), "{error}");
}

#[test]
fn parser_rejects_parameter_lists_beyond_the_admission_budget() {
    // Wide parameter lists drive the backend's pairwise scalar-slot
    // interference graph quadratically; admission stays bounded at parse.
    let params = (0..1200).map(|index| format!("value_{index}: u64")).collect::<Vec<_>>().join(", ");
    let source = format!("module bounds\nfn wide({params}) -> u64 {{ return 0 }}");
    let error = cellscript::frontend::parse(&source, CellScriptEdition::Edition2026).unwrap_err();
    assert!(error.message.contains("parameter lists exceed the parser limit"), "{error}");
}
