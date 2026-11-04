use super::*;

#[test]
fn mixed_explicit_and_legacy_visibility_emits_one_migration_warning() {
    let report = compile_metadata_with_diagnostics(
        "module visibility\npublic fn explicit() -> u64 { return 1 }\nfn implicit() -> u64 { return 2 }\n",
        CURRENT_EDITION,
        None,
    );
    assert!(report.metadata.is_some());
    let warnings = report.diagnostics.iter().filter(|diagnostic| diagnostic.code.as_deref() == Some("W2500")).collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].severity, DiagnosticSeverity::Warning);
    assert!(warnings[0].message.contains("implicit"));
    assert!(report.metadata.unwrap().constraints.warnings.iter().any(|warning| warning.contains("legacy-public")));
}

#[test]
fn compile_metadata_diagnostics_collects_multiple_type_errors() {
    let source = r#"
module multi_errors

action bad_one() -> u64 {
    verification
        return true
}

action bad_two() -> bool {
    verification
        return 1
}
"#;
    let report = compile_metadata_with_diagnostics(source, CURRENT_EDITION, None);
    assert!(report.metadata.is_none());
    assert_eq!(report.diagnostics.len(), 2);
    assert!(report.diagnostics.iter().any(|error| error.message.contains("expected U64, found Bool")));
    assert!(report.diagnostics.iter().any(|error| error.message.contains("expected Bool, found U64")));
}

#[test]
fn compile_metadata_diagnostics_collects_multiple_parse_errors() {
    let source = r#"
module multi_parse_errors

action bad() -> bool {
    verification
        let first: u64 true
        let second: bool 1
        return true
}
"#;
    let report = compile_metadata_with_diagnostics(source, CURRENT_EDITION, None);
    assert!(report.metadata.is_none());
    assert_eq!(report.diagnostics.len(), 2);
    assert!(report.diagnostics.iter().any(|error| error.message.contains("expected '=', found 'true'")));
    assert!(report.diagnostics.iter().any(|error| error.message.contains("expected '=', found integer 1")));
}

#[test]
fn compile_sources_metadata_diagnostics_collects_parse_errors_across_files() {
    let sources = vec![
        InMemorySource {
            path: "src/main.cell".to_string(),
            role: Some("entry".to_string()),
            source: r#"
module demo::main

action bad() -> bool {
    verification
        let first: u64 true
        return true
}
"#
            .to_string(),
        },
        InMemorySource {
            path: "src/helper.cell".to_string(),
            role: None,
            source: r#"
module demo::helper

action also_bad() -> bool {
    verification
        let second: bool 1
        return true
}
"#
            .to_string(),
        },
    ];
    let report = compile_sources_metadata_with_diagnostics(&sources, "src/main.cell", CURRENT_EDITION, None);
    assert!(report.metadata.is_none());
    assert_eq!(report.diagnostics.len(), 2);
    assert!(report.diagnostics.iter().any(|error| error.file.as_ref().is_some_and(|file| file.as_str() == "src/main.cell")));
    assert!(report.diagnostics.iter().any(|error| error.file.as_ref().is_some_and(|file| file.as_str() == "src/helper.cell")));
}

#[test]
fn compile_metadata_diagnostics_collects_multiple_statement_errors() {
    let source = r#"
module multi_errors

action bad() -> bool {
    verification
        let first: u64 = true
        let second: bool = 1
        return true
}
"#;
    let report = compile_metadata_with_diagnostics(source, CURRENT_EDITION, None);
    assert!(report.metadata.is_none());
    assert_eq!(report.diagnostics.len(), 2);
    assert!(report.diagnostics.iter().any(|error| error.message.contains("expected U64, found Bool")));
    assert!(report.diagnostics.iter().any(|error| error.message.contains("expected Bool, found U64")));
}

#[test]
fn compile_metadata_diagnostics_span_primitive_return_errors() {
    let source = r#"
module return_span

action bad() -> bool {
    verification
        return 1
}
"#;
    let report = compile_metadata_with_diagnostics(source, CURRENT_EDITION, None);
    assert!(report.metadata.is_none());
    assert_eq!(report.diagnostics.len(), 1);
    let diagnostic = &report.diagnostics[0];
    assert!(diagnostic.message.contains("expected Bool, found U64"));
    assert_eq!(diagnostic.span.line, 6);
    assert_eq!(diagnostic.span.column, 9);
    assert_eq!(&source[diagnostic.span.start..diagnostic.span.end], "return 1");
}

#[test]
fn compile_metadata_diagnostics_infers_each_independent_action_effect() {
    let source = r#"
module multi_ir_errors

resource Token {
amount: u64,
}

action issue_one(amount: u64) -> Token {
verification
    let out = create Token {
        amount: amount
    }
    return out
}

action issue_two(amount: u64) -> Token {
verification
    let out = create Token {
        amount: amount
    }
    return out
}
"#;
    let report = compile_metadata_with_diagnostics(source, CURRENT_EDITION, None);
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let metadata = report.metadata.unwrap();
    for name in ["issue_one", "issue_two"] {
        assert_eq!(
            metadata.actions.iter().find(|action| action.name == name).unwrap().effects.lifecycle_ops(),
            ir::EffectFacts::lifecycle(cellscript_artifact_checker::effect_facts::LifecycleOperation::Create).lifecycle_ops()
        );
    }
}

#[test]
fn compile_metadata_diagnostics_preserves_independent_ir_errors_during_effect_inference() {
    let source = r#"
module multi_ir_errors
resource Token has store { amount: u64 }
action issue_one(witness handle: ExactScriptHandle, witness expected: Hash) -> u64 {
    let input = ckb::input<Token>(0)
    ckb::require_cell_lock_exact_handle(input, handle, expected)
    return 0
}
action issue_two(witness handle: ExactScriptHandle, witness expected: Hash) -> u64 {
    let input = ckb::input<Token>(0)
    ckb::require_cell_type_exact_handle(input, handle, expected)
    return 0
}
"#;
    let report = compile_metadata_with_diagnostics(source, CellScriptEdition::Edition2027, None);
    assert!(report.metadata.is_none());
    assert_eq!(report.diagnostics.len(), 2, "{:?}", report.diagnostics);
    assert!(report.diagnostics.iter().all(|error| {
        error.message.contains("exact Script handle handle_hash must be a compile-time Hash literal") && error.span.line > 0
    }));
    assert_ne!(report.diagnostics[0].span, report.diagnostics[1].span);
}
