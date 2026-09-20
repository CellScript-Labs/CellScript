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
