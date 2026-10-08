//! Interface admission must consume checked bytes and retain effective contracts.
use cellscript::{compile_with_executable_surface_policy, CompileOptions, ExecutableSurfacePolicy};
use cellscript_artifact_checker::{interface::inspect_bundle, CheckerBudgets, CheckerRejectionCode};

#[test]
fn qualified_types_preserve_generic_shadowing_and_complete_unicode_names() {
    use cellscript_artifact_checker::{
        interface::{qualified_source_type, qualified_source_type_with_parameters},
        NominalDeclarationBinding, NominalDeclarationScope,
    };
    let scope = NominalDeclarationScope {
        module: "consumer".into(),
        bindings: vec![
            NominalDeclarationBinding { local_name: "数".into(), owner_module: "foreign".into(), source_name: "Tag".into() },
            NominalDeclarationBinding { local_name: "箱".into(), owner_module: "owner".into(), source_name: "Box".into() },
        ],
    };
    assert_eq!(qualified_source_type("箱<[数; 2]>", &scope).unwrap(), "owner::Box<[foreign::Tag;2]>");
    assert_eq!(qualified_source_type_with_parameters("箱<(数, &mut 数)>", &scope, &["数"]).unwrap(), "owner::Box<(数,&mut 数)>");
    assert_eq!(qualified_source_type("already::数", &scope).unwrap(), "already::数");
    assert_eq!(qualified_source_type("&mutable", &scope).unwrap(), "&mutable");
    for invalid in ["箱<>", "箱<数,>", "数 数", "[数; -1]", "[数;18446744073709551616]", "数::", "数;;", "&mut", "(数,,数)"]
    {
        assert!(qualified_source_type(invalid, &scope).is_err(), "invalid spelling accepted: {invalid}");
    }
}

#[test]
fn oversized_uninstantiated_templates_keep_the_existing_declaration_only_language() {
    let fields = (0..65).map(|index| format!("field_{index}: T")).collect::<Vec<_>>().join(", ");
    let source = format!("module declaration_only\npublic struct Large<T: fixed_value> {{ {fields} }}\npublic action verify() {{ verification require true }}");
    let compiled = compile_with_executable_surface_policy(
        &source,
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let inspected = inspect_bundle(
        &compiled.artifact_bytes,
        &serde_json::to_vec(&compiled.metadata).unwrap(),
        &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
        &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap();
    assert_eq!(inspected.declared().types[0].fields.len(), 65);
    assert!(inspected.effective().generic_declarations.is_none());
    assert!(inspected.effective().nominal_declarations.is_none());
    assert!(inspected.effective().instantiations.is_empty());
    assert!(inspected.validate_symbolic_declarations().is_err());
}

#[test]
fn interface_generic_projection_checks_nested_structs_enums_and_functions() {
    let source = r#"
module inspected_generic_projection
public struct Pair<T: fixed_value> { left: T, right: T }
public struct Envelope<T: fixed_value> { pairs: [Pair<T>; 2], count: u64 }
public enum Choice<T: fixed_value> { Nested(Envelope<T>), Direct(Pair<T>), Empty }
public struct Tagged<phantom T> has copy, drop, store, fixed, serializable, non_linear { value: u64 }
public struct UnitFields<T: fixed_value> { z: T, a: T }
public struct MixedFields<T: fixed_value> { z: T, a: T, value: u64, tail: T }
public struct WithUnits<T: fixed_value> { nested: (T, ()), empty: () }
public fn first<T: fixed_value>(pair: Pair<T>) -> T { pair.left }
public action verify(witness choice: Choice<Hash>, witness pair: Pair<u64>, witness marker: Tagged<Address>, witness empty: UnitFields<()>, witness mixed: MixedFields<()>, witness units: WithUnits<u64>) {
    verification
    let expected = pair.left
    require first<u64>(pair) == expected
}
"#;
    for opt_level in 0..=3 {
        let compiled = compile_with_executable_surface_policy(
            source,
            CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), opt_level, ..CompileOptions::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let inspected = inspect_bundle(
            &compiled.artifact_bytes,
            &serde_json::to_vec(&compiled.metadata).unwrap(),
            &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap_or_else(|error| panic!("opt={opt_level}: {error}"));
        assert_eq!(inspected.declared(), &compiled.metadata.public_interface);
        inspected.validate_symbolic_declarations().unwrap();
        for template in ["Pair", "Envelope", "Choice", "Tagged", "UnitFields", "MixedFields", "WithUnits", "first"] {
            assert!(inspected.effective().instantiations.iter().any(|instance| instance.template == template));
        }
    }
}

#[test]
fn generic_projection_preserves_unicode_identifiers_and_parameter_boundaries() {
    let source = r#"
module unicode_generic_contract
private struct αT { value: u64 }
public struct 箱<数: fixed_value> { tag: αT, value: 数 }
public fn 取<数: fixed_value>(value: 数) -> 数 { value }
public action verify(witness input: 箱<u64>, witness tag: αT) {
    verification
    require 取<u64>(input.value) == tag.value
}
"#;
    for opt_level in 0..=3 {
        let compiled = compile_with_executable_surface_policy(
            source,
            CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), opt_level, ..CompileOptions::default() },
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap();
        let inspected = inspect_bundle(
            &compiled.artifact_bytes,
            &serde_json::to_vec(&compiled.metadata).unwrap(),
            &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
            &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
            &CheckerBudgets::default(),
        )
        .unwrap();
        assert!(inspected
            .effective()
            .instantiations
            .iter()
            .any(|instance| instance.template == "箱" && instance.parameters[0].name == "数"));
    }
}

#[test]
fn interface_inspection_preserves_declared_and_inferred_effects_separately() {
    let source =
        "module inspected\naction verify() {\nverification\nlet amount = ckb::cell_capacity(source::input(0))\nrequire amount > 0\n}";
    let compiled = compile_with_executable_surface_policy(
        source,
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let metadata = serde_json::to_vec(&compiled.metadata).unwrap();
    let lowering = serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap();
    let source_map = serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap();
    let inspect =
        |artifact: &[u8], metadata: &[u8]| inspect_bundle(artifact, metadata, &lowering, &source_map, &CheckerBudgets::default());
    let result = inspect(&compiled.artifact_bytes, &metadata).unwrap();
    assert_eq!(result.declared(), &compiled.metadata.public_interface);
    assert_eq!(result.declared().callables[0].effect, "Pure");
    assert_eq!(result.effective().entries[0].effect, "ReadOnly");
    assert_eq!(result.report().artifact_hash, compiled.metadata.artifact_hash.as_ref().unwrap().as_str());

    let mut substituted = compiled.artifact_bytes.clone();
    substituted[0] ^= 1;
    assert!(inspect(&substituted, &metadata).is_err());
    let mut changed = serde_json::to_value(&compiled.metadata).unwrap();
    changed["public_interface"]["callables"][0]["effect"] = serde_json::json!("ReadOnly");
    assert!(inspect(&compiled.artifact_bytes, &serde_json::to_vec(&changed).unwrap()).is_err());
}

#[test]
fn interface_metadata_budget_is_applied_before_parsing_untrusted_bytes() {
    let budgets = CheckerBudgets { record_bytes: 2, ..CheckerBudgets::default() };
    let error = inspect_bundle(&[], b"not JSON", &[], &[], &budgets).unwrap_err();
    assert_eq!(error.code, CheckerRejectionCode::V2400BudgetExceeded);
}

#[test]
fn interface_inspection_retains_public_generic_templates_without_runtime_instances() {
    let source = r#"
module inspected_templates
public struct Pair<T: fixed_value> { left: T, right: T }
public fn first<T: fixed_value>(pair: Pair<T>) -> T { pair.left }
public action verify() { verification require true }
"#;
    let compiled = compile_with_executable_surface_policy(
        source,
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let inspected = inspect_bundle(
        &compiled.artifact_bytes,
        &serde_json::to_vec(&compiled.metadata).unwrap(),
        &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
        &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap();
    assert_eq!(inspected.declared(), &compiled.metadata.public_interface);
    let template = inspected.declared().callables.iter().find(|entry| entry.name == "first").unwrap();
    assert_eq!(template.type_parameters.len(), 1);
    assert_eq!(template.params[0].r#type, "Pair<T>");
    assert!(!inspected.effective().entries.iter().any(|entry| entry.name == "first"));
    let catalog = inspected.effective().generic_declarations.as_ref().unwrap();
    assert_eq!(catalog.declarations.iter().map(|contract| contract.name.as_str()).collect::<Vec<_>>(), ["Pair", "first"]);
    assert!(inspected.effective().instantiations.is_empty());
    inspected.validate_symbolic_declarations().unwrap();
}

#[test]
fn interface_inspection_keeps_concrete_enum_variants_and_payload_cardinalities() {
    let compiled = compile_with_executable_surface_policy(
        "module inspected_enum\npublic enum Choice { First(u64), Second(Hash) }\npublic action verify(witness choice: Choice) { verification require true }",
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let inspected = inspect_bundle(
        &compiled.artifact_bytes,
        &serde_json::to_vec(&compiled.metadata).unwrap(),
        &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
        &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap();
    let choice = inspected.declared().types.iter().find(|ty| ty.name == "Choice").unwrap();
    assert!(inspected.effective().types.iter().any(|ty| ty.name == "Choice"));
    assert_eq!(choice.variants.iter().map(|variant| variant.name.as_str()).collect::<Vec<_>>(), ["First", "Second"]);
    assert!(choice.variants.iter().all(|variant| variant.fields.len() == 1));
}

#[test]
fn interface_inspection_matches_nested_fields_and_instantiated_source_type_names() {
    let compiled = compile_with_executable_surface_policy(
        r#"
module inspected_nested
public struct Pair<T: fixed_value> { left: T, right: T }
public struct Envelope { pairs: [Pair<Hash>; 2], count: u64 }
public enum Choice { Nested(Envelope), Direct(Pair<Address>), Bytes([u8; 32]) }
public action verify(witness choice: Choice) { verification require true }
"#,
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..CompileOptions::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let inspected = inspect_bundle(
        &compiled.artifact_bytes,
        &serde_json::to_vec(&compiled.metadata).unwrap(),
        &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
        &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
        &CheckerBudgets::default(),
    )
    .unwrap();
    let envelope = inspected.effective().types.iter().find(|ty| ty.name == "Envelope").unwrap();
    assert!(envelope.fields.iter().any(|field| field.ty.contains("Pair__mono__")));
    let choice = inspected.effective().types.iter().find(|ty| ty.name == "Choice").unwrap();
    assert!(choice.variants.iter().flat_map(|variant| &variant.fields).any(|field| field.ty.contains("Pair__mono__")));
    assert_eq!(inspected.declared(), &compiled.metadata.public_interface);
}

#[test]
fn interface_callable_bindings_preserve_locks_borrows_outputs_and_generic_names() {
    let sources = [
        r#"
module inspected_lock
resource Token { amount: u64 }
shared Config { value: u64 }
lock permit(lock_args minimum: u64, protected token: Token, witness claimed: u64,
            read config: Config, lock_args maximum: u64) -> bool {
    verification
    require token.amount >= minimum
    require token.amount <= maximum
    require claimed == config.value
}
"#,
        include_str!("../examples/language/ownership/borrow.cell"),
        r#"
module inspected_output
resource Token has store, replace, relock { amount: u64 }
shared Config { value: u64 }
action transfer(read config: Config, input token: Token, witness recipient: Address) -> next: Token {
    verification
    require token.amount == config.value
    std::lifecycle::transfer(token, next, recipient) { amount }
    std::cell::preserve_capacity(next, token)
}
"#,
        r#"
module inspected_callable_generic
public struct Pair<T: fixed_value> { left: T, right: T }
public fn first(pair: Pair<u64>) -> u64 { pair.left }
public action verify(witness pair: Pair<u64>) {
    verification
    let expected = pair.left
    require first(pair) == expected
}
"#,
    ];
    for source in sources {
        for opt_level in 0..=3 {
            let compiled = compile_with_executable_surface_policy(
                source,
                CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), opt_level, ..CompileOptions::default() },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap_or_else(|error| panic!("opt={opt_level}: {error}\n{source}"));
            let inspected = inspect_bundle(
                &compiled.artifact_bytes,
                &serde_json::to_vec(&compiled.metadata).unwrap(),
                &serde_json::to_vec(compiled.verified_lowering_record.as_ref().unwrap()).unwrap(),
                &serde_json::to_vec(compiled.source_artifact_map.as_ref().unwrap()).unwrap(),
                &CheckerBudgets::default(),
            )
            .unwrap_or_else(|error| panic!("opt={opt_level}: {error}\n{source}"));
            assert_eq!(inspected.declared(), &compiled.metadata.public_interface);
        }
    }
}
