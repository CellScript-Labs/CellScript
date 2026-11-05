//! Regression boundaries found by comparing the 0.25 and 0.30 source surfaces.
use cellscript::{compile, compile_metadata, CellScriptEdition, CompileOptions, CURRENT_EDITION};

#[test]
fn public_generics_require_explicit_fixed_layout_bounds() {
    for declaration in
        ["struct Box<T: copy> has copy { value: T }", "struct Box<T> { value: T }", "public struct Box<T: copy> has copy { value: T }"]
    {
        let source = format!("module public_layout\n{declaration}\naction main() -> u64 {{ verification let value: Box<u64> = Box<u64> {{ value: 7 }} return value.value }}");
        let error = compile(&source, CompileOptions::default()).unwrap_err();
        assert_eq!(error.code.as_deref(), Some("E2110"));
        assert!(error.message.contains("fixed value layout boundary"), "{error}");
        assert!(compile_metadata(&source, CURRENT_EDITION, None).unwrap_err().message.contains("fixed value layout boundary"));
        let bounded = source.replace("<T: copy>", "<T: fixed_value>").replace("<T>", "<T: fixed_value>");
        let result = compile(&bounded, CompileOptions { target: Some("riscv64-elf".into()), ..Default::default() }).unwrap();
        result.validate().unwrap();
    }
    let source =
        "module public_layout\nenum Choice<T: copy> has copy { Some(T), None }\naction main() -> u64 { verification return 0 }";
    assert!(compile_metadata(source, CURRENT_EDITION, None).unwrap_err().message.contains("fixed value layout boundary"));
    let private = source.replace("enum Choice", "private enum Choice");
    compile_metadata(&private, CURRENT_EDITION, None).unwrap();
}

#[test]
fn formatter_preserves_constraint_spelling_and_explicit_abilities() {
    for constraints in ["copy + drop + store + fixed + serializable + non_linear", "fixed_value"] {
        let source = format!("module format_compat\nstruct Box<T: {constraints}> has copy, drop, store, fixed, serializable, non_linear {{ value: T }}\nenum Choice<T: {constraints}> has copy, drop, store, fixed, serializable, non_linear {{ Some(T), None }}");
        let ast = cellscript::frontend::parse(&source, CellScriptEdition::Edition2027).unwrap();
        let formatted = cellscript::fmt::format_default(&ast).unwrap();
        assert!(formatted.contains(&format!("Box<T: {constraints}> has")), "{formatted}");
        assert!(formatted.contains(&format!("Choice<T: {constraints}> has")), "{formatted}");
        let reparsed = cellscript::frontend::parse(&formatted, CellScriptEdition::Edition2027).unwrap();
        assert_eq!(formatted, cellscript::fmt::format_default(&reparsed).unwrap());
        let before = compile_metadata(&source, CellScriptEdition::Edition2027, None).unwrap();
        let after = compile_metadata(&formatted, CellScriptEdition::Edition2027, None).unwrap();
        assert_eq!(before.public_interface, after.public_interface);
    }
}

#[test]
fn native_lock_targets_preserve_address_and_script_hash_domains() {
    for seed in [
        include_str!("syntax_combo/seeds/edition-2027-type-script.cell"),
        include_str!("syntax_combo/seeds/edition-2027-pool.cell"),
        include_str!("syntax_combo/seeds/edition-2027-retire-fresh.cell"),
    ] {
        let source_for = |domain: &str, treatment: &str| {
            seed.replace("recipient: Address", &format!("recipient: {domain}"))
                .replace("owner: Address", &format!("owner: {domain}"))
                .replace("exact(recipient)", treatment)
        };
        for (domain, treatment) in [
            ("Address", "exact(recipient)"),
            ("ScriptHash", "exact_hash(recipient)"),
            ("Hash", "exact_hash(ckb::script_hash(recipient))"),
        ] {
            let source = source_for(domain, treatment);
            let result = compile(
                &source,
                CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() },
            )
            .unwrap_or_else(|error| panic!("{domain}: {error}"));
            result.validate().unwrap();
            let formatted = cellscript::fmt::format_default(&cellscript::frontend::parse(&source, CURRENT_EDITION).unwrap()).unwrap();
            assert!(formatted.contains(treatment), "{domain}: {formatted}");
            compile_metadata(&formatted, CURRENT_EDITION, None).unwrap();
        }
        for (domain, treatment, expected_domain) in [
            ("Address", "exact_hash(recipient)", "ScriptHash"),
            ("Hash", "exact_hash(recipient)", "ScriptHash"),
            ("ScriptHash", "exact(recipient)", "Address"),
            ("Hash", "exact(recipient)", "Address"),
            ("u64", "exact(recipient)", "Address"),
        ] {
            let invalid = source_for(domain, treatment);
            let metadata_error = compile_metadata(&invalid, CURRENT_EDITION, None).unwrap_err();
            let native_error = compile(
                &invalid,
                CompileOptions { edition: CURRENT_EDITION, target: Some("riscv64-elf".to_string()), ..Default::default() },
            )
            .unwrap_err();
            for error in [metadata_error, native_error] {
                assert!(
                    error.message.contains("lock")
                        && error.message.contains("expects")
                        && error.message.contains(expected_domain)
                        && error.message.contains(&format!("found {domain}")),
                    "{domain}/{treatment}: {error}"
                );
            }
        }
    }
}

#[test]
fn parser_rejects_pathological_flat_binary_chains() {
    // A flat left-associative chain builds one AST nesting level per
    // operator without consuming the shared expression recursion budget, so
    // it needs its own admission bound.
    for term in ["1", "(1)", "((1))", "identity(1)", "[1][0]"] {
        let chain = format!("module bounds\naction verify() -> u64 {{ verification return {} }}", vec![term; 600].join(" + "));
        let error = cellscript::frontend::parse(&chain, CellScriptEdition::Edition2027).unwrap_err();
        assert!(error.message.contains("binary operator chain exceeds the parser limit"), "{term}: {error}");
    }

    let sane = format!("module bounds\naction verify() -> u64 {{ verification return {} }}", vec!["1"; 32].join(" + "));
    cellscript::frontend::parse(&sane, CellScriptEdition::Edition2027).unwrap();
    // Separate statements receive fresh budgets, even after nested expressions.
    let statements = (0..20).map(|index| format!("let value{index} = {}\n", vec!["(1)"; 32].join(" + "))).collect::<String>();
    let independent = format!("module bounds\naction verify() -> u64 {{ verification {statements} return 0 }}");
    cellscript::frontend::parse(&independent, CellScriptEdition::Edition2027).unwrap();
}

#[test]
fn parser_rejects_deeply_nested_binding_and_match_patterns() {
    let depth = 96;
    let opens = "(".repeat(depth);
    let closes = ")".repeat(depth);
    let nested_let = format!("module bounds\naction verify() -> u64 {{ verification let {opens}_{closes} = ignored return 0 }}");
    let error = cellscript::frontend::parse(&nested_let, CellScriptEdition::Edition2027).unwrap_err();
    assert!(error.message.contains("pattern nesting exceeds the parser limit"), "{error}");

    let nested_match = format!(
        "module bounds\naction verify(value: u64) -> u64 {{ verification return match value {{ {opens}_{closes} => {{ 0 }} }} }}"
    );
    let error = cellscript::frontend::parse(&nested_match, CellScriptEdition::Edition2027).unwrap_err();
    assert!(error.message.contains("pattern nesting exceeds the parser limit"), "{error}");
}

#[test]
fn parser_rejects_parameter_lists_beyond_the_admission_budget() {
    // Wide parameter lists drive the backend's pairwise scalar-slot
    // interference graph quadratically; admission stays bounded at parse.
    let params = (0..1200).map(|index| format!("value_{index}: u64")).collect::<Vec<_>>().join(", ");
    let source = format!("module bounds\nfn wide({params}) -> u64 {{ return 0 }}");
    let error = cellscript::frontend::parse(&source, CellScriptEdition::Edition2027).unwrap_err();
    assert!(error.message.contains("parameter lists exceed the parser limit"), "{error}");
}
