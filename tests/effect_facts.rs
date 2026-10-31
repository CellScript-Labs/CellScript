use cellscript::{CompileOptions, CompilerDiagnostics};
use cellscript_artifact_checker::effect_facts::{
    ContextAccess as Access, ContextSource as Source, EffectFacts, LifecycleOperation as Life, ProcessOperation as Process,
};
use cellscript_artifact_checker::{check_bundle_values, CheckerBudgets, CheckerRejectionCode};

#[test]
fn source_interface_conformance_matches_the_current_producer_and_comparator() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!("fixtures/effect_facts_interfaces.json")).unwrap();
    let mut interfaces = std::collections::BTreeMap::new();
    for case in vectors["cases"].as_array().unwrap() {
        let source = case["source"].as_str().unwrap();
        // The fixture uses the ordinary CLI compile path, including optional
        // source pruning. The metadata-only API intentionally retains a
        // different executable-evidence inventory and builder projection.
        let metadata = cellscript::compile(source, CompileOptions::default()).unwrap().metadata;
        assert_eq!(serde_json::to_value(&metadata.public_interface).unwrap(), case["interface"], "{}", case["name"]);
        cellscript::interface::validate(&metadata.public_interface).unwrap();
        interfaces.insert(case["name"].as_str().unwrap(), metadata.public_interface);
    }
    for case in vectors["comparisons"].as_array().unwrap() {
        let required = &interfaces[case["required"].as_str().unwrap()];
        let candidate = &interfaces[case["candidate"].as_str().unwrap()];
        assert_eq!(cellscript::interface::compare(required, candidate).compatible, case["accepts"].as_bool().unwrap(), "{case}");
    }
}

fn compile(source: &str, diagnostics: CompilerDiagnostics) -> cellscript::CompileResult {
    cellscript::compile(
        source,
        CompileOptions { source_contracts: true, diagnostics, target: Some("riscv64-elf".into()), ..Default::default() },
    )
    .unwrap()
}
fn checked(result: &cellscript::CompileResult) {
    check_bundle_values(
        &result.artifact_bytes,
        &serde_json::to_value(&result.metadata).unwrap(),
        result.verified_lowering_record.as_ref().unwrap(),
        result.source_artifact_map.as_ref().unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap();
}
#[test]
fn observation_contract_is_structured_and_optimization_preserves_it() {
    let source="module observation\npublic action verify() { verification if false { let amount = ckb::cell_capacity(source::input(0)) require amount > 0 } require true }";
    let expected = EffectFacts::read(Source::Input, Access::Capacity);
    let mut interface = None;
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let result = compile(source, diagnostics);
        checked(&result);
        assert_eq!(result.metadata.actions[0].effects, expected);
        assert_eq!(result.metadata.public_interface.callables[0].effects, Some(expected.clone()));
        let wire = serde_json::to_value(&result.metadata).unwrap();
        assert!(wire["actions"][0]["effects"].is_object());
        assert!(wire["actions"][0].get("effect_class").is_none());
        if let Some(previous) = &interface {
            assert_eq!(&result.metadata.public_interface, previous);
        }
        interface = Some(result.metadata.public_interface);
    }
}
#[test]
fn checker_reconstructs_process_operations_instead_of_trusting_rebound_facts() {
    let source = "module process\npublic action send(fd: u64) -> u64 { verification return pipe_write(fd, 7) }";
    let result = compile(source, CompilerDiagnostics::without_optimizations());
    checked(&result);
    let expected = EffectFacts::process(Process::PipeWrite);
    assert_eq!(result.metadata.actions[0].effects, expected);
    let mut record = result.verified_lowering_record.unwrap();
    record.typed_semantics.entries[0].effects = EffectFacts::empty();
    record.typed_semantics_hash =
        cellscript_artifact_checker::canonical_hash(cellscript_artifact_checker::TYPED_SEMANTICS_SCHEMA, &record.typed_semantics)
            .unwrap();
    // This raw inference regression is also exercised by complete sidecar
    // rebinding in artifact_checker::effects. Here the metadata must never
    // accept an unbound replacement fact set.
    let error = check_bundle_values(
        &result.artifact_bytes,
        &serde_json::to_value(result.metadata).unwrap(),
        &record,
        &result.source_artifact_map.unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap_err();
    assert!(matches!(
        error.code,
        CheckerRejectionCode::V2410MetadataBindingMismatch | CheckerRejectionCode::V2419TypedSemanticsInvalid
    ));
}
#[test]
fn reading_then_creating_preserves_both_dimensions() {
    let source="module issue\nresource Token has store, create { amount: u64 }\npublic action issue(owner: Address) -> Token { verification let amount = ckb::cell_capacity(source::input(0)) create Token { amount: amount } with_lock(owner) }";
    let result = compile(source, CompilerDiagnostics::without_optimizations());
    checked(&result);
    let facts = &result.metadata.actions[0].effects;
    assert!(facts.context_reads().iter().any(|read| read.source == Source::Input && read.access == Access::Capacity));
    assert_eq!(facts.lifecycle_ops(), EffectFacts::lifecycle(Life::Create).lifecycle_ops());
    assert!(facts.process_ops().is_empty());
}
#[test]
fn source_argument_substitution_reaches_the_independent_checker() {
    let source="module source_bindings\nresource Token has store { amount: u64 }\nfn capacity(view: InputView<Token>) -> u64 { return ckb::cell_capacity(view) }\npublic action inspect() -> u64 { verification return capacity(ckb::input<Token>(0)) }";
    let result = compile(source, CompilerDiagnostics::without_optimizations());
    checked(&result);
    assert_eq!(result.metadata.actions[0].effects, EffectFacts::read(Source::Input, Access::Capacity));
    assert_eq!(result.metadata.functions[0].effects, EffectFacts::read(Source::Parameter { index: 0 }, Access::Capacity));
}

#[test]
fn allocated_local_results_preserve_facts_of_following_calls() {
    let source = r#"module allocated_result_facts
resource Token has store { amount: u64 }
collections Work { limits { bytes: 512, allocations: 4 }
    values: Vec<u64> { items: 4 }
}
fn local(view: &Work) -> u64 { return view.values.get(0) }
fn observe() -> u64 { return ckb::input<Token>(0).data.amount }
public action inspect() -> u64 {
    let mut work = Work::new()
    work.values.push(3)
    let value = local(&work)
    require observe() == value
    return 0
}"#;
    for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
        let result = cellscript::compile(
            source,
            CompileOptions {
                edition: cellscript::CellScriptEdition::Edition2027,
                diagnostics,
                source_contracts: true,
                target: Some("riscv64-elf".into()),
                ..Default::default()
            },
        )
        .unwrap();
        checked(&result);
        assert_eq!(result.metadata.actions[0].effects, EffectFacts::read(Source::Input, Access::CellData));
        assert!(result.metadata.functions.iter().find(|function| function.name == "local").unwrap().effects.is_empty());
    }
}
