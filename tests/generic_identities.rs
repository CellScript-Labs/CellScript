//! Edition 2027 source spelling, public identity, executable bytes and
//! independent admission retain separate characterization contracts.
use cellscript::{CellScriptEdition, CompileOptions, CompilerDiagnostics, EntryWitnessArg, ExecutableSurfacePolicy};
use ckb_testtool::ckb_types::{bytes::Bytes, packed, prelude::*};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[path = "support/ckb_script_runner.rs"]
#[allow(dead_code)]
mod ckb_script_runner;

const SYNTAX: &str = r#"
module identity_syntax
struct Types {
    hash: Hash,
    address: Address,
    unit: (),
    array: [u8; 16],
    pair: (Hash, u64),
    nested: Vec<(u64, [u8; 2])>,
    view: WitnessBytesView<entry, 64>,
    maximum: Family<340282366920938463463374607431768211455>,
}
"#;

const EMPTY: &str = r#"
module identity_empty
public struct Mixed<T: fixed_value> { head: T, value: u64, tail: T }
public fn identity<T: fixed_value>(value: T) -> T { value }
action verify(witness mixed: Mixed<()>) -> u64 {
    verification
    require identity<u64>(mixed.value) == 42
    return 0
}
"#;

const NESTED: &str = r#"
module identity_nested
public struct Box<T: fixed_value> { value: T }
public fn identity<T: fixed_value>(value: T) -> T { value }
action verify() -> u64 {
    verification
    let value: Box<Box<(u64, [u8; 2])>> = Box<Box<(u64, [u8; 2])>> { value: Box<(u64, [u8; 2])> { value: (42, b"\x09\x0a") } }
    let copied = identity<Box<Box<(u64, [u8; 2])>>>(value)
    require copied.value.value.0 == 42
    return 0
}
"#;

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn observe() -> Value {
    let mut rows = Vec::new();
    {
        let edition = CellScriptEdition::Edition2027;
        let formatted = cellscript::fmt::format_source(SYNTAX, edition, Default::default()).unwrap();
        rows.push(json!({"kind": "syntax", "edition": edition.as_str(), "formatted": formatted}));
        for (case, source) in [("empty", EMPTY), ("nested", NESTED)] {
            for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
                let result = cellscript::compile_with_executable_surface_policy(
                    source,
                    CompileOptions {
                        edition,
                        diagnostics,
                        source_contracts: true,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap();
                result.validate().unwrap();
                let record = result.verified_lowering_record.as_ref().unwrap();
                let inspection = cellscript_artifact_checker::interface::inspect_bundle(
                    &result.artifact_bytes,
                    &serde_json::to_vec(&result.metadata).unwrap(),
                    &serde_json::to_vec(record).unwrap(),
                    &serde_json::to_vec(result.source_artifact_map.as_ref().unwrap()).unwrap(),
                    &cellscript_artifact_checker::CheckerBudgets::default(),
                )
                .unwrap();
                let checked = inspection.project_module_contract().unwrap();
                let mut types = record
                    .typed_semantics
                    .entries
                    .iter()
                    .flat_map(|entry| &entry.locals)
                    .map(|local| local.ty.clone())
                    .collect::<Vec<_>>();
                types.sort();
                types.dedup();
                rows.push(json!({"kind":"artifact", "case":case, "edition":edition.as_str(),
                    "optimizations":diagnostics.is_standard(), "elf_sha256":digest(cellscript::strip_vm_abi_trailer(&result.artifact_bytes)),
                    "declared_interface_hash":result.metadata.interface_hash,
                    "checked_projection_sha256":digest(&checked.canonical_bytes().unwrap()),
                    "instantiations":result.metadata.generic_instantiations, "typed_local_types":types}));
                let mut fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                if case == "empty" {
                    let params = &result.metadata.actions.iter().find(|action| action.name == "verify").unwrap().params;
                    let payload = cellscript::encode_entry_witness_args_for_params(
                        params,
                        &[EntryWitnessArg::Bytes(42u64.to_le_bytes().to_vec())],
                    )
                    .unwrap();
                    fixture.witnesses =
                        vec![packed::WitnessArgs::new_builder().input_type(Some(Bytes::from(payload)).pack()).build().as_bytes()];
                }
                assert_eq!(
                    ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code,
                    0,
                    "{case}/{edition}/{diagnostics}"
                );
            }
        }
    }
    json!({"schema":"cellscript-type-identity-characterization-v1","cases":rows})
}

#[test]
fn source_wire_symbol_interface_and_machine_identities_match_characterization() {
    let expected: Value = serde_json::from_str(include_str!("fixtures/type_identity_current.json")).unwrap();
    assert_eq!(observe(), expected);
}

#[test]
#[ignore = "explicit current-edition evidence export; does not update tracked fixtures"]
fn export_type_identity_characterization() {
    let output = std::env::var_os("CELLSCRIPT_TYPE_BASELINE_OUTPUT").expect("set explicit output path");
    std::fs::write(output, serde_json::to_string_pretty(&observe()).unwrap() + "\n").unwrap();
}

#[test]
fn retained_origins_reject_missing_owner_role_and_cyclic_arguments() {
    let result = cellscript::compile_with_executable_surface_policy(
        EMPTY,
        CompileOptions { source_contracts: true, target: Some("riscv64-elf".into()), ..Default::default() },
        ExecutableSurfacePolicy::DenyFailClosed,
    )
    .unwrap();
    let ast = result.ast;
    let name = ast.items.iter().find(|item| item.is_specialization()).unwrap().name().unwrap().to_owned();
    assert!(!ast.specializations.is_empty());

    let mut missing = ast.clone();
    std::sync::Arc::make_mut(&mut missing.specializations).remove(&name);
    let error = cellscript::ir::generate(&missing).unwrap_err();
    assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2111));
    assert!(error.message.contains("no retained specialization origin"));

    let mut wrong_owner = ast.clone();
    std::sync::Arc::make_mut(&mut wrong_owner.specializations).get_mut(&name).unwrap().template.module = "foreign_owner".into();
    let error = cellscript::ir::generate(&wrong_owner).unwrap_err();
    assert!(error.message.contains("owner or kind"));

    let mut wrong_role = ast.clone();
    let item = wrong_role.items.iter_mut().find(|item| item.name() == Some(name.as_str())).unwrap();
    match item {
        cellscript::ast::Item::Struct(definition) => definition.origin = cellscript::ast::DefinitionOrigin::Source,
        cellscript::ast::Item::Function(definition) => definition.origin = cellscript::ast::DefinitionOrigin::Source,
        cellscript::ast::Item::Enum(definition) => definition.origin = cellscript::ast::DefinitionOrigin::Source,
        _ => unreachable!(),
    }
    let error = cellscript::ir::generate(&wrong_role).unwrap_err();
    assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2111));
    assert!(error.message.contains("source declaration cannot claim"));

    for argument in [
        cellscript::ast::Type::Named(name.clone()),
        cellscript::ast::Type::applied("Vec", vec![cellscript::ast::TypeArgument::Type(cellscript::ast::Type::Named(name.clone()))]),
    ] {
        let mut cyclic = ast.clone();
        std::sync::Arc::make_mut(&mut cyclic.specializations).get_mut(&name).unwrap().arguments = vec![argument];
        let error = cellscript::ir::generate(&cyclic).unwrap_err();
        assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2112));
        assert!(error.message.contains("cyclic"));
    }
}

#[test]
fn a_type_parameter_can_shadow_a_generated_symbol_without_becoming_that_type() {
    let source = r#"
module scoped_type_parameter
public struct Box<T: fixed_value> { value: T }
public fn shadow<Box__mono__753634: fixed_value>(value: Box__mono__753634) -> Box__mono__753634 { value }
action verify() -> u64 {
    verification
    let value: Box<u64> = Box<u64> { value: 42 }
    require shadow<u64>(value.value) == 42
    return 0
}

"#;
    {
        let edition = CellScriptEdition::Edition2027;
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile_with_executable_surface_policy(
                source,
                CompileOptions {
                    edition,
                    diagnostics,
                    source_contracts: true,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            result.validate().unwrap();
            let declaration = result
                .metadata
                .typed_semantics
                .generic_declarations
                .as_ref()
                .unwrap()
                .declarations
                .iter()
                .find(|declaration| declaration.name == "shadow")
                .unwrap();
            assert_eq!(declaration.parameters[0].name, "Box__mono__753634");
            let cellscript_artifact_checker::TypedSemanticGenericDeclaration::Function { params, return_type } =
                &declaration.declaration
            else {
                panic!("function contract")
            };
            assert_eq!(params[0].ty, "Box__mono__753634");
            assert_eq!(return_type, "Box__mono__753634");
            let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            assert_eq!(ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture).exit_code, 0);
        }
    }
}

#[test]
fn unit_arguments_have_one_specialization_and_keep_evaluation() {
    let source = r#"
module generic_unit
enum Choice { A, B }
public struct Mixed<T: fixed_value> { head: T, value: u64, tail: T }
public fn forward<T: fixed_value>(value: T) -> T { let copy: T = value return copy }
public fn take<T: fixed_value>(value: T) -> u64 { 42 }
fn checked(divisor: u64) -> () { let value = 42 / divisor return () }
fn keep(value: ()) -> u64 { 42 }
action verify() -> u64 {
    verification
    let value: Mixed<()> = Mixed<()> { head: (), value: 42, tail: () }
    require take<()>(()) == 42
    require take(value.head) == 42
    let unit: () = forward<()>(checked(1))
    require take(unit) == 42
    require keep(unit) == 42
    let selected: () = match (Choice::A) { Choice::A => unit, Choice::B => () }
    require take(selected) == 42
    return 0
}
"#;
    {
        let edition = CellScriptEdition::Edition2027;
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            for (source, should_succeed) in [(source.to_string(), true), (source.replace("checked(1)", "checked(0)"), false)] {
                let result = cellscript::compile_with_executable_surface_policy(
                    &source,
                    CompileOptions {
                        edition,
                        diagnostics,
                        source_contracts: true,
                        target: Some("riscv64-elf".into()),
                        ..Default::default()
                    },
                    ExecutableSurfacePolicy::DenyFailClosed,
                )
                .unwrap();
                result.validate().unwrap();
                let take = result.ast.specializations.values().filter(|origin| origin.local_template == "take").collect::<Vec<_>>();
                assert_eq!(take.len(), 1);
                assert_eq!(take[0].arguments, vec![cellscript::ast::Type::Unit]);
                let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
                let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
                let expected = if should_succeed {
                    0
                } else {
                    cellscript::runtime_errors::CellScriptRuntimeError::NumericOrDiscriminantInvalid as i64
                };
                assert_eq!(execution.exit_code, expected, "{edition:?} {diagnostics:?}: {execution:?}");
            }
        }
    }
}

#[test]
fn absent_returns_cannot_manufacture_unit_values() {
    for expression in [
        "no_value()",
        "take<()>(no_value())",
        "(no_value(), 1)",
        "[no_value()]",
        "no_value() as ()",
        "no_value()..1",
        "match (Choice::A) { Choice::A => no_value(), Choice::B => () }",
    ] {
        let source = format!("module missing_value enum Choice {{ A, B }} fn no_value() {{}} fn take<T: fixed_value>(value: T) -> u64 {{ 42 }} action verify() -> u64 {{ verification let value = {expression} return 0 }}");
        let error = cellscript::compile_with_executable_surface_policy(
            &source,
            CompileOptions::default(),
            ExecutableSurfacePolicy::DenyFailClosed,
        )
        .unwrap_err();
        assert_eq!(error.code, Some(cellscript::error::DiagnosticCode::E2114), "{expression}: {error}");
    }
}

#[test]
fn literal_enums_keep_identity_in_matches_and_calls() {
    let source = r#"
module literal_enum
public enum Choice { A, B }
fn tag(value: Choice) -> u64 {
    match value { Choice::A => 1, Choice::B => 2 }
}
action verify() -> u64 {
    verification
    let first = match (Choice::A) { Choice::A => 1, Choice::B => 2 }
    require first == 1
    require tag(Choice::B) == 2
    return 0
}
"#;
    {
        let edition = CellScriptEdition::Edition2027;
        for diagnostics in [CompilerDiagnostics::default(), CompilerDiagnostics::without_optimizations()] {
            let result = cellscript::compile_with_executable_surface_policy(
                source,
                CompileOptions {
                    edition,
                    diagnostics,
                    source_contracts: true,
                    target: Some("riscv64-elf".into()),
                    ..Default::default()
                },
                ExecutableSurfacePolicy::DenyFailClosed,
            )
            .unwrap();
            result.validate().unwrap();
            let fixture = ckb_script_runner::build_simple_fixture(Bytes::new(), 1, 1);
            let execution = ckb_script_runner::execute_cellscript_script(&result.artifact_bytes, &fixture);
            assert_eq!(execution.exit_code, 0, "{edition:?} {diagnostics:?}: {execution:?}");
        }
    }
}
